//! `Assistant` — kreator kroków (GTK4): nagłówek z tytułem i wskaźnikiem, treść, stopka z akcjami.
//!
//! Widget immediate: trzyma listę kroków i bieżący indeks. Rysuje chrome (nagłówek, kropki
//! postępu, stopkę z przyciskami); treść kroku rysuje aplikacja w [`Assistant::content_rect`].

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};
use libluna_render::TextEngine;

use crate::button::{Button, ButtonVariant};
use crate::ctx::PaintCtx;

/// Kreator kroków.
#[derive(Debug, Clone, Default)]
pub struct Assistant {
    steps: Vec<String>,
    current: usize,
}

impl Assistant {
    pub fn new<I, S>(steps: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            steps: steps.into_iter().map(Into::into).collect(),
            current: 0,
        }
    }

    pub fn count(&self) -> usize {
        self.steps.len()
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn title(&self) -> &str {
        self.steps
            .get(self.current)
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn set_current(&mut self, index: usize) {
        if !self.steps.is_empty() {
            self.current = index.min(self.steps.len() - 1);
        }
    }

    pub fn next(&mut self) {
        self.set_current(self.current + 1);
    }

    pub fn prev(&mut self) {
        self.current = self.current.saturating_sub(1);
    }

    pub fn is_first(&self) -> bool {
        self.current == 0
    }

    pub fn is_last(&self) -> bool {
        self.steps.is_empty() || self.current + 1 >= self.steps.len()
    }

    pub fn header_height(&self, theme: &Theme) -> f64 {
        theme.typography.body.line_height + theme.spacing.md * 2.0
    }

    pub fn footer_height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height + theme.spacing.xs * 2.0
    }

    /// Prostokąt nagłówka (górna część).
    pub fn header_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        Rect::from_xywh(rect.x(), rect.y(), rect.width(), self.header_height(theme))
    }

    /// Prostokąt stopki (dolna część).
    pub fn footer_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let h = self.footer_height(theme);
        Rect::from_xywh(rect.x(), rect.bottom() - h, rect.width(), h)
    }

    /// Obszar treści kroku (między nagłówkiem i stopką).
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let top = self.header_height(theme);
        let bottom = self.footer_height(theme);
        Rect::from_xywh(
            rect.x(),
            rect.y() + top,
            rect.width(),
            (rect.height() - top - bottom).max(0.0),
        )
    }

    /// Prostokąty kropek postępu (wyśrodkowane w nagłówku).
    pub fn indicator_rects(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        let w = self.header_rect(rect, theme);
        let d = 8.0;
        let gap = theme.spacing.sm;
        let n = self.steps.len();
        if n == 0 {
            return Vec::new();
        }
        let total = n as f64 * d + (n as f64 - 1.0) * gap;
        let mut x = w.center().x - total / 2.0;
        (0..n)
            .map(|_| {
                let r = Rect::from_xywh(x, w.bottom() - theme.spacing.sm - d, d, d);
                x += d + gap;
                r
            })
            .collect()
    }

    /// Prostokąty `[anuluj, dalej/zakończ]` w stopce (wg szerokości przycisków).
    pub fn buttons(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        cancel_label: &str,
        next_label: &str,
    ) -> [Rect; 2] {
        let f = self.footer_rect(rect, theme);
        let cy = f.center().y;
        let h = theme.metrics.button.height;
        let pad = theme.spacing.md;
        let next_w = Button::new(next_label).width(text, theme);
        let cancel_w = Button::new(cancel_label).width(text, theme);
        let next = Rect::from_xywh(f.right() - pad - next_w, cy - h / 2.0, next_w, h);
        let cancel = Rect::from_xywh(
            next.x() - theme.spacing.sm - cancel_w,
            cy - h / 2.0,
            cancel_w,
            h,
        );
        [cancel, next]
    }

    /// Rysuje chrome kreatora i przyciski stopki.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, cancel_label: &str, next_label: &str) {
        let theme = *ctx.theme;
        let header = self.header_rect(rect, &theme);

        // Tytuł kroku.
        ctx.label_center(
            header.center().x,
            header.y() + theme.spacing.md + theme.typography.body.line_height / 2.0,
            self.title(),
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.text,
        );

        // Kropki postępu.
        for (i, dot) in self.indicator_rects(rect, &theme).iter().enumerate() {
            let color = if i == self.current {
                theme.colors.accent
            } else {
                theme.colors.border
            };
            ctx.scene.fill_rounded_rect(*dot, dot.width() / 2.0, color);
        }

        // Stopka.
        let footer = self.footer_rect(rect, &theme);
        ctx.scene.fill_rect(footer, theme.colors.window_bg);
        ctx.scene.fill_rect(
            Rect::from_xywh(footer.x(), footer.y(), footer.width(), 1.0),
            theme.colors.border,
        );

        let text = ctx.text;
        let [cancel, next] = self.buttons(rect, text, &theme, cancel_label, next_label);
        Button::new(cancel_label)
            .variant(ButtonVariant::Flat)
            .paint(ctx, cancel);
        Button::new(next_label)
            .variant(ButtonVariant::Suggested)
            .paint(ctx, next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 400.0, 300.0)
    }

    #[test]
    fn navigation_is_clamped() {
        let mut a = Assistant::new(["Jeden", "Dwa", "Trzy"]);
        assert!(a.is_first());
        assert_eq!(a.title(), "Jeden");
        a.prev();
        assert_eq!(a.current(), 0);
        a.next();
        a.next();
        assert!(a.is_last());
        assert_eq!(a.title(), "Trzy");
        a.next();
        assert_eq!(a.current(), 2);
    }

    #[test]
    fn regions_stack_without_overlap() {
        let theme = Theme::light();
        let a = Assistant::new(["A", "B"]);
        let header = a.header_rect(rect(), &theme);
        let content = a.content_rect(rect(), &theme);
        let footer = a.footer_rect(rect(), &theme);
        assert_eq!(content.y(), header.bottom());
        assert_eq!(content.bottom(), footer.y());
        assert!((footer.bottom() - 300.0).abs() < 1e-9);
    }

    #[test]
    fn indicator_has_one_dot_per_step_centered() {
        let theme = Theme::light();
        let a = Assistant::new(["A", "B", "C"]);
        let dots = a.indicator_rects(rect(), &theme);
        assert_eq!(dots.len(), 3);
        let center = (dots[0].center().x + dots[2].center().x) / 2.0;
        assert!((center - 200.0).abs() < 1e-9);
    }

    #[test]
    fn buttons_sit_at_footer_right() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let a = Assistant::new(["A"]);
        let [cancel, next] = a.buttons(rect(), &text, &theme, "Anuluj", "Dalej");
        assert!(next.right() <= 400.0);
        assert!(cancel.right() < next.x());
        assert_eq!(next.height(), theme.metrics.button.height);
    }
}
