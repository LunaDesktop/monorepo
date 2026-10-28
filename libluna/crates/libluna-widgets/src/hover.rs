//! Śledzenie kursora i spójne podświetlanie elementów pod wskaźnikiem.
//!
//! Panel i launcher rysują się „ręcznie”, więc nie mają drzewa widgetów, które
//! samo pamiętałoby hover. Ten typ daje jedno miejsce prawdy: aplikacja podaje mu
//! zdarzenia (`handle_event`), a rysowanie pyta `is_hovered`/`index`.
//!
//! Dla widgetów ze stanem (`set_hovered`) jest [`Hoverable`] i [`Hover::apply`].

use libluna_core::{Color, Event, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Pozycja kursora na powierzchni (albo brak, gdy wskaźnik ją opuścił).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hover {
    position: Option<Point>,
}

impl Hover {
    pub const fn new() -> Self {
        Self { position: None }
    }

    pub fn set_position(&mut self, position: Point) {
        self.position = Some(position);
    }

    pub fn clear(&mut self) {
        self.position = None;
    }

    pub fn position(&self) -> Option<Point> {
        self.position
    }

    /// Aktualizuje pozycję na podstawie zdarzenia wejścia.
    ///
    /// Zwraca `true`, gdy pozycja faktycznie się zmieniła (przydatne do decyzji
    /// o przerysowaniu). `PointerLeave` czyści hover.
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event {
            Event::PointerMotion { position, .. } => {
                if self.position != Some(*position) {
                    self.position = Some(*position);
                    true
                } else {
                    false
                }
            }
            Event::PointerLeave => {
                let had = self.position.is_some();
                self.clear();
                had
            }
            _ => false,
        }
    }

    pub fn is_hovered(&self, rect: Rect) -> bool {
        self.position.is_some_and(|p| rect.contains(p))
    }

    /// Trafienie z marginesem — dla małych elementów (kropki, przełączniki),
    /// które mają powiększony obszar klikalny.
    pub fn is_hovered_within(&self, rect: Rect, margin: f64) -> bool {
        let Some(p) = self.position else {
            return false;
        };
        p.x >= rect.x() - margin
            && p.x <= rect.right() + margin
            && p.y >= rect.y() - margin
            && p.y <= rect.bottom() + margin
    }

    /// Indeks pierwszego prostokąta pod kursorem (np. kafelka albo wiersza menu).
    pub fn index(&self, rects: &[Rect]) -> Option<usize> {
        let p = self.position?;
        rects.iter().position(|r| r.contains(p))
    }

    /// Rysuje standardowe podświetlenie, jeśli `rect` jest pod kursorem.
    /// Zwraca `true`, gdy coś narysowano.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, radius: f64) -> bool {
        if !self.is_hovered(rect) {
            return false;
        }
        ctx.scene
            .fill_rounded_rect(rect, radius, hover_fill(ctx.theme));
        true
    }

    /// Ustawia stan hover na widgecie implementującym [`Hoverable`].
    pub fn apply<W: Hoverable>(&self, rect: Rect, widget: &mut W) {
        widget.set_hovered(self.is_hovered(rect));
    }
}

/// Standardowy kolor podświetlenia — ten sam, którego używają widgety libLuny.
pub fn hover_fill(theme: &Theme) -> Color {
    theme.colors.shade.scale_alpha(1.8)
}

/// Widget, który potrafi pokazać stan „pod kursorem”.
pub trait Hoverable {
    fn set_hovered(&mut self, hovered: bool);
}

impl Hoverable for crate::button::Button {
    fn set_hovered(&mut self, hovered: bool) {
        crate::button::Button::set_hovered(self, hovered);
    }
}

impl Hoverable for crate::menu_button::MenuButton {
    fn set_hovered(&mut self, hovered: bool) {
        crate::menu_button::MenuButton::set_hovered(self, hovered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(10.0, 10.0, 100.0, 40.0)
    }

    #[test]
    fn motion_and_leave_update_position() {
        let mut hover = Hover::new();
        assert!(!hover.handle_event(&Event::PointerLeave));
        assert!(hover.handle_event(&Event::PointerMotion {
            position: Point::new(20.0, 20.0),
            modifiers: libluna_core::Modifiers::NONE,
        }));
        assert_eq!(hover.position(), Some(Point::new(20.0, 20.0)));
        // Ten sam punkt nie jest „zmianą”.
        assert!(!hover.handle_event(&Event::PointerMotion {
            position: Point::new(20.0, 20.0),
            modifiers: libluna_core::Modifiers::NONE,
        }));
        assert!(hover.handle_event(&Event::PointerLeave));
        assert_eq!(hover.position(), None);
    }

    #[test]
    fn hit_tests_and_margin() {
        let mut hover = Hover::new();
        hover.set_position(Point::new(15.0, 15.0));
        assert!(hover.is_hovered(rect()));
        assert!(hover.is_hovered_within(rect(), 10.0));

        // Punkt 5 px od krawędzi: margines 2 px go nie łapie, 10 px — tak.
        hover.set_position(Point::new(5.0, 5.0));
        assert!(!hover.is_hovered(rect()));
        assert!(!hover.is_hovered_within(rect(), 2.0));
        assert!(hover.is_hovered_within(rect(), 10.0));

        let rects = [
            Rect::from_xywh(0.0, 0.0, 10.0, 10.0),
            Rect::from_xywh(10.0, 10.0, 100.0, 40.0),
        ];
        hover.set_position(Point::new(15.0, 15.0));
        assert_eq!(hover.index(&rects), Some(1));

        hover.clear();
        assert!(!hover.is_hovered(rect()));
        assert_eq!(hover.index(&rects), None);
    }
}
