//! `ColumnView` — tabela z kolumnami i wierszami (GTK4).
//!
//! Widget immediate: trzyma definicje kolumn, liczbę wierszy i wybór. Rysuje nagłówek
//! i siatkę; **treść komórek** rysuje aplikacja w prostokątach z [`ColumnView::cell_rect`].

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Definicja kolumny: tytuł i waga szerokości (udziały normalizowane do szerokości tabeli).
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub title: String,
    pub weight: f64,
}

impl Column {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            weight: 1.0,
        }
    }

    /// Ustawia wagę szerokości kolumny.
    pub fn weight(mut self, weight: f64) -> Self {
        self.weight = weight.max(0.0);
        self
    }
}

/// Tabela (widok kolumnowy).
#[derive(Debug, Clone, Default)]
pub struct ColumnView {
    columns: Vec<Column>,
    rows: usize,
    selected: Option<usize>,
    hovered: Option<usize>,
}

impl ColumnView {
    pub fn new(columns: Vec<Column>) -> Self {
        Self {
            columns,
            ..Self::default()
        }
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn set_rows(&mut self, rows: usize) {
        self.rows = rows;
        if let Some(s) = self.selected {
            if s >= rows {
                self.selected = None;
            }
        }
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_selected(&mut self, index: Option<usize>) {
        self.selected = index.filter(|i| *i < self.rows);
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered.filter(|i| *i < self.rows);
    }

    pub fn header_height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    pub fn row_height(&self, theme: &Theme) -> f64 {
        theme.typography.body.line_height + theme.spacing.md
    }

    /// Granice kolumn (współrzędna `x` i szerokość) rozłożone wg wag.
    fn spans(&self, rect: Rect) -> Vec<(f64, f64)> {
        let total: f64 = self.columns.iter().map(|c| c.weight).sum();
        if total <= 0.0 || self.columns.is_empty() {
            return vec![(rect.x(), 0.0); self.columns.len()];
        }
        let mut x = rect.x();
        self.columns
            .iter()
            .map(|c| {
                let w = rect.width() * (c.weight / total);
                let span = (x, w);
                x += w;
                span
            })
            .collect()
    }

    /// Prostokąty nagłówków kolumn.
    pub fn header_cells(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        let h = self.header_height(theme);
        self.spans(rect)
            .into_iter()
            .map(|(x, w)| Rect::from_xywh(x, rect.y(), w, h))
            .collect()
    }

    /// Obszar wierszy (pod nagłówkiem).
    pub fn rows_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let h = self.header_height(theme);
        Rect::from_xywh(
            rect.x(),
            rect.y() + h,
            rect.width(),
            (rect.height() - h).max(0.0),
        )
    }

    /// Prostokąt wiersza `index`.
    pub fn row_rect(&self, rect: Rect, theme: &Theme, index: usize) -> Rect {
        let area = self.rows_rect(rect, theme);
        let rh = self.row_height(theme);
        Rect::from_xywh(area.x(), area.y() + index as f64 * rh, area.width(), rh)
    }

    /// Prostokąt komórki `(row, col)`.
    pub fn cell_rect(&self, rect: Rect, theme: &Theme, row: usize, col: usize) -> Rect {
        let r = self.row_rect(rect, theme, row);
        match self.spans(rect).get(col) {
            Some(&(x, w)) => Rect::from_xywh(x, r.y(), w, r.height()),
            None => Rect::from_xywh(rect.x(), r.y(), 0.0, r.height()),
        }
    }

    /// Indeks wiersza pod punktem `p` (tylko w obszarze wierszy).
    pub fn row_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<usize> {
        let area = self.rows_rect(rect, theme);
        if !area.contains(p) {
            return None;
        }
        let rh = self.row_height(theme);
        if rh <= 0.0 {
            return None;
        }
        let idx = ((p.y - area.y()) / rh) as usize;
        if idx < self.rows {
            Some(idx)
        } else {
            None
        }
    }

    /// Rysuje nagłówek (tło, tytuły, separator dolny i pionowe) oraz zaznaczony wiersz.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let header = self.header_cells(rect, &theme);
        let hh = self.header_height(&theme);
        let header_bg = Rect::from_xywh(rect.x(), rect.y(), rect.width(), hh);
        ctx.scene.fill_rect(header_bg, theme.colors.shade);

        let body = theme.typography.body.size;
        for (i, cell) in header.iter().enumerate() {
            ctx.label_left(
                cell.x() + theme.spacing.md,
                cell.center().y,
                &self.columns[i].title,
                body,
                weight::MEDIUM,
                theme.colors.dim_text,
            );
            if i > 0 {
                let line = Rect::from_xywh(cell.x(), cell.y(), 1.0, cell.height());
                ctx.scene.fill_rect(line, theme.colors.border);
            }
        }
        // Dolna krawędź nagłówka.
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.x(), rect.y() + hh - 1.0, rect.width(), 1.0),
            theme.colors.border,
        );

        // Zaznaczony / podświetlony wiersz.
        let area = self.rows_rect(rect, &theme);
        for i in 0..self.rows {
            let row = self.row_rect(rect, &theme, i);
            if row.y() >= area.bottom() {
                break;
            }
            if self.selected == Some(i) {
                ctx.scene
                    .fill_rect(row, theme.colors.accent.scale_alpha(0.18));
            } else if self.hovered == Some(i) {
                ctx.scene.fill_rect(row, theme.colors.shade);
            }
            // Separator między wierszami.
            ctx.scene.fill_rect(
                Rect::from_xywh(row.x(), row.bottom() - 1.0, row.width(), 1.0),
                theme.colors.border.scale_alpha(0.5),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> ColumnView {
        ColumnView::new(vec![
            Column::new("Nazwa").weight(2.0),
            Column::new("Rozmiar"),
            Column::new("Data"),
        ])
    }

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 400.0, 300.0)
    }

    #[test]
    fn columns_split_by_weight() {
        let theme = Theme::light();
        let v = view();
        let cells = v.header_cells(rect(), &theme);
        assert_eq!(cells.len(), 3);
        // Waga 2 z sumy 4 → połowa szerokości.
        assert!((cells[0].width() - 200.0).abs() < 1e-9);
        assert!((cells[1].width() - 100.0).abs() < 1e-9);
        assert!((cells[2].right() - 400.0).abs() < 1e-9);
    }

    #[test]
    fn rows_start_below_header() {
        let theme = Theme::light();
        let v = view();
        let area = v.rows_rect(rect(), &theme);
        assert_eq!(area.y(), v.header_height(&theme));
        assert_eq!(area.height(), 300.0 - v.header_height(&theme));
    }

    #[test]
    fn row_at_maps_points_and_ignores_header() {
        let theme = Theme::light();
        let mut v = view();
        v.set_rows(5);
        let r1 = v.row_rect(rect(), &theme, 2);
        assert_eq!(v.row_at(rect(), &theme, r1.center()), Some(2));
        // W nagłówku nie ma wiersza.
        assert_eq!(v.row_at(rect(), &theme, Point::new(10.0, 5.0)), None);
    }

    #[test]
    fn selection_is_clamped_to_rows() {
        let mut v = view();
        v.set_rows(3);
        v.set_selected(Some(2));
        assert_eq!(v.selected(), Some(2));
        v.set_rows(1);
        assert_eq!(v.selected(), None);
        v.set_selected(Some(9));
        assert_eq!(v.selected(), None);
    }

    #[test]
    fn cell_rect_matches_row_and_column() {
        let theme = Theme::light();
        let v = view();
        let cell = v.cell_rect(rect(), &theme, 1, 2);
        let row = v.row_rect(rect(), &theme, 1);
        assert_eq!(cell.y(), row.y());
        assert_eq!(cell.height(), row.height());
        assert!(cell.right() <= 400.0 + 1e-9);
    }
}
