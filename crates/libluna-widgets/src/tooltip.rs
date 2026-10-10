//! `Tooltip` — dymek podpowiedzi umieszczany nad elementem (nakładka).
//!
//! Widget immediate: [`Tooltip::rect`] liczy prostokąt nad kotwicą, [`Tooltip::paint`] rysuje
//! powierzchnię o wysokim kontraście z napisem.

use libluna_core::style::{weight, Appearance};
use libluna_core::{Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Dymek podpowiedzi.
#[derive(Debug, Clone, Default)]
pub struct Tooltip {
    text: String,
}

impl Tooltip {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Prostokąt dymka nad kotwicą `anchor` (wyśrodkowany, odsunięty o `gap`).
    pub fn rect(&self, anchor: Rect, theme: &Theme, text: &TextEngine) -> Rect {
        let m = theme.metrics.tooltip;
        let caption = theme.typography.caption;
        let width = m.pad_x * 2.0 + text.measure(&self.text, caption.size, weight::NORMAL).width;
        let height = m.pad_y * 2.0 + caption.line_height;
        Rect::from_xywh(
            anchor.center().x - width / 2.0,
            anchor.y() - m.gap - height,
            width,
            height,
        )
    }

    /// Rysuje dymek (nakładka — na wierzchu sceny).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.tooltip;
        let (bg, fg) = match theme.appearance {
            Appearance::Light => (theme.colors.text, theme.colors.view_bg),
            Appearance::Dark => (theme.colors.card_bg, theme.colors.text),
        };
        ctx.scene.fill_rounded_rect(rect, m.radius, bg);
        ctx.label_center(
            rect.center().x,
            rect.center().y,
            &self.text,
            theme.typography.caption.size,
            weight::NORMAL,
            fg,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_sits_above_anchor_and_is_centered() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let tooltip = Tooltip::new("Zapisz zmiany");
        let anchor = Rect::from_xywh(100.0, 200.0, 40.0, 20.0);
        let r = tooltip.rect(anchor, &theme, &text);
        assert!((r.center().x - anchor.center().x).abs() < 1e-9);
        assert!(r.bottom() <= anchor.y());
        assert!((anchor.y() - r.bottom() - theme.metrics.tooltip.gap).abs() < 1e-9);
    }

    #[test]
    fn width_grows_with_text() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let anchor = Rect::from_xywh(0.0, 100.0, 10.0, 10.0);
        let short = Tooltip::new("a").rect(anchor, &theme, &text);
        let long = Tooltip::new("znacznie dłuższy napis").rect(anchor, &theme, &text);
        assert!(long.width() > short.width());
    }
}
