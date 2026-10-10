//! `wl_data_device_manager` — schowek (selekcja) między klientami.
//!
//! Zakres MVP: **kopiuj/wklej** (selekcja). Drag & drop oraz selekcja pierwotna
//! (`primary selection`) — później.
//!
//! Przepływ: klient-źródło tworzy `wl_data_source`, oferuje typy MIME i wywołuje
//! `wl_data_device.set_selection`. Kompozytor rozsyła pozostałym klientom `wl_data_offer`
//! (`data_offer` → `offer` typów → `selection`). Gdy odbiorca zażąda danych
//! (`wl_data_offer.receive`), przekazujemy żądanie do `wl_data_source.send`.

use std::os::fd::AsFd;
use std::sync::Mutex;

use wayland_server::protocol::wl_data_device::{
    Event as DeviceEvent, Request as DeviceRequest, WlDataDevice,
};
use wayland_server::protocol::wl_data_device_manager::{
    Request as ManagerRequest, WlDataDeviceManager,
};
use wayland_server::protocol::wl_data_offer::{
    Event as OfferEvent, Request as OfferRequest, WlDataOffer,
};
use wayland_server::protocol::wl_data_source::{
    Event as SourceEvent, Request as SourceRequest, WlDataSource,
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;

/// Dane `wl_data_source`: oferowane typy MIME.
#[derive(Default)]
pub struct DataSourceData {
    pub mimes: Mutex<Vec<String>>,
}

/// Dane `wl_data_offer`: źródło, z którego oferta czerpie dane.
pub struct DataOfferData {
    pub source: Mutex<Option<WlDataSource>>,
}

/// Dane `wl_data_device` (na razie puste — urządzenie jest per-seat).
#[derive(Default)]
pub struct DataDeviceData {}

impl GlobalDispatch<WlDataDeviceManager, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WlDataDeviceManager>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &WlDataDeviceManager,
        request: ManagerRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ManagerRequest::CreateDataSource { id } => {
                data_init.init(id, DataSourceData::default());
            }
            ManagerRequest::GetDataDevice { id, seat: _ } => {
                let device = data_init.init(id, DataDeviceData::default());
                state.data_devices.push(device);
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, DataSourceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &WlDataSource,
        request: SourceRequest,
        data: &DataSourceData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            SourceRequest::Offer { mime_type } => {
                data.mimes.lock().unwrap().push(mime_type);
            }
            SourceRequest::Destroy => {
                // Jeśli to była bieżąca selekcja — wyczyść.
                if let Some(cur) = state.selection.as_ref() {
                    if cur.id() == resource.id() {
                        state.selection = None;
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDevice, DataDeviceData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &WlDataDevice,
        request: DeviceRequest,
        _data: &DataDeviceData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            DeviceRequest::SetSelection {
                source,
                serial: _serial,
            } => {
                state.selection = source.clone();
                broadcast_selection(state, handle, resource, source.as_ref());
            }
            DeviceRequest::Release => {
                state.data_devices.retain(|d| d.id() != resource.id());
            }
            // Drag & drop poza zakresem MVP.
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, DataOfferData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &WlDataOffer,
        request: OfferRequest,
        data: &DataOfferData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let OfferRequest::Receive { mime_type, fd } = request {
            // Przekaż żądanie danych do źródła.
            if let Some(src) = data.source.lock().unwrap().clone() {
                let _ = src.send_event(SourceEvent::Send {
                    mime_type,
                    fd: fd.as_fd(),
                });
            }
        }
    }
}

/// Rozsyła bieżącą selekcję (`data_offer` + `offer` MIME + `selection`) do urządzeń klientów.
fn broadcast_selection(
    state: &State,
    handle: &DisplayHandle,
    requester: &WlDataDevice,
    source: Option<&WlDataSource>,
) {
    for device in state.data_devices.clone() {
        // Pomijamy urządzenie, które ustawiło selekcję (jego klient nie potrzebuje oferty).
        if device.id() == requester.id() {
            continue;
        }
        let Some(client) = device.client() else {
            continue;
        };
        match source {
            Some(src) => {
                let mimes = src
                    .data::<DataSourceData>()
                    .map(|d| d.mimes.lock().unwrap().clone())
                    .unwrap_or_default();
                let offer_data = DataOfferData {
                    source: Mutex::new(Some(src.clone())),
                };
                let Ok(offer) = client
                    .create_resource::<WlDataOffer, DataOfferData, State>(handle, 3, offer_data)
                else {
                    continue;
                };
                let _ = device.send_event(DeviceEvent::DataOffer { id: offer.clone() });
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
}
