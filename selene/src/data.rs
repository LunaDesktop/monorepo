//! `wl_data_device_manager` — schowek (selekcja) między klientami.
//!
//! Przepływ: klient-źródło tworzy `wl_data_source`, oferuje typy MIME i wywołuje
//! `wl_data_device.set_selection`. Kompozytor rozsyła `wl_data_offer` do **wszystkich**
//! klientów (także źródła — dzięki temu „kopiuj → wklej” w tej samej aplikacji działa;
//! `data_offer` → `offer` typów → `selection`). Gdy odbiorca zażąda danych
//! (`wl_data_offer.receive`), przekazujemy żądanie do `wl_data_source.send`.
//!
//! Od `ext_data_control_v1` schowek ma dwa rodzaje źródeł: zwykłe `wl_data_source`
//! i źródła menedżera schowka. [`SelectionSource`] ujednolica je, więc oferty dla
//! obu światów mogą wskazywać to samo źródło.

use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wayland_protocols::ext::data_control::v1::server::ext_data_control_source_v1::{
    Event as ControlSourceEvent, ExtDataControlSourceV1,
};
use wayland_protocols::wp::primary_selection::zv1::server::zwp_primary_selection_source_v1::{
    Event as PrimarySourceEvent, ZwpPrimarySelectionSourceV1,
};
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
use crate::sync::LockExt;

/// Górny limit typów MIME oferowanych przez jedno źródło (ochrona przed OOM).
pub const MAX_MIME_TYPES: usize = 64;

/// Górny rozmiar tekstu kopiowanego do mostka X11 (ochrona przed OOM).
const MAX_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;

/// Numer bieżącej selekcji dla mostka X11 — zapobiega wyścigowi, gdy klient
/// ustawia selekcję kilka razy pod rząd (Chrome robi to przy każdym kopiowaniu).
static CLIPBOARD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Szczegółowe logi schowka: `SELENE_DEBUG_CLIPBOARD` albo zwykła diagnostyka sesji.
/// `set_selection`/`get_data_device` zdarzają się rzadko (kopiowanie, start klienta),
/// więc w trwałym logu sesji nie zaśmiecają, a pozwalają ustalić, czy Xwayland
/// w ogóle uczestniczy w moście schowka X11 ↔ Wayland.
fn clipboard_verbose() -> bool {
    debug_clipboard() || std::env::var_os("SELENE_STATS").is_some()
}

fn debug_clipboard() -> bool {
    std::env::var_os("SELENE_DEBUG_CLIPBOARD").is_some()
}

/// Typy MIME oferowane przez źródło — wspólne dla `wl_data_source`,
/// selekcji pierwotnej i `ext_data_control_source_v1`.
#[derive(Default)]
pub struct MimeData {
    pub mimes: Mutex<Vec<String>>,
}

impl MimeData {
    pub fn add(&self, mime: String) {
        let mut mimes = self.mimes.lock_or_recover();
        if mimes.len() < MAX_MIME_TYPES {
            mimes.push(mime);
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.mimes.lock_or_recover().clone()
    }
}

/// Źródło danych: zwykły schowek Waylanda, selekcja pierwotna albo menedżer
/// schowka (`ext_data_control_v1`). Pozwala jednej ofercie obsłużyć oba światy.
#[derive(Clone)]
pub enum SelectionSource {
    Wayland(WlDataSource),
    Primary(ZwpPrimarySelectionSourceV1),
    Control(ExtDataControlSourceV1),
    /// Schowek przejęty z X11 (tekst już pobrany przez XWM).
    X11Cached(Arc<crate::xwm::X11Clipboard>),
}

impl SelectionSource {
    pub fn id(&self) -> wayland_server::backend::ObjectId {
        match self {
            SelectionSource::Wayland(s) => s.id(),
            SelectionSource::Primary(s) => s.id(),
            SelectionSource::Control(s) => s.id(),
            // X11 nie ma obiektu Waylanda — `null` nie koliduje z żadnym
            // zasobem klienta i nigdy nie jest porównywany w `clear_if_current`.
            SelectionSource::X11Cached(_) => wayland_server::backend::ObjectId::null(),
        }
    }

