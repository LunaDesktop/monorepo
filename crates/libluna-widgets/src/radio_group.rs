//! `RadioGroup` — lista opcji z pojedynczym wyborem (radiobuttony).
//!
//! Widget immediate: trzyma opcje i indeks wybranej. Aplikacja woła [`RadioGroup::paint`]
//! oraz [`RadioGroup::index_at`] / [`RadioGroup::click_at`].

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Grupa opcji (jeden wybór).
#[derive(Debug, Clone, Default)]
pub struct RadioGroup {
    options: Vec<String>,
    selected: usize,
    hovered: Option<usize>,
}

impl RadioGroup {
    pub fn new<I, S>(options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
            hovered: None,
        }
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn len(&self) -> usize {
        self.options.len()
    }

    pub fn is_empty(&self) -> bool {
        self.options.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn set_selected(&mut self, index: usize) {
        if !self.options.is_empty() {
            self.selected = index.min(self.options.len() - 1);
        }
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Wysokość całej grupy.
    pub fn height(&self, theme: &Theme) -> f64 {
        self.options.len() as f64 * theme.metrics.radio.row_h
    }

    /// Prostokąt wiersza opcji `i`.
    pub fn row_rect(&self, rect: Rect, i: usize, theme: &Theme) -> Rect {
        Rect::from_xywh(
            rect.x(),
            rect.y() + i as f64 * theme.metrics.radio.row_h,
            rect.width(),
            theme.metrics.radio.row_h,
        )
    }

    /// Indeks opcji pod punktem `p`.
    pub fn index_at(&self, rect: Rect, p: Point, theme: &Theme) -> Option<usize> {
        (0..self.options.len()).find(|i| self.row_rect(rect, *i, theme).contains(p))
    }

    /// Wybiera opcję pod punktem `p`. Zwraca `true` przy trafieniu.
    pub fn click_at(&mut self, rect: Rect, p: Point, theme: &Theme) -> bool {
        match self.index_at(rect, p, theme) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let d = theme.metrics.radio.dot;
        let gap = theme.metrics.radio.gap;

        for (i, label) in self.options.iter().enumerate() {
            let row = self.row_rect(rect, i, &theme);
            let dot = Rect::from_xywh(rect.x(), row.center().y - d / 2.0, d, d);
            let selected = i == self.selected;

            if self.hovered == Some(i) && !selected {
                let halo = dot.inflate(libluna_core::Insets::splat(4.0));
                ctx.scene
                    .fill_rounded_rect(halo, halo.width() / 2.0, colors.shade);
            }

            let ring = if selected {
                colors.accent
            } else {
                colors.border
            };
            ctx.scene.stroke_rounded_rect(dot, d / 2.0, 1.5, ring);
            if selected {
                let inner = dot.deflate(libluna_core::Insets::splat(d * 0.3));
                ctx.scene
                    .fill_rounded_rect(inner, inner.width() / 2.0, colors.accent);
            }

            ctx.label_left(
                dot.right() + gap,
                row.center().y,
                label,
                theme.typography.body.size,
                if selected {
                    weight::MEDIUM
                } else {
                    weight::NORMAL
                },
                colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 120.0)
    }

    #[test]
    fn rows_stack_and_hit() {
        let theme = Theme::light();
        let g = RadioGroup::new(["Nazwy", "Daty", "Rozmiar"]);
        assert_eq!(g.len(), 3);
        let r0 = g.row_rect(rect(), 0, &theme);
        let r2 = g.row_rect(rect(), 2, &theme);
        assert!(r2.y() > r0.y());
        assert_eq!(g.index_at(rect(), r2.center(), &theme), Some(2));
        assert_eq!(g.index_at(rect(), Point::new(500.0, 5.0), &theme), None);
    }

    #[test]
    fn click_selects_and_clamps() {
        let theme = Theme::light();
        let mut g = RadioGroup::new(["A", "B", "C"]);
        let row1 = g.row_rect(rect(), 1, &theme).center();
        assert!(g.click_at(rect(), row1, &theme));
        assert_eq!(g.selected(), 1);
        g.set_selected(99);
        assert_eq!(g.selected(), 2);
    }
}
