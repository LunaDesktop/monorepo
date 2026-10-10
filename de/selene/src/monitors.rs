//! `wl_output` — monitory (na razie jeden; model gotowy na wiele).

use std::sync::{Arc, Mutex};

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
}

impl Default for OutputInfo {
    fn default() -> Self {
        Self {
            name: "nested-0".to_owned(),
            width: 1280,
            height: 800,
            scale: 1,
        }
    }
}

/// Współdzielona lista monitorów.
pub type Outputs = Arc<Mutex<Vec<OutputInfo>>>;

/// Wysyła klientowi opis monitora (geometria, tryb, skala, nazwa, gotowe).
pub fn send_info(output: &WlOutput, info: &OutputInfo) {
    let _ = output.send_event(wl_output::Event::Geometry {
        x: 0,
        y: 0,
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

impl GlobalDispatch<WlOutput, ()> for State {
    fn bind(
        state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlOutput>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let output = data_init.init(resource, ());
        let info = state
            .outputs
            .lock()
            .unwrap()
            .first()
            .cloned()
            .unwrap_or_default();
        send_info(&output, &info);
    }
}

impl Dispatch<WlOutput, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlOutput,
        _request: wl_output::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}
