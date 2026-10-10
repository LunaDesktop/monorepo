//! Globalny stan kompozytora Selene.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use libluna_render::ImageData;

/// Okno (toplevel) w modelu kompozycji: pozycja, rozmiar i gotowe piksele.
pub struct Window {
    pub x: f64,
    pub y: f64,
    pub width: i32,
    pub height: i32,
    /// Piksele (premultiplied RGBA8) ostatnio zacommitowanego bufora.
    pub data: Arc<ImageData>,
}

/// Współdzielony model okien (pisany przez wątek protokołu, czytany przez wyjście).
pub type WindowModel = Arc<Mutex<Vec<Window>>>;

/// Stan współdzielony przez wszystkie handlery protokołu.
pub struct State {
    /// Start pracy kompozytora — do znaczników czasu (frame callbacks).
    pub start: Instant,
    /// Licznik seriali (dla `xdg_surface.configure` itp.).
    serial: u32,
    /// Liczniki diagnostyczne (log startowy / debug).
    pub clients: u32,
    pub surfaces: u32,
    pub toplevels: u32,
    /// Model okien współdzielony z wątkiem wyjścia.
    pub windows: WindowModel,
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
    fn time_ms_is_monotonic() {
        let s = State::default();
        let t0 = s.time_ms();
        let t1 = s.time_ms();
        assert!(t1 >= t0);
    }
}
