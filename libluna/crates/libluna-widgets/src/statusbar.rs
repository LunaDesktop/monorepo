//! `Statusbar` — pasek stanu (GTK4): komunikat po lewej, informacje po prawej.
//!
//! Widget immediate: trzyma dwa napisy (lewy komunikat i prawy opis), liczy geometrię
//! i rysuje tło z górną krawędzią. Aplikacja wpisuje tam np. „Gotowy” i liczbę elementów.

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Pasek stanu.
#[derive(Debug, Clone, Default)]
pub struct Statusbar {
    message: String,
    right: String,
}

impl Statusbar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn right(&self) -> &str {
        &self.right
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    pub fn set_right(&mut self, right: impl Into<String>) {
        self.right = right.into();
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.typography.caption.line_height + theme.spacing.xs * 2.0
    }

    /// Prostokąty: `[lewy komunikat, prawy opis]` wewnątrz `rect`.
    pub fn parts(&self, rect: Rect, theme: &Theme) -> [Rect; 2] {
        let pad = theme.spacing.md;
        let h = rect.height();
        let w = (rect.width() - 2.0 * pad).max(0.0) / 2.0;
        [
            Rect::from_xywh(rect.x() + pad, rect.y(), w, h),
            Rect::from_xywh(rect.right() - pad - w, rect.y(), w, h),
        ]
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        ctx.scene.fill_rect(rect, theme.colors.window_bg);
        let line = Rect::from_xywh(rect.x(), rect.y(), rect.width(), 1.0);
        ctx.scene.fill_rect(line, theme.colors.border);

        let [left, right] = self.parts(rect, &theme);
        let size = theme.typography.caption.size;
        ctx.label_left(
            left.x(),
            left.center().y,
            &self.message,
            size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        ctx.label_right(
            right.right(),
            right.center().y,
            &self.right,
            size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_from_caption_metrics() {
        let theme = Theme::light();
        let s = Statusbar::new();
        assert!(
            (s.height(&theme) - (theme.typography.caption.line_height + theme.spacing.xs * 2.0))
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn parts_are_padded_start_and_end() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 24.0);
        let mut s = Statusbar::new();
        s.set_message("Gotowy");
        s.set_right("4 elementy");
        assert_eq!(s.message(), "Gotowy");
        assert_eq!(s.right(), "4 elementy");
        let [l, r] = s.parts(rect, &theme);
        assert_eq!(l.x(), theme.spacing.md);
        assert!((r.right() - (400.0 - theme.spacing.md)).abs() < 1e-9);
    }
}
