//! Widok listy z kolumnami (Nazwa / Rozmiar / Typ / Modyfikacja).
//!
//! `ItemView` z libLuny rysuje tylko etykietę z dopiskiem — do menedżera plików
//! potrzebne są prawdziwe kolumny, klikalne nagłówki i wirtualizacja. Ten moduł
//! rysuje listę bezpośrednio z `DirView`, więc przewijanie i zaznaczanie zostają
//! spójne z resztą aplikacji.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};
use libluna_widgets::{icon, IconName, PaintCtx};

use crate::model::{format_mtime, human_size, DirView, Kind, SortKey};

/// Wysokość paska nagłówków kolumn.
pub const HEADER_H: f64 = 26.0;

#[derive(Debug, Clone, Copy)]
pub struct Columns {
    pub name: Rect,
    pub size: Rect,
    pub kind: Rect,
    pub modified: Rect,
}

/// Szerokości kolumn: nazwa bierze resztę, reszta ma stałe minima.
pub fn columns(rect: Rect, _theme: &Theme) -> Columns {
    let size_w = 90.0;
    let kind_w = 120.0;
    let date_w = 140.0;
    let name_w = (rect.width() - size_w - kind_w - date_w).max(100.0);
    Columns {
        name: Rect::from_xywh(rect.x(), rect.y(), name_w, rect.height()),
        size: Rect::from_xywh(rect.x() + name_w, rect.y(), size_w, rect.height()),
        kind: Rect::from_xywh(rect.x() + name_w + size_w, rect.y(), kind_w, rect.height()),
        modified: Rect::from_xywh(
            rect.x() + name_w + size_w + kind_w,
            rect.y(),
            date_w,
            rect.height(),
        ),
    }
}

/// Obszar wierszy (poniżej nagłówka).
pub fn rows_rect(rect: Rect) -> Rect {
    Rect::from_xywh(
        rect.x(),
        rect.y() + HEADER_H,
        rect.width(),
        (rect.height() - HEADER_H).max(1.0),
    )
}

/// Trafienie w nagłówek kolumny (sortowanie).
pub fn header_at(rect: Rect, p: Point, theme: &Theme) -> Option<SortKey> {
    if p.y < rect.y() || p.y >= rect.y() + HEADER_H {
        return None;
    }
    let c = columns(rect, theme);
    if c.name.contains(p) {
        Some(SortKey::Name)
    } else if c.size.contains(p) {
        Some(SortKey::Size)
    } else if c.kind.contains(p) {
        Some(SortKey::Kind)
    } else if c.modified.contains(p) {
        Some(SortKey::Modified)
    } else {
        None
    }
}

/// Prostokąt wiersza (z przewinięciem).
pub fn row_rect(rows: Rect, i: usize, scroll: f64, theme: &Theme) -> Rect {
    Rect::from_xywh(
        rows.x(),
        rows.y() + i as f64 * theme.metrics.itemview.row_h - scroll,
        rows.width(),
        theme.metrics.itemview.row_h,
    )
}

/// Indeks wiersza pod punktem.
pub fn index_at(rows: Rect, p: Point, scroll: f64, len: usize, theme: &Theme) -> Option<usize> {
    if !rows.contains(p) {
        return None;
    }
    let row_h = theme.metrics.itemview.row_h;
    let index = ((p.y - rows.y() + scroll) / row_h).floor();
    if index < 0.0 {
        return None;
    }
    let index = index as usize;
    (index < len).then_some(index)
}

