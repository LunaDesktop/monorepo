//! `NavigationView` / `NavigationSplitView` / `OverlaySplitView` / `Sidebar` — nawigacja.
//!
//! Pomocniki układu i „chrome” nawigacji (jak `AdwNavigationView` i widoki dzielone):
//! wyznaczają prostokąty paneli i rysują pasek z przyciskiem „wstecz”.

use libluna_core::{Insets, Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Pasek nawigacji: „wstecz” + obszar tytułu + treść strony.
#[derive(Debug, Clone, Default)]
pub struct NavigationView {
    depth: usize,
}

impl NavigationView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Głębokość stosu (0 = strona główna).
    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn can_pop(&self) -> bool {
        self.depth > 0
    }

    pub fn push(&mut self) {
        self.depth += 1;
    }

    /// Zdejmuje stronę; zwraca `true`, jeśli coś zdjęto.
    pub fn pop(&mut self) -> bool {
        if self.depth > 0 {
            self.depth -= 1;
            true
        } else {
            false
        }
    }

    fn bar_height(theme: &Theme) -> f64 {
        theme.metrics.headerbar.height
    }

    /// Pasek nawigacji (nad treścią strony).
    pub fn bar_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        Rect::from_xywh(rect.x(), rect.y(), rect.width(), Self::bar_height(theme))
    }

    /// Prostokąt treści strony (poniżej paska).
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let h = Self::bar_height(theme);
        Rect::from_xywh(
            rect.x(),
            rect.y() + h,
            rect.width(),
            (rect.height() - h).max(0.0),
        )
    }

    /// Prostokąt przycisku „wstecz” (gdy jest co zdjąć).
    pub fn back_rect(&self, rect: Rect, theme: &Theme) -> Option<Rect> {
        if !self.can_pop() {
            return None;
        }
        let bar = self.bar_rect(rect, theme);
        let control = theme.metrics.headerbar.control;
        Some(Rect::from_xywh(
            rect.x() + theme.spacing.md,
            bar.center().y - control / 2.0,
            control,
            control,
        ))
    }

    pub fn back_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.back_rect(rect, theme).is_some_and(|r| r.contains(p))
    }

    /// Rysuje pasek nawigacji (tło + „wstecz”). Tytuł rysuje aplikacja (np. `WindowTitle`).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let bar = self.bar_rect(rect, &theme);
        ctx.scene.fill_rect(bar, theme.colors.headerbar_bg);
        ctx.scene.fill_rect(
            Rect::from_xywh(bar.x(), bar.bottom() - 1.0, bar.width(), 1.0),
            theme.colors.border,
        );
        if let Some(b) = self.back_rect(rect, &theme) {
            icon::chevron_left(ctx, theme.colors.text, b.center(), 4.0, 2.0);
        }
    }
}

/// Widok dzielony: panel boczny + treść (panel można schować).
#[derive(Debug, Clone)]
pub struct NavigationSplitView {
    show_sidebar: bool,
    sidebar_width: f64,
}

impl Default for NavigationSplitView {
    fn default() -> Self {
        Self {
            show_sidebar: true,
            sidebar_width: 220.0,
        }
    }
}

impl NavigationSplitView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sidebar_width(mut self, width: f64) -> Self {
        self.sidebar_width = width;
        self
    }

    pub fn show_sidebar(&self) -> bool {
        self.show_sidebar
    }

    pub fn set_show_sidebar(&mut self, show: bool) {
        self.show_sidebar = show;
    }

    pub fn toggle(&mut self) {
        self.show_sidebar = !self.show_sidebar;
    }

    /// Prostokąt panelu bocznego (gdy widoczny).
    pub fn sidebar_rect(&self, rect: Rect) -> Option<Rect> {
        self.show_sidebar
            .then(|| Rect::from_xywh(rect.x(), rect.y(), self.sidebar_width, rect.height()))
    }

    /// Prostokąt treści (obok panelu, albo cały gdy schowany).
    pub fn content_rect(&self, rect: Rect) -> Rect {
        let x = if self.show_sidebar {
            self.sidebar_width
        } else {
            0.0
        };
        Rect::from_xywh(
            rect.x() + x,
            rect.y(),
            (rect.width() - x).max(0.0),
            rect.height(),
        )
    }
}

/// Widok dzielony z panelem **nakładanym** (na wąskich oknach).
#[derive(Debug, Clone, Default)]
pub struct OverlaySplitView {
    open: bool,
    sidebar_width: f64,
}

impl OverlaySplitView {
    pub fn new() -> Self {
        Self {
            open: false,
            sidebar_width: 220.0,
        }
    }

    pub fn sidebar_width(mut self, width: f64) -> Self {
        self.sidebar_width = width;
        self
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Prostokąt panelu (przyklejony do lewej).
    pub fn sidebar_rect(&self, rect: Rect) -> Rect {
        Rect::from_xywh(rect.x(), rect.y(), self.sidebar_width, rect.height())
    }

    /// Czy punkt trafia w tło poza panelem (klik zamykający; tylko gdy otwarty).
    pub fn scrim_at(&self, rect: Rect, p: Point) -> bool {
        self.open && !self.sidebar_rect(rect).contains(p)
    }

    pub fn paint_scrim(&self, ctx: &mut PaintCtx, rect: Rect) {
        if self.open {
            let theme = *ctx.theme;
            use libluna_core::Color;
            ctx.scene.fill_rect(
                rect,
                Color::rgba(0.0, 0.0, 0.0, theme.metrics.sheet.scrim as f32),
            );
        }
    }
}

/// Panel boczny: tło + krawędź, obszar zawartości na listę ([`NavList`](crate::NavList)).
#[derive(Debug, Clone, Default)]
pub struct Sidebar;

impl Sidebar {
    pub fn new() -> Self {
        Self
    }

    /// Prostokąt zawartości (bez wewnętrznych wcięć).
    pub fn content_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        rect.deflate(Insets::symmetric(theme.spacing.sm, 0.0))
    }

    /// Rysuje tło panelu i krawędź po prawej.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        ctx.scene.fill_rect(rect, theme.colors.sidebar_bg);
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.right() - 1.0, rect.y(), 1.0, rect.height()),
            theme.colors.border,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 800.0, 600.0)
    }

    #[test]
    fn navigation_view_back_only_after_push() {
        let theme = Theme::light();
        let mut nav = NavigationView::new();
        assert!(nav.back_rect(rect(), &theme).is_none());
        assert!(!nav.can_pop());
        nav.push();
        assert!(nav.can_pop());
        let back = nav.back_rect(rect(), &theme).unwrap();
        assert!(nav.back_at(rect(), &theme, back.center()));
        assert!(nav.pop());
        assert!(!nav.pop());
    }

    #[test]
    fn split_view_content_and_sidebar() {
        let v = NavigationSplitView::new().sidebar_width(200.0);
        assert_eq!(v.sidebar_rect(rect()).unwrap().width(), 200.0);
        assert_eq!(v.content_rect(rect()).x(), 200.0);
        let mut v = v;
        v.set_show_sidebar(false);
        assert!(v.sidebar_rect(rect()).is_none());
        assert_eq!(v.content_rect(rect()), rect());
    }

    #[test]
    fn overlay_scrim_closes_only_when_open() {
        let mut o = OverlaySplitView::new().sidebar_width(200.0);
        assert!(!o.scrim_at(rect(), Point::new(500.0, 300.0)));
        o.set_open(true);
        assert!(o.scrim_at(rect(), Point::new(500.0, 300.0)));
        assert!(!o.scrim_at(rect(), Point::new(50.0, 300.0)));
    }
}
