//! `zwp_idle_inhibit_v1` — aplikacja może zablokować wygaszanie ekranu.
//!
//! Odtwarzacze wideo, prezentacje i gry tworzą inhibitor powiązany z powierzchnią.
//! Dopóki inhibitor żyje, Selene nie wykonuje żadnej akcji bezczynności
//! (`Blank`/`Lock`/`Suspend`) i nie budzi się na ich deadline.

use wayland_protocols::wp::idle_inhibit::zv1::server::{
    zwp_idle_inhibit_manager_v1::{self as manager, ZwpIdleInhibitManagerV1},
    zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;

impl GlobalDispatch<ZwpIdleInhibitManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpIdleInhibitManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpIdleInhibitManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpIdleInhibitManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            manager::Request::CreateInhibitor { id, surface: _ } => {
                let inhibitor = data_init.init(id, ());
                state.idle_inhibitors.retain(|i| i.is_alive());
                state.idle_inhibitors.push(inhibitor);
            }
            manager::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpIdleInhibitorV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpIdleInhibitorV1,
        request: <ZwpIdleInhibitorV1 as Resource>::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let wayland_protocols::wp::idle_inhibit::zv1::server::zwp_idle_inhibitor_v1::Request::Destroy =
            request
        {
            state.idle_inhibitors.retain(|i| i.id() != resource.id());
        }
    }
}

/// Czy istnieje żywy inhibitor bezczynności.
pub fn inhibited(state: &State) -> bool {
    state.idle_inhibitors.iter().any(|i| i.is_alive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_is_not_inhibited() {
        assert!(!inhibited(&State::default()));
    }
}
