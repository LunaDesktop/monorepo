//! `wl_seat` — minimalne wejście (na razie bez zdarzeń; obiekty tworzone, by klienci działali).

use wayland_server::{
    protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch},
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::state::State;

impl GlobalDispatch<wl_seat::WlSeat, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<wl_seat::WlSeat>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let seat = data_init.init(resource, ());
        let _ = seat.send_event(wl_seat::Event::Capabilities {
            capabilities: wayland_server::WEnum::Value(wl_seat::Capability::empty()),
        });
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &wl_seat::WlSeat,
        request: wl_seat::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_seat::Request::GetPointer { id } => {
                data_init.init(id, ());
            }
            wl_seat::Request::GetKeyboard { id } => {
                data_init.init(id, ());
            }
            wl_seat::Request::GetTouch { id } => {
                data_init.init(id, ());
            }
            _ => {}
        }
    }
}

macro_rules! empty_dispatch {
    ($iface:ty) => {
        impl Dispatch<$iface, ()> for State {
            fn request(
                _state: &mut Self,
                _client: &Client,
                _resource: &$iface,
                _request: <$iface as wayland_server::Resource>::Request,
                _data: &(),
                _handle: &DisplayHandle,
                _data_init: &mut DataInit<'_, Self>,
            ) {
            }
        }
    };
}

empty_dispatch!(wl_pointer::WlPointer);
empty_dispatch!(wl_keyboard::WlKeyboard);
empty_dispatch!(wl_touch::WlTouch);
