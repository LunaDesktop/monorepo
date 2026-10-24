//! Warstwa UI: układ, rysowanie i obsługa wejścia.
//!
//! Wszystkie napisy idą przez `libluna_i18n::tr` (klucz = angielski tekst).

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, Insets, KeyEvent, Modifiers, Point, PointerButton, Rect, Size, Theme,
};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    icon, AlertDialog, ControlsSide, Entry as TextEntry, HeaderBar, IconName, Menu, MenuEntry,
    PaintCtx, PathBar, Tab, TabBar, TextInput, ViewMode, WindowControl,
};

use crate::app::{App, ContextMenu, EditState, MenuCmd, MenuTarget, Pane, SideAction, SPLIT_GAP};
use crate::list;
use crate::model::{format_mtime, human_size, Kind, SortKey};
use crate::ops::Job;
use crate::places;
use crate::trash;

const SIDEBAR_W: f64 = 216.0;
const TABS_H: f64 = 34.0;
const NAV_H: f64 = 40.0;
const STATUS_H: f64 = 28.0;
const PREVIEW_W: f64 = 300.0;
const PAD: f64 = 8.0;
const BTN: f64 = 28.0;
const PANE_PATH_H: f64 = 26.0;

struct Layout {
    header: Rect,
    tabs: Rect,
    nav: Rect,
    sidebar: Rect,
    status: Rect,
    panes: [Rect; 2],
}

struct Chrome {
    back: Rect,
    forward: Rect,
    up: Rect,
    search: Rect,
    view: Rect,
    menu: Rect,
    plus: Rect,
    path: Rect,
}

#[derive(Clone, Copy)]
struct PaneRects {
    path: Option<Rect>,
    items: Rect,
    preview: Option<Rect>,
}

/// Obszar interaktywnych wierszy (w trybie listy bez paska nagłówków).
fn file_rows(pane: &Pane, items: Rect) -> Rect {
    if pane.view.mode() == ViewMode::List {
        list::rows_rect(items)
    } else {
        items
    }
}

/// Trafienie w wiersz zależnie od trybu widoku.
fn hit_index(pane: &Pane, rows: Rect, p: Point, theme: &Theme) -> Option<usize> {
    match pane.view.mode() {
        ViewMode::List => list::index_at(rows, p, pane.view.scroll(), pane.view.len(), theme),
        ViewMode::Grid => pane.view.index_at(rows, p, theme),
    }
}

fn layout(app: &App, size: Size, theme: &Theme) -> Layout {
    let hh = theme.metrics.headerbar.height;
    let header = Rect::from_xywh(0.0, 0.0, size.width, hh);
    let tabs = Rect::from_xywh(0.0, hh, size.width, TABS_H);
    let nav = Rect::from_xywh(0.0, tabs.bottom(), size.width, NAV_H);
    let status = Rect::from_xywh(
        0.0,
        (size.height - STATUS_H).max(nav.bottom()),
        size.width,
        STATUS_H,
    );
    let content_h = (status.y() - nav.bottom()).max(0.0);
    let sidebar_w = if app.sidebar_visible && size.width > 720.0 {
        SIDEBAR_W.min(size.width * 0.35)
    } else {
        0.0
    };
    let sidebar = Rect::from_xywh(0.0, nav.bottom(), sidebar_w, content_h);
    let area = Rect::from_xywh(
        sidebar.right(),
        nav.bottom(),
        (size.width - sidebar.right()).max(1.0),
        content_h,
    );
    let mut panes = [Rect::ZERO; 2];
    if app.split {
        let w = ((area.width() - SPLIT_GAP) / 2.0).max(1.0);
        panes[0] = Rect::from_xywh(area.x(), area.y(), w, area.height());
        panes[1] = Rect::from_xywh(area.x() + w + SPLIT_GAP, area.y(), w, area.height());
    } else {
        panes[0] = area;
    }
    Layout {
        header,
        tabs,
        nav,
        sidebar,
        status,
        panes,
    }
}

fn chrome(lay: &Layout, size: Size) -> Chrome {
    let cy = lay.nav.center().y;
    let mut x = lay.nav.x() + PAD;
    let btn = |x: &mut f64| {
        let r = Rect::from_xywh(*x, cy - BTN / 2.0, BTN, BTN);
        *x += BTN + 2.0;
        r
    };
    let back = btn(&mut x);
    let forward = btn(&mut x);
    let up = btn(&mut x);

    let right = lay.nav.right() - PAD;
    let menu = Rect::from_xywh(right - BTN, cy - BTN / 2.0, BTN, BTN);
    let view = Rect::from_xywh(menu.x() - BTN - 4.0, cy - BTN / 2.0, BTN, BTN);
    let search = Rect::from_xywh(view.x() - BTN - 4.0, cy - BTN / 2.0, BTN, BTN);
    let path = Rect::from_xywh(
        up.right() + PAD,
        lay.nav.y() + 5.0,
        (search.x() - PAD - (up.right() + PAD)).max(40.0),
        NAV_H - 10.0,
    );
    let plus = Rect::from_xywh(
        (size.width - PAD - BTN).max(0.0),
        lay.tabs.center().y - BTN / 2.0,
        BTN,
        BTN,
    );
    Chrome {
        back,
        forward,
        up,
        search,
        view,
        menu,
        plus,
        path,
    }
}

fn pane_rects(app: &App, lay: &Layout, i: usize, theme: &Theme) -> PaneRects {
    let pane = &app.panes[i];
    let rect = lay.panes[i];
    let (path, body) = if app.split {
        (
            Some(Rect::from_xywh(
                rect.x(),
                rect.y(),
                rect.width(),
                PANE_PATH_H,
            )),
            Rect::from_xywh(
                rect.x(),
                rect.y() + PANE_PATH_H,
                rect.width(),
                (rect.height() - PANE_PATH_H).max(0.0),
            ),
        )
    } else {
        (None, rect)
    };
    let preview = if pane.preview && body.width() > 560.0 {
        Some(Rect::from_xywh(
            body.right() - PREVIEW_W,
            body.y(),
            PREVIEW_W,
            body.height(),
        ))
    } else {
        None
    };
    let items = match preview {
        Some(p) => Rect::from_xywh(
            body.x() + 6.0,
            body.y() + 4.0,
            (p.x() - body.x() - 12.0).max(1.0),
            (body.height() - 8.0).max(1.0),
        ),
        None => Rect::from_xywh(
            body.x() + 6.0,
            body.y() + 4.0,
            (body.width() - 12.0).max(1.0),
            (body.height() - 8.0).max(1.0),
        ),
    };
    let _ = theme;
    PaneRects {
        path,
        items,
        preview,
    }
}

