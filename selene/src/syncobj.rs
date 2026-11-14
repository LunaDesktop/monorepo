//! `wp_linux_drm_syncobj_v1` — jawna synchronizacja buforów GPU (explicit sync).
//!
//! Przy `dmabuf` kompozytor i klient dzielą pamięć GPU. Bez jawnej synchronizacji
//! kolejność „klient skończył rysować → my próbkujemy” opiera się na niejawnych
//! fence'ach sterownika. Na części układów (szczególnie NVIDIA + XWayland) prowadzi
//! to do prezentacji **starego** bufora i migotania. Protokół pozwala klientowi
//! podać punkt osi czasu DRM (`acquire`), na który czekamy przed użyciem bufora,
//! oraz punkt `release`, który sygnalizujemy, gdy bufor jest znów wolny.
//!
//! Global ogłaszamy **tylko** gdy:
//!  * ścieżka GPU/dmabuf jest włączona (`LUNA_GPU`/`SELENE_GPU`, EGL działa),
//!  * sterownik wspiera `DRM_CAP_SYNCOBJ_TIMELINE`,
//!  * użytkownik nie wyłączył tego przez `SELENE_EXPLICIT_SYNC=0`.
//!
//! Każdy błąd ioctla degraduje się bezpiecznie: czekanie kończy się timeoutem
//! (komponujemy to, co mamy), a nie sygnalizowanie punktu nie blokuje klienta
//! na zawsze — dostaje `wl_buffer.release` tak jak dotąd.

use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use wayland_protocols::wp::linux_drm_syncobj::v1::server::{
    wp_linux_drm_syncobj_manager_v1 as manager, wp_linux_drm_syncobj_surface_v1 as sync_surface,
    wp_linux_drm_syncobj_timeline_v1 as timeline,
};
use wayland_server::protocol::wl_buffer::WlBuffer;
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::compositor::SurfaceData;
use crate::sync::LockExt;

// ── ioctl-e DRM (syncobj) ──

const DRM_IOCTL_GET_CAP: libc::c_ulong = 0xC010_640C;
const DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE: libc::c_ulong = 0xC018_64C2;
const DRM_IOCTL_SYNCOBJ_DESTROY: libc::c_ulong = 0xC008_64C0;
const DRM_IOCTL_SYNCOBJ_TIMELINE_WAIT: libc::c_ulong = 0xC028_64CA;
const DRM_IOCTL_SYNCOBJ_TIMELINE_SIGNAL: libc::c_ulong = 0xC018_64CD;

/// `DRM_CAP_SYNCOBJ_TIMELINE` (drm.h).
const DRM_CAP_SYNCOBJ_TIMELINE: u64 = 0x15;
/// Czekaj także na punkt, który nie został jeszcze „submitnięty” przez klienta.
const WAIT_FOR_SUBMIT: u32 = 1 << 1;

#[repr(C)]
struct GetCap {
    capability: u64,
    value: u64,
}

#[repr(C)]
struct SyncobjHandle {
    handle: u32,
    flags: u32,
    fd: i32,
    pad: u32,
    point: u64,
}

#[repr(C)]
struct SyncobjDestroy {
    handle: u32,
    pad: u32,
}

#[repr(C)]
struct TimelineWait {
    handles: u64,
    points: u64,
    timeout_nsec: i64,
    count_handles: u32,
    flags: u32,
    first_signaled: u32,
    pad: u32,
    /// `deadline_nsec` (jądro ≥ 6.8). Numer ioctla koduje **stary** rozmiar
    /// 40 B, ale jądro kopiuje pełny `struct` (48 B) — musimy mieć to pole,
    /// żeby nie czytać poza naszą strukturą.
    deadline_nsec: u64,
}

#[repr(C)]
struct TimelineArray {
    handles: u64,
    points: u64,
    count_handles: u32,
    flags: u32,
}

// ── Stan urządzenia i punktów ──

/// `dev_t`-owe urządzenie DRM, na którym importujemy osie czasu. `None` = brak wsparcia.
static DEVICE: OnceLock<Option<OwnedFd>> = OnceLock::new();
/// Żeby nie zaśmiecać logu przy każdej klatce.
static TIMEOUT_LOGGED: AtomicBool = AtomicBool::new(false);
static FAILED_LOGGED: AtomicBool = AtomicBool::new(false);

/// Punkt osi czasu: uchwyt `drm_syncobj` + wartość punktu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point {
    pub timeline: u32,
    pub value: u64,
}

/// Global `wp_linux_drm_syncobj_manager_v1` (ogłaszany z `main`).
pub type Manager = manager::WpLinuxDrmSyncobjManagerV1;

/// Czy jawna synchronizacja jest w ogóle dostępna (global ogłoszony).
pub fn enabled() -> bool {
    DEVICE.get().is_some_and(Option::is_some)
}

