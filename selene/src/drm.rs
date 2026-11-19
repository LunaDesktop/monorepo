//! Wykrywanie monitorów przez DRM/KMS (przenośne — działa na dowolnym Linuksie z `/dev/dri`).
//!
//! To świadomie **szkic**: otwieramy karty DRM, odpytujemy KMS o podłączone konektory
//! i ich tryby, a wynik zwracamy jako model [`OutputInfo`]. Gdy DRM jest niedostępne
//! (np. w środowisku bez uprawnień albo w trybie nested), zwracamy `None` i kompozytor
//! korzysta z pojedynczego wyjścia syntetycznego.
//!
//! Skanowanie (page-flip, alokacja buforów, master) dochodzi później — tutaj chodzi o to,
//! by Selene wiedział, ile i jakich monitorów ma użyć.

use std::mem::MaybeUninit;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use rustix::fs::{Mode, OFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};

use libluna_core::{DisplaySettings, OutputMode};
use libluna_render::gpu::ExportedFrame;

use crate::monitors::{layout_horizontal, OutputInfo};

/// Znacznik startu procesu — logi startowe podają czas w ms, żeby z sesji dało
/// się odczytać, ile trwało dojście do gotowego ekranu (modeset na NVIDII bywa
/// najdłuższym etapem startu).
static START: OnceLock<Instant> = OnceLock::new();

/// Zapisuje chwilę startu procesu (woła `main`).
pub fn mark_start() {
    let _ = START.set(Instant::now());
}

fn startup_ms() -> u128 {
    START.get().map(|t| t.elapsed().as_millis()).unwrap_or(0)
}

// — Kody ioctl KMS (`_IOWR`) —
const DRM_IOCTL_MODE_GETRESOURCES: u32 = 0xC040_64A0;
const DRM_IOCTL_MODE_GETCONNECTOR: u32 = 0xC050_64A7;
const DRM_IOCTL_MODE_GETENCODER: u32 = 0xC014_64A6;
const DRM_IOCTL_MODE_GETCRTC: u32 = 0xC068_64A1;
const DRM_IOCTL_MODE_SETCRTC: u32 = 0xC068_64A2;
/// `drm_mode_crtc_lut` (`sizeof` = 32 = 0x20).
const DRM_IOCTL_MODE_SETGAMMA: u32 = 0xC020_64A4;
const DRM_IOCTL_MODE_CREATE_DUMB: u32 = 0xC020_64B2;
const DRM_IOCTL_MODE_MAP_DUMB: u32 = 0xC010_64B3;
const DRM_IOCTL_MODE_DESTROY_DUMB: u32 = 0xC004_64B4;
const DRM_IOCTL_MODE_ADDFB2: u32 = 0xC068_64B8;
/// `drm_mode_rmfb` — usuwa framebuffer (przy zmianie trybu na żywo).
const DRM_IOCTL_MODE_RMFB: u32 = 0xC004_64AF;
/// `drm_prime_handle` — import `dmabuf` do GEM (`PRIME_FD_TO_HANDLE`).
const DRM_IOCTL_PRIME_FD_TO_HANDLE: u32 = 0xC00C_642E;
/// `drm_gem_close` — zwalnia uchwyt GEM zaimportowanego bufora.
const DRM_IOCTL_GEM_CLOSE: u32 = 0x4008_6409;
const DRM_IOCTL_MODE_PAGE_FLIP: u32 = 0xC018_64B0;
/// `drm_mode_cursor` (`sizeof` = 28 = 0x1C).
const DRM_IOCTL_MODE_CURSOR: u32 = 0xC01C_64A3;
/// `drm_get_cap` (`sizeof` = 16 = 0x10).
const DRM_IOCTL_GET_CAP: u32 = 0xC010_640C;
/// `DRM_CAP_CURSOR_WIDTH` / `DRM_CAP_CURSOR_HEIGHT` — maksymalny wymiar
/// płaszczyzny kursora zgłaszany przez sterownik (NVIDIA: 256×256).
const DRM_CAP_CURSOR_WIDTH: u64 = 0x8;
const DRM_CAP_CURSOR_HEIGHT: u64 = 0x9;

// — ioctle bez argumentu (`_IO`) —
const DRM_IOCTL_SET_MASTER: u32 = 0x0000_641E;
const DRM_IOCTL_DROP_MASTER: u32 = 0x0000_641F;

const DRM_MODE_CONNECTED: u32 = 1;
/// `DRM_MODE_TYPE_PREFERRED` (bit 3) — natywny tryb monitora.
const DRM_MODE_TYPE_PREFERRED: u32 = 1 << 3;
/// `DRM_MODE_PAGE_FLIP_EVENT` — proś o zdarzenie po zakończeniu flipu.
const DRM_MODE_PAGE_FLIP_EVENT: u32 = 1;
/// Cztery znaki `XRGB8888` (kolejność bajtów B,G,R,X).
const DRM_FORMAT_XRGB8888: u32 = 0x3432_5258;
/// Cztery znaki `XBGR8888` (kolejność bajtów R,G,B,X) — **taka sama jak wyjście
/// tiny-skia**, więc klatkę można skopiować bez przepisywania kanałów.
const DRM_FORMAT_XBGR8888: u32 = 0x3432_4258;
/// Cztery znaki `ABGR8888` (kolejność bajtów R,G,B,A) — domyślny eksport `GL_RGBA8` z EGL.
const DRM_FORMAT_ABGR8888: u32 = 0x3432_4241;
/// `DRM_FORMAT_MOD_INVALID` — brak jawnego modyfikatora (np. starsze sterowniki EGL).
const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
/// Cztery znaki `ARGB8888` (kolejność bajtów B,G,R,A) — format kursora.
const DRM_FORMAT_ARGB8888: u32 = 0x3432_5241;
/// `DRM_MODE_CURSOR_BO` — ustaw obraz kursora (albo wyłącz, gdy `handle` = 0).
const DRM_MODE_CURSOR_BO: u32 = 0x01;
/// `DRM_MODE_CURSOR_MOVE` — przesuń kursor bez zmiany obrazu.
const DRM_MODE_CURSOR_MOVE: u32 = 0x02;
/// `DRM_EVENT_FLIP_COMPLETE`.
const DRM_EVENT_FLIP_COMPLETE: u32 = 2;

/// `struct drm_mode_card_res`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeCardRes {
    fb_id_ptr: u64,
    crtc_id_ptr: u64,
    connector_id_ptr: u64,
    encoder_id_ptr: u64,
    count_fbs: u32,
    count_crtcs: u32,
    count_connectors: u32,
    count_encoders: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
}

/// `struct drm_mode_get_connector`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeGetConnector {
    encoders_ptr: u64,
    modes_ptr: u64,
    props_ptr: u64,
    prop_values_ptr: u64,
    count_modes: u32,
    count_props: u32,
    count_encoders: u32,
    encoder_id: u32,
    connector_id: u32,
    connector_type: u32,
    connector_type_id: u32,
    connection: u32,
    mm_width: u32,
    mm_height: u32,
    subpixel: u32,
    pad: u32,
}

/// `struct drm_mode_modeinfo`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeInfo {
    clock: u32,
    hdisplay: u16,
    hsync_start: u16,
    hsync_end: u16,
    htotal: u16,
    hskew: u16,
    vdisplay: u16,
    vsync_start: u16,
    vsync_end: u16,
    vtotal: u16,
    vscan: u16,
    vrefresh: u32,
    flags: u32,
    type_: u32,
    name: [u8; 32],
}

/// `struct drm_mode_get_encoder`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeGetEncoder {
    encoder_id: u32,
    encoder_type: u32,
    crtc_id: u32,
    possible_crtcs: u32,
    possible_clones: u32,
}

/// `struct drm_mode_crtc` (używany też przez `GETCRTC`/`SETCRTC`).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeCrtc {
    set_connectors_ptr: u64,
    count_connectors: u32,
    crtc_id: u32,
    fb_id: u32,
    x: u32,
    y: u32,
    gamma_size: u32,
    mode_valid: u32,
    mode: ModeInfo,
}

/// `struct drm_mode_crtc_lut` — tablica korekcji gamma (night light).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeCrtcLut {
    crtc_id: u32,
    gamma_size: u32,
    red: u64,
    green: u64,
    blue: u64,
}

/// `struct drm_mode_create_dumb`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeCreateDumb {
    height: u32,
    width: u32,
    bpp: u32,
    flags: u32,
    handle: u32,
    pitch: u32,
    size: u64,
}

/// `struct drm_mode_map_dumb`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeMapDumb {
    handle: u32,
    pad: u32,
    offset: u64,
}

/// `struct drm_mode_destroy_dumb`.
#[repr(C)]
#[derive(Clone, Copy)]
struct ModeDestroyDumb {
    handle: u32,
}

/// `struct drm_mode_fb_cmd2`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeFbCmd2 {
    fb_id: u32,
    width: u32,
    height: u32,
    pixel_format: u32,
    flags: u32,
    handles: [u32; 4],
    pitches: [u32; 4],
    offsets: [u32; 4],
    modifier: [u64; 4],
}

/// `struct drm_prime_handle` — fd `dmabuf` → uchwyt GEM.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PrimeHandle {
    handle: u32,
    flags: u32,
    fd: i32,
}

/// `struct drm_gem_close` — zwolnienie uchwytu GEM.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GemClose {
    handle: u32,
    pad: u32,
}

/// `struct drm_mode_crtc_page_flip`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModePageFlip {
    crtc_id: u32,
    fb_id: u32,
    flags: u32,
    reserved: u32,
    user_data: u64,
}

/// `struct drm_mode_cursor` — ustawienie/przesunięcie kursora sprzętowego.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModeCursor {
    flags: u32,
    crtc_id: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    handle: u32,
}

/// `struct drm_get_cap`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GetCap {
    capability: u64,
    value: u64,
}

/// Opakowanie na ioctl typu „read-write” z jednym struktem wejścia/wyjścia.
struct Rw<T, const OP: u32>(MaybeUninit<T>);

impl<T, const OP: u32> Rw<T, OP> {
    fn new(value: T) -> Self {
        Self(MaybeUninit::new(value))
    }
}

unsafe impl<T: Copy, const OP: u32> rustix::ioctl::Ioctl for Rw<T, OP> {
    type Output = T;
    const OPCODE: rustix::ioctl::Opcode =
        rustix::ioctl::Opcode::old(OP as rustix::ioctl::RawOpcode);
    const IS_MUTATING: bool = true;

    fn as_ptr(&mut self) -> *mut core::ffi::c_void {
        self.0.as_mut_ptr().cast()
    }

    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        extract: *mut core::ffi::c_void,
    ) -> rustix::io::Result<Self::Output> {
        Ok(*(extract as *const T))
    }
}

/// Wykonuje ioctl „read-write” i zwraca zmodyfikowany strukt.
fn ioctl_rw<T: Copy, const OP: u32, F: AsFd>(fd: F, value: T) -> rustix::io::Result<T> {
    unsafe { rustix::ioctl::ioctl(fd, Rw::<T, OP>::new(value)) }
}

/// Opakowanie na ioctl typu „write-only” (np. `GEM_CLOSE`) — bez wyniku.
struct W<T, const OP: u32>(T);

impl<T, const OP: u32> W<T, OP> {
    fn new(value: T) -> Self {
        Self(value)
    }
}

unsafe impl<T: Copy, const OP: u32> rustix::ioctl::Ioctl for W<T, OP> {
    type Output = ();
    const OPCODE: rustix::ioctl::Opcode =
        rustix::ioctl::Opcode::old(OP as rustix::ioctl::RawOpcode);
    const IS_MUTATING: bool = true;

    fn as_ptr(&mut self) -> *mut core::ffi::c_void {
        (&mut self.0 as *mut T).cast()
    }

    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        _: *mut core::ffi::c_void,
    ) -> rustix::io::Result<Self::Output> {
        Ok(())
    }
}

fn ioctl_w<T: Copy, const OP: u32, F: AsFd>(fd: F, value: T) -> rustix::io::Result<()> {
    unsafe { rustix::ioctl::ioctl(fd, W::<T, OP>::new(value)) }
}

/// ioctl bez argumentu (np. `SET_MASTER`/`DROP_MASTER`).
struct NoArg<const OP: u32>;

unsafe impl<const OP: u32> rustix::ioctl::Ioctl for NoArg<OP> {
    type Output = ();
    const OPCODE: rustix::ioctl::Opcode =
        rustix::ioctl::Opcode::old(OP as rustix::ioctl::RawOpcode);
    const IS_MUTATING: bool = false;

    fn as_ptr(&mut self) -> *mut core::ffi::c_void {
        core::ptr::null_mut()
    }

    unsafe fn output_from_ptr(
        _: rustix::ioctl::IoctlOutput,
        _: *mut core::ffi::c_void,
    ) -> rustix::io::Result<()> {
        Ok(())
    }
}

fn ioctl_none<const OP: u32, F: AsFd>(fd: F) -> rustix::io::Result<()> {
    unsafe { rustix::ioctl::ioctl(fd, NoArg::<OP>) }
}

/// Lista ścieżek kart DRM (`/dev/dri/card*`), posortowana.
fn card_paths() -> Vec<PathBuf> {
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("card"))
                .unwrap_or(false)
        })
        .collect();
    cards.sort();
    cards
}

