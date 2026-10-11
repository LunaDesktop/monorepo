//! Wejście z urządzeń `evdev` (`/dev/input/event*`) — rodzime wejście dla trybu DRM.
//!
//! W trybie nested wejście przychodzi od nadrzędnego kompozytora. Gdy Selene działa
//! bezpośrednio na DRM/KMS, nie ma rodzica — wtedy czytamy zdarzenia wprost z urządzeń
//! wejściowych i tłumaczymy je na [`libluna_core::Event`] (ten sam strumień, który
//! trafia do `seat::handle_input`).
//!
//! Zakres: wskaźnik (względny), przyciski myszy, kółko i klawiatura. Śledzimy wirtualny
//! kursor w kompozytorze (evdev daje przemieszczenia, nie pozycję bezwzględną).

use std::os::fd::{AsRawFd, OwnedFd};

use libluna_core::{
    ButtonState, Event, KeyEvent, Modifiers, Point, PointerButton, PointerEvent, ScrollEvent,
};
use rustix::fs::{Mode, OFlags};

// Typy zdarzeń evdev.
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const SYN_REPORT: u16 = 0;
const REL_HWHEEL: u16 = 0x06;
const REL_WHEEL: u16 = 0x08;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;

/// `struct input_event` (64-bit): `timeval` (2×i64) + typ + kod + wartość.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawEvent {
    time_sec: i64,
    time_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

/// Jedno urządzenie wejściowe.
struct Device {
    fd: OwnedFd,
    #[allow(dead_code)]
    path: String,
}

/// Wejście evdev: zestaw urządzeń + wirtualny kursor.
pub struct EvdevInput {
    devices: Vec<Device>,
    cursor: Point,
    acc_dx: f64,
    acc_dy: f64,
    acc_scroll_x: f64,
    acc_scroll_y: f64,
}

impl EvdevInput {
    /// Otwiera wszystkie czytelne urządzenia `/dev/input/event*`.
    /// Zwraca `None`, gdy nie ma dostępnych urządzeń (np. brak uprawnień).
    pub fn open() -> Option<Self> {
        let dir = std::fs::read_dir("/dev/input").ok()?;
        let mut paths: Vec<String> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().starts_with("event"))
                    .unwrap_or(false)
            })
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        paths.sort();

        let mut devices = Vec::new();
        for path in paths {
            let Ok(fd) = rustix::fs::open(
                path.as_str(),
                OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            ) else {
                continue;
            };
            devices.push(Device { fd, path });
        }
        if devices.is_empty() {
            return None;
        }
        Some(Self {
            devices,
            cursor: Point::new(0.0, 0.0),
            acc_dx: 0.0,
            acc_dy: 0.0,
            acc_scroll_x: 0.0,
            acc_scroll_y: 0.0,
        })
    }

    /// Surowe deskryptory do `poll` (do wykrywania gotowości).
    pub fn fds(&self) -> Vec<std::os::fd::RawFd> {
        self.devices.iter().map(|d| d.fd.as_raw_fd()).collect()
    }

    /// Bieżąca pozycja wirtualnego kursora (współrzędne logiczne).
    pub fn cursor(&self) -> Point {
        self.cursor
    }

    /// Czyta i tłumaczy wszystkie oczekujące zdarzenia.
    ///
    /// `bounds` to rozmiar monitora — kursor jest do niego ograniczany.
    pub fn read_events(&mut self, bounds: (f64, f64)) -> Vec<Event> {
        // Najpierw odczyt (pożyczka współdzielona urządzeń), potem tłumaczenie (mutacja).
        let mut decoded = Vec::new();
        for dev in &self.devices {
            let mut buf = [0u8; std::mem::size_of::<RawEvent>() * 64];
            loop {
                match rustix::io::read(&dev.fd, &mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for chunk in buf[..n].chunks_exact(std::mem::size_of::<RawEvent>()) {
                            if let Some(ev) = decode(chunk) {
                                decoded.push(ev);
                            }
                        }
                    }
                    Err(_) => break, // EAGAIN lub błąd — kończymy na tym urządzeniu
                }
            }
        }
        let mut out = Vec::new();
        for ev in decoded {
            self.apply(ev, bounds, &mut out);
        }
        out
    }

    fn apply(&mut self, ev: Decoded, bounds: (f64, f64), out: &mut Vec<Event>) {
        match ev {
            Decoded::Motion(dx, dy) => {
                self.acc_dx += dx;
                self.acc_dy += dy;
            }
            Decoded::Scroll(dx, dy) => {
                self.acc_scroll_x += dx;
                self.acc_scroll_y += dy;
            }
            Decoded::Button(button, pressed) => {
                let state = if pressed {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                out.push(Event::PointerButton(PointerEvent {
                    position: self.cursor,
                    button,
                    state,
                    modifiers: Modifiers::NONE,
                }));
            }
            Decoded::Key(keysym, pressed) => {
                let state = if pressed {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                out.push(Event::Key(KeyEvent {
                    keysym,
                    text: None,
                    state,
                    modifiers: Modifiers::NONE,
                    repeat: false,
                }));
            }
            Decoded::Syn => {
                if self.acc_dx != 0.0 || self.acc_dy != 0.0 {
                    self.cursor = Point::new(
                        (self.cursor.x + self.acc_dx).clamp(0.0, (bounds.0 - 1.0).max(0.0)),
                        (self.cursor.y + self.acc_dy).clamp(0.0, (bounds.1 - 1.0).max(0.0)),
                    );
                    self.acc_dx = 0.0;
                    self.acc_dy = 0.0;
                    out.push(Event::PointerMotion {
                        position: self.cursor,
                        modifiers: Modifiers::NONE,
                    });
                }
                if self.acc_scroll_x != 0.0 || self.acc_scroll_y != 0.0 {
                    out.push(Event::Scroll(ScrollEvent {
                        position: self.cursor,
                        dx: self.acc_scroll_x,
                        dy: -self.acc_scroll_y,
                    }));
                    self.acc_scroll_x = 0.0;
                    self.acc_scroll_y = 0.0;
                }
            }
        }
    }
}

/// Zdarzenie po przetłumaczeniu z bajtów.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Decoded {
    Motion(f64, f64),
    Scroll(f64, f64),
    Button(PointerButton, bool),
    Key(u32, bool),
    Syn,
}

