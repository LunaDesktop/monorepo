//! `SplitButton` — przycisk dzielony: akcja główna + osobna część ze strzałką (menu).
//!
//! Widget immediate: [`SplitButton::rects`] dzieli prostokąt na część główną i strzałkę,
//! [`SplitButton::main_at`] / [`SplitButton::arrow_at`] obsługują kliknięcia.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Przycisk dzielony.
#[derive(Debug, Clone, Default)]
pub struct SplitButton {
    label: String,
    enabled: bool,
}

/// Podział przycisku na część główną i strzałkę.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitRects {
    pub main: Rect,
    pub arrow: Rect,
}

impl SplitButton {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            enabled: true,
        }
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    /// Podział prostokąta (część główna z lewej, strzałka z prawej).
    pub fn rects(&self, rect: Rect, theme: &Theme) -> SplitRects {
        let arrow_w = theme.metrics.split.arrow_w;
        SplitRects {
            main: Rect::from_xywh(
                rect.x(),
                rect.y(),
                (rect.width() - arrow_w).max(0.0),
                rect.height(),
            ),
            arrow: Rect::from_xywh(rect.right() - arrow_w, rect.y(), arrow_w, rect.height()),
        }
    }

    pub fn main_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.enabled && self.rects(rect, theme).main.contains(p)
    }

    pub fn arrow_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.enabled && self.rects(rect, theme).arrow.contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let r = self.rects(rect, &theme);
        let fg = if self.enabled {
            colors.accent_fg
        } else {
            colors.dim_text
        };

        ctx.scene
            .fill_rounded_rect(rect, theme.radius.small, colors.accent);
        // Separator między akcją a strzałką.
        ctx.scene.fill_rect(
            Rect::from_xywh(
                r.main.right(),
                rect.y() + 5.0,
                theme.metrics.split.divider,
                (rect.height() - 10.0).max(0.0),
            ),
            colors.accent_fg.scale_alpha(0.35),
        );

        ctx.label_center(
            r.main.center().x,
            r.main.center().y,
            &self.label,
            theme.typography.body.size,
            weight::MEDIUM,
            fg,
        );
        icon::chevron_down(ctx, fg, r.arrow.center(), 4.0, 2.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rects_partition() {
        let theme = Theme::light();
        let b = SplitButton::new("Save");
        let rect = Rect::from_xywh(0.0, 0.0, 160.0, 34.0);
        let r = b.rects(rect, &theme);
        assert!((r.main.right() - r.arrow.x()).abs() < 1e-9);
        assert!((r.arrow.width() - theme.metrics.split.arrow_w).abs() < 1e-9);
        assert!((r.arrow.right() - rect.right()).abs() < 1e-9);
    }

    #[test]
    fn hit_test_main_and_arrow() {
        let theme = Theme::light();
        let b = SplitButton::new("Save");
        let rect = Rect::from_xywh(0.0, 0.0, 160.0, 34.0);
        let r = b.rects(rect, &theme);
        assert!(b.main_at(rect, &theme, r.main.center()));
        assert!(b.arrow_at(rect, &theme, r.arrow.center()));
        assert!(!b.main_at(rect, &theme, r.arrow.center()));
        assert!(!SplitButton::new("x")
            .enabled(false)
            .main_at(rect, &theme, r.main.center()));
    }
}