/// Nazwa sterownika DRM dla karty (np. `qxl`, `virtio_gpu`, `i915`).
fn driver_name(card: &PathBuf) -> Option<String> {
    let name = card.file_name()?;
    let link = std::fs::read_link(format!(
        "/sys/class/drm/{}/device/driver",
        name.to_string_lossy()
    ))
    .ok()?;
    link.file_name()
        .map(|name| name.to_string_lossy().to_string())
}

/// Czy sterownik jest wirtualny (QEMU/SPICE, virtio, VirtualBox, VKMS).
///
/// Takie karty nie obsługują `XBGR8888` na płaszczyźnie głównej — akceptują
/// `ADDFB2`, ale obraz wychodzi z zamienionymi kanałami R/B. Dlatego od razu
/// wybieramy `XRGB8888` i przepisujemy kanały przy prezentacji.
///
/// Zmierzone na pełnej klatce 1920×1080: kopia `memcpy` ~0,4 ms, zamiana kanałów
/// ~0,7 ms (`bench_present_copy`). Przy znanym damage i tak kopiujemy tylko brudny
/// prostokąt, więc bezpośrednie rysowanie do scanoutu nie dałoby tu istotnego zysku.
fn is_virtual_driver(driver: Option<&str>) -> bool {
    matches!(
        driver,
        Some(
            "qxl"
                | "virtio_gpu"
                | "virtio-gpu"
                | "bochs"
                | "vboxvideo"
                | "vkms"
                | "cirrus"
                | "mgag200"
        )
    )
}

/// Kolejność formatów bufora scanoutu (pierwszy, który zadziała).
fn scanout_formats(driver: Option<&str>) -> &'static [u32] {
    if is_virtual_driver(driver) {
        &[DRM_FORMAT_XRGB8888]
    } else {
        &[DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888]
    }
}
/// Szczegółowy raport wykrywania (`SELENE_DRM_PROBE=1`).
///
/// Nie zmienia trybu i nie wymaga bycia DRM masterem — wypisuje wszystkie karty,
/// konektory i tryby zgłoszone przez EDID, razem z tym, który Selene by wybrał.
pub fn probe_report() -> String {
    let mut out = String::new();
    let mut used = false;
    for card in card_paths() {
        let Ok(fd) = rustix::fs::open(&card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) else {
            continue;
        };
        let master = ioctl_none::<DRM_IOCTL_SET_MASTER, _>(&fd).is_ok();
        if master {
            let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
        }
        let Some(res) = fetch_resources(fd.as_fd()) else {
            out.push_str(&format!(
                "{}: master={} (nie można odczytać zasobów)\n",
                card.display(),
                master
            ));
            continue;
        };
        let mut connected = 0usize;
        let mut report = String::new();
        for id in &res.connectors {
            let Some((c, modes)) = read_connector(fd.as_fd(), *id) else {
                continue;
            };
            let name = connector_name(c.connector_type, c.connector_type_id);
            if c.connection != DRM_MODE_CONNECTED {
                report.push_str(&format!("  {name}: disconnected\n"));
                continue;
            }
            connected += 1;
            report.push_str(&format!(
                "  {name}: connected, {}x{} mm, tryby={}\n",
                c.mm_width,
                c.mm_height,
                modes.len()
            ));
            for m in &modes {
                let preferred = m.type_ & DRM_MODE_TYPE_PREFERRED != 0;
                report.push_str(&format!(
                    "    {}x{}@{} Hz{}\n",
                    m.hdisplay,
                    m.vdisplay,
                    m.vrefresh,
                    if preferred { "  [PREFERRED]" } else { "" }
                ));
            }
            if let Some(best) = pick_best_mode(&modes) {
                report.push_str(&format!(
                    "    → wybrany: {}x{}@{} Hz{}\n",
                    best.hdisplay,
                    best.vdisplay,
                    best.vrefresh,
                    if best.type_ & DRM_MODE_TYPE_PREFERRED != 0 {
                        " [PREFERRED]"
                    } else {
                        ""
                    }
                ));
            }
        }
        out.push_str(&format!(
            "{}: master={} konektory={} podłączone={}{}\n",
            card.display(),
            master,
            res.connectors.len(),
            connected,
            if !used && connected > 0 {
                "  ← discover() użyje tej karty"
            } else {
                ""
            }
        ));
        out.push_str(&report);
        if connected > 0 {
            used = true;
        }
    }
    if out.is_empty() {
        out.push_str("brak kart DRM w /dev/dri\n");
    }
    out
}

/// Odczytuje konektor wraz z pełną listą trybów (bez zmiany trybu).
fn read_connector(fd: BorrowedFd, id: u32) -> Option<(ModeGetConnector, Vec<ModeInfo>)> {
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(
        fd,
        ModeGetConnector {
            connector_id: id,
            ..ModeGetConnector::default()
        },
    )
    .ok()?;
    let count = c.count_modes as usize;
    if c.connection != DRM_MODE_CONNECTED || count == 0 {
        return Some((c, Vec::new()));
    }
    let mut modes = vec![ModeInfo::default(); count];
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(
        fd,
        ModeGetConnector {
            connector_id: id,
            count_modes: count as u32,
            modes_ptr: modes.as_mut_ptr() as u64,
            ..ModeGetConnector::default()
        },
    )
    .ok()?;
    Some((c, modes))
}

/// Wykrywa monitory. `None` oznacza, że DRM/KMS nie jest dostępne (użyj wyjścia nested).
///
/// Preferujemy kartę, na której **można zostać masterem** — dokładnie tę samą, którą
/// wybierze [`DrmBackend::open`]. Wcześniej braliśmy pierwszą kartę z podłączonymi
/// monitorami, więc na maszynie z dwoma GPU model wyjść mógł pochodzić z innej karty
/// niż faktyczny scanout.
pub fn discover() -> Option<Vec<OutputInfo>> {
    let stats = std::env::var("SELENE_STATS").is_ok();
    let mut fallback: Option<Vec<OutputInfo>> = None;
    for card in card_paths() {
        let Ok(fd) = rustix::fs::open(&card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) else {
            continue;
        };
        let master = ioctl_none::<DRM_IOCTL_SET_MASTER, _>(&fd).is_ok();
        if master {
            let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
        }
        let Some(mut outs) = probe_card(fd.as_fd()) else {
            continue;
        };
        if outs.is_empty() {
            continue;
        }
        layout_horizontal(&mut outs);
        if stats {
            eprintln!(
                "selene/drm: {} master={} wyjścia={}",
                card.display(),
                master,
                outs.len()
            );
        }
        if master {
            return Some(outs);
        }
        fallback.get_or_insert(outs);
    }
    fallback
}

/// Odpytuje jedną kartę DRM o podłączone konektory.
fn probe_card(fd: BorrowedFd) -> Option<Vec<OutputInfo>> {
    // Najpierw liczymy konektory (bez tablic).
    let res =
        ioctl_rw::<ModeCardRes, DRM_IOCTL_MODE_GETRESOURCES, _>(fd, ModeCardRes::default()).ok()?;
    let count = res.count_connectors as usize;
    if count == 0 {
        return Some(Vec::new());
    }

    // Pobieramy identyfikatory konektorów.
    let mut ids = vec![0u32; count];
    let req = ModeCardRes {
        count_connectors: count as u32,
        connector_id_ptr: ids.as_mut_ptr() as u64,
        ..ModeCardRes::default()
    };
    ioctl_rw::<ModeCardRes, DRM_IOCTL_MODE_GETRESOURCES, _>(fd, req).ok()?;

    let mut outs = Vec::new();
    for id in ids {
        if let Some(info) = probe_connector(fd, id) {
            outs.push(info);
        }
    }
    Some(outs)
}

/// Odpytuje jeden konektor: status, tryby, wybiera tryb natywny (lub pierwszy).
fn probe_connector(fd: BorrowedFd, id: u32) -> Option<OutputInfo> {
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(
        fd,
        ModeGetConnector {
            connector_id: id,
            ..ModeGetConnector::default()
        },
    )
    .ok()?;
    if c.connection != DRM_MODE_CONNECTED {
        return None;
    }
    let mode_count = c.count_modes as usize;
    if mode_count == 0 {
        return None;
    }

    let mut modes = vec![ModeInfo::default(); mode_count];
    let req = ModeGetConnector {
        connector_id: id,
        count_modes: mode_count as u32,
        modes_ptr: modes.as_mut_ptr() as u64,
        ..ModeGetConnector::default()
    };
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(fd, req).ok()?;

    let mode = pick_best_mode(&modes)?;

    let scale = if c.mm_width > 0 {
        let dpi = (mode.hdisplay as u64 * 254) / (c.mm_width as u64 * 10);
        if dpi >= 180 {
            2
        } else {
            1
        }
    } else if mode.hdisplay >= 3840 || mode.vdisplay >= 2160 {
        2
    } else {
        1
    };

    Some(OutputInfo {
        name: connector_name(c.connector_type, c.connector_type_id),
        width: mode.hdisplay as i32,
        height: mode.vdisplay as i32,
        refresh_hz: mode.vrefresh as i32,
        scale,
        x: 0,
        y: 0,
    })
}

/// Wybiera tryb monitora: **najpierw tryb natywny z EDID** (`PREFERRED`), a dopiero
/// potem największa rozdzielczość i najwyższy `vrefresh`.
///
/// Wcześniej kolejność była odwrotna („największa rozdzielczość, potem PREFERRED”),
/// więc monitor o natywnym 1600×900, który dodatkowo zgłasza 1920×1080, dostawał
/// 1920×1080 — obraz był skalowany przez monitor i wyglądał na rozmyty.
fn pick_best_mode(modes: &[ModeInfo]) -> Option<&ModeInfo> {
    modes.iter().max_by_key(|m| {
        let area = m.hdisplay as u64 * m.vdisplay as u64;
        let hz = m.vrefresh as u64;
        let preferred = u64::from(m.type_ & DRM_MODE_TYPE_PREFERRED != 0);
        (preferred, area, hz)
    })
}

/// Tryb zapisany przez użytkownika, a gdy go nie ma (albo monitor go nie zgłasza)
/// — automatyczny wybór [`pick_best_mode`].
fn pick_mode(modes: &[ModeInfo], wanted: Option<OutputMode>) -> Option<&ModeInfo> {
    if let Some(wanted) = wanted {
        let mut candidates = modes.iter().filter(|mode| {
            OutputMode::new(
                mode.hdisplay,
                mode.vdisplay,
                mode.vrefresh.min(u16::MAX as u32) as u16,
            )
            .matches(wanted)
        });
        let hit = if wanted.refresh == 0 {
            // Sama rozdzielczość — bierzemy najwyższe dostępne odświeżanie.
            candidates.max_by_key(|mode| mode.vrefresh)
        } else {
            candidates.next()
        };
        if hit.is_some() {
            return hit;
        }
    }
    pick_best_mode(modes)
}

/// Nazwa konektora w stylu Wayland (np. `HDMI-A-1`, `eDP-1`).
fn connector_name(kind: u32, index: u32) -> String {
    // Wartości `enum drm_mode_connector_type` z `drm_mode.h` — **nie przesuwać**.
    // Wcześniej mapa była przesunięta od 6 w górę: prawdziwy DP (10) nazywał się
    // „HDMI-A”, a HDMI-A (11) — „HDMI-B”, przez co nazwy nie zgadzały się z `/sys`
    // i Ustawienia widziały „trzeci” monitor (duplikat z innej listy).
    let base = match kind {
        1 => "VGA",
        2 => "DVI-I",
        3 => "DVI-D",
        4 => "DVI-A",
        5 => "Composite",
        6 => "SVIDEO",
        7 => "LVDS",
        8 => "Component",
        9 => "DIN",
        10 => "DP",
        11 => "HDMI-A",
        12 => "HDMI-B",
        13 => "TV",
        14 => "eDP",
        15 => "Virtual",
        16 => "DSI",
        17 => "DPI",
        18 => "Writeback",
        _ => "Connector",
    };
    format!("{base}-{index}")
}

// ————————————————————————————————————————————————————————————————————————
// Scanout: realne wyjście na monitory przez KMS (dumb buffer + page-flip)
// ————————————————————————————————————————————————————————————————————————

/// Zasoby karty: identyfikatory konektorów, CRTC i enkoderów.
struct Resources {
    connectors: Vec<u32>,
    crtcs: Vec<u32>,
    #[allow(dead_code)]
    encoders: Vec<u32>,
}

fn fetch_resources(fd: BorrowedFd) -> Option<Resources> {
    let res =
        ioctl_rw::<ModeCardRes, DRM_IOCTL_MODE_GETRESOURCES, _>(fd, ModeCardRes::default()).ok()?;
    let (cn, cc, ce) = (
        res.count_connectors as usize,
        res.count_crtcs as usize,
        res.count_encoders as usize,
    );
    let (mut connectors, mut crtcs, mut encoders) =
        (vec![0u32; cn], vec![0u32; cc], vec![0u32; ce]);
    let req = ModeCardRes {
        count_connectors: cn as u32,
        connector_id_ptr: connectors.as_mut_ptr() as u64,
        count_crtcs: cc as u32,
        crtc_id_ptr: crtcs.as_mut_ptr() as u64,
        count_encoders: ce as u32,
        encoder_id_ptr: encoders.as_mut_ptr() as u64,
        ..ModeCardRes::default()
    };
    ioctl_rw::<ModeCardRes, DRM_IOCTL_MODE_GETRESOURCES, _>(fd, req).ok()?;
    Some(Resources {
        connectors,
        crtcs,
        encoders,
    })
}

