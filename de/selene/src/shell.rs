//! `xdg_wm_base` / `xdg_surface` / `xdg_toplevel` — okna aplikacji (protokół xdg-shell).

use std::sync::Mutex;

use wayland_protocols::xdg::shell::server::{
    xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::compositor::SurfaceData;
use crate::state::State;

/// Dane `xdg_surface`.
#[allow(dead_code)] // `wl_surface` przyda się przy renderowaniu/geometrii w kolejnych krokach
pub struct XdgSurfaceData {
    /// Powiązana powierzchnia `wl_surface`.
    pub wl_surface: WlSurface,
    /// Utworzona powierzchnia najwyższego poziomu (jeśli `get_toplevel`).
    pub toplevel: Mutex<Option<xdg_toplevel::XdgToplevel>>,
    /// Czy wysłano już pierwszą konfigurację.
    pub configured: Mutex<bool>,
}

impl XdgSurfaceData {
    pub fn new(wl_surface: WlSurface) -> Self {
        Self {
            wl_surface,
            toplevel: Mutex::new(None),
            configured: Mutex::new(false),
        }
    }
}

/// Dane `xdg_toplevel`.
pub struct XdgToplevelData {
    pub title: Mutex<String>,
    pub app_id: Mutex<String>,
    /// Powiązana powierzchnia (do aktualizacji modelu okien).
    pub surface: WlSurface,
}

impl XdgToplevelData {
    pub fn new(surface: WlSurface) -> Self {
        Self {
            title: Mutex::new(String::new()),
            app_id: Mutex::new(String::new()),
            surface,
        }
    }
}

impl GlobalDispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<xdg_wm_base::XdgWmBase>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &xdg_wm_base::XdgWmBase,
        request: xdg_wm_base::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            xdg_wm_base::Request::CreatePositioner { id } => {
                data_init.init(id, ());
            }
            xdg_wm_base::Request::GetXdgSurface { id, surface } => {
                let xdg = data_init.init(id, XdgSurfaceData::new(surface.clone()));
                // Zapamiętaj powiązanie po stronie `wl_surface` (do konfiguracji przy commit).
                if let Some(sd) = surface.data::<SurfaceData>() {
                    *sd.xdg_surface.lock().unwrap() = Some(xdg);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, XdgSurfaceData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &xdg_surface::XdgSurface,
        request: xdg_surface::Request,
        data: &XdgSurfaceData,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            xdg_surface::Request::GetToplevel { id } => {
                let toplevel = data_init.init(id, XdgToplevelData::new(data.wl_surface.clone()));
                *data.toplevel.lock().unwrap() = Some(toplevel);
            }
            xdg_surface::Request::AckConfigure { .. } => {
                // Konfiguracja potwierdzona — nic więcej nie trzeba po stronie MVP.
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_toplevel::XdgToplevel, XdgToplevelData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &xdg_toplevel::XdgToplevel,
        request: xdg_toplevel::Request,
        data: &XdgToplevelData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            xdg_toplevel::Request::SetTitle { title } => {
                *data.title.lock().unwrap() = title.clone();
                state.toplevels += 1;
                // Odśwież tytuł w modelu okien (dekoracja rysuje go na pasku).
                if let Some(sd) = data.surface.data::<SurfaceData>() {
                    if let Some(slot) = *sd.slot.lock().unwrap() {
                        if let Some(window) = state.windows.lock().unwrap().get_mut(slot) {
                            window.title = title;
                        }
                    }
                }
            }
            xdg_toplevel::Request::SetAppId { app_id } => {
                *data.app_id.lock().unwrap() = app_id;
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_positioner::XdgPositioner, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &xdg_positioner::XdgPositioner,
        _request: xdg_positioner::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}
