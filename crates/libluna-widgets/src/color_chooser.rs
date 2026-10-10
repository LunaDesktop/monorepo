//! `ColorChooser` — wybór koloru z palety (GTK4, w duchu libLuny).
//!
//! To nie jest pełny `GtkColorChooser` z kolem barw — libLuna daje zwięzłą **paletę
//! próbek** (`swatch`): kwadratowa siatka kolorów z wyborem. Idealna do wyboru akcentu
//! w ustawieniach.

use libluna_core::{Color, Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Paleta próbek kolorów.
#[derive(Debug, Clone, Default)]
pub struct ColorChooser {
    colors: Vec<Color>,
    selected: usize,
    columns: usize,
}

impl ColorChooser {
    /// Paleta domyślna (osiem wyrazistych kolorów w duchu libadwaita).
    pub fn default_palette() -> Vec<Color> {
        vec![
            Color::from_rgba8(0x35, 0x84, 0xe4, 0xff),
            Color::from_rgba8(0x91, 0x41, 0xac, 0xff),
            Color::from_rgba8(0xc0, 0x1c, 0x28, 0xff),
            Color::from_rgba8(0xe5, 0xa5, 0x0a, 0xff),
            Color::from_rgba8(0x2e, 0xc2, 0x7e, 0xff),
            Color::from_rgba8(0x1a, 0x5f, 0xb4, 0xff),
            Color::from_rgba8(0x5e, 0x5c, 0x64, 0xff),
            Color::from_rgba8(0xe0, 0x1b, 0x24, 0xff),
        ]
    }

    /// Paleta z listy kolorów (wybrany pierwszy, 4 kolumny).
    pub fn new(colors: Vec<Color>) -> Self {
        Self {
            colors,
            selected: 0,
            columns: 4,
        }
    }

    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    /// Ustawia liczbę kolumn siatki.
    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_color(&self) -> Option<Color> {
        self.colors.get(self.selected).copied()
    }

    /// Ustawia wybraną próbkę (poza zakresem jest ignorowane).
    pub fn set_selected(&mut self, index: usize) {
        if index < self.colors.len() {
            self.selected = index;
        }
    }

    /// Prostokąty próbek (kwadraty wypełniające szerokość, wierszami).
    pub fn cells(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        if self.colors.is_empty() {
            return Vec::new();
        }
        let cols = self.columns.max(1);
        let gap = theme.spacing.sm;
        let side = ((rect.width() - gap * (cols as f64 - 1.0)) / cols as f64).max(0.0);
        self.colors
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let row = i / cols;
                let col = i % cols;
                Rect::from_xywh(
                    rect.x() + col as f64 * (side + gap),
                    rect.y() + row as f64 * (side + gap),
                    side,
                    side,
                )
            })
            .collect()
    }

    /// Indeks próbki pod punktem `p` (lub `None`).
    pub fn index_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<usize> {
        self.cells(rect, theme).iter().position(|c| c.contains(p))
    }

    /// Obsługuje klik: wybiera próbkę. Zwraca `true`, gdy wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        match self.index_at(rect, theme, p) {
            Some(i) if i != self.selected => {
                self.selected = i;
                true
            }
            _ => false,
        }
    }

    /// Wysokość potrzebna na całą siatkę.
    pub fn height(&self, rect_width: f64, theme: &Theme) -> f64 {
        if self.colors.is_empty() {
            return 0.0;
        }
        let cols = self.columns.max(1);
        let gap = theme.spacing.sm;
        let side = ((rect_width - gap * (cols as f64 - 1.0)) / cols as f64).max(0.0);
        let rows = self.colors.len().div_ceil(cols) as f64;
        rows * side + (rows - 1.0) * gap
    }

    /// Rysuje siatkę próbek; wybrana dostaje obwódkę akcentu.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let cells = self.cells(rect, &theme);
        for (i, cell) in cells.iter().enumerate() {
            let swatch = cell.deflate(libluna_core::Insets::splat(2.0));
            ctx.scene
                .fill_rounded_rect(swatch, theme.radius.small, self.colors[i]);
            if i == self.selected {
                ctx.scene
                    .stroke_rounded_rect(swatch, theme.radius.small, 2.0, theme.colors.accent);
            } else {
                ctx.scene
                    .stroke_rounded_rect(swatch, theme.radius.small, 1.0, theme.colors.border);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chooser() -> ColorChooser {
        ColorChooser::new(ColorChooser::default_palette())
    }

    #[test]
    fn cells_fill_width_row_major() {
        let theme = Theme::light();
        let c = chooser();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 200.0);
        let cells = c.cells(rect, &theme);
        assert_eq!(cells.len(), 8);
        let gap = theme.spacing.sm;
        let side = (200.0 - 3.0 * gap) / 4.0;
        assert!((cells[0].width() - side).abs() < 1e-9);
        // Piąta próbka zaczyna drugi wiersz.
        assert_eq!(cells[4].x(), 0.0);
        assert!(cells[4].y() > cells[0].y());
        assert!((cells[3].right() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn click_selects_and_reports_change() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 200.0);
        let mut c = chooser();
        let cells = c.cells(rect, &theme);
        assert!(c.click_at(rect, &theme, cells[2].center()));
        assert_eq!(c.selected(), 2);
        assert_eq!(c.selected_color(), Some(c.colors()[2]));
        // Ponowny klik w tę samą próbkę nic nie zmienia.
        assert!(!c.click_at(rect, &theme, cells[2].center()));
        // Klik poza siatką nie zmienia wyboru.
        assert!(!c.click_at(rect, &theme, Point::new(999.0, 999.0)));
    }

    #[test]
    fn height_matches_grid_rows() {
        let theme = Theme::light();
        let c = chooser(); // 8 kolorów, 4 kolumny → 2 wiersze
        let w = 200.0;
        let side = (w - 3.0 * theme.spacing.sm) / 4.0;
        let expected = 2.0 * side + theme.spacing.sm;
        assert!((c.height(w, &theme) - expected).abs() < 1e-9);
    }

    #[test]
    fn selection_out_of_range_is_ignored() {
        let mut c = chooser();
        c.set_selected(99);
        assert_eq!(c.selected(), 0);
    }
}