// ---------------------------------------------------------------------------
// Rysowanie
// ---------------------------------------------------------------------------

pub fn paint(app: &App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(app, size, theme);
    let ch = chrome(&lay, size);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    paint_header(&mut ctx, app, &lay);
    paint_tabs(&mut ctx, app, &lay, &ch);
    paint_nav(&mut ctx, app, &lay, &ch);
    paint_sidebar(&mut ctx, app, &lay);
    paint_panes(&mut ctx, app, &lay);
    paint_status(&mut ctx, app, &lay);

    if let Some(context) = &app.context {
        context.menu.paint(&mut ctx, context.panel);
    }
    app.toast.paint(&mut ctx, size);
    if let Some(conflict) = &app.conflict {
        conflict.dialog.paint(&mut ctx, size);
    }
    if let Some(confirm) = &app.confirm_empty {
        confirm.paint(&mut ctx, size);
    }
}

fn paint_header(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let title = if app.pane().dir.path == Path::new("/") {
        libluna_i18n::tr("File System")
    } else {
        app.pane().dir.title()
    };
    let header = HeaderBar::new().title(title).controls(ControlsSide::Right);
    header.paint(ctx, lay.header, None);
}

fn paint_tabs(ctx: &mut PaintCtx, app: &App, lay: &Layout, ch: &Chrome) {
    let pane = app.pane();
    let tabs: Vec<Tab> = pane
        .tabs
        .iter()
        .map(|p| Tab::new(tab_title(p)).closable(pane.tabs.len() > 1))
        .collect();
    let mut bar = TabBar::with_tabs(tabs).expand(false);
    bar.set_selected(pane.tab);
    let rect = Rect::from_xywh(
        lay.tabs.x(),
        lay.tabs.y(),
        (ch.plus.x() - lay.tabs.x() - PAD).max(1.0),
        lay.tabs.height(),
    );
    bar.paint(ctx, rect);
    icon_button(ctx, ch.plus, IconName::Plus, true, false);
}

fn paint_nav(ctx: &mut PaintCtx, app: &App, lay: &Layout, ch: &Chrome) {
    let pane = app.pane();
    ctx.scene.fill_rect(lay.nav, ctx.theme.colors.headerbar_bg);

    icon_button(
        ctx,
        ch.back,
        IconName::ChevronLeft,
        !pane.back.is_empty(),
        false,
    );
    icon_button(
        ctx,
        ch.forward,
        IconName::ChevronRight,
        !pane.forward.is_empty(),
        false,
    );
    icon_up(ctx, ch.up, pane.dir.parent().is_some());

    if let Some(edit) = &pane.edit {
        edit.entry.paint(ctx, ch.path);
    } else if pane.search_active {
        pane.search.paint(ctx, ch.path);
    } else {
        let bar = PathBar::new(path_components(&pane.dir.path));
        bar.paint(ctx, ch.path);
    }

    icon_button(ctx, ch.search, IconName::Search, true, pane.search_active);
    let view_icon = match pane.view.mode() {
        ViewMode::List => IconName::Grid,
        ViewMode::Grid => IconName::List,
    };
    icon_button(ctx, ch.view, view_icon, true, false);
    icon_button(ctx, ch.menu, IconName::Menu, true, false);
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
    app.sidebar
        .paint(ctx, lay.sidebar.deflate(Insets::splat(6.0)));
}

fn paint_panes(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let count = if app.split { 2 } else { 1 };
    for i in 0..count {
        let pane = &app.panes[i];
        let rect = lay.panes[i];
        let pr = pane_rects(app, lay, i, ctx.theme);

        if app.split {
            let active = i == app.active;
            ctx.scene.stroke_rounded_rect(
                rect,
                6.0,
                if active { 1.5 } else { 1.0 },
                if active {
                    ctx.theme.colors.accent
                } else {
                    ctx.theme.colors.border
                },
            );
            if let Some(path) = pr.path {
                ctx.scene.fill_rect(path, ctx.theme.colors.headerbar_bg);
                let title = path_text(&pane.dir.path);
                ctx.label_left(
                    path.x() + 10.0,
                    path.center().y,
                    &title,
                    ctx.theme.typography.caption.size,
                    weight::MEDIUM,
                    ctx.theme.colors.dim_text,
                );
            }
        }

        // Pasek przewijania i zawartość (lista ma własne kolumny).
        let rows = file_rows(pane, pr.items);
        let mut sv = pane.scroll.clone();
        sv.set_content_height(pane.view.content_height(rows, ctx.theme));
        sv.set_offset(pane.view.scroll(), rows);
        sv.paint_scrollbar(ctx, rows);

        if pane.view.mode() == ViewMode::List {
            list::paint(
                ctx,
                pr.items,
                &pane.dir,
                pane.view.selected_indices(),
                pane.view.hovered(),
                pane.view.scroll(),
                ctx.theme,
            );
        } else {
            pane.view.paint(ctx, rows);
        }

        if pane.view.is_empty() {
            let msg = if !pane.dir.filter().is_empty() {
                libluna_i18n::tr("No matching files")
            } else {
                libluna_i18n::tr("Folder is empty")
            };
            ctx.label_center(
                rows.center().x,
                rows.center().y,
                &msg,
                ctx.theme.typography.body.size,
                weight::NORMAL,
                ctx.theme.colors.dim_text,
            );
        }

        if let Some(prect) = pr.preview {
            paint_preview(ctx, pane, prect);
        }
    }
}

