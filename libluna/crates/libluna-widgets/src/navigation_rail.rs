//! `NavigationRail` — kompaktowa szyna nawigacji (pionowa lista pozycji z wyborem).
//!
//! Własny widget libLuny (nie klon `GtkXxx`): wąski pasek pozycji — w wersji pełnej z etykietą,
//! a zwinięty (`collapsed`) z pojedynczym znakiem. Wybrana pozycja dostaje pasek-wskaźnik
//! przy lewej krawędzi.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Kompaktowa szyna nawigacji.
#[derive(Debug, Clone, Default)]
pub struct NavigationRail {
    items: Vec<String>,
    selected: usize,
    hovered: Option<usize>,
    collapsed: bool,
}

impl NavigationRail {
    pub fn new<I, S>(items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            items: items.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    /// Zwinięta szyna (tylko inicjały zamiast etykiet).
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.items.get(self.selected).map(String::as_str)
    }

    pub fn set_selected(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
        }
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered.filter(|i| *i < self.items.len());
    }

    /// Szerokość szyny wynikająca z trybu (zwinięta/pełna).
    pub fn width(&self, theme: &Theme) -> f64 {
        if self.collapsed {
            theme.metrics.nav.item_h
        } else {
            theme.metrics.nav.item_h * 4.0
        }
    }

    /// Wysokość jednej pozycji.
    pub fn item_height(&self, theme: &Theme) -> f64 {
        theme.metrics.nav.item_h
    }

    /// Prostokąt pozycji `index`.
    pub fn item_rect(&self, rect: Rect, theme: &Theme, index: usize) -> Rect {
        let h = self.item_height(theme);
        Rect::from_xywh(rect.x(), rect.y() + index as f64 * h, rect.width(), h)
    }

    /// Indeks pozycji pod punktem `p` (lub `None`).
    pub fn index_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<usize> {
        (0..self.items.len()).find(|&i| self.item_rect(rect, theme, i).contains(p))
    }

    /// Obsługuje klik: ustawia wybór. Zwraca `true`, gdy wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        match self.index_at(rect, theme, p) {
            Some(i) if i != self.selected => {
                self.selected = i;
                true
            }
            _ => false,
        }
    }

    pub fn hover_at(&mut self, rect: Rect, theme: &Theme, p: Option<Point>) {
        self.hovered = p.and_then(|p| self.index_at(rect, theme, p));
    }

    /// Rysuje szynę: tło, wskaźnik zaznaczenia i etykiety (lub inicjały).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        ctx.scene.fill_rect(rect, theme.colors.sidebar_bg);
        let body = theme.typography.body.size;
        let indicator_w = 3.0;
        for (i, label) in self.items.iter().enumerate() {
            let item = self.item_rect(rect, &theme, i);
            let selected = i == self.selected;
            if selected || self.hovered == Some(i) {
                ctx.scene.fill_rounded_rect(
                    item.deflate(libluna_core::Insets::new(4.0, 6.0, 4.0, 6.0)),
                    theme.radius.small,
                    theme.colors.shade,
                );
            }
            if selected {
                let bar = Rect::from_xywh(
                    rect.x() + 2.0,
                    item.y() + 8.0,
                    indicator_w,
                    (item.height() - 16.0).max(0.0),
                );
                ctx.scene.fill_rounded_rect(bar, 1.5, theme.colors.accent);
            }
            let text = if self.collapsed {
                label.chars().next().map(|c| c.to_uppercase().to_string())
            } else {
                Some(label.clone())
            };
            if let Some(text) = text {
                let x = if self.collapsed {
                    item.center().x
                } else {
                    item.x() + 16.0
                };
                ctx.label_left(
                    x,
                    item.center().y,
                    &text,
                    body,
                    weight::MEDIUM,
                    theme.colors.text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rail() -> NavigationRail {
        NavigationRail::new(["Przegląd", "Sieć", "System"])
    }

    fn area() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 200.0)
    }

    #[test]
    fn items_stack_vertically() {
        let theme = Theme::light();
        let r = rail();
        let h = r.item_height(&theme);
        let r0 = r.item_rect(area(), &theme, 0);
        let r2 = r.item_rect(area(), &theme, 2);
        assert_eq!(r0.height(), h);
        assert_eq!(r2.y(), 2.0 * h);
    }

    #[test]
    fn click_selects_item() {
        let theme = Theme::light();
        let mut r = rail();
        let target = r.item_rect(area(), &theme, 1).center();
        assert!(r.click_at(area(), &theme, target));
        assert_eq!(r.selected(), 1);
        assert_eq!(r.selected_label(), Some("Sieć"));
        assert!(!r.click_at(area(), &theme, target));
    }

    #[test]
    fn width_depends_on_mode() {
        let theme = Theme::light();
        let full = rail();
        let collapsed = rail().collapsed(true);
        assert!(collapsed.width(&theme) < full.width(&theme));
    }

    #[test]
    fn hover_at_clears_outside() {
        let theme = Theme::light();
        let mut r = rail();
        r.hover_at(
            area(),
            &theme,
            Some(r.item_rect(area(), &theme, 0).center()),
        );
        assert_eq!(r.hovered, Some(0));
        r.hover_at(area(), &theme, Some(Point::new(-10.0, -10.0)));
        assert_eq!(r.hovered, None);
    }
}