/// Bufor „dumb” (bez GPU): sterta pamięci zmapowana do renderowania i skanowania.
struct DumbBuffer {
    fb_id: u32,
    /// Uchwyt GEM (potrzebny m.in. do kursora sprzętowego).
    handle: u32,
    pitch: usize,
    size: usize,
    /// Rozmiar alokacji — kursor może zmienić rozmiar (motyw 24 px, klient 64 px),
    /// więc bufor odtwarzamy, zamiast pisać poza starszy, mniejszy zakres.
    width: u32,
    height: u32,
    ptr: *mut u8,
}

// Bezpieczne: bufor należy wyłącznie do wątku wyjścia i nie jest współdzielony.
unsafe impl Send for DumbBuffer {}

impl Drop for DumbBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                let _ = munmap(self.ptr.cast(), self.size);
            }
        }
    }
}

/// Jedno wyjście DRM: konektor + CRTC + podwójny bufor.
struct DrmOutput {
    name: String,
    /// Konektor, do którego przypięty jest CRTC (potrzebny przy zmianie trybu).
    connector_id: u32,
    crtc_id: u32,
    /// Rozmiar sprzętowej tablicy gamma (0 = brak wsparcia dla night lightu).
    gamma_size: u32,
    width: u32,
    height: u32,
    hz: u32,
    /// Wszystkie tryby zgłoszone przez monitor (bez duplikatów, malejąco).
    modes: Vec<OutputMode>,
    /// Surowa lista trybów z modesetu — `MODE_SETCRTC` wymaga pełnego `ModeInfo`.
    raw_modes: Vec<ModeInfo>,
    buffers: [DumbBuffer; 2],
    front: usize,
    /// Czy `present` musi przepisywać kanały (bufor `XRGB8888` zamiast `XBGR8888`).
    swizzle: bool,
    /// Bufor obrazu kursora sprzętowego (tworzony leniwie).
    cursor: Option<DumbBuffer>,
    /// Framebuffery z `dmabuf` (zero-copy), czekające na zwolnienie.
    /// Trzymamy je do momentu, gdy flip je zastąpi — wcześniejsze zwolnienie
    /// kazałoby kernelowi skanować zwolnioną pamięć.
    dmabuf_fbs: std::collections::VecDeque<(u32, Vec<u32>)>,
    /// Czy CRTC skanuje teraz framebuffer z `dmabuf` (zero-copy).
    ///
    /// Po udanym `present_dmabuf` bufory `dumb` są nieaktualne, a ostatni fb
    /// z `dmabuf_fbs` jest skanowany — nie wolno go jeszcze zwalniać.
    front_dmabuf: bool,
    /// Czy na tym CRTC wisi flip, na który nie odebraliśmy jeszcze
    /// `DRM_EVENT_FLIP_COMPLETE`.
    ///
    /// Dopóki jest `true`, nie wolno zwalniać framebufferów `dmabuf` (kernel może
    /// je jeszcze skanować) ani wysyłać kolejnego `PAGE_FLIP` (jądro zwróci
    /// `EBUSY`). Flaga schodzi dopiero po zdarzeniu z pasującym `user_data`.
    flip_pending: bool,
    /// Ile razy z rzędu nie doczekaliśmy się `FLIP_COMPLETE` do limitu czasu.
    ///
    /// Sterowniki po suspend/resume potrafią zgubić zdarzenie. Czekanie na nie
    /// w każdej klatce blokowałoby wątek wyjścia (kursor i okna „stoją”), więc
    /// po dwóch seriach przestajemy czekać i traktujemy klatkę jako pokazaną.
    flip_stalls: u8,
    /// Ile najbliższych prezentacji musi skopiować **całą** klatkę, mimo damage.
    ///
    /// Oba bufory `DumbBuffer` są spójne z historią damage tylko wtedy, gdy
    /// poprzednie klatki przeszły przez `present_region`. Po zero-copy
    /// (`present_dmabuf` zmienia framebuffer CRTC), po nieudanym flipie i po
    /// zmianie trybu/wybudzeniu zawartość „back” jest nieaktualna — wtedy damage
    /// rysowałby nowy fragment na starym tle (kwadratowe artefakty, mruganie
    /// wokół odtwarzacza wideo). Wartość 2 pokrywa podwójne buforowanie.
    force_full_frames: u8,
}

/// Region do skopiowania w `present_region`: `force_full_frames > 0` wymusza całą klatkę.
///
/// Damage jest poprawny tylko wtedy, gdy **oba** bufory `dumb` przeszły w poprzednich
/// klatkach przez `present_region`. Po zero-copy (`present_dmabuf` zmienia framebuffer
/// CRTC), po nieudanym flipie i po zmianie trybu/wybudzeniu zawartość bufora „back”
/// jest nieaktualna — wtedy nowy fragment trafiłby na stare tło (kwadratowe artefakty
/// i mruganie wokół odtwarzacza wideo w przeglądarce).
fn effective_region(force_full_frames: u8, region: Option<PixelRect>) -> Option<PixelRect> {
    if force_full_frames > 0 {
        None
    } else {
        region
    }
}

/// Ile framebufferów `dmabuf` musi pozostać w kolejce wyjścia.
///
/// `None` = nie wolno nic zwalniać: flip jeszcze trwa i kernel może skanować
/// dowolny z zakolejkowanych buforów. `Some(1)` = CRTC skanuje najnowszy fb
/// `dmabuf` (starsze wolno oddać). `Some(0)` = CRTC skanuje bufor `dumb`, więc
/// wszystkie framebuffery `dmabuf` są wolne.
fn dmabuf_keep(flip_pending: bool, front_dmabuf: bool) -> Option<usize> {
    if flip_pending {
        None
    } else if front_dmabuf {
        Some(1)
    } else {
        Some(0)
    }
}

/// Powód, dla którego nie udało się otworzyć wyjścia DRM (do diagnostyki).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrmError {
    /// Inna sesja trzyma mastera DRM — nie odbieramy ekranu.
    MasterBusy,
    /// Nie udało się odczytać zasobów karty (`GETRESOURCES`).
    Resources,
    /// Nie udało się ustawić trybu na żadnym podłączonym monitorze.
    NoOutputs,
}

/// Backend wyjścia DRM/KMS (jedna karta, wiele monitorów).
pub struct DrmBackend {
    fd: OwnedFd,
    outputs: Vec<DrmOutput>,
    /// Nazwa sterownika karty (`qxl`, `virtio_gpu`, `i915`, …).
    /// Sterowniki wirtualne nie obsługują `XBGR8888` — patrz [`scanout_formats`].
    driver: Option<String>,
    /// Maksymalny wymiar płaszczyzny kursora (`DRM_CAP_CURSOR_WIDTH/HEIGHT`).
    cursor_cap: (u32, u32),
}

impl Drop for DrmBackend {
    fn drop(&mut self) {
        let fd = self.fd.as_fd();
        for out in &mut self.outputs {
            for (fb, handles) in out.dmabuf_fbs.drain(..) {
                destroy_fb(fd, fb);
                release_gem(fd, &handles);
            }
            for buf in &out.buffers {
                destroy_fb(fd, buf.fb_id);
                destroy_dumb(fd, buf.handle);
            }
            if let Some(cur) = out.cursor.take() {
                destroy_dumb(fd, cur.handle);
            }
        }
        // Zwolnij bycie masterem, gdy kończymy.
        let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&self.fd);
    }
}

