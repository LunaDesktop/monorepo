//! `TabBar` — pasek zakładek (np. w HeaderBarze).
//!
//! Widget immediate: trzyma zakładki, bieżący wybór i podświetlenie. Aplikacja wywołuje
//! [`TabBar::paint`] oraz [`TabBar::index_at`] / [`TabBar::click_at`]. Szerokości zakładek
//! wynikają z tokenów motywu ([`Metrics::tabs`](luna_core::TabsMetrics)) — w trybie
//! `expand` są równe, a bez niego dopasowane do etykiety.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::icon;

/// Szerokość przycisku przewijania (chevron) na krawędzi paska.
const SCROLL_W: f64 = 22.0;

/// Kierunek przewijania paska zakładek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Left,
    Right,
}

/// Pojedyncza zakładka.
#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    /// Etykieta (już przetłumaczona) — albo klucz, zależnie od aplikacji.
    pub title: String,
    /// Czy pokazywać przycisk zamknięcia.
    pub closable: bool,
}

impl Tab {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            closable: false,
        }
    }

    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }
}

/// Pasek zakładek.
#[derive(Debug, Clone, Default)]
pub struct TabBar {
    tabs: Vec<Tab>,
    selected: usize,
    hovered: Option<usize>,
    expand: bool,
    /// Przesunięcie poziome (gdy zakładki się nie mieszczą).
    scroll: f64,
}

