//! `FlowBox` — zawijająca siatka o stałych komórkach z wyborem (GTK4, w duchu libLuny).
//!
//! W odróżnieniu od [`WrapBox`](crate::WrapBox) (dowolne naturalne rozmiary), `FlowBox` ma
//! **jednakowe komórki** i wybiera jedną z nich — przydatne np. do siatki skrótów czy emoji.
//! Zawijanie liczy [`WrapBox`], a `FlowBox` dokłada stan zaznaczenia i tło komórek.

use libluna_core::{Point, Rect, Size};

use crate::ctx::PaintCtx;
use crate::helpers::WrapBox;

/// Zawijająca siatka jednakowych komórek.
#[derive(Debug, Clone, Default)]
pub struct FlowBox {
    cell: Size,
    spacing: f64,
    line_spacing: f64,
    count: usize,
    selected: Option<usize>,
    hovered: Option<usize>,
}

impl FlowBox {
    /// Siatka o komórkach `cell × cell`.
    pub fn new(cell: f64) -> Self {
        Self {
            cell: Size::new(cell, cell),
            spacing: 8.0,
            line_spacing: 8.0,
            ..Self::default()
        }
    }

    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn line_spacing(mut self, spacing: f64) -> Self {
        self.line_spacing = spacing;
        self
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn set_count(&mut self, count: usize) {
        self.count = count;
        if self.selected.is_some_and(|s| s >= count) {
            self.selected = None;
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_selected(&mut self, index: Option<usize>) {
        self.selected = index.filter(|i| *i < self.count);
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered.filter(|i| *i < self.count);
    }

    fn wrap(&self) -> WrapBox {
        WrapBox::new()
            .spacing(self.spacing)
            .line_spacing(self.line_spacing)
    }

    /// Prostokąty kolejnych komórek (zawinięte do szerokości `rect`).
    pub fn cells(&self, rect: Rect) -> Vec<Rect> {
        let sizes = vec![self.cell; self.count];
        self.wrap().layout(rect, &sizes)
    }

    /// Indeks komórki pod punktem `p` (lub `None`).
    pub fn index_at(&self, rect: Rect, p: Point) -> Option<usize> {
        self.cells(rect).iter().position(|c| c.contains(p))
    }

    /// Obsługuje klik: ustawia wybór. Zwraca `true`, gdy wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, p: Point) -> bool {
        match self.index_at(rect, p) {
            Some(i) if self.selected != Some(i) => {
                self.selected = Some(i);
                true
            }
            _ => false,
        }
    }

    /// Wysokość potrzebna na zawinięcie wszystkich komórek w szerokości `width`.
    pub fn height(&self, width: f64) -> f64 {
        let sizes = vec![self.cell; self.count];
        self.wrap().height(width, &sizes)
    }

    /// Rysuje tło zaznaczonej/podświetlonej komórki (treść rysuje aplikacja).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        for (i, cell) in self.cells(rect).iter().enumerate() {
            if self.selected == Some(i) {
                ctx.scene.fill_rounded_rect(
                    *cell,
                    theme.radius.medium,
                    theme.colors.shade.scale_alpha(1.8),
                );
                ctx.scene
                    .stroke_rounded_rect(*cell, theme.radius.medium, 1.0, theme.colors.accent);
            } else if self.hovered == Some(i) {
                ctx.scene
                    .fill_rounded_rect(*cell, theme.radius.medium, theme.colors.shade);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 300.0, 200.0)
    }

    #[test]
    fn cells_wrap_into_lines() {
        let mut f = FlowBox::new(64.0).spacing(10.0).line_spacing(10.0);
        f.set_count(10);
        let cells = f.cells(rect());
        assert_eq!(cells.len(), 10);
        // 300 px: 4 komórki w linii (4*64 + 3*10 = 286) → 10 komórek to 3 linie.
        assert_eq!(cells[0].y(), cells[3].y());
        assert!(cells[4].y() > cells[3].y());
        assert_eq!(f.height(300.0), 3.0 * 64.0 + 2.0 * 10.0);
    }

    #[test]
    fn click_selects_cell() {
        let mut f = FlowBox::new(50.0);
        f.set_count(6);
        let cells = f.cells(rect());
        assert!(f.click_at(rect(), cells[2].center()));
        assert_eq!(f.selected(), Some(2));
        assert!(!f.click_at(rect(), cells[2].center()));
        assert!(!f.click_at(rect(), Point::new(-5.0, -5.0)));
    }

    #[test]
    fn selection_clamps_when_count_shrinks() {
        let mut f = FlowBox::new(50.0);
        f.set_count(6);
        f.set_selected(Some(5));
        assert_eq!(f.selected(), Some(5));
        f.set_count(3);
        assert_eq!(f.selected(), None);
    }
}
