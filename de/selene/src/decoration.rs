//! `zxdg_decoration_manager_v1` — negocjacja dekoracji okien.
//!
//! Selene zawsze rysuje dekoracje po stronie serwera (SSD), więc klientom
//! korzystającym z tego protokołu odpowiadamy trybem `server_side`.

use wayland_protocols::xdg::decoration::zv1::server::{
    zxdg_decoration_manager_v1 as decoration_manager,
    zxdg_toplevel_decoration_v1 as toplevel_decoration,
};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::shell::XdgToplevelData;
use crate::state::State;

/// Uchwyt obiektu dekoracji okna.
pub type ToplevelDecoration = toplevel_decoration::ZxdgToplevelDecorationV1;

impl GlobalDispatch<decoration_manager::ZxdgDecorationManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<decoration_manager::ZxdgDecorationManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<decoration_manager::ZxdgDecorationManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &decoration_manager::ZxdgDecorationManagerV1,
        request: decoration_manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let decoration_manager::Request::GetToplevelDecoration { id, toplevel } = request {
            let deco = data_init.init(id, ());
            if let Some(td) = toplevel.data::<XdgToplevelData>() {
                *td.decoration.lock().unwrap() = Some(deco);
            }
        }
    }
}

impl Dispatch<toplevel_decoration::ZxdgToplevelDecorationV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &toplevel_decoration::ZxdgToplevelDecorationV1,
        request: toplevel_decoration::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            // Klient prosi o tryb — my i tak dekorujemy po stronie serwera.
            toplevel_decoration::Request::SetMode { .. }
            | toplevel_decoration::Request::UnsetMode => {
                let _ = resource.send_event(toplevel_decoration::Event::Configure {
                    mode: WEnum::Value(toplevel_decoration::Mode::ServerSide),
                });
            }
            _ => {}
        }
    }
}