fn device() -> Option<&'static OwnedFd> {
    DEVICE.get()?.as_ref()
}

/// Sprawdza wsparcie sterownika i otwiera urządzenie. Idempotentne.
///
/// Wołane raz przy starcie, **przed** ogłoszeniem globalu — dzięki temu klienty
/// nie dowiedzą się o protokole, którego nie umiemy obsłużyć.
pub fn probe() -> bool {
    if std::env::var("SELENE_EXPLICIT_SYNC")
        .is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("false"))
    {
        return false;
    }
    let device = DEVICE.get_or_init(open_device);
    let ok = device.is_some();
    if std::env::var_os("SELENE_STATS").is_some() {
        eprintln!(
            "selene/syncobj: jawna synchronizacja {}",
            if ok {
                "włączona"
            } else {
                "niedostępna (brak DRM_CAP_SYNCOBJ_TIMELINE)"
            }
        );
    }
    ok
}

/// Otwiera węzeł DRM z obsługą osi czasu syncobj.
///
/// Najpierw węzeł renderowania wybrany dla klientów (ten sam, na którym alokują
/// bufory `dmabuf`), potem pozostałe `renderD*`/`card*`.
fn open_device() -> Option<OwnedFd> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(node) = crate::dmabuf::render_node() {
        candidates.push(node);
    }
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("renderD") || name.starts_with("card") {
                let path = entry.path();
                if !candidates.contains(&path) {
                    candidates.push(path);
                }
            }
        }
    }
    for path in candidates {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let fd = OwnedFd::from(file);
        let mut cap = GetCap {
            capability: DRM_CAP_SYNCOBJ_TIMELINE,
            value: 0,
        };
        if unsafe { libc::ioctl(fd.as_raw_fd(), DRM_IOCTL_GET_CAP, &mut cap) } == 0
            && cap.value != 0
        {
            if std::env::var_os("SELENE_STATS").is_some() {
                eprintln!("selene/syncobj: urządzenie {}", path.display());
            }
            return Some(fd);
        }
    }
    None
}

/// Importuje oś czasu klienta (fd `drm_syncobj`) jako uchwyt w naszym urządzeniu.
fn import_timeline(fd: RawFd) -> Option<u32> {
    let device = device()?;
    let mut arg = SyncobjHandle {
        handle: 0,
        flags: 0,
        fd,
        pad: 0,
        point: 0,
    };
    let rc = unsafe { libc::ioctl(device.as_raw_fd(), DRM_IOCTL_SYNCOBJ_FD_TO_HANDLE, &mut arg) };
    (rc == 0 && arg.handle != 0).then_some(arg.handle)
}

/// Czeka, aż punkt `acquire` zostanie zasygnalizowany (bounded — nie zawieszamy sesji).
///
/// Zwraca `true`, gdy punkt doszedł (albo gdy synchronizacja jest niedostępna).
pub fn wait_acquire(point: Point, timeout: Duration) -> bool {
    let Some(device) = device() else {
        return true;
    };
    let handles = [point.timeline];
    let points = [point.value];
    let mut arg = TimelineWait {
        handles: handles.as_ptr() as u64,
        points: points.as_ptr() as u64,
        timeout_nsec: timeout.as_nanos().min(i64::MAX as u128) as i64,
        count_handles: 1,
        flags: WAIT_FOR_SUBMIT,
        first_signaled: 0,
        pad: 0,
        deadline_nsec: 0,
    };
    let rc = unsafe {
        libc::ioctl(
            device.as_raw_fd(),
            DRM_IOCTL_SYNCOBJ_TIMELINE_WAIT,
            &mut arg,
        )
    };
    if rc == 0 {
        return true;
    }
    // Timeout nie może blokować obrazu: komponujemy to, co mamy, i jedziemy dalej.
    if !TIMEOUT_LOGGED.swap(true, Ordering::Relaxed) && std::env::var_os("SELENE_STATS").is_some() {
        eprintln!("selene/syncobj: acquire point nie zdążył — komponuję mimo to");
    }
    false
}

/// Sygnalizuje punkt `release` (kompozytor skończył używać bufora).
pub fn signal_release(point: Point) {
    let Some(device) = device() else {
        return;
    };
    let handles = [point.timeline];
    let points = [point.value];
    let mut arg = TimelineArray {
        handles: handles.as_ptr() as u64,
        points: points.as_ptr() as u64,
        count_handles: 1,
        flags: 0,
    };
    let rc = unsafe {
        libc::ioctl(
            device.as_raw_fd(),
            DRM_IOCTL_SYNCOBJ_TIMELINE_SIGNAL,
            &mut arg,
        )
    };
    if rc != 0
        && !FAILED_LOGGED.swap(true, Ordering::Relaxed)
        && std::env::var_os("SELENE_STATS").is_some()
    {
        eprintln!("selene/syncobj: nie udało się zasygnalizować punktu release");
    }
}

