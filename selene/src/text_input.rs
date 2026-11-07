//! IME: `zwp_text_input_v3` po stronie aplikacji i `zwp_input_method_v2`
//! po stronie input method (fcitx5, ibus, własna klawiatura ekranowa).
//!
//! Model przepływu:
//!
//! ```text
//! aplikacja --text_input_v3--> Selene --input_method_v2--> IME (fcitx5)
//!     ^                           |                          |
//!     |                           `--grab_keyboard---------->|  (surowe klawisze)
//!     `--preedit/commit_string----'<--commit----------------'
//! ```
//!
//! Gdy IME jest aktywne i trzyma grab, fizyczne klawisze **nie** idą do
//! aplikacji — trafiają do IME. Klawisze, których IME nie skonsumowało, wracają
//! przez `zwp_virtual_keyboard_v1` i dopiero wtedy trafiają do klienta (bez
//! ponownego przetwarzania skrótów pulpitu). Dzięki temu działają języki CJK,
//! a zwykłe pisanie nie zatrzymuje się, gdy IME nie jest zainteresowane klawiszem.

use wayland_protocols::wp::text_input::zv3::server::{
    zwp_text_input_manager_v3::{self as ti_manager, ZwpTextInputManagerV3},
    zwp_text_input_v3::{Event as TextInputEvent, Request as TextInputRequest, ZwpTextInputV3},
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource, WEnum,
};

use crate::compositor::SurfaceData;
use crate::protocols::input_method_v2::{
    zwp_input_method_keyboard_grab_v2::{
        Event as GrabEvent, Request as GrabRequest, ZwpInputMethodKeyboardGrabV2,
    },
    zwp_input_method_manager_v2::{Request as ImManagerRequest, ZwpInputMethodManagerV2},
    zwp_input_method_v2::{Event as ImEvent, Request as ImRequest, ZwpInputMethodV2},
    zwp_input_popup_surface_v2::{
        Event as PopupEvent, Request as PopupRequest, ZwpInputPopupSurfaceV2,
    },
};
use crate::protocols::virtual_keyboard_v1::{
    zwp_virtual_keyboard_manager_v1::{Request as VkManagerRequest, ZwpVirtualKeyboardManagerV1},
    zwp_virtual_keyboard_v1::{Request as VkRequest, ZwpVirtualKeyboardV1},
};
use crate::state::State;
use crate::sync::LockExt;

/// Stan tekstowego wejścia po zastosowaniu `commit` (double-buffered).
#[derive(Default, Clone)]
pub struct TextInputState {
    pub enabled: bool,
    pub surrounding: Option<(String, i32, i32)>,
    pub change_cause: u32,
    pub content_type: (u32, u32),
    pub cursor_rect: Option<(i32, i32, i32, i32)>,
}

/// Zmiany zgłoszone przez klienta, zastosowane przy `commit`.
#[derive(Default)]
pub struct PendingTextInput {
    pub enabled: Option<bool>,
    pub surrounding: Option<(String, i32, i32)>,
    pub change_cause: Option<u32>,
    pub content_type: Option<(u32, u32)>,
    pub cursor_rect: Option<(i32, i32, i32, i32)>,
}

pub struct TextInputEntry {
    pub resource: ZwpTextInputV3,
    /// Powierzchnia z ostatniego `enter` (`None` = brak fokusu tekstowego).
    pub surface: Option<WlSurface>,
    pub pending: PendingTextInput,
    pub current: TextInputState,
    /// Liczba `commit` wysłanych przez klienta — serial w `done`.
    pub commits: u32,
}

/// Zmiany od IME, zastosowane przy `zwp_input_method_v2.commit`.
#[derive(Default)]
pub struct PendingImCommit {
    /// `Some(None)` = wyczyść preedit.
    pub preedit: Option<Option<String>>,
    pub preedit_cursor: (i32, i32),
    pub commit_string: Option<String>,
    pub delete: Option<(u32, u32)>,
}

pub struct InputMethodEntry {
    pub resource: ZwpInputMethodV2,
    pub grab: Option<ZwpInputMethodKeyboardGrabV2>,
    pub popups: Vec<PopupEntry>,
    pub active: bool,
    /// Liczba wysłanych `done` — serial wymagany w `commit`.
    pub done_count: u32,
    pub pending: PendingImCommit,
}

pub struct PopupEntry {
    pub resource: ZwpInputPopupSurfaceV2,
    pub surface: WlSurface,
}