/// Dekoduje jeden `input_event`.
fn decode(bytes: &[u8]) -> Option<Decoded> {
    if bytes.len() < std::mem::size_of::<RawEvent>() {
        return None;
    }
    let ev = RawEvent {
        time_sec: i64::from_ne_bytes(bytes[0..8].try_into().ok()?),
        time_usec: i64::from_ne_bytes(bytes[8..16].try_into().ok()?),
        type_: u16::from_ne_bytes(bytes[16..18].try_into().ok()?),
        code: u16::from_ne_bytes(bytes[18..20].try_into().ok()?),
        value: i32::from_ne_bytes(bytes[20..24].try_into().ok()?),
    };
    match ev.type_ {
        EV_SYN if ev.code == SYN_REPORT => Some(Decoded::Syn),
        EV_REL => match ev.code {
            REL_X => Some(Decoded::Motion(ev.value as f64, 0.0)),
            REL_Y => Some(Decoded::Motion(0.0, ev.value as f64)),
            REL_HWHEEL => Some(Decoded::Scroll(ev.value as f64, 0.0)),
            REL_WHEEL => Some(Decoded::Scroll(0.0, ev.value as f64)),
            _ => None,
        },
        EV_KEY => {
            let pressed = ev.value != 0;
            match ev.code {
                0x110 => Some(Decoded::Button(PointerButton::Left, pressed)),
                0x111 => Some(Decoded::Button(PointerButton::Right, pressed)),
                0x112 => Some(Decoded::Button(PointerButton::Middle, pressed)),
                0x115 => Some(Decoded::Button(PointerButton::Forward, pressed)),
                0x116 => Some(Decoded::Button(PointerButton::Back, pressed)),
                code => evdev_to_keysym(code).map(|ks| Decoded::Key(ks, pressed)),
            }
        }
        _ => None,
    }
}