impl TabBar {
    /// Tworzy pasek z listą etykiet (pierwsza zaznaczona, bez przycisków zamknięcia).
    pub fn new<I, S>(titles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            tabs: titles.into_iter().map(Tab::new).collect(),
            selected: 0,
            hovered: None,
            expand: true,
            scroll: 0.0,
        }
    }

    /// Tworzy pasek z gotowych zakładek.
    pub fn with_tabs(tabs: Vec<Tab>) -> Self {
        Self {
            tabs,
            selected: 0,
            hovered: None,
            expand: true,
            scroll: 0.0,
        }
    }

    /// Czy zakładki rozciągają się równo na całą szerokość (domyślnie tak).
    pub fn expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_title(&self) -> Option<&str> {
        self.tabs.get(self.selected).map(|t| t.title.as_str())
    }

    /// Ustawia wybór (poza zakresem jest przycinany).
    pub fn set_selected(&mut self, index: usize) {
        if !self.tabs.is_empty() {
            self.selected = index.min(self.tabs.len() - 1);
        }
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Wysokość paska (z tokenów motywu).
    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.tabs.height
    }

    /// Szerokości zakładek (bez odstępów) dla zadanego prostokąta.
    fn widths(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> Vec<f64> {
        let m = theme.metrics.tabs;
        let n = self.tabs.len();
        if n == 0 {
            return Vec::new();
        }
        if self.expand {
            let gaps = m.gap * (n as f64 - 1.0);
            let avail = (rect.width() - gaps).max(0.0);
            vec![(avail / n as f64).clamp(m.min_width, m.max_width); n]
        } else {
            self.tabs
                .iter()
                .map(|t| {
                    let mut w = text
                        .measure(&t.title, theme.typography.body.size, weight::NORMAL)
                        .width
                        + 2.0 * m.pad_x;
                    if t.closable {
                        w += m.close + m.pad_x / 2.0;
                    }
                    w.clamp(m.min_width, m.max_width)
                })
                .collect()
        }
    }

    /// Łączna szerokość zawartości (zakładki + odstępy).
    pub fn content_width(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.tabs;
        let n = self.tabs.len();
        if n == 0 {
            return 0.0;
        }
        self.widths(rect, text, theme).iter().sum::<f64>() + m.gap * (n as f64 - 1.0)
    }

    /// Maksymalne przewinięcie (0, gdy wszystko się mieści).
    pub fn max_scroll(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> f64 {
        let content = self.content_width(rect, text, theme);
        // Gdy się nie mieści, po bokach są przyciski przewijania — one zawężają widok.
        let inset = if content > rect.width() {
            SCROLL_W
        } else {
            0.0
        };
        let visible = (rect.width() - 2.0 * inset).max(0.0);
        (content - visible).max(0.0)
    }

    /// Czy zakładki nie mieszczą się w pasku (trzeba przewijać).
    pub fn is_overflowing(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> bool {
        self.max_scroll(rect, text, theme) > 0.5
    }

    pub fn scroll(&self) -> f64 {
        self.scroll
    }

    /// Ustawia przewinięcie (przycięte do zakresu).
    pub fn set_scroll(&mut self, scroll: f64, rect: Rect, text: &TextEngine, theme: &Theme) {
        self.scroll = scroll.clamp(0.0, self.max_scroll(rect, text, theme));
    }

    /// Przewija o `dx`; zwraca `true`, jeśli zawartość da się przewinąć.
    pub fn on_scroll(&mut self, dx: f64, rect: Rect, text: &TextEngine, theme: &Theme) -> bool {
        if !self.is_overflowing(rect, text, theme) {
            return false;
        }
        self.set_scroll(self.scroll + dx, rect, text, theme);
        true
    }

    /// Przewija tak, aby zakładka `index` była w pełni widoczna.
    pub fn scroll_into_view(&mut self, index: usize, rect: Rect, text: &TextEngine, theme: &Theme) {
        let segs = self.segments(rect, text, theme);
        let Some(seg) = segs.get(index).copied() else {
            return;
        };
        let inset = if self.is_overflowing(rect, text, theme) {
            SCROLL_W
        } else {
            0.0
        };
        let left = rect.x() + inset;
        let right = rect.right() - inset;
        if seg.x() < left {
            self.set_scroll(self.scroll - (left - seg.x()), rect, text, theme);
        } else if seg.right() > right {
            self.set_scroll(self.scroll + (seg.right() - right), rect, text, theme);
        }
    }

    /// Prostokąt przycisku przewijania przy danej krawędzi.
    fn scroll_button(&self, rect: Rect, dir: ScrollDir) -> Rect {
        let x = match dir {
            ScrollDir::Left => rect.x(),
            ScrollDir::Right => rect.right() - SCROLL_W,
        };
        Rect::from_xywh(x, rect.y(), SCROLL_W, rect.height())
    }

    /// Czy punkt trafia w przycisk przewijania (tylko gdy jest co przewijać w tę stronę).
    pub fn scroll_dir_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<ScrollDir> {
        if !self.is_overflowing(rect, text, theme) {
            return None;
        }
        let left = self.scroll_button(rect, ScrollDir::Left);
        let right = self.scroll_button(rect, ScrollDir::Right);
        if self.scroll > 0.0 && left.contains(p) {
            Some(ScrollDir::Left)
        } else if self.scroll < self.max_scroll(rect, text, theme) - 0.5 && right.contains(p) {
            Some(ScrollDir::Right)
        } else {
            None
        }
    }

    /// Prostokąty kolejnych zakładek (z uwzględnieniem przewijania).
    pub fn segments(&self, rect: Rect, text: &TextEngine, theme: &Theme) -> Vec<Rect> {
        let m = theme.metrics.tabs;
        let n = self.tabs.len();
        if n == 0 || rect.width() <= 0.0 {
            return Vec::new();
        }
        let widths = self.widths(rect, text, theme);
        let gaps = m.gap * (n as f64 - 1.0);
        let total: f64 = widths.iter().sum::<f64>() + gaps;
        let overflowing = total > rect.width();
        let inset = if overflowing { SCROLL_W } else { 0.0 };
        let avail = (rect.width() - 2.0 * inset).max(0.0);
        let mut x = if overflowing {
            rect.x() + inset - self.scroll
        } else {
            rect.x() + inset + ((avail - total) / 2.0).max(0.0)
        };
        let mut out = Vec::with_capacity(n);
        for w in widths {
            out.push(Rect::from_xywh(x, rect.y(), w, rect.height()));
            x += w + m.gap;
        }
        out
    }

    /// Indeks zakładki pod punktem `p` (lub `None`).
    pub fn index_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        self.segments(rect, text, theme)
            .into_iter()
            .position(|r| r.contains(p))
    }

    /// Indeks zakładki, której przycisk zamknięcia trafiono (tylko zakładki `closable`).
    pub fn close_at(
        &self,
        rect: Rect,
        text: &TextEngine,
        theme: &Theme,
        p: Point,
    ) -> Option<usize> {
        let m = theme.metrics.tabs;
        for (i, seg) in self.segments(rect, text, theme).into_iter().enumerate() {
            if !self.tabs[i].closable {
                continue;
            }
            let c = close_center(seg, m.close, m.pad_x);
            let box_ = Rect::from_xywh(c.x - m.close / 2.0, c.y - m.close / 2.0, m.close, m.close);
            if box_.contains(p) {
                return Some(i);
            }
        }
        None
    }

    /// Obsługuje klik: ustawia wybór. Zwraca `true`, jeśli wybór się zmienił.
    pub fn click_at(&mut self, rect: Rect, text: &TextEngine, theme: &Theme, p: Point) -> bool {
        match self.index_at(rect, text, theme, p) {
            Some(i) if i != self.selected => {
                self.selected = i;
                self.scroll_into_view(i, rect, text, theme);
                true
            }
            _ => false,
        }
    }

    /// Rysuje pasek zakładek w prostokącie `rect` (z przycinaniem i przewijaniem).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let radius = theme.radius.small;
        let size = theme.typography.body.size;
        let m = theme.metrics.tabs;
        let text = ctx.text;
        let overflow = self.is_overflowing(rect, text, &theme);

        ctx.scene.push_clip(rect);
        for (i, seg) in self.segments(rect, text, &theme).iter().enumerate() {
            let tab = &self.tabs[i];
            let selected = i == self.selected;
            let hovered = self.hovered == Some(i) && !selected;

            if selected {
                ctx.scene
                    .fill_rounded_rect(*seg, radius, colors.shade.scale_alpha(1.8));
            } else if hovered {
                ctx.scene.fill_rounded_rect(*seg, radius, colors.shade);
            }

            let fg = if selected {
                colors.text
            } else {
                colors.dim_text
            };
            let weight = if selected {
                weight::MEDIUM
            } else {
                weight::NORMAL
            };

            // Etykieta: wyśrodkowana, z miejscem na przycisk zamknięcia gdy jest widoczny.
            let close_shown = tab.closable && (selected || hovered);
            let offset = if tab.closable { m.close / 2.0 } else { 0.0 };
            let cx = seg.center().x - offset;
            ctx.label_center(cx, seg.center().y, &tab.title, size, weight, fg);

            if close_shown {
                let c = close_center(*seg, m.close, m.pad_x);
                icon::close(
                    ctx,
                    colors.dim_text,
                    Rect::from_xywh(c.x - m.close / 2.0, c.y - m.close / 2.0, m.close, m.close),
                    m.close * 0.28,
                );
            }
        }
        ctx.scene.pop_clip();

        // Przyciski przewijania na krawędziach, gdy zakładki się nie mieszczą.
        if overflow {
            let max = self.max_scroll(rect, text, &theme);
            if self.scroll > 0.0 {
                self.paint_scroll_button(ctx, rect, ScrollDir::Left);
            }
            if self.scroll < max - 0.5 {
                self.paint_scroll_button(ctx, rect, ScrollDir::Right);
            }
        }
    }

    fn paint_scroll_button(&self, ctx: &mut PaintCtx, rect: Rect, dir: ScrollDir) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let r = self.scroll_button(rect, dir);
        // Tło zasłaniające przewinięte zakładki + chevron.
        ctx.scene.fill_rect(r, colors.card_bg);
        let center = r.center();
        match dir {
            ScrollDir::Left => icon::chevron_left(ctx, colors.dim_text, center, 4.0, 2.0),
            ScrollDir::Right => icon::chevron_right(ctx, colors.dim_text, center, 4.0, 2.0),
        }
    }
}

