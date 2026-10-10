//! `MenuButton` — przycisk otwierający popover (GTK4).
//!
//! Warianty: z etykietą (+ opcjonalny chevron) albo ikoną „hamburger” (`☰`).
//! Widget immediate: trzyma stan otwarcia i wygląda jak płaski przycisk.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;

/// Przycisk otwierający menu/popover.
#[derive(Debug, Clone, Default)]
pub struct MenuButton {
    label: Option<String>,
    burger: bool,
    chevron: bool,
    open: bool,
    hovered: bool,
}

impl MenuButton {
    /// Przycisk z etykietą (z chevronem rozwijania).
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: Some(label.into()),
            chevron: true,
            ..Self::default()
        }
    }

    /// Przycisk ikonowy „hamburger” (`☰`) — np. menu główne.
    pub fn burger() -> Self {
        Self {
            burger: true,
            ..Self::default()
        }
    }

    /// Włącza/wyłącza chevron przy etykiecie.
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Przełącza stan; zwraca nowy stan.
    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.open
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    pub fn width(&self, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.button;
        if self.burger || self.label.is_none() {
            return self.height(theme).max(30.0);
        }
        let label = self.label.as_deref().unwrap_or("");
        let body = theme.typography.body.size;
        let mut w = m.pad_x * 2.0 + text.measure(label, body, weight::NORMAL).width;
        if self.chevron {
            w += m.chevron_gap + m.chevron;
        }
        w
    }

    /// Zwraca `true`, jeśli klik trafił (i przełączył stan).
    pub fn click_at(&mut self, rect: Rect, p: Point) -> bool {
        if rect.contains(p) {
            self.toggle();
            true
        } else {
            false
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.button;
        if self.hovered || self.open {
            let bg = if self.open {
                theme.colors.shade.scale_alpha(1.8)
            } else {
                theme.colors.shade
            };
            ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
        }
        let fg = theme.colors.text;
        let cy = rect.center().y;

        if self.burger {
            icon::burger(ctx, fg, rect.center(), 7.0, 1.8);
            return;
        }

        let label = self.label.as_deref().unwrap_or("");
        let body = theme.typography.body.size;
        let label_w = ctx.measure(label, body, weight::NORMAL);
        let content_w = label_w
            + if self.chevron {
                m.chevron_gap + m.chevron
            } else {
                0.0
            };
        let start_x = rect.center().x - content_w / 2.0;
        ctx.label_left(start_x, cy, label, body, weight::NORMAL, fg);
        if self.chevron {
            let cx = start_x + label_w + m.chevron_gap + m.chevron / 2.0;
            icon::chevron_down(ctx, fg, Point::new(cx, cy), 4.0, 2.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burger_is_square_and_small() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let b = MenuButton::burger();
        assert_eq!(b.width(&text, &theme), b.height(&theme).max(30.0));
    }

    #[test]
    fn labelled_button_grows_with_chevron() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let plain = MenuButton::new("Opcje").chevron(false);
        let with = MenuButton::new("Opcje");
        assert!(with.width(&text, &theme) > plain.width(&text, &theme));
    }

    #[test]
    fn click_toggles_only_inside() {
        let rect = Rect::from_xywh(0.0, 0.0, 44.0, 34.0);
        let mut b = MenuButton::burger();
        assert!(!b.is_open());
        assert!(b.click_at(rect, rect.center()));
        assert!(b.is_open());
        assert!(!b.click_at(rect, Point::new(200.0, 200.0)));
        assert!(b.is_open());
        assert!(b.click_at(rect, rect.center()));
        assert!(!b.is_open());
    }
}