    pub fn is_alive(&self) -> bool {
        match self {
            SelectionSource::Wayland(s) => s.is_alive(),
            SelectionSource::Primary(s) => s.is_alive(),
            SelectionSource::Control(s) => s.is_alive(),
            SelectionSource::X11Cached(_) => true,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            SelectionSource::Wayland(_) => "wayland",
            SelectionSource::Primary(_) => "primary",
            SelectionSource::Control(_) => "control",
            SelectionSource::X11Cached(_) => "x11",
        }
    }

    pub fn mimes(&self) -> Vec<String> {
        match self {
            SelectionSource::Wayland(s) => data_mimes(s.data::<MimeData>()),
            SelectionSource::Primary(s) => data_mimes(s.data::<MimeData>()),
            SelectionSource::Control(s) => data_mimes(s.data::<MimeData>()),
            SelectionSource::X11Cached(clip) => clip.mimes.clone(),
        }
    }

    /// Prosi źródło o dane. `fd` jest duplikowany na poziomie protokołu.
    /// Zwraca `false`, gdy źródło już nie istnieje (np. klient się rozłączył).
    pub fn send(&self, mime_type: String, fd: BorrowedFd<'_>) -> bool {
        match self {
            SelectionSource::Wayland(s) => {
                s.send_event(SourceEvent::Send { mime_type, fd }).is_ok()
            }
            SelectionSource::Primary(s) => s
                .send_event(PrimarySourceEvent::Send { mime_type, fd })
                .is_ok(),
            SelectionSource::Control(s) => s
                .send_event(ControlSourceEvent::Send { mime_type, fd })
                .is_ok(),
            SelectionSource::X11Cached(clip) => {
                // Dane są już w pamięci — zapisujemy je do rury od razu.
                let mut rest = clip.data.as_slice();
                while !rest.is_empty() {
                    match rustix::io::write(fd, rest) {
                        Ok(0) => return false,
                        Ok(written) => rest = &rest[written..],
                        Err(rustix::io::Errno::INTR) => continue,
                        Err(_) => return false,
                    }
                }
                true
            }
        }
    }

    /// Powiadamia źródło, że przestało być właścicielem selekcji.
    pub fn cancel(&self) {
        match self {
            SelectionSource::Wayland(s) => {
                let _ = s.send_event(SourceEvent::Cancelled);
            }
            SelectionSource::Primary(s) => {
                let _ = s.send_event(PrimarySourceEvent::Cancelled);
            }
            SelectionSource::Control(s) => {
                let _ = s.send_event(ControlSourceEvent::Cancelled);
            }
            // Kopia X11 nie ma właściciela po stronie Waylanda.
            SelectionSource::X11Cached(_) => {}
        }
    }
}

fn data_mimes(data: Option<&MimeData>) -> Vec<String> {
    data.map(MimeData::snapshot).unwrap_or_default()
}

/// Dane `wl_data_offer`: źródło, z którego oferta czerpie dane.
pub struct DataOfferData {
    pub source: Mutex<Option<SelectionSource>>,
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
        handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ManagerRequest::CreateDataSource { id } => {
                data_init.init(id, MimeData::default());
            }
            ManagerRequest::GetDataDevice { id, seat: _ } => {
                let device = data_init.init(id, DataDeviceData::default());
                state.data_devices.push(device.clone());
                if clipboard_verbose() {
                    eprintln!(
                        "clipboard: get_data_device id={} selection={} devices={}",
                        device.id().protocol_id(),
                        state.selection.is_some(),
                        state.data_devices.len()
                    );
                }
                // Nowy klient powinien od razu znać bieżącą selekcję (np. uruchomiony
                // później terminal ma móc wkleić to, co skopiowano wcześniej).
                send_selection(handle, &device, state.selection.as_ref());
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataSource, MimeData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &WlDataSource,
        request: SourceRequest,
        data: &MimeData,
        handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            SourceRequest::Offer { mime_type } => {
                if debug_clipboard() {
                    eprintln!("clipboard: source offers {mime_type}");
                }
                data.add(mime_type);
            }
            SourceRequest::Destroy => {
                // Jeśli to była bieżąca selekcja — wyczyść i powiadom klientów,
                // żeby nie trzymali oferty wskazującej na martwe źródło.
                let source = SelectionSource::Wayland(resource.clone());
                clear_if_current(state, handle, &source);
            }
            _ => {}
        }
    }
}