/// Środek przycisku zamknięcia w zakładce (wspólny dla rysowania i trafień).
fn close_center(seg: Rect, close: f64, pad_x: f64) -> Point {
    Point::new(seg.right() - pad_x / 2.0 - close / 2.0, seg.center().y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 420.0, 34.0)
    }

    fn theme() -> Theme {
        Theme::light()
    }

    #[test]
    fn expand_partitions_and_centers() {
        let text = TextEngine::system();
        let bar = TabBar::new(["A", "B", "C"]);
        let segs = bar.segments(rect(), &text, &theme());
        assert_eq!(segs.len(), 3);
        // Równe szerokości, bez wychodzenia poza obszar.
        assert!((segs[0].width() - segs[1].width()).abs() < 1e-9);
        assert!(segs[2].right() <= rect().right() + 1e-9);
    }

    #[test]
    fn index_at_maps_points() {
        let text = TextEngine::system();
        let bar = TabBar::new(["A", "B", "C"]);
        let r = rect();
        let segs = bar.segments(r, &text, &theme());
        assert_eq!(bar.index_at(r, &text, &theme(), segs[0].center()), Some(0));
        assert_eq!(bar.index_at(r, &text, &theme(), segs[2].center()), Some(2));
        assert_eq!(
            bar.index_at(r, &text, &theme(), Point::new(10.0, 100.0)),
            None
        );
    }

    #[test]
    fn click_changes_selection_once() {
        let text = TextEngine::system();
        let mut bar = TabBar::new(["A", "B", "C"]);
        let r = rect();
        let segs = bar.segments(r, &text, &theme());
        assert!(bar.click_at(r, &text, &theme(), segs[1].center()));
        assert_eq!(bar.selected(), 1);
        assert!(!bar.click_at(r, &text, &theme(), segs[1].center()));
    }

    #[test]
    fn close_hit_only_for_closable_tabs() {
        let text = TextEngine::system();
        let theme = theme();
        let r = rect();
        let plain = TabBar::new(["A", "B"]);
        let seg0 = plain.segments(r, &text, &theme)[0];
        // Bez `closable` brak trafień w „×”.
        let c = close_center(seg0, theme.metrics.tabs.close, theme.metrics.tabs.pad_x);
        assert_eq!(plain.close_at(r, &text, &theme, c), None);

        let closable = TabBar::with_tabs(vec![Tab::new("A").closable(true), Tab::new("B")]);
        let seg0 = closable.segments(r, &text, &theme)[0];
        let c = close_center(seg0, theme.metrics.tabs.close, theme.metrics.tabs.pad_x);
        assert_eq!(closable.close_at(r, &text, &theme, c), Some(0));
    }

    #[test]
    fn set_selected_clamps() {
        let mut bar = TabBar::new(["A", "B"]);
        bar.set_selected(9);
        assert_eq!(bar.selected(), 1);
        assert_eq!(bar.selected_title(), Some("B"));
    }

    #[test]
    fn natural_width_clamps_to_bounds() {
        let text = TextEngine::system();
        let theme = theme();
        let m = theme.metrics.tabs;
        let bar = TabBar::new(["bardzo długa etykieta zakładki", "B"]).expand(false);
        let segs = bar.segments(rect(), &text, &theme);
        for s in &segs {
            assert!(s.width() >= m.min_width - 1e-9);
            assert!(s.width() <= m.max_width + 1e-9);
        }
    }

    #[test]
    fn overflow_scrolls_and_clamps() {
        let text = TextEngine::system();
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 34.0);
        let mut bar = TabBar::new(["A", "B", "C", "D", "E", "F"]);
        assert!(bar.is_overflowing(rect, &text, &theme));
        let max = bar.max_scroll(rect, &text, &theme);
        assert!(max > 0.0);

        assert!(bar.on_scroll(30.0, rect, &text, &theme));
        assert!((bar.scroll() - 30.0).abs() < 1e-9);
        bar.on_scroll(9999.0, rect, &text, &theme);
        assert!((bar.scroll() - max).abs() < 1e-9);
        bar.on_scroll(-9999.0, rect, &text, &theme);
        assert_eq!(bar.scroll(), 0.0);
    }

    #[test]
    fn scroll_into_view_reveals_last_tab() {
        let text = TextEngine::system();
        let theme = theme();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 34.0);
        let mut bar = TabBar::new(["A", "B", "C", "D", "E", "F"]);
        bar.scroll_into_view(5, rect, &text, &theme);
        let segs = bar.segments(rect, &text, &theme);
        assert!(segs[5].right() <= rect.right() - SCROLL_W + 1e-6);
        assert!(segs[5].x() >= rect.x() + SCROLL_W - 1e-6);
    }

    #[test]
    fn scroll_buttons_hit_only_when_scrollable() {
        let text = TextEngine::system();
        let theme = theme();
        let narrow = Rect::from_xywh(0.0, 0.0, 200.0, 34.0);
        let bar = TabBar::new(["A", "B", "C", "D", "E", "F"]);
        let left = bar.scroll_button(narrow, ScrollDir::Left);
        let right = bar.scroll_button(narrow, ScrollDir::Right);
        // Na starcie: lewy nieaktywny, prawy aktywny.
        assert_eq!(
            bar.scroll_dir_at(narrow, &text, &theme, left.center()),
            None
        );
        assert_eq!(
            bar.scroll_dir_at(narrow, &text, &theme, right.center()),
            Some(ScrollDir::Right)
        );
        // Gdy wszystko się mieści — brak przycisków.
        let fits = TabBar::new(["A"]);
        assert_eq!(
            fits.scroll_dir_at(rect(), &text, &theme, rect().center()),
            None
        );
    }
}
