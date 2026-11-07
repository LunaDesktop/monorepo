//! `wp_viewporter` — niezależny od bufora rozmiar powierzchni (skalowanie/kadrowanie).
//!
//! Klient może ustawić `destination` (rozmiar logiczny) i `source` (wycinek bufora).
//! Dziś wykorzystujemy `destination` do rysowania bufora w mniejszym/większym
//! prostokącie — to podstawa dla `wp_fractional_scale_v1` i skalowania wideo.
//! `source` jest zapamiętywany i będzie używany przez shadery/CPU w kolejnym kroku.

use std::sync::Mutex;

use wayland_protocols::wp::viewporter::server::{
    wp_viewport::{Request as ViewportRequest, WpViewport},
    wp_viewporter::{Request as ViewporterRequest, WpViewporter},
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::compositor::SurfaceData;
use crate::state::{State, ViewportState};
use crate::sync::LockExt;

/// Dane `wp_viewport` — powierzchnia, do której należy.
pub struct ViewportData {
    pub surface: WlSurface,
}

impl GlobalDispatch<WpViewporter, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpViewporter>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpViewporter, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WpViewporter,
        request: ViewporterRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ViewporterRequest::GetViewport { id, surface } => {
                let data = ViewportData { surface };
                data_init.init(id, data);
            }
            ViewporterRequest::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<WpViewport, ViewportData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WpViewport,
        request: ViewportRequest,
        data: &ViewportData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Some(surface_data) = data.surface.data::<SurfaceData>() else {
            return;
        };
        match request {
            ViewportRequest::SetSource {
                x,
                y,
                width,
                height,
            } => {
                let state = viewport_for(&surface_data);
                let mut state = state.lock_or_recover();
                let vp = state.get_or_insert_with(ViewportState::default);
                if (x, y, width, height) == (-1.0, -1.0, -1.0, -1.0) {
                    vp.source = None;
                } else if x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0 {
                    vp.source = Some((x, y, width, height));
                }
            }
            ViewportRequest::SetDestination { width, height } => {
                let state = viewport_for(&surface_data);
                let mut state = state.lock_or_recover();
                let vp = state.get_or_insert_with(ViewportState::default);
                if width == -1 && height == -1 {
                    vp.destination = None;
                } else if width > 0 && height > 0 {
                    vp.destination = Some((width, height));
                }
            }
            ViewportRequest::Destroy => {
                *viewport_for(&surface_data).lock_or_recover() = None;
            }
            _ => {}
        }
    }
}

fn viewport_for(data: &SurfaceData) -> &Mutex<Option<ViewportState>> {
    &data.viewport
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_state_default_is_empty() {
        let vp = ViewportState::default();
        assert!(vp.source.is_none());
        assert!(vp.destination.is_none());
    }
}
