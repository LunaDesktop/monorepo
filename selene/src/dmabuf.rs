//! `zwp_linux_dmabuf_v1` — bufory GPU klientów bez kopiowania przez CPU.
//!
//! Ogłaszamy ten protokół **tylko**, gdy kompozytor rysuje przez GPU
//! (`LUNA_GPU=1`, ustawiane też przez `SELENE_GPU=1`) i EGL potrafi zaimportować
//! `dmabuf` (`libluna_render::gpu::dmabuf_formats`). Bez GPU klienci zostają
//! przy `wl_shm` — nie ma sensu obiecywać czegoś, czego nie da się wyświetlić.
//!
//! Zakres (`docs/gpu.md`):
//! - **wersja 4** protokołu: `format`/`modifier` dla klientów v1–v3 oraz
//!   `feedback` (`format_table`, `main_device`, tranche) dla v4 — bez niego Mesa
//!   nie wie, na którym urządzeniu DRM alokować bufory, i kończy na ostrzeżeniach
//!   `failed to get driver name for fd -1` z fallbackiem do softwaru,
//! - formaty RGBA (`ARGB/XRGB/ABGR/XBGR8888`) oraz wieloplanowe YUV 4:2:0
//!   (`NV12/NV21/YUV420/YV12`), z modyfikatorami z EGL (tiling/compression),
//! - `create_immed` (ścieżka większości klientów: GTK, wgpu/Zed, Firefox)
//!   oraz `create` przez `created` (starsze klienty),
//! - import wykonuje `GpuRenderer` leniwie, po `Arc<DmabufImage>`.
//!
//! Bufor jest zwalniany klientowi (`wl_buffer.release`) **przy następnym commicie**
//! tej powierzchni — GPU ma wtedy pewność, że poprzednia klatka została odczytana.

use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use libluna_render::gpu;
use libluna_render::{DmabufImage, DmabufPlane};
use wayland_protocols::wp::linux_dmabuf::zv1::server::{
    zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
    zwp_linux_dmabuf_feedback_v1::{self, ZwpLinuxDmabufFeedbackV1},
    zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
};
use wayland_server::{
    protocol::{wl_buffer, wl_shm},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::shm::BufferData;
use crate::state::State;

/// Formaty `DRM_FORMAT_*` i liczba planów, które obsługujemy.
///
/// Cztery pierwsze to zwykłe bufory RGBA (jeden plan). Reszta to YUV 4:2:0
/// używane przez odtwarzacze i kamery — EGL importuje je jako jedną teksturę
/// (NV12/NV21) albo trzy planowane obrazy (YUV420/YV12).
const SUPPORTED_FORMATS: [(u32, usize); 8] = [
    (0x3432_5241, 1), // DRM_FORMAT_ARGB8888
    (0x3432_5258, 1), // DRM_FORMAT_XRGB8888
    (0x3432_4241, 1), // DRM_FORMAT_ABGR8888
    (0x3432_4258, 1), // DRM_FORMAT_XBGR8888
    (0x3231_564e, 2), // DRM_FORMAT_NV12
    (0x3132_564e, 2), // DRM_FORMAT_NV21
    (0x3231_5559, 3), // DRM_FORMAT_YUV420 (YU12)
    (0x3231_5659, 3), // DRM_FORMAT_YVU420 (YV12)
];

/// Maksymalna liczba planów w protokole.
const MAX_PLANES: usize = 4;

/// `DRM_FORMAT_MOD_INVALID` — brak jawnego modyfikatora.
const MOD_INVALID: u64 = u64::MAX;

/// Czy global `zwp_linux_dmabuf_v1` został zarejestrowany (diagnostyka/testy).
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Liczba formatów i par format/modyfikator w ogłoszeniu.
pub fn counts() -> (usize, usize) {
    (
        supported().len(),
        supported().iter().map(|(_, m)| m.len()).sum(),
    )
}

/// Format + obsługiwane modyfikatory (raz na proces).
fn supported() -> &'static [(u32, Vec<u64>)] {
    static CACHE: OnceLock<Vec<(u32, Vec<u64>)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let all = gpu::dmabuf_formats();
        let mut out = Vec::new();
        for (format, _) in SUPPORTED_FORMATS {
            let Some((_, mods)) = all.iter().find(|(f, _)| *f == format) else {
                continue;
            };
            let mut mods = mods.clone();
            if mods.is_empty() {
                mods.push(MOD_INVALID);
            }
            out.push((format, mods));
        }
        out
    })
}

