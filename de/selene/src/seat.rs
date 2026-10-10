//! `wl_seat` — wejście: wskaźnik (`wl_pointer`), klawiatura (`wl_keyboard`) i trasowanie.
//!
//! Zdarzenia wejścia pochodzą z magistrali (`crate::input`) — przekazuje je wątek wyjścia,
//! który odbiera je od nadrzędnego kompozytora. Tutaj tłumaczymy je na zdarzenia protokołu
//! wysyłane do sfokusowanych powierzchni klientów oraz na operacje okien (przeciąganie,
//! przeskalowanie, zamknięcie) wykonywane przez wbudowaną dekorację (SSD).

use std::io::{Seek, Write};
use std::os::fd::{AsFd, OwnedFd};

use libluna_core::{ButtonState, Event, Modifiers, Point, PointerButton};
use wayland_protocols::xdg::shell::server::{xdg_popup, xdg_surface, xdg_toplevel};
use wayland_server::protocol::{wl_keyboard, wl_pointer, wl_seat, wl_touch};
use wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::compositor::SurfaceData;
use crate::shell::XdgSurfaceData;
use crate::state::{DragState, ResizeState, State, TITLEBAR};

/// Kompaktowa mapa klawiszy (standardowe reguły XKB z systemowych danych).
const KEYMAP: &str = r#"xkb_keymap {
    xkb_keycodes { include "evdev+aliases(qwerty)" };
    xkb_types { include "complete" };
    xkb_compat { include "complete" };
    xkb_symbols { include "pc+us+inet(evdev)" };
    xkb_geometry { include "pc(pc105)" };
};
"#;

/// Minimalny rozmiar treści okna przy przeskalowaniu.
const MIN_W: f64 = 120.0;
const MIN_H: f64 = 80.0;

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
        let caps = wl_seat::Capability::Pointer | wl_seat::Capability::Keyboard;
        let _ = seat.send_event(wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(caps),
        });
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &wl_seat::WlSeat,
        request: wl_seat::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_seat::Request::GetPointer { id } => {
                state.pointer = Some(data_init.init(id, ()));
            }
            wl_seat::Request::GetKeyboard { id } => {
                let kb = data_init.init(id, ());
                if let Some((fd, size)) = make_keymap() {
                    let _ = kb.send_event(wl_keyboard::Event::Keymap {
                        format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
                        fd: fd.as_fd(),
                        size,
                    });
                }
                state.keyboard = Some(kb);
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

/// Buduje deskryptor z mapą klawiszy (memfd + tekst XKB, zakończony NUL).
fn make_keymap() -> Option<(OwnedFd, u32)> {
    let fd = rustix::fs::memfd_create("selene-keymap", rustix::fs::MemfdFlags::CLOEXEC).ok()?;
    let mut file = std::fs::File::from(fd);
    let mut bytes = KEYMAP.as_bytes().to_vec();
    bytes.push(0);
    file.write_all(&bytes).ok()?;
    file.rewind().ok()?;
    Some((OwnedFd::from(file), bytes.len() as u32))
}

// --- Trasowanie wejścia ---

/// Część okna, w którą trafił wskaźnik.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Close,
    Grip,
    Title,
    Content,
}

/// Trasuje jedno zdarzenie wejścia do klientów/okien.
pub fn handle_input(state: &mut State, event: &Event) {
    match event {
        Event::PointerMotion { position, .. } => pointer_motion(state, *position),
        Event::PointerButton(pe) => pointer_button(state, pe.position, pe.button, pe.state),
        Event::PointerLeave => pointer_leave(state),
        Event::Scroll(se) => pointer_axis(state, se.dx, se.dy),
        Event::Key(ke) => keyboard_key(state, ke.keysym, ke.state),
        _ => {}
    }
}

