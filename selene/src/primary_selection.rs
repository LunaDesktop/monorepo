//! `zwp_primary_selection_v1` — selekcja pierwotna (wklejanie środkowym przyciskiem).
//!
//! To osobny schowek od `wl_data_device` (Ctrl+C/Ctrl+V). Terminale, edytory
//! i przeglądarki wklejają z niego po kliknięciu środkowym przyciskiem, bez
//! nadpisywania „głównego” schowka. Protokół jest prawie identyczny z
//! `wl_data_device`, ale nie ma drag & drop i wymaga zachowania kolejności:
//! `selection` musi dotrzeć bezpośrednio przed `wl_keyboard.enter` — inaczej
//! klienty (GTK, XWayland) unieważniają ofertę po utracie fokusu.
//!
//! Źródłem może być zwykły `zwp_primary_selection_source_v1` albo
//! `ext_data_control_source_v1` (menedżer schowka). Ujednolica je
//! [`SelectionSource`] z [`crate::data`].

use std::os::fd::AsFd;
use std::sync::Mutex;

use wayland_protocols::wp::primary_selection::zv1::server::{
    zwp_primary_selection_device_manager_v1::{
        self as manager, ZwpPrimarySelectionDeviceManagerV1,
    },
    zwp_primary_selection_device_v1::{
        Event as DeviceEvent, Request as DeviceRequest, ZwpPrimarySelectionDeviceV1,
    },
    zwp_primary_selection_offer_v1::{
        Event as OfferEvent, Request as OfferRequest, ZwpPrimarySelectionOfferV1,
    },
    zwp_primary_selection_source_v1::{Request as SourceRequest, ZwpPrimarySelectionSourceV1},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::data::{MimeData, SelectionSource};
use crate::state::State;
use crate::sync::LockExt;

/// Dane `zwp_primary_selection_offer_v1`: źródło, z którego oferta czerpie dane.
pub struct OfferData {
    pub source: Mutex<Option<SelectionSource>>,
}

#[derive(Default)]
pub struct PrimaryDeviceData {}

impl GlobalDispatch<ZwpPrimarySelectionDeviceManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpPrimarySelectionDeviceManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpPrimarySelectionDeviceManagerV1,
        request: manager::Request,
        _data: &(),
        handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            manager::Request::CreateSource { id } => {
                data_init.init(id, MimeData::default());
            }
            manager::Request::GetDevice { id, seat: _ } => {
                let dev = data_init.init(id, PrimaryDeviceData::default());
                state.primary_devices.retain(|d| d.is_alive());
                state.primary_devices.push(dev.clone());
                // Nowy klient od razu dostaje bieżącą selekcję — tak samo jak
                // `wl_data_device` przy `get_data_device`.
                send_selection(handle, &dev, state.primary_selection.as_ref());
            }
            manager::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionSourceV1, MimeData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpPrimarySelectionSourceV1,
        request: SourceRequest,
        data: &MimeData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            SourceRequest::Offer { mime_type } => data.add(mime_type),
            SourceRequest::Destroy => {
                // Martwe źródło nie może zostać bieżącą selekcją — klient
                // zakończył się bez `set_selection(None)`.
                let source = SelectionSource::Primary(resource.clone());
                if state
                    .primary_selection
                    .as_ref()
                    .is_some_and(|current| current.id() == source.id())
                {
                    state.primary_selection = None;
                    broadcast_selection(state, handle, None);
                    crate::data_control::broadcast_primary_selection(state, handle, None);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionDeviceV1, PrimaryDeviceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpPrimarySelectionDeviceV1,
        request: DeviceRequest,
        _data: &PrimaryDeviceData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            DeviceRequest::SetSelection { source, serial: _ } => {
                let new = source.clone().map(SelectionSource::Primary);
                if let Some(old) = state.primary_selection.as_ref() {
                    let same = new.as_ref().is_some_and(|new| new.id() == old.id());
                    if !same {
                        old.cancel();
                    }
                }
                state.primary_selection = new;
                let current = state.primary_selection.clone();
                broadcast_selection(state, handle, current.as_ref());
                crate::data_control::broadcast_primary_selection(state, handle, current.as_ref());
            }
            DeviceRequest::Destroy => {
                state.primary_devices.retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPrimarySelectionOfferV1, OfferData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ZwpPrimarySelectionOfferV1,
        request: OfferRequest,
        data: &OfferData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let OfferRequest::Receive { mime_type, fd } = request {
            if let Some(src) = data.source.lock_or_recover().clone() {
                src.send(mime_type, fd.as_fd());
            }
        }
    }
}

/// Wysyła bieżącą selekcję pierwotną klientowi okna `slot`.
///
/// Musi być wywołane bezpośrednio przed `wl_keyboard.enter` (patrz
/// [`crate::data::send_selection_for_slot`]).
pub fn send_selection_for_slot(state: &mut State, slot: usize) {
    if state
        .primary_selection
        .as_ref()
        .is_some_and(|source| !source.is_alive())
    {
        state.primary_selection = None;
    }
    let Some(device) = target_device(state, slot) else {
        return;
    };
    let Some(handle) = state.display_handle.clone() else {
        return;
    };
    send_selection(&handle, &device, state.primary_selection.as_ref());
}

/// Urządzenie selekcji pierwotnej klienta będącego właścicielem okna `slot`.
fn target_device(state: &State, slot: usize) -> Option<ZwpPrimarySelectionDeviceV1> {
    let surface = state.slots.get(slot)?.clone();
    let client_id = surface.client()?.id();
    state
        .primary_devices
        .iter()
        .find(|d| d.client().map(|c| c.id() == client_id).unwrap_or(false))
        .cloned()
}

pub(crate) fn broadcast_selection(
    state: &State,
    handle: &DisplayHandle,
    source: Option<&SelectionSource>,
) {
    for device in state.primary_devices.clone() {
        send_selection(handle, &device, source);
    }
}

/// `data_offer` + typy MIME + `selection` do jednego urządzenia.
fn send_selection(
    handle: &DisplayHandle,
    device: &ZwpPrimarySelectionDeviceV1,
    source: Option<&SelectionSource>,
) {
    match source {
        Some(src) => {
            let Some(client) = device.client() else {
                return;
            };
            let mimes = src.mimes();
            let offer_data = OfferData {
                source: Mutex::new(Some(src.clone())),
            };
            let Ok(offer) = client.create_resource::<ZwpPrimarySelectionOfferV1, OfferData, State>(
                handle, 1, offer_data,
            ) else {
                return;
            };
            let _ = device.send_event(DeviceEvent::DataOffer {
                offer: offer.clone(),
            });
            for mime in mimes {
                let _ = offer.send_event(OfferEvent::Offer { mime_type: mime });
            }
            let _ = device.send_event(DeviceEvent::Selection { id: Some(offer) });
        }
        None => {
            let _ = device.send_event(DeviceEvent::Selection { id: None });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::data::MAX_MIME_TYPES;

    #[test]
    fn mime_limit_is_shared_with_normal_clipboard() {
        assert_eq!(MAX_MIME_TYPES, 64);
    }
}
