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
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::path::PathBuf;

use rustix::fs::{Mode, OFlags};
use rustix::mm::{mmap, munmap, MapFlags, ProtFlags};

use crate::monitors::{layout_horizontal, OutputInfo};

// — Kody ioctl KMS (`_IOWR`) —
const DRM_IOCTL_MODE_GETRESOURCES: u32 = 0xC040_64A0;
const DRM_IOCTL_MODE_GETCONNECTOR: u32 = 0xC050_64A7;
const DRM_IOCTL_MODE_GETENCODER: u32 = 0xC014_64A6;
const DRM_IOCTL_MODE_SETCRTC: u32 = 0xC068_64A2;
const DRM_IOCTL_MODE_CREATE_DUMB: u32 = 0xC020_64B2;
const DRM_IOCTL_MODE_MAP_DUMB: u32 = 0xC010_64B3;
const DRM_IOCTL_MODE_ADDFB2: u32 = 0xC068_64B8;
const DRM_IOCTL_MODE_PAGE_FLIP: u32 = 0xC018_64B0;
/// `drm_mode_cursor` (`sizeof` = 28 = 0x1C).
const DRM_IOCTL_MODE_CURSOR: u32 = 0xC01C_64A3;

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

/// Diagnostyka: sprawdza, czy można przejąć mastera DRM **bez zmiany trybu**.
/// Zwraca ścieżkę karty, jeśli master jest wolny (i od razu go zwalnia).
pub fn check_master() -> Option<PathBuf> {
    for card in card_paths() {
        let Ok(fd) = rustix::fs::open(&card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) else {
            continue;
        };
        if ioctl_none::<DRM_IOCTL_SET_MASTER, _>(&fd).is_ok() {
            let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
            return Some(card);
        }
    }
    None
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

/// Wykrywa monitory. `None` oznacza, że DRM/KMS nie jest dostępne (użyj wyjścia nested).
pub fn discover() -> Option<Vec<OutputInfo>> {
    for card in card_paths() {
        let Ok(fd) = rustix::fs::open(&card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) else {
            continue;
        };
        if let Some(mut outs) = probe_card(fd.as_fd()) {
            if !outs.is_empty() {
                layout_horizontal(&mut outs);
                return Some(outs);
            }
        }
    }
    None
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

    Some(OutputInfo {
        name: connector_name(c.connector_type, c.connector_type_id),
        width: mode.hdisplay as i32,
        height: mode.vdisplay as i32,
        scale: 1,
        x: 0,
        y: 0,
    })
}

/// Wybiera najlepszy tryb monitora: **największa rozdzielczość**, potem **najwyższy
/// `vrefresh`**; przy remisie preferuje tryb natywny (`PREFERRED`).
fn pick_best_mode(modes: &[ModeInfo]) -> Option<&ModeInfo> {
    modes.iter().max_by_key(|m| {
        let area = m.hdisplay as u64 * m.vdisplay as u64;
        let hz = m.vrefresh as u64;
        let preferred = u64::from(m.type_ & DRM_MODE_TYPE_PREFERRED != 0);
        (area, hz, preferred)
    })
}

/// Nazwa konektora w stylu Wayland (np. `HDMI-A-1`, `eDP-1`).
fn connector_name(kind: u32, index: u32) -> String {
    let base = match kind {
        1 => "VGA",
        2 => "DVI-I",
        3 => "DVI-D",
        4 => "DVI-A",
        5 => "Composite",
        6 => "LVDS",
        7 => "Component",
        8 => "DIN",
        9 => "DP",
        10 => "HDMI-A",
        11 => "HDMI-B",
        12 => "TV",
        13 => "eDP",
        14 => "Virtual",
        15 => "DSI",
        16 => "DPI",
        17 => "Writeback",
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
    crtc_id: u32,
    width: u32,
    height: u32,
    hz: u32,
    buffers: [DumbBuffer; 2],
    front: usize,
    /// Bufor obrazu kursora sprzętowego (tworzony leniwie).
    cursor: Option<DumbBuffer>,
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
}

impl Drop for DrmBackend {
    fn drop(&mut self) {
        // Zwolnij bycie masterem, gdy kończymy.
        let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&self.fd);
    }
}

impl DrmBackend {
    /// Otwiera kartę, przejmuje mastera i ustawia tryb na wszystkich podłączonych monitorach.
    /// Zwraca `None`, gdy DRM jest niedostępne lub ktoś inny trzyma mastera (np. sesja użytkownika).
    pub fn open() -> Result<Self, DrmError> {
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
            let mut outputs = Vec::new();
            for &conn in &res.connectors {
                if let Some(out) = setup_output(fd.as_fd(), conn, &res) {
                    outputs.push(out);
                }
            }
            if outputs.is_empty() {
                let _ = ioctl_none::<DRM_IOCTL_DROP_MASTER, _>(&fd);
                last = DrmError::NoOutputs;
                continue;
            }
            return Ok(Self { fd, outputs });
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

    /// Wypycha klatkę (RGBA, wymiary = rozmiar wyjścia) i kolejkuje page-flip.
    /// Po wywołaniu należy poczekać na [`DrmBackend::wait_flip`].
    pub fn present(&mut self, idx: usize, rgba: &[u8]) -> bool {
        let fd = &self.fd;
        let Some(out) = self.outputs.get_mut(idx) else {
            return false;
        };
        let back = (out.front + 1) & 1;
        swizzle_into(
            out.buffers[back].ptr,
            out.buffers[back].pitch,
            rgba,
            out.width as usize,
            out.height as usize,
        );
        let req = ModePageFlip {
            crtc_id: out.crtc_id,
            fb_id: out.buffers[back].fb_id,
            flags: DRM_MODE_PAGE_FLIP_EVENT,
            reserved: 0,
            user_data: idx as u64,
        };
        if ioctl_rw::<ModePageFlip, DRM_IOCTL_MODE_PAGE_FLIP, _>(fd, req).is_err() {
            return false;
        }
        out.front = back;
        true
    }

    /// Czeka na zakończenie page-flipu (zdarzenie z jądra).
    pub fn wait_flip(&self) {
        let mut buf = [0u8; 32];
        loop {
            match rustix::io::read(&self.fd, &mut buf) {
                Ok(n) if n >= 8 => {
                    let ty = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    if ty == DRM_EVENT_FLIP_COMPLETE {
                        return;
                    }
                }
                _ => return,
            }
        }
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
        let Some(out) = self.outputs.get_mut(idx) else {
            return false;
        };
        if out.cursor.is_none() {
            match create_dumb_fmt(fd, w, h, DRM_FORMAT_ARGB8888, false) {
                Some(b) => out.cursor = Some(b),
                None => return false,
            }
        }
        let Some(cur) = out.cursor.as_mut() else {
            return false;
        };
        swizzle_argb_into(cur.ptr, cur.pitch, rgba, w as usize, h as usize);
        let req = ModeCursor {
            flags: DRM_MODE_CURSOR_BO,
            crtc_id: out.crtc_id,
            x,
            y,
            width: w,
            height: h,
            handle: cur.handle,
        };
        ioctl_rw::<ModeCursor, DRM_IOCTL_MODE_CURSOR, _>(&self.fd, req).is_ok()
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
        let req = ModeCursor {
            flags: DRM_MODE_CURSOR_BO,
            crtc_id,
            ..ModeCursor::default()
        };
        ioctl_rw::<ModeCursor, DRM_IOCTL_MODE_CURSOR, _>(&self.fd, req).is_ok()
    }
}

/// Konfiguruje jedno wyjście: tryb, bufor dumb, framebuffer i `SETCRTC`.
fn setup_output(fd: BorrowedFd, conn_id: u32, res: &Resources) -> Option<DrmOutput> {
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
    let mode = *pick_best_mode(&modes)?;

    // Enkoder → CRTC (lub pierwszy możliwy).
    let enc = ioctl_rw::<ModeGetEncoder, DRM_IOCTL_MODE_GETENCODER, _>(
        fd,
        ModeGetEncoder {
            encoder_id: c.encoder_id,
            ..ModeGetEncoder::default()
        },
    )
    .ok()?;
    let crtc_id = if enc.crtc_id != 0 {
        enc.crtc_id
    } else {
        let idx = enc.possible_crtcs.trailing_zeros() as usize;
        *res.crtcs.get(idx).or_else(|| res.crtcs.first())?
    };

    let width = mode.hdisplay as u32;
    let height = mode.vdisplay as u32;
    let b0 = create_dumb(fd, width, height)?;
    let b1 = create_dumb(fd, width, height)?;
    if !set_crtc(fd, crtc_id, b0.fb_id, conn_id, &mode) {
        return None;
    }
    Some(DrmOutput {
        name: connector_name(c.connector_type, c.connector_type_id),
        crtc_id,
        width,
        height,
        hz: mode.vrefresh,
        buffers: [b0, b1],
        front: 0,
        cursor: None,
    })
}

/// Tworzy bufor dumb, mapuje go i rejestruje jako framebuffer (scanout).
fn create_dumb(fd: BorrowedFd, width: u32, height: u32) -> Option<DumbBuffer> {
    create_dumb_fmt(fd, width, height, DRM_FORMAT_XRGB8888, true)
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
        ptr,
    })
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

/// Przepisuje klatkę RGBA do bufora w formacie `XRGB8888` (kolejność B,G,R,X).
fn swizzle_into(dst: *mut u8, pitch: usize, rgba: &[u8], width: usize, height: usize) {
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
                *d.add(3) = 0xff;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_names_follow_wayland_convention() {
        assert_eq!(connector_name(13, 1), "eDP-1");
        assert_eq!(connector_name(10, 2), "HDMI-A-2");
        assert_eq!(connector_name(9, 1), "DP-1");
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
    }

    #[test]
    fn xrgb8888_fourcc_is_little_endian_bgrx() {
        // 'X','R','2','4' — kontroluje kolejność bajtów przy przepisywaniu klatki.
        assert_eq!(DRM_FORMAT_XRGB8888.to_le_bytes(), *b"XR24");
        // 'A','R','2','4' — format kursora (z kanałem alfa).
        assert_eq!(DRM_FORMAT_ARGB8888.to_le_bytes(), *b"AR24");
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
    fn preferred_mode_breaks_ties() {
        let modes = [mode(1920, 1080, 60, false), mode(1920, 1080, 60, true)];
        assert!(pick_best_mode(&modes).unwrap().type_ & DRM_MODE_TYPE_PREFERRED != 0);
    }
}
