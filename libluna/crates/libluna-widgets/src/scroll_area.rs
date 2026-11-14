//! `ScrollArea` — stan przewijania + obsługa wskaźnika i klawiatury w jednym miejscu.
//!
//! Aplikacje w modelu immediate muszą wiedzieć, kiedy klik trafił w pasek (a nie
//! w treść) i kiedy przesunięcie się zmieniło. Wcześniej każda aplikacja robiła to
//! sama (własna flaga `scroll_dragging`, własny `begin_drag`/`drag_to` w trzech
//! miejscach). [`ScrollArea`] spina to w jedną maszynę stanu:
//!
//! ```no_run
//! # use libluna_widgets::{ScrollArea, ScrollDrag};
//! # use libluna_core::{Event, Theme, Rect};
//! # fn demo(area: &mut ScrollArea, event: &Event, theme: &Theme, viewport: Rect) {
//! area.configure(viewport, 1200.0); // przed obsługą zdarzeń
//! match area.handle_pointer(event, theme) {
//!     ScrollDrag::Captured => { /* nie klikaj w treść */ }
//!     ScrollDrag::Moved => { /* zsynchronizuj offset, jeśli trzymasz go osobno */ }
//!     ScrollDrag::Ignored => { /* zwykła obsługa treści */ }
//! }
//! # }
//! ```
//!
//! [`ScrollArea`] dereferencjonuje do [`ScrollView`], więc `offset()`,
//! `set_offset()`, `paint_scrollbar()` i reszta API działają bez zmian.

use std::ops::{Deref, DerefMut};

use libluna_core::{ButtonState, Event, Point, PointerButton, Rect, Theme};

use crate::scroll_view::ScrollView;

/// Krok rolki w pikselach — wspólny dla wszystkich aplikacji.
pub const SCROLL_STEP: f64 = 64.0;

/// Wynik obsługi zdarzenia wskaźnika.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDrag {
    /// Zdarzenie nie dotyczy paska przewijania.
    Ignored,
    /// Przeciąganie przechwycone (start/koniec) — aplikacja nie powinna klikać treści.
    Captured,
    /// Przesunięcie się zmieniło — aplikacja ma zsynchronizować własny stan.
    Moved,
}

/// Obszar przewijania: [`ScrollView`] + zapamiętany viewport + stan przeciągania.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollArea {
    view: ScrollView,
    viewport: Rect,
    dragging: bool,
}

impl ScrollArea {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ustawia viewport i wysokość treści przed obsługą zdarzeń.
    ///
    /// Wołaj na początku klatki/obsługi zdarzeń — dzięki temu pasek i trafienia
    /// liczą się z aktualną geometrią (także po zmianie rozmiaru okna).
    pub fn configure(&mut self, viewport: Rect, content_height: f64) {
        self.viewport = viewport;
        self.view.set_content_height(content_height);
        self.view.set_offset(self.view.offset(), viewport);
    }

    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Wciśnięcie lewego przycisku: chwyt za kciuk albo skok w torze.
    pub fn press(&mut self, p: Point, theme: &Theme) -> ScrollDrag {
        if self.view.begin_drag(p, self.viewport, theme) {
            self.dragging = true;
            ScrollDrag::Captured
        } else {
            ScrollDrag::Ignored
        }
    }

    /// Ruch wskaźnika podczas przeciągania.
    pub fn drag(&mut self, p: Point, theme: &Theme) -> ScrollDrag {
        if !self.dragging {
            return ScrollDrag::Ignored;
        }
        self.view.drag_to(p, self.viewport, theme);
        ScrollDrag::Moved
    }

    /// Zwolnienie lewego przycisku.
    pub fn release(&mut self) -> ScrollDrag {
        if !self.dragging {
            return ScrollDrag::Ignored;
        }
        self.view.end_drag();
        self.dragging = false;
        ScrollDrag::Captured
    }

