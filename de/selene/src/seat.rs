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
use crate::data;
use crate::shell::XdgSurfaceData;
use crate::state::{DragState, ResizeState, SavedGeom, State, TITLEBAR};
use crate::sync::LockExt;

/// Buduje tekst mapy klawiszy XKB dla danego układu (i opcjonalnego wariantu).
///
/// Nazwy są sanityzowane, aby nie można było wstrzyknąć treści spoza zestawu
/// znaków używanego przez XKB.
fn keymap_text_for(layout: &str, variant: Option<&str>) -> String {
    let layout = sanitize_xkb(layout);
    let layout = if layout.is_empty() {
        "us".to_owned()
    } else {
        layout
    };
    let variant = variant.map(sanitize_xkb).filter(|v| !v.is_empty());
    let symbols = match variant {
        Some(v) => format!("pc+{layout}({v})+inet(evdev)"),
        None => format!("pc+{layout}+inet(evdev)"),
    };
    format!(
        "xkb_keymap {{\n    xkb_keycodes {{ include \"evdev+aliases(qwerty)\" }};\n    xkb_types {{ include \"complete\" }};\n    xkb_compat {{ include \"complete\" }};\n    xkb_symbols {{ include \"{symbols}\" }};\n    xkb_geometry {{ include \"pc(pc105)\" }};\n}};\n"
    )
}

/// Bieżący układ z otoczenia (`LUNA_KEYBOARD_LAYOUT` / `LUNA_KEYBOARD_VARIANT`).
fn keymap_text() -> String {
    let layout = std::env::var("LUNA_KEYBOARD_LAYOUT").unwrap_or_else(|_| "us".to_owned());
    let variant = std::env::var("LUNA_KEYBOARD_VARIANT").ok();
    keymap_text_for(&layout, variant.as_deref())
}

/// Dopuszcza tylko znaki legalne w nazwie układu XKB.
fn sanitize_xkb(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// Maski krawędzi zmiany rozmiaru.
const EDGE_TOP: u8 = 1;
const EDGE_BOTTOM: u8 = 2;
const EDGE_LEFT: u8 = 4;
const EDGE_RIGHT: u8 = 8;
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
                // Powtarzanie klawiszy: 25/s po 600 ms.
                let _ = kb.send_event(wl_keyboard::Event::RepeatInfo {
                    rate: 25,
                    delay: 600,
                });
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
    let mut bytes = keymap_text().into_bytes();
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
    Maximize,
    Minimize,
    Title,
    Content,
    /// Zmiana rozmiaru za krawędź/narożnik (bitmaska: 1=góra,2=dół,4=lewo,8=prawo).
    Resize(u8),
}

/// Trasuje jedno zdarzenie wejścia do klientów/okien.
pub fn handle_input(state: &mut State, handle: &DisplayHandle, event: &Event) {
    match event {
        Event::PointerMotion { position, .. } => pointer_motion(state, handle, *position),
        Event::PointerButton(pe) => pointer_button(state, handle, pe.position, pe.button, pe.state),
        Event::PointerLeave => pointer_leave(state),
        Event::Scroll(se) => pointer_axis(state, se.dx, se.dy),
        Event::Key(ke) => keyboard_key(state, ke.keysym, ke.state),
        _ => {}
    }
}