/// Trafienie w okno (najwyższe wg `z`) i w którą jego część.
fn hit(state: &State, pos: Point) -> Option<(usize, Part)> {
    let windows = state.windows.lock().unwrap();
    let mut best: Option<(usize, i32)> = None;
    for (i, w) in windows.iter().enumerate() {
        if w.mapped && w.outer().contains(pos) {
            match best {
                Some((_, z)) if z >= w.z => {}
                _ => best = Some((i, w.z)),
            }
        }
    }
    let (i, _) = best?;
    let w = &windows[i];
    let part = if w.close_rect().contains(pos) {
        Part::Close
    } else if w.grip_rect().contains(pos) {
        Part::Grip
    } else if w.titlebar().contains(pos) {
        Part::Title
    } else {
        Part::Content
    };
    Some((i, part))
}

/// Najwyższe okno, którego **treść** zawiera punkt (do zdarzeń klienta).
fn content_window(state: &State, pos: Point) -> Option<usize> {
    let windows = state.windows.lock().unwrap();
    let mut best: Option<(usize, i32)> = None;
    for (i, w) in windows.iter().enumerate() {
        if w.mapped && w.content().contains(pos) {
            match best {
                Some((_, z)) if z >= w.z => {}
                _ => best = Some((i, w.z)),
            }
        }
    }
    best.map(|(i, _)| i)
}

/// Zamyka popupy, poza którymi kliknięto (wysyła `xdg_popup.popup_done`, zdejmuje z widoku).
fn dismiss_popups(state: &mut State, pos: Point) {
    loop {
        let top = {
            let windows = state.windows.lock().unwrap();
            let mut best: Option<(usize, i32)> = None;
            for (i, w) in windows.iter().enumerate() {
                if w.popup && w.mapped {
                    match best {
                        Some((_, z)) if z >= w.z => {}
                        _ => best = Some((i, w.z)),
                    }
                }
            }
            best.map(|(i, _)| i)
        };
        let Some(slot) = top else { break };
        let inside = state
            .windows
            .lock()
            .unwrap()
            .get(slot)
            .map(|w| w.outer().contains(pos))
            .unwrap_or(false);
        if inside {
            break;
        }
        if let Some(surface) = state.slots.get(slot).cloned() {
            if let Some(sd) = surface.data::<SurfaceData>() {
                if let Some(xs) = sd.xdg_surface.lock().unwrap().clone() {
                    if let Some(d) = xs.data::<XdgSurfaceData>() {
                        if let Some(popup) = d.popup.lock().unwrap().as_ref() {
                            let _ = popup.send_event(xdg_popup::Event::PopupDone);
                        }
                    }
                }
            }
        }
        if let Some(w) = state.windows.lock().unwrap().get_mut(slot) {
            w.mapped = false;
        }
    }
}

fn window_geometry(state: &State, slot: usize) -> Option<(f64, f64, f64, f64)> {
    let windows = state.windows.lock().unwrap();
    windows
        .get(slot)
        .map(|w| (w.x, w.y, w.width as f64, w.height as f64))
}

fn content_local(state: &State, slot: usize, pos: Point) -> (f64, f64) {
    match window_geometry(state, slot) {
        Some((x, y, _, _)) => (pos.x - x, pos.y - (y + TITLEBAR)),
        None => (pos.x, pos.y),
    }
}

fn pointer_motion(state: &mut State, pos: Point) {
    if state.resize.is_some() {
        do_resize(state, pos);
        return;
    }
    if let Some(drag) = state.drag {
        let mut windows = state.windows.lock().unwrap();
        if let Some(w) = windows.get_mut(drag.slot) {
            w.x = pos.x - drag.off_x;
            w.y = pos.y - drag.off_y;
        }
    }
    update_pointer_focus(state, pos);
    if let Some((slot, _)) = state.pointer_focus.clone() {
        let ptr = state.pointer.clone();
        let (lx, ly) = content_local(state, slot, pos);
        let time = state.time_ms();
        if let Some(ptr) = ptr {
            let _ = ptr.send_event(wl_pointer::Event::Motion {
                time,
                surface_x: lx,
                surface_y: ly,
            });
        }
    }
}

