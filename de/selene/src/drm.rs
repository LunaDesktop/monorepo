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
use std::os::fd::{AsFd, BorrowedFd};
use std::path::PathBuf;

use rustix::fs::{Mode, OFlags};

use crate::monitors::{layout_horizontal, OutputInfo};

// — Kody ioctl KMS (`_IOWR`) —
const DRM_IOCTL_MODE_GETRESOURCES: u32 = 0xC040_64A0;
const DRM_IOCTL_MODE_GETCONNECTOR: u32 = 0xC050_64A7;

const DRM_MODE_CONNECTED: u32 = 1;
/// `DRM_MODE_TYPE_PREFERRED` (bit 3) — natywny tryb monitora.
const DRM_MODE_TYPE_PREFERRED: u32 = 1 << 3;

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

/// Wykrywa monitory. `None` oznacza, że DRM/KMS nie jest dostępne (użyj wyjścia nested).
pub fn discover() -> Option<Vec<OutputInfo>> {
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("card"))
                .unwrap_or(false)
        })
        .collect();
    cards.sort();

    for card in &cards {
        let Ok(fd) = rustix::fs::open(card, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty()) else {
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

    let mode = modes
        .iter()
        .find(|m| m.type_ & DRM_MODE_TYPE_PREFERRED != 0)
        .or_else(|| modes.first())?;

    Some(OutputInfo {
        name: connector_name(c.connector_type, c.connector_type_id),
        width: mode.hdisplay as i32,
        height: mode.vdisplay as i32,
        scale: 1,
        x: 0,
        y: 0,
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
    }
}
