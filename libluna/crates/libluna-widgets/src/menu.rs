//! `Menu` — popover z listą pozycji (akcje, separatory, sekcje, skróty).
//!
//! Widget immediate: trzyma zawartość i stan podświetlenia. Aplikacja wywołuje
//! [`Menu::paint`] oraz [`Menu::index_at`] / [`Menu::click_at`] i sama decyduje o
//! zamknięciu (klik poza panelem, `Esc`). Widget liczy też własny rozmiar i potrafi
//! **umieścić się** pod kotwicą, przycinając do krawędzi okna.

use libluna_core::style::weight;
use libluna_core::{Insets, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;

/// Pozycja menu.
#[derive(Debug, Clone)]
pub enum MenuEntry {
    /// Akcja z etykietą, opcjonalnym skrótem i stanem aktywności.
    Action {
        label: String,
        shortcut: Option<String>,
        enabled: bool,
    },
    /// Nagłówek sekcji.
    Section(String),
    /// Poziomy separator.
    Separator,
}

impl MenuEntry {
    pub fn action(label: impl Into<String>) -> Self {
        MenuEntry::Action {
            label: label.into(),
            shortcut: None,
            enabled: true,
        }
    }

    pub fn shortcut(mut self, text: impl Into<String>) -> Self {
        if let MenuEntry::Action { shortcut, .. } = &mut self {
            *shortcut = Some(text.into());
        }
        self
    }

    pub fn disabled(mut self) -> Self {
        if let MenuEntry::Action { enabled, .. } = &mut self {
            *enabled = false;
        }
        self
    }

    pub fn section(title: impl Into<String>) -> Self {
        MenuEntry::Section(title.into())
    }

    pub fn separator() -> Self {
        MenuEntry::Separator
    }
}

/// Wiersz rozłożony w prostokącie menu (wynik [`Menu::rows`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuRow {
    pub entry: usize,
    pub rect: Rect,
    pub selectable: bool,
}

/// Popover z pozycjami.
#[derive(Debug, Clone, Default)]
pub struct Menu {
    entries: Vec<MenuEntry>,
    hovered: Option<usize>,
}

impl Menu {
    pub fn new(entries: Vec<MenuEntry>) -> Self {
        Self {
            entries,
            hovered: None,
        }
    }