/// Czyści selekcję, jeśli `source` wciąż jest jej właścicielem, i rozsyła `None`
/// do zwykłych klientów oraz menedżerów schowka.
pub(crate) fn clear_if_current(
    state: &mut State,
    handle: &DisplayHandle,
    source: &SelectionSource,
) {
    if state
        .selection
        .as_ref()
        .is_some_and(|current| current.id() == source.id())
    {
        state.selection = None;
        note_wayland_selection_set();
        broadcast_selection(state, handle, None);
        crate::data_control::broadcast_selection(state, handle, None);
        clear_x11_clipboard();
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
                // Stare źródło przestaje być właścicielem schowka — protokół wymaga
                // wysłania mu `cancelled`, inaczej klient dalej myśli, że jest źródłem.
                let new = source.clone().map(SelectionSource::Wayland);
                if clipboard_verbose() {
                    eprintln!(
                        "clipboard: set_selection kind={} mimes={:?} source={:?} devices={}",
                        new.as_ref().map(|s| s.kind()).unwrap_or("none"),
                        new.as_ref().map(|s| s.mimes()),
                        new.as_ref().map(|s| s.id().protocol_id()),
                        state.data_devices.len()
                    );
                }
                if let Some(old) = state.selection.as_ref() {
                    let same = new.as_ref().is_some_and(|new| new.id() == old.id());
                    if !same {
                        old.cancel();
                    }
                }
                state.selection = new;
                note_wayland_selection_set();
                let current = state.selection.clone();
                broadcast_selection(state, handle, current.as_ref());
                crate::data_control::broadcast_selection(state, handle, current.as_ref());
                // Most do X11 (Zed i inne aplikacje X11 nie widzą inaczej schowka).
                cache_selection_for_x11(current.as_ref());
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
            OfferRequest::Accept {
                serial: _,
                mime_type,
            } => {
                // `accept` to informacja zwrotna odbiorcy. Przekazujemy ją
                // źródłu jako `wl_data_source.target` — Xwayland (i część
                // narzędzi CLI) czeka na nią, zanim odpowie na `receive`.
                if let Some(src) = data.source.lock_or_recover().clone() {
                    if let SelectionSource::Wayland(source) = &src {
                        let _ = source.send_event(SourceEvent::Target { mime_type });
                    }
                }
            }
            OfferRequest::Receive { mime_type, fd } => {
                if debug_clipboard() {
                    eprintln!("clipboard: receive mime={mime_type}");
                }
                // Przekaż żądanie danych do źródła. `target` przed `send` jest
                // bezpiecznym uzupełnieniem dla źródeł, które nie dostały
                // wcześniej `accept` (np. wklejanie bez świeżego serialu).
                if let Some(src) = data.source.lock_or_recover().clone() {
                    if let SelectionSource::Wayland(source) = &src {
                        let _ = source.send_event(SourceEvent::Target {
                            mime_type: Some(mime_type.clone()),
                        });
                    }
                    src.send(mime_type, fd.as_fd());
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
            // `dnd_drop_performed`/`dnd_finished` istnieją dopiero od v3 — klient,
            // który związał `wl_data_device_manager` w wersji 1/2, przerwie się
            // w libwayland (brak funkcji listenera dla opcode 3/4), więc bramkujemy.
            if let Some(src) = dnd.source {
                if src.version() >= 3 {
                    let _ = src.send_event(SourceEvent::DndDropPerformed);
                }
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
            // Zdarzenie v3 — patrz `dnd_drop`.
            if src.version() >= 3 {
                let _ = src.send_event(SourceEvent::DndFinished);
            }
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
        .and_then(|s| s.data::<MimeData>())
        .map(MimeData::snapshot)
        .unwrap_or_default();
    let offer_data = DataOfferData {
        source: Mutex::new(source.map(SelectionSource::Wayland)),
    };
    let offer_version = dev.version().min(3);
    let Ok(offer) = client.create_resource::<WlDataOffer, DataOfferData, State>(
        handle,
        offer_version,
        offer_data,
    ) else {
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
    match state.windows.lock_or_recover().get(slot) {
        Some(w) => {
            let c = w.content();
            (pos.0 - c.x(), pos.1 - c.y())
        }
        None => pos,
    }
}

/// Wysyła bieżącą selekcję do klienta, do którego należy okno `slot`.
///
/// Protokół wymaga, aby `wl_data_device.selection` dotarło **bezpośrednio
/// przed** `wl_keyboard.enter`: klienci (GTK, Xwayland) unieważniają ofertę po
/// utracie fokusu. Wysyłanie selekcji wyłącznie przy jej zmianie powodowało, że
/// po przełączeniu okna wklejanie nie miało ważnej oferty — działało tylko
/// wewnątrz jednej aplikacji i między aplikacjami libLuny, które trzymają
/// ofertę niezależnie od fokusu.
pub fn send_selection_for_slot(state: &mut State, slot: usize) {
    // Martwe źródło (klient zakończył się bez `destroy`) nie może być oferowane.
    if state
        .selection
        .as_ref()
        .is_some_and(|source| !source.is_alive())
    {
        state.selection = None;
    }
    let Some(device) = target_device(state, slot) else {
        return;
    };
    let Some(handle) = state.display_handle.clone() else {
        return;
    };
    send_selection(&handle, &device, state.selection.as_ref());
}

/// Rozsyła bieżącą selekcję do **wszystkich** urządzeń klientów.
///
/// Nie pomijamy źródła: Wayland pozwala mu otrzymać własną ofertę, a libLuna
/// potrzebuje jej do wewnętrznego „kopiuj → wklej” w tym samym oknie.
pub(crate) fn broadcast_selection(
    state: &State,
    handle: &DisplayHandle,
    source: Option<&SelectionSource>,
) {
    for device in state.data_devices.clone() {
        send_selection(handle, &device, source);
    }
}

/// Pobranie tekstu od źródła Wayland dla mostka X11 — w toku.
struct PendingFetch {
    source: SelectionSource,
    mime: String,
    generation: u64,
    /// Koniec odczytu rury; `None` dopóki nie wywołamy `wl_data_source.send`.
    read: Option<OwnedFd>,
    out: Vec<u8>,
    deadline: Instant,
}

/// Bieżące pobranie. `Mutex` zamiast kanału: `poll` dostaje deskryptor rury,
/// więc nie potrzebujemy budzić wątku protokołu osobnym gniazdem.
static PENDING: Mutex<Option<PendingFetch>> = Mutex::new(None);

/// Most **Wayland → X11**: przygotowuje pobranie tekstu od źródła.
///
/// Xwayland nie implementuje `wl_data_device` — most schowka należy do
/// kompozytora (tak robią KWin i Mutter). Samo pobranie wykonuje
/// [`pump_clipboard`] w pętli głównej: `wl_data_source.send` musi iść z wątku
/// protokołu, a rura jest dołączona do `poll`.
pub(crate) fn cache_selection_for_x11(source: Option<&SelectionSource>) {
    let generation = CLIPBOARD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(source) = source.cloned() else {
        *PENDING.lock_or_recover() = None;
        if let Some(xwm) = crate::xwm::global() {
            xwm.clear_clipboard();
        }
        return;
    };
    // Schowek przejęty z X11 nie może być odsyłany z powrotem do X11 (pętla).
    if matches!(source, SelectionSource::X11Cached(_)) {
        return;
    }
    let mimes = source.mimes();
    let Some(mime) = choose_text_mime(&mimes) else {
        *PENDING.lock_or_recover() = None;
        if let Some(xwm) = crate::xwm::global() {
            xwm.clear_clipboard();
        }
        return;
    };
    *PENDING.lock_or_recover() = Some(PendingFetch {
        source,
        mime,
        generation,
        read: None,
        out: Vec::new(),
        deadline: Instant::now() + Duration::from_millis(1500),
    });
}

/// Wybiera jeden tekstowy MIME — jedno pobranie obsłuży potem UTF8_STRING,
/// STRING, TEXT i `text/plain`.
fn choose_text_mime(mimes: &[String]) -> Option<String> {
    const PREFERRED: [&str; 5] = [
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "TEXT",
        "STRING",
    ];
    PREFERRED
        .iter()
        .find(|wanted| mimes.iter().any(|m| m.eq_ignore_ascii_case(wanted)))
        .map(|mime| (*mime).to_owned())
}

/// Czyści schowek X11 (Wayland nie ma już selekcji albo brak danych tekstowych).
pub(crate) fn clear_x11_clipboard() {
    CLIPBOARD_GENERATION.fetch_add(1, Ordering::SeqCst);
    *PENDING.lock_or_recover() = None;
    if let Some(xwm) = crate::xwm::global() {
        xwm.clear_clipboard();
    }
}

/// Czas (ms) ostatniego ustawienia selekcji **przez Wayland** — starsza migawka
/// X11 nie może jej nadpisać.
static WAYLAND_SELECTION_AT: AtomicU64 = AtomicU64::new(0);
/// Generacja ostatniej zastosowanej migawki X11 (zapobiega wielokrotnemu rozsyłaniu).
static LAST_APPLIED_X11: AtomicU64 = AtomicU64::new(0);

/// Wołane przy każdym `set_selection`/wyczyszczeniu schowka przez klienta Wayland.
pub(crate) fn note_wayland_selection_set() {
    WAYLAND_SELECTION_AT.store(crate::xwm::now_ms(), Ordering::SeqCst);
}

/// Most **X11 → Wayland**: jeśli XWM opublikował nowszą migawkę tekstu z X11,
/// ustawia ją jako selekcję Waylanda i rozsyła oferty klientom.
///
/// Wołane raz na iterację pętli głównej. Publikuje tylko raz na generację.
pub(crate) fn sync_x11_selection(state: &mut State, handle: &DisplayHandle) {
    let Some(clip) = crate::xwm::x11_clipboard() else {
        return;
    };
    if clip.generation == LAST_APPLIED_X11.load(Ordering::SeqCst) {
        return;
    }
    if clip.created_at <= WAYLAND_SELECTION_AT.load(Ordering::SeqCst) {
        // Nowsza selekcja Waylanda — ignorujemy starszą migawkę X11.
        LAST_APPLIED_X11.store(clip.generation, Ordering::SeqCst);
        return;
    }
    LAST_APPLIED_X11.store(clip.generation, Ordering::SeqCst);
    if clip.mimes.is_empty() {
        // Właściciel X11 zniknął — czyścimy tylko źródło pochodzące z X11.
        if matches!(state.selection, Some(SelectionSource::X11Cached(_))) {
            state.selection = None;
            broadcast_selection(state, handle, None);
            crate::data_control::broadcast_selection(state, handle, None);
        }
        return;
    }
    let len = clip.data.len();
    let source = SelectionSource::X11Cached(clip);
    state.selection = Some(source.clone());
    broadcast_selection(state, handle, Some(&source));
    crate::data_control::broadcast_selection(state, handle, Some(&source));
    if clipboard_verbose() {
        eprintln!("clipboard: X11 → Wayland ({len} B)");
    }
}

/// Deskryptor rury z danymi od źródła — pętla główna dodaje go do `poll`.
pub(crate) fn clipboard_fd() -> Option<RawFd> {
    PENDING
        .lock_or_recover()
        .as_ref()
        .and_then(|pending| pending.read.as_ref().map(AsRawFd::as_raw_fd))
}

/// Czy most czeka na dane (pętla główna skraca wtedy timeout `poll`).
pub(crate) fn clipboard_pending() -> bool {
    PENDING.lock_or_recover().is_some()
}

/// Uruchamia pobieranie i odbiera dostępne bajty; finalizuje transfer.
///
/// Wołane w każdej iteracji pętli głównej — najpierw przed `poll` (start),
/// potem po przebudzeniu (odczyt).
pub(crate) fn pump_clipboard() {
    let mut slot = PENDING.lock_or_recover();
    let Some(pending) = slot.as_mut() else {
        return;
    };
    if pending.read.is_none() {
        match rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC) {
            Ok((read_fd, write_fd)) => {
                let sent = pending.source.send(pending.mime.clone(), write_fd.as_fd());
                // Nasz koniec piszący zamykamy od razu — klient pisze do swojej
                // kopii, a my dostaniemy EOF, gdy skończy.
                drop(write_fd);
                if clipboard_verbose() {
                    eprintln!(
                        "clipboard: most X11: send_event={sent} mime={}",
                        pending.mime
                    );
                }
                let _ = rustix::fs::fcntl_setfl(&read_fd, rustix::fs::OFlags::NONBLOCK);
                pending.read = Some(read_fd);
                if !sent {
                    pending.deadline = Instant::now();
                }
            }
            Err(err) => {
                if clipboard_verbose() {
                    eprintln!("clipboard: most X11: pipe nie wyszedł: {err}");
                }
                pending.deadline = Instant::now();
            }
        }
    }
    let mut eof = false;
    let mut failed = false;
    if let Some(fd) = pending.read.as_ref() {
        let mut buf = [0u8; 16384];
        loop {
            match rustix::io::read(fd, &mut buf) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    pending.out.extend_from_slice(&buf[..n]);
                    if pending.out.len() > MAX_CLIPBOARD_BYTES {
                        failed = true;
                        break;
                    }
                }
                Err(rustix::io::Errno::AGAIN) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
    }
    let timed_out = Instant::now() >= pending.deadline;
    if !eof && !failed && !timed_out {
        return;
    }
    let done = slot.take().expect("pending fetch");
    let still_current = CLIPBOARD_GENERATION.load(Ordering::SeqCst) == done.generation;
    drop(slot);
    if !still_current {
        return;
    }
    if done.out.is_empty() || failed {
        if clipboard_verbose() {
            eprintln!("clipboard: most X11: źródło nie oddało danych");
        }
        clear_x11_clipboard();
    } else {
        if clipboard_verbose() {
            eprintln!("clipboard: most X11: {} B → XWM", done.out.len());
        }
        if let Some(xwm) = crate::xwm::global() {
            xwm.set_clipboard(done.out);
        }
    }
}

/// Wysyła `data_offer` + typy MIME + `selection` do jednego urządzenia.
fn send_selection(handle: &DisplayHandle, device: &WlDataDevice, source: Option<&SelectionSource>) {
    if debug_clipboard() {
        eprintln!(
            "clipboard: send_selection device={} source={:?}",
            device.id().protocol_id(),
            source.map(|s| s.id().protocol_id())
        );
    }
    match source {
        Some(src) => {
            let Some(client) = device.client() else {
                return;
            };
            let mimes = src.mimes();
            let offer_data = DataOfferData {
                source: Mutex::new(Some(src.clone())),
            };
            // Wersja obiektu nie może przekroczyć wersji `wl_data_device` klienta —
            // inaczej starsze klienty (v1/v2) dostają ofertę v3 i zrywają połączenie.
            let offer_version = device.version().min(3);
            let Ok(offer) = client.create_resource::<WlDataOffer, DataOfferData, State>(
                handle,
                offer_version,
                offer_data,
            ) else {
                return;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_mime_prefers_utf8() {
        let mimes = vec![
            "TEXT".to_owned(),
            "STRING".to_owned(),
            "text/plain;charset=UTF-8".to_owned(),
        ];
        assert_eq!(
            choose_text_mime(&mimes).as_deref(),
            Some("text/plain;charset=utf-8")
        );
        // Sam obraz nie ma czego mostkować do X11 (jeszcze).
        assert!(choose_text_mime(&["image/png".to_owned()]).is_none());
    }

    #[test]
    fn x11_cached_source_serves_cached_bytes() {
        let clip = Arc::new(crate::xwm::X11Clipboard {
            generation: 1,
            created_at: 0,
            mimes: vec!["text/plain;charset=utf-8".to_owned()],
            data: Arc::new(b"hello".to_vec()),
        });
        let source = SelectionSource::X11Cached(clip);
        assert_eq!(source.kind(), "x11");
        assert_eq!(source.mimes(), vec!["text/plain;charset=utf-8"]);
        let (read, write) = rustix::pipe::pipe().expect("pipe");
        assert!(source.send("text/plain".to_owned(), write.as_fd()));
        drop(write);
        let mut buf = [0u8; 32];
        let n = rustix::io::read(&read, &mut buf).expect("read");
        assert_eq!(&buf[..n], b"hello");
    }

    #[test]
    fn mime_data_respects_limit() {
        let data = MimeData::default();
        for i in 0..(MAX_MIME_TYPES + 5) {
            data.add(format!("text/plain;{i}"));
        }
        assert_eq!(data.snapshot().len(), MAX_MIME_TYPES);
    }
}