    /// Wygodny wariant dla pętli zdarzeń: wciśnięcie/ruch/zwolnienie.
    pub fn handle_pointer(&mut self, event: &Event, theme: &Theme) -> ScrollDrag {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                self.press(button.position, theme)
            }
            Event::PointerMotion { position, .. } => self.drag(*position, theme),
            Event::PointerButton(button)
                if button.button == PointerButton::Left
                    && button.state == ButtonState::Released =>
            {
                self.release()
            }
            _ => ScrollDrag::Ignored,
        }
    }

    /// Rolka nad obszarem; zwraca `true`, gdy przesunięcie się zmieniło.
    pub fn handle_wheel(&mut self, event: &Event) -> bool {
        let Event::Scroll(scroll) = event else {
            return false;
        };
        if !self.viewport.contains(scroll.position) {
            return false;
        }
        self.view
            .on_scroll(scroll.notches_y() * SCROLL_STEP, self.viewport)
    }

    /// PageUp/PageDown/Home/End; zwraca `true`, gdy klawisz został zużyty.
    ///
    /// Strzałki zostają aplikacjom — w listach często poruszają zaznaczeniem,
    /// a nie przewijaniem.
    pub fn handle_key(&mut self, keysym: u32) -> bool {
        let page = (self.viewport.height() * 0.85).max(1.0);
        match keysym {
            0xff55 => {
                self.view.on_scroll(-page, self.viewport);
                true
            }
            0xff56 => {
                self.view.on_scroll(page, self.viewport);
                true
            }
            0xff50 => {
                self.view.set_offset(0.0, self.viewport);
                true
            }
            0xff57 => {
                self.view
                    .set_offset(self.view.max_offset(self.viewport), self.viewport);
                true
            }
            _ => false,
        }
    }
}

impl Deref for ScrollArea {
    type Target = ScrollView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

impl DerefMut for ScrollArea {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Point;

    fn area() -> (ScrollArea, Rect) {
        let viewport = Rect::from_xywh(0.0, 0.0, 200.0, 100.0);
        let mut area = ScrollArea::new();
        area.configure(viewport, 400.0);
        (area, viewport)
    }

    fn press_event(p: Point) -> Event {
        Event::PointerButton(libluna_core::PointerEvent {
            position: p,
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: libluna_core::Modifiers::default(),
        })
    }

    fn release_event(p: Point) -> Event {
        Event::PointerButton(libluna_core::PointerEvent {
            position: p,
            button: PointerButton::Left,
            state: ButtonState::Released,
            modifiers: libluna_core::Modifiers::default(),
        })
    }

    #[test]
    fn drag_through_events_scrolls_to_the_end() {
        let theme = Theme::light();
        let (mut area, viewport) = area();
        let thumb = area.thumb_rect(viewport, &theme).expect("kciuk");

        assert_eq!(
            area.handle_pointer(&press_event(thumb.center()), &theme),
            ScrollDrag::Captured
        );
        assert!(area.is_dragging());
        let end = Point::new(thumb.center().x, viewport.bottom() - 1.0);
        assert_eq!(
            area.handle_pointer(
                &Event::PointerMotion {
                    position: end,
                    modifiers: libluna_core::Modifiers::default(),
                },
                &theme
            ),
            ScrollDrag::Moved
        );
        assert_eq!(area.offset(), area.max_offset(viewport));
        assert_eq!(
            area.handle_pointer(&release_event(end), &theme),
            ScrollDrag::Captured
        );
        assert!(!area.is_dragging());
    }

    #[test]
    fn click_outside_the_bar_is_ignored() {
        let theme = Theme::light();
        let (mut area, viewport) = area();
        let _ = viewport;
        assert_eq!(
            area.handle_pointer(&press_event(Point::new(20.0, 20.0)), &theme),
            ScrollDrag::Ignored
        );
        assert!(!area.is_dragging());
    }

    #[test]
    fn wheel_and_page_keys_change_the_offset() {
        let (mut area, viewport) = area();
        let wheel = Event::Scroll(libluna_core::ScrollEvent {
            position: Point::new(50.0, 50.0),
            dx: 0.0,
            dy: 50.0,
        });
        assert!(area.handle_wheel(&wheel));
        assert!(area.offset() > 0.0);

        assert!(area.handle_key(0xff50));
        assert_eq!(area.offset(), 0.0);
        assert!(area.handle_key(0xff57));
        assert_eq!(area.offset(), area.max_offset(viewport));
        assert!(area.handle_key(0xff55));
        assert!(area.offset() < area.max_offset(viewport));
    }
}