impl DrmBackend {
    /// Otwiera kartę, przejmuje mastera i ustawia tryb na wszystkich podłączonych monitorach.
    /// Zwraca `None`, gdy DRM jest niedostępne lub ktoś inny trzyma mastera (np. sesja użytkownika).
    ///
    /// `preferences` pochodzi z `~/.config/luna/displays.conf` — zapisany tryb
    /// (rozdzielczość + Hz) wygrywa z automatycznym wyborem „natywny/NAJWiększy”.
    pub fn open(preferences: &DisplaySettings) -> Result<Self, DrmError> {
        let mut last = DrmError::NoOutputs;
        for card in card_paths() {
            let Ok(fd) = rustix::fs::open(&card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
            else {
                continue;
            };
            if ioctl_none::<DRM_IOCTL_SET_MASTER, _>(&fd).is_err() {
                last = DrmError::MasterBusy; // inna sesja trzyma mastera
                continue;
            }
            let Some(res) = fetch_resources(fd.as_fd()) else {
                let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
                last = DrmError::Resources;
                continue;
            };
            let driver = driver_name(&card);
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/drm: {} sterownik={:?}",
                    card.display(),
                    driver.as_deref().unwrap_or("?")
                );
            }
            let mut outputs = Vec::new();
            let mut used_crtcs = Vec::new();
            let mut skipped = 0usize;
            for &conn in &res.connectors {
                if let Some(out) = setup_output(
                    fd.as_fd(),
                    conn,
                    &res,
                    &mut used_crtcs,
                    preferences,
                    driver.as_deref(),
                ) {
                    outputs.push(out);
                } else {
                    skipped += 1;
                }
            }
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/drm: skonfigurowano {} wyjść (pominięto {} z {} złączy) po {} ms",
                    outputs.len(),
                    skipped,
                    res.connectors.len(),
                    startup_ms()
                );
            }
            if outputs.is_empty() {
                let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
                last = DrmError::NoOutputs;
                continue;
            }
            let cursor_cap = cursor_cap_of(fd.as_fd());
            return Ok(Self {
                fd,
                outputs,
                driver,
                cursor_cap,
            });
        }
        Err(last)
    }

    /// `(nazwa, szerokość, wysokość, odświeżanie)` każdego wyjścia.
    pub fn outputs(&self) -> Vec<(String, u32, u32, u32)> {
        self.outputs
            .iter()
            .map(|o| (o.name.clone(), o.width, o.height, o.hz))
            .collect()
    }

    /// Wyjścia wraz z pełną listą trybów (dla Ustawień i pliku `luna-monitors`).
    pub fn outputs_with_modes(&self) -> Vec<(String, u32, u32, u32, Vec<OutputMode>)> {
        self.outputs
            .iter()
            .map(|o| (o.name.clone(), o.width, o.height, o.hz, o.modes.clone()))
            .collect()
    }

    /// Czy którykolwiek CRTC ma sprzętową tablicę gamma (night light).
    pub fn supports_night_light(&self) -> bool {
        self.outputs.iter().any(|o| o.gamma_size > 0)
    }

    /// Czy `ADDFB2` przyjmie modyfikator bufora — warunek zero-copy przez EGL.
    ///
    /// `nvidia-drm` odrzuca **każdy** modyfikator (`EINVAL`, zmierzone na
    /// sterowniku 615.78.08: `PRIME` przechodzi, `ADDFB2` nie), a liniowego
    /// bufora z GBM nie da się użyć ani jako celu renderu, ani jako tekstury.
    /// Zero-copy jest tam więc niewykonalne — zamiast płacić za nieudaną
    /// pierwszą klatkę i mylący log, od razu wybieramy render GPU + readback.
    pub fn supports_fb_modifiers(&self) -> bool {
        !matches!(self.driver.as_deref(), Some("nvidia"))
    }

    /// Czy można używać sprzętowej płaszczyzny kursora.
    ///
    /// Karty wirtualne (qxl, virtio_gpu, bochs, ...) mają płaszczyznę kursora,
    /// ale nie nadaje się ona do użytku: qxl wywraca jądro w
    /// `qxl_plane_prepare_fb` (zapis poza VRAM) już przy pierwszym
    /// `DRM_IOCTL_MODE_CURSOR`, a pozostałym zdarza się obraz z zamienionymi
    /// kanałami. Na takich kartach kursor rysuje scena (`cursor::paint`).
    pub fn supports_hw_cursor(&self) -> bool {
        !is_virtual_driver(self.driver.as_deref()) && self.cursor_cap.0 > 0 && self.cursor_cap.1 > 0
    }

    /// Maksymalny rozmiar kursora sprzętowego na tej karcie.
    pub fn cursor_size(&self) -> (u32, u32) {
        self.cursor_cap
    }

    /// Po powrocie z uśpienia: odzyskaj mastera DRM i odtwórz tryb na wyjściach.
    ///
    /// Jądro po suspend/resume może zgubić stan CRTC albo mastera DRM. Ponawiamy
    /// `SET_MASTER` (na już-istniejącym masterze zwraca błąd — ignorujemy go)
    /// i robimy modeset na bieżącym buforze każdego monitora.
    ///
    /// Zwraca `false`, gdy któryś CRTC odrzucił tryb — pętla wyjścia spróbuje
    /// ponownie przy następnej klatce (np. gdy monitor jeszcze się budzi).
    pub fn resume(&mut self) -> bool {
        let fd = self.fd.as_fd();
        let _ = ioctl_none::<DRM_IOCTL_SET_MASTER, _>(&fd);
        let mut ok = true;
        for out in &mut self.outputs {
            out.flip_pending = false;
            out.front_dmabuf = false;
            // Po wybudzeniu zawartość buforów jest nieznana.
            out.force_full_frames = 2;
            let wanted = OutputMode::new(out.width as u16, out.height as u16, out.hz as u16);
            let Some(mode) = pick_mode(&out.raw_modes, Some(wanted)).copied() else {
                ok = false;
                continue;
            };
            if !set_crtc(
                fd,
                out.crtc_id,
                out.buffers[out.front].fb_id,
                out.connector_id,
                &mode,
            ) {
                ok = false;
            }
        }
        ok
    }

    /// Zwalnia DRM master — **przed** przełączeniem VT.
    ///
    /// Konsola jądra (fbcon) może przejąć ekran tylko wtedy, gdy nikt nie trzyma
    /// mastera; bez tego nowy VT zostaje czarny. Master odzyskuje [`Self::resume`]
    /// po powrocie na nasz VT.
    pub fn drop_master(&mut self) {
        let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&self.fd.as_fd());
    }

    /// Zmienia tryb wyjścia **na żywo** (bez restartu sesji).
    ///
    /// Tworzy nowe bufory i framebuffery, przepina CRTC i dopiero po sukcesie
    /// zwalnia stare framebuffery; stare bufory znikają przy przypisaniu nowych.
    /// Zwraca indeks wyjścia przy sukcesie, `None` gdy monitor nie istnieje,
    /// nie zgłasza takiego trybu albo jądro odrzuciło modeset.
    pub fn set_mode(&mut self, name: &str, wanted: Option<OutputMode>) -> Option<usize> {
        let index = self.outputs.iter().position(|out| out.name == name)?;
        let fd = self.fd.as_fd();
        let (connector_id, crtc_id, current, mode) = {
            let out = &self.outputs[index];
            let mode = *pick_mode(&out.raw_modes, wanted)?;
            (
                out.connector_id,
                out.crtc_id,
                (out.width, out.height, out.hz),
                mode,
            )
        };
        if current == (mode.hdisplay as u32, mode.vdisplay as u32, mode.vrefresh) {
            return Some(index);
        }

        let width = mode.hdisplay as u32;
        let height = mode.vdisplay as u32;
        let make_pair = |format: u32| -> Option<(DumbBuffer, DumbBuffer)> {
            Some((
                create_dumb(fd, width, height, format)?,
                create_dumb(fd, width, height, format)?,
            ))
        };
        // Ta sama kolejność formatów co przy starcie: karty wirtualne (QXL)
        // potrzebują `XRGB8888` z przepisaniem kanałów.
        let mut chosen = None;
        for &format in scanout_formats(self.driver.as_deref()) {
            let Some((b0, b1)) = make_pair(format) else {
                continue;
            };
            if set_crtc(fd, crtc_id, b0.fb_id, connector_id, &mode) {
                chosen = Some((format, b0, b1));
                break;
            }
            destroy_fb(fd, b0.fb_id);
            destroy_fb(fd, b1.fb_id);
            destroy_dumb(fd, b0.handle);
            destroy_dumb(fd, b1.handle);
        }
        let (format, b0, b1) = chosen?;
        let swizzle = format == DRM_FORMAT_XRGB8888;

        let out = &mut self.outputs[index];
        for old in &out.buffers {
            destroy_fb(fd, old.fb_id);
            destroy_dumb(fd, old.handle);
        }
        out.width = width;
        out.height = height;
        out.hz = mode.vrefresh;
        out.swizzle = swizzle;
        out.front = 0;
        out.flip_pending = false;
        out.front_dmabuf = false;
        out.force_full_frames = 2;
        out.buffers = [b0, b1];
        // Bufory zero-copy sprzed zmiany trybu nie pasują już do nowego framebuffera.
        for (fb, handles) in out.dmabuf_fbs.drain(..) {
            destroy_fb(fd, fb);
            release_gem(fd, &handles);
        }
        // Bufor kursora zostanie utworzony na nowo przy następnym ruchu.
        if let Some(cur) = out.cursor.take() {
            destroy_dumb(fd, cur.handle);
        }
        Some(index)
    }

    /// Ustawia ciepłą korektę gamma na wszystkich wyjściach.
    ///
    /// `percent` = 0 przywraca neutralną rampę, 100 to najcieplejszy wariant.
    /// Zwraca `true`, jeśli udało się ustawić LUT na przynajmniej jednym CRTC.
    pub fn set_night_light(&self, percent: u8) -> bool {
        let strength = percent.min(100) as f64 / 100.0;
        let mut any = false;
        for out in &self.outputs {
            if out.gamma_size == 0 {
                continue;
            }
            if set_gamma_lut(self.fd.as_fd(), out.crtc_id, out.gamma_size, strength) {
                any = true;
            }
        }
        any
    }

    /// Wypycha klatkę (RGBA, wymiary = rozmiar wyjścia) i kolejkuje page-flip.
    /// Po wywołaniu należy poczekać na [`DrmBackend::wait_flip`].
    pub fn present(&mut self, idx: usize, rgba: &[u8]) -> bool {
        self.present_region(idx, rgba, None)
    }

    /// Jak [`DrmBackend::present`], ale kopiuje tylko `region` (piksele fizyczne).
    ///
    /// Przy podwójnym buforowaniu wołający musi podać sumę damage z **dwóch**
    /// ostatnich klatek: bufor docelowy był ostatnio prezentowany dwa flipy temu.
    /// `None` = cała klatka (bezpieczny fallback).
    pub fn present_region(&mut self, idx: usize, rgba: &[u8], region: Option<PixelRect>) -> bool {
        // Flipy poprzedniej klatki muszą się zakończyć, zanim nadpiszemy bufor,
        // który kernel jeszcze skanuje (i zanim wyślemy kolejny — inaczej `EBUSY`).
        self.settle_flips(idx);
        // Framebuffery `dmabuf`, których CRTC już nie skanuje, można oddać dopiero
        // teraz — po odebraniu `FLIP_COMPLETE`.
        self.prune_dmabuf_fbs(idx);
        let Some((back, fb_id, copied, name, width, height)) = self.outputs.get(idx).map(|out| {
            let back = (out.front + 1) & 1;
            let (ptr, pitch) = (out.buffers[back].ptr, out.buffers[back].pitch);
            let (w, h) = (out.width as usize, out.height as usize);
            // Po zero-copy, nieudanym flipie lub zmianie trybu historia damage
            // nie opisuje zawartości tego bufora — kopiujemy całą klatkę.
            let region = effective_region(out.force_full_frames, region);
            let copied = match (region, out.swizzle) {
                (None, true) => swizzle_into(ptr, pitch, rgba, w, h),
                (None, false) => copy_rows_into(ptr, pitch, rgba, w, h),
                (Some(r), true) => swizzle_region_into(ptr, pitch, rgba, w, h, r),
                (Some(r), false) => copy_region_into(ptr, pitch, rgba, w, h, r),
            };
            (
                back,
                out.buffers[back].fb_id,
                copied,
                out.name.clone(),
                out.width,
                out.height,
            )
        }) else {
            return false;
        };
        // Rozmiar klatki renderera musi zgadzać się z trybem wyjścia. Gdy się nie
        // zgadza (np. renderer został z poprzedniego trybu), **nie wolno** robić
        // page-flipu: pokazalibyśmy nieodświeżony bufor, czyli migotanie.
        if !copied {
            self.outputs[idx].force_full_frames = 2;
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/drm: {} pominięty flip — klatka {} B, wyjście {}x{}",
                    name,
                    rgba.len(),
                    width,
                    height
                );
            }
            return false;
        }
        if !self.queue_page_flip(idx, fb_id) {
            // Bufor „back” nie trafił na ekran — następna klatka musi być pełna,
            // inaczej damage narysuje fragment na nieaktualnej zawartości.
            self.outputs[idx].force_full_frames = 2;
            return false;
        }
        let out = &mut self.outputs[idx];
        out.front = back;
        out.front_dmabuf = false;
        out.force_full_frames = out.force_full_frames.saturating_sub(1);
        true
    }

    /// Czeka, aż wyjście `idx` nie ma oczekującego flipa (z limitem czasu).
    ///
    /// Normalnie flaga jest już zdjęta przez `wait_flips_any()` w pętli kompozytora;
    /// to zabezpieczenie na wypadek zgubionego zdarzenia albo wywołania
    /// prezentacji poza główną pętlą.
    fn settle_flips(&mut self, idx: usize) {
        let Some(out) = self.outputs.get(idx) else {
            return;
        };
        if !out.flip_pending {
            return;
        }
        // Po dwóch seriach zgubionych zdarzeń nie blokujemy już wątku renderu.
        // Wcześniej podwójne czekanie po 200 ms dawało do 400 ms zamrożenia
        // kursora i okien przy każdej klatce.
        if out.flip_stalls >= 2 {
            // Zgubione zdarzenie: nie blokujemy wątku, ale stan buforów jest
            // nieznany — następne klatki kopiujemy w całości.
            self.outputs[idx].flip_pending = false;
            self.outputs[idx].force_full_frames = 2;
            return;
        }
        // Czekamy na flip **własnego** wyjścia. Czekanie na wszystkie
        // monitory sprawiało, że 75 Hz traciło klatkę przy każdym vblanku 60 Hz.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
        while self.outputs.get(idx).is_some_and(|o| o.flip_pending) {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            if !self.wait_flips_any(remaining) {
                break;
            }
        }
    }

    /// Zwalnia framebuffery `dmabuf`, których CRTC już nie skanuje.
    ///
    /// Wolno to zrobić **tylko** bez oczekującego flipa: po `FLIP_COMPLETE`
    /// aktywne są albo bufory `dumb` (wtedy wszystkie fb `dmabuf` są wolne),
    /// albo ostatni fb `dmabuf` (wtedy wolno zwolnić starsze, nie ten).
    fn prune_dmabuf_fbs(&mut self, idx: usize) {
        let Some(keep) = self
            .outputs
            .get(idx)
            .and_then(|out| dmabuf_keep(out.flip_pending, out.front_dmabuf))
        else {
            return;
        };
        let fd = self.fd.as_fd();
        let out = &mut self.outputs[idx];
        while out.dmabuf_fbs.len() > keep {
            if let Some((fb, handles)) = out.dmabuf_fbs.pop_front() {
                destroy_fb(fd, fb);
                release_gem(fd, &handles);
            }
        }
    }

    /// Kolejkuje page-flip na CRTC wyjścia `idx`; przy przejściowym `EBUSY`
    /// zbiera zaległe zdarzenia flipów i ponawia raz.
    fn queue_page_flip(&mut self, idx: usize, fb_id: u32) -> bool {
        let Some(crtc_id) = self.outputs.get(idx).map(|o| o.crtc_id) else {
            return false;
        };
        let req = ModePageFlip {
            crtc_id,
            fb_id,
            flags: DRM_MODE_PAGE_FLIP_EVENT,
            reserved: 0,
            user_data: idx as u64,
        };
        match ioctl_rw::<ModePageFlip, DRM_IOCTL_MODE_PAGE_FLIP, _>(&self.fd, req) {
            Ok(_) => {
                self.outputs[idx].flip_pending = true;
                true
            }
            Err(rustix::io::Errno::BUSY) => {
                // `EBUSY` = poprzedni flip tego CRTC jeszcze trwa. Czekamy tylko
                // na **to** wyjście, nie na wszystkie monitory.
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
                while self.outputs.get(idx).is_some_and(|o| o.flip_pending) {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    if !self.wait_flips_any(remaining) {
                        break;
                    }
                }
                if ioctl_rw::<ModePageFlip, DRM_IOCTL_MODE_PAGE_FLIP, _>(&self.fd, req).is_ok() {
                    self.outputs[idx].flip_pending = true;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Prezentuje klatkę **bez kopiowania przez CPU** (zero-copy skanout).
    ///
    /// `dmabuf` z tekstury GPU → `PRIME_FD_TO_HANDLE` → `AddFB2WithModifiers` →
    /// page-flip. Zamiast 8–13 MB `glReadPixels` + memcpy na klatkę mamy kilka
    /// ioctlów. Zwraca `false`, gdy sterownik odrzucił bufor — wołający robi wtedy
    /// zwykły readback (`present`), więc zero-copy nigdy nie psuje obrazu.
    pub fn present_dmabuf(&mut self, idx: usize, frame: &ExportedFrame) -> bool {
        let Some((out_w, out_h)) = self.outputs.get(idx).map(|o| (o.width, o.height)) else {
            return false;
        };
        let reject = |reason: &str| {
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!(
                    "selene/drm: skanout odrzucony na wyjściu {idx}: {reason} (fourcc 0x{:08x}, modifier 0x{:016x})",
                    frame.fourcc, frame.modifier
                );
            }
            false
        };
        if frame.width != out_w
            || frame.height != out_h
            || frame.planes.is_empty()
            || frame.planes.len() > 4
        {
            return reject("rozmiar/liczba planów");
        }
        // Framebuffery z dwóch klatek wstecz są już po flipie (na flipy czekamy
        // na początku iteracji), więc wolno je zwolnić.
        // Zero-copy też musi poczekać na zakończenie poprzedniego flipa — inaczej
        // jądro zwróci `EBUSY` i klatka przepadnie. Przy okazji zwalniamy fb,
        // których CRTC już nie skanuje (tylko po `FLIP_COMPLETE`).
        self.settle_flips(idx);
        self.prune_dmabuf_fbs(idx);

        let fd = self.fd.as_fd();
        let mut handles: Vec<u32> = Vec::with_capacity(frame.planes.len());
        for (plane_fd, _offset, _stride) in &frame.planes {
            let request = PrimeHandle {
                handle: 0,
                flags: 0,
                fd: plane_fd.as_raw_fd(),
            };
            match ioctl_rw::<PrimeHandle, DRM_IOCTL_PRIME_FD_TO_HANDLE, _>(fd, request) {
                Ok(done) if done.handle != 0 => handles.push(done.handle),
                _ => {
                    release_gem(fd, &handles);
                    return reject("PRIME_FD_TO_HANDLE");
                }
            }
        }

        // Bufor liniowy (`0`) i brak modyfikatora (`DRM_FORMAT_MOD_INVALID` / `u64::MAX`)
        // dodajemy bez flagi `DRM_MODE_FB_MODIFIERS` — `nvidia-drm` i starsze sterowniki
        // zwracają `EINVAL`, gdy flaga jest ustawiona dla liniowego lub nieznanego modyfikatora.
        let has_modifier = frame.modifier != 0
            && frame.modifier != DRM_FORMAT_MOD_INVALID
            && frame.modifier != u64::MAX;
        let mut fb_desc = ModeFbCmd2 {
            fb_id: 0,
            width: out_w,
            height: out_h,
            pixel_format: frame.fourcc,
            flags: if has_modifier { 2 } else { 0 },
            ..ModeFbCmd2::default()
        };
        for (i, handle) in handles.iter().enumerate() {
            fb_desc.handles[i] = *handle;
        }
        for (i, (_, offset, stride)) in frame.planes.iter().enumerate() {
            fb_desc.pitches[i] = *stride;
            fb_desc.offsets[i] = *offset;
            if has_modifier {
                fb_desc.modifier[i] = frame.modifier;
            }
        }
        let added = match ioctl_rw::<ModeFbCmd2, DRM_IOCTL_MODE_ADDFB2, _>(fd, fb_desc) {
            Ok(added) if added.fb_id != 0 => added,
            _ if frame.fourcc == DRM_FORMAT_ABGR8888 => {
                // Część sterowników odrzuca format z kanałem alfa (`ABGR8888`) na
                // płaszczyźnie głównej, ale akceptuje identyczny w pamięci `XBGR8888`.
                fb_desc.pixel_format = DRM_FORMAT_XBGR8888;
                match ioctl_rw::<ModeFbCmd2, DRM_IOCTL_MODE_ADDFB2, _>(fd, fb_desc) {
                    Ok(added) if added.fb_id != 0 => added,
                    _ => {
                        release_gem(fd, &handles);
                        return reject("ADDFB2");
                    }
                }
            }
            _ => {
                release_gem(fd, &handles);
                return reject("ADDFB2");
            }
        };

        if !self.queue_page_flip(idx, added.fb_id) {
            let fd = self.fd.as_fd();
            destroy_fb(fd, added.fb_id);
            release_gem(fd, &handles);
            return reject("PAGE_FLIP");
        }
        let out = &mut self.outputs[idx];
        out.dmabuf_fbs.push_back((added.fb_id, handles));
        out.front_dmabuf = true;
        // Bufory `dumb` nie są już aktualne — dopiero po powrocie na readback
        // trzeba będzie skopiować całą klatkę.
        out.force_full_frames = 2;
        true
    }

    /// Czeka na zakończenie wszystkich oczekujących page-flipów (zdarzenia z jądra).
    ///
    /// Obsługuje wiele monitorów naraz (dopasowując `user_data` do indeksu wyjścia),
    /// `EINTR` na `poll`/`read` oraz bezpieczny limit czasu po suspend/resume.
    /// Czy któreś wyjście ma jeszcze niezakończony page-flip.
    ///
    /// Wątek renderu pyta o to przed czekaniem: licznik `pending_flips`
    /// w pętli wyjścia może się rozjechać po modesecie, a stan backendu jest
    /// wtedy jedynym wiarygodnym źródłem prawdy.
    pub fn has_pending_flips(&self) -> bool {
        self.outputs.iter().any(|o| o.flip_pending)
    }

    /// Czy **to** wyjście czeka na zakończenie page-flipa.
    ///
    /// Prezentacja jest per wyjście: 75 Hz nie może czekać na vblank 60 Hz.
    pub fn flip_pending(&self, idx: usize) -> bool {
        self.outputs.get(idx).is_some_and(|o| o.flip_pending)
    }

    /// Czeka na **pierwszy** zakończony page-flip (dowolnego wyjścia).
    ///
    /// Wyjścia mają różne odświeżanie (np. 75 Hz + 60 Hz). Czekanie na
    /// wszystkie naraz kazało szybszemu monitorowi gubić co drugi vblank
    /// (widoczny wzór 37,5 kl./s). Zwraca `true`, gdy co najmniej jeden flip
    /// się zakończył, `false` po przekroczeniu `max_wait` lub błędzie.
    pub fn wait_flips_any(&mut self, max_wait: std::time::Duration) -> bool {
        use rustix::event::{poll, PollFd, PollFlags};
        use std::time::Instant;

        if !self.outputs.iter().any(|o| o.flip_pending) {
            return false;
        }
        let deadline = Instant::now() + max_wait;
        let mut buf = [0u8; 1024];
        let mut completed = false;

        while !completed {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let timeout_ms = remaining.as_millis().min(200).max(1) as i32;
            let mut fds = [PollFd::new(&self.fd, PollFlags::IN)];
            match poll(&mut fds, timeout_ms) {
                Ok(0) => break,
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(_) => break,
            }
            match rustix::io::read(&self.fd, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let mut offset = 0usize;
                    while offset + 8 <= n {
                        let ty = u32::from_ne_bytes([
                            buf[offset],
                            buf[offset + 1],
                            buf[offset + 2],
                            buf[offset + 3],
                        ]);
                        let len = u32::from_ne_bytes([
                            buf[offset + 4],
                            buf[offset + 5],
                            buf[offset + 6],
                            buf[offset + 7],
                        ]) as usize;
                        if len < 8 || offset + len > n {
                            break;
                        }
                        if ty == DRM_EVENT_FLIP_COMPLETE && len >= 16 {
                            let idx = u64::from_ne_bytes([
                                buf[offset + 8],
                                buf[offset + 9],
                                buf[offset + 10],
                                buf[offset + 11],
                                buf[offset + 12],
                                buf[offset + 13],
                                buf[offset + 14],
                                buf[offset + 15],
                            ]) as usize;
                            if let Some(out) = self.outputs.get_mut(idx) {
                                out.flip_pending = false;
                                out.flip_stalls = 0;
                            }
                            completed = true;
                        }
                        offset += len;
                    }
                }
                Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
                Err(_) => break,
            }
        }
        if completed {
            return true;
        }
        // Timeout albo błąd odczytu: nie wiemy, które flipy się zakończyły.
        // Zgubione zdarzenie nie może zablokować kompozytora na zawsze, więc
        // zdejmujemy flagi — ale tylko tam, gdzie flip naprawdę wisiał, i każemy
        // następnym klatkom iść w całości (stan buforów jest nieznany).
        // Wcześniej robiliśmy to bezwarunkowo po każdym `wait_flips`, co kasowało
        // całą optymalizację damage i samo powodowało pełne kopie 8 MB na klatkę.
        for out in &mut self.outputs {
            if out.flip_pending {
                out.flip_stalls = out.flip_stalls.saturating_add(1);
                out.force_full_frames = 2;
                out.flip_pending = false;
            }
        }
        false
    }

    /// Ustawia obraz kursora sprzętowego na wyjściu `idx` w pozycji `(x, y)`.
    ///
    /// `false`, gdy sterownik nie obsługuje kursora — wywołujący wraca do kursora
    /// programowego (rysowanego w scenie).
    pub fn set_cursor_image(
        &mut self,
        idx: usize,
        rgba: &[u8],
        w: u32,
        h: u32,
        x: i32,
        y: i32,
    ) -> bool {
        let fd = self.fd.as_fd();
        let (cap_w, cap_h) = self.cursor_cap;
        // Płaszczyzna ma skończony rozmiar (NVIDIA: 256×256); większy obraz
        // musi narysować scena.
        if w == 0 || h == 0 || w > cap_w || h > cap_h {
            return false;
        }
        let Some(out) = self.outputs.get_mut(idx) else {
            return false;
        };
        // Rozmiar kursora może się zmienić (motyw 24 px → obraz klienta 64 px),
        // a BO ma stały pitch — wtedy odtwarzamy bufor.
        let reusable = out
            .cursor
            .as_ref()
            .is_some_and(|c| c.height == h && c.width >= w);
        if !reusable {
            if let Some(old) = out.cursor.take() {
                destroy_dumb(fd, old.handle);
            }
            out.cursor = create_cursor_bo(fd, w, h, cap_w);
        }
        let Some(cur) = out.cursor.as_mut() else {
            return false;
        };
        // Padding na prawo od obrazu musi być przezroczysty — bez zerowania
        // widać było śmieci z poprzedniego kursora (pitch > width * 4).
        unsafe { std::ptr::write_bytes(cur.ptr, 0, cur.size) };
        swizzle_argb_into(cur.ptr, cur.pitch, rgba, w as usize, h as usize);
        let req = ModeCursor {
            flags: DRM_MODE_CURSOR_BO,
            crtc_id: out.crtc_id,
            x,
            y,
            // Jądro liczy pitch framebufferu jako `width * 4`, więc zgłaszamy
            // szerokość **alokacji** (z paddingiem), nie sam obraz.
            width: cur.width,
            height: h,
            handle: cur.handle,
        };
        match ioctl_rw::<ModeCursor, DRM_IOCTL_MODE_CURSOR, _>(&self.fd, req) {
            Ok(_) => true,
            Err(e) => {
                eprintln!(
                    "selene/drm: kursor sprzętowy odrzucony na wyjściu {idx} ({}×{}, BO {}×{}, pitch {}: {:?})",
                    w, h, cur.width, h, cur.pitch, e
                );
                false
            }
        }
    }

    /// Przesuwa kursor sprzętowy na wyjściu `idx` (bez zmiany obrazu).
    pub fn move_cursor(&mut self, idx: usize, x: i32, y: i32) -> bool {
        let Some(crtc_id) = self.outputs.get(idx).map(|o| o.crtc_id) else {
            return false;
        };
        let req = ModeCursor {
            flags: DRM_MODE_CURSOR_MOVE,
            crtc_id,
            x,
            y,
            ..ModeCursor::default()
        };
        ioctl_rw::<ModeCursor, DRM_IOCTL_MODE_CURSOR, _>(&self.fd, req).is_ok()
    }

    /// Wyłącza kursor sprzętowy na wyjściu `idx`.
    pub fn hide_cursor(&mut self, idx: usize) -> bool {
        let Some(crtc_id) = self.outputs.get(idx).map(|o| o.crtc_id) else {
            return false;
        };
        // `handle = 0` + szerokość/wysokość 0 = wyłączenie płaszczyzny.
        let req = ModeCursor {
            flags: DRM_MODE_CURSOR_BO,
            crtc_id,
            ..ModeCursor::default()
        };
        match ioctl_rw::<ModeCursor, DRM_IOCTL_MODE_CURSOR, _>(&self.fd, req) {
            Ok(_) => true,
            Err(e) => {
                eprintln!("selene/drm: nie udało się ukryć kursora na wyjściu {idx}: {e:?}");
                false
            }
        }
    }
}

/// Konfiguruje jedno wyjście: tryb, bufor dumb, framebuffer i `SETCRTC`.
fn setup_output(
    fd: BorrowedFd,
    conn_id: u32,
    res: &Resources,
    used_crtcs: &mut Vec<u32>,
    preferences: &DisplaySettings,
    driver: Option<&str>,
) -> Option<DrmOutput> {
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(
        fd,
        ModeGetConnector {
            connector_id: conn_id,
            ..ModeGetConnector::default()
        },
    )
    .ok()?;
    if c.connection != DRM_MODE_CONNECTED {
        return None;
    }
    let count = c.count_modes as usize;
    if count == 0 {
        return None;
    }
    let mut modes = vec![ModeInfo::default(); count];
    let c = ioctl_rw::<ModeGetConnector, DRM_IOCTL_MODE_GETCONNECTOR, _>(
        fd,
        ModeGetConnector {
            connector_id: conn_id,
            count_modes: count as u32,
            modes_ptr: modes.as_mut_ptr() as u64,
            ..ModeGetConnector::default()
        },
    )
    .ok()?;
    let name = connector_name(c.connector_type, c.connector_type_id);
    let wanted = preferences.mode_for(&name);
    let mode = *pick_mode(&modes, wanted)?;
    let mut offered: Vec<OutputMode> = modes
        .iter()
        .filter(|mode| mode.hdisplay > 0 && mode.vdisplay > 0)
        .map(|mode| {
            OutputMode::new(
                mode.hdisplay,
                mode.vdisplay,
                mode.vrefresh.min(u16::MAX as u32) as u16,
            )
        })
        .collect();
    offered.sort_by(|a, b| {
        b.width
            .cmp(&a.width)
            .then_with(|| b.height.cmp(&a.height))
            .then_with(|| b.refresh.cmp(&a.refresh))
    });
    offered.dedup();

    // Enkoder → CRTC. Po przejęciu ekranu CRTC po poprzedniej sesji bywa jeszcze
    // zajęty (`set_crtc` kończy się `-EBUSY`) — dlatego próbujemy **kolejnych**
    // kandydatów. Wcześniej poddawaliśmy się po pierwszej nieudanej próbie i losowo
    // brakowało jednego monitora („działa tylko jeden ekran”).
    let enc = ioctl_rw::<ModeGetEncoder, DRM_IOCTL_MODE_GETENCODER, _>(
        fd,
        ModeGetEncoder {
            encoder_id: c.encoder_id,
            ..ModeGetEncoder::default()
        },
    )
    .ok()?;

    // Kolejność kandydatów: obecny CRTC enkodera → wg maski `possible_crtcs` → reszta karty.
    let mut candidates: Vec<u32> = Vec::new();
    if enc.crtc_id != 0 {
        candidates.push(enc.crtc_id);
    }
    for (idx, &crtc) in res.crtcs.iter().enumerate() {
        if (enc.possible_crtcs & (1 << idx)) != 0 {
            candidates.push(crtc);
        }
    }
    candidates.extend(res.crtcs.iter().copied());

    let width = mode.hdisplay as u32;
    let height = mode.vdisplay as u32;
    let make_pair = |fmt: u32| -> Option<(DumbBuffer, DumbBuffer)> {
        Some((
            create_dumb(fd, width, height, fmt)?,
            create_dumb(fd, width, height, fmt)?,
        ))
    };

    // Preferujemy `XBGR8888`: pamięć to R,G,B,X, czyli dokładnie wyjście tiny-skia
    // (`present` kopiuje wiersze bez przepisywania kanałów). Karty wirtualne
    // (QXL/SPICE, virtio, VKMS) obsługują tylko `XRGB8888` — tam od razu
    // wybieramy ten format i przepisujemy kanały. Gdy sterownik odrzuci
    // `XBGR8888` przy `SETCRTC`, próbujemy kolejnego formatu.
    let mut placed = None;
    for &format in scanout_formats(driver) {
        let Some((b0, b1)) = make_pair(format) else {
            continue;
        };
        let mut crtc_id = None;
        for &cand in &candidates {
            if used_crtcs.contains(&cand) {
                continue;
            }
            if set_crtc(fd, cand, b0.fb_id, conn_id, &mode) {
                crtc_id = Some(cand);
                break;
            }
            if std::env::var("SELENE_STATS").is_ok() {
                eprintln!("selene/drm: CRTC {cand} odrzucił tryb — próbuję następny");
            }
        }
        if let Some(crtc_id) = crtc_id {
            placed = Some((format, crtc_id, b0, b1));
            break;
        }
        // Ten format nie zadziałał — zwolnij bufor i framebuffer, spróbuj następnego.
        destroy_fb(fd, b0.fb_id);
        destroy_fb(fd, b1.fb_id);
        destroy_dumb(fd, b0.handle);
        destroy_dumb(fd, b1.handle);
    }
    let (format, crtc_id, b0, b1) = placed?;
    // CRTC oznaczamy jako użyty **dopiero po udanym** `set_crtc`.
    used_crtcs.push(crtc_id);
    Some(DrmOutput {
        name,
        connector_id: conn_id,
        crtc_id,
        gamma_size: gamma_size(fd, crtc_id),
        width,
        height,
        hz: mode.vrefresh,
        modes: offered,
        raw_modes: modes,
        buffers: [b0, b1],
        front: 0,
        swizzle: format == DRM_FORMAT_XRGB8888,
        cursor: None,
        dmabuf_fbs: std::collections::VecDeque::new(),
        front_dmabuf: false,
        flip_pending: false,
        flip_stalls: 0,
        // Pierwsze dwie klatki po starcie zawsze w całości (bufory są świeże,
        // ale historia damage jeszcze nie istnieje).
        force_full_frames: 2,
    })
}

/// Zwalnia uchwyty GEM zaimportowanych buforów (`dmabuf` → `PRIME_FD_TO_HANDLE`).
fn release_gem(fd: BorrowedFd, handles: &[u32]) {
    for handle in handles {
        let _ = ioctl_w::<GemClose, DRM_IOCTL_GEM_CLOSE, _>(
            fd,
            GemClose {
                handle: *handle,
                pad: 0,
            },
        );
    }
}

/// Usuwa framebuffer (po udanym modesecie na nowy obraz).
fn destroy_fb(fd: BorrowedFd, fb_id: u32) {
    let _ = ioctl_rw::<u32, DRM_IOCTL_MODE_RMFB, _>(fd, fb_id);
}

/// Tworzy bufor dumb, mapuje go i rejestruje jako framebuffer (scanout).
fn create_dumb(fd: BorrowedFd, width: u32, height: u32, format: u32) -> Option<DumbBuffer> {
    create_dumb_fmt(fd, width, height, format, true)
}

/// Tworzy bufor dumb w zadanym formacie; `register_fb` dodaje też framebuffer
/// (niepotrzebne dla kursora, gdzie liczy się tylko uchwyt GEM).
fn create_dumb_fmt(
    fd: BorrowedFd,
    width: u32,
    height: u32,
    format: u32,
    register_fb: bool,
) -> Option<DumbBuffer> {
    let dumb = ioctl_rw::<ModeCreateDumb, DRM_IOCTL_MODE_CREATE_DUMB, _>(
        fd,
        ModeCreateDumb {
            width,
            height,
            bpp: 32,
            ..ModeCreateDumb::default()
        },
    )
    .ok()?;
    let map = ioctl_rw::<ModeMapDumb, DRM_IOCTL_MODE_MAP_DUMB, _>(
        fd,
        ModeMapDumb {
            handle: dumb.handle,
            ..ModeMapDumb::default()
        },
    )
    .ok()?;
    let ptr = unsafe {
        mmap(
            core::ptr::null_mut(),
            dumb.size as usize,
            ProtFlags::READ | ProtFlags::WRITE,
            MapFlags::SHARED,
            fd,
            map.offset,
        )
    }
    .ok()? as *mut u8;
    let fb_id = if register_fb {
        ioctl_rw::<ModeFbCmd2, DRM_IOCTL_MODE_ADDFB2, _>(
            fd,
            ModeFbCmd2 {
                width,
                height,
                pixel_format: format,
                handles: [dumb.handle, 0, 0, 0],
                pitches: [dumb.pitch, 0, 0, 0],
                ..ModeFbCmd2::default()
            },
        )
        .ok()?
        .fb_id
    } else {
        0
    };
    Some(DumbBuffer {
        fb_id,
        handle: dumb.handle,
        pitch: dumb.pitch as usize,
        size: dumb.size as usize,
        width,
        height,
        ptr,
    })
}

/// Zwalnia uchwyt GEM (`destroy_dumb`) — inaczej każdy nowy rozmiar kursora
/// zostawiałby w jądrze martwy bufor.
fn destroy_dumb(fd: BorrowedFd, handle: u32) {
    let _ =
        ioctl_rw::<ModeDestroyDumb, DRM_IOCTL_MODE_DESTROY_DUMB, _>(fd, ModeDestroyDumb { handle });
}

/// Maksymalny rozmiar płaszczyzny kursora (`DRM_CAP_CURSOR_WIDTH/HEIGHT`).
///
/// Gdy sterownik nie zgłasza caps (starsze jądra), przyjmujemy 64×64 — taki
/// rozmiar akceptuje każdy znany sterownik.
fn cursor_cap_of(fd: BorrowedFd) -> (u32, u32) {
    let read = |capability: u64| -> Option<u32> {
        ioctl_rw::<GetCap, DRM_IOCTL_GET_CAP, _>(
            fd,
            GetCap {
                capability,
                value: 0,
            },
        )
        .ok()
        .map(|c| c.value as u32)
        .filter(|&v| v > 0)
    };
    (
        read(DRM_CAP_CURSOR_WIDTH).unwrap_or(64).clamp(1, 512),
        read(DRM_CAP_CURSOR_HEIGHT).unwrap_or(64).clamp(1, 512),
    )
}

/// Tworzy bufor kursora, którego `pitch == width * 4`.
///
/// Jądro tworzy framebuffer kursora z `pitch = width * 4` (`drm_mode_cursor`),
/// a `drm_mode_create_dumb` wyrównuje pitch do 256 B (NVIDIA): dla kursora
/// 24 px bufor ma 256 B, więc framebuffer dostaje inny pitch niż alokacja
/// i `MODE_CURSOR` kończy się `EINVAL` — przez to sprzętowy kursor był
/// wyłączany w całości. Bierzemy naturalny pitch sterownika i powiększamy
/// **szerokość** BO do `pitch / 4`; obraz zajmuje tylko lewy górny róg,
/// a resztę wypełniamy przezroczystym tłem.
/// Następna szerokość BO dla naturalnego pitcha sterownika.
///
/// `None` = brak zbieżności (pitch nie jest wielokrotnością 4 albo wynik
/// przekracza płaszczyznę kursora).
fn next_cursor_width(width: u32, pitch: usize, cap_w: u32) -> Option<u32> {
    let next = (pitch / 4) as u32;
    (next > width && next <= cap_w).then_some(next)
}

fn create_cursor_bo(fd: BorrowedFd, w: u32, h: u32, cap_w: u32) -> Option<DumbBuffer> {
    let mut width = w;
    for _ in 0..4 {
        let bo = create_dumb_fmt(fd, width, h, DRM_FORMAT_ARGB8888, false)?;
        if bo.pitch == width as usize * 4 {
            return Some(bo);
        }
        let handle = bo.handle;
        let next = next_cursor_width(width, bo.pitch, cap_w);
        drop(bo); // Drop zwalnia tylko mapowanie.
        destroy_dumb(fd, handle);
        width = next?;
    }
    None
}

/// Odczytuje rozmiar sprzętowej tablicy gamma CRTC (0 = brak wsparcia).
fn gamma_size(fd: BorrowedFd, crtc_id: u32) -> u32 {
    ioctl_rw::<ModeCrtc, DRM_IOCTL_MODE_GETCRTC, _>(
        fd,
        ModeCrtc {
            crtc_id,
            ..ModeCrtc::default()
        },
    )
    .map(|c| c.gamma_size)
    .unwrap_or(0)
}

/// Buduje neutralną lub ciepłą rampę gamma.
///
/// `strength` = 0 daje idealną przekątną (brak zmiany kolorów), a 1 najmocniej
/// tłumi kanał niebieski. Wartości mają pełny zakres 16 bitów na kanał.
fn gamma_ramp(size: u32, strength: f64) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
    let n = size as usize;
    let mut red = vec![0u16; n];
    let mut green = vec![0u16; n];
    let mut blue = vec![0u16; n];
    if n == 0 {
        return (red, green, blue);
    }
    let strength = strength.clamp(0.0, 1.0);
    let last = (n - 1).max(1) as f64;
    for i in 0..n {
        let level = i as f64 / last * 65535.0;
        red[i] = level.round() as u16;
        green[i] = (level * (1.0 - 0.10 * strength)).round() as u16;
        blue[i] = (level * (1.0 - 0.38 * strength)).round() as u16;
    }
    (red, green, blue)
}