/// Czy jest sens ogłaszać `zwp_linux_dmabuf_v1`.
pub fn available() -> bool {
    !supported().is_empty()
}

/// Jeden plan dodany przez klienta.
struct Plane {
    fd: OwnedFd,
    offset: u32,
    stride: u32,
    modifier: u64,
}

#[derive(Default)]
struct ParamsInner {
    planes: Vec<Option<Plane>>,
    used: bool,
}

/// Dane `zwp_linux_buffer_params_v1`.
#[derive(Default)]
pub struct Params {
    inner: Mutex<ParamsInner>,
}

impl Params {
    fn is_used(&self) -> bool {
        self.inner.lock().map(|p| p.used).unwrap_or(true)
    }

    fn add(&self, plane_idx: u32, plane: Plane) {
        let Ok(mut p) = self.inner.lock() else {
            return;
        };
        let idx = plane_idx as usize;
        if p.used || idx >= MAX_PLANES {
            return;
        }
        if p.planes.len() <= idx {
            p.planes.resize_with(idx + 1, || None);
        }
        if p.planes[idx].is_some() {
            return;
        }
        p.planes[idx] = Some(plane);
    }

    /// Buduje obraz z parametrów (i oznacza obiekt jako użyty).
    fn build(&self, width: i32, height: i32, format: u32, flags: u32) -> Option<DmabufImage> {
        let mut p = self.inner.lock().ok()?;
        if p.used || width <= 0 || height <= 0 || width > 16384 || height > 16384 {
            return None;
        }
        // Na razie bez `y_invert`/`interlaced` — klient dostanie `failed`.
        if flags != 0 {
            return None;
        }
        let Some((_, mods)) = supported().iter().find(|(f, _)| *f == format) else {
            return None;
        };
        let plane_count = SUPPORTED_FORMATS
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, n)| *n)?;
        if p.planes.len() < plane_count || p.planes.iter().take(plane_count).any(|p| p.is_none()) {
            return None;
        }
        let mut planes = Vec::with_capacity(plane_count);
        let mut modifier = None;
        for index in 0..plane_count {
            let plane = p.planes[index].take()?;
            if plane.stride == 0
                || !(mods.contains(&plane.modifier) || plane.modifier == MOD_INVALID)
            {
                return None;
            }
            // Protokół wymaga jednego modyfikatora dla całego obrazu (plan 0).
            match modifier {
                None => modifier = Some(plane.modifier),
                Some(m) if m != plane.modifier => return None,
                _ => {}
            }
            planes.push(DmabufPlane {
                fd: plane.fd,
                offset: plane.offset,
                stride: plane.stride,
            });
        }
        p.used = true;
        Some(DmabufImage {
            width: width as u32,
            height: height as u32,
            format,
            modifier: modifier.unwrap_or(MOD_INVALID),
            planes,
        })
    }
}

