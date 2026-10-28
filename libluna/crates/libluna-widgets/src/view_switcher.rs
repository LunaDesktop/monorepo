//! `ViewSwitcher` — segmentowy przełącznik (np. `Lista | Siatka`, `Jasny | Ciemny | Auto`).
//!
//! Widget immediate: trzyma etykiety i bieżący wybór. Aplikacja wywołuje [`ViewSwitcher::paint`]
//! oraz [`ViewSwitcher::click_at`] / [`ViewSwitcher::index_at`].

use libluna_core::style::weight;
use libluna_core::{Insets, Point, Rect};

use crate::ctx::PaintCtx;

/// Segmentowy przełącznik widoku.
#[derive(Debug, Clone)]
pub struct ViewSwitcher {
    labels: Vec<String>,
    selected: usize,
    hovered: Option<usize>,
}

impl ViewSwitcher {
    /// Tworzy przełącznik z listą etykiet (pierwsza zaznaczona).
    pub fn new<I, S>(labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            labels: labels.into_iter().map(Into::into).collect(),
            selected: 0,
            hovered: None,
        }
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn count(&self) -> usize {
        self.labels.len()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.labels.get(self.selected).map(String::as_str)
    }

    /// Ustawia wybór (poza zakresem jest przycinany).
    pub fn set_selected(&mut self, index: usize) {
        if !self.labels.is_empty() {
            self.selected = index.min(self.labels.len() - 1);
        }
    }

    /// Ustawia podświetlenie (np. z pozycji wskaźnika).
    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Prostokąty kolejnych segmentów (równy podział szerokości `rect`).
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

    /// Indeks segmentu pod punktem `p` (lub `None`).
    pub fn index_at(&self, rect: Rect, p: Point) -> Option<usize> {
        if !rect.contains(p) {
            return None;
        }
        let n = self.labels.len();
        if n == 0 {
            return None;
        }
        let rel = (p.x - rect.x()) / rect.width();
        Some(((rel * n as f64) as usize).min(n - 1))
    }

    /// Obsługuje klik: ustawia wybór. Zwraca `true`, jeśli wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, p: Point) -> bool {
        match self.index_at(rect, p) {
            Some(i) if i != self.selected => {
                self.selected = i;
                true
            }
            _ => false,
        }
    }

    /// Rysuje przełącznik w prostokącie `rect`.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = ctx.theme;
        let radius = theme.radius;
        let colors = theme.colors;
        let size = theme.typography.body.size;

        // Tło-segment „szyny”.
        let track = rect.inflate(Insets::splat(theme.metrics.segmented.track_pad));
        ctx.scene
            .fill_rounded_rect(track, radius.medium, colors.shade);

        for (i, seg) in self.segments(rect).iter().enumerate() {
            let selected = i == self.selected;
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
            let label = &self.labels[i];
            ctx.label_center(
                seg.center().x,
                seg.center().y,
                label,
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
    fn segments_partition_width() {
        let vs = ViewSwitcher::new(["A", "B", "C"]);
        let segs = vs.segments(rect());
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], Rect::from_xywh(0.0, 0.0, 80.0, 32.0));
        assert_eq!(segs[2].right(), 240.0);
    }

    #[test]
    fn index_at_maps_points() {
        let vs = ViewSwitcher::new(["A", "B", "C"]);
        let r = rect();
        assert_eq!(vs.index_at(r, Point::new(10.0, 16.0)), Some(0));
        assert_eq!(vs.index_at(r, Point::new(90.0, 16.0)), Some(1));
        assert_eq!(vs.index_at(r, Point::new(200.0, 16.0)), Some(2));
        assert_eq!(vs.index_at(r, Point::new(300.0, 16.0)), None);
    }

    #[test]
    fn click_changes_selection_only_once() {
        let mut vs = ViewSwitcher::new(["A", "B", "C"]);
        assert!(vs.click_at(rect(), Point::new(90.0, 16.0)));
        assert_eq!(vs.selected(), 1);
        assert_eq!(vs.selected_label(), Some("B"));
        // Klik w już wybrany segment nic nie zmienia.
        assert!(!vs.click_at(rect(), Point::new(90.0, 16.0)));
    }
}