/// Wgrywa tablicę gamma na CRTC (`SETGAMMA`).
fn set_gamma_lut(fd: BorrowedFd, crtc_id: u32, size: u32, strength: f64) -> bool {
    if size == 0 {
        return false;
    }
    let (red, green, blue) = gamma_ramp(size, strength);
    let lut = ModeCrtcLut {
        crtc_id,
        gamma_size: size,
        red: red.as_ptr() as u64,
        green: green.as_ptr() as u64,
        blue: blue.as_ptr() as u64,
    };
    ioctl_rw::<ModeCrtcLut, DRM_IOCTL_MODE_SETGAMMA, _>(fd, lut).is_ok()
}

/// Ustawia tryb na CRTC (`SETCRTC`).
fn set_crtc(fd: BorrowedFd, crtc_id: u32, fb_id: u32, connector_id: u32, mode: &ModeInfo) -> bool {
    let mut connectors = [connector_id];
    let req = ModeCrtc {
        set_connectors_ptr: connectors.as_mut_ptr() as u64,
        count_connectors: 1,
        crtc_id,
        fb_id,
        mode_valid: 1,
        mode: *mode,
        ..ModeCrtc::default()
    };
    ioctl_rw::<ModeCrtc, DRM_IOCTL_MODE_SETCRTC, _>(fd, req).is_ok()
}