fn paint_preview(ctx: &mut PaintCtx, pane: &Pane, rect: Rect) {
    ctx.scene
        .fill_rounded_rect(rect, 8.0, ctx.theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, 8.0, 1.0, ctx.theme.colors.border);
    let inner = rect.deflate(Insets::splat(10.0));
    let Some(entry) = pane.primary() else {
        ctx.label_center(
            inner.center().x,
            inner.center().y,
            &libluna_i18n::tr("No preview"),
            ctx.theme.typography.body.size,
            weight::NORMAL,
            ctx.theme.colors.dim_text,
        );
        return;
    };
    let body = ctx.theme.typography.body.size;
    let caption = ctx.theme.typography.caption.size;
    ctx.label_left(
        inner.x(),
        inner.y() + 8.0,
        &entry.name,
        body,
        weight::BOLD,
        ctx.theme.colors.text,
    );
    let below = Rect::from_xywh(
        inner.x(),
        inner.y() + 22.0,
        inner.width(),
        (inner.height() - 22.0).max(1.0),
    );
    match pane.preview_cache.as_ref().map(|(_, p)| p) {
        Some(crate::app::Preview::Text(text)) => {
            ctx.scene.push_clip(below);
            let line_h = body * 1.25;
            let max_lines = (below.height() / line_h).floor().max(1.0) as usize;
            for (i, line) in text.lines().take(max_lines).enumerate() {
                ctx.label_left(
                    below.x(),
                    below.y() + i as f64 * line_h + line_h * 0.75,
                    line,
                    body,
                    weight::NORMAL,
                    ctx.theme.colors.text,
                );
            }
            ctx.scene.pop_clip();
        }
        Some(crate::app::Preview::Image(img)) => {
            let scale = (below.width() / img.width as f64)
                .min(below.height() / img.height as f64)
                .max(0.0);
            let w = img.width as f64 * scale;
            let h = img.height as f64 * scale;
            let target = Rect::from_xywh(
                below.center().x - w / 2.0,
                below.center().y - h / 2.0,
                w.max(1.0),
                h.max(1.0),
            );
            ctx.scene.image(target, std::sync::Arc::clone(img));
        }
        _ => {
            let mut y = below.y() + 8.0;
            for (label, value) in [
                (libluna_i18n::tr("Type"), entry.kind.label()),
                (libluna_i18n::tr("Size"), human_size(entry.size)),
                (libluna_i18n::tr("Modified"), format_mtime(entry.modified)),
                (libluna_i18n::tr("Path"), entry.path.display().to_string()),
            ] {
                ctx.label_left(
                    below.x(),
                    y,
                    &format!("{label}: {value}"),
                    caption,
                    weight::NORMAL,
                    ctx.theme.colors.dim_text,
                );
                y += caption * 1.8;
            }
        }
    }
}

