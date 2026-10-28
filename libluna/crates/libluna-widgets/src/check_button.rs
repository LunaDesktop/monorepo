//! `CheckButton` — pole wyboru (checkbox) z etykietą.
//!
//! Widget immediate: trzyma stan zaznaczenia. Aplikacja woła [`CheckButton::paint`] oraz
//! [`CheckButton::click_at`].

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Pole wyboru.
#[derive(Debug, Clone, Default)]
pub struct CheckButton {
    label: String,
    checked: bool,
    hovered: bool,
}

impl CheckButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            ..Self::default()
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    /// Prostokąt kwadratu (checkbox) po lewej stronie wiersza.
    pub fn box_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let size = theme.metrics.check.size;
        Rect::from_xywh(rect.x(), rect.center().y - size / 2.0, size, size)
    }

    pub fn hit(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Przełącza zaznaczenie, jeśli kliknięto w obszar. Zwraca `true` przy trafieniu.
    pub fn click_at(&mut self, rect: Rect, p: Point) -> bool {
        if rect.contains(p) {
            self.checked = !self.checked;
            true
        } else {
            false
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let radius = theme.radius.small;
        let b = self.box_rect(rect, &theme);

        if self.checked {
            ctx.scene.fill_rounded_rect(b, radius, colors.accent);
            // „Ptaszek”.
            let s = b.width();
            ctx.scene.stroke_line(
                Point::new(b.x() + s * 0.24, b.y() + s * 0.54),
                Point::new(b.x() + s * 0.43, b.y() + s * 0.72),
                2.0,
                colors.accent_fg,
            );
            ctx.scene.stroke_line(
                Point::new(b.x() + s * 0.43, b.y() + s * 0.72),
                Point::new(b.x() + s * 0.78, b.y() + s * 0.28),
                2.0,
                colors.accent_fg,
            );
        } else {
            let fill = if self.hovered {
                colors.shade.scale_alpha(1.8)
            } else {
                colors.shade
            };
            ctx.scene.fill_rounded_rect(b, radius, fill);
            ctx.scene.stroke_rounded_rect(b, radius, 1.0, colors.border);
        }

        ctx.label_left(
            b.right() + theme.metrics.check.gap,
            rect.center().y,
            &self.label,
            theme.typography.body.size,
            weight::NORMAL,
            colors.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 34.0)
    }

    #[test]
    fn click_toggles_only_inside() {
        let mut c = CheckButton::new("Pokazuj ukryte");
        assert!(!c.checked());
        assert!(c.click_at(rect(), Point::new(10.0, 17.0)));
        assert!(c.checked());
        assert!(!c.click_at(rect(), Point::new(500.0, 17.0)));
        assert!(c.checked());
    }

    #[test]
    fn box_is_left_aligned_and_centered() {
        let theme = Theme::light();
        let c = CheckButton::new("X");
        let b = c.box_rect(rect(), &theme);
        assert_eq!(b.x(), rect().x());
        assert!((b.center().y - rect().center().y).abs() < 1e-9);
        assert!((b.width() - theme.metrics.check.size).abs() < 1e-9);
    }
}