fn pointer_button(state: &mut State, pos: Point, button: PointerButton, st: ButtonState) {
    if st == ButtonState::Released {
        state.drag = None;
        state.resize = None;
    } else {
        // Klik poza popupem zamyka popup (przed trafieniem w okno).
        dismiss_popups(state, pos);
    }

    // Obsłuż dekorację/okno pod kursorem.
    if let Some((slot, part)) = hit(state, pos) {
        if st == ButtonState::Pressed {
            raise_and_focus(state, slot);
            match part {
                Part::Close => {
                    pointer_leave(state);
                    send_close(state, slot);
                    return;
                }
                Part::Grip => {
                    pointer_leave(state);
                    if let Some((_, _, w, h)) = window_geometry(state, slot) {
                        state.resize = Some(ResizeState {
                            slot,
                            start_w: w,
                            start_h: h,
                            start_px: pos.x,
                            start_py: pos.y,
                        });
                    }
                    return;
                }
                Part::Title => {
                    pointer_leave(state);
                    if let Some((ox, oy, _, _)) = window_geometry(state, slot) {
                        state.drag = Some(DragState {
                            slot,
                            off_x: pos.x - ox,
                            off_y: pos.y - oy,
                        });
                    }
                    return;
                }
                Part::Content => update_pointer_focus(state, pos),
            }
        } else if part == Part::Content {
            // Zwolnienie nad treścią → przekaż je klientowi poniżej.
            update_pointer_focus(state, pos);
        } else {
            return;
        }
    } else {
        update_pointer_focus(state, pos);
    }

    // Zdarzenie przycisku do klienta — tylko gdy kursor nad treścią okna.
    let focused = state.pointer_focus.clone();
    let ptr = state.pointer.clone();
    if let (Some(_), Some(ptr)) = (focused, ptr) {
        let serial = state.next_serial();
        let time = state.time_ms();
        let wl_state = match st {
            ButtonState::Pressed => WEnum::Value(wl_pointer::ButtonState::Pressed),
            ButtonState::Released => WEnum::Value(wl_pointer::ButtonState::Released),
        };
        let _ = ptr.send_event(wl_pointer::Event::Button {
            serial,
            time,
            button: button_code(button),
            state: wl_state,
        });
    }
}

fn pointer_leave(state: &mut State) {
    let ptr = state.pointer.clone();
    if let (Some((_, surface)), Some(ptr)) = (state.pointer_focus.clone(), ptr) {
        let serial = state.next_serial();
        let _ = ptr.send_event(wl_pointer::Event::Leave { serial, surface });
    }
    state.pointer_focus = None;
}

fn pointer_axis(state: &mut State, dx: f64, dy: f64) {
    let ptr = state.pointer.clone();
    if state.pointer_focus.is_some() {
        if let Some(ptr) = ptr {
            let time = state.time_ms();
            if dy != 0.0 {
                let _ = ptr.send_event(wl_pointer::Event::Axis {
                    time,
                    axis: WEnum::Value(wl_pointer::Axis::VerticalScroll),
                    value: dy,
                });
            }
            if dx != 0.0 {
                let _ = ptr.send_event(wl_pointer::Event::Axis {
                    time,
                    axis: WEnum::Value(wl_pointer::Axis::HorizontalScroll),
                    value: dx,
                });
            }
        }
    }
}

