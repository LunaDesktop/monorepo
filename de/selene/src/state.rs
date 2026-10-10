//! Globalny stan kompozytora Selene.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use libluna_core::{Modifiers, Rect};
use libluna_render::ImageData;
use wayland_server::protocol::wl_keyboard::WlKeyboard;
use wayland_server::protocol::wl_pointer::WlPointer;
use wayland_server::protocol::wl_surface::WlSurface;

use crate::monitors::{OutputInfo, Outputs};

/// Wysokość paska tytułu dekoracji (SSD), w pikselach logicznych.
pub const TITLEBAR: f64 = 30.0;
/// Bok uchwytu przeskalowania w prawym-dolnym narożniku.
pub const GRIP: f64 = 16.0;

/// Okno (toplevel) w modelu kompozycji: pozycja, rozmiar, kolejność i gotowe piksele.
pub struct Window {
    pub x: f64,
    pub y: f64,
    /// Rozmiar **treści** (bufora klienta).
    pub width: i32,
    pub height: i32,
    /// Kolejność (wyższa = wyżej); do renderu i trafień.
    pub z: i32,
    /// Tytuł okna (z `xdg_toplevel.set_title`).
    pub title: String,
    /// Piksele (premultiplied RGBA8) ostatnio zacommitowanego bufora.
    pub data: Arc<ImageData>,
}

impl Window {
    /// Prostokąt całego okna (dekoracja + treść).
    pub fn outer(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y,
            self.width as f64,
            TITLEBAR + self.height as f64,
        )
    }

    /// Pasek tytułu (górna część dekoracji).
    pub fn titlebar(&self) -> Rect {
        Rect::from_xywh(self.x, self.y, self.width as f64, TITLEBAR)
    }

    /// Obszar treści (klienta).
    pub fn content(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y + TITLEBAR,
            self.width as f64,
            self.height as f64,
        )
    }

    /// Kwadratowy przycisk zamknięcia w prawym górnym narożniku.
    pub fn close_rect(&self) -> Rect {
        Rect::from_xywh(
            self.x + self.width as f64 - TITLEBAR,
            self.y,
            TITLEBAR,
            TITLEBAR,
        )
    }

    /// Uchwyt przeskalowania (prawy-dolny narożnik treści).
    pub fn grip_rect(&self) -> Rect {
        let c = self.content();
        Rect::from_xywh(c.right() - GRIP, c.bottom() - GRIP, GRIP, GRIP)
    }
}

/// Współdzielony model okien (pisany przez wątek protokołu, czytany przez wyjście).
pub type WindowModel = Arc<Mutex<Vec<Window>>>;

/// Trwające przeciąganie okna.
#[derive(Debug, Clone, Copy)]
pub struct DragState {
    pub slot: usize,
    /// Przesunięcie kursora względem lewego-górnego narożnika okna w chwili wciśnięcia.
    pub off_x: f64,
    pub off_y: f64,
}

/// Trwające przeskalowanie okna (uchwyt w prawym-dolnym narożniku).
#[derive(Debug, Clone, Copy)]
pub struct ResizeState {
    pub slot: usize,
    pub start_w: f64,
    pub start_h: f64,
    pub start_px: f64,
    pub start_py: f64,
}

/// Stan współdzielony przez wszystkie handlery protokołu.
pub struct State {
    /// Start pracy kompozytora — do znaczników czasu (frame callbacks).
    pub start: Instant,
    /// Licznik seriali (dla `xdg_surface.configure` itp.).
    serial: u32,
    /// Liczniki diagnostyczne.
    pub clients: u32,
    pub surfaces: u32,
    pub toplevels: u32,
    /// Model okien współdzielony z wątkiem wyjścia.
    pub windows: WindowModel,
    /// Powierzchnie okien (indeks = slot w [`State::windows`]).
    pub slots: Vec<WlSurface>,
    /// Licznik kolejności (z-order).
    z_counter: i32,
    /// Monitory (współdzielone z wątkiem wyjścia).
    pub outputs: Outputs,

    // — Wejście —
    pub pointer: Option<WlPointer>,
    pub keyboard: Option<WlKeyboard>,
    pub pointer_focus: Option<(usize, WlSurface)>,
    pub keyboard_focus: Option<(usize, WlSurface)>,
    /// Modyfikatory, o których już powiadomiliśmy klienta klawiatury.
    pub sent_mods: Modifiers,
    /// Trwające przeciąganie okna.
    pub drag: Option<DragState>,
    /// Trwające przeskalowanie okna.
    pub resize: Option<ResizeState>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            serial: 0,
            clients: 0,
            surfaces: 0,
            toplevels: 0,
            windows: Arc::new(Mutex::new(Vec::new())),
            slots: Vec::new(),
            z_counter: 1,
            outputs: Arc::new(Mutex::new(vec![OutputInfo::default()])),
            pointer: None,
            keyboard: None,
            pointer_focus: None,
            keyboard_focus: None,
            sent_mods: Modifiers::NONE,
            drag: None,
            resize: None,
        }
    }
}

impl State {
    /// Kolejny serial protokołu.
    pub fn next_serial(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.serial
    }

    /// Czas od startu w milisekundach (dla `wl_callback.done`).
    pub fn time_ms(&self) -> u32 {
        self.start.elapsed().as_millis() as u32
    }

    /// Kolejna wartość z-order (podnosi okno na wierzch).
    pub fn next_z(&mut self) -> i32 {
        self.z_counter += 1;
        self.z_counter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serials_are_non_zero_and_increasing() {
        let mut s = State::default();
        let a = s.next_serial();
        let b = s.next_serial();
        assert!(a >= 1);
        assert!(b > a);
    }

    #[test]
    fn z_order_increases() {
        let mut s = State::default();
        let a = s.next_z();
        let b = s.next_z();
        assert!(b > a);
    }
}
