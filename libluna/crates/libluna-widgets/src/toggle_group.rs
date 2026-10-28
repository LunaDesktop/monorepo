//! `ToggleGroup` — grupa przełączników (segmentowa, wielokrotnego wyboru).
//!
//! Widget immediate: trzyma etykiety i stan „wł.” każdego segmentu. Aplikacja wywołuje
//! [`ToggleGroup::paint`] oraz [`ToggleGroup::click_at`] / [`ToggleGroup::index_at`].
//! Różni się od [`ViewSwitcher`](crate::ViewSwitcher) tym, że segmenty mogą być włączone
//! jednocześnie (np. `Pogrubienie | Kursywa`).

use libluna_core::style::weight;
use libluna_core::{Insets, Point, Rect};

use crate::ctx::PaintCtx;

/// Grupa przełączników.
#[derive(Debug, Clone, Default)]
pub struct ToggleGroup {
    labels: Vec<String>,
    active: Vec<bool>,
    hovered: Option<usize>,
}

impl ToggleGroup {
    pub fn new<I, S>(labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        let active = vec![false; labels.len()];
        Self {
            labels,
            active,
            hovered: None,
        }
    }

    /// Tworzy grupę z jednym aktywnym segmentem.
    pub fn with_active(mut self, index: usize, on: bool) -> Self {
        self.set_active(index, on);
        self
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn count(&self) -> usize {
        self.labels.len()
    }

    pub fn is_active(&self, index: usize) -> bool {
        self.active.get(index).copied().unwrap_or(false)
    }

    pub fn set_active(&mut self, index: usize, on: bool) {
        if let Some(slot) = self.active.get_mut(index) {
            *slot = on;
        }
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Prostokąty segmentów (równy podział szerokości `rect`).
    pub fn segments(&self, rect: Rect) -> Vec<Rect> {
        let n = self.labels.len();
        if n == 0 || rect.width() <= 0.0 {
            return Vec::new();
        }
        let w = rect.width() / n as f64;
        (0..n)
            .map(|i| Rect::from_xywh(rect.x() + i as f64 * w, rect.y(), w, rect.height()))
            .collect()
    }

    pub fn index_at(&self, rect: Rect, p: Point) -> Option<usize> {
        if !rect.contains(p) || self.labels.is_empty() {
            return None;
        }
        let rel = (p.x - rect.x()) / rect.width();
        Some(((rel * self.labels.len() as f64) as usize).min(self.labels.len() - 1))
    }

    /// Obsługuje klik: przełącza segment. Zwraca indeks trafionego segmentu.
    pub fn click_at(&mut self, rect: Rect, p: Point) -> Option<usize> {
        let i = self.index_at(rect, p)?;
        self.active[i] = !self.active[i];
        Some(i)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let radius = theme.radius;
        let colors = theme.colors;
        let size = theme.typography.body.size;

        let track = rect.inflate(Insets::splat(theme.metrics.segmented.track_pad));
        ctx.scene
            .fill_rounded_rect(track, radius.medium, colors.shade);

        for (i, seg) in self.segments(rect).iter().enumerate() {
            let selected = self.is_active(i);
            if selected {
                ctx.scene
                    .fill_rounded_rect(*seg, radius.small, colors.accent);
            } else if self.hovered == Some(i) {
                ctx.scene
                    .fill_rounded_rect(*seg, radius.small, colors.shade);
            }
            let fg = if selected {
                colors.accent_fg
            } else {
                colors.text
            };
            ctx.label_center(
                seg.center().x,
                seg.center().y,
                &self.labels[i],
                size,
                weight::MEDIUM,
                fg,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 240.0, 32.0)
    }

    #[test]
    fn segments_and_index() {
        let g = ToggleGroup::new(["Bold", "Italic", "Underline"]);
        let segs = g.segments(rect());
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].width(), 80.0);
        assert_eq!(g.index_at(rect(), Point::new(90.0, 16.0)), Some(1));
    }

    #[test]
    fn click_toggles_multiple() {
        let mut g = ToggleGroup::new(["Bold", "Italic"]);
        assert_eq!(g.click_at(rect(), Point::new(60.0, 16.0)), Some(0));
        assert_eq!(g.click_at(rect(), Point::new(180.0, 16.0)), Some(1));
        assert!(g.is_active(0) && g.is_active(1));
        // Ponowny klik wyłącza.
        g.click_at(rect(), Point::new(60.0, 16.0));
        assert!(!g.is_active(0));
        assert!(g.is_active(1));
    }
}