fn update_pointer_focus(state: &mut State, pos: Point) {
    let target = content_window(state, pos);
    let same = matches!((state.pointer_focus.as_ref(), target), (Some((s, _)), Some(t)) if *s == t);
    if same {
        return;
    }
    let ptr = state.pointer.clone();
    if let (Some((_, surface)), Some(ptr)) = (state.pointer_focus.clone(), ptr.clone()) {
        let serial = state.next_serial();
        let _ = ptr.send_event(wl_pointer::Event::Leave { serial, surface });
    }
    state.pointer_focus = None;
    if let Some(slot) = target {
        if let Some(surface) = state.slots.get(slot).cloned() {
            let serial = state.next_serial();
            let (lx, ly) = content_local(state, slot, pos);
            if let Some(ptr) = ptr {
                let _ = ptr.send_event(wl_pointer::Event::Enter {
                    serial,
                    surface: surface.clone(),
                    surface_x: lx,
                    surface_y: ly,
                });
            }
            state.pointer_focus = Some((slot, surface));
        }
    }
}

/// Przeskalowanie okna w trakcie przeciągania uchwytu; wysyła `xdg_toplevel.configure`.
fn do_resize(state: &mut State, pos: Point) {
    let Some(rs) = state.resize else {
        return;
    };
    let nw = (rs.start_w + (pos.x - rs.start_px)).max(MIN_W);
    let nh = (rs.start_h + (pos.y - rs.start_py)).max(MIN_H);
    {
        let mut windows = state.windows.lock().unwrap();
        if let Some(w) = windows.get_mut(rs.slot) {
            w.width = nw as i32;
            w.height = nh as i32;
        }
    }
    send_configure(state, rs.slot, nw, nh);
}

/// Wysyła konfigurację rozmiaru (`xdg_surface.configure` + `xdg_toplevel.configure`).
fn send_configure(state: &mut State, slot: usize, w: f64, h: f64) {
    let Some(surface) = state.slots.get(slot).cloned() else {
        return;
    };
    let Some(sd) = surface.data::<SurfaceData>() else {
        return;
    };
    let Some(xs) = sd.xdg_surface.lock().unwrap().clone() else {
        return;
    };
    let serial = state.next_serial();
    let _ = xs.send_event(xdg_surface::Event::Configure { serial });
    if let Some(xd) = xs.data::<XdgSurfaceData>() {
        if let Some(tl) = xd.toplevel.lock().unwrap().as_ref() {
            let _ = tl.send_event(xdg_toplevel::Event::Configure {
                width: w as i32,
                height: h as i32,
                states: vec![3],
            });
        }
    }
}

/// Prosi klienta o zamknięcie okna (`xdg_toplevel.close`).
fn send_close(state: &mut State, slot: usize) {
    let Some(surface) = state.slots.get(slot).cloned() else {
        return;
    };
    let Some(sd) = surface.data::<SurfaceData>() else {
        return;
    };
    let Some(xs) = sd.xdg_surface.lock().unwrap().clone() else {
        return;
    };
    if let Some(xd) = xs.data::<XdgSurfaceData>() {
        if let Some(tl) = xd.toplevel.lock().unwrap().as_ref() {
            let _ = tl.send_event(xdg_toplevel::Event::Close);
        }
    }
}

fn raise_and_focus(state: &mut State, slot: usize) {
    let z = state.next_z();
    {
        let mut windows = state.windows.lock().unwrap();
        if let Some(w) = windows.get_mut(slot) {
            // Powierzchnie warstwowe (panele/docki) trzymają stałą kolejność.
            if !w.layer {
                w.z = z;
            }
        }
    }
    set_keyboard_focus(state, slot);
}

fn set_keyboard_focus(state: &mut State, slot: usize) {
    if state.keyboard_focus.as_ref().map(|(s, _)| *s) == Some(slot) {
        return;
    }
    let kb = state.keyboard.clone();
    if let (Some((_, surface)), Some(kb)) = (state.keyboard_focus.clone(), kb.clone()) {
        let serial = state.next_serial();
        let _ = kb.send_event(wl_keyboard::Event::Leave { serial, surface });
    }
    state.keyboard_focus = None;
    if let Some(surface) = state.slots.get(slot).cloned() {
        if let Some(kb) = kb {
            let serial = state.next_serial();
            let _ = kb.send_event(wl_keyboard::Event::Enter {
                serial,
                surface: surface.clone(),
                keys: vec![],
            });
        }
        state.keyboard_focus = Some((slot, surface));
    }
}