pub struct VirtualKeyboardEntry {
    pub resource: ZwpVirtualKeyboardV1,
    pub keymap_set: bool,
}

/// Cały stan IME współdzielony z resztą kompozytora.
#[derive(Default)]
pub struct ImeState {
    pub text_inputs: Vec<TextInputEntry>,
    pub input_method: Option<InputMethodEntry>,
    pub virtual_keyboards: Vec<VirtualKeyboardEntry>,
    /// Włączone tekstowe wejście na powierzchni z fokusem (niezależnie od IME).
    pub active_input: Option<ZwpTextInputV3>,
    pub active_surface: Option<WlSurface>,
    /// Ostatni prostokąt kursora w współrzędnych powierzchni (dla IME).
    pub cursor_rect_local: Option<(i32, i32, i32, i32)>,
    /// Ten sam prostokąt w współrzędnych globalnych (do umieszczenia popupu).
    pub cursor_rect_global: Option<(i32, i32, i32, i32)>,
}

// — zwp_text_input_v3 —

impl GlobalDispatch<ZwpTextInputManagerV3, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpTextInputManagerV3>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpTextInputManagerV3,
        request: ti_manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ti_manager::Request::GetTextInput { id, seat: _ } => {
                let resource = data_init.init(id, ());
                state.ime.text_inputs.retain(|e| e.resource.is_alive());
                let index = state.ime.text_inputs.len();
                state.ime.text_inputs.push(TextInputEntry {
                    resource: resource.clone(),
                    surface: None,
                    pending: PendingTextInput::default(),
                    current: TextInputState::default(),
                    commits: 0,
                });
                // Klient może utworzyć text-input już po nadaniu fokusu.
                if let Some(surface) = focused_surface(state) {
                    if surface.client().as_ref() == resource.client().as_ref() {
                        enter_surface(state, index, surface);
                    }
                }
            }
            ti_manager::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputV3, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpTextInputV3,
        request: TextInputRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Some(index) = find_text_input(state, resource.id()) else {
            return;
        };
        match request {
            TextInputRequest::Enable => {
                state.ime.text_inputs[index].pending.enabled = Some(true);
            }
            TextInputRequest::Disable => {
                state.ime.text_inputs[index].pending.enabled = Some(false);
            }
            TextInputRequest::SetSurroundingText {
                text,
                cursor,
                anchor,
            } => {
                state.ime.text_inputs[index].pending.surrounding = Some((text, cursor, anchor));
            }
            TextInputRequest::SetTextChangeCause { cause } => {
                state.ime.text_inputs[index].pending.change_cause = Some(enum_u32(cause));
            }
            TextInputRequest::SetContentType { hint, purpose } => {
                state.ime.text_inputs[index].pending.content_type =
                    Some((enum_u32(hint), enum_u32(purpose)));
            }
            TextInputRequest::SetCursorRectangle {
                x,
                y,
                width,
                height,
            } => {
                state.ime.text_inputs[index].pending.cursor_rect = Some((x, y, width, height));
            }
            TextInputRequest::Commit => commit_text_input(state, index),
            TextInputRequest::Destroy => {
                let was_active = state
                    .ime
                    .active_input
                    .as_ref()
                    .is_some_and(|a| a.id() == resource.id());
                state
                    .ime
                    .text_inputs
                    .retain(|e| e.resource.id() != resource.id());
                if was_active {
                    deactivate_im(state);
                    state.ime.active_input = None;
                    state.ime.active_surface = None;
                }
            }
            _ => {}
        }
    }
}

fn enum_u32<T>(value: WEnum<T>) -> u32
where
    T: Copy + Into<u32>,
{
    match value {
        WEnum::Value(v) => v.into(),
        WEnum::Unknown(v) => v,
    }
}

fn find_text_input(state: &State, id: wayland_server::backend::ObjectId) -> Option<usize> {
    state
        .ime
        .text_inputs
        .iter()
        .position(|e| e.resource.id() == id && e.resource.is_alive())
}

fn focused_surface(state: &State) -> Option<WlSurface> {
    state.keyboard_focus.as_ref().map(|(_, s)| s.clone())
}