fn paint_status(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let colors = ctx.theme.colors;
    ctx.scene.fill_rect(lay.status, colors.headerbar_bg);
    ctx.label_left(
        lay.status.x() + PAD,
        lay.status.center().y,
        &app.status,
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    if let Some((done, total)) = app.op_progress {
        let ratio = if total > 0 {
            (done as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let bar = Rect::from_xywh(
            lay.status.x(),
            lay.status.y() - 3.0,
            lay.status.width() * ratio,
            3.0,
        );
        ctx.scene.fill_rect(bar, colors.accent);
        let speed = if app.op_speed > 1.0 {
            format!("{}/s", human_size(app.op_speed as u64))
        } else {
            String::new()
        };
        let eta = if app.op_speed > 1.0 && total > done {
            let secs = ((total - done) as f64 / app.op_speed).round() as u64;
            format!("{:02}:{:02}", secs / 60, secs % 60)
        } else {
            String::new()
        };
        let mut label = format!(
            "{} {}%",
            app.op_label.clone().unwrap_or_default(),
            (ratio * 100.0).round() as i64
        );
        if !speed.is_empty() {
            label.push_str(" · ");
            label.push_str(&speed);
        }
        if !eta.is_empty() {
            label.push_str(" · ");
            label.push_str(&libluna_i18n::tr_args("{time} left", &[("time", &eta)]));
        }
        if !app.op_current.is_empty() {
            label.push_str(" — ");
            label.push_str(&app.op_current);
        }
        let cancel = op_cancel_rect(lay);
        ctx.label_right(
            cancel.x() - PAD,
            lay.status.center().y,
            &label,
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.accent,
        );
        icon::paint_icon(
            ctx,
            IconName::Close,
            cancel.deflate(Insets::splat(4.0)),
            colors.text,
        );
    }
}

fn icon_button(ctx: &mut PaintCtx, rect: Rect, name: IconName, enabled: bool, active: bool) {
    if active || enabled {
        let bg = if active {
            ctx.theme.colors.shade
        } else {
            Color::TRANSPARENT
        };
        if active {
            ctx.scene.fill_rounded_rect(rect, 6.0, bg);
        }
    }
    let color = if enabled {
        ctx.theme.colors.text
    } else {
        ctx.theme.colors.dim_text.scale_alpha(0.5)
    };
    icon::paint_icon(ctx, name, rect.deflate(Insets::splat(6.0)), color);
}

fn icon_up(ctx: &mut PaintCtx, rect: Rect, enabled: bool) {
    let color = if enabled {
        ctx.theme.colors.text
    } else {
        ctx.theme.colors.dim_text.scale_alpha(0.5)
    };
    icon::chevron_up(ctx, color, rect.center(), 5.0, 1.6);
}

/// Przycisk anulowania operacji w belce statusu.
fn op_cancel_rect(lay: &Layout) -> Rect {
    Rect::from_xywh(
        lay.status.right() - PAD - 18.0,
        lay.status.center().y - 9.0,
        18.0,
        18.0,
    )
}

// ---------------------------------------------------------------------------
// Ścieżki i zakładki
// ---------------------------------------------------------------------------

fn tab_title(path: &Path) -> String {
    if path == Path::new("/") {
        "/".to_owned()
    } else if path == places::home() {
        "~".to_owned()
    } else {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    }
}

fn path_text(path: &Path) -> String {
    if path == places::home() {
        return "~".to_owned();
    }
    if let Ok(rest) = path.strip_prefix(places::home()) {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

fn path_components(path: &Path) -> Vec<String> {
    let home = places::home();
    if path == home {
        return vec!["~".to_owned()];
    }
    if let Ok(rest) = path.strip_prefix(&home) {
        let mut out = vec!["~".to_owned()];
        for c in rest.components() {
            if let Component::Normal(s) = c {
                out.push(s.to_string_lossy().into_owned());
            }
        }
        return out;
    }
    let mut out = vec!["/".to_owned()];
    for c in path.components() {
        if let Component::Normal(s) = c {
            out.push(s.to_string_lossy().into_owned());
        }
    }
    if out.len() == 1 {
        return vec!["/".to_owned()];
    }
    out
}

fn segment_path(path: &Path, index: usize) -> PathBuf {
    let comps = path_components(path);
    if index == 0 {
        return if comps[0] == "~" {
            places::home()
        } else {
            PathBuf::from("/")
        };
    }
    let base = if comps[0] == "~" {
        places::home()
    } else {
        PathBuf::from("/")
    };
    let mut out = base;
    for c in comps.iter().skip(1).take(index) {
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Zdarzenia
// ---------------------------------------------------------------------------

pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme);
    let ch = chrome(&lay, size);

    // Modal konfliktu blokuje resztę interfejsu.
    if app.conflict.is_some() {
        for event in events {
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    let index = app
                        .conflict
                        .as_ref()
                        .and_then(|c| c.dialog.action_at(size, theme, text, pe.position));
                    if let Some(index) = index {
                        app.answer_conflict(index);
                    }
                }
            }
        }
        return;
    }

    // Modal potwierdzenia (opróżnienie kosza).
    if app.confirm_empty.is_some() {
        for event in events {
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    let index = app
                        .confirm_empty
                        .as_ref()
                        .and_then(|d| d.action_at(size, theme, text, pe.position));
                    match index {
                        Some(0) => {
                            if crate::app::debug() {
                                eprintln!("luna-files: confirm empty trash");
                            }
                            let _ = trash::empty_trash();
                            app.confirm_empty = None;
                            app.reload_all();
                            app.refresh_devices();
                            app.show_toast(libluna_i18n::tr("Trash emptied"));
                        }
                        Some(_) => {
                            if crate::app::debug() {
                                eprintln!("luna-files: confirm cancelled");
                            }
                            app.confirm_empty = None;
                        }
                        None => {}
                    }
                }
            }
        }
        return;
    }

    for event in events {
        match event {
            Event::PointerButton(pe) => match (pe.button, pe.state) {
                (PointerButton::Left, ButtonState::Pressed) => {
                    on_left_press(app, pe.position, pe.modifiers, &lay, &ch, size, theme, text)
                }
                (PointerButton::Right, ButtonState::Pressed) => {
                    on_right_press(app, pe.position, &lay, size, theme, text)
                }
                (PointerButton::Back, ButtonState::Pressed) => app.back(),
                (PointerButton::Forward, ButtonState::Pressed) => app.forward(),
                (_, ButtonState::Released) => {}
                _ => {}
            },
            Event::PointerMotion { position, .. } => {
                let p = *position;
                if let Some(context) = app.context.as_mut() {
                    context.menu.hover_at(context.panel, Some(p), theme);
                    continue;
                }
                let _hovered =
                    app.sidebar
                        .index_at(lay.sidebar.deflate(Insets::splat(6.0)), p, theme);
                app.sidebar.set_hovered(_hovered);
                for i in 0..if app.split { 2 } else { 1 } {
                    let pr = pane_rects(app, &lay, i, theme);
                    let rows = file_rows(&app.panes[i], pr.items);
                    let hovered = hit_index(&app.panes[i], rows, p, theme);
                    app.panes[i].view.set_hovered(hovered);
                    app.panes[i]
                        .search
                        .set_hover_clear(app.panes[i].search.clear_at(ch.path, theme, p));
                }
            }
            Event::PointerLeave => {
                app.sidebar.set_hovered(None);
                for pane in &mut app.panes {
                    pane.view.set_hovered(None);
                }
            }
            Event::Scroll(se) => {
                let count = if app.split { 2 } else { 1 };
                for i in 0..count {
                    let pr = pane_rects(app, &lay, i, theme);
                    let rows = file_rows(&app.panes[i], pr.items);
                    if !rows.contains(se.position) {
                        continue;
                    }
                    app.active = i;
                    let step = theme.metrics.itemview.row_h;
                    let pane = &mut app.panes[i];
                    let max = pane.view.max_scroll(rows, theme);
                    let next = (pane.view.scroll() + se.dy * step).clamp(0.0, max);
                    pane.view.set_scroll(next);
                    pane.scroll.set_offset(next, rows);
                    break;
                }
            }
            Event::Key(ke) if ke.state == ButtonState::Pressed => {
                on_key(app, ke, &lay, &ch, size, theme, text)
            }
            _ => {}
        }
    }
}

fn on_left_press(
    app: &mut App,
    p: Point,
    mods: Modifiers,
    lay: &Layout,
    ch: &Chrome,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    // Anulowanie trwającej operacji ma pierwszeństwo.
    if app.op_progress.is_some() && op_cancel_rect(lay).contains(p) {
        app.cancel_op();
        return;
    }
    // 1. Menu kontekstowe ma pierwszeństwo.
    if let Some(context) = &mut app.context {
        if let Some(index) = context.menu.click_at(context.panel, p, theme) {
            run_menu(app, index);
        } else if !context.panel.contains(p) {
            app.context = None;
        }
        return;
    }

    // 2. Kontrolki okna (nasz HeaderBar).
    let header = HeaderBar::new().controls(ControlsSide::Right);
    if lay.header.contains(p) {
        if let Some(control) = header.control_at(lay.header, theme, p) {
            match control {
                WindowControl::Close => app.pending_close = true,
                WindowControl::Minimize => app.pending_minimize = true,
                WindowControl::Maximize => app.pending_maximize = true,
            }
            return;
        }
        // Przeciąganie za wolny obszar paska.
        let content = header.controls_layout(lay.header, theme).content;
        if content.contains(p) {
            app.pending_move = true;
        }
        return;
    }

    // 3. Zakładki.
    if lay.tabs.contains(p) {
        if ch.plus.contains(p) {
            app.new_tab();
            return;
        }
        let pane = app.pane();
        let tabs: Vec<Tab> = pane
            .tabs
            .iter()
            .map(|tp| Tab::new(tab_title(tp)).closable(pane.tabs.len() > 1))
            .collect();
        let mut bar = TabBar::with_tabs(tabs).expand(false);
        bar.set_selected(pane.tab);
        let rect = Rect::from_xywh(
            lay.tabs.x(),
            lay.tabs.y(),
            (ch.plus.x() - lay.tabs.x() - PAD).max(1.0),
            lay.tabs.height(),
        );
        if let Some(i) = bar.close_at(rect, text, theme, p) {
            app.select_tab(i);
            app.close_tab();
        } else if let Some(i) = bar.index_at(rect, text, theme, p) {
            app.select_tab(i);
        }
        return;
    }

    // 4. Pasek nawigacji.
    if lay.nav.contains(p) {
        if ch.back.contains(p) {
            app.back();
            return;
        }
        if ch.forward.contains(p) {
            app.forward();
            return;
        }
        if ch.up.contains(p) {
            app.up();
            return;
        }
        if ch.search.contains(p) {
            let pane = app.pane_mut();
            pane.search_active = !pane.search_active;
            pane.search.set_focused(pane.search_active);
            if !pane.search_active {
                pane.search.clear();
                pane.dir.set_filter("");
                pane.sync_view();
            }
            return;
        }
        if ch.view.contains(p) {
            let pane = app.pane_mut();
            pane.view.toggle_mode();
            return;
        }
        if ch.menu.contains(p) {
            open_menu(app, p, MenuTarget::AppMenu, size, theme, text);
            return;
        }
        if ch.path.contains(p) {
            let pane = app.pane_mut();
            if pane.edit.is_some() {
                pane.edit.as_mut().unwrap().entry.set_focused(true);
                return;
            }
            if pane.search_active {
                pane.search.set_focused(true);
                return;
            }
            let path = pane.dir.path.clone();
            let comps = PathBar::new(path_components(&path));
            if let Some(i) = comps.index_at(ch.path, text, theme, p) {
                let target = segment_path(&path, i);
                app.navigate(app.active, target);
            }
            return;
        }
    }

    // 5. Panel boczny.
    if lay.sidebar.contains(p) {
        let entry = app
            .sidebar
            .click_at(lay.sidebar.deflate(Insets::splat(6.0)), p, theme);
        if entry {
            let idx = app.sidebar.selected();
            if let Some(action) = idx
                .and_then(|e| app.sidebar_entry_action.get(e).copied().flatten())
                .and_then(|ai| app.sidebar_actions.get(ai))
            {
                match action {
                    SideAction::Go(path) => {
                        let path = path.clone();
                        app.navigate(app.active, path);
                    }
                    SideAction::Unmount(i) => {
                        if let Some(m) = app.mounts.get(*i) {
                            places::unmount_device(&m.dev);
                            app.show_toast(libluna_i18n::tr("Unmounting…"));
                        }
                    }
                    SideAction::Mount(i) => {
                        if let Some(m) = app.mounts.get(*i) {
                            places::mount_device(&m.dev);
                            app.show_toast(libluna_i18n::tr("Mounting…"));
                        }
                    }
                }
            }
        }
        return;
    }

    // 6. Panele.
    let count = if app.split { 2 } else { 1 };
    for i in 0..count {
        if !lay.panes[i].contains(p) {
            continue;
        }
        app.active = i;
        let pr = pane_rects(app, lay, i, theme);
        // Klik w nagłówek kolumny sortuje (tak jak w Nautilusie).
        if app.panes[i].view.mode() == ViewMode::List
            && p.y < pr.items.y() + list::HEADER_H
            && pr.items.contains(p)
        {
            if let Some(key) = list::header_at(pr.items, p, theme) {
                let pane = &mut app.panes[i];
                if pane.dir.sort_key() == key {
                    let asc = pane.dir.ascending();
                    pane.dir.set_ascending(!asc);
                } else {
                    pane.dir.set_sort_key(key);
                    // Rozmiar i data naturalnie malejąco.
                    pane.dir
                        .set_ascending(!matches!(key, SortKey::Size | SortKey::Modified));
                }
                pane.sync_view();
                if crate::app::debug() {
                    eprintln!(
                        "luna-files: sort {:?} asc={}",
                        pane.dir.sort_key(),
                        pane.dir.ascending()
                    );
                }
            }
            return;
        }
        let rows = file_rows(&app.panes[i], pr.items);
        if rows.contains(p) {
            let index = hit_index(&app.panes[i], rows, p, theme);
            let Some(index) = index else {
                app.panes[i].view.clear_selection();
                app.panes[i].preview_cache = None;
                return;
            };
            let now = Instant::now();
            let double = app
                .last_click
                .map(|(last, t)| {
                    last == index && now.duration_since(t) < Duration::from_millis(400)
                })
                .unwrap_or(false);
            if double {
                app.last_click = None;
                let entry = app.panes[i].dir.entry(index).cloned();
                if let Some(entry) = entry {
                    if entry.is_dir {
                        app.navigate(i, entry.path);
                    } else {
                        let _ = std::process::Command::new("xdg-open")
                            .arg(&entry.path)
                            .spawn();
                    }
                }
            } else {
                app.last_click = Some((index, now));
                if mods.ctrl {
                    app.panes[i].view.toggle_selected(index);
                } else if mods.shift {
                    app.panes[i].view.select_range(index);
                } else {
                    app.panes[i].view.set_selected(Some(index));
                }
                app.panes[i].preview_cache = None;
            }
            return;
        }
        // Klik poza listą czyści zaznaczenie.
        app.panes[i].view.clear_selection();
        app.panes[i].preview_cache = None;
        return;
    }
}

fn on_right_press(
    app: &mut App,
    p: Point,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    if let Some(context) = &app.context {
        if context.panel.contains(p) {
            return;
        }
        app.context = None;
    }

    // Panel boczny: menu urządzenia.
    if lay.sidebar.contains(p) {
        let entry = app
            .sidebar
            .index_at(lay.sidebar.deflate(Insets::splat(6.0)), p, theme);
        if let Some(entry) = entry {
            app.sidebar.set_selected(Some(entry));
            if let Some(ai) = app.sidebar_entry_action.get(entry).copied().flatten() {
                if let Some(SideAction::Go(path)) = app.sidebar_actions.get(ai) {
                    if let Some(mi) = app.mounts.iter().position(|m| &m.mount == path) {
                        open_menu(app, p, MenuTarget::Device(mi), size, theme, text);
                        return;
                    }
                }
            }
        }
        return;
    }

    let count = if app.split { 2 } else { 1 };
    for i in 0..count {
        if !lay.panes[i].contains(p) {
            continue;
        }
        app.active = i;
        let pr = pane_rects(app, lay, i, theme);
        let rows = file_rows(&app.panes[i], pr.items);
        if let Some(index) = hit_index(&app.panes[i], rows, p, theme) {
            if !app.panes[i].view.is_selected(index) {
                app.panes[i].view.set_selected(Some(index));
            }
            if let Some(entry) = app.panes[i].dir.entry(index).cloned() {
                let in_trash = app.panes[i].dir.path == trash::trash_root().join("files");
                let target = if in_trash {
                    MenuTarget::TrashItem(entry.path)
                } else {
                    MenuTarget::Item(entry.path, entry.is_dir)
                };
                open_menu(app, p, target, size, theme, text);
            }
        } else {
            open_menu(app, p, MenuTarget::Background, size, theme, text);
        }
        return;
    }
}

fn on_key(
    app: &mut App,
    ke: &KeyEvent,
    lay: &Layout,
    ch: &Chrome,
    _size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    // Menu kontekstowe: Escape zamyka.
    if app.context.is_some() {
        if ke.keysym == 0xff1b {
            app.context = None;
        }
        return;
    }

    // Escape anuluje trwającą operację (gdy nie edytujemy tekstu).
    if ke.keysym == 0xff1b
        && app.ops.busy()
        && app.pane().edit.is_none()
        && !app.pane().search_active
        && app.conflict.is_none()
    {
        app.cancel_op();
        return;
    }

    // Pole edycji (ścieżka / nazwa).
    if app.pane().edit.is_some() {
        if ke.keysym == 0xff1b {
            app.pane_mut().edit = None;
            return;
        }
        if ke.keysym == 0xff0d {
            apply_edit(app);
            return;
        }
        let handled = {
            let pane = app.pane_mut();
            let edit = pane.edit.as_mut().unwrap();
            handle_text_key(&mut edit.entry, ke)
        };
        if handled {
            return;
        }
    }

    // Wyszukiwanie.
    if app.pane().search_active && app.pane().search.is_focused() {
        if ke.keysym == 0xff1b || ke.keysym == 0xff0d {
            let pane = app.pane_mut();
            pane.search_active = false;
            pane.search.set_focused(false);
            pane.search.clear();
            pane.dir.set_filter("");
            pane.sync_view();
            return;
        }
        let handled = {
            let pane = app.pane_mut();
            handle_text_key(&mut pane.search, ke)
        };
        if handled {
            let query = app.pane().search.text().to_owned();
            let pane = app.pane_mut();
            pane.dir.set_filter(query);
            pane.sync_view();
            return;
        }
    }

    let ctrl = ke.modifiers.ctrl;
    let shift = ke.modifiers.shift;
    let alt = ke.modifiers.alt;

    if ctrl {
        match ke.keysym {
            0x6c => {
                // Ctrl+L — edycja ścieżki
                let path = app.pane().dir.path.clone();
                app.pane_mut().edit = Some(EditState::path(&path));
            }
            0x66 => {
                // Ctrl+F — szukanie
                let pane = app.pane_mut();
                pane.search_active = true;
                pane.search.set_focused(true);
            }
            0x74 => app.new_tab(),                    // Ctrl+T
            0x77 => app.close_tab(),                  // Ctrl+W
            0x63 => copy_selection(app, false),       // Ctrl+C
            0x78 => copy_selection(app, true),        // Ctrl+X
            0x76 => paste(app),                       // Ctrl+V
            0x61 => app.pane_mut().view.select_all(), // Ctrl+A
            0x68 => toggle_hidden(app),               // Ctrl+H
            0x6e => new_folder_edit(app),             // Ctrl+N
            0x72 => app.reload_pane(app.active),      // Ctrl+R
            _ => {}
        }
        return;
    }

    match ke.keysym {
        0x72 => app.reload_pane(app.active), // r
        0xff0d => app.open_selected(),       // Enter
        0xff08 => app.up(),                  // Backspace
        0xffff => {
            // Delete — do kosza (Shift = trwale)
            if shift {
                delete_selection(app, true);
            } else {
                delete_selection(app, false);
            }
        }
        0xff1b => {
            let pane = app.pane_mut();
            pane.preview = false;
            pane.preview_cache = None;
            pane.view.clear_selection();
        }
        0x20 => {
            // Space — podgląd
            let pane = app.pane_mut();
            pane.preview = !pane.preview;
            pane.preview_cache = None;
        }
        0xffbe => {} // F1
        0xffbf => {
            // F2 — zmiana nazwy
            if let Some(entry) = app.pane().primary().cloned() {
                app.pane_mut().edit = Some(EditState::rename(&entry.path));
            }
        }
        0xffc0 => {
            // F3 — dwa panele
            app.split = !app.split;
            if !app.split {
                app.active = 0;
            }
        }
        0xffc1 => app.open_terminal(),                        // F4
        0xffc2 => app.reload_pane(app.active),                // F5
        0xffc6 => app.sidebar_visible = !app.sidebar_visible, // F9
        0xff51 => {
            if alt {
                app.back();
            } else {
                move_selection(app, -1, shift, lay, theme);
            }
        }
        0xff53 => {
            if alt {
                app.forward();
            } else {
                move_selection(app, 1, shift, lay, theme);
            }
        }
        0xff52 => move_selection(app, -1, shift, lay, theme),
        0xff54 => move_selection(app, 1, shift, lay, theme),
        0xff55 => move_selection(app, -10, shift, lay, theme),
        0xff56 => move_selection(app, 10, shift, lay, theme),
        0xff50 => select_boundary(app, false, shift, lay, theme),
        0xff57 => select_boundary(app, true, shift, lay, theme),
        0xff09 => {
            // Tab — następna zakładka (bez Ctrl)
            let next = {
                let p = app.pane();
                (p.tab + 1) % p.tabs.len().max(1)
            };
            app.select_tab(next);
        }
        _ => {
            // Pisanie bez fokusu pola = szybkie wyszukiwanie.
            if let Some(t) = &ke.text {
                if !t.is_empty() && !t.chars().all(char::is_control) {
                    let pane = app.pane_mut();
                    pane.search_active = true;
                    pane.search.set_focused(true);
                    pane.search.insert_str(t);
                    let query = pane.search.text().to_owned();
                    pane.dir.set_filter(query);
                    pane.sync_view();
                }
            }
        }
    }

    // Pasek edycji ma priorytet nad resztą — nic więcej nie rób.
    if app.pane().edit.is_some() {
        let _ = ch;
        let _ = text;
    }
}

fn handle_text_key<T: TextInput>(entry: &mut T, ke: &KeyEvent) -> bool {
    let shift = ke.modifiers.shift;
    let ctrl = ke.modifiers.ctrl;
    if ctrl && ke.keysym == 0x61 {
        entry.select_all();
        return true;
    }
    if !ctrl {
        if let Some(t) = &ke.text {
            if !t.is_empty() && !t.chars().all(char::is_control) {
                entry.insert_str(t);
                return true;
            }
        }
    }
    match ke.keysym {
        0xff08 => entry.backspace(),
        0xffff => entry.delete_forward(),
        0xff51 => {
            if shift {
                entry.move_left_selecting(true)
            } else {
                entry.move_left()
            }
        }
        0xff53 => {
            if shift {
                entry.move_right_selecting(true)
            } else {
                entry.move_right()
            }
        }
        0xff50 => {
            if shift {
                entry.home_selecting(true)
            } else {
                entry.home()
            }
        }
        0xff57 => {
            if shift {
                entry.end_selecting(true)
            } else {
                entry.end()
            }
        }
        _ => return false,
    }
    true
}

fn apply_edit(app: &mut App) {
    let dir = app.pane().dir.path.clone();
    let edit = app.pane_mut().edit.take();
    let Some(edit) = edit else { return };
    let text = edit.entry.text().trim().to_owned();
    if text.is_empty() {
        return;
    }
    if let Some(from) = edit.rename {
        let to = from
            .parent()
            .map(|p| p.join(&text))
            .unwrap_or_else(|| dir.join(&text));
        if to != from {
            app.submit(Job::Rename { from, to });
        }
    } else if edit.new_folder {
        app.submit(Job::Mkdir {
            path: dir.join(&text),
        });
    } else {
        let target = if text.starts_with('/') {
            PathBuf::from(&text)
        } else if text == "~" {
            places::home()
        } else if let Some(rest) = text.strip_prefix("~/") {
            places::home().join(rest)
        } else {
            dir.join(&text)
        };
        app.navigate(app.active, target);
    }
}

fn move_selection(app: &mut App, delta: i32, extend: bool, lay: &Layout, theme: &Theme) {
    let pr = pane_rects(app, lay, app.active, theme);
    let rows = file_rows(&app.panes[app.active], pr.items);
    let p = &mut app.panes[app.active];
    let len = p.view.len();
    if len == 0 {
        return;
    }
    let current = p.view.selected().unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, len as i32 - 1) as usize;
    if extend {
        p.view.select_range(next);
    } else {
        p.view.set_selected(Some(next));
    }
    let slot = p.view.slot_rect(rows, next, theme);
    if slot.y() < rows.y() {
        let d = rows.y() - slot.y();
        p.view.set_scroll(p.view.scroll() - d);
    } else if slot.bottom() > rows.bottom() {
        let d = slot.bottom() - rows.bottom();
        p.view.set_scroll(p.view.scroll() + d);
    }
    p.view.clamp_scroll(rows, theme);
    p.preview_cache = None;
}

