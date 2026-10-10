//! `wl_output` — monitory. Model gotowy na wiele monitorów; wykrywanie w [`crate::drm`].
//!
//! Każdy monitor ma własną pozycję w globalnej przestrzeni współrzędnych, dzięki czemu
//! okna i powierzchnie warstwowe można rozkładać i kotwiczyć na konkretnym wyświetlaczu.

use std::sync::{Arc, Mutex};

use libluna_core::Rect;
use wayland_server::protocol::wl_output::{self, WlOutput};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;

/// Informacje o monitorze (współdzielone z wątkiem wyjścia, który zna bieżący rozmiar).
#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
    /// Pozycja logiczna (lewy-górny narożnik) w globalnej przestrzeni współrzędnych.
    pub x: i32,
    pub y: i32,
}

impl Default for OutputInfo {
    fn default() -> Self {
        Self {
            name: "nested-0".to_owned(),
            width: 1280,
            height: 800,
            scale: 1,
            x: 0,
            y: 0,
        }
    }
}

impl OutputInfo {
    /// Prostokąt monitora w globalnej przestrzeni współrzędnych.
    pub fn rect(&self) -> Rect {
        Rect::from_xywh(
            self.x as f64,
            self.y as f64,
            self.width as f64,
            self.height as f64,
        )
    }
}

/// Współdzielona lista monitorów.
pub type Outputs = Arc<Mutex<Vec<OutputInfo>>>;

/// Układa monitory poziomo, od lewej do prawej (przydatne dla trybu nested i układu lustrzanego).
pub fn layout_horizontal(outs: &mut [OutputInfo]) {
    let mut x = 0;
    for o in outs.iter_mut() {
        o.x = x;
        o.y = 0;
        x += o.width;
    }
}

/// Indeks monitora zawierającego punkt (lub pierwszy, gdy żaden nie pasuje).
pub fn output_at(outs: &[OutputInfo], x: f64, y: f64) -> usize {
    outs.iter()
        .position(|o| o.rect().contains(libluna_core::Point::new(x, y)))
        .unwrap_or(0)
}

/// Wysyła klientowi opis monitora (geometria, tryb, skala, nazwa, gotowe).
pub fn send_info(output: &WlOutput, info: &OutputInfo) {
    let _ = output.send_event(wl_output::Event::Geometry {
        x: info.x,
        y: info.y,
        physical_width: info.width * 300 / 1000,
        physical_height: info.height * 300 / 1000,
        subpixel: wayland_server::WEnum::Value(wl_output::Subpixel::Unknown),
        make: "Luna".to_owned(),
        model: info.name.clone(),
        transform: wayland_server::WEnum::Value(wl_output::Transform::Normal),
    });
    let _ = output.send_event(wl_output::Event::Mode {
        flags: wayland_server::WEnum::Value(wl_output::Mode::Current),
        width: info.width,
        height: info.height,
        refresh: 60_000,
    });
    let _ = output.send_event(wl_output::Event::Scale { factor: info.scale });
    let _ = output.send_event(wl_output::Event::Name {
        name: info.name.clone(),
    });
    let _ = output.send_event(wl_output::Event::Done);
}

impl GlobalDispatch<WlOutput, usize> for State {
    fn bind(
        state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlOutput>,
        data: &usize,
        data_init: &mut DataInit<'_, Self>,
    ) {
        // Zasób niesie swój indeks monitora (potrzebny m.in. dla `wlr-layer-shell`).
        let output = data_init.init(resource, *data);
        let info = state
            .outputs
            .lock()
            .unwrap()
            .get(*data)
            .cloned()
            .unwrap_or_default();
        send_info(&output, &info);
    }
}

impl Dispatch<WlOutput, usize> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlOutput,
        _request: wl_output::Request,
        _data: &usize,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(name: &str, w: i32, h: i32) -> OutputInfo {
        OutputInfo {
            name: name.to_owned(),
            width: w,
            height: h,
            scale: 1,
            x: 0,
            y: 0,
        }
    }

    #[test]
    fn horizontal_layout_places_monitors_side_by_side() {
        let mut outs = vec![out("eDP-1", 1920, 1080), out("HDMI-1", 1280, 720)];
        layout_horizontal(&mut outs);
        assert_eq!((outs[0].x, outs[0].y), (0, 0));
        assert_eq!((outs[1].x, outs[1].y), (1920, 0));
    }

    #[test]
    fn output_at_finds_the_monitor_under_the_point() {
        let mut outs = vec![out("eDP-1", 1920, 1080), out("HDMI-1", 1280, 720)];
        layout_horizontal(&mut outs);
        assert_eq!(output_at(&outs, 100.0, 100.0), 0);
        assert_eq!(output_at(&outs, 2000.0, 100.0), 1);
        // Poza wszystkimi monitorami → pierwszy.
        assert_eq!(output_at(&outs, -500.0, -500.0), 0);
    }
}