    pub fn entries(&self) -> &[MenuEntry] {
        &self.entries
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    /// Wysokość panelu.
    pub fn height(&self, theme: &Theme) -> f64 {
        let m = theme.metrics.menu;
        let body: f64 = self
            .entries
            .iter()
            .map(|e| match e {
                MenuEntry::Action { .. } => m.item_h,
                MenuEntry::Section(_) => m.section_h,
                MenuEntry::Separator => m.separator_h,
            })
            .sum();
        m.pad_v * 2.0 + body
    }

    /// Sugerowana szerokość — dopasowana do najdłuższej pozycji (z etykietą i skrótem).
    pub fn width(&self, text: &TextEngine, theme: &Theme) -> f64 {
        let m = theme.metrics.menu;
        let body = theme.typography.body.size;
        let caption = theme.typography.caption.size;
        let mut w = m.min_width;
        for entry in &self.entries {
            if let MenuEntry::Action {
                label, shortcut, ..
            } = entry
            {
                let mut row = text.measure(label, body, weight::NORMAL).width + 2.0 * m.pad_h;
                if let Some(shortcut) = shortcut {
                    row += m.label_gap + text.measure(shortcut, caption, weight::NORMAL).width;
                }
                w = w.max(row);
            }
        }
        w
    }

    /// Umieszcza panel pod `anchor`, przycinając do `viewport`
    /// (przy prawej krawędzi wyrównuje do prawej, przy dolnej otwiera do góry).
    pub fn place(&self, anchor: Rect, width: f64, viewport: Rect, theme: &Theme) -> Rect {
        let m = theme.metrics.menu;
        let height = self.height(theme);
        let margin = m.margin;

        let mut x = anchor.x();
        if x + width > viewport.right() - margin {
            x = viewport.right() - margin - width;
        }
        x = x.max(viewport.x() + margin);

        let mut y = anchor.bottom() + m.anchor_gap;
        if y + height > viewport.bottom() - margin {
            y = anchor.y() - height - m.anchor_gap;
        }
        y = y.max(viewport.y() + margin);

        Rect::from_xywh(x, y, width, height)
    }

    /// Rozkłada wiersze od góry panelu.
    pub fn rows(&self, panel: Rect, theme: &Theme) -> Vec<MenuRow> {
        let m = theme.metrics.menu;
        let mut rows = Vec::with_capacity(self.entries.len());
        let mut y = panel.y() + m.pad_v;
        for (i, entry) in self.entries.iter().enumerate() {
            let (h, selectable) = match entry {
                MenuEntry::Action { enabled, .. } => (m.item_h, *enabled),
                MenuEntry::Section(_) => (m.section_h, false),
                MenuEntry::Separator => (m.separator_h, false),
            };
            rows.push(MenuRow {
                entry: i,
                rect: Rect::from_xywh(panel.x(), y, panel.width(), h),
                selectable,
            });
            y += h;
        }
        rows
    }

    /// Indeks akcji pod punktem `p` (sekcje, separatory i akcje wyłączone → `None`).
    pub fn index_at(&self, panel: Rect, p: Point, theme: &Theme) -> Option<usize> {
        self.rows(panel, theme)
            .into_iter()
            .find(|row| row.selectable && row.rect.contains(p))
            .map(|row| row.entry)
    }

    /// Obsługa kliknięcia: zwraca indeks wybranej akcji (decyzję o zamknięciu podejmuje aplikacja).
    pub fn click_at(&mut self, panel: Rect, p: Point, theme: &Theme) -> Option<usize> {
        self.index_at(panel, p, theme)
    }

    /// Podświetlenie z pozycji wskaźnika.
    pub fn hover_at(&mut self, panel: Rect, p: Option<Point>, theme: &Theme) {
        self.hovered = p.and_then(|p| self.index_at(panel, p, theme));
    }

    /// Rysuje panel.
    pub fn paint(&self, ctx: &mut PaintCtx, panel: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let radius = theme.radius;

        // Tło panelu + obramowanie.
        ctx.scene
            .fill_rounded_rect(panel, radius.medium, colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(panel, radius.medium, 1.0, colors.border);

        let body = theme.typography.body.size;
        let caption = theme.typography.caption.size;
        let m = theme.metrics.menu;

        for row in self.rows(panel, &theme) {
            match &self.entries[row.entry] {
                MenuEntry::Separator => {
                    let y = row.rect.center().y;
                    ctx.scene.fill_rect(
                        Rect::from_xywh(
                            panel.x() + m.pad_h,
                            y,
                            (panel.width() - 2.0 * m.pad_h).max(0.0),
                            1.0,
                        ),
                        colors.border,
                    );
                }
                MenuEntry::Section(title) => {
                    ctx.label_left(
                        panel.x() + m.pad_h,
                        row.rect.center().y,
                        title,
                        caption,
                        weight::BOLD,
                        colors.dim_text,
                    );
                }
                MenuEntry::Action {
                    label,
                    shortcut,
                    enabled,
                } => {
                    let highlighted = self.hovered == Some(row.entry) && *enabled;
                    if highlighted {
                        let hl =
                            row.rect
                                .deflate(Insets::new(0.0, m.pad_h - 2.0, 0.0, m.pad_h - 2.0));
                        ctx.scene.fill_rounded_rect(hl, radius.small, colors.shade);
                    }
                    let fg = if *enabled {
                        colors.text
                    } else {
                        colors.dim_text
                    };
                    ctx.label_left(
                        panel.x() + m.pad_h,
                        row.rect.center().y,
                        label,
                        body,
                        weight::NORMAL,
                        fg,
                    );
                    if let Some(shortcut) = shortcut {
                        ctx.label_right(
                            panel.right() - m.pad_h,
                            row.rect.center().y,
                            shortcut,
                            caption,
                            weight::NORMAL,
                            colors.dim_text,
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> Menu {
        Menu::new(vec![
            MenuEntry::action("New Window").shortcut("Ctrl+N"),
            MenuEntry::action("Shortcuts"),
            MenuEntry::separator(),
            MenuEntry::action("Settings"),
            MenuEntry::action("Disabled").disabled(),
        ])
    }

    fn panel(theme: &Theme) -> Rect {
        Rect::from_xywh(100.0, 100.0, 200.0, menu().height(theme))
    }

    #[test]
    fn height_counts_separators_and_items() {
        let m = Theme::light().metrics.menu;
        let h = menu().height(&Theme::light());
        assert_eq!(h, m.pad_v * 2.0 + 4.0 * m.item_h + m.separator_h);
    }

    #[test]
    fn index_at_skips_separator_and_disabled() {
        let theme = Theme::light();
        let m = menu();
        let p = panel(&theme);
        let rows = m.rows(p, &theme);
        assert_eq!(m.index_at(p, rows[0].rect.center(), &theme), Some(0));
        assert_eq!(m.index_at(p, rows[2].rect.center(), &theme), None); // separator
        assert_eq!(m.index_at(p, rows[4].rect.center(), &theme), None); // wyłączona
    }

    #[test]
    fn place_shifts_left_near_right_edge() {
        let theme = Theme::light();
        let m = menu();
        let viewport = Rect::from_xywh(0.0, 0.0, 300.0, 400.0);
        let anchor = Rect::from_xywh(280.0, 0.0, 20.0, 20.0);
        let placed = m.place(anchor, 200.0, viewport, &theme);
        assert!(placed.right() <= viewport.right() - theme.metrics.menu.margin + 0.001);
        assert!(placed.x() >= 0.0);
    }

    #[test]
    fn place_flips_up_near_bottom_edge() {
        let theme = Theme::light();
        let m = menu();
        let viewport = Rect::from_xywh(0.0, 0.0, 300.0, 400.0);
        let anchor = Rect::from_xywh(10.0, 380.0, 20.0, 20.0);
        let placed = m.place(anchor, 200.0, viewport, &theme);
        assert!(placed.bottom() <= anchor.y());
    }
}