fn select_boundary(app: &mut App, end: bool, extend: bool, lay: &Layout, theme: &Theme) {
    let pr = pane_rects(app, lay, app.active, theme);
    let rows = file_rows(&app.panes[app.active], pr.items);
    let p = &mut app.panes[app.active];
    if p.view.len() == 0 {
        return;
    }
    let index = if end { p.view.len() - 1 } else { 0 };
    if extend {
        p.view.select_range(index);
    } else {
        p.view.set_selected(Some(index));
    }
    let slot = p.view.slot_rect(rows, index, theme);
    if slot.y() < rows.y() {
        let d = rows.y() - slot.y();
        p.view.set_scroll(p.view.scroll() - d);
    } else if slot.bottom() > rows.bottom() {
        let d = slot.bottom() - rows.bottom();
        p.view.set_scroll(p.view.scroll() + d);
    }
    p.view.clamp_scroll(rows, theme);
    p.preview_cache = None;
}

fn toggle_hidden(app: &mut App) {
    let pane = app.pane_mut();
    let show = !pane.dir.show_hidden();
    pane.dir.set_show_hidden(show);
    pane.sync_view();
}

fn new_folder_edit(app: &mut App) {
    let dir = app.pane().dir.path.clone();
    let mut edit = EditState::path(&dir);
    let mut entry = TextEntry::new(libluna_i18n::tr("Name"));
    entry.set_text(libluna_i18n::tr("New Folder"));
    entry.set_focused(true);
    entry.select_all();
    edit.entry = entry;
    edit.rename = None;
    edit.new_folder = true;
    app.pane_mut().edit = Some(edit);
}