impl GlobalDispatch<ZwpLinuxDmabufV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpLinuxDmabufV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let dmabuf = data_init.init(resource, ());
        // v1–v3: `format` + `modifier`. Od v4 oba są przestarzałe — klient bierze
        // komplet informacji z `zwp_linux_dmabuf_feedback_v1` (żądanie poniżej).
        // `modifier` istnieje od v3 — wysłanie go klientowi v1/v2 kończy klienta
        // twardym przerwaniem w libwayland.
        let version = dmabuf.version();
        if version < 4 {
            for (format, modifiers) in supported() {
                let _ = dmabuf.format(*format);
                if version >= 3 {
                    for m in modifiers {
                        let _ = dmabuf.modifier(*format, (m >> 32) as u32, *m as u32);
                    }
                }
            }
        }
        if std::env::var("SELENE_STATS").is_ok() {
            eprintln!(
                "selene/dmabuf: bind {} formatów, {} par format/modyfikator",
                supported().len(),
                supported().iter().map(|(_, m)| m.len()).sum::<usize>()
            );
        }
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwpLinuxDmabufV1,
        request: zwp_linux_dmabuf_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // v3: `create_params`; v4: `feedback` — bez niego Mesa nie zna urządzenia
        // DRM i kończy na ostrzeżeniach `failed to get driver name for fd -1`.
        match request {
            zwp_linux_dmabuf_v1::Request::CreateParams { params_id } => {
                data_init.init(params_id, Params::default());
            }
            zwp_linux_dmabuf_v1::Request::GetDefaultFeedback { id }
            | zwp_linux_dmabuf_v1::Request::GetSurfaceFeedback { id, .. } => {
                // Opis urządzenia jest ten sam dla całego kompozytora, więc
                // `get_surface_feedback` dostaje identyczny zestaw.
                let feedback = data_init.init(id, ());
                send_feedback(&feedback);
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwpLinuxDmabufFeedbackV1,
        _request: zwp_linux_dmabuf_feedback_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

/// Wysyła klientowi opis: tablicę format/modyfikator, urządzenie główne i tranche.
///
/// Kolejność jest częścią protokołu: `format_table` i `main_device` przed
/// tranche, a `done` na końcu całej sekwencji.
fn send_feedback(feedback: &ZwpLinuxDmabufFeedbackV1) {
    let pairs: Vec<(u32, u64)> = supported()
        .iter()
        .flat_map(|(format, modifiers)| modifiers.iter().map(move |m| (*format, *m)))
        .collect();
    let Some((fd, size)) = format_table_fd(&pairs) else {
        return;
    };
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::FormatTable {
        fd: fd.as_fd(),
        size,
    });
    // Bez urządzenia nie ma sensu wysyłać tranche — klient zostałby z pustym
    // zestawem i tak wróciłby do `wl_shm`.
    let Some(device) = render_device() else {
        return;
    };
    let device_bytes = device.to_ne_bytes().to_vec();
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::MainDevice {
        device: device_bytes.clone(),
    });
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::TrancheTargetDevice {
        device: device_bytes,
    });
    // Bez flagi `scanout`: kompozycja idzie przez GPU kompozytora, więc nie
    // obiecujemy klientowi, że bufor trafi bezpośrednio na plane.
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::TrancheFlags {
        flags: WEnum::Value(zwp_linux_dmabuf_feedback_v1::TrancheFlags::empty()),
    });
    let mut indices = Vec::with_capacity(pairs.len() * 2);
    for index in 0..pairs.len() {
        indices.extend_from_slice(&(index as u16).to_ne_bytes());
    }
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::TrancheFormats {
        indices,
    });
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::TrancheDone);
    let _ = feedback.send_event(zwp_linux_dmabuf_feedback_v1::Event::Done);
}

/// Zapisuje tablicę (4 B formatu + 8 B modyfikatora na pozycję) do `memfd`.
fn format_table_fd(pairs: &[(u32, u64)]) -> Option<(OwnedFd, u32)> {
    use std::io::Write;
    let fd =
        rustix::fs::memfd_create(c"selene-dmabuf-table", rustix::fs::MemfdFlags::CLOEXEC).ok()?;
    let mut file = std::fs::File::from(fd.try_clone().ok()?);
    for (format, modifier) in pairs {
        file.write_all(&format.to_ne_bytes()).ok()?;
        file.write_all(&modifier.to_ne_bytes()).ok()?;
    }
    file.flush().ok()?;
    Some((fd, (pairs.len() * 12) as u32))
}

/// `dev_t` węzła renderowania, na którym klienci mają alokować bufory.
///
/// Bierzemy `/dev/dri/renderD*` pasujący do sterownika GPU używanego przez Selene;
/// `LUNA_RENDER_NODE` pozwala wskazać węzeł jawnie (układy hybrydowe).
fn render_device() -> Option<u64> {
    render_node().and_then(|path| device_of(&path))
}

