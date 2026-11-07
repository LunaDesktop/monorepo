//! `zwp_keyboard_shortcuts_inhibit_v1` — klient może przejąć skróty pulpitu.
//!
//! Potrzebują tego gry, emulatory i klienty zdalnego pulpitu: bez inhibitora
//! Super/Alt+Tab/Alt+F4 są zjadane przez kompozytor i aplikacja nigdy ich nie widzi.

use std::sync::Mutex;

use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::server::{
    zwp_keyboard_shortcuts_inhibit_manager_v1 as manager,
    zwp_keyboard_shortcuts_inhibitor_v1 as inhibitor,
};
use wayland_server::protocol::wl_seat::WlSeat;
use wayland_server::protocol::wl_surface::WlSurface;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;
use crate::sync::LockExt;

/// Inhibitor jednej powierzchni.
pub struct InhibitorData {
    surface: WlSurface,
    seat: WlSeat,
    /// Czy klient już dostał `active`/`inactive` — bez tego sypalibyśmy zdarzeniami
    /// przy każdym kliknięciu w okno.
    active: Mutex<bool>,
}

impl GlobalDispatch<manager::ZwpKeyboardShortcutsInhibitManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<manager::ZwpKeyboardShortcutsInhibitManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<manager::ZwpKeyboardShortcutsInhibitManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &manager::ZwpKeyboardShortcutsInhibitManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let manager::Request::InhibitShortcuts { id, surface, seat } = request {
            let resource = data_init.init(
                id,
                InhibitorData {
                    surface,
                    seat,
                    active: Mutex::new(false),
                },
            );
            state.shortcuts_inhibitors.retain(|r| r.is_alive());
            state.shortcuts_inhibitors.push(resource);
            refresh(state);
        }
    }
}

impl Dispatch<inhibitor::ZwpKeyboardShortcutsInhibitorV1, InhibitorData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &inhibitor::ZwpKeyboardShortcutsInhibitorV1,
        request: inhibitor::Request,
        _data: &InhibitorData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if matches!(request, inhibitor::Request::Destroy) {
            state
                .shortcuts_inhibitors
                .retain(|r| r.id() != resource.id());
        }
    }
}

/// Czy inhibitor dotyczy powierzchni, która ma teraz klawiaturę.
///
/// Fokus trzyma toplevel, a klient może zahamować skróty powierzchnią popupu lub
/// powierzchnią X11 — dlatego porównujemy okno (slot), a gdy inhibitor nie jest
/// jeszcze oknem, wystarcza ten sam klient.
fn is_active(state: &State, data: &InhibitorData) -> bool {
    let Some((focus_slot, focus_surface)) = state.keyboard_focus.as_ref() else {
        return false;
    };
    if focus_surface == &data.surface {
        return true;
    }
    let same_client = match (focus_surface.client(), data.surface.client()) {
        (Some(a), Some(b)) => a.id() == b.id(),
        _ => false,
    };
    if !same_client {
        return false;
    }
    match state.slots.iter().position(|s| s == &data.surface) {
        Some(slot) => slot == *focus_slot,
        None => true,
    }
}

/// Czy któryś aktywny inhibitor blokuje skróty pulpitu.
pub fn shortcuts_inhibited(state: &State) -> bool {
    state.shortcuts_inhibitors.iter().any(|r| {
        r.is_alive()
            && r.data::<InhibitorData>()
                .map(|data| is_active(state, data))
                .unwrap_or(false)
    })
}

/// Przelicza stan inhibitorów i wysyła `active`/`inactive` tylko przy zmianie.
pub fn refresh(state: &mut State) {
    state.shortcuts_inhibitors.retain(|r| r.is_alive());
    for resource in &state.shortcuts_inhibitors {
        let Some(data) = resource.data::<InhibitorData>() else {
            continue;
        };
        if !data.seat.is_alive() {
            continue;
        }
        let active = is_active(state, data);
        let mut known = data.active.lock_or_recover();
        if *known != active {
            *known = active;
            if active {
                let _ = resource.send_event(inhibitor::Event::Active);
            } else {
                let _ = resource.send_event(inhibitor::Event::Inactive);
            }
        }
    }
}
