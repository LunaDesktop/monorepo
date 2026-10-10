//! `Popover` — generyczny panel unoszący się nad kotwicą (GTK4).
//!
//! W odróżnieniu od [`Menu`](crate::Menu) (gotowy popover z listą pozycji), `Popover` daje
//! samo **umiejscowienie i chrome**: cztery kierunki (`Below`/`Above`/`Right`/`Left`),
//! przycinanie do widoku i zamykanie po kliknięciu poza. Treść rysuje aplikacja.

use libluna_core::{Point, Rect, Size, Theme};

use crate::ctx::PaintCtx;

/// Kierunek osadzenia popovera względem kotwicy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PopoverPlacement {
    #[default]
    Below,
    Above,
    Right,
    Left,
}

/// Panel unoszący się nad kotwicą.
#[derive(Debug, Clone, Copy, Default)]
pub struct Popover {
    placement: PopoverPlacement,
    open: bool,
}

impl Popover {
    pub fn new(placement: PopoverPlacement) -> Self {
        Self {
            placement,
            open: false,
        }
    }

    pub fn placement(&self) -> PopoverPlacement {
        self.placement
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Przełącza stan; zwraca nowy stan.
    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.open
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Umieszcza panel o rozmiarze `size` przy `anchor`, przycinając do `viewport`.
    pub fn place(&self, anchor: Rect, size: Size, viewport: Rect, theme: &Theme) -> Rect {
        let margin = theme.spacing.sm;
        let gap = theme.spacing.xs.max(6.0);
        let (mut x, mut y) = match self.placement {
            PopoverPlacement::Below => (anchor.x(), anchor.bottom() + gap),
            PopoverPlacement::Above => (anchor.x(), anchor.y() - size.height - gap),
            PopoverPlacement::Right => (anchor.right() + gap, anchor.y()),
            PopoverPlacement::Left => (anchor.x() - size.width - gap, anchor.y()),
        };
        let max_x = (viewport.right() - margin - size.width).max(viewport.x() + margin);
        let max_y = (viewport.bottom() - margin - size.height).max(viewport.y() + margin);
        x = x.clamp(viewport.x() + margin, max_x);
        y = y.clamp(viewport.y() + margin, max_y);
        Rect::from_xywh(x, y, size.width, size.height)
    }

    /// Czy klik w `p` powinien zamknąć popover (poza panelem i poza kotwicą).
    pub fn dismiss_at(&self, panel: Rect, anchor: Rect, p: Point) -> bool {
        !panel.contains(p) && !anchor.contains(p)
    }

    /// Rysuje chrome panelu (tło karty + obramowanie).
    pub fn paint(&self, ctx: &mut PaintCtx, panel: Rect) {
        let theme = *ctx.theme;
        ctx.scene
            .fill_rounded_rect(panel, theme.radius.medium, theme.colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(panel, theme.radius.medium, 1.0, theme.colors.border);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_xywh(0.0, 0.0, 400.0, 300.0)
    }

    #[test]
    fn places_below_anchor_by_default() {
        let theme = Theme::light();
        let p = Popover::new(PopoverPlacement::Below);
        let anchor = Rect::from_xywh(50.0, 40.0, 30.0, 20.0);
        let panel = p.place(anchor, Size::new(120.0, 80.0), viewport(), &theme);
        assert_eq!(panel.x(), 50.0);
        assert!(panel.y() >= anchor.bottom());
    }

    #[test]
    fn flips_and_clamps_near_edges() {
        let theme = Theme::light();
        let p = Popover::new(PopoverPlacement::Below);
        let anchor = Rect::from_xywh(380.0, 290.0, 20.0, 20.0);
        let panel = p.place(anchor, Size::new(200.0, 120.0), viewport(), &theme);
        assert!(panel.right() <= viewport().right() - theme.spacing.sm + 1e-6);
        assert!(panel.bottom() <= viewport().bottom() - theme.spacing.sm + 1e-6);
        assert!(panel.x() >= viewport().x() + theme.spacing.sm);
    }

    #[test]
    fn dismiss_outside_panel_and_anchor() {
        let p = Popover::new(PopoverPlacement::Below);
        let panel = Rect::from_xywh(50.0, 70.0, 100.0, 80.0);
        let anchor = Rect::from_xywh(50.0, 40.0, 30.0, 20.0);
        assert!(!p.dismiss_at(panel, anchor, panel.center()));
        assert!(!p.dismiss_at(panel, anchor, anchor.center()));
        assert!(p.dismiss_at(panel, anchor, Point::new(390.0, 10.0)));
    }

    #[test]
    fn toggle_flips_open_state() {
        let mut p = Popover::new(PopoverPlacement::Above);
        assert!(!p.is_open());
        assert!(p.toggle());
        assert!(!p.toggle());
    }
}
