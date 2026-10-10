//! `ShortcutLabel` — skrót klawiszowy jako klawisze (`Ctrl` `+` `N`).
//!
//! Widget immediate: [`ShortcutLabel::key_rects`] liczy prostokąty klawiszy, a
//! [`ShortcutLabel::paint`] rysuje je z separatorem `+`. `width`/`height` pozwalają wyrównać
//! skrót do prawej (np. w [`Menu`](crate::Menu) albo [`ActionRow`](crate::ActionRow)).

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Etykieta skrótu klawiszowego.
#[derive(Debug, Clone, Default)]
pub struct ShortcutLabel {
    keys: Vec<String>,
}

impl ShortcutLabel {
    pub fn new<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            keys: keys.into_iter().map(Into::into).collect(),
        }
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    /// Skrót jako tekst (np. `Ctrl+N`).
    pub fn string(&self) -> String {
        self.keys.join("+")
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn caption(theme: &Theme) -> f64 {
        theme.typography.caption.size
    }

    fn key_width(&self, key: &str, theme: &Theme, text: &TextEngine) -> f64 {
        let m = theme.metrics.shortcut;
        2.0 * m.pad_x
            + text
                .measure(key, Self::caption(theme), weight::NORMAL)
                .width
    }

    fn plus_width(theme: &Theme, text: &TextEngine) -> f64 {
        text.measure("+", Self::caption(theme), weight::NORMAL)
            .width
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        let m = theme.metrics.shortcut;
        2.0 * m.pad_y + theme.typography.caption.line_height
    }

    pub fn width(&self, theme: &Theme, text: &TextEngine) -> f64 {
        let m = theme.metrics.shortcut;
        if self.keys.is_empty() {
            return 0.0;
        }
        let keys: f64 = self
            .keys
            .iter()
            .map(|k| self.key_width(k, theme, text))
            .sum();
        let joints = (self.keys.len() - 1) as f64 * (2.0 * m.gap + Self::plus_width(theme, text));
        keys + joints
    }

    /// Prostokąty klawiszy, wyrównane do lewej, wyśrodkowane pionowo w `rect`.
    pub fn key_rects(&self, rect: Rect, theme: &Theme, text: &TextEngine) -> Vec<Rect> {
        let m = theme.metrics.shortcut;
        let h = self.height(theme);
        let y = rect.y() + (rect.height() - h) / 2.0;
        let plus = Self::plus_width(theme, text);
        let mut x = rect.x();
        let mut out = Vec::with_capacity(self.keys.len());
        for (i, key) in self.keys.iter().enumerate() {
            let w = self.key_width(key, theme, text);
            out.push(Rect::from_xywh(x, y, w, h));
            x += w;
            if i + 1 < self.keys.len() {
                x += 2.0 * m.gap + plus;
            }
        }
        out
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let m = theme.metrics.shortcut;
        let caption = theme.typography.caption.size;
        let rects = self.key_rects(rect, &theme, ctx.text);

        for (i, key_rect) in rects.iter().enumerate() {
            ctx.scene
                .fill_rounded_rect(*key_rect, m.radius, colors.shade);
            ctx.scene
                .stroke_rounded_rect(*key_rect, m.radius, 1.0, colors.border);
            ctx.label_center(
                key_rect.center().x,
                key_rect.center().y,
                &self.keys[i],
                caption,
                weight::NORMAL,
                colors.text,
            );
            // Separator „+” między klawiszami.
            if i + 1 < rects.len() {
                let cx = key_rect.right() + m.gap + Self::plus_width(&theme, ctx.text) / 2.0;
                ctx.label_center(
                    cx,
                    key_rect.center().y,
                    "+",
                    caption,
                    weight::NORMAL,
                    colors.dim_text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_laid_out_left_to_right() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let s = ShortcutLabel::new(["Ctrl", "Shift", "N"]);
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 24.0);
        let keys = s.key_rects(rect, &theme, &text);
        assert_eq!(keys.len(), 3);
        assert!(keys[1].x() > keys[0].right());
        assert!(keys[2].x() > keys[1].right());
        assert!(s.width(&theme, &text) > 0.0);
        assert_eq!(s.string(), "Ctrl+Shift+N");
    }

    #[test]
    fn empty_is_zero_width() {
        let theme = Theme::light();
        let text = TextEngine::system();
        assert_eq!(
            ShortcutLabel::new(Vec::<String>::new()).width(&theme, &text),
            0.0
        );
    }
}