/// Trafienie w okno (najwyższe wg `z`) i w którą jego część.
fn hit(state: &State, pos: Point) -> Option<(usize, Part)> {
    let windows = state.windows.lock_or_recover();
    let mut best: Option<(usize, i32)> = None;
    for (i, w) in windows.iter().enumerate() {
        if w.mapped && !w.minimized && w.outer().contains(pos) {
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
    } else if w.maximize_rect().contains(pos) {
        Part::Maximize
    } else if w.minimize_rect().contains(pos) {
        Part::Minimize
    } else {
        let edges = w.resize_edges(pos);
        if edges != 0 {
            Part::Resize(edges)
        } else if w.titlebar().contains(pos) {
            Part::Title
        } else {
            Part::Content
        }
    };
    Some((i, part))
}

/// Najwyższe okno, którego **treść** zawiera punkt (do zdarzeń klienta).
fn content_window(state: &State, pos: Point) -> Option<usize> {
    let windows = state.windows.lock_or_recover();
    let mut best: Option<(usize, i32)> = None;
    for (i, w) in windows.iter().enumerate() {
        if w.mapped && !w.minimized && w.content().contains(pos) {
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
            let windows = state.windows.lock_or_recover();
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
            .lock_or_recover()
            .get(slot)
            .map(|w| w.outer().contains(pos))
            .unwrap_or(false);
        if inside {
            break;
        }
        if let Some(surface) = state.slots.get(slot).cloned() {
            if let Some(sd) = surface.data::<SurfaceData>() {
                if let Some(xs) = sd.xdg_surface.lock_or_recover().clone() {
                    if let Some(d) = xs.data::<XdgSurfaceData>() {
                        if let Some(popup) = d.popup.lock_or_recover().as_ref() {
                            let _ = popup.send_event(xdg_popup::Event::PopupDone);
                        }
                    }
                }
            }
        }
        if let Some(w) = state.windows.lock_or_recover().get_mut(slot) {
            w.mapped = false;
        }
    }
}

/// Okno docelowe dla punktu (publiczne dla drag & drop).
pub(crate) fn window_at(state: &State, pos: (f64, f64)) -> Option<usize> {
    content_window(state, Point::new(pos.0, pos.1))
}

fn window_geometry(state: &State, slot: usize) -> Option<(f64, f64, f64, f64)> {
    let windows = state.windows.lock_or_recover();
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

fn pointer_motion(state: &mut State, handle: &DisplayHandle, pos: Point) {
    state.pointer_pos = (pos.x, pos.y);
    if state.resize.is_some() {
        do_resize(state, pos);
        return;
    }
    if let Some(drag) = state.drag {
        let mut windows = state.windows.lock_or_recover();
        if let Some(w) = windows.get_mut(drag.slot) {
            w.x = pos.x - drag.off_x;
            w.y = pos.y - drag.off_y;
        }
    }
    // Przeciąganie danych (drag & drop) aktualizuje cel.
    if state.dnd.is_some() {
        data::dnd_motion(state, handle, (pos.x, pos.y));
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

fn pointer_button(
    state: &mut State,
    handle: &DisplayHandle,
    pos: Point,
    button: PointerButton,
    st: ButtonState,
) {
    if st == ButtonState::Released {
        state.drag = None;
        state.resize = None;
        // Zakończ przeciąganie danych, jeśli trwało.
        if state.dnd.is_some() {
            data::dnd_drop(state, handle);
        }
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
                Part::Minimize => {
                    pointer_leave(state);
                    toggle_minimize(state, slot);
                    return;
                }
                Part::Maximize => {
                    pointer_leave(state);
                    toggle_maximize(state, slot);
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
                Part::Resize(edges) => {
                    pointer_leave(state);
                    if let Some((x, y, w, h)) = window_geometry(state, slot) {
                        state.resize = Some(ResizeState {
                            slot,
                            edges,
                            start_x: x,
                            start_y: y,
                            start_w: w,
                            start_h: h,
                            start_px: pos.x,
                            start_py: pos.y,
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

/// Przeskalowanie okna w trakcie przeciągania krawędzi/narożnika; wysyła `xdg_toplevel.configure`.
fn do_resize(state: &mut State, pos: Point) {
    let Some(rs) = state.resize else {
        return;
    };
    let dx = pos.x - rs.start_px;
    let dy = pos.y - rs.start_py;
    let mut x = rs.start_x;
    let mut y = rs.start_y;
    let mut w = rs.start_w;
    let mut h = rs.start_h;
    if rs.edges & EDGE_LEFT != 0 {
        w = (rs.start_w - dx).max(MIN_W);
        x = rs.start_x + (rs.start_w - w);
    } else if rs.edges & EDGE_RIGHT != 0 {
        w = (rs.start_w + dx).max(MIN_W);
    }
    if rs.edges & EDGE_TOP != 0 {
        h = (rs.start_h - dy).max(MIN_H);
        y = rs.start_y + (rs.start_h - h);
    } else if rs.edges & EDGE_BOTTOM != 0 {
        h = (rs.start_h + dy).max(MIN_H);
    }
    {
        let mut windows = state.windows.lock_or_recover();
        if let Some(w2) = windows.get_mut(rs.slot) {
            w2.x = x;
            w2.y = y;
            w2.width = w as i32;
            w2.height = h as i32;
        }
    }
    send_configure(state, rs.slot, w, h);
}

/// Wysyła konfigurację rozmiaru i stanów (`xdg_surface.configure` + `xdg_toplevel.configure`).
fn send_configure(state: &mut State, slot: usize, w: f64, h: f64) {
    let Some(surface) = state.slots.get(slot).cloned() else {
        return;
    };
    let Some(sd) = surface.data::<SurfaceData>() else {
        return;
    };
    let Some(xs) = sd.xdg_surface.lock_or_recover().clone() else {
        return;
    };
    let states = window_states(state, slot);
    let serial = state.next_serial();
    let _ = xs.send_event(xdg_surface::Event::Configure { serial });
    if let Some(xd) = xs.data::<XdgSurfaceData>() {
        if let Some(tl) = xd.toplevel.lock_or_recover().as_ref() {
            let _ = tl.send_event(xdg_toplevel::Event::Configure {
                width: w as i32,
                height: h as i32,
                states,
            });
        }
    }
}

/// Stany `xdg_toplevel` dla okna (ACTIVATED + ewentualnie MAXIMIZED/FULLSCREEN).
fn window_states(state: &State, slot: usize) -> Vec<u8> {
    let mut states = vec![4u8]; // ACTIVATED
    if let Some(w) = state.windows.lock_or_recover().get(slot) {
        if w.maximized {
            states.push(1); // MAXIMIZED
        }
        if w.fullscreen {
            states.push(2); // FULLSCREEN
        }
    }
    states
}

/// Prostokąt monitora, na którym leży okno (x, y, szerokość, wysokość).
fn window_output_rect(state: &State, slot: usize) -> (f64, f64, f64, f64) {
    let (cx, cy) = state
        .windows
        .lock_or_recover()
        .get(slot)
        .map(|w| {
            let c = w.content().center();
            (c.x, c.y)
        })
        .unwrap_or((0.0, 0.0));
    let outs = state.outputs.lock_or_recover();
    let idx = crate::monitors::output_at(&outs, cx, cy);
    match outs.get(idx).or_else(|| outs.first()) {
        Some(o) => (o.x as f64, o.y as f64, o.width as f64, o.height as f64),
        None => (0.0, 0.0, 1280.0, 800.0),
    }
}

/// Przełącza minimalizację (ukrycie przez kompozytor).
fn toggle_minimize(state: &mut State, slot: usize) {
    let minimized = {
        let mut windows = state.windows.lock_or_recover();
        match windows.get_mut(slot) {
            Some(w) if !w.popup && !w.layer => {
                w.minimized = !w.minimized;
                w.minimized
            }
            _ => return,
        }
    };
    if minimized {
        clear_focus_if(state, slot);
    }
    crate::foreign_toplevel::broadcast_states(state);
}

/// Przełącza maksymalizację (zapamiętuje/odtwarza geometrię).
fn toggle_maximize(state: &mut State, slot: usize) {
    let (ox, oy, ow, oh) = window_output_rect(state, slot);
    {
        let mut windows = state.windows.lock_or_recover();
        let Some(w) = windows.get_mut(slot) else {
            return;
        };
        if w.popup || w.layer {
            return;
        }
        if w.maximized {
            if let Some(sv) = w.saved.take() {
                w.x = sv.x;
                w.y = sv.y;
                w.width = sv.width;
                w.height = sv.height;
            }
            w.maximized = false;
        } else {
            if !w.fullscreen {
                w.saved = Some(SavedGeom {
                    x: w.x,
                    y: w.y,
                    width: w.width,
                    height: w.height,
                });
            }
            w.maximized = true;
            w.fullscreen = false;
            w.minimized = false;
            w.x = ox;
            w.y = oy;
            w.width = ow as i32;
            w.height = (oh - TITLEBAR).max(MIN_H) as i32;
        }
    }
    let (nw, nh) = window_geometry(state, slot)
        .map(|(_, _, w, h)| (w, h))
        .unwrap_or((ow, oh));
    send_configure(state, slot, nw, nh);
    crate::foreign_toplevel::broadcast_states(state);
}

/// Przełącza pełny ekran (ukrywa dekorację).
fn toggle_fullscreen(state: &mut State, slot: usize) {
    let (ox, oy, ow, oh) = window_output_rect(state, slot);
    {
        let mut windows = state.windows.lock_or_recover();
        let Some(w) = windows.get_mut(slot) else {
            return;
        };
        if w.popup || w.layer {
            return;
        }
        if w.fullscreen {
            if let Some(sv) = w.saved.take() {
                w.x = sv.x;
                w.y = sv.y;
                w.width = sv.width;
                w.height = sv.height;
            }
            w.fullscreen = false;
        } else {
            if !w.maximized {
                w.saved = Some(SavedGeom {
                    x: w.x,
                    y: w.y,
                    width: w.width,
                    height: w.height,
                });
            }
            w.fullscreen = true;
            w.maximized = false;
            w.minimized = false;
            w.x = ox;
            w.y = oy;
            w.width = ow as i32;
            w.height = oh as i32;
        }
    }
    let (nw, nh) = window_geometry(state, slot)
        .map(|(_, _, w, h)| (w, h))
        .unwrap_or((ow, oh));
    send_configure(state, slot, nw, nh);
    crate::foreign_toplevel::broadcast_states(state);
}

/// Zdejmuje fokus klawiatury/wskaźnika z okna (np. po minimalizacji).
fn clear_focus_if(state: &mut State, slot: usize) {
    if state.keyboard_focus.as_ref().map(|(s, _)| *s) == Some(slot) {
        let kb = state.keyboard.clone();
        if let (Some((_, surface)), Some(kb)) = (state.keyboard_focus.clone(), kb) {
            let serial = state.next_serial();
            let _ = kb.send_event(wl_keyboard::Event::Leave { serial, surface });
        }
        state.keyboard_focus = None;
    }
    if state.pointer_focus.as_ref().map(|(s, _)| *s) == Some(slot) {
        pointer_leave(state);
    }
    crate::foreign_toplevel::broadcast_states(state);
}

/// Prosi klienta o zamknięcie okna (`xdg_toplevel.close`).
fn send_close(state: &mut State, slot: usize) {
    let Some(surface) = state.slots.get(slot).cloned() else {
        return;
    };
    let Some(sd) = surface.data::<SurfaceData>() else {
        return;
    };
    let Some(xs) = sd.xdg_surface.lock_or_recover().clone() else {
        return;
    };
    if let Some(xd) = xs.data::<XdgSurfaceData>() {
        if let Some(tl) = xd.toplevel.lock_or_recover().as_ref() {
            let _ = tl.send_event(xdg_toplevel::Event::Close);
        }
    }
}

fn raise_and_focus(state: &mut State, slot: usize) {
    let z = state.next_z();
    {
        let mut windows = state.windows.lock_or_recover();
        if let Some(w) = windows.get_mut(slot) {
            // Powierzchnie warstwowe (panele/docki) trzymają stałą kolejność.
            if !w.layer {
                w.z = z;
            }
        }
    }
    set_keyboard_focus(state, slot);
    crate::foreign_toplevel::broadcast_states(state);
}

/// Aktywuje okno o podanym slocie (z `zwlr_foreign_toplevel_handle_v1.activate`).
pub fn activate_slot(state: &mut State, slot: usize) {
    raise_and_focus(state, slot);
}

/// Zamyka okno reprezentowane przez `surface` (z `...handle_v1.close`).
pub fn close_surface(state: &mut State, surface: &wayland_server::protocol::wl_surface::WlSurface) {
    if let Some(sd) = surface.data::<SurfaceData>() {
        if let Some(slot) = *sd.slot.lock_or_recover() {
            send_close(state, slot);
        }
    }
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
    let kb = state.keyboard.clone();
    let Some(kb) = kb else { return };
    sync_modifiers(state, &kb, keysym, st);
    if is_modifier_keysym(keysym) {
        return;
    }
    // Skróty menedżera okien (Super + …) przechwytujemy przed klientem.
    if state.sent_mods.logo && window_shortcut(state, keysym, st) {
        return;
    }
    if state.keyboard_focus.is_none() {
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

/// Skróty menedżera okien z modyfikatorem Super. Zwraca `true`, gdy obsłużono.
fn window_shortcut(state: &mut State, keysym: u32, st: ButtonState) -> bool {
    if st != ButtonState::Pressed {
        // Zjedz także zwolnienia naszych klawiszy, by nie trafiły do klienta.
        return matches!(
            keysym,
            0x006d | 0x0066 | 0x006c | 0x004c | 0xff52 | 0xff54 | 0xff09
        );
    }
    let focused = state.keyboard_focus.as_ref().map(|(s, _)| *s);
    match keysym {
        0x006d => {
            // Super+M — minimalizuj/przywróć.
            if let Some(s) = focused {
                toggle_minimize(state, s);
            }
            true
        }
        0x0066 => {
            // Super+F — pełny ekran.
            if let Some(s) = focused {
                toggle_fullscreen(state, s);
            }
            true
        }
        0x006c | 0x004c => {
            // Super+L — wyloguj. Kończymy kompozytor (sesję), więc menedżer logowania
            // wraca do greetera; jądro zwalnia mastera DRM przy zamknięciu deskryptorów.
            println!("{}", libluna_i18n::tr("logging out"));
            std::process::exit(0);
        }
        0xff52 => {
            // Super+Up — maksymalizuj/przywróć.
            if let Some(s) = focused {
                toggle_maximize(state, s);
            }
            true
        }
        0xff54 => {
            // Super+Down — przywróć z maksymalizacji/fullscreenu.
            if let Some(s) = focused {
                restore_window(state, s);
            }
            true
        }
        0xff09 => {
            // Super+Tab — przełącz okna (przywraca zminimalizowane).
            cycle_windows(state);
            true
        }
        _ => false,
    }
}

/// Przywraca okno z minimalizacji/maksymalizacji/fullscreenu.
fn restore_window(state: &mut State, slot: usize) {
    let (min, max, full) = {
        let windows = state.windows.lock_or_recover();
        match windows.get(slot) {
            Some(w) => (w.minimized, w.maximized, w.fullscreen),
            None => return,
        }
    };
    if min {
        toggle_minimize(state, slot);
    } else if full {
        toggle_fullscreen(state, slot);
    } else if max {
        toggle_maximize(state, slot);
    }
}

/// Przełącza fokus na kolejne okno (przywracając je, jeśli było zminimalizowane).
fn cycle_windows(state: &mut State) {
    let slots: Vec<usize> = {
        let windows = state.windows.lock_or_recover();
        windows
            .iter()
            .enumerate()
            .filter(|(_, w)| w.mapped && !w.popup && !w.layer)
            .map(|(i, _)| i)
            .collect()
    };
    if slots.is_empty() {
        return;
    }
    let cur = state.keyboard_focus.as_ref().map(|(s, _)| *s);
    let next = match cur.and_then(|c| slots.iter().position(|&s| s == c)) {
        Some(p) => slots[(p + 1) % slots.len()],
        None => slots[0],
    };
    if let Some(w) = state.windows.lock_or_recover().get_mut(next) {
        w.minimized = false;
    }
    raise_and_focus(state, next);
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
            minimized: false,
            maximized: false,
            fullscreen: false,
            saved: None,
            data: Arc::new(ImageData::empty(200, 120)),
        }
    }

    #[test]
    fn window_controls_are_laid_out() {
        let w = sample_window();
        // Pasek tytułu na górze, treść pod nim.
        assert_eq!(w.titlebar().bottom(), w.y + TITLEBAR);
        assert_eq!(w.content().y(), w.y + TITLEBAR);
        // Zamknij w prawym górnym narożniku.
        let c = w.close_rect();
        assert!(c.contains(Point::new(w.x + 200.0 - 1.0, w.y + 1.0)));
        // Maksymalizuj po lewej od zamknij, minimalizuj po lewej od maksymalizuj.
        let m = w.maximize_rect();
        let n = w.minimize_rect();
        assert!(m.right() <= c.x() + 0.01);
        assert!(n.right() <= m.x() + 0.01);
    }

    #[test]
    fn resize_edges_detect_corners() {
        let w = sample_window();
        let o = w.outer();
        // Lewy-górny narożnik → góra + lewo.
        assert_eq!(
            w.resize_edges(Point::new(o.x() + 1.0, o.y() + 1.0)),
            EDGE_TOP | EDGE_LEFT
        );
        // Prawy-dolny narożnik → dół + prawo.
        assert_eq!(
            w.resize_edges(Point::new(o.right() - 1.0, o.bottom() - 1.0)),
            EDGE_BOTTOM | EDGE_RIGHT
        );
        // Środek → brak krawędzi.
        assert_eq!(w.resize_edges(o.center()), 0);
    }

    #[test]
    fn fullscreen_hides_titlebar() {
        let mut w = sample_window();
        w.fullscreen = true;
        assert_eq!(w.titlebar().bottom(), w.y);
        assert_eq!(w.content().y(), w.y);
    }

    #[test]
    fn keymap_defaults_to_us_layout() {
        let km = keymap_text_for("us", None);
        assert!(km.contains("xkb_keymap"));
        assert!(km.contains("pc+us+inet(evdev)"));
    }

    #[test]
    fn keymap_supports_variants_and_sanitizes_input() {
        assert!(keymap_text_for("pl", Some("legacy")).contains("pc+pl(legacy)+inet(evdev)"));
        // Znaki spoza zestawu XKB są odfiltrowane (bezpieczeństwo).
        assert!(keymap_text_for("u; rm -rf", None).contains("pc+urm-rf+inet(evdev)"));
    }

    #[test]
    fn empty_layout_falls_back_to_us() {
        assert!(keymap_text_for("!!!", None).contains("pc+us+inet(evdev)"));
    }
}