fn copy_selection(app: &mut App, cut: bool) {
    let paths = app.pane().selection();
    if paths.is_empty() {
        return;
    }
    let n = paths.len();
    app.clipboard = Some(crate::app::Clipboard { paths, cut });
    app.show_toast(if cut {
        libluna_i18n::tr_args("{n} items cut", &[("n", &n.to_string())])
    } else {
        libluna_i18n::tr_args("{n} items copied", &[("n", &n.to_string())])
    });
}

fn paste(app: &mut App) {
    let Some(clip) = app.clipboard.clone() else {
        return;
    };
    let dst = app.pane().dir.path.clone();
    if clip.cut {
        app.submit(Job::Move {
            srcs: clip.paths.clone(),
            dst,
        });
        app.clipboard = None;
    } else {
        app.submit(Job::Copy {
            srcs: clip.paths,
            dst,
        });
    }
}

fn delete_selection(app: &mut App, permanent: bool) {
    let paths = app.pane().selection();
    if paths.is_empty() {
        return;
    }
    if permanent {
        app.submit(Job::Delete { paths });
    } else {
        app.submit(Job::Trash { paths });
    }
}

// ---------------------------------------------------------------------------
// Menu
// ---------------------------------------------------------------------------

fn open_menu(
    app: &mut App,
    at: Point,
    target: MenuTarget,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let (entries, cmds) = menu_entries(app, &target);
    let menu = Menu::new(entries);
    let width = menu.width(text, theme);
    let anchor = Rect::from_xywh(at.x, at.y, 1.0, 1.0);
    let panel = menu.place(anchor, width, Rect::from_size(size), theme);
    app.context = Some(ContextMenu {
        menu,
        panel,
        target,
        cmds,
    });
}

