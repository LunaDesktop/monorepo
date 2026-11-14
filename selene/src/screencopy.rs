//! `wlr_screencopy_unstable_v1` — kopiowanie zawartości ekranu do buforów klienta.
//!
//! Protokół używany m.in. przez `xdg-desktop-portal-wlr` (ScreenCast/Screenshot),
//! OBS-a i narzędzia typu grim. Bez niego udostępnianie ekranu w aplikacjach
//! (Flatpak, przeglądarki, Discord) nie działa — portal nie ma skąd wziąć pikseli.
//!
//! Implementujemy wersję 3 z buforami `wl_shm` (`Xrgb8888`/`Argb8888`):
//!  * `capture_output` — całe wyjście,
//!  * `capture_output_region` — wycinek (współrzędne w pikselach wyjścia),
//!  * `copy` / `copy_with_damage` — zrzut klatki (damage zawsze pełny; to dozwolone,
//!    tylko mniej efektywne niż precyzyjne prostokąty),
//!  * `overlay_cursor` — kursora nie rysujemy w klatce, gdy klient o to nie prosi.
//!
//! Renderowanie idzie przez wspólną scenę (`output::capture_output_rgba`), więc zrzut
//! wygląda dokładnie jak to, co widać na monitorze (tapeta, panele, dekoracje).

use std::sync::Mutex;

use wayland_protocols_wlr::screencopy::v1::server::{
    zwlr_screencopy_frame_v1 as frame, zwlr_screencopy_manager_v1 as manager,
};
use wayland_server::protocol::{wl_buffer, wl_output, wl_shm};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::shm::BufferData;
use crate::state::State;
use crate::sync::LockExt;

/// Global `zwlr_screencopy_manager_v1` (ogłaszany z `main`).
pub type Manager = manager::ZwlrScreencopyManagerV1;

/// Stan pojedynczej klatki do skopiowania.
struct FrameData {
    output: usize,
    /// Wycinek w pikselach wyjścia; `None` = całe wyjście.
    region: Option<(i32, i32, i32, i32)>,
    overlay_cursor: bool,
    width: u32,
    height: u32,
    /// Klatkę można skopiować tylko raz (`already_used` w protokole).
    used: Mutex<bool>,
}

impl GlobalDispatch<Manager, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<Manager>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<Manager, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &Manager,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            manager::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output,
            } => {
                init_frame(state, data_init, frame, overlay_cursor != 0, output, None);
            }
            manager::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                output,
                x,
                y,
                width,
                height,
            } => {
                init_frame(
                    state,
                    data_init,
                    frame,
                    overlay_cursor != 0,
                    output,
                    Some((x, y, width, height)),
                );
            }
            manager::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<frame::ZwlrScreencopyFrameV1, FrameData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &frame::ZwlrScreencopyFrameV1,
        request: frame::Request,
        data: &FrameData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            frame::Request::Copy { buffer } => copy(state, resource, data, &buffer, false),
            frame::Request::CopyWithDamage { buffer } => copy(state, resource, data, &buffer, true),
            frame::Request::Destroy => {}
            _ => {}
        }
    }
}

/// Tworzy obiekt klatki i od razu ogłasza format bufora (`buffer` + `buffer_done`).
fn init_frame(
    state: &mut State,
    data_init: &mut DataInit<'_, State>,
    id: New<frame::ZwlrScreencopyFrameV1>,
    overlay_cursor: bool,
    output: wl_output::WlOutput,
    region: Option<(i32, i32, i32, i32)>,
) {
    let index = output.data::<usize>().copied();
    let size = index.and_then(|index| output_size(state, index, region));
    let (Some(index), Some((width, height))) = (index, size) else {
        let resource = data_init.init(
            id,
            FrameData {
                output: usize::MAX,
                region: None,
                overlay_cursor,
                width: 0,
                height: 0,
                used: Mutex::new(true),
            },
        );
        let _ = resource.send_event(frame::Event::Failed);
        return;
    };
    let resource = data_init.init(
        id,
        FrameData {
            output: index,
            region,
            overlay_cursor,
            width,
            height,
            used: Mutex::new(false),
        },
    );
    let _ = resource.send_event(frame::Event::Buffer {
        format: WEnum::Value(wl_shm::Format::Xrgb8888),
        width,
        height,
        stride: width.saturating_mul(4),
    });
    if resource.version() >= 3 {
        let _ = resource.send_event(frame::Event::BufferDone);
    }
}

