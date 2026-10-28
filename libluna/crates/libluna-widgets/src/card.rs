//! `Card` — karta (powierzchnia podniesiona) z nagłówkiem i obszarem treści.
//!
//! Widget immediate: rysuje tło, obramowanie, nagłówek i separator, a [`Card::paint`] zwraca
//! obszar zawartości, w którym aplikacja umieszcza własne wiersze.

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Karta z nagłówkiem.
#[derive(Debug, Clone, Default)]
pub struct Card {
    title: String,
}

impl Card {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Wysokość nagłówka (z metryk).
    pub fn header_height(&self, theme: &Theme) -> f64 {
        theme.metrics.card.header
    }

    /// Obszar zawartości (poniżej nagłówka i separatora).
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let h = self.header_height(theme);
        Rect::from_xywh(
            rect.x(),
            rect.y() + h,
            rect.width(),
            (rect.height() - h).max(0.0),
        )
    }

    /// Rysuje kartę; zwraca obszar zawartości.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) -> Rect {
        let theme = *ctx.theme;
        ctx.scene
            .fill_rounded_rect(rect, theme.radius.large, theme.colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.large, 1.0, theme.colors.border);

        let h = self.header_height(&theme);
        ctx.label_left(
            rect.x() + theme.metrics.card.pad,
            rect.y() + h / 2.0,
            &self.title,
            theme.typography.body.size,
            weight::BOLD,
            theme.colors.text,
        );
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.x(), rect.y() + h, rect.width(), 1.0),
            theme.colors.border,
        );
        self.content_rect(rect, &theme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_is_below_header() {
        let theme = Theme::light();
        let card = Card::new("Wygląd");
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let content = card.content_rect(rect, &theme);
        assert!((content.y() - (rect.y() + theme.metrics.card.header)).abs() < 1e-9);
        assert!((content.height() - (200.0 - theme.metrics.card.header)).abs() < 1e-9);
    }

    #[test]
    fn header_height_from_metrics() {
        let theme = Theme::light();
        assert_eq!(
            Card::new("X").header_height(&theme),
            theme.metrics.card.header
        );
    }
}