/// Zastosowanie `commit` klienta. Double-buffered: dopiero tutaj zmiany stają
/// się widoczne dla kompozytora i IME.
fn commit_text_input(state: &mut State, index: usize) {
    let Some(surface) = state.ime.text_inputs[index].surface.clone() else {
        // Po `leave` kompozytor ignoruje żądania do następnego `enter`.
        state.ime.text_inputs[index].commits = state.ime.text_inputs[index].commits.wrapping_add(1);
        return;
    };
    state.ime.text_inputs[index].commits = state.ime.text_inputs[index].commits.wrapping_add(1);
    let pending = std::mem::take(&mut state.ime.text_inputs[index].pending);
    let entry = &mut state.ime.text_inputs[index];
    let was_enabled = entry.current.enabled;
    if let Some(enabled) = pending.enabled {
        entry.current.enabled = enabled;
    }
    if let Some(surrounding) = pending.surrounding {
        entry.current.surrounding = Some(surrounding);
    }
    if let Some(cause) = pending.change_cause {
        entry.current.change_cause = cause;
    }
    if let Some(content_type) = pending.content_type {
        entry.current.content_type = content_type;
    }
    if let Some(rect) = pending.cursor_rect {
        entry.current.cursor_rect = Some(rect);
        update_cursor_rect(state, &surface, rect);
    }
    let enabled = state.ime.text_inputs[index].current.enabled;
    let resource = state.ime.text_inputs[index].resource.clone();

    if enabled {
        // To jest aktywne wejście tylko wtedy, gdy powierzchnia ma fokus.
        let focused = focused_surface(state);
        let same_surface = focused.as_ref().is_some_and(|s| s.id() == surface.id());
        if same_surface {
            state.ime.active_input = Some(resource);
            state.ime.active_surface = Some(surface);
            if !was_enabled {
                activate_im(state);
            } else {
                push_im_state(state);
            }
        }
    } else if state
        .ime
        .active_input
        .as_ref()
        .is_some_and(|a| a.id() == state.ime.text_inputs[index].resource.id())
    {
        deactivate_im(state);
        state.ime.active_input = None;
        state.ime.active_surface = None;
    }
}

fn update_cursor_rect(state: &mut State, surface: &WlSurface, rect: (i32, i32, i32, i32)) {
    state.ime.cursor_rect_local = Some(rect);
    let Some(global) = cursor_rect_global(state, surface, rect) else {
        return;
    };
    state.ime.cursor_rect_global = Some(global);
    send_popup_rect(state);
}

fn cursor_rect_global(
    state: &State,
    surface: &WlSurface,
    rect: (i32, i32, i32, i32),
) -> Option<(i32, i32, i32, i32)> {
    let slot = *surface.data::<SurfaceData>()?.slot.lock_or_recover();
    let windows = state.windows.lock_or_recover();
    let w = windows.get(slot?)?;
    let c = w.content();
    let ox = c.x() - f64::from(w.image_offset.0);
    let oy = c.y() - f64::from(w.image_offset.1);
    Some((
        rect.0 + ox.round() as i32,
        rect.1 + oy.round() as i32,
        rect.2,
        rect.3,
    ))
}

/// Fokus klawiatury przeszedł na inną powierzchnię: `leave` do wszystkich
/// obiektów starego klienta, `enter` do wszystkich obiektów nowego.
pub fn focus_changed(state: &mut State, old: Option<&WlSurface>, new: Option<&WlSurface>) {
    if let Some(old_surface) = old {
        let mut any_left = false;
        for entry in &mut state.ime.text_inputs {
            if entry
                .surface
                .as_ref()
                .is_some_and(|s| s.id() == old_surface.id())
            {
                let _ = entry.resource.send_event(TextInputEvent::Leave {
                    surface: old_surface.clone(),
                });
                entry.surface = None;
                entry.current.enabled = false;
                entry.pending = PendingTextInput::default();
                any_left = true;
            }
        }
        if any_left {
            deactivate_im(state);
            state.ime.active_input = None;
            state.ime.active_surface = None;
        }
    }
    if let Some(new_surface) = new {
        let client = new_surface.client();
        state.ime.text_inputs.retain(|e| e.resource.is_alive());
        let indexes: Vec<usize> = state
            .ime
            .text_inputs
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.surface.is_none()
                    && e.resource.client().as_ref() == client.as_ref()
                    && client.is_some()
            })
            .map(|(i, _)| i)
            .collect();
        for index in indexes {
            enter_surface(state, index, new_surface.clone());
        }
    }
}

fn enter_surface(state: &mut State, index: usize, surface: WlSurface) {
    let resource = state.ime.text_inputs[index].resource.clone();
    let _ = resource.send_event(TextInputEvent::Enter {
        surface: surface.clone(),
    });
    state.ime.text_inputs[index].surface = Some(surface);
    state.ime.text_inputs[index].current = TextInputState::default();
    state.ime.text_inputs[index].pending = PendingTextInput::default();
}