/// Łączy `point_hi`/`point_lo` z protokołu w wartość 64-bitową.
fn combine_point(hi: u32, lo: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

fn destroy_handle(handle: u32) {
    let Some(device) = device() else {
        return;
    };
    let mut arg = SyncobjDestroy { handle, pad: 0 };
    let _ = unsafe { libc::ioctl(device.as_raw_fd(), DRM_IOCTL_SYNCOBJ_DESTROY, &mut arg) };
}

/// Zapamiętuje punkt `release` przy konkretnym buforze.
///
/// Sygnalizujemy go dopiero przy `wl_buffer.release` — to ten sam moment, w którym
/// kompozytor uznaje bufor za wolny (Selene oddaje bufory `dmabuf` o jeden commit
/// później, gdy GPU na pewno skończyło czytać). Wcześniejsze sygnalizowanie
/// pozwoliłoby klientowi nadpisać bufor jeszcze wyświetlany.
pub fn remember_release(data: &SurfaceData, buffer: &WlBuffer, point: Point) {
    let id = buffer.id().protocol_id();
    let mut points = data.release_points.lock_or_recover();
    // Jeden bufor = najwyżej jeden oczekujący punkt; powtórzenia nadpisujemy.
    points.retain(|(buffer_id, _)| *buffer_id != id);
    if points.len() >= 16 {
        points.remove(0);
    }
    points.push((id, point));
}

/// Sygnalizuje punkt `release` przypisany do właśnie oddawanego bufora.
pub fn release_buffer(data: &SurfaceData, buffer: &WlBuffer) {
    let id = buffer.id().protocol_id();
    let point = {
        let mut points = data.release_points.lock_or_recover();
        points
            .iter()
            .position(|(buffer_id, _)| *buffer_id == id)
            .map(|index| points.remove(index).1)
    };
    if let Some(point) = point {
        signal_release(point);
    }
}

// ── Protokół ──

struct TimelineData {
    handle: u32,
}

struct SyncSurfaceData {
    surface: WlSurface,
}

impl GlobalDispatch<manager::WpLinuxDrmSyncobjManagerV1, ()> for crate::state::State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<manager::WpLinuxDrmSyncobjManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<manager::WpLinuxDrmSyncobjManagerV1, ()> for crate::state::State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &manager::WpLinuxDrmSyncobjManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            manager::Request::ImportTimeline { id, fd } => {
                let handle = import_timeline(fd.as_raw_fd());
                if let Some(handle) = handle {
                    data_init.init(id, TimelineData { handle });
                } else if std::env::var_os("SELENE_STATS").is_some() {
                    eprintln!("selene/syncobj: import osi czasu nie powiódł się");
                }
            }
            manager::Request::GetSurface { id, surface } => {
                data_init.init(id, SyncSurfaceData { surface });
            }
            manager::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<timeline::WpLinuxDrmSyncobjTimelineV1, TimelineData> for crate::state::State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &timeline::WpLinuxDrmSyncobjTimelineV1,
        request: timeline::Request,
        data: &TimelineData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let timeline::Request::Destroy = request {
            destroy_handle(data.handle);
        }
    }
}

impl Dispatch<sync_surface::WpLinuxDrmSyncobjSurfaceV1, SyncSurfaceData> for crate::state::State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &sync_surface::WpLinuxDrmSyncobjSurfaceV1,
        request: sync_surface::Request,
        data: &SyncSurfaceData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Some(sd) = data.surface.data::<SurfaceData>() else {
            return;
        };
        match request {
            sync_surface::Request::SetAcquirePoint {
                timeline,
                point_hi,
                point_lo,
            } => {
                if let Some(td) = timeline.data::<TimelineData>() {
                    *sd.sync_acquire.lock_or_recover() = Some(Point {
                        timeline: td.handle,
                        value: combine_point(point_hi, point_lo),
                    });
                }
            }
            sync_surface::Request::SetReleasePoint {
                timeline,
                point_hi,
                point_lo,
            } => {
                if let Some(td) = timeline.data::<TimelineData>() {
                    *sd.sync_release.lock_or_recover() = Some(Point {
                        timeline: td.handle,
                        value: combine_point(point_hi, point_lo),
                    });
                }
            }
            sync_surface::Request::Destroy => {
                *sd.sync_acquire.lock_or_recover() = None;
                *sd.sync_release.lock_or_recover() = None;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_combines_hi_and_lo() {
        assert_eq!(combine_point(0, 0), 0);
        assert_eq!(combine_point(1, 0), 0x0000_0001_0000_0000);
        assert_eq!(combine_point(0, 2), 2);
        assert_eq!(combine_point(1, 2), 0x0000_0001_0000_0002);
        assert_eq!(combine_point(u32::MAX, u32::MAX), u64::MAX);
    }
}
