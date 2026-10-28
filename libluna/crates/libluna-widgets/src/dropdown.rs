//! `DropDown` — kontrolka wyboru jednej opcji. Zamknięty stan to pudełko
//! z etykietą i chevronem; rozwinięcie realizuje [`Menu`] (patrz [`DropDown::menu`]).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;
use crate::menu::{Menu, MenuEntry};

/// Lista rozwijana.
#[derive(Debug, Clone)]
pub struct DropDown {
    options: Vec<String>,
    selected: usize,
}

impl DropDown {
    pub fn new<I, S>(options: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
        }
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_label(&self) -> Option<&str> {
        self.options.get(self.selected).map(String::as_str)
    }

    pub fn set_selected(&mut self, index: usize) {
        if !self.options.is_empty() {
            self.selected = index.min(self.options.len() - 1);
        }
    }

    /// Szerokość dopasowana do najdłuższej opcji.
    pub fn width(&self, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.dropdown;
        let body = theme.typography.body.size;
        let mut w = m.min_width;
        for option in &self.options {
            w = w.max(
                text.measure(option, body, weight::NORMAL).width
                    + 2.0 * m.pad_x
                    + m.chevron_gap
                    + m.chevron,
            );
        }
        w
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.dropdown.height
    }

    pub fn hit(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Zwraca `true`, jeśli klik trafił w kontrolkę (rozwinięcie robi aplikacja).
    pub fn click_at(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Pozycje do popoveru (menu rozwijane).
    pub fn menu(&self) -> Menu {
        Menu::new(
            self.options
                .iter()
                .cloned()
                .map(MenuEntry::action)
                .collect(),
        )
    }

    /// Rysuje zamkniętą kontrolkę.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        ctx.scene
            .fill_rounded_rect(rect, theme.radius.small, colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.small, 1.0, colors.border);

        let body = theme.typography.body.size;
        let pad_x = theme.metrics.dropdown.pad_x;
        let label = self.selected_label().unwrap_or("");
        ctx.label_left(
            rect.x() + pad_x,
            rect.center().y,
            label,
            body,
            weight::NORMAL,
            colors.text,
        );
        icon::chevron_down(
            ctx,
            colors.dim_text,
            Point::new(rect.right() - pad_x, rect.center().y),
            4.0,
            2.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_and_menu_len() {
        let mut dd = DropDown::new(["A", "B", "C"]);
        assert_eq!(dd.selected_label(), Some("A"));
        dd.set_selected(2);
        assert_eq!(dd.selected_label(), Some("C"));
        assert_eq!(dd.menu().entries().len(), 3);
    }

    #[test]
    fn hit_test() {
        let dd = DropDown::new(["A"]);
        let rect = Rect::from_xywh(0.0, 0.0, 160.0, 34.0);
        assert!(dd.click_at(rect, Point::new(10.0, 10.0)));
        assert!(!dd.click_at(rect, Point::new(300.0, 10.0)));
    }
}