fn keyboard_key(state: &mut State, keysym: u32, st: ButtonState) {
    if state.keyboard_focus.is_none() {
        return;
    }
    let kb = state.keyboard.clone();
    let Some(kb) = kb else { return };
    sync_modifiers(state, &kb, keysym, st);
    if is_modifier_keysym(keysym) {
        return;
    }
    if let Some(key) = keysym_to_keycode(keysym) {
        let serial = state.next_serial();
        let time = state.time_ms();
        let wl_state = match st {
            ButtonState::Pressed => WEnum::Value(wl_keyboard::KeyState::Pressed),
            ButtonState::Released => WEnum::Value(wl_keyboard::KeyState::Released),
        };
        let _ = kb.send_event(wl_keyboard::Event::Key {
            serial,
            time,
            key,
            state: wl_state,
        });
    }
}

/// Wysyła wciśnięcia/zwolnienia klawiszy modyfikujących wynikające z bieżącego stanu.
fn sync_modifiers(state: &mut State, kb: &wl_keyboard::WlKeyboard, keysym: u32, st: ButtonState) {
    let mut mods = state.sent_mods;
    if let Some(field) = modifier_for_keysym(keysym) {
        match st {
            ButtonState::Pressed => *field(&mut mods) = true,
            ButtonState::Released => *field(&mut mods) = false,
        }
    }
    for (before, now, code) in [
        (state.sent_mods.shift, mods.shift, 50u32),
        (state.sent_mods.ctrl, mods.ctrl, 37),
        (state.sent_mods.alt, mods.alt, 64),
        (state.sent_mods.logo, mods.logo, 133),
    ] {
        if before != now {
            let serial = state.next_serial();
            let time = state.time_ms();
            let ks = if now {
                WEnum::Value(wl_keyboard::KeyState::Pressed)
            } else {
                WEnum::Value(wl_keyboard::KeyState::Released)
            };
            let _ = kb.send_event(wl_keyboard::Event::Key {
                serial,
                time,
                key: code,
                state: ks,
            });
        }
    }
    if mods != state.sent_mods {
        state.sent_mods = mods;
        let serial = state.next_serial();
        let _ = kb.send_event(wl_keyboard::Event::Modifiers {
            serial,
            mods_depressed: modifier_mask(mods),
            mods_latched: 0,
            mods_locked: 0,
            group: 0,
        });
    }
}

fn modifier_for_keysym(keysym: u32) -> Option<fn(&mut Modifiers) -> &mut bool> {
    match keysym {
        0xffe1 | 0xffe2 => Some(|m| &mut m.shift), // Shift_L / Shift_R
        0xffe3 | 0xffe4 => Some(|m| &mut m.ctrl),  // Control_L / Control_R
        0xffe9 | 0xffea => Some(|m| &mut m.alt),   // Alt_L / Alt_R
        0xffeb | 0xffec => Some(|m| &mut m.logo),  // Super_L / Super_R
        _ => None,
    }
}

fn is_modifier_keysym(keysym: u32) -> bool {
    modifier_for_keysym(keysym).is_some()
}

fn modifier_mask(m: Modifiers) -> u32 {
    (m.shift as u32) | ((m.ctrl as u32) << 2) | ((m.alt as u32) << 3) | ((m.logo as u32) << 6)
}

fn button_code(button: PointerButton) -> u32 {
    match button {
        PointerButton::Left => 0x110,
        PointerButton::Right => 0x111,
        PointerButton::Middle => 0x112,
        PointerButton::Back => 0x116,
        PointerButton::Forward => 0x115,
        PointerButton::Other(c) => c,
    }
}