/// Ścieżka węzła renderowania, na którym klienci mają alokować bufory.
///
/// Używana przez `dmabuf` (do `main_device` w feedbacku) i przez jawną
/// synchronizację (`syncobj`) — obie ścieżki **muszą** patrzeć na to samo
/// urządzenie, inaczej import osi czasu się nie uda.
pub(crate) fn render_node() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("LUNA_RENDER_NODE") {
        return Some(std::path::PathBuf::from(path));
    }
    let mut fallback = None;
    for entry in std::fs::read_dir("/dev/dri").ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        if !name.starts_with("renderD") {
            continue;
        }
        if device_of(&entry.path()).is_none() {
            continue;
        }
        let driver = std::fs::read_link(format!("/sys/class/drm/{name}/device/driver"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        match driver {
            Some(driver) if driver_matches_gpu(&driver) => return Some(entry.path()),
            _ => {
                fallback.get_or_insert(entry.path());
            }
        }
    }
    fallback
}

/// `dev_t` pliku urządzenia (to, czego oczekuje `main_device` w protokole).
fn device_of(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|meta| meta.rdev())
}

/// Czy sterownik węzła DRM odpowiada GPU, przez które rysuje Selene.
fn driver_matches_gpu(driver: &str) -> bool {
    let Some(info) = gpu::info() else {
        return false;
    };
    let haystack = format!("{} {}", info.vendor, info.renderer).to_lowercase();
    if haystack.contains("nvidia") {
        driver == "nvidia"
    } else if haystack.contains("amd") || haystack.contains("radeon") {
        driver == "amdgpu"
    } else if haystack.contains("intel") {
        driver == "i915" || driver == "xe"
    } else {
        false
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, Params> for State {
    fn request(
        _state: &mut Self,
        client: &Client,
        resource: &ZwpLinuxBufferParamsV1,
        request: zwp_linux_buffer_params_v1::Request,
        data: &Params,
        handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwp_linux_buffer_params_v1::Request::Add {
                fd,
                plane_idx,
                offset,
                stride,
                modifier_hi,
                modifier_lo,
            } => {
                data.add(
                    plane_idx,
                    Plane {
                        fd,
                        offset,
                        stride,
                        modifier: (u64::from(modifier_hi) << 32) | u64::from(modifier_lo),
                    },
                );
            }
            zwp_linux_buffer_params_v1::Request::Create {
                width,
                height,
                format,
                flags,
            } => {
                let flags = match flags {
                    WEnum::Value(f) => f.bits(),
                    WEnum::Unknown(v) => v,
                };
                match data.build(width, height, format, flags) {
                    Some(image) => {
                        let bd = BufferData::dmabuf(
                            std::sync::Arc::new(image),
                            wl_shm::Format::Argb8888,
                        );
                        match client.create_resource::<wl_buffer::WlBuffer, BufferData, State>(
                            handle, 1, bd,
                        ) {
                            Ok(buffer) => {
                                let _ = resource.created(&buffer);
                            }
                            Err(_) => {
                                let _ = resource.failed();
                            }
                        }
                    }
                    None => {
                        let _ = resource.failed();
                    }
                }
            }
            zwp_linux_buffer_params_v1::Request::CreateImmed {
                buffer_id,
                width,
                height,
                format,
                flags,
            } => {
                let flags = match flags {
                    WEnum::Value(f) => f.bits(),
                    WEnum::Unknown(v) => v,
                };
                match data.build(width, height, format, flags) {
                    Some(image) => {
                        data_init.init(
                            buffer_id,
                            BufferData::dmabuf(
                                std::sync::Arc::new(image),
                                wl_shm::Format::Argb8888,
                            ),
                        );
                    }
                    None => {
                        // Protokół pozwala utworzyć „zepsuty” bufor i wysłać `failed`.
                        let _ = resource.failed();
                        data_init.init(buffer_id, BufferData::failed(width.max(1), height.max(1)));
                    }
                }
            }
            zwp_linux_buffer_params_v1::Request::Destroy => {}
            _ => {}
        }
        if std::env::var("SELENE_STATS").is_ok() && data.is_used() {
            // Diagnostyka: kolejne żądanie po `create` powinno być tylko `destroy`.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(_format: u32, stride: u32, modifier: u64) -> Params {
        let p = Params::default();
        let fd: OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
        p.add(
            0,
            Plane {
                fd,
                offset: 0,
                stride,
                modifier,
            },
        );
        p
    }

    fn format_planes(format: u32) -> usize {
        SUPPORTED_FORMATS
            .iter()
            .find(|(f, _)| *f == format)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }

    /// Pierwszy ogłoszony format jednoplanowy (testy parametrów nie budują YUV).
    fn single_plane_format() -> (u32, Vec<u64>) {
        supported()
            .iter()
            .find(|(f, _)| format_planes(*f) == 1)
            .cloned()
            .expect("brak formatu jednoplanowego")
    }

    #[test]
    fn advertises_at_least_one_of_our_formats_when_gpu_can_import() {
        if gpu::dmabuf_formats().is_empty() {
            eprintln!("pominięto: brak importu dmabuf");
            return;
        }
        assert!(
            !supported().is_empty(),
            "EGL importuje dmabuf, ale żaden z formatów ARGB/XRGB/ABGR/XBGR nie jest dostępny"
        );
    }

    #[test]
    fn accepts_advertised_format_once() {
        if !available() {
            eprintln!("pominięto: brak EGL");
            return;
        }
        let (format, mods) = single_plane_format();
        let p = params(format, 4 * 64, mods[0]);
        assert!(p.build(64, 64, format, 0).is_some());
        // Obiekt parametrów można użyć tylko raz.
        assert!(p.build(64, 64, format, 0).is_none());
    }

    #[test]
    fn rejects_invalid_arguments() {
        if !available() {
            return;
        }
        let (format, mods) = single_plane_format();
        // Zły format.
        assert!(params(0xdead_beef, 256, mods[0])
            .build(64, 64, 0xdead_beef, 0)
            .is_none());
        // Zły stride.
        assert!(params(format, 0, mods[0])
            .build(64, 64, format, 0)
            .is_none());
        // Niepoprawne wymiary.
        assert!(params(format, 256, mods[0])
            .build(0, 64, format, 0)
            .is_none());
        // Flagi (y_invert/interlaced) nie są obsługiwane w etapie 2.
        assert!(params(format, 256, mods[0])
            .build(64, 64, format, 1)
            .is_none());
        // Modyfikator spoza listy.
        if !mods.contains(&0x1234_5678_9abc_def0) {
            assert!(params(format, 256, 0x1234_5678_9abc_def0)
                .build(64, 64, format, 0)
                .is_none());
        }
    }

    #[test]
    fn rejects_duplicate_plane() {
        if !available() {
            return;
        }
        let (format, mods) = single_plane_format();
        let p = Params::default();
        let fd: OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
        p.add(
            0,
            Plane {
                fd,
                offset: 0,
                stride: 256,
                modifier: mods[0],
            },
        );
        let fd2: OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
        p.add(
            0,
            Plane {
                fd: fd2,
                offset: 0,
                stride: 256,
                modifier: mods[0],
            },
        );
        // Druga próba na tym samym indeksie jest ignorowana, więc nadal jest plan 0.
        assert!(p.build(64, 64, format, 0).is_some());
    }

    #[test]
    fn multipane_format_requires_all_planes() {
        if !available() {
            return;
        }
        let Some((format, mods)) = supported()
            .iter()
            .find(|(f, _)| format_planes(*f) >= 2)
            .cloned()
        else {
            eprintln!("pominięto: EGL nie ogłasza formatów wieloplanowych");
            return;
        };
        // Sam plan Y to za mało dla NV12/YUV420 — EGL dostałby niekompletny obraz.
        assert!(params(format, 256, mods[0])
            .build(64, 64, format, 0)
            .is_none());

        let planes = format_planes(format);
        let p = Params::default();
        for index in 0..planes {
            let fd: OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
            p.add(
                index as u32,
                Plane {
                    fd,
                    offset: 0,
                    stride: 256,
                    modifier: mods[0],
                },
            );
        }
        let image = p.build(64, 64, format, 0).expect("komplet planów");
        assert_eq!(image.planes.len(), planes);
    }
}