/// Przepisuje RGBA do bufora kursora w formacie `ARGB8888` (kolejność B,G,R,A).
fn swizzle_argb_into(dst: *mut u8, pitch: usize, rgba: &[u8], width: usize, height: usize) {
    if dst.is_null() || rgba.len() < width * height * 4 {
        return;
    }
    unsafe {
        for y in 0..height {
            let src = &rgba[y * width * 4..(y + 1) * width * 4];
            let row = dst.add(y * pitch);
            for x in 0..width {
                let s = &src[x * 4..x * 4 + 4];
                let d = row.add(x * 4);
                *d = s[2];
                *d.add(1) = s[1];
                *d.add(2) = s[0];
                *d.add(3) = s[3];
            }
        }
    }
}

/// Prostokąt w pikselach fizycznych (współrzędne są przycinane do bufora).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Przepisuje **wycinek** klatki RGBA do bufora `XRGB8888` (bez zamiany kanałów).
fn copy_region_into(
    dst: *mut u8,
    pitch: usize,
    rgba: &[u8],
    width: usize,
    height: usize,
    r: PixelRect,
) -> bool {
    if dst.is_null() || rgba.len() < width * height * 4 {
        return false;
    }
    let x0 = (r.x as usize).min(width);
    let y0 = (r.y as usize).min(height);
    let x1 = (r.x as usize).saturating_add(r.w as usize).min(width);
    let y1 = (r.y as usize).saturating_add(r.h as usize).min(height);
    if x0 >= x1 || y0 >= y1 {
        return true;
    }
    let row_bytes = (x1 - x0) * 4;
    unsafe {
        for y in y0..y1 {
            let src = rgba.as_ptr().add((y * width + x0) * 4);
            std::ptr::copy_nonoverlapping(src, dst.add(y * pitch + x0 * 4), row_bytes);
        }
    }
    true
}

