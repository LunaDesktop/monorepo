//! `wp_fractional_scale_v1` — kompozytor informuje o preferowanej skali ułamkowej.
//!
//! Selene ma dziś wyjścia 1×, więc wysyła `preferred_scale = 120` (1.0). Protokół
//! jest już jednak obecny, a klienty (GTK4, Chromium) mogą z niego korzystać bez
//! rozjeżdżania UI — po dodaniu HiDPI wystarczy zmienić wartość i `buffer_scale`.
//! Skalowanie realizuje `wp_viewporter` (destination w rozmiarze logicznym).

use wayland_protocols::wp::fractional_scale::v1::server::{
    wp_fractional_scale_manager_v1::{Request as ManagerRequest, WpFractionalScaleManagerV1},
    wp_fractional_scale_v1::{Event as ScaleEvent, Request as ScaleRequest, WpFractionalScaleV1},
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::compositor::SurfaceData;
use crate::state::State;
use crate::sync::LockExt;

/// Skala wyjść w jednostkach 1/120 (120 = 1.0).
pub const SCALE_120: u32 = 120;

pub struct FractionalScaleData {
    pub surface: WlSurface,
}

impl GlobalDispatch<WpFractionalScaleManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpFractionalScaleManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpFractionalScaleManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WpFractionalScaleManagerV1,
        request: ManagerRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let ManagerRequest::GetFractionalScale { id, surface } = request {
            let resource = data_init.init(
                id,
                FractionalScaleData {
                    surface: surface.clone(),
                },
            );
            // Preferowana skala jest znana od razu (wyjścia mają dziś 1×).
            let _ = resource.send_event(ScaleEvent::PreferredScale { scale: SCALE_120 });
            if let Some(data) = surface.data::<SurfaceData>() {
                *data.fractional_scale.lock_or_recover() = Some(resource);
            }
        }
    }
}

impl Dispatch<WpFractionalScaleV1, FractionalScaleData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &WpFractionalScaleV1,
        request: ScaleRequest,
        data: &FractionalScaleData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let ScaleRequest::Destroy = request {
            if let Some(surface_data) = data.surface.data::<SurfaceData>() {
                let mut slot = surface_data.fractional_scale.lock_or_recover();
                if slot.as_ref().is_some_and(|r| r.id() == resource.id()) {
                    *slot = None;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_one_is_120() {
        assert_eq!(SCALE_120, 120);
    }
}
