//! `FocusModel` + pierścień fokusu — wspólna nawigacja klawiaturą.
//!
//! W modelu immediate aplikacja zna kolejność kontrolek, a nie ich identyfikatory.
//! Wystarczy raz na klatkę zgłosić liczbę elementów, a potem pytać
//! [`FocusModel::is_focused`]. `Tab`/`Shift+Tab` obsługuje model; `Enter`/spacja
//! aktywują element (patrz [`is_activate_key`]).
//!
//! Fokus jest opcjonalny: dopóki użytkownik nie użyje `Tab`, `current()` zwraca
//! `None`, więc mysz i skróty aplikacji działają jak wcześniej.
//!
//! ```no_run
//! # use libluna_widgets::{FocusModel, paint_focus_ring};
//! # use libluna_core::{KeyEvent, Rect};
//! # use libluna_widgets::PaintCtx;
//! # fn demo(focus: &mut FocusModel, key: &KeyEvent, ctx: &mut PaintCtx, rect: Rect) {
//! focus.set_count(3);              // kolejność: przycisk, pole, przełącznik
//! if focus.handle_key(key) { /* zużyto Tab */ }
//! if focus.is_focused(0) {
//!     paint_focus_ring(ctx, rect, 8.0);
//! }
//! # }
//! ```

use libluna_core::{Insets, KeyEvent, Rect};

use crate::ctx::PaintCtx;

/// Klawisz Tab (XKB).
const KEY_TAB: u32 = 0xff09;

/// Model fokusu klawiaturowego dla listy elementów w kolejności Tab.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FocusModel {
    count: usize,
    current: Option<usize>,
}

impl FocusModel {
    pub const fn new() -> Self {
        Self {
            count: 0,
            current: None,
        }
    }

    /// Liczba elementów w bieżącej klatce. Wołaj przed obsługą klawiatury.
    pub fn set_count(&mut self, count: usize) {
        self.count = count;
        match self.current {
            Some(index) if index >= count => {
                self.current = count.checked_sub(1);
            }
            _ => {}
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// Indeks fokusu albo `None`, gdy użytkownik jeszcze nie użył klawiatury.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn is_focused(&self, index: usize) -> bool {
        self.current == Some(index) && index < self.count
    }

    /// Ustawia fokus na elemencie (np. po kliknięciu myszą).
    pub fn focus(&mut self, index: usize) {
        if index < self.count {
            self.current = Some(index);
        }
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    /// Następny element (zawija); z `None` zaczyna od pierwszego.
    pub fn next(&mut self) {
        if self.count == 0 {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            None => 0,
            Some(index) => (index + 1) % self.count,
        });
    }

    /// Poprzedni element (zawija); z `None` zaczyna od ostatniego.
    pub fn prev(&mut self) {
        if self.count == 0 {
            self.current = None;
            return;
        }
        self.current = Some(match self.current {
            None => self.count - 1,
            Some(index) => (index + self.count - 1) % self.count,
        });
    }

    /// Obsługuje `Tab` / `Shift+Tab`; zwraca `true`, jeśli klawisz zużyto.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        if key.keysym != KEY_TAB {
            return false;
        }
        if key.modifiers.shift {
            self.prev();
        } else {
            self.next();
        }
        true
    }
}

/// Czy klawisz aktywuje fokusowany element (`Enter` / `KP_Enter` / spacja).
pub fn is_activate_key(key: &KeyEvent) -> bool {
    matches!(key.keysym, 0xff0d | 0xff8d | 0x20)
}

/// Rysuje pierścień fokusu wokół kontrolki.
///
/// Grubość i odstęp pochodzą z `theme.metrics.focus` (wysoki kontrast pogrubia
/// obwódkę), a kolor z akcentu motywu.
pub fn paint_focus_ring(ctx: &mut PaintCtx, rect: Rect, radius: f64) {
    let theme = *ctx.theme;
    let metrics = theme.metrics.focus;
    let ring = rect.inflate(Insets::splat(metrics.offset));
    ctx.scene.stroke_rounded_rect(
        ring,
        (radius + metrics.offset).max(0.0),
        metrics.width.max(1.0),
        theme.colors.accent,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Modifiers;

    fn key(keysym: u32, shift: bool) -> KeyEvent {
        KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: libluna_core::ButtonState::Pressed,
            modifiers: Modifiers {
                shift,
                ..Modifiers::NONE
            },
            repeat: false,
        }
    }

    #[test]
    fn focus_is_absent_until_tab() {
        let mut focus = FocusModel::new();
        focus.set_count(3);
        assert_eq!(focus.current(), None);
        assert!(!focus.is_focused(0));
        assert!(focus.handle_key(&key(KEY_TAB, false)));
        assert_eq!(focus.current(), Some(0));
        assert!(focus.is_focused(0));
    }

    #[test]
    fn tab_wraps_in_both_directions() {
        let mut focus = FocusModel::new();
        focus.set_count(3);
        focus.next();
        focus.next();
        focus.next();
        focus.next();
        assert_eq!(focus.current(), Some(0));
        focus.prev();
        assert_eq!(focus.current(), Some(2));
    }

    #[test]
    fn shrinking_the_list_keeps_focus_valid() {
        let mut focus = FocusModel::new();
        focus.set_count(4);
        focus.focus(3);
        focus.set_count(2);
        assert_eq!(focus.current(), Some(1));
        focus.set_count(0);
        assert_eq!(focus.current(), None);
        assert!(!focus.is_focused(0));
    }

    #[test]
    fn other_keys_are_not_consumed() {
        let mut focus = FocusModel::new();
        focus.set_count(2);
        assert!(!focus.handle_key(&key(0xff52, false)));
        assert!(!focus.handle_key(&key(0xff0d, false)));
        assert!(is_activate_key(&key(0xff0d, false)));
        assert!(is_activate_key(&key(0x20, false)));
    }
}
