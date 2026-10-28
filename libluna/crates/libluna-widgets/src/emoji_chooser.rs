//! `EmojiChooser` — wybór emoji z siatki (GTK4, w duchu libLuny).
//!
//! Prosta, samowystarczalna siatka glifów emoji: wybór jednego z nich. To nie jest pełny
//! `GtkEmojiChooser` (bez kategorii i wyszukiwania) — libLuna daje zwięzłą paletę znaków.

use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Siatka emoji.
#[derive(Debug, Clone, Default)]
pub struct EmojiChooser {
    emojis: Vec<String>,
    columns: usize,
    selected: usize,
    hovered: Option<usize>,
}

impl EmojiChooser {
    /// Krótka, domyślna paleta emoji (do szybkiego wyboru).
    pub fn default_palette() -> Vec<String> {
        [
            "😀", "😄", "😉", "😍", "🤔", "😎", "🥳", "😴", "👍", "👏", "🙏", "💪", "🔥", "✨",
            "🎉", "❤️", "⭐", "✅", "⚠️", "💡", "📁", "📄", "🔍", "🔒",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    /// Siatka z listy znaków (4 kolumny, wybrany pierwszy).
    pub fn new(emojis: Vec<String>) -> Self {
        Self {
            emojis,
            columns: 4,
            selected: 0,
            hovered: None,
        }
    }

    pub fn emojis(&self) -> &[String] {
        &self.emojis
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn set_columns(&mut self, columns: usize) {
        self.columns = columns.max(1);
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_emoji(&self) -> Option<&str> {
        self.emojis.get(self.selected).map(String::as_str)
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.emojis.len() {
            self.selected = index;
        }
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered.filter(|i| *i < self.emojis.len());
    }

    /// Prostokąty komórek (kwadraty wypełniające szerokość, wierszami).
    pub fn cells(&self, rect: Rect, theme: &Theme) -> Vec<Rect> {
        if self.emojis.is_empty() {
            return Vec::new();
        }
        let cols = self.columns.max(1);
        let gap = theme.spacing.sm;
        let side = ((rect.width() - gap * (cols as f64 - 1.0)) / cols as f64).max(0.0);
        self.emojis
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

    pub fn index_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<usize> {
        self.cells(rect, theme).iter().position(|c| c.contains(p))
    }

    /// Obsługuje klik: wybiera emoji. Zwraca `true`, gdy wybór się zmienił.
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
    pub fn height(&self, width: f64, theme: &Theme) -> f64 {
        if self.emojis.is_empty() {
            return 0.0;
        }
        let cols = self.columns.max(1);
        let gap = theme.spacing.sm;
        let side = ((width - gap * (cols as f64 - 1.0)) / cols as f64).max(0.0);
        let rows = self.emojis.len().div_ceil(cols) as f64;
        rows * side + (rows - 1.0) * gap
    }

    /// Rysuje siatkę emoji; wybrana dostaje tło akcentu.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let size = theme.typography.title.size;
        for (i, cell) in self.cells(rect, &theme).iter().enumerate() {
            let inset = cell.deflate(libluna_core::Insets::splat(2.0));
            if i == self.selected {
                ctx.scene
                    .fill_rounded_rect(inset, theme.radius.small, theme.colors.accent);
            } else if self.hovered == Some(i) {
                ctx.scene
                    .fill_rounded_rect(inset, theme.radius.small, theme.colors.shade);
            }
            ctx.label_center(
                cell.center().x,
                cell.center().y,
                &self.emojis[i],
                size,
                libluna_core::style::weight::NORMAL,
                theme.colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chooser() -> EmojiChooser {
        EmojiChooser::new(EmojiChooser::default_palette())
    }

    #[test]
    fn cells_grid_row_major() {
        let theme = Theme::light();
        let c = chooser();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 200.0);
        let cells = c.cells(rect, &theme);
        assert_eq!(cells.len(), 24);
        assert_eq!(cells[4].x(), cells[0].x());
        assert!(cells[4].y() > cells[0].y());
    }

    #[test]
    fn click_selects_emoji() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 200.0);
        let mut c = chooser();
        let cells = c.cells(rect, &theme);
        assert!(c.click_at(rect, &theme, cells[1].center()));
        assert_eq!(c.selected_emoji(), Some("😄"));
        assert!(!c.click_at(rect, &theme, cells[1].center()));
        assert!(!c.click_at(rect, &theme, Point::new(999.0, 999.0)));
    }

    #[test]
    fn height_matches_rows() {
        let theme = Theme::light();
        let c = chooser(); // 24 emoji, 4 kolumny → 6 wierszy
        let w = 200.0;
        let side = (w - 3.0 * theme.spacing.sm) / 4.0;
        let expected = 6.0 * side + 5.0 * theme.spacing.sm;
        assert!((c.height(w, &theme) - expected).abs() < 1e-9);
    }
}