/// Jak [`copy_region_into`], ale z zamianą R/B (`XRGB8888`).
fn swizzle_region_into(
    dst: *mut u8,
    pitch: usize,
    rgba: &[u8],
    width: usize,
    height: usize,
    r: PixelRect,
) -> bool {
    if dst.is_null() || rgba.len() < width * height * 4 {
        return false;
    }
    let x0 = (r.x as usize).min(width);
    let y0 = (r.y as usize).min(height);
    let x1 = (r.x as usize).saturating_add(r.w as usize).min(width);
    let y1 = (r.y as usize).saturating_add(r.h as usize).min(height);
    if x0 >= x1 || y0 >= y1 {
        return true;
    }
    let count = x1 - x0;
    unsafe {
        for y in y0..y1 {
            let src_u32 = std::slice::from_raw_parts(
                (rgba.as_ptr() as *const u32).add(y * width + x0),
                count,
            );
            let row_dst =
                std::slice::from_raw_parts_mut(dst.add(y * pitch + x0 * 4) as *mut u32, count);
            for (d, &px) in row_dst.iter_mut().zip(src_u32.iter()) {
                // RGBA na little-endian: bajty [R, G, B, A] -> u32 0xAABBGGRR.
                // XRGB w pamięci: bajty [B, G, R, 0xFF] -> u32 0xFFRRGGBB.
                *d = (px & 0x0000_FF00)
                    | ((px & 0x0000_00FF) << 16)
                    | ((px & 0x00FF_0000) >> 16)
                    | 0xFF00_0000;
            }
        }
    }
    true
}

/// Przepisuje klatkę RGBA do bufora w formacie `XRGB8888` (kolejność B,G,R,X).
fn swizzle_into(dst: *mut u8, pitch: usize, rgba: &[u8], width: usize, height: usize) -> bool {
    if dst.is_null() || rgba.len() < width * height * 4 {
        return false;
    }
    unsafe {
        for y in 0..height {
            let src_u32 =
                std::slice::from_raw_parts((rgba.as_ptr() as *const u32).add(y * width), width);
            let row_dst = std::slice::from_raw_parts_mut(dst.add(y * pitch) as *mut u32, width);
            for (d, &px) in row_dst.iter_mut().zip(src_u32.iter()) {
                *d = (px & 0x0000_FF00)
                    | ((px & 0x0000_00FF) << 16)
                    | ((px & 0x00FF_0000) >> 16)
                    | 0xFF00_0000;
            }
        }
    }
    true
}

