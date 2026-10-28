//! Przyciski GTK4: `ToggleButton`, `LinkButton`, `ColorButton`.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Przycisk przełączany (pozostaje „wciśnięty”).
#[derive(Debug, Clone, Default)]
pub struct ToggleButton {
    label: String,
    active: bool,
    hovered: bool,
}

impl ToggleButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            active: false,
            hovered: false,
        }
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    /// Przełącza stan; zwraca nowy stan.
    pub fn click_at(&mut self, rect: Rect, p: Point) -> bool {
        if rect.contains(p) {
            self.active = !self.active;
        }
        self.active
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let bg = if self.active {
            colors.accent
        } else if self.hovered {
            colors.shade.scale_alpha(1.8)
        } else {
            colors.shade
        };
        ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
        let fg = if self.active {
            colors.accent_fg
        } else {
            colors.text
        };
        ctx.label_center(
            rect.center().x,
            rect.center().y,
            &self.label,
            theme.typography.body.size,
            weight::MEDIUM,
            fg,
        );
    }
}

/// Przycisk z odnośnikiem (kolor akcentu + podkreślenie).
#[derive(Debug, Clone, Default)]
pub struct LinkButton {
    label: String,
}

impl LinkButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let size = theme.typography.body.size;
        let text_w = ctx.measure(&self.label, size, weight::NORMAL);
        let x = rect.center().x - text_w / 2.0;
        ctx.label_left(
            x,
            rect.center().y,
            &self.label,
            size,
            weight::NORMAL,
            theme.colors.accent,
        );
        let underline_y = rect.center().y + size * 0.55;
        ctx.scene.stroke_line(
            Point::new(x, underline_y),
            Point::new(x + text_w, underline_y),
            1.0,
            theme.colors.accent,
        );
    }
}

/// Przycisk koloru (próbka koloru); otwiera wybór koloru po stronie aplikacji.
#[derive(Debug, Clone, Copy)]
pub struct ColorButton {
    color: Color,
}

impl Default for ColorButton {
    fn default() -> Self {
        Self {
            color: Color::from_rgba8(0x35, 0x84, 0xe4, 0xff),
        }
    }
}

impl ColorButton {
    pub fn new(color: Color) -> Self {
        Self { color }
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let swatch = rect.inflate(libluna_core::Insets::splat(-2.0));
        ctx.scene
            .fill_rounded_rect(swatch, theme.radius.small - 2.0, theme.colors.shade);
        ctx.scene.fill_rounded_rect(
            rect.deflate(libluna_core::Insets::splat(2.0)),
            theme.radius.small - 2.0,
            self.color,
        );
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.small, 1.0, theme.colors.border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_button_flips() {
        let mut b = ToggleButton::new("Bold");
        let rect = Rect::from_xywh(0.0, 0.0, 80.0, 34.0);
        assert!(!b.is_active());
        assert!(b.click_at(rect, rect.center()));
        assert!(!b.click_at(rect, Rect::ZERO.center()));
    }

    #[test]
    fn color_button_roundtrip() {
        let mut b = ColorButton::default();
        let c = Color::from_rgba8(0x10, 0x20, 0x30, 0xff);
        b.set_color(c);
        assert_eq!(b.color(), c);
    }

    #[test]
    fn link_button_label() {
        assert_eq!(LinkButton::new("Docs").label(), "Docs");
    }
}
