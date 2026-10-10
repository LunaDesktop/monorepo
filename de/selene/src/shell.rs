//! `xdg_wm_base` / `xdg_surface` / `xdg_toplevel` — okna aplikacji (protokół xdg-shell).

use std::sync::Mutex;

use wayland_protocols::xdg::shell::server::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::compositor::SurfaceData;
use crate::state::State;

/// Geometria popupu zebrana z `xdg_positioner` (uproszczona: prostokąt kotwicy + offset).
#[derive(Debug, Clone, Copy, Default)]
pub struct PopupGeom {
    pub width: i32,
    pub height: i32,
    pub anchor_x: f64,
    pub anchor_y: f64,
    pub anchor_w: f64,
    pub anchor_h: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

/// Dane `xdg_positioner`.
#[derive(Debug, Default)]
pub struct XdgPositionerData {
    pub geom: Mutex<PopupGeom>,
}

/// Dane `xdg_surface`.
#[allow(dead_code)]
pub struct XdgSurfaceData {
    /// Powiązana powierzchnia `wl_surface`.
    pub wl_surface: WlSurface,
    /// Utworzona powierzchnia najwyższego poziomu (jeśli `get_toplevel`).
    pub toplevel: Mutex<Option<xdg_toplevel::XdgToplevel>>,
    /// Utworzony popup (jeśli `get_popup`).
    pub popup: Mutex<Option<xdg_popup::XdgPopup>>,
    /// Powierzchnia rodzica (dla popupu).
    pub parent: Mutex<Option<WlSurface>>,
    /// Geometria popupu z positionera.
    pub geom: Mutex<PopupGeom>,
    /// Czy to popup.
    pub is_popup: Mutex<bool>,
    /// Czy wysłano już pierwszą konfigurację.
    pub configured: Mutex<bool>,
}

impl XdgSurfaceData {
    pub fn new(wl_surface: WlSurface) -> Self {
        Self {
            wl_surface,
            toplevel: Mutex::new(None),
            popup: Mutex::new(None),
            parent: Mutex::new(None),
            geom: Mutex::new(PopupGeom::default()),
            is_popup: Mutex::new(false),
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
                data_init.init(id, XdgPositionerData::default());
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
            xdg_surface::Request::GetPopup {
                id,
                parent,
                positioner,
            } => {
                let popup = data_init.init(id, ());
                *data.popup.lock().unwrap() = Some(popup);
                *data.is_popup.lock().unwrap() = true;
                *data.parent.lock().unwrap() = parent
                    .as_ref()
                    .and_then(|p| p.data::<XdgSurfaceData>())
                    .map(|p| p.wl_surface.clone());
                *data.geom.lock().unwrap() = positioner
                    .data::<XdgPositionerData>()
                    .map(|p| *p.geom.lock().unwrap())
                    .unwrap_or_default();
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

impl Dispatch<xdg_positioner::XdgPositioner, XdgPositionerData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &xdg_positioner::XdgPositioner,
        request: xdg_positioner::Request,
        data: &XdgPositionerData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let mut geom = data.geom.lock().unwrap();
        match request {
            xdg_positioner::Request::SetSize { width, height } => {
                geom.width = width;
                geom.height = height;
            }
            xdg_positioner::Request::SetAnchorRect {
                x,
                y,
                width,
                height,
            } => {
                geom.anchor_x = x as f64;
                geom.anchor_y = y as f64;
                geom.anchor_w = width as f64;
                geom.anchor_h = height as f64;
            }
            xdg_positioner::Request::SetOffset { x, y } => {
                geom.offset_x = x as f64;
                geom.offset_y = y as f64;
            }
            _ => {}
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &xdg_popup::XdgPopup,
        _request: xdg_popup::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}