struct MenuBuild {
    entries: Vec<MenuEntry>,
    cmds: Vec<Option<MenuCmd>>,
}

impl MenuBuild {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            cmds: Vec::new(),
        }
    }

    fn action(&mut self, label: impl Into<String>, shortcut: Option<&str>, cmd: MenuCmd) {
        let mut entry = MenuEntry::action(label);
        if let Some(shortcut) = shortcut {
            entry = entry.shortcut(shortcut);
        }
        self.entries.push(entry);
        self.cmds.push(Some(cmd));
    }

    fn disabled(&mut self, label: impl Into<String>) {
        self.entries.push(MenuEntry::action(label).disabled());
        self.cmds.push(None);
    }

    fn sep(&mut self) {
        self.entries.push(MenuEntry::separator());
        self.cmds.push(None);
    }
}

fn menu_entries(app: &App, target: &MenuTarget) -> (Vec<MenuEntry>, Vec<Option<MenuCmd>>) {
    let tr = libluna_i18n::tr;
    let mut m = MenuBuild::new();
    match target {
        MenuTarget::Item(..) => {
            m.action(tr("Open"), Some("Enter"), MenuCmd::Open);
            m.action(tr("Open in Terminal"), Some("F4"), MenuCmd::OpenTerminal);
            m.sep();
            m.action(tr("Copy"), Some("Ctrl+C"), MenuCmd::Copy);
            m.action(tr("Cut"), Some("Ctrl+X"), MenuCmd::Cut);
            m.sep();
            m.action(tr("Rename"), Some("F2"), MenuCmd::Rename);
            m.action(tr("Move to Trash"), Some("Delete"), MenuCmd::Trash);
            m.action(
                tr("Delete Permanently"),
                Some("Shift+Delete"),
                MenuCmd::Delete,
            );
            m.sep();
            m.action(tr("Properties"), None, MenuCmd::Properties);
        }
        MenuTarget::TrashItem(_) => {
            m.action(tr("Restore"), None, MenuCmd::Restore);
            m.action(
                tr("Delete Permanently"),
                Some("Shift+Delete"),
                MenuCmd::Purge,
            );
            m.sep();
            m.action(tr("Properties"), None, MenuCmd::Properties);
        }
        MenuTarget::Device(i) => {
            m.action(tr("Unmount"), None, MenuCmd::Unmount(*i));
            m.action(tr("Open"), None, MenuCmd::OpenDevice(*i));
        }
        MenuTarget::Background | MenuTarget::AppMenu => {
            let pane = app.pane();
            if matches!(target, MenuTarget::Background) {
                if app.clipboard.is_some() {
                    m.action(tr("Paste"), Some("Ctrl+V"), MenuCmd::Paste);
                } else {
                    m.disabled(tr("Paste"));
                }
                m.action(tr("Select All"), Some("Ctrl+A"), MenuCmd::SelectAll);
                m.sep();
            }
            m.action(tr("New Folder"), Some("Ctrl+N"), MenuCmd::NewFolder);
            m.action(tr("New Tab"), Some("Ctrl+T"), MenuCmd::NewTab);
            m.action(
                if app.split {
                    tr("Single Pane")
                } else {
                    tr("Split View")
                },
                None,
                MenuCmd::Split,
            );
            m.sep();
            m.action(
                if pane.dir.show_hidden() {
                    tr("Hide Hidden Files")
                } else {
                    tr("Show Hidden Files")
                },
                Some("Ctrl+H"),
                MenuCmd::ToggleHidden,
            );
            m.sep();
            for key in [
                SortKey::Name,
                SortKey::Size,
                SortKey::Modified,
                SortKey::Kind,
            ] {
                let mark = if pane.dir.sort_key() == key {
                    "• "
                } else {
                    "  "
                };
                m.action(format!("{mark}{}", key.label()), None, MenuCmd::Sort(key));
            }
            m.action(
                if pane.dir.ascending() {
                    tr("Descending")
                } else {
                    tr("Ascending")
                },
                None,
                MenuCmd::ToggleAsc,
            );
            m.sep();
            m.action(tr("Refresh"), Some("F5"), MenuCmd::Refresh);
            m.action(tr("Empty Trash"), None, MenuCmd::EmptyTrash);
        }
    }
    (m.entries, m.cmds)
}

