//! Warstwa UI: układ, rysowanie i obsługa wejścia.
//!
//! Wszystkie napisy idą przez `libluna_i18n::tr` (klucz = angielski tekst).

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, IconTheme, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, paint_icon_themed, Button, ButtonVariant, ControlsSide, HeaderBar, IconName,
    MenuButton, PaintCtx, PathBar, ToggleButton, WindowControl,
};

use crate::app::{display_dir, App, Mode};
use crate::model::{human_size, MediaKind};
use crate::places;

const STATUS_H: f64 = 28.0;
const SIDEBAR_W: f64 = 194.0;
const DETAILS_W: f64 = 292.0;
const PAD: f64 = 10.0;
const TILE_MIN: f64 = 170.0;
const TILE_GAP: f64 = 12.0;
const GRID_PAD: f64 = 14.0;
const MEDIA_BG: Color = Color::from_rgba8(0x0f, 0x11, 0x16, 0xff);

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_ENTER: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_HOME: u32 = 0xff50;
const KEY_LEFT: u32 = 0xff51;
const KEY_UP: u32 = 0xff52;
const KEY_RIGHT: u32 = 0xff53;
const KEY_DOWN: u32 = 0xff54;
const KEY_PAGE_UP: u32 = 0xff55;
const KEY_PAGE_DOWN: u32 = 0xff56;
const KEY_END: u32 = 0xff57;
const KEY_F5: u32 = 0xffc2;
const KEY_F9: u32 = 0xffc6;
const KEY_F11: u32 = 0xffc8;
const KEY_KP_ADD: u32 = 0xffab;
const KEY_KP_SUBTRACT: u32 = 0xffad;

struct Layout {
    window: Size,
    header: Rect,
    sidebar: Rect,
    folders: Rect,
    content: Rect,
    grid: Rect,
    status: Rect,
    details: Option<Rect>,
}

/// Widgety umieszczone w obszarze `content` paska tytułu.
struct HeaderWidgets {
    back: Rect,
    path: Rect,
    details: Rect,
    menu: Rect,
}

#[derive(Debug, Clone, Copy)]
struct GridMetrics {
    area: Rect,
    cols: usize,
    tile_w: f64,
    tile_h: f64,
    thumb_h: f64,
    gap: f64,
    content_h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerAction {
    PagePrev,
    PageNext,
    ZoomOut,
    Fit,
    Actual,
    ZoomIn,
    Slideshow,
    External,
}

/// Zestaw przycisków dolnego paska podglądu (PDF dostaje nawigację stron).
fn viewer_actions(pdf: bool) -> Vec<ViewerAction> {
    let mut actions = Vec::new();
    if pdf {
        actions.push(ViewerAction::PagePrev);
        actions.push(ViewerAction::PageNext);
    }
    actions.extend([
        ViewerAction::ZoomOut,
        ViewerAction::Fit,
        ViewerAction::Actual,
        ViewerAction::ZoomIn,
    ]);
    if !pdf {
        actions.push(ViewerAction::Slideshow);
    }
    actions.push(ViewerAction::External);
    actions
}

fn viewer_action_label(action: ViewerAction) -> Option<String> {
    match action {
        ViewerAction::PagePrev => Some("‹".to_owned()),
        ViewerAction::PageNext => Some("›".to_owned()),
        ViewerAction::ZoomOut => Some("−".to_owned()),
        ViewerAction::Fit => Some(libluna_i18n::tr("Fit")),
        ViewerAction::Actual => Some("1:1".to_owned()),
        ViewerAction::ZoomIn => Some("+".to_owned()),
        ViewerAction::Slideshow | ViewerAction::External => None,
    }
}

fn viewer_action_icon(action: ViewerAction, slideshow: bool) -> Option<IconName> {
    match action {
        ViewerAction::Slideshow => Some(if slideshow {
            IconName::Pause
        } else {
            IconName::Play
        }),
        ViewerAction::External => Some(IconName::Globe),
        _ => None,
    }
}

fn viewer_action_enabled(app: &App, action: ViewerAction) -> bool {
    match action {
        ViewerAction::PagePrev => app.viewer.pdf.is_some_and(|pdf| pdf.page > 0),
        ViewerAction::PageNext => app.viewer.pdf.is_some_and(|pdf| pdf.page + 1 < pdf.pages),
        ViewerAction::Slideshow => app.viewer.pdf.is_none(),
        _ => true,
    }
}

struct ViewerUi {
    prev: Option<Rect>,
    next: Option<Rect>,
    toolbar: Option<Rect>,
    buttons: Vec<(ViewerAction, Rect)>,
    controls: Option<Rect>,
    mute: Option<Rect>,
    external: Option<Rect>,
}

fn layout(app: &App, size: Size, theme: &Theme) -> Layout {
    let hh = theme.metrics.headerbar.height;
    let header = Rect::from_xywh(0.0, 0.0, size.width, hh);
    let status_h = if app.mode == Mode::Gallery {
        STATUS_H
    } else {
        0.0
    };
    let status = Rect::from_xywh(
        0.0,
        (size.height - status_h).max(header.bottom()),
        size.width,
        status_h,
    );
    let body = Rect::from_xywh(
        0.0,
        header.bottom(),
        size.width,
        (status.y() - header.bottom()).max(1.0),
    );
    let sidebar_w = if app.mode == Mode::Gallery && app.sidebar && size.width > 760.0 {
        SIDEBAR_W.min(size.width * 0.30)
    } else {
        0.0
    };
    let sidebar = Rect::from_xywh(body.x(), body.y(), sidebar_w, body.height());
    let after_sidebar = Rect::from_xywh(
        sidebar.right(),
        body.y(),
        (body.right() - sidebar.right()).max(1.0),
        body.height(),
    );
    let folders_h = if app.mode == Mode::Gallery && !app.folders.is_empty() {
        38.0
    } else {
        0.0
    };
    let folders = Rect::from_xywh(
        after_sidebar.x(),
        after_sidebar.y(),
        after_sidebar.width(),
        folders_h,
    );
    let inner = Rect::from_xywh(
        after_sidebar.x(),
        after_sidebar.y() + folders_h,
        after_sidebar.width(),
        (after_sidebar.height() - folders_h).max(1.0),
    );
    let details = if app.details && inner.width() > 720.0 {
        Some(Rect::from_xywh(
            inner.right() - DETAILS_W,
            inner.y(),
            DETAILS_W,
            inner.height(),
        ))
    } else {
        None
    };
    let content = match details {
        Some(details) => Rect::from_xywh(
            inner.x(),
            inner.y(),
            (details.x() - inner.x()).max(1.0),
            inner.height(),
        ),
        None => inner,
    };
    Layout {
        window: size,
        header,
        sidebar,
        folders,
        content,
        grid: content,
        status,
        details,
    }
}

/// Rozkłada widgety wolnym obszarze `HeaderBar::content`.
///
/// To jest miejsce, które libLuna oddaje aplikacji obok tytułu i kontrolek okna —
/// nie rysujemy własnego drugiego paska nawigacji.
fn header_widgets(lay: &Layout, theme: &Theme) -> HeaderWidgets {
    let content = HeaderBar::new()
        .controls(ControlsSide::Right)
        .controls_layout(lay.header, theme)
        .content;
    let h = (lay.header.height() - 8.0).clamp(22.0, 32.0);
    let cy = lay.header.center().y;
    let back = Rect::from_xywh(content.x(), cy - h / 2.0, 32.0, h);

    let menu_h = MenuButton::burger().height(theme).clamp(22.0, h);
    let menu = Rect::from_xywh(content.right() - menu_h, cy - menu_h / 2.0, menu_h, menu_h);
    let details = Rect::from_xywh(menu.x() - 6.0 - 32.0, cy - h / 2.0, 32.0, h);

    let path = Rect::from_xywh(
        back.right() + 8.0,
        cy - 13.0,
        (details.x() - 8.0 - (back.right() + 8.0)).max(20.0),
        26.0,
    );
    HeaderWidgets {
        back,
        path,
        details,
        menu,
    }
}

fn grid_metrics(len: usize, area: Rect) -> GridMetrics {
    let area = area.deflate(Insets::splat(GRID_PAD));
    let usable = (area.width() + TILE_GAP).max(TILE_MIN + TILE_GAP);
    let cols = ((usable / (TILE_MIN + TILE_GAP)).floor() as usize).max(1);
    let tile_w =
        ((area.width() - TILE_GAP * (cols.saturating_sub(1) as f64)) / cols as f64).max(1.0);
    let thumb_h = (tile_w * 0.60).max(80.0);
    let tile_h = thumb_h + 46.0;
    let rows = len.div_ceil(cols);
    let content_h = if rows == 0 {
        0.0
    } else {
        rows as f64 * tile_h + (rows as f64 - 1.0) * TILE_GAP
    };
    GridMetrics {
        area,
        cols,
        tile_w,
        tile_h,
        thumb_h,
        gap: TILE_GAP,
        content_h,
    }
}

impl GridMetrics {
    fn tile_rect(&self, index: usize, offset: f64) -> Rect {
        let col = index % self.cols;
        let row = index / self.cols;
        Rect::from_xywh(
            self.area.x() + col as f64 * (self.tile_w + self.gap),
            self.area.y() + row as f64 * (self.tile_h + self.gap) - offset,
            self.tile_w,
            self.tile_h,
        )
    }