// — zwp_input_method_v2 —

impl GlobalDispatch<ZwpInputMethodManagerV2, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpInputMethodManagerV2>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpInputMethodManagerV2,
        request: ImManagerRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ImManagerRequest::GetInputMethod {
                seat: _,
                input_method,
            } => {
                let alive = state
                    .ime
                    .input_method
                    .as_ref()
                    .is_some_and(|im| im.resource.is_alive());
                let resource = data_init.init(input_method, ());
                if alive {
                    // Protokół pozwala na jeden input method na seat.
                    let _ = resource.send_event(ImEvent::Unavailable);
                    return;
                }
                state.ime.input_method = Some(InputMethodEntry {
                    resource,
                    grab: None,
                    popups: Vec::new(),
                    active: false,
                    done_count: 0,
                    pending: PendingImCommit::default(),
                });
                if state.ime.active_input.is_some() {
                    activate_im(state);
                }
            }
            ImManagerRequest::Destroy => {}
        }
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpInputMethodV2,
        request: ImRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ImRequest::CommitString { text } => {
                if let Some(im) = im_for(state, resource.id()) {
                    im.pending.commit_string = Some(text);
                }
            }
            ImRequest::SetPreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if let Some(im) = im_for(state, resource.id()) {
                    im.pending.preedit = Some(if text.is_empty() { None } else { Some(text) });
                    im.pending.preedit_cursor = (cursor_begin, cursor_end);
                }
            }
            ImRequest::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                if let Some(im) = im_for(state, resource.id()) {
                    im.pending.delete = Some((before_length, after_length));
                }
            }
            ImRequest::Commit { serial } => {
                let Some(im_index) = state
                    .ime
                    .input_method
                    .as_ref()
                    .filter(|im| im.resource.id() == resource.id())
                    .map(|_| 0)
                else {
                    return;
                };
                let _ = im_index;
                apply_im_commit(state, serial);
            }
            ImRequest::GetInputPopupSurface { id, surface } => {
                let popup = data_init.init(id, ());
                if let Some(data) = surface.data::<SurfaceData>() {
                    *data.ime_popup.lock_or_recover() = true;
                }
                if let Some(im) = state
                    .ime
                    .input_method
                    .as_mut()
                    .filter(|im| im.resource.id() == resource.id())
                {
                    im.popups.retain(|p| p.resource.is_alive());
                    im.popups.push(PopupEntry {
                        resource: popup,
                        surface,
                    });
                }
                send_popup_rect(state);
            }
            ImRequest::GrabKeyboard { keyboard } => {
                let grab = data_init.init(keyboard, ());
                let Some(im) = state
                    .ime
                    .input_method
                    .as_mut()
                    .filter(|im| im.resource.id() == resource.id())
                else {
                    // Obiekt nie należy do aktywnego input method — nie zapisujemy grabu.
                    return;
                };
                im.grab = Some(grab.clone());
                send_grab_keymap(state, &grab);
            }
            ImRequest::Destroy => {
                if state
                    .ime
                    .input_method
                    .as_ref()
                    .is_some_and(|im| im.resource.id() == resource.id())
                {
                    state.ime.input_method = None;
                }
            }
        }
    }
}

fn im_for<'a>(
    state: &'a mut State,
    id: wayland_server::backend::ObjectId,
) -> Option<&'a mut InputMethodEntry> {
    state
        .ime
        .input_method
        .as_mut()
        .filter(|im| im.resource.id() == id)
}

fn send_grab_keymap(state: &State, grab: &ZwpInputMethodKeyboardGrabV2) {
    if let Some((fd, size)) = crate::seat::make_keymap() {
        let _ = grab.send_event(GrabEvent::Keymap {
            format: WEnum::Value(wayland_server::protocol::wl_keyboard::KeymapFormat::XkbV1),
            fd: std::os::fd::AsFd::as_fd(&fd),
            size,
        });
    }
    let _ = grab.send_event(GrabEvent::RepeatInfo {
        rate: state.repeat_rate as i32,
        delay: state.repeat_delay as i32,
    });
    let _ = grab.send_event(GrabEvent::Modifiers {
        serial: 0,
        mods_depressed: 0,
        mods_latched: 0,
        mods_locked: 0,
        group: 0,
    });
}