fn run_menu(app: &mut App, index: usize) {
    let Some(context) = app.context.take() else {
        return;
    };
    let Some(cmd) = context.cmds.get(index).copied().flatten() else {
        return;
    };
    if crate::app::debug() {
        eprintln!("luna-files: menu {cmd:?}");
    }
    let target = match &context.target {
        MenuTarget::Item(path, is_dir) => Some((path.clone(), *is_dir)),
        MenuTarget::TrashItem(path) => Some((path.clone(), false)),
        _ => None,
    };
    match cmd {
        MenuCmd::Open => {
            if let Some((path, is_dir)) = target {
                if is_dir {
                    app.navigate(app.active, path);
                } else {
                    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                }
            }
        }
        MenuCmd::OpenTerminal => {
            if let Some((path, is_dir)) = target {
                let dir = if is_dir {
                    path
                } else {
                    path.parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| app.pane().dir.path.clone())
                };
                app.open_terminal_at(&dir);
            }
        }
        MenuCmd::Copy | MenuCmd::Cut => {
            if let Some((path, _)) = target {
                app.clipboard = Some(crate::app::Clipboard {
                    paths: vec![path],
                    cut: matches!(cmd, MenuCmd::Cut),
                });
            }
        }
        MenuCmd::Rename => {
            if let Some((path, _)) = target {
                app.pane_mut().edit = Some(EditState::rename(&path));
            }
        }
        MenuCmd::Trash => {
            if let Some((path, _)) = target {
                app.submit(Job::Trash { paths: vec![path] });
            }
        }
        MenuCmd::Delete => {
            if let Some((path, _)) = target {
                app.submit(Job::Delete { paths: vec![path] });
            }
        }
        MenuCmd::Restore => {
            if let Some((path, _)) = target {
                app.submit(Job::Restore { path });
            }
        }
        MenuCmd::Purge => {
            if let Some((path, _)) = target {
                app.submit(Job::Purge { path });
            }
        }
        MenuCmd::Properties => {
            if let Some((path, _)) = target {
                show_properties(app, &path);
            }
        }
        MenuCmd::Paste => paste(app),
        MenuCmd::SelectAll => app.pane_mut().view.select_all(),
        MenuCmd::NewFolder => new_folder_edit(app),
        MenuCmd::NewTab => app.new_tab(),
        MenuCmd::Split => {
            app.split = !app.split;
            if !app.split {
                app.active = 0;
            }
        }
        MenuCmd::ToggleHidden => toggle_hidden(app),
        MenuCmd::Sort(key) => set_sort(app, key),
        MenuCmd::ToggleAsc => {
            let asc = app.pane().dir.ascending();
            app.pane_mut().dir.set_ascending(!asc);
            app.pane_mut().sync_view();
        }
        MenuCmd::Refresh => app.reload_pane(app.active),
        MenuCmd::EmptyTrash => {
            app.confirm_empty = Some(
                AlertDialog::new(
                    libluna_i18n::tr("Empty Trash"),
                    libluna_i18n::tr("Permanently delete all items in the Trash?"),
                )
                .button(libluna_i18n::tr("Empty Trash"))
                .button(libluna_i18n::tr("Cancel")),
            );
        }
        MenuCmd::Unmount(i) => {
            if let Some(m) = app.mounts.get(i) {
                places::unmount_device(&m.dev);
                app.show_toast(libluna_i18n::tr("Unmounting…"));
            }
        }
        MenuCmd::OpenDevice(i) => {
            if let Some(m) = app.mounts.get(i) {
                let mount = m.mount.clone();
                app.navigate(app.active, mount);
            }
        }
    }
}

fn set_sort(app: &mut App, key: SortKey) {
    let pane = app.pane_mut();
    pane.dir.set_sort_key(key);
    pane.sync_view();
}

fn show_properties(app: &mut App, path: &Path) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let kind = if meta.is_dir() {
        Kind::Folder.label()
    } else {
        libluna_i18n::tr("File")
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    let msg = libluna_i18n::tr_args(
        "{name} · {kind} · {size} · {date}",
        &[
            (
                "name",
                &path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            ("kind", &kind),
            (
                "size",
                &human_size(if meta.is_dir() { 0 } else { meta.len() }),
            ),
            ("date", &format_mtime(modified)),
        ],
    );
    app.show_toast(msg);
}