/// Kopiuje klatkę RGBA do bufora `XBGR8888` (ta sama kolejność bajtów w pamięci)
/// wiersz po wierszu. Gdy `pitch` zgadza się z szerokością, to jeden `memcpy`.
fn copy_rows_into(dst: *mut u8, pitch: usize, rgba: &[u8], width: usize, height: usize) -> bool {
    if dst.is_null() || rgba.len() < width * height * 4 {
        return false;
    }
    let row_bytes = width * 4;
    unsafe {
        if pitch == row_bytes {
            std::ptr::copy_nonoverlapping(rgba.as_ptr(), dst, row_bytes * height);
        } else {
            for y in 0..height {
                std::ptr::copy_nonoverlapping(
                    rgba.as_ptr().add(y * row_bytes),
                    dst.add(y * pitch),
                    row_bytes,
                );
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qxl_uses_xrgb_scanout() {
        // QXL/SPICE nie obsługuje XBGR8888 — bufory muszą być XRGB8888
        // (z przepisaniem kanałów przy prezentacji).
        assert!(is_virtual_driver(Some("qxl")));
        assert_eq!(scanout_formats(Some("qxl")), &[DRM_FORMAT_XRGB8888]);
        assert_eq!(scanout_formats(Some("virtio_gpu")), &[DRM_FORMAT_XRGB8888]);
    }

    #[test]
    fn real_gpu_prefers_xbgr_with_xrgb_fallback() {
        assert!(!is_virtual_driver(Some("i915")));
        assert_eq!(
            scanout_formats(Some("i915")),
            &[DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888]
        );
        // Brak informacji o sterowniku = zachowujemy dotychczasową kolejność.
        assert_eq!(
            scanout_formats(None),
            &[DRM_FORMAT_XBGR8888, DRM_FORMAT_XRGB8888]
        );
    }

    /// Damage w prezentacji DRM: kopiujemy **tylko** prostokąt, resztę bufora
    /// zostawiamy nietkniętą (to jest cała oszczędność przy animacji panelu).
    #[test]
    fn present_region_copies_only_damage() {
        let (w, h, pitch) = (4usize, 3usize, 4 * 4);
        let mut dst = vec![0xAAu8; pitch * h];
        let src = vec![0x11u8; w * h * 4];
        assert!(copy_region_into(
            dst.as_mut_ptr(),
            pitch,
            &src,
            w,
            h,
            PixelRect {
                x: 1,
                y: 1,
                w: 2,
                h: 1
            },
        ));
        for y in 0..h {
            for x in 0..w {
                let v = dst[y * pitch + x * 4];
                let inside = y == 1 && (x == 1 || x == 2);
                assert_eq!(v, if inside { 0x11 } else { 0xAA }, "piksel ({x},{y})");
            }
        }
    }

    /// `force_full_frames` musi wymusić całą klatkę po zero-copy/nieudanym flipie.
    #[test]
    fn effective_region_respects_force_full() {
        let r = PixelRect {
            x: 10,
            y: 20,
            w: 100,
            h: 50,
        };
        assert_eq!(effective_region(0, Some(r)), Some(r));
        assert_eq!(effective_region(0, None), None);
        assert_eq!(effective_region(1, Some(r)), None);
        assert_eq!(effective_region(2, Some(r)), None);
    }

    /// Nie wolno zwalniać fb `dmabuf`, dopóki flip się nie skończy (kernel może
    /// je jeszcze skanować). Gdy CRTC skanuje `dumb`, wszystkie są wolne.
    #[test]
    fn dmabuf_keep_waits_for_flip_complete() {
        assert_eq!(dmabuf_keep(true, true), None);
        assert_eq!(dmabuf_keep(true, false), None);
        assert_eq!(dmabuf_keep(false, true), Some(1));
        assert_eq!(dmabuf_keep(false, false), Some(0));
    }

    /// Prostokąt wystający poza bufor musi zostać przycięty, a nie wyjść poza pamieć.
    #[test]
    fn present_region_clamps_to_buffer() {
        let (w, h, pitch) = (2usize, 2usize, 2 * 4);
        let mut dst = vec![0u8; pitch * h];
        let src = vec![0x55u8; w * h * 4];
        assert!(copy_region_into(
            dst.as_mut_ptr(),
            pitch,
            &src,
            w,
            h,
            PixelRect {
                x: 1,
                y: 1,
                w: 100,
                h: 100
            },
        ));
        assert_eq!(dst[1 * pitch + 1 * 4], 0x55);
        assert_eq!(dst[0], 0x00);
    }

    /// Zero-copy: klatka z GPU musi dać się zaimportować (`PRIME`) i zarejestrować
    /// jako framebuffer (`AddFB2WithModifiers`). To dokładnie ta ścieżka, którą idzie
    /// `present_dmabuf` — test łapie brak wsparcia sterownika **bez** przejmowania ekranu.
    #[test]
    fn exported_dmabuf_becomes_a_framebuffer() {
        use libluna_render::{gpu::GpuRenderer, Renderer, Scene};

        let Ok(card) = rustix::fs::open(
            "/dev/dri/card1",
            OFlags::RDWR | OFlags::CLOEXEC,
            Mode::empty(),
        ) else {
            eprintln!("pominięto: brak /dev/dri/card1");
            return;
        };
        let fd = card.as_fd();

        // Sprawdzamy kilka rozmiarów: mały (test), potem realny 1080p.
        let mut any_ok = false;
        for (w, h) in [(64u32, 48u32), (256, 256), (1920, 1080)] {
            let Ok(mut gpu) = GpuRenderer::new(w, h) else {
                eprintln!("pominięto: brak EGL/GPU");
                return;
            };
            gpu.draw(
                &Scene::new(),
                1.0,
                libluna_core::Color::from_rgba8(255, 0, 0, 255),
            );
            let Some(frame) = gpu.export_dmabuf() else {
                eprintln!("pominięto: sterownik nie eksportuje dmabuf");
                return;
            };
            let mut handles: Vec<u32> = Vec::new();
            for (plane_fd, _, _) in &frame.planes {
                let request = PrimeHandle {
                    handle: 0,
                    flags: 0,
                    fd: plane_fd.as_raw_fd(),
                };
                match ioctl_rw::<PrimeHandle, DRM_IOCTL_PRIME_FD_TO_HANDLE, _>(fd, request) {
                    Ok(done) if done.handle != 0 => handles.push(done.handle),
                    _ => {
                        release_gem(fd, &handles);
                        eprintln!("pominięto: PRIME_FD_TO_HANDLE odrzucone (sterownik)");
                        return;
                    }
                }
            }
            let mut desc = ModeFbCmd2 {
                fb_id: 0,
                width: frame.width,
                height: frame.height,
                pixel_format: frame.fourcc,
                flags: 2, // DRM_MODE_FB_MODIFIERS
                ..ModeFbCmd2::default()
            };
            for (i, handle) in handles.iter().enumerate() {
                desc.handles[i] = *handle;
            }
            for (i, (_, offset, stride)) in frame.planes.iter().enumerate() {
                desc.pitches[i] = *stride;
                desc.offsets[i] = *offset;
                desc.modifier[i] = frame.modifier;
            }
            let with_modifier = ioctl_rw::<ModeFbCmd2, DRM_IOCTL_MODE_ADDFB2, _>(fd, desc);
            let added_ok = matches!(&with_modifier, Ok(added) if added.fb_id != 0);
            if let Ok(added) = with_modifier {
                if added.fb_id != 0 {
                    destroy_fb(fd, added.fb_id);
                }
                any_ok |= added_ok;
            }
            // Druga próba: ten sam bufor jako **liniowy** (`flags=0`, modifier 0).
            // EGL na NVIDIA potrafi zwrócić modyfikator block-linear, którego
            // `nvidia-drm` nie przyjmie, a pamięć i tak jest liniowa (pitch = w*4).
            desc.flags = 0;
            desc.modifier = [0; 4];
            let linear = ioctl_rw::<ModeFbCmd2, DRM_IOCTL_MODE_ADDFB2, _>(fd, desc);
            let linear_ok = matches!(&linear, Ok(added) if added.fb_id != 0);
            if let Ok(added) = linear {
                if added.fb_id != 0 {
                    destroy_fb(fd, added.fb_id);
                }
            }
            eprintln!(
                "selene/dmabuf: {w}x{h} fourcc=0x{:x} modifier=0x{:x} pitch={:?} → modyfikator: {}, liniowo: {}",
                frame.fourcc,
                frame.modifier,
                frame.planes.iter().map(|(_, _, s)| *s).collect::<Vec<_>>(),
                if added_ok {
                    "OK".to_owned()
                } else {
                    format!("{:?}", added_err(&with_modifier))
                },
                if linear_ok { "OK".to_owned() } else { "odrzucone".to_owned() }
            );
            // Dowód, dlaczego **nie** wolno awaryjnie deklarować liniowości:
            // import tego samego bufora z `modifier=0` daje zły obraz (pamięć jest
            // kafelkowana), więc `AddFB2` bez modyfikatora przyjąłby bufor, ale na
            // ekranie byłyby śmieci. Dlatego fallback idzie do readbacku, nie do „linear”.
            if w == 64 {
                if let Ok(mut check) = GpuRenderer::new(w, h) {
                    let image = std::sync::Arc::new(libluna_render::DmabufImage {
                        width: w,
                        height: h,
                        format: frame.fourcc,
                        modifier: 0,
                        planes: frame
                            .planes
                            .iter()
                            .map(|(plane_fd, offset, stride)| libluna_render::DmabufPlane {
                                fd: plane_fd.try_clone().expect("dup fd"),
                                offset: *offset,
                                stride: *stride,
                            })
                            .collect(),
                    });
                    let mut scene = Scene::new();
                    scene.image_dmabuf(
                        libluna_core::Rect::from_xywh(0.0, 0.0, w as f64, h as f64),
                        image,
                    );
                    check.draw(&scene, 1.0, libluna_core::Color::from_rgba8(0, 0, 255, 255));
                    eprintln!(
                        "  import z modifier=0 (kontrola) → R,G,B,A = {:?}",
                        &check.data()[..4]
                    );
                }
            }
            release_gem(fd, &handles);
        }
        // Zero-copy nie może **wywrócić** sesji: brak wsparcia = fallback do readbacku.
        // Test pilnuje tylko, żeby ścieżka eksportu w ogóle działała (PRIME weszło).
        let _ = any_ok;
    }

    /// Eksport musi zwracać klatkę, która **właśnie** została narysowana.
    ///
    /// Cele renderu są podwójne (ping-pong), a zamiana następowała po narysowaniu
    /// klatki — `export_dmabuf` czytał wtedy teksturę sprzed dwóch klatek, czyli
    /// na starcie pustą, a potem nieaktualną. Zero-copy prezentował więc
    /// nieaktualny obraz, a readback po odrzuceniu — czarne tło.
    #[test]
    fn export_returns_just_drawn_frame() {
        use libluna_core::{Color, Rect};
        use libluna_render::{gpu::GpuRenderer, Renderer, Scene};

        let (w, h) = (64u32, 48u32);
        let Ok(mut gpu) = GpuRenderer::new(w, h) else {
            eprintln!("pominięto: brak EGL/GPU");
            return;
        };
        let mut scene = Scene::new();
        scene.fill_rect(
            Rect::from_xywh(0.0, 0.0, f64::from(w), f64::from(h)),
            Color::from_rgba8(255, 0, 0, 255),
        );
        gpu.draw(&scene, 1.0, Color::from_rgba8(0, 0, 0, 255));
        let Some(frame) = gpu.export_dmabuf() else {
            eprintln!("pominięto: sterownik nie eksportuje dmabuf");
            return;
        };

        // Import z powrotem: klatka musi być czerwona, nie czarna i nie niebieska.
        let mut check = GpuRenderer::new(w, h).expect("drugi kontekst");
        let mut check_scene = Scene::new();
        check_scene.image_dmabuf(
            Rect::from_xywh(0.0, 0.0, f64::from(w), f64::from(h)),
            std::sync::Arc::new(libluna_render::DmabufImage {
                width: w,
                height: h,
                format: frame.fourcc,
                modifier: frame.modifier,
                planes: frame
                    .planes
                    .iter()
                    .map(|(plane_fd, offset, stride)| libluna_render::DmabufPlane {
                        fd: plane_fd.try_clone().expect("dup fd"),
                        offset: *offset,
                        stride: *stride,
                    })
                    .collect(),
            }),
        );
        check.draw(&check_scene, 1.0, Color::from_rgba8(0, 0, 255, 255));
        let px = &check.data()[..4];
        assert!(
            px[0] > 200 && px[1] < 40 && px[2] < 40,
            "eksport zwrócił nieaktualną klatkę: {px:?}"
        );
    }

    fn added_err(r: &rustix::io::Result<ModeFbCmd2>) -> Option<std::io::ErrorKind> {
        r.as_ref().err().map(|e| e.kind())
    }

    #[test]
    fn connector_names_follow_wayland_convention() {
        assert_eq!(connector_name(14, 1), "eDP-1");
        assert_eq!(connector_name(11, 2), "HDMI-A-2");
        assert_eq!(connector_name(12, 1), "HDMI-B-1");
        assert_eq!(connector_name(10, 1), "DP-1");
        assert_eq!(connector_name(999, 0), "Connector-0");
    }

    #[test]
    fn ioctl_struct_sizes_match_kernel_padding() {
        // Rozmiary z nagłówków DRM — sprawdzają poprawność `#[repr(C)]` na 64-bit.
        assert_eq!(core::mem::size_of::<ModeCardRes>(), 64);
        assert_eq!(core::mem::size_of::<ModeGetConnector>(), 80);
        assert_eq!(core::mem::size_of::<ModeInfo>(), 68);
        assert_eq!(core::mem::size_of::<ModeGetEncoder>(), 20);
        assert_eq!(core::mem::size_of::<ModeCrtc>(), 104);
        assert_eq!(core::mem::size_of::<ModeCreateDumb>(), 32);
        assert_eq!(core::mem::size_of::<ModeMapDumb>(), 16);
        assert_eq!(core::mem::size_of::<ModeFbCmd2>(), 104);
        assert_eq!(core::mem::size_of::<ModePageFlip>(), 24);
        assert_eq!(core::mem::size_of::<ModeCursor>(), 28);
        assert_eq!(core::mem::size_of::<GetCap>(), 16);
    }

    #[test]
    fn cursor_buffer_width_matches_kernel_pitch() {
        // `drm_mode_cursor` tworzy framebuffer kursora z `pitch = width * 4`.
        // NVIDIA wyrównuje pitch dumb-bufora do 256 B, więc dla kursora 24 px
        // (96 B) `MODE_CURSOR` kończy się `EINVAL` — dlatego BO musi mieć
        // szerokość 64 px (pitch 256). Wartości zmierzone na sterowniku nvidia.
        assert_eq!(next_cursor_width(24, 256, 256), Some(64));
        assert_eq!(next_cursor_width(32, 256, 256), Some(64));
        assert_eq!(next_cursor_width(48, 256, 256), Some(64));
        assert_eq!(next_cursor_width(65, 512, 256), Some(128));
        assert_eq!(next_cursor_width(129, 768, 256), Some(192));
        // 64 px to już zgodny pitch — dalej nie ma czego powiększać.
        assert_eq!(next_cursor_width(64, 256, 256), None);
        // Bez płaszczyzny większej niż obraz nie da się dopasować.
        assert_eq!(next_cursor_width(100, 1024, 128), None);
    }

    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_present_copy() {
        let (w, h) = (1920usize, 1080usize);
        let src = vec![7u8; w * h * 4];
        let mut dst = vec![0u8; w * h * 4];
        let n = 20;
        let t = std::time::Instant::now();
        for _ in 0..n {
            copy_rows_into(
                std::hint::black_box(dst.as_mut_ptr()),
                w * 4,
                std::hint::black_box(&src),
                w,
                h,
            );
        }
        let _ = std::hint::black_box(dst[12345]);
        println!(
            "copy_rows_into (XBGR8888): {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
        let t = std::time::Instant::now();
        for _ in 0..n {
            swizzle_into(
                std::hint::black_box(dst.as_mut_ptr()),
                w * 4,
                std::hint::black_box(&src),
                w,
                h,
            );
        }
        let _ = std::hint::black_box(dst[12345]);
        println!(
            "swizzle_into (XRGB8888):  {:.2} ms/klatke",
            t.elapsed().as_secs_f64() * 1000.0 / n as f64
        );
    }

    #[test]
    fn xrgb8888_fourcc_is_little_endian_bgrx() {
        // 'X','R','2','4' — kontroluje kolejność bajtów przy przepisywaniu klatki.
        assert_eq!(DRM_FORMAT_XRGB8888.to_le_bytes(), *b"XR24");
        // 'A','R','2','4' — format kursora (z kanałem alfa).
        assert_eq!(DRM_FORMAT_ARGB8888.to_le_bytes(), *b"AR24");
        // 'X','B','2','4' — w pamięci R,G,B,X, czyli wprost wyjście tiny-skia.
        assert_eq!(DRM_FORMAT_XBGR8888.to_le_bytes(), *b"XB24");
    }

    #[test]
    fn copy_rows_matches_rgba_layout() {
        // 2×2 RGBA → bufor XBGR8888: bajty bez zmian, tylko wiersze wg `pitch`.
        let src: Vec<u8> = (0..16).collect();
        let mut dst = vec![0u8; 16];
        copy_rows_into(dst.as_mut_ptr(), 8, &src, 2, 2);
        assert_eq!(dst, src);

        // Większy `pitch` (padding w wierszu) — kopiowane tylko 8 bajtów na wiersz.
        let mut dst = vec![0xAAu8; 24];
        copy_rows_into(dst.as_mut_ptr(), 12, &src, 2, 2);
        assert_eq!(&dst[0..8], &src[0..8]);
        assert_eq!(&dst[12..20], &src[8..16]);
        // Padding pozostaje nietknięty.
        assert!(dst[8..12].iter().all(|&b| b == 0xAA));
    }

    #[test]
    fn swizzle_writes_bgrx_and_leaves_padding_intact() {
        // RGBA (R,G,B,A) → XRGB8888 w pamięci: B,G,R,0xFF. Sprawdza przepisanie
        // kanałów na nieparzystej szerokości (3 px) i wycinek z paddingiem `pitch`.
        let (w, h) = (3usize, 2usize);
        let mut src = vec![0u8; w * h * 4];
        for (i, px) in src.chunks_exact_mut(4).enumerate() {
            px[0] = i as u8; // R
            px[1] = 0x10 + i as u8; // G
            px[2] = 0x80 + i as u8; // B
            px[3] = 0xAA; // A — w XRGB zastępowane przez 0xFF
        }

        let mut dst = vec![0u8; w * h * 4];
        assert!(swizzle_into(dst.as_mut_ptr(), w * 4, &src, w, h));
        for i in 0..(w * h) {
            assert_eq!(
                &dst[i * 4..i * 4 + 4],
                &[0x80 + i as u8, 0x10 + i as u8, i as u8, 0xFF],
                "piksel {i}"
            );
        }

        // Wycinek 1×1 (wiersz 1, kolumna 1) w buforze z paddingiem jednego piksela.
        let pitch = (w + 1) * 4;
        let mut dst = vec![0u8; pitch * h];
        assert!(swizzle_region_into(
            dst.as_mut_ptr(),
            pitch,
            &src,
            w,
            h,
            PixelRect {
                x: 1,
                y: 1,
                w: 1,
                h: 1
            }
        ));
        let i = w + 1; // indeks piksela w źródle
        let at = pitch + 4; // początek kolumny 1 w wierszu 1
        assert_eq!(
            &dst[at..at + 4],
            &[0x80 + i as u8, 0x10 + i as u8, i as u8, 0xFF]
        );
        // Sąsiednie piksele i padding pozostają nietknięte.
        assert!(dst[at + 4..].iter().all(|&b| b == 0));
        assert!(dst[..at].iter().all(|&b| b == 0));
    }

    fn mode(w: u16, h: u16, hz: u32, preferred: bool) -> ModeInfo {
        ModeInfo {
            hdisplay: w,
            vdisplay: h,
            vrefresh: hz,
            type_: if preferred {
                DRM_MODE_TYPE_PREFERRED
            } else {
                0
            },
            ..ModeInfo::default()
        }
    }

    #[test]
    fn picks_highest_resolution_then_refresh() {
        let modes = [
            mode(1280, 720, 60, false),
            mode(1920, 1080, 60, false),
            mode(1920, 1080, 144, false),
        ];
        let best = pick_best_mode(&modes).unwrap();
        assert_eq!(
            (best.hdisplay, best.vdisplay, best.vrefresh),
            (1920, 1080, 144)
        );
    }

    #[test]
    fn configured_mode_wins_over_the_best_one() {
        let modes = [
            mode(1280, 720, 60, false),
            mode(1920, 1080, 60, false),
            mode(1920, 1080, 144, false),
        ];
        let wanted = OutputMode::new(1280, 720, 60);
        let picked = pick_mode(&modes, Some(wanted)).unwrap();
        assert_eq!(
            (picked.hdisplay, picked.vdisplay, picked.vrefresh),
            (1280, 720, 60)
        );
    }

    #[test]
    fn missing_or_refresh_less_mode_falls_back_to_auto() {
        let modes = [
            mode(1280, 720, 60, false),
            mode(1920, 1080, 60, false),
            mode(1920, 1080, 144, false),
        ];
        // Zapisana rozdzielczość bez Hz — wygrywa najwyższe odświeżanie.
        let picked = pick_mode(&modes, Some(OutputMode::new(1920, 1080, 0))).unwrap();
        assert_eq!(picked.vrefresh, 144);

        // Monitor nie zgłasza zapisanego trybu — wracamy do automatycznego wyboru.
        let picked = pick_mode(&modes, Some(OutputMode::new(3840, 2160, 60))).unwrap();
        assert_eq!((picked.hdisplay, picked.vdisplay), (1920, 1080));
    }

    #[test]
    fn native_mode_wins_over_larger_scale() {
        // Monitor o natywnym 1600x900, który dodatkowo zgłasza 1920x1080 — bierzemy natywny.
        let modes = [
            mode(1920, 1080, 60, false),
            mode(1600, 900, 60, true),
            mode(1280, 720, 60, false),
        ];
        let best = pick_best_mode(&modes).unwrap();
        assert_eq!((best.hdisplay, best.vdisplay), (1600, 900));
    }

    #[test]
    fn fallback_ignores_missing_preferred_flag() {
        // Gdy EDID nie oznacza żadnego trybu jako natywny — bierzemy największy.
        let modes = [mode(1280, 720, 60, false), mode(1920, 1080, 60, false)];
        assert_eq!(pick_best_mode(&modes).unwrap().hdisplay, 1920);
    }

    #[test]
    fn preferred_mode_breaks_ties() {
        let modes = [mode(1920, 1080, 60, false), mode(1920, 1080, 60, true)];
        assert!(pick_best_mode(&modes).unwrap().type_ & DRM_MODE_TYPE_PREFERRED != 0);
    }

    #[test]
    fn drm_gamma_structs_match_kernel_abi() {
        assert_eq!(std::mem::size_of::<ModeCrtcLut>(), 32);
        assert_eq!(std::mem::size_of::<ModeCrtc>(), 104);
        assert_eq!(DRM_IOCTL_MODE_GETCRTC, 0xC068_64A1);
        assert_eq!(DRM_IOCTL_MODE_SETGAMMA, 0xC020_64A4);
    }

    #[test]
    fn neutral_gamma_is_identity() {
        let (r, g, b) = gamma_ramp(256, 0.0);
        assert_eq!(r, g);
        assert_eq!(g, b);
        assert_eq!(r[0], 0);
        assert_eq!(r[255], 65535);
    }

    #[test]
    fn warm_gamma_reduces_blue_most() {
        let (r, g, b) = gamma_ramp(256, 0.6);
        assert_eq!(r[0], 0);
        assert_eq!(r[255], 65535);
        assert!(b[128] < g[128], "niebieski powinien być najciemniejszy");
        assert!(
            g[128] <= r[128],
            "zielony nie powinien przebijać czerwonego"
        );
        // Biały punkt staje się cieplejszy — niebieski nie dobija do pełnej skali.
        assert!(b[255] < 65535);
    }
}
