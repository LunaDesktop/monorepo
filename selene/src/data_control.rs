//! `ext_data_control_v1` — menedżer schowka (historia, przetrwanie zamknięcia źródła).
//!
//! Menedżer (np. clipse, wl-clip-persist) dostaje osobne urządzenie danych.
//! Oferty i źródła tego protokołu są innymi interfejsami niż `wl_data_device`,
//! więc kompozytor tłumaczy je na wspólny [`SelectionSource`]:
//!
//! ```text
//! przeglądarka (wl_data_source) --+
//!                                  +--> SelectionSource --> zwykli klienci (wl_data_offer)
//! menedżer (ext_data_control_source_v1) --+                --> menedżer (ext_data_control_offer_v1)
//! ```
//!
//! Dzięki temu menedżer widzi każdą kopię, może ją zapisać, a po zamknięciu
//! aplikacji-źródła wystawić zapisaną selekcję z powrotem wszystkim klientom.
//! Obsługiwana jest też selekcja pierwotna.

use std::os::fd::AsFd;
use std::sync::Mutex;

use wayland_protocols::ext::data_control::v1::server::{
    ext_data_control_device_v1::{
        Event as DeviceEvent, ExtDataControlDeviceV1, Request as DeviceRequest,
    },
    ext_data_control_manager_v1::{ExtDataControlManagerV1, Request as ManagerRequest},
    ext_data_control_offer_v1::{
        Event as OfferEvent, ExtDataControlOfferV1, Request as OfferRequest,
    },
    ext_data_control_source_v1::{ExtDataControlSourceV1, Request as SourceRequest},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::data::{MimeData, SelectionSource};
use crate::state::State;
use crate::sync::LockExt;

#[derive(Default)]
pub struct ControlDeviceData {}

/// Oferta przekazana menedżerowi — wskazuje wspólne źródło danych.
pub struct ControlOfferData {
    pub source: Mutex<Option<SelectionSource>>,
}

impl GlobalDispatch<ExtDataControlManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExtDataControlManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ExtDataControlManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ExtDataControlManagerV1,
        request: ManagerRequest,
        _data: &(),
        handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ManagerRequest::CreateDataSource { id } => {
                data_init.init(id, MimeData::default());
            }
            ManagerRequest::GetDataDevice { id, seat: _ } => {
                let device = data_init.init(id, ControlDeviceData::default());
                state.control_devices.retain(|d| d.is_alive());
                state.control_devices.push(device.clone());
                send_selection(handle, &device, state.selection.as_ref());
                send_primary(handle, &device, state.primary_selection.as_ref());
            }
            ManagerRequest::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlSourceV1, MimeData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ExtDataControlSourceV1,
        request: SourceRequest,
        data: &MimeData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            SourceRequest::Offer { mime_type } => data.add(mime_type),
            SourceRequest::Destroy => {
                let source = SelectionSource::Control(resource.clone());
                let selection_current = state
                    .selection
                    .as_ref()
                    .is_some_and(|current| current.id() == source.id());
                if selection_current {
                    state.selection = None;
                    crate::data::note_wayland_selection_set();
                    crate::data::broadcast_selection(state, handle, None);
                    broadcast_selection(state, handle, None);
                    crate::data::clear_x11_clipboard();
                }
                let primary_current = state
                    .primary_selection
                    .as_ref()
                    .is_some_and(|current| current.id() == source.id());
                if primary_current {
                    state.primary_selection = None;
                    crate::primary_selection::broadcast_selection(state, handle, None);
                    broadcast_primary_selection(state, handle, None);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ControlDeviceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ExtDataControlDeviceV1,
        request: DeviceRequest,
        _data: &ControlDeviceData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            DeviceRequest::SetSelection { source } => {
                let new = source.clone().map(SelectionSource::Control);
                if let Some(old) = state.selection.as_ref() {
                    let same = new.as_ref().is_some_and(|new| new.id() == old.id());
                    if !same {
                        old.cancel();
                    }
                }
                state.selection = new;
                crate::data::note_wayland_selection_set();
                let current = state.selection.clone();
                crate::data::broadcast_selection(state, handle, current.as_ref());
                broadcast_selection(state, handle, current.as_ref());
                // Menedżer schowka też musi zasilać most do X11.
                crate::data::cache_selection_for_x11(current.as_ref());
            }
            DeviceRequest::SetPrimarySelection { source } => {
                let new = source.clone().map(SelectionSource::Control);
                if let Some(old) = state.primary_selection.as_ref() {
                    let same = new.as_ref().is_some_and(|new| new.id() == old.id());
                    if !same {
                        old.cancel();
                    }
                }
                state.primary_selection = new;
                let current = state.primary_selection.clone();
                crate::primary_selection::broadcast_selection(state, handle, current.as_ref());
                broadcast_primary_selection(state, handle, current.as_ref());
            }
            DeviceRequest::Destroy => {
                state.control_devices.retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtDataControlOfferV1, ControlOfferData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &ExtDataControlOfferV1,
        request: OfferRequest,
        data: &ControlOfferData,
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

/// Rozsyła bieżącą selekcję do wszystkich menedżerów schowka.
pub(crate) fn broadcast_selection(
    state: &State,
    handle: &DisplayHandle,
    source: Option<&SelectionSource>,
) {
    for device in state.control_devices.clone() {
        send_selection(handle, &device, source);
    }
}

/// Rozsyła bieżącą selekcję pierwotną do wszystkich menedżerów schowka.
pub(crate) fn broadcast_primary_selection(
    state: &State,
    handle: &DisplayHandle,
    source: Option<&SelectionSource>,
) {
    for device in state.control_devices.clone() {
        send_primary(handle, &device, source);
    }
}

fn send_selection(
    handle: &DisplayHandle,
    device: &ExtDataControlDeviceV1,
    source: Option<&SelectionSource>,
) {
    match source {
        Some(src) => {
            let Some(offer) = make_offer(handle, device, src) else {
                return;
            };
            let _ = device.send_event(DeviceEvent::Selection { id: Some(offer) });
        }
        None => {
            let _ = device.send_event(DeviceEvent::Selection { id: None });
        }
    }
}

fn send_primary(
    handle: &DisplayHandle,
    device: &ExtDataControlDeviceV1,
    source: Option<&SelectionSource>,
) {
    match source {
        Some(src) => {
            let Some(offer) = make_offer(handle, device, src) else {
                return;
            };
            let _ = device.send_event(DeviceEvent::PrimarySelection { id: Some(offer) });
        }
        None => {
            let _ = device.send_event(DeviceEvent::PrimarySelection { id: None });
        }
    }
}

fn make_offer(
    handle: &DisplayHandle,
    device: &ExtDataControlDeviceV1,
    src: &SelectionSource,
) -> Option<ExtDataControlOfferV1> {
    let client = device.client()?;
    let offer_data = ControlOfferData {
        source: Mutex::new(Some(src.clone())),
    };
    let offer = client
        .create_resource::<ExtDataControlOfferV1, ControlOfferData, State>(handle, 1, offer_data)
        .ok()?;
    let _ = device.send_event(DeviceEvent::DataOffer { id: offer.clone() });
    for mime in src.mimes() {
        let _ = offer.send_event(OfferEvent::Offer { mime_type: mime });
    }
    Some(offer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MAX_MIME_TYPES;

    #[test]
    fn control_source_uses_same_mime_limit() {
        let data = MimeData::default();
        for i in 0..(MAX_MIME_TYPES + 3) {
            data.add(format!("text/plain;{i}"));
        }
        assert_eq!(data.snapshot().len(), MAX_MIME_TYPES);
    }
}
