//! Globalny stan kompozytora Selene.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use libluna_core::{Modifiers, Point, Rect};
use libluna_render::ImageData;
use wayland_server::protocol::wl_data_device::WlDataDevice;
use wayland_server::protocol::wl_data_source::WlDataSource;
use wayland_server::protocol::wl_keyboard::WlKeyboard;
use wayland_server::protocol::wl_pointer::WlPointer;
use wayland_server::protocol::wl_surface::WlSurface;

use crate::monitors::{OutputInfo, Outputs};

/// Wysokość paska tytułu dekoracji (SSD), w pikselach logicznych.
pub const TITLEBAR: f64 = 30.0;
/// Szerokość przycisku sterowania oknem (minimalizuj/maksymalizuj/zamknij).
pub const BUTTON_W: f64 = 30.0;
/// Grubość krawędzi wyłapywanej do zmiany rozmiaru.
pub const BORDER: f64 = 6.0;

/// Zapamiętana geometria przed maksymalizacją/fullscreenem (do przywrócenia).
#[derive(Debug, Clone, Copy)]
pub struct SavedGeom {
    pub x: f64,
    pub y: f64,
    pub width: i32,
    pub height: i32,
}

/// Okno (toplevel/popup) w modelu kompozycji: pozycja, rozmiar, kolejność i piksele.
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
    /// Czy to `xdg_popup` (bez dekoracji, zamykany kliknięciem poza).
    pub popup: bool,
    /// Czy to powierzchnia `wlr-layer-shell` (panel/dock; bez dekoracji, kotwiczona).
    pub layer: bool,
    /// Czy powierzchnia jest widoczna (popup znika po `popup_done`).
    pub mapped: bool,
    /// Zminimalizowane (ukryte przez kompozytor; przywracane skrótem).
    pub minimized: bool,
    /// Zmaksymalizowane do obszaru monitora.
    pub maximized: bool,
    /// Pełny ekran.
    pub fullscreen: bool,
    /// Geometria zapamiętana przed maksymalizacją/fullscreenem.
    pub saved: Option<SavedGeom>,
    /// Piksele (premultiplied RGBA8) ostatnio zacommitowanego bufora.
    pub data: Arc<ImageData>,
}

impl Window {
    /// Wysokość dekoracji (zero dla popupów, paneli i pełnego ekranu).
    fn bar(&self) -> f64 {
        if self.popup || self.layer || self.fullscreen {
            0.0
        } else {
            TITLEBAR
        }
    }

    /// Prostokąt całego okna (dekoracja + treść).
    pub fn outer(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y,
            self.width as f64,
            self.bar() + self.height as f64,
        )
    }

    /// Pasek tytułu (zerowy dla popupów).
    pub fn titlebar(&self) -> Rect {
        Rect::from_xywh(self.x, self.y, self.width as f64, self.bar())
    }

    /// Obszar treści (klienta).
    pub fn content(&self) -> Rect {
        Rect::from_xywh(
            self.x,
            self.y + self.bar(),
            self.width as f64,
            self.height as f64,
        )
    }

    /// Przycisk sterowania oknem licząc od prawej (0 = zamknij, 1 = maksymalizuj, 2 = minimalizuj).
    fn control_rect(&self, from_right: usize) -> Rect {
        if self.popup || self.layer {
            return Rect::ZERO;
        }
        Rect::from_xywh(
            self.x + self.width as f64 - BUTTON_W * (from_right as f64 + 1.0),
            self.y,
            BUTTON_W,
            TITLEBAR,
        )
    }

    /// Kwadratowy przycisk zamknięcia (tylko dla topleveli).
    pub fn close_rect(&self) -> Rect {
        self.control_rect(0)
    }

    /// Przycisk maksymalizacji.
    pub fn maximize_rect(&self) -> Rect {
        self.control_rect(1)
    }

    /// Przycisk minimalizacji.
    pub fn minimize_rect(&self) -> Rect {
        self.control_rect(2)
    }

    /// Bitmaska krawędzi wyłapywanej do zmiany rozmiaru dla punktu (0 gdy brak).
    /// 1 = góra, 2 = dół, 4 = lewo, 8 = prawo.
    pub fn resize_edges(&self, p: Point) -> u8 {
        if self.popup || self.layer || self.fullscreen || self.maximized {
            return 0;
        }
        let o = self.outer();
        if !o.contains(p) {
            return 0;
        }
        let mut e = 0u8;
        if p.x <= o.x() + BORDER {
            e |= 4;
        }
        if p.x >= o.right() - BORDER {
            e |= 8;
        }
        if p.y <= o.y() + BORDER {
            e |= 1;
        }
        if p.y >= o.bottom() - BORDER {
            e |= 2;
        }
        e
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

/// Trwające przeskalowanie okna (za dowolną krawędź/narożnik).
#[derive(Debug, Clone, Copy)]
pub struct ResizeState {
    pub slot: usize,
    /// Bitmaska krawędzi (1=góra, 2=dół, 4=lewo, 8=prawo).
    pub edges: u8,
    pub start_x: f64,
    pub start_y: f64,
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
    /// Bieżąca selekcja (schowek): źródło danych.
    pub selection: Option<WlDataSource>,
    /// Urządzenia schowka zarejestrowane przez klientów (do rozsyłania selekcji).
    pub data_devices: Vec<WlDataDevice>,
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
            selection: None,
            data_devices: Vec::new(),
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
