//! `zwp_relative_pointer_v1` — względny ruch kursora dla gier i mouselooku.
//!
//! Gry (natywne i przez XWayland) potrzebują przesunięć niezależnych od krawędzi
//! ekranu. Bez tego protokołu kursor „zatrzymywał się” na brzegu monitora, a celowanie
//! i obracanie kamery było niemożliwe. Zdarzenia `relative_motion` wysyłamy razem
//! z `wl_pointer.motion` w tej samej klatce, dla obiektów tego samego klienta.

use wayland_protocols::wp::relative_pointer::zv1::server::{
    zwp_relative_pointer_manager_v1::{self, ZwpRelativePointerManagerV1},
    zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
};
use wayland_server::protocol::wl_pointer::WlPointer;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::{RelativePointer, State};

impl GlobalDispatch<ZwpRelativePointerManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpRelativePointerManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpRelativePointerManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpRelativePointerManagerV1,
        request: zwp_relative_pointer_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwp_relative_pointer_manager_v1::Request::GetRelativePointer { id, pointer } => {
                state
                    .relative_pointers
                    .retain(|entry| entry.resource.is_alive() && entry.pointer.is_alive());
                let resource = data_init.init(id, ());
                state
                    .relative_pointers
                    .push(RelativePointer { pointer, resource });
            }
            zwp_relative_pointer_manager_v1::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpRelativePointerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpRelativePointerV1,
        request: zwp_relative_pointer_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwp_relative_pointer_v1::Request::Destroy = request {
            state
                .relative_pointers
                .retain(|entry| entry.resource.id() != resource.id());
        }
    }
}

/// Wysyła `relative_motion` wszystkim obiektom klienta, do którego należy `pointer`.
pub fn send_relative_motion(state: &mut State, pointer: &WlPointer, dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    state
        .relative_pointers
        .retain(|entry| entry.resource.is_alive() && entry.pointer.is_alive());
    let Some(client) = pointer.client() else {
        return;
    };
    let utime = u64::from(state.time_ms()) * 1000;
    for entry in &state.relative_pointers {
        if entry.pointer.client().as_ref() != Some(&client) {
            continue;
        }
        let _ = entry
            .resource
            .send_event(zwp_relative_pointer_v1::Event::RelativeMotion {
                utime_hi: (utime >> 32) as u32,
                utime_lo: (utime & 0xffff_ffff) as u32,
                dx,
                dy,
                dx_unaccel: dx,
                dy_unaccel: dy,
            });
    }
}