/// Rysuje nagłówek + widoczne wiersze.
pub fn paint(
    ctx: &mut PaintCtx,
    rect: Rect,
    dir: &DirView,
    selection: &[usize],
    hovered: Option<usize>,
    scroll: f64,
    theme: &Theme,
) {
    let colors = theme.colors;
    let c = columns(rect, theme);
    let rows = rows_rect(rect);

    // Nagłówek
    let header = Rect::from_xywh(rect.x(), rect.y(), rect.width(), HEADER_H);
    ctx.scene.fill_rect(header, colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(header.x(), header.bottom() - 1.0, header.width(), 1.0),
        colors.border,
    );
    let caption = theme.typography.caption.size;
    let arrow = if dir.ascending() { "▲" } else { "▼" };
    let labels = [
        (c.name, SortKey::Name, "Name", false),
        (c.size, SortKey::Size, "Size", true),
        (c.kind, SortKey::Kind, "Type", false),
        (c.modified, SortKey::Modified, "Modified", false),
    ];
    for (area, key, label, right) in labels {
        let text = if dir.sort_key() == key {
            format!("{} {arrow}", libluna_i18n::tr(label))
        } else {
            libluna_i18n::tr(label)
        };
        let color = if dir.sort_key() == key {
            colors.text
        } else {
            colors.dim_text
        };
        let cy = header.center().y;
        if right {
            ctx.label_right(
                area.right() - 10.0,
                cy,
                &text,
                caption,
                weight::MEDIUM,
                color,
            );
        } else {
            ctx.label_left(area.x() + 10.0, cy, &text, caption, weight::MEDIUM, color);
        }
    }

    // Wiersze (wirtualizacja)
    ctx.scene.push_clip(rows);
    let row_h = theme.metrics.itemview.row_h;
    let first = ((0.0f64).max(scroll / row_h)).floor() as usize;
    let visible = (rows.height() / row_h).ceil() as usize + 2;
    let visible_indices = dir.visible();
    for i in first..(first + visible).min(visible_indices.len()) {
        let entry = &dir.entries[visible_indices[i]];
        let r = row_rect(rows, i, scroll, theme);
        if selection.binary_search(&i).is_ok() {
            ctx.scene
                .fill_rounded_rect(r, theme.radius.small, colors.shade.scale_alpha(1.8));
        } else if hovered == Some(i) {
            ctx.scene
                .fill_rounded_rect(r, theme.radius.small, colors.shade);
        }
        let icon_box = Rect::from_xywh(r.x() + 10.0, r.center().y - 9.0, 18.0, 18.0);
        draw_kind(ctx, entry.kind, icon_box, colors.accent, colors.dim_text);
        let name_color = if entry.hidden {
            colors.dim_text
        } else {
            colors.text
        };
        ctx.label_left(
            c.name.x() + 38.0,
            r.center().y,
            &entry.name,
            theme.typography.body.size,
            weight::NORMAL,
            name_color,
        );
        if !entry.is_dir {
            ctx.label_right(
                c.size.right() - 10.0,
                r.center().y,
                &human_size(entry.size),
                theme.typography.caption.size,
                weight::NORMAL,
                colors.dim_text,
            );
        }
        ctx.label_left(
            c.kind.x() + 10.0,
            r.center().y,
            &entry.kind.label(),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        ctx.label_left(
            c.modified.x() + 10.0,
            r.center().y,
            &format_mtime(entry.modified),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    ctx.scene.pop_clip();
}

fn draw_kind(
    ctx: &mut PaintCtx,
    kind: Kind,
    rect: Rect,
    folder: libluna_core::Color,
    file: libluna_core::Color,
) {
    match kind {
        Kind::Folder => {
            // Najpierw pastelowy `places/folder.png` z aktywnego zestawu Luna-Icons;
            // wektorowy folder zostaje jako fallback bez zainstalowanych ikon.
            if !icon::paint_named_icon(ctx, Some("folder"), rect) {
                icon::folder(ctx, folder, rect);
            }
        }
        Kind::Image => icon::paint_icon(ctx, IconName::Image, rect, file),
        Kind::Audio => icon::paint_icon(ctx, IconName::Music, rect, file),
        Kind::Video => icon::paint_icon(ctx, IconName::Video, rect, file),
        Kind::Archive => icon::paint_icon(ctx, IconName::Archive, rect, file),
        Kind::Document => icon::paint_icon(ctx, IconName::Document, rect, file),
        Kind::Code => icon::paint_icon(ctx, IconName::Code, rect, file),
        Kind::Text => icon::paint_icon(ctx, IconName::Document, rect, file),
        Kind::Other => {
            if !icon::paint_named_icon(ctx, Some("application-x-executable"), rect) {
                icon::file(ctx, file, rect);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 700.0, 300.0)
    }

    #[test]
    fn columns_tile_the_width() {
        let theme = Theme::light();
        let c = columns(rect(), &theme);
        assert!(
            (c.name.width() + c.size.width() + c.kind.width() + c.modified.width() - 700.0).abs()
                < 0.01
        );
        assert!(c.modified.right() <= 700.0 + 0.01);
    }

    #[test]
    fn header_hits_sort_columns() {
        let theme = Theme::light();
        let c = columns(rect(), &theme);
        let p = Point::new(c.size.center().x, 12.0);
        assert_eq!(header_at(rect(), p, &theme), Some(SortKey::Size));
        assert_eq!(header_at(rect(), Point::new(5.0, 40.0), &theme), None);
    }

    #[test]
    fn row_index_respects_scroll() {
        let theme = Theme::light();
        let rows = rows_rect(rect());
        let row_h = theme.metrics.itemview.row_h;
        // Drugi wiersz przy przewinięciu 0.
        let p = Point::new(20.0, rows.y() + row_h * 1.5);
        assert_eq!(index_at(rows, p, 0.0, 10, &theme), Some(1));
        // Przy przewinięciu o jeden wiersz ten sam punkt to trzeci wiersz.
        assert_eq!(index_at(rows, p, row_h, 10, &theme), Some(2));
        // Poza listą.
        assert_eq!(
            index_at(rows, Point::new(20.0, rows.bottom() + 5.0), 0.0, 10, &theme),
            None
        );
    }
}
