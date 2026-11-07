//! `wp_tearing_control_v1` — gry mogą prosić o prezentację bez vsync.
//!
//! Przyjmujemy i zapamiętujemy preferencję, ale zawsze synchronizujemy do vblanku:
//! protokół wprost pozwala kompozytorowi zignorować `async`, a overlay/scanout
//! z tearingiem wymaga wsparcia w warstwie wyjścia (DRM atomic + plane). Ważne, by
//! global istniał — klient (np. Proton/Wine) inaczej zgłasza brak wsparcia i nie
//! włącza trybu „low latency”.

use wayland_protocols::wp::tearing_control::v1::server::{
    wp_tearing_control_manager_v1 as manager, wp_tearing_control_v1 as control,
};
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::compositor::SurfaceData;
use crate::state::State;
use crate::sync::LockExt;

/// Preferencja prezentacji zgłoszona przez klienta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresentationHint {
    /// Synchronizacja do vblanku (domyślne zachowanie kompozytora).
    #[default]
    Vsync,
    /// Klient akceptuje tearing.
    Async,
}

/// Stan zasobu `wp_tearing_control_v1`. Preferencja trafia do danych powierzchni.
pub struct TearingData {
    surface: WlSurface,
}

impl GlobalDispatch<manager::WpTearingControlManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<manager::WpTearingControlManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<manager::WpTearingControlManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &manager::WpTearingControlManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let manager::Request::GetTearingControl { id, surface } = request {
            data_init.init(id, TearingData { surface });
        }
    }
}

impl Dispatch<control::WpTearingControlV1, TearingData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &control::WpTearingControlV1,
        request: control::Request,
        data: &TearingData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let control::Request::SetPresentationHint { hint } = request {
            let value = match hint {
                WEnum::Value(control::PresentationHint::Async) => PresentationHint::Async,
                _ => PresentationHint::Vsync,
            };
            if let Some(surface_data) = data.surface.data::<SurfaceData>() {
                *surface_data.tearing.lock_or_recover() = value;
            }
        }
    }
}
