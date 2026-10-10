//! `SearchBar` — pasek wyszukiwania (GTK4): odsłaniana szuflada z polem wyszukiwania.
//!
//! W modelu immediate aplikacja rysuje [`SearchEntry`](crate::SearchEntry) w prostokącie
//! [`SearchBar::entry_rect`], a widget odpowiada za tryb (widoczny/ukryty), geometrię
//! i tło. Przełączanie trybu: [`SearchBar::toggle`] / [`SearchBar::set_search_mode`].

use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Pasek wyszukiwania.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchBar {
    mode: bool,
}

impl SearchBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: bool) -> Self {
        Self { mode }
    }

    pub fn is_search_mode(&self) -> bool {
        self.mode
    }

    pub fn set_search_mode(&mut self, mode: bool) {
        self.mode = mode;
    }

    /// Przełącza tryb; zwraca nowy stan.
    pub fn toggle(&mut self) -> bool {
        self.mode = !self.mode;
        self.mode
    }

    /// Wysokość paska (gdy widoczny).
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.entry.height + theme.spacing.sm * 2.0
    }

    /// Obszar paska w `rect` (pas u góry kontenera). Gdy ukryty — o wysokości zerowej.
    pub fn reveal_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let h = if self.mode { self.height(theme) } else { 0.0 };
        Rect::from_xywh(rect.x(), rect.y(), rect.width(), h)
    }

    /// Prostokąt pola wyszukiwania (z wewnętrznym marginesem); zerowy, gdy ukryty.
    pub fn entry_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let bar = self.reveal_rect(rect, theme);
        if bar.height() <= 0.0 {
            return Rect::from_xywh(bar.x(), bar.y(), bar.width(), 0.0);
        }
        let pad = theme.spacing.sm;
        Rect::from_xywh(
            bar.x() + pad,
            bar.y() + pad,
            (bar.width() - 2.0 * pad).max(0.0),
            (bar.height() - 2.0 * pad).max(0.0),
        )
    }

    /// Czy punkt leży w obszarze paska (np. by nie zaczynać przeciągania okna).
    pub fn hit(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.reveal_rect(rect, theme).contains(p)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        if !self.mode {
            return;
        }
        let theme = *ctx.theme;
        let bar = self.reveal_rect(rect, &theme);
        ctx.scene.fill_rect(bar, theme.colors.window_bg);
        let line = Rect::from_xywh(bar.x(), bar.bottom() - 1.0, bar.width(), 1.0);
        ctx.scene.fill_rect(line, theme.colors.border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_bar_has_zero_height() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 300.0);
        let b = SearchBar::new();
        assert_eq!(b.reveal_rect(rect, &theme).height(), 0.0);
        assert_eq!(b.entry_rect(rect, &theme).height(), 0.0);
        assert!(!b.hit(rect, &theme, Point::new(10.0, 10.0)));
    }

    #[test]
    fn visible_bar_reveals_entry_with_padding() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 300.0);
        let mut b = SearchBar::new();
        assert!(b.toggle());
        let bar = b.reveal_rect(rect, &theme);
        assert!(
            (bar.height() - (theme.metrics.entry.height + theme.spacing.sm * 2.0)).abs() < 1e-9
        );
        let e = b.entry_rect(rect, &theme);
        assert_eq!(e.x(), theme.spacing.sm);
        assert_eq!(e.width(), 400.0 - 2.0 * theme.spacing.sm);
        assert!(b.hit(rect, &theme, e.center()));
        assert!(!b.hit(rect, &theme, Point::new(10.0, 250.0)));
    }
}