/// Keysym → kod klawisza XKB (evdev + 8) dla typowych klawiszy.
fn keysym_to_keycode(keysym: u32) -> Option<u32> {
    let code = match keysym {
        0x0020 => 65,  // space
        0xff0d => 36,  // Return
        0xff1b => 9,   // Escape
        0xff08 => 22,  // BackSpace
        0xff09 => 23,  // Tab
        0xffff => 119, // Delete
        0xff50 => 110, // Home
        0xff57 => 115, // End
        0xff51 => 113, // Left
        0xff52 => 111, // Up
        0xff53 => 114, // Right
        0xff54 => 116, // Down
        // litery (małe)
        0x0061 => 38,
        0x0062 => 56,
        0x0063 => 54,
        0x0064 => 40,
        0x0065 => 26,
        0x0066 => 41,
        0x0067 => 42,
        0x0068 => 43,
        0x0069 => 31,
        0x006a => 44,
        0x006b => 45,
        0x006c => 46,
        0x006d => 58,
        0x006e => 57,
        0x006f => 32,
        0x0070 => 33,
        0x0071 => 24,
        0x0072 => 27,
        0x0073 => 39,
        0x0074 => 28,
        0x0075 => 30,
        0x0076 => 55,
        0x0077 => 25,
        0x0078 => 53,
        0x0079 => 29,
        0x007a => 52,
        // cyfry
        0x0031 => 10,
        0x0032 => 11,
        0x0033 => 12,
        0x0034 => 13,
        0x0035 => 14,
        0x0036 => 15,
        0x0037 => 16,
        0x0038 => 17,
        0x0039 => 18,
        0x0030 => 19,
        // interpunkcja
        0x002d => 20,
        0x003d => 21,
        0x005b => 34,
        0x005d => 35,
        0x003b => 47,
        0x0027 => 48,
        0x0060 => 49,
        0x002c => 59,
        0x002e => 60,
        0x002f => 61,
        0x005c => 51,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Window;
    use libluna_render::ImageData;
    use std::sync::Arc;

    #[test]
    fn maps_common_keys() {
        assert_eq!(keysym_to_keycode(0x0061), Some(38)); // a
        assert_eq!(keysym_to_keycode(0xff0d), Some(36)); // Enter
        assert_eq!(keysym_to_keycode(0xff1b), Some(9)); // Escape
        assert_eq!(keysym_to_keycode(0x9999), None);
    }

    #[test]
    fn modifier_mask_matches_xkb() {
        let m = Modifiers {
            shift: true,
            ctrl: true,
            alt: false,
            logo: true,
        };
        assert_eq!(modifier_mask(m), 1 | 4 | 64);
    }

    #[test]
    fn recognizes_modifier_keysyms() {
        assert!(is_modifier_keysym(0xffe1)); // Shift_L
        assert!(!is_modifier_keysym(0x0061));
    }

    fn sample_window() -> Window {
        Window {
            x: 100.0,
            y: 50.0,
            width: 200,
            height: 120,
            z: 1,
            title: "Test".to_owned(),
            popup: false,
            layer: false,
            mapped: true,
            data: Arc::new(ImageData::empty(200, 120)),
        }
    }

    #[test]
    fn decoration_parts_are_laid_out() {
        let w = sample_window();
        // Pasek tytułu na górze, treść pod nim.
        assert_eq!(w.titlebar().bottom(), w.y + TITLEBAR);
        assert_eq!(w.content().y(), w.y + TITLEBAR);
        // Przycisk zamknięcia w prawym górnym narożniku.
        let c = w.close_rect();
        assert!(c.contains(Point::new(w.x + 200.0 - 1.0, w.y + 1.0)));
        assert!(!c.contains(Point::new(w.x + 10.0, w.y + 5.0)));
        // Uchwyt przeskalowania w prawym-dolnym narożniku treści.
        let g = w.grip_rect();
        let c = w.content();
        assert!(g.contains(Point::new(c.right() - 1.0, c.bottom() - 1.0)));
    }
}