impl Dispatch<ZwpInputPopupSurfaceV2, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        resource: &ZwpInputPopupSurfaceV2,
        request: PopupRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let PopupRequest::Destroy = request;
        if let Some(im) = _state.ime.input_method.as_mut() {
            im.popups.retain(|p| p.resource.id() != resource.id());
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpInputMethodKeyboardGrabV2,
        request: GrabRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let GrabRequest::Release = request;
        if let Some(im) = state.ime.input_method.as_mut() {
            if im.grab.as_ref().is_some_and(|g| g.id() == resource.id()) {
                im.grab = None;
            }
        }
    }
}

/// Aktywuje IME dla bieżącego tekstowego wejścia.
fn activate_im(state: &mut State) {
    let Some(current) = active_text_input_state(state) else {
        return;
    };
    if let Some(im) = state.ime.input_method.as_mut() {
        if !im.active {
            let _ = im.resource.send_event(ImEvent::Activate);
            im.active = true;
        }
        send_im_state(im, &current);
        send_im_done(im);
    }
    send_popup_rect(state);
}

fn active_text_input_state(state: &State) -> Option<TextInputState> {
    let active = state.ime.active_input.as_ref()?;
    state
        .ime
        .text_inputs
        .iter()
        .find(|e| e.resource.id() == active.id())
        .map(|e| e.current.clone())
}

fn send_im_state(im: &mut InputMethodEntry, current: &TextInputState) {
    if let Some((text, cursor, anchor)) = &current.surrounding {
        let _ = im.resource.send_event(ImEvent::SurroundingText {
            text: text.clone(),
            cursor: *cursor as u32,
            anchor: *anchor as u32,
        });
    }
    let _ = im.resource.send_event(ImEvent::TextChangeCause {
        cause: WEnum::Unknown(current.change_cause),
    });
    let _ = im.resource.send_event(ImEvent::ContentType {
        hint: WEnum::Unknown(current.content_type.0),
        purpose: WEnum::Unknown(current.content_type.1),
    });
}

fn send_im_done(im: &mut InputMethodEntry) {
    im.done_count = im.done_count.wrapping_add(1);
    let _ = im.resource.send_event(ImEvent::Done);
}

/// Aktualny stan tekstowego wejścia dla nowego `commit` od IME.
fn push_im_state(state: &mut State) {
    let Some(current) = active_text_input_state(state) else {
        return;
    };
    if let Some(im) = state.ime.input_method.as_mut() {
        if im.active {
            send_im_state(im, &current);
            send_im_done(im);
        }
    }
}

fn deactivate_im(state: &mut State) {
    if let Some(im) = state.ime.input_method.as_mut() {
        if im.active {
            let _ = im.resource.send_event(ImEvent::Deactivate);
            im.active = false;
            send_im_done(im);
        }
    }
    hide_ime_popups(state);
}

/// Dezaktywacja IME chowa okno kandydatów — klient często zostawia je
/// „committed”, a bez tego wisiałoby nad polem tekstowym po zmianie fokusu.
fn hide_ime_popups(state: &mut State) {
    let surfaces: Vec<WlSurface> = state
        .ime
        .input_method
        .as_ref()
        .map(|im| im.popups.iter().map(|p| p.surface.clone()).collect())
        .unwrap_or_default();
    for surface in surfaces {
        let Some(data) = surface.data::<SurfaceData>() else {
            continue;
        };
        let Some(slot) = *data.slot.lock_or_recover() else {
            continue;
        };
        if let Some(win) = state.windows.lock_or_recover().get_mut(slot) {
            win.mapped = false;
        }
    }
}

fn apply_im_commit(state: &mut State, serial: u32) {
    let Some(im) = state.ime.input_method.as_mut() else {
        return;
    };
    let valid = serial == im.done_count;
    let pending = std::mem::take(&mut im.pending);
    if !valid {
        return;
    }
    let Some(active) = state.ime.active_input.clone() else {
        return;
    };
    // Preedit jest wysyłany zawsze (pusty czyści kompozycję), potem usunięcie
    // i wstawienie zatwierdzonego tekstu — kolejność wymagana przez protokół.
    let (preedit, cursor_begin, cursor_end) = match pending.preedit {
        Some(text) => {
            let (b, e) = pending.preedit_cursor;
            (text, b, e)
        }
        None => (None, 0, 0),
    };
    let _ = active.send_event(TextInputEvent::PreeditString {
        text: preedit,
        cursor_begin,
        cursor_end,
    });
    if let Some((before, after)) = pending.delete {
        let _ = active.send_event(TextInputEvent::DeleteSurroundingText {
            before_length: before,
            after_length: after,
        });
    }
    if let Some(text) = pending.commit_string {
        if !text.is_empty() {
            let _ = active.send_event(TextInputEvent::CommitString { text: Some(text) });
        }
    }
    // Serial `done` to liczba `commit` klienta (nie nasza).
    let commit_serial = state
        .ime
        .text_inputs
        .iter()
        .find(|e| e.resource.id() == active.id())
        .map(|e| e.commits)
        .unwrap_or(0);
    let _ = active.send_event(TextInputEvent::Done {
        serial: commit_serial,
    });
}

