//! `wl_compositor`, `wl_surface`, `wl_region`, `wl_callback` — rdzeń powierzchni.

use std::sync::Mutex;

use wayland_protocols::xdg::shell::server::{xdg_surface, xdg_toplevel};
use wayland_server::{
    protocol::{wl_callback, wl_compositor, wl_region, wl_surface},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::shell::XdgSurfaceData;
use crate::state::State;

/// Dane powierzchni (`wl_surface`).
pub struct SurfaceData {
    /// Oczekujące `frame`-callbacki do wywołania przy następnym `commit`.
    pub frames: Mutex<Vec<wl_callback::WlCallback>>,
    /// Powiązana powierzchnia `xdg_surface` (jeśli klient ją utworzył).
    pub xdg_surface: Mutex<Option<xdg_surface::XdgSurface>>,
}

impl SurfaceData {
    pub fn new() -> Self {
        Self {
            frames: Mutex::new(Vec::new()),
            xdg_surface: Mutex::new(None),
        }
    }
}

impl Default for SurfaceData {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalDispatch<wl_compositor::WlCompositor, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<wl_compositor::WlCompositor>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                data_init.init(id, SurfaceData::new());
                state.surfaces += 1;
            }
            wl_compositor::Request::CreateRegion { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_region::WlRegion, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_region::WlRegion,
        _request: wl_region::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_callback::WlCallback,
        _request: wl_callback::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, SurfaceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &wl_surface::WlSurface,
        request: wl_surface::Request,
        data: &SurfaceData,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Frame { callback } => {
                let cb = data_init.init(callback, ());
                data.frames.lock().unwrap().push(cb);
            }
            wl_surface::Request::Commit => {
                // Wywołaj oczekujące frame-callbacki.
                let now = state.time_ms();
                for cb in data.frames.lock().unwrap().drain(..) {
                    let _ = cb.send_event(wl_callback::Event::Done { callback_data: now });
                }
                // Pierwszy commit powierzchni z `xdg_surface` → wyślij konfigurację.
                maybe_configure_xdg(state, data);
            }
            _ => {}
        }
    }
}

/// Jeśli powierzchnia ma `xdg_surface` jeszcze nie skonfigurowany, wyślij configure.
fn maybe_configure_xdg(state: &mut State, data: &SurfaceData) {
    let Some(xdg_surface) = data.xdg_surface.lock().unwrap().clone() else {
        return;
    };
    let Some(xdg_data) = xdg_surface.data::<XdgSurfaceData>() else {
        return;
    };
    if *xdg_data.configured.lock().unwrap() {
        return;
    }
    let serial = state.next_serial();
    let _ = xdg_surface.send_event(xdg_surface::Event::Configure { serial });
    if let Some(toplevel) = xdg_data.toplevel.lock().unwrap().as_ref() {
        let _ = toplevel.send_event(xdg_toplevel::Event::Configure {
            width: 1024,
            height: 720,
            states: vec![3], // ACTIVATED
        });
        let _ = toplevel.send_event(xdg_toplevel::Event::WmCapabilities {
            capabilities: vec![],
        });
    }
    *xdg_data.configured.lock().unwrap() = true;
}