    fn visible_range(&self, offset: f64) -> (usize, usize) {
        if self.content_h <= 0.0 {
            return (0, 0);
        }
        let first_row = (offset / (self.tile_h + self.gap)).floor().max(0.0) as usize;
        let last_row = ((offset + self.area.height()) / (self.tile_h + self.gap)).ceil() as usize;
        let start = first_row * self.cols;
        let end = (last_row + 1) * self.cols;
        (start, end)
    }
}

fn path_components(path: &Path) -> Vec<(String, PathBuf)> {
    let home = places::home();
    let mut out = Vec::new();
    if path == home {
        out.push((libluna_i18n::tr("Home"), home));
        return out;
    }
    let (mut base, tail): (PathBuf, Vec<Component<'_>>) = if let Ok(relative) =
        path.strip_prefix(&home)
    {
        out.push((libluna_i18n::tr("Home"), home.clone()));
        (home, relative.components().collect())
    } else {
        out.push(("/".to_owned(), PathBuf::from("/")));
        (
            PathBuf::from("/"),
            path.components()
                .filter(|component| !matches!(component, Component::RootDir | Component::Prefix(_)))
                .collect(),
        )
    };
    for component in tail {
        if let Component::Normal(name) = component {
            base.push(name);
            out.push((name.to_string_lossy().into_owned(), base.clone()));
        }
    }
    out
}

fn ellipsize(s: &str, max_w: f64, size: f64, weight: u16, text: &TextEngine) -> String {
    if max_w <= 0.0 {
        return String::new();
    }
    if text.measure(s, size, weight).width <= max_w {
        return s.to_owned();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if text.measure(&candidate, size, weight).width <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

// ---------------------------------------------------------------------------
// Rysowanie
// ---------------------------------------------------------------------------

pub fn paint(app: &App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(app, size, theme);
    let hdr = header_widgets(&lay, theme);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);
    // Sam pasek tytułu (tło, kontrolki okna) i widgety w jego obszarze `content`.
    HeaderBar::new()
        .controls(ControlsSide::Right)
        .paint(&mut ctx, lay.header, app.controls_hover);
    paint_header_widgets(&mut ctx, app, &hdr);

    if app.mode == Mode::Gallery {
        paint_sidebar(&mut ctx, app, &lay);
        paint_folders(&mut ctx, app, &lay);
        paint_gallery(&mut ctx, app, &lay);
    } else {
        paint_viewer(&mut ctx, app, &lay);
    }
    if app.mode == Mode::Gallery {
        paint_status(&mut ctx, app, &lay);
    }
    if let Some(details) = lay.details {
        paint_details(&mut ctx, app, details);
    }

    if let Some(state) = &app.menu {
        state.menu.paint(&mut ctx, state.panel);
    }
    app.toast.paint(&mut ctx, size);
    if let Some(about) = &app.about {
        about.paint(&mut ctx, size);
    }
}

/// Widgety wewnątrz `HeaderBar::content` — w całości z libLuny.
fn paint_header_widgets(ctx: &mut PaintCtx, app: &App, hdr: &HeaderWidgets) {
    let pointer = app.pointer;
    let hovered = |rect: Rect| pointer.is_some_and(|p| rect.contains(p));

    let back_label = if app.mode == Mode::Viewer {
        "‹"
    } else {
        "↑"
    };
    let mut back = Button::new(back_label).variant(ButtonVariant::Flat);
    back.set_hovered(hovered(hdr.back));
    back.paint(ctx, hdr.back);

    let components = path_components(&app.dir);
    let labels: Vec<String> = components.iter().map(|(label, _)| label.clone()).collect();
    let mut bar = PathBar::new(labels);
    bar.set_hovered(pointer.and_then(|p| bar.index_at(hdr.path, ctx.text, ctx.theme, p)));
    bar.paint(ctx, hdr.path);

    let mut details = ToggleButton::new("i").active(app.details);
    details.set_hovered(hovered(hdr.details));
    details.paint(ctx, hdr.details);

    let mut menu = MenuButton::burger();
    menu.set_open(app.menu.is_some());
    menu.set_hovered(hovered(hdr.menu));
    menu.paint(ctx, hdr.menu);
}

fn paint_sidebar(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    if lay.sidebar.width() <= 0.0 {
        return;
    }
    ctx.scene
        .fill_rect(lay.sidebar, ctx.theme.colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.sidebar.right() - 1.0,
            lay.sidebar.y(),
            1.0,
            lay.sidebar.height(),
        ),
        ctx.theme.colors.border,
    );
    let inner = lay.sidebar.deflate(Insets::new(PAD, 12.0, 0.0, 12.0));
    ctx.label_left(
        inner.x(),
        inner.y() + 12.0,
        &libluna_i18n::tr("Places"),
        ctx.theme.typography.caption.size,
        weight::BOLD,
        ctx.theme.colors.dim_text,
    );
    let row_h = 34.0;
    let mut y = inner.y() + 30.0;
    for place in &app.places {
        let row = Rect::from_xywh(inner.x(), y, inner.width(), row_h);
        let active = app.dir == place.path;
        let hovered = app.pointer.is_some_and(|p| row.contains(p));
        if active {
            ctx.scene
                .fill_rounded_rect(row, 8.0, ctx.theme.colors.accent.scale_alpha(0.18));
        } else if hovered {
            ctx.scene
                .fill_rounded_rect(row, 8.0, ctx.theme.colors.shade);
        }
        let color = if active {
            ctx.theme.colors.accent
        } else {
            ctx.theme.colors.text
        };
        let icon = Rect::from_xywh(row.x() + 8.0, row.center().y - 9.0, 18.0, 18.0);
        paint_icon_themed(ctx, place.icon, icon, color, IconTheme::Monochrome);
        ctx.label_left(
            row.x() + 34.0,
            row.center().y,
            &ellipsize(
                &place.label,
                row.width() - 42.0,
                ctx.theme.typography.body.size,
                weight::MEDIUM,
                ctx.text,
            ),
            ctx.theme.typography.body.size,
            weight::MEDIUM,
            color,
        );
        y += row_h + 2.0;
    }
}

fn paint_folders(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    if lay.folders.height() <= 0.0 {
        return;
    }
    ctx.scene.fill_rect(lay.folders, ctx.theme.colors.window_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.folders.x(),
            lay.folders.bottom() - 1.0,
            lay.folders.width(),
            1.0,
        ),
        ctx.theme.colors.border,
    );
    for (index, rect) in folder_chips(app, lay.folders, ctx.theme, ctx.text) {
        let hovered = app.pointer.is_some_and(|p| rect.contains(p));
        ctx.scene.fill_rounded_rect(
            rect,
            8.0,
            if hovered {
                ctx.theme.colors.shade
            } else {
                ctx.theme.colors.card_bg
            },
        );
        paint_icon(
            ctx,
            IconName::Files,
            Rect::from_xywh(rect.x() + 7.0, rect.center().y - 8.0, 16.0, 16.0),
            ctx.theme.colors.dim_text,
        );
        let label = ellipsize(
            &app.folders[index].0,
            rect.width() - 34.0,
            ctx.theme.typography.caption.size,
            weight::MEDIUM,
            ctx.text,
        );
        ctx.label_left(
            rect.x() + 28.0,
            rect.center().y,
            &label,
            ctx.theme.typography.caption.size,
            weight::MEDIUM,
            ctx.theme.colors.text,
        );
    }
}

/// Kafelki folderów, które mieszczą się w pasku (od lewej).
fn folder_chips(app: &App, area: Rect, theme: &Theme, text: &TextEngine) -> Vec<(usize, Rect)> {
    let mut chips = Vec::new();
    let mut x = area.x() + PAD;
    let cap = theme.typography.caption.size;
    for (index, (name, _)) in app.folders.iter().enumerate() {
        let label = ellipsize(name, 132.0, cap, weight::MEDIUM, text);
        let width = text.measure(&label, cap, weight::MEDIUM).width + 34.0;
        let rect = Rect::from_xywh(x, area.center().y - 14.0, width, 28.0);
        if rect.right() > area.right() - PAD {
            break;
        }
        chips.push((index, rect));
        x += width + 6.0;
    }
    chips
}

fn paint_gallery(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let g = grid_metrics(app.items.len(), lay.grid);
    if app.items.is_empty() {
        let center = g.area.center();
        let icon = Rect::from_xywh(center.x - 32.0, center.y - 70.0, 64.0, 64.0);
        paint_icon(ctx, IconName::Image, icon, ctx.theme.colors.dim_text);
        ctx.label_center(
            center.x,
            center.y + 6.0,
            &libluna_i18n::tr("No media in this folder"),
            ctx.theme.typography.heading.size,
            weight::BOLD,
            ctx.theme.colors.text,
        );
        let subtitle = app
            .nav_error
            .clone()
            .unwrap_or_else(|| display_dir(&app.dir));
        ctx.label_center(
            center.x,
            center.y + 32.0,
            &ellipsize(
                &subtitle,
                (g.area.width() - 40.0).max(40.0),
                ctx.theme.typography.caption.size,
                weight::NORMAL,
                ctx.text,
            ),
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            ctx.theme.colors.dim_text,
        );
        return;
    }

    let offset = app.scroll.offset();
    let (start, end) = g.visible_range(offset);
    let end = end.min(app.items.len());
    ctx.scene.push_clip(g.area);
    for index in start..end {
        let tile = g.tile_rect(index, offset);
        if tile.bottom() < g.area.y() - 4.0 || tile.y() > g.area.bottom() + 4.0 {
            continue;
        }
        paint_tile(ctx, app, index, tile, &g);
    }
    ctx.scene.pop_clip();
    app.scroll.paint_scrollbar(ctx, g.area);
}

fn paint_thumb_image(
    ctx: &mut PaintCtx,
    area: Rect,
    image: &Arc<libluna_render::ImageData>,
    cover: bool,
    white_background: bool,
) {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    if white_background {
        ctx.scene
            .fill_rect(area, Color::from_rgba8(0xff, 0xff, 0xff, 0xff));
    }
    ctx.scene.push_clip(area);
    let (iw, ih) = (image.width.max(1) as f64, image.height.max(1) as f64);
    let scale = if cover {
        (area.width() / iw).max(area.height() / ih)
    } else {
        (area.width() / iw).min(area.height() / ih)
    };
    let (w, h) = (iw * scale, ih * scale);
    ctx.scene.image(
        Rect::from_xywh(area.center().x - w / 2.0, area.center().y - h / 2.0, w, h),
        Arc::clone(image),
    );
    ctx.scene.pop_clip();
}

fn paint_tile(ctx: &mut PaintCtx, app: &App, index: usize, tile: Rect, g: &GridMetrics) {
    let Some(item) = app.items.get(index) else {
        return;
    };
    let selected = index == app.selected;
    let hovered = app.pointer.is_some_and(|p| tile.contains(p));
    let theme = ctx.theme;

    ctx.scene
        .fill_rounded_rect(tile, 10.0, theme.colors.card_bg);
    if selected {
        ctx.scene
            .stroke_rounded_rect(tile, 10.0, 2.0, theme.colors.accent);
    } else {
        ctx.scene
            .stroke_rounded_rect(tile, 10.0, 1.0, theme.colors.border);
    }

    let thumb = Rect::from_xywh(
        tile.x() + 4.0,
        tile.y() + 4.0,
        (tile.width() - 8.0).max(1.0),
        (g.thumb_h - 8.0).max(1.0),
    );
    ctx.scene.fill_rounded_rect(thumb, 7.0, theme.colors.shade);

    match app.thumbs.get(&item.path) {
        Some(image) => paint_thumb_image(
            ctx,
            thumb,
            image,
            item.kind != MediaKind::Pdf,
            item.kind == MediaKind::Pdf,
        ),
        None => {
            let side = (thumb.height() * 0.34).clamp(24.0, 46.0);
            paint_icon(
                ctx,
                item.kind.icon(),
                Rect::from_xywh(
                    thumb.center().x - side / 2.0,
                    thumb.center().y - side / 2.0,
                    side,
                    side,
                ),
                theme
                    .colors
                    .dim_text
                    .scale_alpha(if app.thumbs.is_pending(&item.path) {
                        0.55
                    } else {
                        0.8
                    }),
            );
        }
    }

    if hovered && !selected {
        ctx.scene
            .fill_rounded_rect(thumb, 7.0, Color::rgba(1.0, 1.0, 1.0, 0.05));
    }

    // Plakietka formatu (np. „JPG”).
    let badge = item.format_badge();
    let cap = theme.typography.caption.size;
    let badge_w = ctx.text.measure(&badge, cap, weight::BOLD).width + 10.0;
    let badge_rect = Rect::from_xywh(
        thumb.right() - badge_w - 7.0,
        thumb.y() + 7.0,
        badge_w,
        18.0,
    );
    ctx.scene
        .fill_rounded_rect(badge_rect, 5.0, Color::rgba(0.0, 0.0, 0.0, 0.55));
    ctx.label_center(
        badge_rect.center().x,
        badge_rect.center().y,
        &badge,
        cap,
        weight::BOLD,
        Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
    );

    if item.kind == MediaKind::Video {
        let center = Point::new(thumb.x() + 22.0, thumb.bottom() - 22.0);
        ctx.scene
            .fill_circle(center, 13.0, Color::rgba(0.0, 0.0, 0.0, 0.6));
        paint_icon(
            ctx,
            IconName::Play,
            Rect::from_xywh(center.x - 7.0, center.y - 7.0, 14.0, 14.0),
            Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
        );
    }

    // Podpis: nazwa + rozmiar.
    let name = ellipsize(
        &item.name,
        tile.width() - 18.0,
        theme.typography.body.size,
        weight::MEDIUM,
        ctx.text,
    );
    ctx.label_left(
        tile.x() + 9.0,
        tile.y() + g.thumb_h + 6.0,
        &name,
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    let detail = format!("{} · {}", human_size(item.size), item.kind.label());
    ctx.label_left(
        tile.x() + 9.0,
        tile.y() + g.thumb_h + 26.0,
        &ellipsize(
            &detail,
            tile.width() - 18.0,
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn paint_status(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    ctx.scene.fill_rect(lay.status, ctx.theme.colors.window_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(lay.status.x(), lay.status.y(), lay.status.width(), 1.0),
        ctx.theme.colors.border,
    );
    let cap = ctx.theme.typography.caption.size;
    let status = ellipsize(
        &app.status_text(),
        (lay.status.width() * 0.55).max(40.0),
        cap,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        lay.status.x() + PAD,
        lay.status.center().y,
        &status,
        cap,
        weight::NORMAL,
        ctx.theme.colors.dim_text,
    );
    let index = if app.items.is_empty() {
        String::new()
    } else {
        format!("{} / {}", app.selected + 1, app.items.len())
    };
    let path = display_dir(&app.dir);
    let right = format!("{index}   {path}");
    ctx.label_right(
        lay.status.right() - PAD,
        lay.status.center().y,
        &ellipsize(
            &right,
            lay.status.width() * 0.5,
            cap,
            weight::NORMAL,
            ctx.text,
        ),
        cap,
        weight::NORMAL,
        ctx.theme.colors.dim_text,
    );
}

fn paint_details(ctx: &mut PaintCtx, app: &App, rect: Rect) {
    let theme = *ctx.theme;
    ctx.scene.fill_rect(rect, theme.colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(rect.x(), rect.y(), 1.0, rect.height()),
        theme.colors.border,
    );
    let inner = rect.deflate(Insets::new(14.0, 14.0, 14.0, 14.0));
    ctx.label_left(
        inner.x(),
        inner.y() + 10.0,
        &libluna_i18n::tr("Details"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );

    let Some(item) = app.shown_item() else {
        ctx.label_left(
            inner.x(),
            inner.y() + 42.0,
            &libluna_i18n::tr("No media in this folder"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        return;
    };

    // Podgląd / miniatura.
    let preview_h = (inner.width() * 0.56).min(164.0);
    let preview = Rect::from_xywh(inner.x(), inner.y() + 26.0, inner.width(), preview_h);
    ctx.scene
        .fill_rounded_rect(preview, 8.0, theme.colors.shade);
    match app.thumbs.get(&item.path) {
        Some(image) => paint_thumb_image(
            ctx,
            preview,
            image,
            item.kind != MediaKind::Pdf,
            item.kind == MediaKind::Pdf,
        ),
        None => {
            let side = 38.0;
            paint_icon(
                ctx,
                item.kind.icon(),
                Rect::from_xywh(
                    preview.center().x - side / 2.0,
                    preview.center().y - side / 2.0,
                    side,
                    side,
                ),
                theme.colors.dim_text,
            );
        }
    }

    let name = ellipsize(
        &item.name,
        inner.width(),
        theme.typography.body.size,
        weight::BOLD,
        ctx.text,
    );
    ctx.label_left(
        inner.x(),
        preview.bottom() + 16.0,
        &name,
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.text,
    );

    let mut y = preview.bottom() + 42.0;
    let mut row = |label: &str, value: &str, y: &mut f64| {
        ctx.label_left(
            inner.x(),
            *y,
            &libluna_i18n::tr(label),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        let shown = ellipsize(
            value,
            inner.width(),
            theme.typography.caption.size,
            weight::MEDIUM,
            ctx.text,
        );
        ctx.label_left(
            inner.x(),
            *y + 16.0,
            &shown,
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.text,
        );
        *y += 34.0;
    };
    row("Type", &item.kind.label(), &mut y);
    row("Size", &human_size(item.size), &mut y);
    row(
        "Modified",
        &crate::model::format_mtime(item.modified),
        &mut y,
    );
    if let Some((w, h)) = app.image_dimensions() {
        row("Dimensions", &format!("{w} × {h}"), &mut y);
    }
    if let Some(pdf) = app.viewer.pdf {
        row(
            "Pages",
            &format!("{} / {}", pdf.page + 1, pdf.pages),
            &mut y,
        );
    }
    if let Some(spec) = app.video_spec() {
        row(
            "Dimensions",
            &format!("{} × {}", spec.width, spec.height),
            &mut y,
        );
        row(
            "Duration",
            &crate::model::format_duration(spec.duration),
            &mut y,
        );
        row("Codec", &crate::model::codec_label(&spec.codec), &mut y);
    }
    row("Path", &item.path.display().to_string(), &mut y);
}

// ---------------------------------------------------------------------------
// Podgląd
// ---------------------------------------------------------------------------

fn image_dest(app: &App, canvas: Rect) -> Option<Rect> {
    let image = app.viewer.image.as_ref()?;
    let (iw, ih) = (image.width.max(1) as f64, image.height.max(1) as f64);
    if canvas.width() <= 1.0 || canvas.height() <= 1.0 {
        return None;
    }
    let base = (canvas.width() / iw).min(canvas.height() / ih);
    let scale = base * app.viewer.zoom;
    let (w, h) = (iw * scale, ih * scale);
    let max_x = ((w - canvas.width()) / 2.0).max(0.0);
    let max_y = ((h - canvas.height()) / 2.0).max(0.0);
    let pan = Point::new(
        app.viewer.pan.x.clamp(-max_x, max_x),
        app.viewer.pan.y.clamp(-max_y, max_y),
    );
    Some(Rect::from_xywh(
        canvas.center().x + pan.x - w / 2.0,
        canvas.center().y + pan.y - h / 2.0,
        w,
        h,
    ))
}

fn pdf_word_screen_rect(
    text: &crate::decode::PdfText,
    word: &crate::decode::PdfWord,
    dest: Rect,
) -> Rect {
    let sx = dest.width() / (text.width.max(1.0) as f64);
    let sy = dest.height() / (text.height.max(1.0) as f64);
    Rect::from_xywh(
        dest.x() + word.x as f64 * sx,
        dest.y() + word.y as f64 * sy,
        (word.width as f64 * sx).max(1.0),
        (word.height as f64 * sy).max(1.0),
    )
}

fn paint_pdf_selection(ctx: &mut PaintCtx, app: &App, dest: Rect) {
    let (Some(text), Some((a, b))) = (app.viewer.pdf_text.as_ref(), app.viewer.text_sel) else {
        return;
    };
    if text.words.is_empty() {
        return;
    }
    let (lo, hi) = (
        a.min(b).min(text.words.len() - 1),
        a.max(b).min(text.words.len() - 1),
    );
    let color = ctx.theme.colors.accent.scale_alpha(0.38);
    for word in &text.words[lo..=hi] {
        ctx.scene
            .fill_rect(pdf_word_screen_rect(text, word, dest), color);
    }
}

fn pdf_word_hit(app: &App, canvas: Rect, p: Point) -> Option<usize> {
    let text = app.viewer.pdf_text.as_ref()?;
    let dest = image_dest(app, canvas)?;
    text.words.iter().position(|word| {
        pdf_word_screen_rect(text, word, dest)
            .inflate(Insets::splat(2.0))
            .contains(p)
    })
}

fn pdf_nearest_word(app: &App, canvas: Rect, p: Point) -> Option<usize> {
    let text = app.viewer.pdf_text.as_ref()?;
    let dest = image_dest(app, canvas)?;
    if text.words.is_empty() {
        return None;
    }
    let sx = dest.width() / (text.width.max(1.0) as f64);
    let sy = dest.height() / (text.height.max(1.0) as f64);
    if sx <= 0.0 || sy <= 0.0 {
        return None;
    }
    let x = ((p.x - dest.x()) / sx) as f32;
    let y = ((p.y - dest.y()) / sy) as f32;
    text.words
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let da = word_distance(a, x, y);
            let db = word_distance(b, x, y);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
}

fn word_distance(word: &crate::decode::PdfWord, x: f32, y: f32) -> f32 {
    let dx = if x < word.x {
        word.x - x
    } else if x > word.x + word.width {
        x - (word.x + word.width)
    } else {
        0.0
    };
    let dy = if y < word.y {
        word.y - y
    } else if y > word.y + word.height {
        y - (word.y + word.height)
    } else {
        0.0
    };
    dx * dx + dy * dy
}

fn paint_viewer(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let canvas = lay.content;
    ctx.scene.fill_rect(canvas, MEDIA_BG);

    if let Some(image) = &app.viewer.image {
        if let Some(dest) = image_dest(app, canvas) {
            ctx.scene.push_clip(canvas);
            ctx.scene.image(dest, Arc::clone(image));
            paint_pdf_selection(ctx, app, dest);
            ctx.scene.pop_clip();
        }
    } else if app.viewer.loading {
        ctx.label_center(
            canvas.center().x,
            canvas.center().y,
            &libluna_i18n::tr("Loading…"),
            ctx.theme.typography.body.size,
            weight::NORMAL,
            Color::rgba(1.0, 1.0, 1.0, 0.65),
        );
    } else if let Some(error) = app
        .viewer
        .video_error
        .as_deref()
        .or(app.viewer.error.as_deref())
        .or_else(|| app.viewer.video.as_ref().and_then(|v| v.error()))
    {
        let icon = app
            .items
            .get(app.viewer.index)
            .map(|item| item.kind.icon())
            .unwrap_or(IconName::Video);
        paint_viewer_error(ctx, canvas, error, icon);
    } else if let Some(video) = &app.viewer.video {
        if let Some(frame) = video.frame() {
            draw_contain(ctx, canvas, frame);
        } else {
            ctx.label_center(
                canvas.center().x,
                canvas.center().y,
                &libluna_i18n::tr("Video is loading…"),
                ctx.theme.typography.body.size,
                weight::NORMAL,
                Color::rgba(1.0, 1.0, 1.0, 0.65),
            );
        }
    } else {
        paint_viewer_error(
            ctx,
            canvas,
            &libluna_i18n::tr("Cannot show this file"),
            IconName::Image,
        );
    }

    if let Some(item) = app.items.get(app.viewer.index) {
        paint_viewer_overlay(ctx, app, item, canvas);
    }
    paint_viewer_controls(ctx, app, canvas);
}

fn draw_contain(ctx: &mut PaintCtx, area: Rect, image: &Arc<libluna_render::ImageData>) {
    let (iw, ih) = (image.width.max(1) as f64, image.height.max(1) as f64);
    let scale = (area.width() / iw).min(area.height() / ih);
    let (w, h) = (iw * scale, ih * scale);
    let dest = Rect::from_xywh(area.center().x - w / 2.0, area.center().y - h / 2.0, w, h);
    ctx.scene.push_clip(area);
    ctx.scene.image(dest, Arc::clone(image));
    ctx.scene.pop_clip();
}

fn paint_viewer_error(ctx: &mut PaintCtx, area: Rect, error: &str, icon: IconName) {
    let center = area.center();
    paint_icon(
        ctx,
        icon,
        Rect::from_xywh(center.x - 26.0, center.y - 74.0, 52.0, 52.0),
        Color::rgba(1.0, 1.0, 1.0, 0.5),
    );
    ctx.label_center(
        center.x,
        center.y + 2.0,
        &libluna_i18n::tr("Cannot show this file"),
        ctx.theme.typography.heading.size,
        weight::BOLD,
        Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
    );
    let message = libluna_i18n::tr(error);
    ctx.label_center(
        center.x,
        center.y + 28.0,
        &ellipsize(
            &message,
            (area.width() - 60.0).max(60.0),
            ctx.theme.typography.body.size,
            weight::NORMAL,
            ctx.text,
        ),
        ctx.theme.typography.body.size,
        weight::NORMAL,
        Color::rgba(1.0, 1.0, 1.0, 0.65),
    );
}

fn overlay_pill(ctx: &mut PaintCtx, rect: Rect, text: &str) {
    ctx.scene
        .fill_rounded_rect(rect, rect.height() / 2.0, Color::rgba(0.0, 0.0, 0.0, 0.52));
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        text,
        ctx.theme.typography.caption.size,
        weight::MEDIUM,
        Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
    );
}

fn paint_viewer_overlay(
    ctx: &mut PaintCtx,
    app: &App,
    item: &crate::model::MediaItem,
    canvas: Rect,
) {
    let cap = ctx.theme.typography.caption.size;
    let name = ellipsize(
        &item.name,
        (canvas.width() * 0.5).max(80.0),
        cap,
        weight::MEDIUM,
        ctx.text,
    );
    let w = ctx.text.measure(&name, cap, weight::MEDIUM).width + 22.0;
    let pill = Rect::from_xywh(canvas.x() + 12.0, canvas.y() + 10.0, w, 26.0);
    overlay_pill(ctx, pill, &name);

    let counter = format!("{} / {}", app.viewer.index + 1, app.items.len().max(1));
    let cw = ctx.text.measure(&counter, cap, weight::MEDIUM).width + 22.0;
    let counter_rect = Rect::from_xywh(pill.right() + 6.0, pill.y(), cw, 26.0);
    overlay_pill(ctx, counter_rect, &counter);

    let info = if let Some(spec) = app.video_spec() {
        spec.summary()
    } else if let Some(pdf) = app.viewer.pdf {
        let pages = format!(
            "{} {} / {}",
            libluna_i18n::tr("Page"),
            pdf.page + 1,
            pdf.pages
        );
        match app.image_dimensions() {
            Some((w, h)) => format!("{pages} · {w} × {h}"),
            None => pages,
        }
    } else if let Some((w, h)) = app.image_dimensions() {
        let base = if canvas.width() > 0.0 && canvas.height() > 0.0 {
            (canvas.width() / w as f64).min(canvas.height() / h as f64)
        } else {
            1.0
        };
        format!("{w} × {h} · {:.0}%", base * app.viewer.zoom * 100.0)
    } else {
        String::new()
    };
    if !info.is_empty() {
        let iw = ctx.text.measure(&info, cap, weight::NORMAL).width + 22.0;
        let rect = Rect::from_xywh(
            (canvas.right() - iw - 12.0).max(canvas.x() + 12.0),
            canvas.y() + 10.0,
            iw.min(canvas.width() - 24.0),
            26.0,
        );
        overlay_pill(
            ctx,
            rect,
            &ellipsize(&info, rect.width() - 12.0, cap, weight::NORMAL, ctx.text),
        );
    }
}

fn viewer_ui(app: &App, canvas: Rect, theme: &Theme, text: &TextEngine) -> ViewerUi {
    let mut ui = ViewerUi {
        prev: None,
        next: None,
        toolbar: None,
        buttons: Vec::new(),
        controls: None,
        mute: None,
        external: None,
    };
    let cy = canvas.center().y;
    if app.items.len() > 1 {
        ui.prev = Some(Rect::from_xywh(canvas.x() + 12.0, cy - 19.0, 38.0, 38.0));
        ui.next = Some(Rect::from_xywh(
            canvas.right() - 50.0,
            cy - 19.0,
            38.0,
            38.0,
        ));
    }

    if app.viewer.video.is_some() {
        let h = app.viewer.controls.height(theme);
        let w = (canvas.width() - 200.0)
            .clamp(300.0, 640.0)
            .min((canvas.width() - 24.0).max(1.0));
        let controls = Rect::from_xywh(
            canvas.center().x - w / 2.0,
            canvas.bottom() - h - 16.0,
            w,
            h,
        );
        ui.controls = Some(controls);
        let size = 36.0;
        let gap = 8.0;
        let right_x = controls.right() + gap;
        let (mute, external) = if right_x + size <= canvas.right() - 10.0 {
            let mute = Rect::from_xywh(right_x, controls.center().y - size / 2.0, size, size);
            let external = Rect::from_xywh(mute.right() + gap, mute.y(), size, size);
            let external = (external.right() <= canvas.right() - 10.0).then_some(external);
            (Some(mute), external)
        } else {
            let mute = Rect::from_xywh(
                controls.x() - size - gap,
                controls.center().y - size / 2.0,
                size,
                size,
            );
            if mute.x() >= canvas.x() + 10.0 {
                let external = Rect::from_xywh(mute.x() - size - gap, mute.y(), size, size);
                let external = (external.x() >= canvas.x() + 10.0).then_some(external);
                (Some(mute), external)
            } else {
                (None, None)
            }
        };
        ui.mute = mute;
        ui.external = external;
    } else if app.viewer.image.is_some() {
        let actions = viewer_actions(app.viewer.pdf.is_some());
        let cap = theme.typography.caption.size;
        let gap = 4.0;
        let widths: Vec<f64> = actions
            .iter()
            .map(|action| match viewer_action_label(*action) {
                Some(label) => (text.measure(&label, cap, weight::MEDIUM).width + 20.0).max(32.0),
                None => 32.0,
            })
            .collect();
        let total: f64 = widths.iter().sum::<f64>() + gap * (widths.len() as f64 - 1.0) + 12.0;
        let total = total.min((canvas.width() - 20.0).max(1.0));
        let h = 38.0;
        let toolbar = Rect::from_xywh(
            canvas.center().x - total / 2.0,
            canvas.bottom() - h - 14.0,
            total,
            h,
        );
        ui.toolbar = Some(toolbar);
        let mut x = toolbar.x() + 6.0;
        let by = toolbar.center().y - 15.0;
        for (index, action) in actions.iter().enumerate() {
            let width = widths[index];
            ui.buttons
                .push((*action, Rect::from_xywh(x, by, width, 30.0)));
            x += width + gap;
        }
    }
    ui
}

fn paint_viewer_controls(ctx: &mut PaintCtx, app: &App, canvas: Rect) {
    let ui = viewer_ui(app, canvas, ctx.theme, ctx.text);
    let pointer = app.pointer;

    // Strzałki poprzedni/następny.
    for (rect, icon) in [
        (ui.prev, IconName::ChevronLeft),
        (ui.next, IconName::ChevronRight),
    ] {
        let Some(rect) = rect else { continue };
        let hovered = pointer.is_some_and(|p| rect.contains(p));
        ctx.scene.fill_circle(
            rect.center(),
            19.0,
            Color::rgba(0.0, 0.0, 0.0, if hovered { 0.55 } else { 0.32 }),
        );
        paint_icon(
            ctx,
            icon,
            Rect::from_xywh(rect.center().x - 9.0, rect.center().y - 9.0, 18.0, 18.0),
            Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
        );
    }

    if let Some(toolbar) = ui.toolbar {
        ctx.scene.fill_rounded_rect(
            toolbar,
            toolbar.height() / 2.0,
            Color::rgba(0.0, 0.0, 0.0, 0.5),
        );
        for (action, rect) in &ui.buttons {
            let hovered = pointer.is_some_and(|p| rect.contains(p));
            let active = *action == ViewerAction::Slideshow && app.viewer.slideshow;
            let enabled = viewer_action_enabled(app, *action);
            let icon = viewer_action_icon(*action, app.viewer.slideshow);
            let label = viewer_action_label(*action);
            if (hovered && enabled) || active {
                ctx.scene.fill_rounded_rect(
                    *rect,
                    7.0,
                    if active {
                        ctx.theme.colors.accent
                    } else {
                        Color::rgba(1.0, 1.0, 1.0, 0.16)
                    },
                );
            }
            let color = if enabled {
                Color::from_rgba8(0xff, 0xff, 0xff, 0xff)
            } else {
                Color::rgba(1.0, 1.0, 1.0, 0.35)
            };
            if let Some(icon) = icon {
                paint_icon(
                    ctx,
                    icon,
                    Rect::from_xywh(rect.center().x - 8.0, rect.center().y - 8.0, 16.0, 16.0),
                    color,
                );
            } else if let Some(label) = label {
                ctx.label_center(
                    rect.center().x,
                    rect.center().y,
                    &label,
                    ctx.theme.typography.caption.size,
                    weight::MEDIUM,
                    color,
                );
            }
        }
    }

    if let Some(controls) = ui.controls {
        app.viewer.controls.paint(ctx, controls);
        if let Some(mute) = ui.mute {
            let hovered = pointer.is_some_and(|p| mute.contains(p));
            ctx.scene.fill_circle(
                mute.center(),
                18.0,
                if hovered {
                    Color::rgba(1.0, 1.0, 1.0, 0.2)
                } else {
                    Color::rgba(1.0, 1.0, 1.0, 0.12)
                },
            );
            let icon = Rect::from_xywh(mute.center().x - 9.0, mute.center().y - 9.0, 18.0, 18.0);
            let color = if app.prefs.muted {
                ctx.theme.colors.error
            } else {
                Color::from_rgba8(0xff, 0xff, 0xff, 0xff)
            };
            paint_icon(ctx, IconName::Volume, icon, color);
            if app.prefs.muted {
                ctx.scene.stroke_line(
                    Point::new(icon.x() + 2.0, icon.bottom() - 2.0),
                    Point::new(icon.right() - 2.0, icon.y() + 2.0),
                    2.0,
                    ctx.theme.colors.error,
                );
            }
        }
        // „Otwórz zewnętrznie” obok wyciszenia.
        if let Some(button) = ui.external {
            let hovered = pointer.is_some_and(|p| button.contains(p));
            ctx.scene.fill_circle(
                button.center(),
                18.0,
                if hovered {
                    Color::rgba(1.0, 1.0, 1.0, 0.2)
                } else {
                    Color::rgba(1.0, 1.0, 1.0, 0.12)
                },
            );
            paint_icon(
                ctx,
                IconName::Globe,
                Rect::from_xywh(button.center().x - 9.0, button.center().y - 9.0, 18.0, 18.0),
                Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Wejście
// ---------------------------------------------------------------------------

pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme);
    let hdr = header_widgets(&lay, theme);
    sync_metrics(app, &lay);
    let _ = text;

    // Modal „O aplikacji” blokuje resztę.
    if app.about.is_some() {
        for event in events {
            if let Event::Key(ke) = event {
                if ke.state == ButtonState::Pressed && ke.keysym == KEY_ESCAPE {
                    app.about = None;
                }
            }
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    let close = app.about.as_ref().is_some_and(|about| {
                        let rects = about.rects(size, theme, text);
                        about.action_at(size, theme, text, pe.position).is_some()
                            || rects.dialog.close.contains(pe.position)
                            || !rects.dialog.panel.contains(pe.position)
                    });
                    if close {
                        app.about = None;
                    }
                }
            }
        }
        return;
    }

    // Menu.
    if app.menu.is_some() {
        for event in events {
            match event {
                Event::PointerButton(pe)
                    if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed =>
                {
                    let index = app
                        .menu
                        .as_ref()
                        .and_then(|state| state.menu.index_at(state.panel, pe.position, theme));
                    match index {
                        Some(index) => app.run_menu(index),
                        None => {
                            let inside = app
                                .menu
                                .as_ref()
                                .is_some_and(|state| state.panel.contains(pe.position));
                            if !inside {
                                app.menu = None;
                            }
                        }
                    }
                }
                Event::PointerMotion { position, .. } => {
                    if let Some(state) = &mut app.menu {
                        state.menu.hover_at(state.panel, Some(*position), theme);
                    }
                }
                Event::Key(ke) if ke.state == ButtonState::Pressed && ke.keysym == KEY_ESCAPE => {
                    app.menu = None;
                }
                _ => {}
            }
        }
        return;
    }

    for event in events {
        match event {
            Event::PointerButton(pe) => match (pe.button, pe.state) {
                (PointerButton::Left, ButtonState::Pressed) => {
                    on_left_press(
                        app,
                        pe.position,
                        pe.modifiers.shift,
                        &lay,
                        &hdr,
                        theme,
                        text,
                    );
                }
                (PointerButton::Left, ButtonState::Released) => {
                    app.viewer.drag = None;
                    app.pdf_end_selection();
                }
                (PointerButton::Back, ButtonState::Pressed) => {
                    if app.mode == Mode::Viewer {
                        app.back_to_gallery();
                    } else {
                        app.up();
                    }
                }
                _ => {}
            },
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                if lay.header.contains(*position) {
                    app.controls_hover = HeaderBar::new()
                        .controls(ControlsSide::Right)
                        .control_at(lay.header, theme, *position);
                } else {
                    app.controls_hover = None;
                }
                if app.viewer.text_selecting {
                    if let Some(word) = pdf_nearest_word(app, lay.content, *position) {
                        app.pdf_update_selection(word);
                    }
                }
                if let Some((start, origin)) = app.viewer.drag {
                    app.viewer.pan = Point::new(
                        origin.x + position.x - start.x,
                        origin.y + position.y - start.y,
                    );
                }
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.controls_hover = None;
            }
            Event::Scroll(se) => {
                if app.mode == Mode::Gallery {
                    let g = grid_metrics(app.items.len(), lay.grid);
                    if g.area.contains(se.position) {
                        app.scroll.on_scroll(se.notches_y() * 60.0, g.area);
                    }
                } else if app.viewer.image.is_some() && lay.content.contains(se.position) {
                    let factor = (-se.notches_y() * 0.15).exp();
                    app.viewer.zoom = (app.viewer.zoom * factor).clamp(0.1, 32.0);
                    if app.viewer.zoom <= 1.0 {
                        app.viewer.zoom = 1.0;
                        app.viewer.pan = Point::ZERO;
                    }
                }
            }
            Event::Key(ke) if ke.state == ButtonState::Pressed => {
                on_key(app, ke, &lay, size, theme, text)
            }
            _ => {}
        }
    }
}

fn sync_metrics(app: &mut App, lay: &Layout) {
    if app.mode == Mode::Viewer {
        if let Some(item) = app.items.get(app.viewer.index) {
            let (path, kind) = (item.path.clone(), item.kind);
            app.thumbs.ensure(&path, kind);
        }
        return;
    }
    let g = grid_metrics(app.items.len(), lay.grid);
    app.scroll.set_content_height(g.content_h);
    app.scroll.set_offset(app.scroll.offset(), g.area);
    let (start, end) = g.visible_range(app.scroll.offset());
    let end = end.min(app.items.len());
    let items = &app.items;
    let thumbs = &mut app.thumbs;
    for index in start..end {
        if let Some(item) = items.get(index) {
            thumbs.ensure(&item.path, item.kind);
        }
    }
    if let Some(item) = app.items.get(app.selected) {
        app.thumbs.ensure(&item.path, item.kind);
    }
}

fn on_left_press(
    app: &mut App,
    p: Point,
    shift: bool,
    lay: &Layout,
    hdr: &HeaderWidgets,
    theme: &Theme,
    text: &TextEngine,
) {
    // Pasek tytułu: kontrolki okna, widgety w `content`, na końcu przeciąganie.
    if lay.header.contains(p) {
        if let Some(control) = HeaderBar::new()
            .controls(ControlsSide::Right)
            .control_at(lay.header, theme, p)
        {
            match control {
                WindowControl::Close => app.pending_close = true,
                WindowControl::Minimize => app.pending_minimize = true,
                WindowControl::Maximize => app.pending_maximize = true,
            }
            return;
        }
        if hdr.back.contains(p) {
            if app.mode == Mode::Viewer {
                app.back_to_gallery();
            } else {
                app.up();
            }
            return;
        }
        if hdr.menu.contains(p) {
            app.open_menu(hdr.menu, lay.window, theme, text);
            return;
        }
        if hdr.details.contains(p) {
            app.toggle_details();
            return;
        }
        if hdr.path.contains(p) {
            let components = path_components(&app.dir);
            let labels: Vec<String> = components.iter().map(|(label, _)| label.clone()).collect();
            let bar = PathBar::new(labels);
            if let Some(index) = bar.index_at(hdr.path, text, theme, p) {
                if let Some((_, path)) = components.get(index) {
                    if *path == app.dir {
                        // Klik w bieżący katalog to dobry uchwyt do przesuwania okna.
                        app.pending_move = true;
                    } else {
                        let path = path.clone();
                        app.navigate_to(path);
                    }
                }
                return;
            }
            // Puste miejsce między okruszkami — przeciąganie okna.
            app.pending_move = true;
            return;
        }
        // Pusty fragment paska tytułu przeciąga okno (CSD).
        app.pending_move = true;
        return;
    }
    // Panel miejsc.
    if lay.sidebar.width() > 0.0 && lay.sidebar.contains(p) {
        if let Some(index) = sidebar_row_at(app, lay.sidebar, p) {
            if let Some(place) = app.places.get(index).cloned() {
                app.navigate_to(place.path);
            }
        }
        return;
    }
    // Pasek podkatalogów.
    if lay.folders.height() > 0.0 && lay.folders.contains(p) {
        if let Some((index, _)) = folder_chips(app, lay.folders, theme, text)
            .into_iter()
            .find(|(_, rect)| rect.contains(p))
        {
            if let Some((_, path)) = app.folders.get(index).cloned() {
                app.navigate_to(path);
            }
        }
        return;
    }

    if app.mode == Mode::Viewer {
        let canvas = lay.content;
        let ui = viewer_ui(app, canvas, theme, text);
        if let Some(mute) = ui.mute {
            if mute.contains(p) {
                app.toggle_mute();
                return;
            }
        }
        if let Some(external) = ui.external {
            if external.contains(p) {
                app.open_external();
                return;
            }
        }
        for (action, rect) in &ui.buttons {
            if rect.contains(p) {
                run_viewer_action(app, *action, canvas);
                return;
            }
        }
        if let Some(controls) = ui.controls {
            let play = app.viewer.controls.play_at(controls, theme, text, p);
            if play {
                if let Some(video) = &mut app.viewer.video {
                    video.toggle();
                }
                return;
            }
            if let Some(fraction) = app.viewer.controls.seek_at(controls, theme, text, p) {
                if let Some(video) = &mut app.viewer.video {
                    let duration = video.duration();
                    video.seek(fraction * duration);
                }
                return;
            }
        }
        if let Some(prev) = ui.prev {
            if prev.contains(p) {
                app.viewer_step(-1);
                return;
            }
        }
        if let Some(next) = ui.next {
            if next.contains(p) {
                app.viewer_step(1);
                return;
            }
        }
        if canvas.contains(p) {
            if app.viewer.video.is_some() {
                if let Some(video) = &mut app.viewer.video {
                    video.toggle();
                }
            } else if app.viewer.image.is_some() {
                if shift && app.viewer.pdf_text.is_some() {
                    app.viewer.drag = None;
                    app.last_canvas_click = None;
                    match pdf_word_hit(app, canvas, p).or_else(|| pdf_nearest_word(app, canvas, p))
                    {
                        Some(word) => app.pdf_begin_selection(word),
                        None => {
                            app.viewer.text_sel = None;
                            app.viewer.text_selecting = false;
                        }
                    }
                    return;
                }
                let now = Instant::now();
                let double = app
                    .last_canvas_click
                    .is_some_and(|last| now.duration_since(last) < Duration::from_millis(400));
                if double {
                    app.last_canvas_click = None;
                    if app.viewer.zoom <= 1.001 {
                        app.zoom_actual(canvas);
                    } else {
                        app.zoom_fit();
                    }
                } else {
                    app.last_canvas_click = Some(now);
                    app.viewer.drag = Some((p, app.viewer.pan));
                }
            }
        }
        return;
    }

    // Galeria.
    let g = grid_metrics(app.items.len(), lay.grid);
    if g.area.contains(p) {
        if let Some(index) = tile_at(app.items.len(), &g, app.scroll.offset(), p) {
            let now = Instant::now();
            let double = app.last_click.is_some_and(|(last_index, at)| {
                last_index == index && now.duration_since(at) < Duration::from_millis(400)
            });
            app.selected = index;
            if double {
                app.last_click = None;
                app.open_viewer(index);
            } else {
                app.last_click = Some((index, now));
            }
        }
    }
}

fn run_viewer_action(app: &mut App, action: ViewerAction, canvas: Rect) {
    match action {
        ViewerAction::PagePrev => app.pdf_step(-1),
        ViewerAction::PageNext => app.pdf_step(1),
        ViewerAction::ZoomOut => app.zoom_out(),
        ViewerAction::Fit => app.zoom_fit(),
        ViewerAction::Actual => app.zoom_actual(canvas),
        ViewerAction::ZoomIn => app.zoom_in(),
        ViewerAction::Slideshow => app.toggle_slideshow(),
        ViewerAction::External => app.open_external(),
    }
}

fn tile_at(len: usize, g: &GridMetrics, offset: f64, p: Point) -> Option<usize> {
    if !g.area.contains(p) {
        return None;
    }
    let rel_x = p.x - g.area.x();
    let rel_y = p.y - g.area.y() + offset;
    let col = (rel_x / (g.tile_w + g.gap)).floor() as usize;
    let row = (rel_y / (g.tile_h + g.gap)).floor() as usize;
    if col >= g.cols {
        return None;
    }
    // Przerwa między kafelkami nie jest trafieniem.
    if rel_x - col as f64 * (g.tile_w + g.gap) > g.tile_w
        || rel_y - row as f64 * (g.tile_h + g.gap) > g.tile_h
    {
        return None;
    }
    let index = row * g.cols + col;
    (index < len).then_some(index)
}

fn sidebar_row_at(app: &App, rect: Rect, p: Point) -> Option<usize> {
    let inner = rect.deflate(Insets::new(PAD, 12.0, 0.0, 12.0));
    let row_h = 34.0;
    let start_y = inner.y() + 30.0;
    if p.x < inner.x() || p.x > inner.right() || p.y < start_y {
        return None;
    }
    let index = ((p.y - start_y) / (row_h + 2.0)).floor() as usize;
    let within = (p.y - start_y) - index as f64 * (row_h + 2.0);
    (within <= row_h && index < app.places.len()).then_some(index)
}

fn reveal_selected(app: &mut App, lay: &Layout) {
    let g = grid_metrics(app.items.len(), lay.grid);
    if g.cols == 0 || app.items.is_empty() {
        return;
    }
    let offset = app.scroll.offset();
    let tile = g.tile_rect(app.selected.min(app.items.len() - 1), offset);
    if tile.y() < g.area.y() {
        app.scroll
            .set_offset(offset - (g.area.y() - tile.y()), g.area);
    } else if tile.bottom() > g.area.bottom() {
        app.scroll
            .set_offset(offset + (tile.bottom() - g.area.bottom()), g.area);
    }
    let _ = lay;
}

fn on_key(
    app: &mut App,
    ke: &KeyEvent,
    lay: &Layout,
    _size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let _ = theme;
    let _ = text;
    let key = ke.keysym;
    if app.mode == Mode::Viewer && ke.modifiers.ctrl {
        if matches!(key, 0x63 | 0x43) && app.viewer.text_sel.is_some() {
            app.copy_pdf_text(true);
            return;
        }
        if matches!(key, 0x61 | 0x41) {
            if let Some(words) = app.viewer.pdf_text.as_ref().map(|text| text.words.len()) {
                if words > 0 {
                    app.viewer.text_sel = Some((0, words - 1));
                    return;
                }
            }
        }
    }
    if key == KEY_ESCAPE {
        if app.mode == Mode::Viewer {
            app.back_to_gallery();
        } else {
            app.pending_close = true;
        }
        return;
    }
    if key == KEY_F11 || key == 0x66 {
        // F11 / f
        app.pending_maximize = true;
        return;
    }
    if key == KEY_F5 {
        app.reload();
        return;
    }
    if key == KEY_F9 {
        app.sidebar = !app.sidebar;
        return;
    }
    if key == 0x69 {
        // i
        app.toggle_details();
        return;
    }
    if key == 0x6d {
        // m
        if app.mode == Mode::Viewer {
            app.toggle_mute();
        }
        return;
    }
    if key == 0x6f {
        // o
        app.open_external();
        return;
    }

    match app.mode {
        Mode::Gallery => {
            if ke.modifiers.ctrl && key == KEY_DOWN {
                if let Some((_, path)) = app.folders.first().cloned() {
                    app.navigate_to(path);
                }
                return;
            }
            if ke.modifiers.ctrl && key == KEY_UP {
                app.up();
                return;
            }
            let g = grid_metrics(app.items.len(), lay.grid);
            let cols = g.cols;
            match key {
                KEY_LEFT => app.move_selection(-1, 0, cols),
                KEY_RIGHT => app.move_selection(1, 0, cols),
                KEY_UP => app.move_selection(0, -1, cols),
                KEY_DOWN => app.move_selection(0, 1, cols),
                KEY_HOME => app.select(0),
                KEY_END => app.select(app.items.len().saturating_sub(1)),
                KEY_PAGE_UP => app.move_selection(0, -3, cols),
                KEY_PAGE_DOWN => app.move_selection(0, 3, cols),
                KEY_ENTER | KEY_KP_ENTER | 0x20 => app.open_viewer(app.selected),
                KEY_BACKSPACE => app.up(),
                0x72 => app.reload(), // r
                _ => return,
            }
            reveal_selected(app, lay);
        }
        Mode::Viewer => {
            let canvas = lay.content;
            match key {
                KEY_LEFT => app.viewer_step(-1),
                KEY_RIGHT => app.viewer_step(1),
                KEY_UP | KEY_DOWN => {
                    if app.viewer.pdf.is_some() {
                        app.pdf_step(if key == KEY_UP { -1 } else { 1 });
                    } else {
                        app.viewer_step(if key == KEY_UP { -1 } else { 1 });
                    }
                }
                KEY_PAGE_UP | KEY_PAGE_DOWN => {
                    let delta = if key == KEY_PAGE_UP { -1 } else { 1 };
                    if app.viewer.pdf.is_some() {
                        app.pdf_step(delta);
                    } else if let Some(video) = &mut app.viewer.video {
                        video.seek_by(delta as f64 * 10.0);
                    } else {
                        app.viewer_step(delta);
                    }
                }
                KEY_HOME | KEY_END => {
                    if app.viewer.pdf.is_some() {
                        app.pdf_edge(key == KEY_END);
                    } else if let Some(video) = &mut app.viewer.video {
                        let position = if key == KEY_HOME {
                            0.0
                        } else {
                            video.duration()
                        };
                        video.seek(position);
                    } else if app.items.is_empty() {
                    } else {
                        let index = if key == KEY_HOME {
                            0
                        } else {
                            app.items.len() - 1
                        };
                        app.open_viewer(index);
                    }
                }
                0x20 => {
                    if app.viewer.pdf.is_some() {
                        app.pdf_step(1);
                    } else if let Some(video) = &mut app.viewer.video {
                        video.toggle();
                    } else {
                        app.toggle_slideshow();
                    }
                }
                0x2b | 0x3d | KEY_KP_ADD => app.zoom_in(), // + / =
                0x2d | KEY_KP_SUBTRACT => app.zoom_out(),  // -
                0x30 => app.zoom_fit(),                    // 0
                0x31 => app.zoom_actual(canvas),           // 1
                0x73 => app.toggle_slideshow(),            // s
                KEY_BACKSPACE => app.back_to_gallery(),
                0x72 => app.reload(), // r
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode};
    use crate::prefs::Prefs;
    use libluna_render::ImageData;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luna-media-ui-{tag}-{}-{}",
            std::process::id(),
            crate::model::now_secs()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn header_widgets_fit_between_edges_and_window_controls() {
        let dir = temp_dir("header");
        let app = App::new(dir.clone(), None, Prefs::default());
        let theme = Theme::dark();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme);
        assert_eq!(lay.header.height(), theme.metrics.headerbar.height);
        let hdr = header_widgets(&lay, &theme);
        let controls = HeaderBar::new()
            .controls(ControlsSide::Right)
            .controls_layout(lay.header, &theme);
        assert!(hdr.back.x() >= controls.content.x());
        assert!(hdr.back.right() <= hdr.path.x());
        assert!(hdr.path.right() <= hdr.details.x());
        assert!(hdr.details.right() <= hdr.menu.x());
        assert!(hdr.menu.right() <= controls.content.right() + 0.5);
        assert!(controls.content.right() <= controls.minimize.unwrap().x());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_header_area_starts_window_drag() {
        let dir = temp_dir("drag");
        let mut app = App::new(dir.clone(), None, Prefs::default());
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme);
        let hdr = header_widgets(&lay, &theme);
        let p = Point::new(hdr.path.right() - 2.0, hdr.path.center().y);
        on_left_press(&mut app, p, false, &lay, &hdr, &theme, &text);
        assert!(
            app.pending_move,
            "puste miejsce paska ścieżki musi przeciągać okno"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pdf_toolbar_offers_page_navigation_only_for_pdfs() {
        let dir = temp_dir("pdf-toolbar");
        std::fs::write(dir.join("a.pdf"), b"x").unwrap();
        let mut app = App::new(dir.clone(), None, Prefs::default());
        app.mode = Mode::Viewer;
        app.viewer.index = 0;
        app.viewer.image = Some(Arc::new(ImageData::new(100, 100, vec![255; 100 * 100 * 4])));
        app.viewer.pdf = Some(crate::app::PdfState { page: 1, pages: 3 });
        let theme = Theme::dark();
        let text = TextEngine::none();
        let canvas = Rect::from_xywh(0.0, 40.0, 800.0, 600.0);
        let ui = viewer_ui(&app, canvas, &theme, &text);
        let actions: Vec<ViewerAction> = ui.buttons.iter().map(|(a, _)| *a).collect();
        assert_eq!(actions.first(), Some(&ViewerAction::PagePrev));
        assert_eq!(actions.get(1), Some(&ViewerAction::PageNext));
        assert!(!actions.contains(&ViewerAction::Slideshow));
        assert!(viewer_action_enabled(&app, ViewerAction::PagePrev));
        assert!(viewer_action_enabled(&app, ViewerAction::PageNext));

        app.viewer.pdf = Some(crate::app::PdfState { page: 0, pages: 3 });
        assert!(!viewer_action_enabled(&app, ViewerAction::PagePrev));
        app.viewer.pdf = Some(crate::app::PdfState { page: 2, pages: 3 });
        assert!(!viewer_action_enabled(&app, ViewerAction::PageNext));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn maps_pdf_words_to_screen_and_hits_them() {
        let dir = temp_dir("pdf-text");
        std::fs::write(dir.join("a.pdf"), b"x").unwrap();
        let mut app = App::new(dir.clone(), None, Prefs::default());
        app.mode = Mode::Viewer;
        app.viewer.index = 0;
        app.viewer.image = Some(Arc::new(ImageData::new(100, 100, vec![255; 100 * 100 * 4])));
        app.viewer.pdf_text = Some(Arc::new(crate::decode::PdfText {
            width: 100.0,
            height: 100.0,
            words: vec![
                crate::decode::PdfWord {
                    text: "Ala".into(),
                    x: 10.0,
                    y: 20.0,
                    width: 20.0,
                    height: 10.0,
                },
                crate::decode::PdfWord {
                    text: "ma".into(),
                    x: 40.0,
                    y: 20.0,
                    width: 15.0,
                    height: 10.0,
                },
            ],
        }));
        let canvas = Rect::from_xywh(0.0, 0.0, 100.0, 100.0);
        assert_eq!(pdf_word_hit(&app, canvas, Point::new(15.0, 25.0)), Some(0));
        assert_eq!(pdf_word_hit(&app, canvas, Point::new(45.0, 25.0)), Some(1));
        assert_eq!(pdf_word_hit(&app, canvas, Point::new(90.0, 90.0)), None);
        assert_eq!(
            pdf_nearest_word(&app, canvas, Point::new(90.0, 90.0)),
            Some(1)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn paints_gallery_and_viewer_without_a_window() {
        let dir = temp_dir("paint");
        std::fs::write(dir.join("a.png"), b"x").unwrap();
        std::fs::write(dir.join("b.mp4"), b"x").unwrap();
        let mut app = App::new(dir.clone(), None, Prefs::default());
        let theme = Theme::dark();
        let text = TextEngine::none();
        let size = Size::new(1120.0, 720.0);

        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(scene.len() > 10, "galeria powinna coś narysować");

        app.mode = Mode::Viewer;
        app.viewer.index = 0;
        app.viewer.image = Some(Arc::new(ImageData::new(320, 200, vec![255; 320 * 200 * 4])));
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        assert!(scene.len() > 10, "podgląd powinien coś narysować");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tile_hit_ignores_gaps_and_out_of_range() {
        let g = grid_metrics(6, Rect::from_xywh(0.0, 0.0, 600.0, 400.0));
        let first = g.tile_rect(0, 0.0);
        assert_eq!(tile_at(6, &g, 0.0, first.center()), Some(0));
        // Punkt w przerwie między kafelkami.
        let gap = Point::new(first.right() + 4.0, first.center().y);
        assert_eq!(tile_at(6, &g, 0.0, gap), None);
        // Poza obszarem siatki.
        assert_eq!(tile_at(6, &g, 0.0, Point::new(5.0, 5.0)), None);
        // Poza liczbą elementów.
        let g = grid_metrics(1, Rect::from_xywh(0.0, 0.0, 600.0, 400.0));
        let second = g.tile_rect(1, 0.0);
        assert_eq!(tile_at(1, &g, 0.0, second.center()), None);
    }

    #[test]
    fn breadcrumbs_start_at_home() {
        let path = places::home().join("Obrazy").join("2026");
        let components = path_components(&path);
        assert_eq!(components[0].0, libluna_i18n::tr("Home"));
        assert_eq!(components[0].1, places::home());
        assert_eq!(components.last().unwrap().1, path);
    }
}