fn send_popup_rect(state: &mut State) {
    let Some(rect) = state.ime.cursor_rect_local else {
        return;
    };
    let Some(im) = state.ime.input_method.as_mut() else {
        return;
    };
    im.popups.retain(|p| p.resource.is_alive());
    for popup in &im.popups {
        let _ = popup.resource.send_event(PopupEvent::TextInputRectangle {
            x: rect.0,
            y: rect.1,
            width: rect.2,
            height: rect.3,
        });
    }
}

/// Grab klawiatury aktywnego IME (jeśli istnieje i jest aktywny).
pub fn keyboard_grab(state: &State) -> Option<ZwpInputMethodKeyboardGrabV2> {
    let im = state.ime.input_method.as_ref()?;
    if !im.active {
        return None;
    }
    im.grab.as_ref().filter(|g| g.is_alive()).cloned()
}

/// Wysyła fizyczny klawisz do IME (gdy trzyma grab).
pub fn send_key(state: &mut State, keycode: u32, pressed: bool) {
    let Some(grab) = keyboard_grab(state) else {
        return;
    };
    let serial = state.next_serial();
    let time = state.time_ms();
    let state_value = if pressed {
        WEnum::Value(wayland_server::protocol::wl_keyboard::KeyState::Pressed)
    } else {
        WEnum::Value(wayland_server::protocol::wl_keyboard::KeyState::Released)
    };
    let _ = grab.send_event(GrabEvent::Key {
        serial,
        time,
        key: keycode,
        state: state_value,
    });
}

/// Wysyła stan modyfikatorów do grabu IME.
pub fn send_modifiers(state: &State, depressed: u32, locked: u32) {
    if let Some(grab) = keyboard_grab(state) {
        let _ = grab.send_event(GrabEvent::Modifiers {
            serial: 0,
            mods_depressed: depressed,
            mods_latched: 0,
            mods_locked: locked,
            group: 0,
        });
    }
}

// — zwp_virtual_keyboard_v1 —

impl GlobalDispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpVirtualKeyboardManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpVirtualKeyboardManagerV1,
        request: VkManagerRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let VkManagerRequest::CreateVirtualKeyboard { seat: _, id } = request;
        let resource = data_init.init(id, ());
        state
            .ime
            .virtual_keyboards
            .retain(|v| v.resource.is_alive());
        state.ime.virtual_keyboards.push(VirtualKeyboardEntry {
            resource,
            keymap_set: false,
        });
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpVirtualKeyboardV1,
        request: VkRequest,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Some(index) = state
            .ime
            .virtual_keyboards
            .iter()
            .position(|v| v.resource.id() == resource.id())
        else {
            return;
        };
        match request {
            VkRequest::Keymap { .. } => {
                state.ime.virtual_keyboards[index].keymap_set = true;
            }
            VkRequest::Key {
                key,
                state: key_state,
                ..
            } => {
                if !state.ime.virtual_keyboards[index].keymap_set {
                    return;
                }
                let pressed = key_state == 1;
                // Klawisze wracają do klienta **bez** ponownego przetwarzania
                // skrótów pulpitu — IME już zdecydowało, że ma je przepuścić.
                crate::seat::inject_virtual_key(state, key, pressed);
            }
            VkRequest::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } => {
                crate::seat::inject_virtual_modifiers(
                    state,
                    mods_depressed,
                    mods_latched,
                    mods_locked,
                    group,
                );
            }
            VkRequest::Destroy => {
                state
                    .ime
                    .virtual_keyboards
                    .retain(|v| v.resource.id() != resource.id());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_no_grab_and_no_active_input() {
        let state = State::default();
        assert!(keyboard_grab(&state).is_none());
        assert!(state.ime.active_input.is_none());
    }

    #[test]
    fn enum_conversion_keeps_unknown_values() {
        use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::ChangeCause;
        assert_eq!(enum_u32(WEnum::<ChangeCause>::Unknown(77u32)), 77);
    }
}