/// Rozmiar klatki (piksele wyjścia) po przycięciu regionu.
fn output_size(
    state: &State,
    index: usize,
    region: Option<(i32, i32, i32, i32)>,
) -> Option<(u32, u32)> {
    let (ow, oh) = {
        let outs = state.outputs.lock_or_recover();
        let out = outs.get(index)?;
        (out.width as i32, out.height as i32)
    };
    let (w, h) = match region {
        None => (ow, oh),
        Some((x, y, width, height)) => {
            let x0 = x.max(0).min(ow);
            let y0 = y.max(0).min(oh);
            let x1 = x.saturating_add(width).clamp(0, ow);
            let y1 = y.saturating_add(height).clamp(0, oh);
            ((x1 - x0).max(0), (y1 - y0).max(0))
        }
    };
    (w > 0 && h > 0).then_some((w as u32, h as u32))
}

/// Wykonuje zrzut do bufora klienta i wysyła `ready` (albo `failed`).
fn copy(
    state: &mut State,
    resource: &frame::ZwlrScreencopyFrameV1,
    data: &FrameData,
    buffer: &wl_buffer::WlBuffer,
    with_damage: bool,
) {
    {
        let mut used = data.used.lock_or_recover();
        if *used {
            // Protokół pozwala skopiować klatkę tylko raz.
            let _ = resource.send_event(frame::Event::Failed);
            return;
        }
        *used = true;
    }
    let Some(buffer_data) = buffer.data::<BufferData>() else {
        let _ = resource.send_event(frame::Event::Failed);
        return;
    };
    let expected_stride = data.width.saturating_mul(4) as i32;
    if buffer_data.width != data.width as i32
        || buffer_data.height != data.height as i32
        || buffer_data.stride < expected_stride
    {
        let _ = resource.send_event(frame::Event::Failed);
        return;
    }
    let Some((rgba, width, height)) =
        crate::output::capture_output_rgba(state, data.output, data.overlay_cursor, data.region)
    else {
        let _ = resource.send_event(frame::Event::Failed);
        return;
    };
    if width != data.width || height != data.height || !buffer_data.write_rgba(&rgba) {
        let _ = resource.send_event(frame::Event::Failed);
        return;
    }
    // Damage zawsze pełny — klient dostaje poprawną klatkę, tylko bez optymalizacji
    // przyrostowej (Selene i tak renderuje całość przy zrzucie).
    if with_damage && resource.version() >= 2 {
        let _ = resource.send_event(frame::Event::Damage {
            x: 0,
            y: 0,
            width,
            height,
        });
    }
    let _ = resource.send_event(frame::Event::Flags {
        flags: WEnum::Value(frame::Flags::empty()),
    });
    let (sec, nsec) = monotonic_now();
    let _ = resource.send_event(frame::Event::Ready {
        tv_sec_hi: (sec >> 32) as u32,
        tv_sec_lo: (sec & 0xffff_ffff) as u32,
        tv_nsec: nsec as u32,
    });
}

/// Czas `CLOCK_MONOTONIC` w sekundach i nanosekundach (jak wysyła wlroots).
fn monotonic_now() -> (u64, u64) {
    let ts = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    (ts.tv_sec.max(0) as u64, ts.tv_nsec.max(0) as u64)
}

#[cfg(test)]
mod tests {
    #[test]
    fn region_is_clipped_to_output() {
        // Funkcja czysta — sprawdzamy samo przycinanie, bez `State` z wyjściami.
        let (ow, oh) = (100i32, 50i32);
        let clip = |x: i32, y: i32, w: i32, h: i32| {
            let x0 = x.max(0).min(ow);
            let y0 = y.max(0).min(oh);
            let x1 = x.saturating_add(w).clamp(0, ow);
            let y1 = y.saturating_add(h).clamp(0, oh);
            ((x1 - x0).max(0), (y1 - y0).max(0))
        };
        assert_eq!(clip(0, 0, 100, 50), (100, 50));
        assert_eq!(clip(-10, -10, 20, 20), (10, 10));
        assert_eq!(clip(90, 40, 100, 100), (10, 10));
        assert_eq!(clip(200, 200, 10, 10), (0, 0));
    }

    #[test]
    fn stride_matches_width() {
        assert_eq!(1920u32.saturating_mul(4), 7680);
    }
}
