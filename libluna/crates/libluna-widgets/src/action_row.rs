//! `ActionRow` — wiersz ustawień: tytuł (i podtytuł) z lewej, obszar na kontrolkę z prawej.
//!
//! Widget immediate: rysuje tekst wiersza i zwraca [`Rect`] na kontrolkę (`suffix`), którą
//! umieszcza aplikacja. Padding pochodzi z tokenów motywu ([`ActionRowMetrics`](libluna_core::ActionRowMetrics)).

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Wiersz akcji (ustawienie).
#[derive(Debug, Clone, Default)]
pub struct ActionRow {
    title: String,
    subtitle: Option<String>,
}

impl ActionRow {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn subtitle_text(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    /// Obszar na kontrolkę (prawa strona) o zadanej szerokości.
    pub fn suffix_rect(&self, rect: Rect, theme: &Theme, suffix_width: f64) -> Rect {
        let pad = theme.metrics.row.pad_x;
        Rect::from_xywh(
            rect.right() - pad - suffix_width,
            rect.y(),
            suffix_width,
            rect.height(),
        )
    }

    /// Rysuje tytuł (i podtytuł); zwraca obszar na kontrolkę.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, suffix_width: f64) -> Rect {
        let theme = *ctx.theme;
        let pad = theme.metrics.row.pad_x;
        let x = rect.x() + pad;
        match &self.subtitle {
            Some(subtitle) => {
                // Odstęp zależy od rozmiaru tekstu — przy skali dostępności (150 %)
                // stały `gap` powodowałby nachodzenie tytułu na podtytuł.
                let title_size = theme.typography.body.size;
                let subtitle_size = theme.typography.caption.size;
                let gap = theme
                    .metrics
                    .row
                    .gap
                    .max((title_size + subtitle_size) * 0.12);
                ctx.label_left(
                    x,
                    rect.center().y - (subtitle_size + gap) / 2.0,
                    &self.title,
                    title_size,
                    weight::NORMAL,
                    theme.colors.text,
                );
                ctx.label_left(
                    x,
                    rect.center().y + (title_size + gap) / 2.0,
                    subtitle,
                    subtitle_size,
                    weight::NORMAL,
                    theme.colors.dim_text,
                );
            }
            None => ctx.label_left(
                x,
                rect.center().y,
                &self.title,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            ),
        }
        self.suffix_rect(rect, &theme, suffix_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_is_right_aligned_with_padding() {
        let theme = Theme::light();
        let row = ActionRow::new("Motyw");
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 56.0);
        let s = row.suffix_rect(rect, &theme, 120.0);
        assert!((s.right() - (rect.right() - theme.metrics.row.pad_x)).abs() < 1e-9);
        assert!((s.width() - 120.0).abs() < 1e-9);
        assert!((s.height() - rect.height()).abs() < 1e-9);
    }

    #[test]
    fn subtitle_builder() {
        let row = ActionRow::new("Sieć").subtitle("Włączone");
        assert_eq!(row.title(), "Sieć");
        assert_eq!(row.subtitle_text(), Some("Włączone"));
    }
}
