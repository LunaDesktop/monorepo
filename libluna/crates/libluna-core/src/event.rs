//! Zdarzenia wejścia i okna — wspólny format dla całej reszty toolkitu.
//!
//! Warstwa platformy tłumaczy zdarzenia Waylanda na [`Event`]; widgety konsumują
//! je w jednym drzewie i zwracają, czy zdarzenie zostało obsłużone.

use crate::geometry::{Point, Size};

/// Klawisz myszy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Middle,
    Right,
    Back,
    Forward,
    Other(u32),
}

/// Stan wciśnięcia (dla przycisków i klawiszy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

/// Modyfikatory klawiatury.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    /// AltGr / `ISO_Level3_Shift` — poziom 3 układu (m.in. polskie ą, ć, ę, ł, ń, ó, ś, ź, ż).
    pub level3: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
        level3: false,
    };
}

/// Zdarzenie przycisku wskaźnika.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub position: Point,
    pub button: PointerButton,
    pub state: ButtonState,
    pub modifiers: Modifiers,
}

/// Przewinięcie jednego ząbka kółka myszy w pikselach logicznych.
///
/// [`ScrollEvent::dx`]/[`ScrollEvent::dy`] są **zawsze w pikselach**: warstwa
/// platformy zamienia `axis_discrete`/`axis_value120` na `ząbki ×`
/// [`SCROLL_NOTCH_PX`], a gładzik przekazuje `axis` bez zmian. Dzięki temu
/// aplikacje nie mnożą już surowych zdarzeń przez wysokość wiersza.
pub const SCROLL_NOTCH_PX: f64 = 64.0;

/// Standardowy krok `wl_pointer.axis` dla jednego ząbka rolki (w pikselach).
///
/// Tyle wysyłają kompozytory zgodne z libinput/wlroots. Klienty zewnętrzne
/// (GTK, Chromium, XWayland) zakładają właśnie taki rząd wielkości — wysłanie
/// 64 px na ząbek powodowało u nich przeskok (XWayland dzieli wartość przez 10
/// i generuje kilka kliknięć rolki). Wewnątrz libLuny platforma skaluje ten
/// krok do [`SCROLL_NOTCH_PX`].
pub const SCROLL_WHEEL_AXIS_PX: f64 = 15.0;

/// Zdarzenie przewijania (rolka/gładzik); wartości w pikselach logicznych.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollEvent {
    pub position: Point,
    pub dx: f64,
    pub dy: f64,
}

impl ScrollEvent {
    /// Przewinięcie w poziomie wyrażone w „ząbkach” kółka (1.0 = jeden ząbek).
    ///
    /// Używać, gdy aplikacja chce przewinąć stałą liczbę wierszy na ząbek;
    /// do przewijania w pikselach wystarczy [`ScrollEvent::dx`].
    pub fn notches_x(&self) -> f64 {
        self.dx / SCROLL_NOTCH_PX
    }

    /// Przewinięcie w pionie wyrażone w „ząbkach” kółka (1.0 = jeden ząbek).
    pub fn notches_y(&self) -> f64 {
        self.dy / SCROLL_NOTCH_PX
    }
}

/// Zdarzenie klawiatury.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyEvent {
    /// Keysym w rozumieniu XKB.
    pub keysym: u32,
    /// Fizyczny kod klawisza (Linux input `KEY_*`, protokół Wayland `wl_keyboard.key`).
    ///
    /// Protokół przekazuje **kod fizyczny**, a klient sam liczy z niego znak według
    /// swojej mapy klawiszy — dlatego nie wolno tu wstawiać kodu X11 (`evdev + 8`).
    pub keycode: u32,
    /// Tekst wygenerowany przez klawisz (jeśli dotyczy).
    pub text: Option<String>,
    pub state: ButtonState,
    pub modifiers: Modifiers,
    /// Czy zdarzenie zostało powtórzone przez auto-repeat.
    pub repeat: bool,
}

/// Zdarzenia cyklu życia okna.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEvent {
    /// Nowy rozmiar w pikselach logicznych.
    Resize(Size),
    /// Skala ekranu uległa zmianie.
    ScaleChanged(f64),
    /// Uzyskano/utracono fokus.
    Focused(bool),
    /// Użytkownik (lub kompozytor) poprosił o zamknięcie okna.
    CloseRequested,
    /// Należy przerysować zawartość.
    Redraw,
}

/// Zunifikowane zdarzenie trafiające do drzewa widgetów.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    PointerButton(PointerEvent),
    /// Ruch wskaźnika; `position` w pikselach logicznych.
    PointerMotion {
        position: Point,
        modifiers: Modifiers,
    },
    /// Wskaźnik opuścił powierzchnię.
    PointerLeave,
    Scroll(ScrollEvent),
    Key(KeyEvent),
    /// Wprowadzony tekst (po złożeniu IME).
    Text(String),
    /// Tekst wklejony ze schowka systemowego (odpowiedź na `Frame::paste_from_clipboard`).
    Paste(String),
    /// Przeciąganie z innej aplikacji jest nad oknem (`position` w pikselach logicznych).
    ///
    /// `move_hint` mówi, że odbiorca może przenieść pliki zamiast je kopiować
    /// (akcja wybrana przez kompozytor na podstawie źródła i modyfikatorów).
    DragHover {
        position: Point,
        move_hint: bool,
    },
    /// Przeciąganie opuściło okno albo zostało anulowane.
    DragLeave,
    /// Upuszczono pliki z innej aplikacji.
    Drop {
        position: Point,
        paths: Vec<std::path::PathBuf>,
        /// `true` = kopiuj, `false` = przenieś.
        copy: bool,
    },
    Window(WindowEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_event_is_copy() {
        let e = PointerEvent {
            position: Point::new(1.0, 2.0),
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        };
        let copy = e;
        assert_eq!(e, copy);
    }
}
