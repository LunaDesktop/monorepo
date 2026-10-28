//! `ItemView` — widok elementów (pliki/katalogi) w trybie **siatki** lub **listy**.
//!
//! Widget immediate: trzyma listę elementów, tryb wyświetlania, zaznaczenie i podświetlenie.
//! Aplikacja woła [`ItemView::paint`], [`ItemView::index_at`] / [`ItemView::click_at`] oraz
//! [`ItemView::slot_rect`] (np. do rysowania podpisów). Geometria wynika z tokenów motywu
//! ([`ItemViewMetrics`](libluna_core::ItemViewMetrics)).

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::display::ellipsize;
use crate::icon::{self, IconName};

/// Rodzaj elementu (wpływa na ikonę).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    Folder,
    #[default]
    File,
    Image,
    Audio,
    Video,
    Archive,
    Document,
    Code,
    Text,
}

/// Pojedynczy element (plik/katalog).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    /// Krótki dopisek (np. typ lub rozmiar).
    pub detail: Option<String>,
    pub kind: ItemKind,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            kind: ItemKind::File,
        }
    }

    pub fn folder(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            detail: None,
            kind: ItemKind::Folder,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn kind(mut self, kind: ItemKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Tryb prezentacji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

/// Widok elementów.
#[derive(Debug, Clone, Default)]
pub struct ItemView {
    items: Vec<Item>,
    mode: ViewMode,
    /// Zaznaczenie „główne” (kotwica dla Shift+klik i klawiatury).
    selected: Option<usize>,
    /// Wszystkie zaznaczone elementy (rosnąco, bez duplikatów).
    selected_set: Vec<usize>,
    hovered: Option<usize>,
    scroll: f64,
}

impl ItemView {
    pub fn new(items: Vec<Item>) -> Self {
        Self {
            items,
            ..Self::default()
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        self.selected = None;
        self.selected_set.clear();
        self.scroll = 0.0;
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            ViewMode::List => ViewMode::Grid,
            ViewMode::Grid => ViewMode::List,
        };
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Wszystkie zaznaczone indeksy (rosnąco).
    pub fn selected_indices(&self) -> &[usize] {
        &self.selected_set
    }

    pub fn is_selected(&self, i: usize) -> bool {
        self.selected_set.binary_search(&i).is_ok()
    }

    pub fn set_selected(&mut self, selected: Option<usize>) {
        self.selected = selected;
        self.selected_set.clear();
        if let Some(i) = selected {
            self.selected_set.push(i);
        }
    }

    /// Dodaje/usuwa element z zaznaczenia (Ctrl+klik); staje się kotwicą.
    pub fn toggle_selected(&mut self, i: usize) {
        match self.selected_set.binary_search(&i) {
            Ok(pos) => {
                self.selected_set.remove(pos);
            }
            Err(pos) => self.selected_set.insert(pos, i),
        }
        self.selected = Some(i);
    }

    /// Zaznacza zakres od kotwicy do `to` (Shift+klik).
    pub fn select_range(&mut self, to: usize) {
        let from = self.selected.unwrap_or(to);
        let (a, b) = if from <= to { (from, to) } else { (to, from) };
        self.selected_set = (a..=b).collect();
        self.selected = Some(to);
    }

    pub fn select_all(&mut self) {
        self.selected_set = (0..self.items.len()).collect();
        self.selected = self.items.len().checked_sub(1);
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
        self.selected_set.clear();
    }

    pub fn set_hovered(&mut self, hovered: Option<usize>) {
        self.hovered = hovered;
    }

    pub fn hovered(&self) -> Option<usize> {
        self.hovered
    }

    pub fn scroll(&self) -> f64 {
        self.scroll
    }

    pub fn set_scroll(&mut self, scroll: f64) {
        self.scroll = scroll.max(0.0);
    }

    /// Liczba kolumn w trybie siatki dla zadanej szerokości.
    pub fn columns(&self, rect: Rect, theme: &Theme) -> usize {
        let m = theme.metrics.itemview;
        if m.tile_w + m.gap <= 0.0 {
            return 1;
        }
        (((rect.width() + m.gap) / (m.tile_w + m.gap)).floor() as usize).max(1)
    }

    /// Wysokość całej zawartości dla zadanej szerokości (do przycinania przewijania).
    pub fn content_height(&self, rect: Rect, theme: &Theme) -> f64 {
        let m = theme.metrics.itemview;
        if self.items.is_empty() {
            return 0.0;
        }
        match self.mode {
            ViewMode::Grid => {
                let cols = self.columns(rect, theme);
                let rows = self.items.len().div_ceil(cols);
                rows as f64 * (m.tile_h + m.gap) - m.gap
            }
            ViewMode::List => self.items.len() as f64 * m.row_h,
        }
    }

    /// Maksymalne przewinięcie (0, gdy wszystko się mieści).
    pub fn max_scroll(&self, rect: Rect, theme: &Theme) -> f64 {
        (self.content_height(rect, theme) - rect.height()).max(0.0)
    }

    /// Przytina przewinięcie do dozwolonego zakresu.
    pub fn clamp_scroll(&mut self, rect: Rect, theme: &Theme) {
        self.scroll = self.scroll.clamp(0.0, self.max_scroll(rect, theme));
    }

    /// Prostokąt elementu `i` (z uwzględnieniem trybu i przewinięcia).
    pub fn slot_rect(&self, rect: Rect, i: usize, theme: &Theme) -> Rect {
        let m = theme.metrics.itemview;
        match self.mode {
            ViewMode::Grid => {
                let cols = self.columns(rect, theme);
                let col = i % cols;
                let row = i / cols;
                Rect::from_xywh(
                    rect.x() + col as f64 * (m.tile_w + m.gap),
                    rect.y() + row as f64 * (m.tile_h + m.gap) - self.scroll,
                    m.tile_w,
                    m.tile_h,
                )
            }
            ViewMode::List => Rect::from_xywh(
                rect.x(),
                rect.y() + i as f64 * m.row_h - self.scroll,
                rect.width(),
                m.row_h,
            ),
        }
    }

    /// Indeks elementu pod punktem `p` (tylko jeśli trafienie jest w widocznym obszarze).
    pub fn index_at(&self, rect: Rect, p: Point, theme: &Theme) -> Option<usize> {
        if !rect.contains(p) {
            return None;
        }
        (0..self.items.len()).find(|i| self.slot_rect(rect, *i, theme).contains(p))
    }

    /// Zaznacza element pod punktem `p`; zwraca jego indeks.
    pub fn click_at(&mut self, rect: Rect, p: Point, theme: &Theme) -> Option<usize> {
        let i = self.index_at(rect, p, theme)?;
        self.set_selected(Some(i));
        Some(i)
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.itemview;
        let colors = theme.colors;
        let body = theme.typography.body.size;
        let caption = theme.typography.caption.size;

        ctx.scene.push_clip(rect);
        for (i, item) in self.items.iter().enumerate() {
            let s = self.slot_rect(rect, i, &theme);
            // Wirtualizacja: elementy poza widokiem nie generują komend rysowania.
            // Bez tego katalog z tysiącami plików zamulał każdą klatkę.
            if s.bottom() <= rect.y() || s.y() >= rect.bottom() {
                continue;
            }
            let selected = self.is_selected(i);
            if selected {
                ctx.scene
                    .fill_rounded_rect(s, theme.radius.small, colors.shade.scale_alpha(1.8));
            } else if self.hovered == Some(i) {
                ctx.scene
                    .fill_rounded_rect(s, theme.radius.small, colors.shade);
            }

            match self.mode {
                ViewMode::Grid => {
                    let ib =
                        Rect::from_xywh(s.center().x - m.icon / 2.0, s.y() + 8.0, m.icon, m.icon);
                    draw_icon(ctx, item.kind, ib, colors.accent, colors.dim_text);
                    // Etykieta nie może wychodzić poza kafelek — inaczej długie nazwy
                    // nachodzą na sąsiednie kafelki i przestają być czytelne.
                    let max_w = (s.width() - 2.0 * theme.spacing.sm).max(1.0);
                    let label = ellipsize(&item.label, max_w, body, ctx.text);
                    ctx.label_center(
                        s.center().x,
                        ib.bottom() + 12.0,
                        &label,
                        body,
                        weight::NORMAL,
                        colors.text,
                    );
                    if let Some(detail) = &item.detail {
                        let detail = ellipsize(detail, max_w, caption, ctx.text);
                        ctx.label_center(
                            s.center().x,
                            s.bottom() - 8.0,
                            &detail,
                            caption,
                            weight::NORMAL,
                            colors.dim_text,
                        );
                    }
                }
                ViewMode::List => {
                    let ib =
                        Rect::from_xywh(s.x() + 12.0, s.center().y - m.icon / 2.0, m.icon, m.icon);
                    draw_icon(ctx, item.kind, ib, colors.accent, colors.dim_text);
                    let tx = ib.right() + 12.0;
                    let detail_w = item
                        .detail
                        .as_ref()
                        .map(|d| ctx.text.measure(d, caption, weight::NORMAL).width + 12.0)
                        .unwrap_or(0.0);
                    let max_w = (s.right() - 12.0 - tx - detail_w).max(1.0);
                    let label = ellipsize(&item.label, max_w, body, ctx.text);
                    ctx.label_left(tx, s.center().y, &label, body, weight::NORMAL, colors.text);
                    if let Some(detail) = &item.detail {
                        let detail =
                            ellipsize(detail, (s.width() * 0.35).max(40.0), caption, ctx.text);
                        ctx.label_right(
                            s.right() - 12.0,
                            s.center().y,
                            &detail,
                            caption,
                            weight::NORMAL,
                            colors.dim_text,
                        );
                    }
                }
            }
        }
        ctx.scene.pop_clip();
    }
}

fn draw_icon(ctx: &mut PaintCtx, kind: ItemKind, box_: Rect, folder: Color, file: Color) {
    match kind {
        ItemKind::Folder => icon::folder(ctx, folder, box_),
        ItemKind::File => icon::file(ctx, file, box_),
        ItemKind::Image => icon::paint_icon(ctx, IconName::Image, box_, file),
        ItemKind::Audio => icon::paint_icon(ctx, IconName::Music, box_, file),
        ItemKind::Video => icon::paint_icon(ctx, IconName::Video, box_, file),
        ItemKind::Archive => icon::paint_icon(ctx, IconName::Archive, box_, file),
        ItemKind::Document => icon::paint_icon(ctx, IconName::Document, box_, file),
        ItemKind::Code => icon::paint_icon(ctx, IconName::Code, box_, file),
        ItemKind::Text => icon::paint_icon(ctx, IconName::Document, box_, file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        vec![
            Item::folder("Projekty"),
            Item::new("Raport").detail("PDF"),
            Item::new("Notatki").detail("TXT"),
            Item::new("Budżet").detail("XLSX"),
        ]
    }

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 400.0, 200.0)
    }

    #[test]
    fn grid_columns_and_slots() {
        let theme = Theme::light();
        let mut v = ItemView::new(items());
        v.set_mode(ViewMode::Grid);
        // (400 + 12) / (100 + 12) = 3 kolumny.
        assert_eq!(v.columns(rect(), &theme), 3);
        let a = v.slot_rect(rect(), 0, &theme);
        let b = v.slot_rect(rect(), 1, &theme);
        assert!(b.x() > a.x());
        let d = v.slot_rect(rect(), 3, &theme); // 4. element → 2. rząd
        assert!(d.y() > a.y());
    }

    #[test]
    fn multi_selection_toggle_range_and_all() {
        let mut v = ItemView::new(items());
        v.set_selected(Some(1));
        assert!(v.is_selected(1));
        v.toggle_selected(3);
        assert_eq!(v.selected_indices(), &[1, 3]);
        v.toggle_selected(1);
        assert_eq!(v.selected_indices(), &[3]);
        v.set_selected(Some(1));
        v.select_range(3);
        assert_eq!(v.selected_indices(), &[1, 2, 3]);
        v.select_all();
        assert_eq!(v.selected_indices().len(), 4);
        v.clear_selection();
        assert!(v.selected_indices().is_empty());
    }

    #[test]
    fn list_slots_stack_vertically() {
        let theme = Theme::light();
        let v = ItemView::new(items());
        let r0 = v.slot_rect(rect(), 0, &theme);
        let r1 = v.slot_rect(rect(), 1, &theme);
        assert_eq!(r0.x(), r1.x());
        assert!((r1.y() - r0.y() - theme.metrics.itemview.row_h).abs() < 1e-9);
    }

    #[test]
    fn click_selects_item() {
        let theme = Theme::light();
        let mut v = ItemView::new(items());
        let center = v.slot_rect(rect(), 2, &theme).center();
        assert_eq!(v.click_at(rect(), center, &theme), Some(2));
        assert_eq!(v.selected(), Some(2));
        // Klik poza widokiem nic nie robi.
        assert_eq!(v.click_at(rect(), Point::new(1000.0, 1000.0), &theme), None);
    }

    #[test]
    fn toggle_mode_switches() {
        let mut v = ItemView::new(items());
        assert_eq!(v.mode(), ViewMode::List);
        v.toggle_mode();
        assert_eq!(v.mode(), ViewMode::Grid);
    }

    #[test]
    fn scroll_is_clamped_to_content() {
        let theme = Theme::light();
        let mut v = ItemView::new(items());
        let short = Rect::from_xywh(0.0, 0.0, 400.0, 100.0);
        v.set_scroll(9999.0);
        v.clamp_scroll(short, &theme);
        assert!(v.scroll() <= v.max_scroll(short, &theme) + 1e-9);
    }
}
