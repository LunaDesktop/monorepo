//! `Expander` — rozwijana sekcja (GTK4): nagłówek z tytułem i chevronem, pod nim treść.
//!
//! Widget immediate: trzyma stan rozwinięcia i tytuł. Aplikacja rysuje treść w prostokącie
//! [`Expander::content_rect`] (zerowej wysokości, gdy zwinięty).

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Rozwijana sekcja.
#[derive(Debug, Clone, Default)]
pub struct Expander {
    title: String,
    expanded: bool,
    hovered: bool,
}

impl Expander {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }

    /// Przełącza rozwinięcie; zwraca nowy stan.
    pub fn toggle(&mut self) -> bool {
        self.expanded = !self.expanded;
        self.expanded
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn header_height(&self, theme: &Theme) -> f64 {
        theme.metrics.button.height
    }

    /// Nagłówek (górna część `rect`).
    pub fn header_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        Rect::from_xywh(rect.x(), rect.y(), rect.width(), self.header_height(theme))
    }

    /// Treść pod nagłówkiem; zerowej wysokości, gdy zwinięty.
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let hdr = self.header_rect(rect, theme);
        let h = if self.expanded {
            (rect.height() - hdr.height()).max(0.0)
        } else {
            0.0
        };
        Rect::from_xywh(rect.x(), hdr.bottom(), rect.width(), h)
    }

    /// Kwadrat chevronu po lewej stronie nagłówka.
    pub fn chevron_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let hdr = self.header_rect(rect, theme);
        let s = theme.metrics.button.chevron * 2.0;
        Rect::from_xywh(hdr.x() + theme.spacing.sm, hdr.center().y - s / 2.0, s, s)
    }

    /// Obsługuje klik w nagłówek (przełącza rozwinięcie).
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        if self.header_rect(rect, theme).contains(p) {
            self.toggle();
            true
        } else {
            false
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let hdr = self.header_rect(rect, &theme);
        if self.hovered {
            ctx.scene.fill_rect(hdr, theme.colors.shade);
        }
        let chev = self.chevron_rect(rect, &theme).center();
        if self.expanded {
            icon::chevron_down(ctx, theme.colors.dim_text, chev, 4.0, 2.0);
        } else {
            icon::chevron_right(ctx, theme.colors.dim_text, chev, 4.0, 2.0);
        }
        let text_x = chev.x + theme.metrics.button.chevron + theme.spacing.sm;
        ctx.label_left(
            text_x,
            hdr.center().y,
            &self.title,
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.text,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 300.0, 200.0)
    }

    #[test]
    fn collapsed_has_no_content() {
        let theme = Theme::light();
        let e = Expander::new("Więcej");
        assert_eq!(e.content_rect(rect(), &theme).height(), 0.0);
        assert_eq!(
            e.header_rect(rect(), &theme).height(),
            e.header_height(&theme)
        );
    }

    #[test]
    fn expanded_content_fills_below_header() {
        let theme = Theme::light();
        let e = Expander::new("Więcej").expanded(true);
        let c = e.content_rect(rect(), &theme);
        assert!((c.height() - (200.0 - e.header_height(&theme))).abs() < 1e-9);
        assert_eq!(c.y(), e.header_height(&theme));
    }

    #[test]
    fn click_header_toggles_only_there() {
        let theme = Theme::light();
        let mut e = Expander::new("Sekcja");
        assert!(e.click_at(rect(), &theme, Point::new(150.0, 10.0)));
        assert!(e.is_expanded());
        assert!(!e.click_at(rect(), &theme, Point::new(150.0, 150.0)));
        assert!(e.is_expanded());
        assert!(e.click_at(rect(), &theme, Point::new(150.0, 10.0)));
        assert!(!e.is_expanded());
    }
}
