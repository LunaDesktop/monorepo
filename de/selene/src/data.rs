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
/// Stan trwającego przeciągania (drag & drop).
#[derive(Default)]
pub struct DndState {
    /// Źródło danych przeciągania (jeśli klient je podał).
    pub source: Option<WlDataSource>,
    /// Okno docelowe pod kursorem.
    pub slot: Option<usize>,
    /// Oferta przekazana bieżącemu celowi.
    pub offer: Option<WlDataOffer>,
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
            DeviceRequest::StartDrag {
                source,
                origin: _,
                icon: _,
                serial: _,
            } => {
                // Zaczynamy przeciąganie; cel wybierzemy przy ruchu wskaźnika.
                state.dnd = Some(DndState {
                    source,
                    ..DndState::default()
                });
            }
            DeviceRequest::Release => {
                state.data_devices.retain(|d| d.id() != resource.id());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataOffer, DataOfferData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &WlDataOffer,
        request: OfferRequest,
        data: &DataOfferData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            OfferRequest::Receive { mime_type, fd } => {
                // Przekaż żądanie danych do źródła.
                if let Some(src) = data.source.lock().unwrap().clone() {
                    let _ = src.send_event(SourceEvent::Send {
                        mime_type,
                        fd: fd.as_fd(),
                    });
                }
            }
            OfferRequest::Finish => {
                // Cel zakończył transfer — domknij przeciąganie (jeśli to jego oferta).
                let is_dnd = state
                    .dnd
                    .as_ref()
                    .and_then(|d| d.offer.as_ref())
                    .map(|o| o.id() == resource.id())
                    .unwrap_or(false);
                if is_dnd {
                    finish_dnd(state);
                }
            }
            _ => {}
        }
    }
}

/// Ruch wskaźnika w trakcie przeciągania: aktualizuje cel (`enter`/`leave`/`motion`).
pub fn dnd_motion(state: &mut State, handle: &DisplayHandle, pos: (f64, f64)) {
    if state.dnd.is_none() {
        return;
    }
    let target = crate::seat::window_at(state, pos);
    let changed = state.dnd.as_ref().and_then(|d| d.slot) != target;
    if !changed {
        if let Some(slot) = target {
            send_motion(state, slot, pos);
        }
        return;
    }
    // Wyjście z poprzedniego celu.
    let previous = state.dnd.as_mut().and_then(|d| {
        d.offer = None;
        d.slot
    });
    if let Some(prev) = previous {
        if let Some(dev) = target_device(state, prev) {
            let _ = dev.send_event(DeviceEvent::Leave);
        }
    }
    // Wejście do nowego celu.
    if let Some(dnd) = state.dnd.as_mut() {
        dnd.slot = target;
    }
    if let Some(slot) = target {
        enter_target(state, handle, slot, pos);
    }
}

/// Upuszczenie w trakcie przeciągania: `drop` do celu, powiadomienie źródła.
pub fn dnd_drop(state: &mut State, _handle: &DisplayHandle) {
    let Some(dnd) = state.dnd.take() else {
        return;
    };
    match dnd.slot {
        Some(slot) => {
            if let Some(dev) = target_device(state, slot) {
                let _ = dev.send_event(DeviceEvent::Drop);
            }
            // Źródło dowiaduje się o wykonaniu (bez negocjacji akcji).
            if let Some(src) = dnd.source {
                let _ = src.send_event(SourceEvent::DndDropPerformed);
            }
        }
        None => {
            // Upuszczenie poza celem = anulowanie.
            if let Some(src) = dnd.source {
                let _ = src.send_event(SourceEvent::Cancelled);
            }
        }
    }
}

fn finish_dnd(state: &mut State) {
    if let Some(dnd) = state.dnd.take() {
        if let Some(src) = dnd.source {
            let _ = src.send_event(SourceEvent::DndFinished);
        }
    }
}

/// Urządzenie schowka klienta będącego właścicielem powierzchni okna `slot`.
fn target_device(state: &State, slot: usize) -> Option<WlDataDevice> {
    let surface = state.slots.get(slot)?.clone();
    let client_id = surface.client()?.id();
    state
        .data_devices
        .iter()
        .find(|d| d.client().map(|c| c.id() == client_id).unwrap_or(false))
        .cloned()
}

/// Wysyła `motion` do bieżącego celu.
fn send_motion(state: &State, slot: usize, pos: (f64, f64)) {
    let Some(dev) = target_device(state, slot) else {
        return;
    };
    let (lx, ly) = local_coords(state, slot, pos);
    let _ = dev.send_event(DeviceEvent::Motion {
        time: 0,
        x: lx,
        y: ly,
    });
}

/// Tworzy ofertę i wysyła `data_offer` + `enter` nowemu celowi.
fn enter_target(state: &mut State, handle: &DisplayHandle, slot: usize, pos: (f64, f64)) {
    let Some(dev) = target_device(state, slot) else {
        return;
    };
    let Some(client) = dev.client() else { return };
    let Some(surface) = state.slots.get(slot).cloned() else {
        return;
    };
    let source = state.dnd.as_ref().and_then(|d| d.source.clone());
    let mimes = source
        .as_ref()
        .and_then(|s| s.data::<DataSourceData>())
        .map(|d| d.mimes.lock().unwrap().clone())
        .unwrap_or_default();
    let offer_data = DataOfferData {
        source: Mutex::new(source),
    };
    let Ok(offer) =
        client.create_resource::<WlDataOffer, DataOfferData, State>(handle, 3, offer_data)
    else {
        return;
    };
    let _ = dev.send_event(DeviceEvent::DataOffer { id: offer.clone() });
    for mime in mimes {
        let _ = offer.send_event(OfferEvent::Offer { mime_type: mime });
    }
    let serial = state.next_serial();
    let (lx, ly) = local_coords(state, slot, pos);
    let _ = dev.send_event(DeviceEvent::Enter {
        serial,
        surface,
        x: lx,
        y: ly,
        id: Some(offer.clone()),
    });
    if let Some(dnd) = state.dnd.as_mut() {
        dnd.offer = Some(offer);
    }
}

/// Współrzędne powierzchniowe (lokalne dla treści okna `slot`).
fn local_coords(state: &State, slot: usize, pos: (f64, f64)) -> (f64, f64) {
    match state.windows.lock().unwrap().get(slot) {
        Some(w) => {
            let c = w.content();
            (pos.0 - c.x(), pos.1 - c.y())
        }
        None => pos,
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
