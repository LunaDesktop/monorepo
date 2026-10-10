//! `PreferencesGroup` / `PreferencesPage` — grupa ustawień i strona z grup.
//!
//! `PreferencesGroup` rysuje tytuł nad ramką (jak „karta”) i zwraca obszar zawartości na wiersze
//! ([`Row`](crate::Row)). `PreferencesPage` układa grupy pionowo z odstępem i (opcjonalnie)
//! przewija całość — aplikacja rysuje wiersze w zwróconych obszarach.

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Grupa ustawień (tytuł + ramka z wierszami).
#[derive(Debug, Clone)]
pub struct PreferencesGroup {
    title: String,
}

impl PreferencesGroup {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Wysokość nagłówka grupy (z metryk).
    pub fn header_height(&self, theme: &Theme) -> f64 {
        theme.metrics.group.header
    }

    /// Prostokąt wiersza `index` wewnątrz obszaru zawartości grupy.
    pub fn row_rect(content: Rect, index: usize, row_h: f64) -> Rect {
        Rect::from_xywh(
            content.x(),
            content.y() + index as f64 * row_h,
            content.width(),
            row_h,
        )
    }

    /// Rysuje tytuł i ramkę grupy; zwraca obszar zawartości (na wiersze).
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) -> Rect {
        let theme = *ctx.theme;
        let header = theme.metrics.group.header;
        ctx.label_left(
            rect.x() + theme.metrics.group.pad_x,
            rect.y() + header / 2.0,
            &self.title,
            theme.typography.caption.size,
            weight::BOLD,
            theme.colors.dim_text,
        );
        let box_ = Rect::from_xywh(
            rect.x(),
            rect.y() + header,
            rect.width(),
            (rect.height() - header).max(0.0),
        );
        ctx.scene
            .fill_rounded_rect(box_, theme.radius.large, theme.colors.card_bg);
        ctx.scene
            .stroke_rounded_rect(box_, theme.radius.large, 1.0, theme.colors.border);
        box_
    }
}

/// Strona ustawień: pionowy stos grup.
#[derive(Debug, Clone, Default)]
pub struct PreferencesPage {
    groups: Vec<(PreferencesGroup, usize)>,
}

impl PreferencesPage {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dodaje grupę z liczbą wierszy.
    pub fn add(mut self, group: PreferencesGroup, rows: usize) -> Self {
        self.groups.push((group, rows));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    fn group_height(&self, rows: usize, row_h: f64, theme: &Theme) -> f64 {
        theme.metrics.group.header + rows as f64 * row_h
    }

    /// Wysokość całej zawartości (z odstępami między grupami).
    pub fn content_height(&self, row_h: f64, theme: &Theme) -> f64 {
        let gap = theme.metrics.group.gap;
        let n = self.groups.len();
        if n == 0 {
            return 0.0;
        }
        let sum: f64 = self
            .groups
            .iter()
            .map(|(_, rows)| self.group_height(*rows, row_h, theme))
            .sum();
        sum + gap * (n as f64 - 1.0)
    }

    /// Prostokąty ramek grup (bez nagłówków) w prostokącie `rect`.
    pub fn group_rects(&self, rect: Rect, row_h: f64, theme: &Theme) -> Vec<Rect> {
        let gap = theme.metrics.group.gap;
        let mut y = rect.y();
        let mut out = Vec::with_capacity(self.groups.len());
        for (_, rows) in &self.groups {
            let h = self.group_height(*rows, row_h, theme);
            out.push(Rect::from_xywh(rect.x(), y, rect.width(), h));
            y += h + gap;
        }
        out
    }

    /// Rysuje wszystkie grupy; zwraca obszary zawartości (na wiersze), w kolejności grup.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, row_h: f64) -> Vec<Rect> {
        let theme = *ctx.theme;
        let gap = theme.metrics.group.gap;
        let mut out = Vec::with_capacity(self.groups.len());
        let mut y = rect.y();
        for (group, rows) in &self.groups {
            let h = self.group_height(*rows, row_h, &theme);
            let grect = Rect::from_xywh(rect.x(), y, rect.width(), h);
            out.push(group.paint(ctx, grect));
            y += h + gap;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_paint_returns_content_below_header() {
        let theme = Theme::light();
        let text = libluna_render::TextEngine::system();
        let mut scene = libluna_render::Scene::new();
        let mut ctx = PaintCtx {
            scene: &mut scene,
            theme: &theme,
            text: &text,
        };
        let group = PreferencesGroup::new("Wygląd");
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 200.0);
        let content = group.paint(&mut ctx, rect);
        assert_eq!(content.y(), rect.y() + theme.metrics.group.header);
        assert_eq!(content.width(), rect.width());
    }

    #[test]
    fn page_stacks_groups_with_gap() {
        let theme = Theme::light();
        let page = PreferencesPage::new()
            .add(PreferencesGroup::new("A"), 3)
            .add(PreferencesGroup::new("B"), 2);
        let rect = Rect::from_xywh(0.0, 0.0, 300.0, 500.0);
        let rects = page.group_rects(rect, 56.0, &theme);
        assert_eq!(rects.len(), 2);
        let a_h = theme.metrics.group.header + 3.0 * 56.0;
        assert!((rects[1].y() - (a_h + theme.metrics.group.gap)).abs() < 1e-9);
        assert!(
            (page.content_height(56.0, &theme)
                - (a_h + theme.metrics.group.header + 2.0 * 56.0 + theme.metrics.group.gap))
                .abs()
                < 1e-9
        );
    }
}
