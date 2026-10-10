//! `wl_compositor`, `wl_surface`, `wl_region`, `wl_callback` — rdzeń powierzchni.

use std::sync::Mutex;

use libluna_render::ImageData;
use wayland_protocols::xdg::shell::server::{xdg_surface, xdg_toplevel};
use wayland_server::{
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_region, wl_surface},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::shell::XdgSurfaceData;
use crate::shm::BufferData;
use crate::state::{State, Window};

/// Dane powierzchni (`wl_surface`).
pub struct SurfaceData {
    /// Oczekujące `frame`-callbacki do wywołania przy następnym `commit`.
    pub frames: Mutex<Vec<wl_callback::WlCallback>>,
    /// Powiązana powierzchnia `xdg_surface` (jeśli klient ją utworzyła).
    pub xdg_surface: Mutex<Option<xdg_surface::XdgSurface>>,
    /// Ostatnio przypięty bufor (`wl_surface.attach`).
    pub buffer: Mutex<Option<wl_buffer::WlBuffer>>,
    /// Slot okna w modelu (przydzielany przy pierwszym commicie).
    pub slot: Mutex<Option<usize>>,
}

impl SurfaceData {
    pub fn new() -> Self {
        Self {
            frames: Mutex::new(Vec::new()),
            xdg_surface: Mutex::new(None),
            buffer: Mutex::new(None),
            slot: Mutex::new(None),
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
            wl_surface::Request::Attach { buffer, .. } => {
                *data.buffer.lock().unwrap() = buffer;
            }
            wl_surface::Request::Commit => {
                // Wywołaj oczekujące frame-callbacki.
                let now = state.time_ms();
                for cb in data.frames.lock().unwrap().drain(..) {
                    let _ = cb.send_event(wl_callback::Event::Done { callback_data: now });
                }
                // Pierwszy commit powierzchni z `xdg_surface` → wyślij konfigurację.
                maybe_configure_xdg(state, data);
                // Skomponuj okno (jeśli to toplevel z buforem).
                composite(state, data);
            }
            _ => {}
        }
    }
}

/// Skomponuj powierzchnię do modelu okien, jeśli jest to toplevel z buforem `wl_shm`.
fn composite(state: &mut State, data: &SurfaceData) {
    let Some(xdg_surface) = data.xdg_surface.lock().unwrap().clone() else {
        return;
    };
    let is_toplevel = xdg_surface
        .data::<XdgSurfaceData>()
        .map(|d| d.toplevel.lock().unwrap().is_some())
        .unwrap_or(false);
    if !is_toplevel {
        return;
    }
    let Some(buffer) = data.buffer.lock().unwrap().clone() else {
        return;
    };
    let Some(buf_data) = buffer.data::<BufferData>() else {
        return;
    };
    let (w, h) = (buf_data.width, buf_data.height);
    let Some(rgba) = buf_data.to_rgba() else {
        return;
    };
    let image = std::sync::Arc::new(ImageData::new(w as u32, h as u32, rgba));

    let mut slot = data.slot.lock().unwrap();
    let mut windows = state.windows.lock().unwrap();
    let idx = match *slot {
        Some(i) => i,
        None => {
            let i = windows.len();
            windows.push(Window {
                x: 60.0 + i as f64 * 36.0,
                y: 60.0 + i as f64 * 36.0,
                width: w,
                height: h,
                data: image.clone(),
            });
            *slot = Some(i);
            i
        }
    };
    if let Some(window) = windows.get_mut(idx) {
        window.width = w;
        window.height = h;
        window.data = image;
    }
    drop(windows);
    drop(slot);
    // Pozwól klientowi ponownie użyć bufora.
    let _ = buffer.send_event(wl_buffer::Event::Release);
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