/// Kod evdev → keysym (podzbiór wspólny z mapą klawiatury Selene).
fn evdev_to_keysym(code: u16) -> Option<u32> {
    // Litery (evdev: a=30 … l=38, m=50 …; użyto standardowego układu evdev).
    let keysym = match code {
        1 => 0xff1b,   // Escape
        14 => 0xff08,  // BackSpace
        15 => 0xff09,  // Tab
        28 => 0xff0d,  // Return
        57 => 0x0020,  // space
        103 => 0xff52, // Up
        105 => 0xff51, // Left
        106 => 0xff53, // Right
        108 => 0xff54, // Down
        110 => 0xff50, // Home
        111 => 0xffff, // Delete
        115 => 0xff57, // End
        // cyfry 1..0
        2 => 0x0031,
        3 => 0x0032,
        4 => 0x0033,
        5 => 0x0034,
        6 => 0x0035,
        7 => 0x0036,
        8 => 0x0037,
        9 => 0x0038,
        10 => 0x0039,
        11 => 0x0030,
        // litery
        30 => 0x0061, // a
        48 => 0x0062, // b
        46 => 0x0063, // c
        32 => 0x0064, // d
        18 => 0x0065, // e
        33 => 0x0066, // f
        34 => 0x0067, // g
        35 => 0x0068, // h
        23 => 0x0069, // i
        36 => 0x006a, // j
        37 => 0x006b, // k
        38 => 0x006c, // l
        50 => 0x006d, // m
        49 => 0x006e, // n
        24 => 0x006f, // o
        25 => 0x0070, // p
        16 => 0x0071, // q
        19 => 0x0072, // r
        31 => 0x0073, // s
        20 => 0x0074, // t
        22 => 0x0075, // u
        47 => 0x0076, // v
        17 => 0x0077, // w
        45 => 0x0078, // x
        21 => 0x0079, // y
        44 => 0x007a, // z
        // modyfikatory
        42 | 54 => 0xffe1,   // Shift
        29 | 97 => 0xffe3,   // Control
        56 | 100 => 0xffe9,  // Alt
        125 | 126 => 0xffeb, // Super
        _ => return None,
    };
    Some(keysym)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(type_: u16, code: u16, value: i32) -> [u8; 24] {
        let mut b = [0u8; 24];
        b[16..18].copy_from_slice(&type_.to_ne_bytes());
        b[18..20].copy_from_slice(&code.to_ne_bytes());
        b[20..24].copy_from_slice(&value.to_ne_bytes());
        b
    }

    #[test]
    fn raw_event_layout_matches_kernel() {
        assert_eq!(std::mem::size_of::<RawEvent>(), 24);
    }

    #[test]
    fn decodes_button_and_key_and_rel() {
        assert_eq!(
            decode(&raw(EV_KEY, 0x110, 1)),
            Some(Decoded::Button(PointerButton::Left, true))
        );
        assert_eq!(
            decode(&raw(EV_KEY, 30, 0)),
            Some(Decoded::Key(0x0061, false))
        );
        assert_eq!(
            decode(&raw(EV_REL, REL_X, 5)),
            Some(Decoded::Motion(5.0, 0.0))
        );
        assert_eq!(decode(&raw(EV_SYN, SYN_REPORT, 0)), Some(Decoded::Syn));
        assert_eq!(decode(&raw(EV_KEY, 0x2ff, 1)), None); // nieznany klawisz
    }

    #[test]
    fn accumulates_motion_and_emits_on_syn() {
        let mut input = EvdevInput {
            devices: Vec::new(),
            cursor: Point::new(100.0, 100.0),
            acc_dx: 0.0,
            acc_dy: 0.0,
            acc_scroll_x: 0.0,
            acc_scroll_y: 0.0,
        };
        let mut out = Vec::new();
        input.apply(Decoded::Motion(10.0, 5.0), (1280.0, 800.0), &mut out);
        assert!(out.is_empty()); // jeszcze bez SYN
        input.apply(Decoded::Syn, (1280.0, 800.0), &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            matches!(out[0], Event::PointerMotion { position, .. } if position.x == 110.0 && position.y == 105.0)
        );
    }

    #[test]
    fn cursor_is_clamped_to_bounds() {
        let mut input = EvdevInput {
            devices: Vec::new(),
            cursor: Point::new(1270.0, 0.0),
            acc_dx: 0.0,
            acc_dy: 0.0,
            acc_scroll_x: 0.0,
            acc_scroll_y: 0.0,
        };
        let mut out = Vec::new();
        input.apply(Decoded::Motion(1000.0, -50.0), (1280.0, 800.0), &mut out);
        input.apply(Decoded::Syn, (1280.0, 800.0), &mut out);
        assert!(
            matches!(out[0], Event::PointerMotion { position, .. } if position.x == 1279.0 && position.y == 0.0)
        );
    }
}
