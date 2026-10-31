//! Warstwa UI: układ, rysowanie i obsługa wejścia.
//!
//! Wszystkie napisy idą przez `libluna_i18n::tr` (klucz = angielski tekst).

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, Insets, KeyEvent, Modifiers, Point, PointerButton, Rect, Size,
    Theme, ThemePreference,
};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    clipboard_command, icon, AlertDialog, Button, ButtonVariant, ControlsSide, Dialog,
    Entry as TextEntry, HeaderBar, IconName, Menu, MenuEntry, PaintCtx, PathBar, PreferencesDialog,
    Switch, Tab, TabBar, TextClipboardCommand, TextInput, ViewMode, ViewSwitcher, WindowControl,
};

use crate::app::{
    App, ContextMenu, DragState, DropTarget, EditState, HoverTip, MenuCmd, MenuTarget, Pane,
    SideAction, TextPasteTarget, SPLIT_GAP,
};
use crate::archive;
use crate::list;
use crate::model::{format_mtime, human_size, SortKey};
use crate::open_with::{self, DesktopApp, OpenWithDialog, ROW_H as OPEN_ROW_H};
use crate::ops::Job;
use crate::places;
use crate::prefs;
use crate::trash;

const SIDEBAR_W: f64 = 216.0;
const STATUS_H: f64 = 28.0;
const PREVIEW_W: f64 = 300.0;
const PAD: f64 = 8.0;
const BTN: f64 = 28.0;
const PANE_PATH_H: f64 = 26.0;

struct Layout {
    /// Cały pasek tytułu: jeden wiersz albo dwa (gdy otwarte są zakładki).
    header: Rect,
    /// Górny wiersz paska — kontrolki okna i zakładki.
    top: Rect,
    /// Pasek zakładek wewnątrz `HeaderBar` (jego obszar `content`).
    tabs: Rect,
    /// Przycisk „nowa zakładka” obok zakładek (tylko przy >1 zakładce).
    plus: Rect,
    /// Wiersz nawigacji (przy jednej zakładce to ten sam wiersz co `top`).
    nav: Rect,
    sidebar: Rect,
    status: Rect,
    panes: [Rect; 2],
}

struct Chrome {
    sidebar_toggle: Rect,
    back: Rect,
    forward: Rect,
    up: Rect,
    search: Rect,
    view: Rect,
    menu: Rect,
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

/// Pasek zakładek z bieżącego stanu aplikacji.
fn tab_bar(app: &App) -> TabBar {
    let pane = app.pane();
    let tabs: Vec<Tab> = pane
        .tabs
        .iter()
        .map(|p| Tab::new(tab_title(p)).closable(true))
        .collect();
    let mut bar = TabBar::with_tabs(tabs).expand(false);
    bar.set_selected(pane.tab);
    bar.set_hovered(app.tab_hover);
    bar
}

fn layout(app: &App, size: Size, theme: &Theme, text: &TextEngine) -> Layout {
    let hh = theme.metrics.headerbar.height;
    let multi = app.pane().tabs.len() > 1;
    // Pasek jest **jeden**: nawigacja siedzi w tym samym wierszu co kontrolki okna.
    // Dopiero przy wielu zakładkach dochodzi drugi wiersz (na górze), bo tam jest
    // ich miejsce — dokładnie jak w GNOME Files.
    let header_h = if multi { hh * 2.0 } else { hh };
    let header = Rect::from_xywh(0.0, 0.0, size.width, header_h);
    let top = Rect::from_xywh(0.0, 0.0, size.width, hh);
    let nav = if multi {
        Rect::from_xywh(0.0, hh, size.width, hh)
    } else {
        top
    };
    // Zakładki zajmują `content` górnego wiersza — między krawędzią a kontrolkami okna.
    let content = HeaderBar::new()
        .controls(ControlsSide::Right)
        .controls_layout(top, theme)
        .content;
    let tabs_w = (content.width() - BTN - 4.0).max(1.0);
    let tab_h = theme.metrics.tabs.height.min(content.height());
    let tabs = Rect::from_xywh(content.x(), content.center().y - tab_h / 2.0, tabs_w, tab_h);
    // „+” tuż za ostatnią zakładką (jak w przeglądarce), ale nigdy pod kontrolkami okna.
    let plus = if multi {
        let last_right = tab_bar(app)
            .segments(tabs, text, theme)
            .last()
            .map(|r| r.right())
            .unwrap_or(tabs.x());
        Rect::from_xywh(
            (last_right + 6.0).min(content.right() - BTN),
            content.center().y - BTN / 2.0,
            BTN,
            BTN,
        )
    } else {
        Rect::ZERO
    };
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
        top,
        tabs,
        plus,
        nav,
        sidebar,
        status,
        panes,
    }
}

fn chrome(app: &App, lay: &Layout, theme: &Theme) -> Chrome {
    let multi = app.pane().tabs.len() > 1;
    // Przy jednej zakładce nawigacja dzieli wiersz z kontrolkami okna, więc kończymy
    // przed nimi. Przy dwóch wierszach dolny jest wolny na całej szerokości.
    let (left, right) = if multi {
        (lay.nav.x() + PAD, lay.nav.right() - PAD)
    } else {
        let content = HeaderBar::new()
            .controls(ControlsSide::Right)
            .controls_layout(lay.top, theme)
            .content;
        (content.x(), content.right())
    };
    let cy = lay.nav.center().y;
    let mut x = left;
    let btn = |x: &mut f64| {
        let r = Rect::from_xywh(*x, cy - BTN / 2.0, BTN, BTN);
        *x += BTN + 2.0;
        r
    };
    let sidebar_toggle = btn(&mut x);
    let back = btn(&mut x);
    let forward = btn(&mut x);
    let up = btn(&mut x);

    let menu = Rect::from_xywh(right - BTN, cy - BTN / 2.0, BTN, BTN);
    let view = Rect::from_xywh(menu.x() - BTN - 4.0, cy - BTN / 2.0, BTN, BTN);
    let search = Rect::from_xywh(view.x() - BTN - 4.0, cy - BTN / 2.0, BTN, BTN);
    let path = Rect::from_xywh(
        up.right() + PAD,
        lay.nav.y() + 5.0,
        (search.x() - PAD - (up.right() + PAD)).max(40.0),
        (lay.nav.height() - 10.0).max(20.0),
    );
    Chrome {
        sidebar_toggle,
        back,
        forward,
        up,
        search,
        view,
        menu,
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
    let lay = layout(app, size, theme, text);
    let ch = chrome(app, &lay, theme);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene
        .fill_rect(Rect::from_size(size), theme.colors.window_bg);

    paint_header(&mut ctx, app, &lay);
    paint_nav(&mut ctx, app, &ch);
    paint_sidebar(&mut ctx, app, &lay);
    paint_panes(&mut ctx, app, &lay);
    paint_status(&mut ctx, app, &lay);
    paint_hover_tip(&mut ctx, app, size);
    paint_drag(&mut ctx, app, size);

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
    if app.settings_open {
        paint_settings(&mut ctx, app, size);
    }
    if let Some(about) = &app.about {
        about.paint(&mut ctx, size);
    }
    if let Some(open_with) = &app.open_with {
        paint_open_with(&mut ctx, open_with, size);
    }
}

fn paint_header(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    let multi = app.pane().tabs.len() > 1;
    // Jeden pasek: tło obejmuje też dolny wiersz, gdy są zakładki.
    ctx.scene
        .fill_rect(lay.header, ctx.theme.colors.headerbar_bg);
    // Kontrolki okna rysuje widget — bez tytułu, bo w jednym wierszu jest pasek ścieżki.
    HeaderBar::new()
        .controls(ControlsSide::Right)
        .paint(ctx, lay.top, app.controls_hover);
    if multi {
        // Oba wiersze mają wyglądać jak jeden pasek: zamalowuj linię pod górnym.
        ctx.scene.fill_rect(
            Rect::from_xywh(lay.top.x(), lay.top.bottom() - 1.0, lay.top.width(), 1.0),
            ctx.theme.colors.headerbar_bg,
        );
        paint_tabs(ctx, app, lay);
    }
    // Jedna linia pod całym paskiem (nie dwie).
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.header.x(),
            lay.header.bottom() - 1.0,
            lay.header.width(),
            1.0,
        ),
        ctx.theme.colors.border,
    );
}

fn paint_tabs(ctx: &mut PaintCtx, app: &App, lay: &Layout) {
    tab_bar(app).paint(ctx, lay.tabs);
    icon_button(
        ctx,
        lay.plus,
        IconName::Plus,
        true,
        false,
        is_hovered(app, lay.plus),
    );
}

fn paint_nav(ctx: &mut PaintCtx, app: &App, ch: &Chrome) {
    let pane = app.pane();

    // Przełącznik paska bocznego (jak w aplikacjach libLuny).
    let sidebar_bg = if is_hovered(app, ch.sidebar_toggle) {
        ctx.theme.colors.shade.scale_alpha(1.6)
    } else {
        ctx.theme.colors.shade
    };
    ctx.scene
        .fill_rounded_rect(ch.sidebar_toggle, ctx.theme.radius.small, sidebar_bg);
    sidebar_icon(
        ctx,
        ctx.theme.colors.text,
        ch.sidebar_toggle,
        app.sidebar_visible,
    );

    icon_button(
        ctx,
        ch.back,
        IconName::ChevronLeft,
        !pane.back.is_empty(),
        false,
        is_hovered(app, ch.back),
    );
    icon_button(
        ctx,
        ch.forward,
        IconName::ChevronRight,
        !pane.forward.is_empty(),
        false,
        is_hovered(app, ch.forward),
    );
    icon_up(
        ctx,
        ch.up,
        pane.dir.parent().is_some(),
        is_hovered(app, ch.up),
    );

    if let Some(edit) = &pane.edit {
        edit.entry.paint(ctx, ch.path);
    } else if pane.search_active {
        pane.search.paint(ctx, ch.path);
    } else {
        let bar = PathBar::new(path_components(&pane.dir.path));
        bar.paint(ctx, ch.path);
    }

    icon_button(
        ctx,
        ch.search,
        IconName::Search,
        true,
        pane.search_active,
        is_hovered(app, ch.search),
    );
    let view_icon = match pane.view.mode() {
        ViewMode::List => IconName::Grid,
        ViewMode::Grid => IconName::List,
    };
    icon_button(
        ctx,
        ch.view,
        view_icon,
        true,
        false,
        is_hovered(app, ch.view),
    );
    icon_button(
        ctx,
        ch.menu,
        IconName::Menu,
        true,
        false,
        is_hovered(app, ch.menu),
    );
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

/// Podświetlenie celu i „duch” przeciąganych plików.
fn paint_drag(ctx: &mut PaintCtx, app: &App, size: Size) {
    let Some(drag) = &app.drag else {
        return;
    };
    if !drag.active {
        return;
    }
    if let Some(rect) = drag.target_rect {
        let color = if matches!(drag.target, Some(DropTarget::Trash)) {
            ctx.theme.colors.error
        } else {
            ctx.theme.colors.accent
        };
        ctx.scene
            .fill_rounded_rect(rect, 6.0, color.scale_alpha(0.16));
        ctx.scene.stroke_rounded_rect(rect, 6.0, 1.5, color);
    }
    let Some(pointer) = app.pointer else {
        return;
    };
    let caption = ctx.theme.typography.caption.size;
    let n = drag.paths.len();
    let mut label = if n == 1 {
        drag.paths[0]
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| drag.paths[0].display().to_string())
    } else {
        libluna_i18n::tr_args("{n} items", &[("n", &n.to_string())])
    };
    if drag.copy {
        label = format!("{} · {label}", libluna_i18n::tr("Copy"));
    }
    let label = libluna_widgets::display::ellipsize(&label, 260.0, caption, ctx.text);
    let w = ctx.text.measure(&label, caption, weight::NORMAL).width + 24.0;
    let h = caption * 1.6 + 12.0;
    let rect = Rect::from_xywh(
        (pointer.x + 16.0).clamp(8.0, (size.width - w - 8.0).max(8.0)),
        (pointer.y + 18.0).clamp(8.0, (size.height - h - 8.0).max(8.0)),
        w,
        h,
    );
    ctx.scene
        .fill_rounded_rect(rect, 6.0, ctx.theme.colors.accent);
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        &label,
        caption,
        weight::MEDIUM,
        ctx.theme.colors.accent_fg,
    );
}

/// Dymek z informacjami o elemencie pod kursorem (po chwili najechania).
fn paint_hover_tip(ctx: &mut PaintCtx, app: &App, size: Size) {
    let Some(tip) = &app.tooltip else {
        return;
    };
    if tip.since.elapsed() < Duration::from_millis(650)
        || app.context.is_some()
        || app.settings_open
        || app.about.is_some()
        || app.conflict.is_some()
        || app.confirm_empty.is_some()
    {
        return;
    }
    let Some(pointer) = app.pointer else {
        return;
    };
    let theme = *ctx.theme;
    let body = theme.typography.body.size;
    let caption = theme.typography.caption.size;
    let pad = theme.spacing.md;
    let line_h = caption * 1.5;
    let max_w = 420.0;
    let entry = &tip.entry;
    let kind = if entry.is_dir {
        libluna_i18n::tr("Folder")
    } else {
        entry.kind.label()
    };
    let size_text = if entry.is_dir {
        String::new()
    } else {
        human_size(entry.size)
    };
    let second = if size_text.is_empty() {
        kind
    } else {
        format!("{kind} · {size_text}")
    };
    let lines: [(String, f64, u16, Color); 4] = [
        (entry.name.clone(), body, weight::BOLD, theme.colors.text),
        (second, caption, weight::NORMAL, theme.colors.dim_text),
        (
            format!(
                "{}: {}",
                libluna_i18n::tr("Modified"),
                format_mtime(entry.modified)
            ),
            caption,
            weight::NORMAL,
            theme.colors.dim_text,
        ),
        (
            entry.path.display().to_string(),
            caption,
            weight::NORMAL,
            theme.colors.dim_text,
        ),
    ];
    let content_w = (max_w - 2.0 * pad).max(1.0);
    let lines: Vec<(String, f64, u16, Color)> = lines
        .into_iter()
        .map(|(text, size, weight, color)| {
            (
                libluna_widgets::display::ellipsize(&text, content_w, size, ctx.text),
                size,
                weight,
                color,
            )
        })
        .collect();
    let measured = lines
        .iter()
        .map(|(text, size, weight, _)| ctx.text.measure(text, *size, *weight).width)
        .fold(0.0_f64, f64::max);
    let w = (measured + 2.0 * pad).clamp(180.0, max_w);
    let h = 2.0 * pad + line_h * lines.len() as f64;
    let mut x = pointer.x + 16.0;
    let mut y = pointer.y + 22.0;
    if x + w > size.width - 8.0 {
        x = (pointer.x - w - 12.0).max(8.0);
    }
    if y + h > size.height - 8.0 {
        y = (pointer.y - h - 12.0).max(8.0);
    }
    let rect = Rect::from_xywh(
        x.clamp(8.0, (size.width - w - 8.0).max(8.0)),
        y.clamp(8.0, (size.height - h - 8.0).max(8.0)),
        w,
        h,
    );
    ctx.scene.fill_rounded_rect(rect, 8.0, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, 8.0, 1.0, theme.colors.border);
    for (i, (text, size, weight, color)) in lines.iter().enumerate() {
        ctx.label_left(
            rect.x() + pad,
            rect.y() + pad + line_h * (i as f64 + 0.75),
            text,
            *size,
            *weight,
            *color,
        );
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
        if total > 0 {
            let bar = Rect::from_xywh(
                lay.status.x(),
                lay.status.y() - 3.0,
                lay.status.width() * ratio,
                3.0,
            );
            ctx.scene.fill_rect(bar, colors.accent);
        }
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
        // Operacje bez znanego rozmiaru (np. archiwa) nie mają procentów — pokazujemy
        // samą nazwę zadania i bieżący element.
        let mut label = if total > 0 {
            format!(
                "{} {}%",
                app.op_label.clone().unwrap_or_default(),
                (ratio * 100.0).round() as i64
            )
        } else {
            app.op_label.clone().unwrap_or_default()
        };
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

fn icon_button(
    ctx: &mut PaintCtx,
    rect: Rect,
    name: IconName,
    enabled: bool,
    active: bool,
    hover: bool,
) {
    if enabled && !active && hover {
        ctx.scene
            .fill_rounded_rect(rect, 6.0, ctx.theme.colors.shade);
    }
    if active && enabled {
        ctx.scene
            .fill_rounded_rect(rect, 6.0, ctx.theme.colors.shade);
    }
    let color = if enabled {
        ctx.theme.colors.text
    } else {
        ctx.theme.colors.dim_text.scale_alpha(0.5)
    };
    icon::paint_icon(ctx, name, rect.deflate(Insets::splat(6.0)), color);
}

/// Czy kursor jest nad prostokątem (dla podświetlenia przycisków).
fn is_hovered(app: &App, rect: Rect) -> bool {
    app.pointer.is_some_and(|p| rect.contains(p))
}

/// Ikona przełącznika paska bocznego (◧ / ▭) — jak w pozostałych aplikacjach Luny.
fn sidebar_icon(ctx: &mut PaintCtx, color: Color, box_: Rect, filled: bool) {
    let c = box_.center();
    let (w, h) = (18.0, 14.0);
    let outer = Rect::from_xywh(c.x - w / 2.0, c.y - h / 2.0, w, h);
    ctx.scene.stroke_rounded_rect(outer, 3.0, 1.5, color);
    if filled {
        let inner = Rect::from_xywh(outer.x() + 2.0, outer.y() + 2.0, w * 0.36, h - 4.0);
        ctx.scene.fill_rounded_rect(inner, 2.0, color);
    }
}

fn icon_up(ctx: &mut PaintCtx, rect: Rect, enabled: bool, hover: bool) {
    if enabled && hover {
        ctx.scene
            .fill_rounded_rect(rect, 6.0, ctx.theme.colors.shade);
    }
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

// ---------------------------------------------------------------------------
// Ustawienia / O aplikacji
// ---------------------------------------------------------------------------

struct SettingsLayout {
    rows: [Rect; 4],
    theme_seg: Rect,
    view_seg: Rect,
    lang_seg: Rect,
    hidden_switch: Rect,
    done: Rect,
}

fn settings_dialog() -> PreferencesDialog {
    PreferencesDialog::new(libluna_i18n::tr("Settings"), 376.0)
}

fn settings_layout(size: Size, theme: &Theme) -> SettingsLayout {
    let rects = settings_dialog().rects(size, theme);
    let pad = theme.spacing.lg;
    let inner = rects.content.deflate(Insets::new(pad, pad, pad, pad));
    let row_h = 46.0;
    let rows: [Rect; 4] = std::array::from_fn(|i| {
        Rect::from_xywh(
            inner.x(),
            inner.y() + i as f64 * row_h,
            inner.width(),
            row_h,
        )
    });
    let seg_w = 232.0_f64.min(inner.width() * 0.55);
    let seg = |i: usize| {
        Rect::from_xywh(
            inner.right() - seg_w,
            rows[i].center().y - 15.0,
            seg_w,
            30.0,
        )
    };
    let (sw, sh) = Switch::new(false).size(theme);
    let hidden_switch = Rect::from_xywh(inner.right() - sw, rows[3].center().y - sh / 2.0, sw, sh);
    let bh = theme.metrics.button.height;
    let done = Rect::from_xywh(inner.right() - 120.0, inner.bottom() - bh, 120.0, bh);
    SettingsLayout {
        rows,
        theme_seg: seg(0),
        view_seg: seg(1),
        lang_seg: seg(2),
        hidden_switch,
        done,
    }
}

fn paint_settings(ctx: &mut PaintCtx, app: &App, size: Size) {
    settings_dialog().paint(ctx, size);
    let theme = *ctx.theme;
    let lay = settings_layout(size, &theme);
    let body = theme.typography.body.size;
    let labels = [
        libluna_i18n::tr("Theme"),
        libluna_i18n::tr("View"),
        libluna_i18n::tr("Language"),
        libluna_i18n::tr("Show hidden files"),
    ];
    for (i, label) in labels.iter().enumerate() {
        ctx.label_left(
            lay.rows[i].x() + 12.0,
            lay.rows[i].center().y,
            label,
            body,
            weight::NORMAL,
            theme.colors.text,
        );
        if i > 0 {
            ctx.scene.fill_rect(
                Rect::from_xywh(lay.rows[i].x(), lay.rows[i].y(), lay.rows[i].width(), 1.0),
                theme.colors.border,
            );
        }
    }

    let mut theme_switcher = ViewSwitcher::new([
        libluna_i18n::tr("Light"),
        libluna_i18n::tr("Dark"),
        libluna_i18n::tr("Auto"),
    ]);
    let theme_index = ThemePreference::ALL
        .iter()
        .position(|p| *p == app.theme_pref)
        .unwrap_or(2);
    theme_switcher.set_selected(theme_index);
    theme_switcher.paint(ctx, lay.theme_seg);

    let mut view_switcher = ViewSwitcher::new([libluna_i18n::tr("List"), libluna_i18n::tr("Grid")]);
    view_switcher.set_selected(match app.pane().view.mode() {
        ViewMode::List => 0,
        ViewMode::Grid => 1,
    });
    view_switcher.paint(ctx, lay.view_seg);

    // Nazwy języków pokazujemy natywnie — tak jak w systemowych ustawieniach.
    let mut lang_switcher = ViewSwitcher::new(["Polski", "English", "Deutsch"]);
    lang_switcher.set_selected(prefs::language_index(&app.prefs.language));
    lang_switcher.paint(ctx, lay.lang_seg);

    Switch::new(app.pane().dir.show_hidden()).paint(ctx, lay.hidden_switch);
    Button::new(libluna_i18n::tr("Done")).paint(ctx, lay.done);
}

// ---------------------------------------------------------------------------
// „Otwórz za pomocą…”
// ---------------------------------------------------------------------------

/// Szerokość okna wyboru aplikacji.
const OPEN_WITH_WIDTH: f64 = 520.0;

struct OpenWithLayout {
    name: Rect,
    list: Rect,
    remember: Rect,
    remember_label: Rect,
    open: Rect,
    cancel: Rect,
}

fn open_with_dialog(count: usize, remember: bool, theme: &Theme) -> Dialog {
    let pad = theme.spacing.lg;
    let gap = theme.spacing.md;
    let rows = count.clamp(1, open_with::MAX_VISIBLE_ROWS) as f64;
    let switch_h = if remember {
        theme.metrics.switch.height.max(20.0)
    } else {
        0.0
    };
    let height = theme.metrics.sheet.header
        + pad
        + theme.typography.body.line_height
        + gap
        + rows * OPEN_ROW_H
        + gap
        + switch_h
        + gap
        + theme.metrics.button.height
        + pad;
    Dialog::new(libluna_i18n::tr("Open with")).size(OPEN_WITH_WIDTH, height)
}

fn open_with_layout(
    ow: &OpenWithDialog,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) -> OpenWithLayout {
    let remember = ow.can_remember();
    let rects = open_with_dialog(ow.apps.len(), remember, theme).rects(size, theme);
    let pad = theme.spacing.lg;
    let gap = theme.spacing.md;
    let inner = rects.content.deflate(Insets::new(pad, pad, pad, pad));

    let mut y = inner.y();
    let line = theme.typography.body.line_height;
    let name = Rect::from_xywh(inner.x(), y, inner.width(), line);
    y += line + gap;
    let rows = ow.apps.len().clamp(1, open_with::MAX_VISIBLE_ROWS) as f64;
    let list = Rect::from_xywh(inner.x(), y, inner.width(), rows * OPEN_ROW_H);
    y += list.height() + gap;

    let (sw, sh) = Switch::new(false).size(theme);
    let switch_h = if remember { sh.max(20.0) } else { 0.0 };
    let remember_row = Rect::from_xywh(inner.x(), y, inner.width(), switch_h);
    let remember = Rect::from_xywh(
        inner.right() - sw,
        remember_row.center().y - sh / 2.0,
        sw,
        sh,
    );
    let remember_label = Rect::from_xywh(
        inner.x(),
        remember_row.y(),
        (inner.width() - sw - gap).max(1.0),
        switch_h,
    );
    y += switch_h + gap;

    let btn_h = theme.metrics.button.height;
    let open_w = Button::new(libluna_i18n::tr("Open"))
        .variant(ButtonVariant::Suggested)
        .width(text, theme);
    let cancel_w = Button::new(libluna_i18n::tr("Cancel")).width(text, theme);
    let button_y = (inner.bottom() - btn_h).max(y);
    let open = Rect::from_xywh(inner.right() - open_w, button_y, open_w, btn_h);
    let cancel = Rect::from_xywh(open.x() - gap - cancel_w, button_y, cancel_w, btn_h);

    OpenWithLayout {
        name,
        list,
        remember,
        remember_label,
        open,
        cancel,
    }
}

fn paint_open_with(ctx: &mut PaintCtx, ow: &OpenWithDialog, size: Size) {
    let theme = *ctx.theme;
    let lay = open_with_layout(ow, size, &theme, ctx.text);
    open_with_dialog(ow.apps.len(), ow.can_remember(), &theme).paint(ctx, size);

    let body = theme.typography.body.size;
    let caption = theme.typography.caption.size;

    // Nazwa pliku po lewej, typ MIME po prawej.
    let file_name = ow
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| ow.path.display().to_string());
    let mime = ow.mime.clone().unwrap_or_default();
    let mime_w = ctx.measure(&mime, caption, weight::NORMAL);
    let name_w = (lay.name.width() - mime_w - 8.0).max(24.0);
    let name = libluna_widgets::display::ellipsize(&file_name, name_w, body, ctx.text);
    ctx.label_left(
        lay.name.x(),
        lay.name.center().y,
        &name,
        body,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_right(
        lay.name.right(),
        lay.name.center().y,
        &mime,
        caption,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Lista aplikacji (przewijana).
    ctx.scene.fill_rounded_rect(
        lay.list,
        theme.radius.medium,
        theme.colors.shade.scale_alpha(0.5),
    );
    let mut scroll = ow.scroll;
    scroll.set_content_height(ow.apps.len() as f64 * OPEN_ROW_H);
    scroll.set_offset(ow.scroll.offset(), lay.list);
    ctx.scene.push_clip(lay.list);
    for index in 0..ow.apps.len() {
        let row = Rect::from_xywh(
            lay.list.x() + 2.0,
            lay.list.y() + index as f64 * OPEN_ROW_H - scroll.offset(),
            (lay.list.width() - 4.0).max(1.0),
            OPEN_ROW_H,
        );
        if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
            continue;
        }
        paint_open_with_row(ctx, ow, index, row);
    }
    ctx.scene.pop_clip();
    scroll.paint_scrollbar(ctx, lay.list);

    if ow.apps.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.center().y,
            &libluna_i18n::tr("No applications found"),
            body,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }

    if ow.can_remember() {
        Switch::new(ow.remember).paint(ctx, lay.remember);
        ctx.label_left(
            lay.remember_label.x(),
            lay.remember.center().y,
            &libluna_i18n::tr("Always use for this file type"),
            body,
            weight::NORMAL,
            theme.colors.text,
        );
    }

    let enabled = ow.selected_app().is_some_and(|app| app.available());
    Button::new(libluna_i18n::tr("Open"))
        .variant(ButtonVariant::Suggested)
        .enabled(enabled)
        .paint(ctx, lay.open);
    Button::new(libluna_i18n::tr("Cancel")).paint(ctx, lay.cancel);
}

fn paint_open_with_row(ctx: &mut PaintCtx, ow: &OpenWithDialog, index: usize, row: Rect) {
    let theme = *ctx.theme;
    let app = &ow.apps[index];
    let selected = ow.selected == Some(index);
    let available = app.available();
    if selected {
        ctx.scene
            .fill_rounded_rect(row, theme.radius.small, theme.colors.accent);
    } else if ow.hovered == Some(index) {
        ctx.scene
            .fill_rounded_rect(row, theme.radius.small, theme.colors.shade);
    }
    let fg = if selected {
        theme.colors.accent_fg
    } else if available {
        theme.colors.text
    } else {
        theme.colors.dim_text
    };
    let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - 11.0, 22.0, 22.0);
    if !libluna_widgets::paint_named_icon(ctx, app.icon.as_deref(), icon_rect) {
        icon::paint_icon(ctx, IconName::App, icon_rect, fg);
    }

    let body = theme.typography.body.size;
    let caption = theme.typography.caption.size;
    let name = app.display_name(libluna_i18n::locale().language());
    let badge = if ow.default_id.as_deref() == Some(app.desktop_id.as_str()) {
        libluna_i18n::tr("Default")
    } else {
        String::new()
    };
    let badge_w = if badge.is_empty() {
        0.0
    } else {
        ctx.measure(&badge, caption, weight::NORMAL) + 12.0
    };
    let name_w = (row.width() - 38.0 - 8.0 - badge_w).max(20.0);
    let label = libluna_widgets::display::ellipsize(name, name_w, body, ctx.text);
    ctx.label_left(
        row.x() + 38.0,
        row.center().y,
        &label,
        body,
        if selected {
            weight::MEDIUM
        } else {
            weight::NORMAL
        },
        fg,
    );
    if !badge.is_empty() {
        ctx.label_right(
            row.right() - 8.0,
            row.center().y,
            &badge,
            caption,
            weight::NORMAL,
            if selected {
                theme.colors.accent_fg
            } else {
                theme.colors.dim_text
            },
        );
    }
}

/// Akcja wyliczona bez trzymania pożyczki stanu okna.
#[derive(Clone, Copy)]
enum OpenWithAct {
    None,
    Close,
    Open,
    OpenAt(usize),
    Select(usize),
    ToggleRemember,
    Hover(Option<usize>),
    Scroll(f64, Rect),
    Move(i32, f64),
}

/// Modalne okno „Otwórz za pomocą…” pochłania całe wejście.
fn on_open_with_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let mut finish = None;
    for event in events {
        let act = {
            let Some(ow) = app.open_with.as_ref() else {
                return;
            };
            let lay = open_with_layout(ow, size, theme, text);
            let dialog = open_with_dialog(ow.apps.len(), ow.can_remember(), theme);
            match event {
                Event::PointerButton(pe)
                    if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed =>
                {
                    let p = pe.position;
                    if dialog.close_at(size, theme, p)
                        || dialog.scrim_at(size, theme, p)
                        || lay.cancel.contains(p)
                    {
                        OpenWithAct::Close
                    } else if lay.open.contains(p) {
                        if ow.selected_app().is_some_and(|app| app.available()) {
                            OpenWithAct::Open
                        } else {
                            OpenWithAct::None
                        }
                    } else if ow.can_remember()
                        && (lay.remember.contains(p) || lay.remember_label.contains(p))
                    {
                        OpenWithAct::ToggleRemember
                    } else if let Some(index) = ow.index_at(lay.list, p) {
                        if !ow.apps[index].available() {
                            OpenWithAct::None
                        } else {
                            let now = Instant::now();
                            let double = ow.last_click.is_some_and(|(last, at)| {
                                last == index && now.duration_since(at) < Duration::from_millis(400)
                            });
                            if double {
                                OpenWithAct::OpenAt(index)
                            } else {
                                OpenWithAct::Select(index)
                            }
                        }
                    } else {
                        OpenWithAct::None
                    }
                }
                Event::PointerMotion { position, .. } => {
                    OpenWithAct::Hover(ow.index_at(lay.list, *position))
                }
                Event::Scroll(se) if lay.list.contains(se.position) => {
                    OpenWithAct::Scroll(se.dy, lay.list)
                }
                Event::Key(ke) if ke.state == ButtonState::Pressed => match ke.keysym {
                    0xff1b => OpenWithAct::Close,
                    0xff0d | 0xff8d => {
                        if ow.selected_app().is_some_and(|app| app.available()) {
                            OpenWithAct::Open
                        } else {
                            OpenWithAct::None
                        }
                    }
                    0xff52 => OpenWithAct::Move(-1, lay.list.height()),
                    0xff54 => OpenWithAct::Move(1, lay.list.height()),
                    _ => OpenWithAct::None,
                },
                _ => OpenWithAct::None,
            }
        };

        let Some(ow) = app.open_with.as_mut() else {
            return;
        };
        match act {
            OpenWithAct::None => {}
            OpenWithAct::Close => {
                finish = Some(false);
                break;
            }
            OpenWithAct::Open => {
                finish = Some(true);
                break;
            }
            OpenWithAct::OpenAt(index) => {
                ow.selected = Some(index);
                ow.last_click = None;
                finish = Some(true);
                break;
            }
            OpenWithAct::Select(index) => {
                ow.selected = Some(index);
                ow.last_click = Some((index, Instant::now()));
            }
            OpenWithAct::ToggleRemember => ow.remember = !ow.remember,
            OpenWithAct::Hover(index) => ow.hovered = index,
            OpenWithAct::Scroll(dy, viewport) => {
                ow.scroll
                    .set_content_height(ow.apps.len() as f64 * OPEN_ROW_H);
                ow.scroll.on_scroll(dy, viewport);
            }
            OpenWithAct::Move(delta, viewport) => {
                ow.move_selection(delta);
                ow.scroll_to_selection(viewport);
            }
        }
    }

    match finish {
        Some(false) => app.open_with = None,
        Some(true) => open_with_confirm(app),
        None => {}
    }
}

/// Uruchamia wybraną aplikację (opcjonalnie zapisując skojarzenie typu MIME).
fn open_with_confirm(app: &mut App) {
    let Some(ow) = app.open_with.take() else {
        return;
    };
    let Some(index) = ow.selected else {
        app.open_with = Some(ow);
        return;
    };
    if !ow.apps.get(index).is_some_and(DesktopApp::available) {
        // Wpis bez programu — nie zamykamy okna, użytkownik może wybrać inny.
        app.open_with = Some(ow);
        return;
    }
    let Some(chosen) = ow.apps.get(index).cloned() else {
        return;
    };
    if ow.remember {
        if let Some(mime) = ow.mime.as_deref() {
            if let Err(err) = open_with::set_default_app(mime, &chosen.desktop_id) {
                if crate::app::debug() {
                    eprintln!("luna-files: xdg-mime default: {err}");
                }
                app.show_toast(libluna_i18n::tr("Could not set the default application"));
            }
        }
    }
    if let Err(err) = open_with::launch(&chosen, &ow.path) {
        app.show_toast(err);
    }
}

/// Obsługuje klik w otwartym oknie Ustawień (modal blokuje resztę).
fn on_settings_press(app: &mut App, p: Point, size: Size, theme: &Theme) {
    let lay = settings_layout(size, theme);
    let dialog = settings_dialog();
    if dialog.close_at(size, theme, p) || dialog.scrim_at(size, theme, p) || lay.done.contains(p) {
        app.settings_open = false;
        return;
    }

    let theme_switcher = ViewSwitcher::new([
        libluna_i18n::tr("Light"),
        libluna_i18n::tr("Dark"),
        libluna_i18n::tr("Auto"),
    ]);
    if let Some(i) = theme_switcher.index_at(lay.theme_seg, p) {
        let pref = ThemePreference::ALL[i];
        app.theme_pref = pref;
        app.pending_theme = Some(pref);
        return;
    }

    let view_switcher = ViewSwitcher::new([libluna_i18n::tr("List"), libluna_i18n::tr("Grid")]);
    if let Some(i) = view_switcher.index_at(lay.view_seg, p) {
        app.set_view(if i == 0 {
            ViewMode::List
        } else {
            ViewMode::Grid
        });
        return;
    }

    let lang_switcher = ViewSwitcher::new(["Polski", "English", "Deutsch"]);
    if let Some(i) = lang_switcher.index_at(lay.lang_seg, p) {
        app.set_language(prefs::LANGUAGES[i]);
        return;
    }

    if lay.hidden_switch.contains(p) {
        let on = app.pane().dir.show_hidden();
        app.set_show_hidden(!on);
    }
}

pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme, text);
    let ch = chrome(app, &lay, theme);

    // Modal „Otwórz za pomocą…” jest najwyżej — pochłania całe wejście.
    if app.open_with.is_some() {
        on_open_with_events(app, events, size, theme, text);
        return;
    }

    // Modale (Ustawienia, O aplikacji) pochłaniają całe wejście.
    if app.settings_open {
        for event in events {
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    on_settings_press(app, pe.position, size, theme);
                }
            }
        }
        return;
    }
    if app.about.is_some() {
        for event in events {
            if let Event::PointerButton(pe) = event {
                if pe.button == PointerButton::Left && pe.state == ButtonState::Pressed {
                    let close = app.about.as_ref().is_some_and(|about| {
                        let r = about.rects(size, theme, text);
                        about.action_at(size, theme, text, pe.position).is_some()
                            || r.dialog.close.contains(pe.position)
                            || !r.dialog.panel.contains(pe.position)
                    });
                    if close {
                        app.about = None;
                    }
                }
            }
        }
        return;
    }

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
                (PointerButton::Left, ButtonState::Released) => {
                    on_left_release(app, pe.position, pe.modifiers, &lay, size, theme)
                }
                (PointerButton::Right, ButtonState::Pressed) => {
                    on_right_press(app, pe.position, &lay, size, theme, text)
                }
                (PointerButton::Middle, ButtonState::Pressed) => {
                    on_middle_press(app, pe.position, &lay, theme)
                }
                (PointerButton::Back, ButtonState::Pressed) => app.back(),
                (PointerButton::Forward, ButtonState::Pressed) => app.forward(),
                (_, ButtonState::Released) => {}
                _ => {}
            },
            Event::PointerMotion {
                position,
                modifiers,
            } => {
                let p = *position;
                app.pointer = Some(p);
                // Przeciąganie plików ma pierwszeństwo nad podświetleniami.
                if let Some(drag) = &mut app.drag {
                    drag.copy = modifiers.ctrl;
                    if !drag.active {
                        let dx = p.x - drag.start.x;
                        let dy = p.y - drag.start.y;
                        if dx * dx + dy * dy > 16.0 {
                            drag.active = true;
                        }
                    }
                }
                if app.drag.as_ref().is_some_and(|d| d.active) {
                    let (target, rect) = drop_target_at(app, &lay, p, theme);
                    if let Some(drag) = &mut app.drag {
                        drag.target = target;
                        drag.target_rect = rect;
                    }
                    app.tooltip = None;
                    app.controls_hover = None;
                    app.tab_hover = None;
                    app.sidebar.set_hovered(None);
                    for pane in &mut app.panes {
                        pane.view.set_hovered(None);
                    }
                    continue;
                }
                // Podświetlenie kontrolek okna (— □ ✕): `HeaderBar::paint` rysuje je
                // tylko dla kontrolki przekazanej jako `hovered`.
                app.controls_hover = if lay.top.contains(p) {
                    HeaderBar::new()
                        .controls(ControlsSide::Right)
                        .control_at(lay.top, theme, p)
                } else {
                    None
                };
                if let Some(context) = app.context.as_mut() {
                    context.menu.hover_at(context.panel, Some(p), theme);
                    app.tooltip = None;
                    continue;
                }
                let tab_hover = if app.pane().tabs.len() > 1 && lay.tabs.contains(p) {
                    tab_bar(app).index_at(lay.tabs, text, theme, p)
                } else {
                    None
                };
                app.tab_hover = tab_hover;
                let _hovered =
                    app.sidebar
                        .index_at(lay.sidebar.deflate(Insets::splat(6.0)), p, theme);
                app.sidebar.set_hovered(_hovered);
                let mut hover_entry = None;
                for i in 0..if app.split { 2 } else { 1 } {
                    let pr = pane_rects(app, &lay, i, theme);
                    let rows = file_rows(&app.panes[i], pr.items);
                    let hovered = hit_index(&app.panes[i], rows, p, theme);
                    app.panes[i].view.set_hovered(hovered);
                    if rows.contains(p) {
                        if let Some(index) = hovered {
                            if let Some(entry) = app.panes[i].dir.entry(index).cloned() {
                                hover_entry = Some(entry);
                            }
                        }
                    }
                    app.panes[i]
                        .search
                        .set_hover_clear(app.panes[i].search.clear_at(ch.path, theme, p));
                }
                // Dymek pokazujemy po chwili najechania na ten sam element; ruch
                // w obrębie kafelka nie resetuje licznika.
                let same = match (&app.tooltip, &hover_entry) {
                    (Some(tip), Some(entry)) => tip.entry.path == entry.path,
                    _ => false,
                };
                if !same {
                    app.tooltip = hover_entry.map(|entry| HoverTip {
                        entry,
                        since: Instant::now(),
                    });
                }
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.controls_hover = None;
                app.tab_hover = None;
                app.tooltip = None;
                if !app.drag.as_ref().is_some_and(|drag| drag.active) {
                    app.drag = None;
                }
                app.sidebar.set_hovered(None);
                for pane in &mut app.panes {
                    pane.view.set_hovered(None);
                }
            }
            Event::Scroll(se) => {
                app.tooltip = None;
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
            Event::Paste(text) => paste_text_into(app, &text),
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
    app.tooltip = None;
    app.drag = None;
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

    // 2. Kontrolki okna (górny wiersz paska).
    let header = HeaderBar::new().controls(ControlsSide::Right);
    if lay.top.contains(p) {
        if let Some(control) = header.control_at(lay.top, theme, p) {
            match control {
                WindowControl::Close => app.pending_close = true,
                WindowControl::Minimize => app.pending_minimize = true,
                WindowControl::Maximize => app.pending_maximize = true,
            }
            return;
        }
    }

    // 3. Zakładki (tylko gdy jest ich więcej niż jedna).
    if app.pane().tabs.len() > 1 && lay.top.contains(p) {
        if lay.plus.contains(p) {
            app.new_tab();
            return;
        }
        let bar = tab_bar(app);
        if let Some(i) = bar.close_at(lay.tabs, text, theme, p) {
            app.select_tab(i);
            app.close_tab();
            return;
        }
        if let Some(i) = bar.index_at(lay.tabs, text, theme, p) {
            app.select_tab(i);
            return;
        }
    }

    // 4. Pasek nawigacji (przy jednej zakładce w tym samym wierszu co kontrolki).
    if lay.nav.contains(p) {
        if ch.sidebar_toggle.contains(p) {
            app.sidebar_visible = !app.sidebar_visible;
            return;
        }
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
                return;
            }
            // Puste miejsce paska ścieżki (między segmentami) to nasz „pasek tytułu”
            // — bez tego okna nie dało się przesunąć, bo pasek zajmuje cały wiersz.
            app.pending_move = true;
            return;
        }
    }

    // 5. Reszta paska (wolne miejsce, także dolny wiersz z zakładkami)
    //    przeciąga okno — dokładnie jak pasek tytułu.
    if lay.header.contains(p) {
        app.pending_move = true;
        return;
    }

    // 6. Panel boczny.
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
                            app.last_mount_refresh = Instant::now() - Duration::from_secs(10);
                        }
                    }
                    SideAction::Mount(i) => {
                        if let Some(m) = app.mounts.get(*i) {
                            places::mount_device(&m.dev);
                            app.show_toast(libluna_i18n::tr("Mounting…"));
                            app.last_mount_refresh = Instant::now() - Duration::from_secs(10);
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
                        app.open_file(&entry.path);
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
                // Kandydat do przeciągania; aktywuje się dopiero po ruchu > 4 px.
                let paths = app.panes[i].selection();
                if !paths.is_empty() {
                    app.drag = Some(DragState {
                        paths,
                        start: p,
                        active: false,
                        copy: mods.ctrl,
                        target: None,
                        target_rect: None,
                    });
                }
            }
            return;
        }
        // Klik poza listą czyści zaznaczenie.
        app.panes[i].view.clear_selection();
        app.panes[i].preview_cache = None;
        return;
    }
}

fn on_left_release(
    app: &mut App,
    p: Point,
    mods: Modifiers,
    lay: &Layout,
    size: Size,
    theme: &Theme,
) {
    let Some(drag) = app.drag.take() else {
        return;
    };
    if !drag.active || !Rect::from_size(size).contains(p) {
        return;
    }
    // Cel liczymy ponownie na pozycji puszczenia — ostatni ruch mógł nie dotrzeć.
    let (target, _) = drop_target_at(app, lay, p, theme);
    let Some(target) = target else {
        return;
    };
    if let Some(job) = drag_job(target, drag.paths, mods.ctrl) {
        app.submit(job);
    }
}

/// Co znajduje się pod kursorem podczas przeciągania (katalog/kosz + prostokąt podświetlenia).
fn drop_target_at(
    app: &App,
    lay: &Layout,
    p: Point,
    theme: &Theme,
) -> (Option<DropTarget>, Option<Rect>) {
    if lay.sidebar.contains(p) {
        let sidebar = lay.sidebar.deflate(Insets::splat(6.0));
        let Some(entry) = app.sidebar.index_at(sidebar, p, theme) else {
            return (None, None);
        };
        let action = app
            .sidebar_entry_action
            .get(entry)
            .copied()
            .flatten()
            .and_then(|ai| app.sidebar_actions.get(ai));
        let Some(SideAction::Go(path)) = action else {
            return (None, None);
        };
        let trash = trash::trash_root().join("files");
        let target = if path == &trash {
            DropTarget::Trash
        } else {
            DropTarget::Directory(path.clone())
        };
        let rect = app
            .sidebar
            .rows(sidebar, theme)
            .into_iter()
            .find(|row| row.entry == entry)
            .map(|row| row.rect);
        return (Some(target), rect);
    }

    let count = if app.split { 2 } else { 1 };
    for i in 0..count {
        if !lay.panes[i].contains(p) {
            continue;
        }
        let pr = pane_rects(app, lay, i, theme);
        let rows = file_rows(&app.panes[i], pr.items);
        if let Some(index) = hit_index(&app.panes[i], rows, p, theme) {
            if let Some(entry) = app.panes[i].dir.entry(index) {
                if !entry.is_dir {
                    return (None, None);
                }
                let slot = app.panes[i].view.slot_rect(rows, index, theme);
                return (Some(DropTarget::Directory(entry.path.clone())), Some(slot));
            }
            return (None, None);
        }
        if rows.contains(p) {
            // Upuszczenie na tło panelu = do jego bieżącego katalogu.
            let dir = app.panes[i].dir.path.clone();
            return (Some(DropTarget::Directory(dir)), Some(rows));
        }
    }
    (None, None)
}

/// Buduje zadanie dla upuszczenia; `copy` (Ctrl) decyduje o kopiowaniu.
fn drag_job(target: DropTarget, paths: Vec<PathBuf>, copy: bool) -> Option<Job> {
    match target {
        DropTarget::Trash => {
            let trash = trash::trash_root().join("files");
            let paths: Vec<PathBuf> = paths
                .into_iter()
                .filter(|path| !path.starts_with(&trash))
                .collect();
            (!paths.is_empty()).then_some(Job::Trash { paths })
        }
        DropTarget::Directory(dst) => {
            if !dst.is_dir() {
                return None;
            }
            let srcs: Vec<PathBuf> = paths
                .into_iter()
                .filter(|src| {
                    // Nie przenosimy katalogu do niego samego ani do jego wnętrza.
                    if dst == *src || dst.starts_with(src) {
                        return false;
                    }
                    // Przenoszenie do tego samego katalogu nic nie zmienia.
                    if !copy && src.parent() == Some(dst.as_path()) {
                        return false;
                    }
                    true
                })
                .collect();
            if srcs.is_empty() {
                return None;
            }
            Some(if copy {
                Job::Copy { srcs, dst }
            } else {
                Job::Move { srcs, dst }
            })
        }
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
    app.tooltip = None;
    app.drag = None;
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
                match app.sidebar_actions.get(ai) {
                    Some(SideAction::Go(path)) => {
                        if let Some(mi) = app
                            .mounts
                            .iter()
                            .position(|m| m.mounted && &m.mount == path)
                        {
                            open_menu(app, p, MenuTarget::Device(mi), size, theme, text);
                            return;
                        }
                    }
                    Some(SideAction::Mount(i)) | Some(SideAction::Unmount(i)) => {
                        open_menu(app, p, MenuTarget::Device(*i), size, theme, text);
                        return;
                    }
                    _ => {}
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

/// Środkowy przycisk myszy: otwiera folder (albo miejsce z panelu bocznego)
/// w nowej karcie — jak w Nautilusie. Dla plików nic nie robi.
fn on_middle_press(app: &mut App, p: Point, lay: &Layout, theme: &Theme) {
    app.tooltip = None;
    app.drag = None;
    let count = if app.split { 2 } else { 1 };
    for i in 0..count {
        if !lay.panes[i].contains(p) {
            continue;
        }
        let pr = pane_rects(app, lay, i, theme);
        let rows = file_rows(&app.panes[i], pr.items);
        if let Some(index) = hit_index(&app.panes[i], rows, p, theme) {
            if let Some(entry) = app.panes[i].dir.entry(index) {
                let path = entry.path.clone();
                if entry.is_dir {
                    app.new_tab_at(path);
                }
            }
        }
        return;
    }

    if lay.sidebar.contains(p) {
        let hovered = app
            .sidebar
            .index_at(lay.sidebar.deflate(Insets::splat(6.0)), p, theme);
        if let Some(entry) = hovered {
            if let Some(action) = app.sidebar_entry_action.get(entry).copied().flatten() {
                if let Some(SideAction::Go(path)) = app.sidebar_actions.get(action) {
                    let path = path.clone();
                    app.new_tab_at(path);
                }
            }
        }
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
    app.tooltip = None;
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
        let clipboard = {
            let pane = app.pane_mut();
            let edit = pane.edit.as_mut().unwrap();
            clipboard_command(&mut edit.entry, ke)
        };
        if let Some(cmd) = clipboard {
            apply_text_clipboard(app, cmd, TextPasteTarget::Edit);
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
        let clipboard = {
            let pane = app.pane_mut();
            clipboard_command(&mut pane.search, ke)
        };
        if let Some(cmd) = clipboard {
            let cut = matches!(cmd, TextClipboardCommand::Cut(_));
            apply_text_clipboard(app, cmd, TextPasteTarget::Search);
            if cut {
                refresh_search(app);
            }
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
            0x2c => app.settings_open = true,         // Ctrl+,
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

fn apply_text_clipboard(app: &mut App, cmd: TextClipboardCommand, target: TextPasteTarget) {
    match cmd {
        TextClipboardCommand::Copy(text) | TextClipboardCommand::Cut(text) => {
            if let Some(text) = text {
                app.text_clipboard = Some(text);
            }
        }
        TextClipboardCommand::Paste => {
            app.text_paste_target = Some(target);
            app.text_paste_requested = true;
        }
    }
}

fn refresh_search(app: &mut App) {
    let query = app.pane().search.text().to_owned();
    let pane = app.pane_mut();
    pane.dir.set_filter(query);
    pane.sync_view();
}

/// Wpisuje tekst z systemowego schowka do pola zapamiętanego przy Ctrl+V.
fn paste_text_into(app: &mut App, text: &str) {
    let Some(target) = app.text_paste_target.take() else {
        return;
    };
    match target {
        TextPasteTarget::Edit => {
            if let Some(edit) = app.pane_mut().edit.as_mut() {
                edit.entry.paste(text);
            }
        }
        TextPasteTarget::Search => {
            if app.pane().search_active {
                app.pane_mut().search.paste(text);
                refresh_search(app);
            }
        }
    }
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
            if let MenuTarget::Item(_, is_dir) = target {
                if !*is_dir {
                    m.action(tr("Open with…"), None, MenuCmd::OpenWith);
                }
            }
            m.action(tr("Preview"), Some("Space"), MenuCmd::TogglePreview);
            m.action(tr("Open in Terminal"), Some("F4"), MenuCmd::OpenTerminal);
            m.sep();
            m.action(tr("Compress to ZIP"), None, MenuCmd::Compress);
            if let MenuTarget::Item(path, is_dir) = target {
                if !*is_dir && archive::is_archive(path) {
                    m.action(tr("Extract Here"), None, MenuCmd::ExtractHere);
                    m.action(tr("Extract to Folder"), None, MenuCmd::ExtractTo);
                }
            }
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
            if app.mounts.get(*i).map(|m| m.mounted).unwrap_or(false) {
                m.action(tr("Open"), None, MenuCmd::OpenDevice(*i));
                m.action(tr("Unmount"), None, MenuCmd::Unmount(*i));
            } else {
                m.action(tr("Mount"), None, MenuCmd::Mount(*i));
            }
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
                if pane.preview {
                    tr("Hide Preview")
                } else {
                    tr("Preview")
                },
                Some("Space"),
                MenuCmd::TogglePreview,
            );
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
            if matches!(target, MenuTarget::AppMenu) {
                m.sep();
                m.action(tr("Settings"), Some("Ctrl+,"), MenuCmd::Settings);
                m.action(tr("About Files"), None, MenuCmd::About);
            }
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
                    app.open_file(&path);
                }
            }
        }
        MenuCmd::OpenWith => {
            if let Some((path, is_dir)) = target {
                if !is_dir {
                    app.show_open_with(path);
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
                app.show_properties(&path);
            }
        }
        MenuCmd::TogglePreview => {
            let pane = app.pane_mut();
            pane.preview = !pane.preview;
            pane.preview_cache = None;
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
                app.last_mount_refresh = Instant::now() - Duration::from_secs(10);
            }
        }
        MenuCmd::Mount(i) => {
            if let Some(m) = app.mounts.get(i) {
                places::mount_device(&m.dev);
                app.show_toast(libluna_i18n::tr("Mounting…"));
                app.last_mount_refresh = Instant::now() - Duration::from_secs(10);
            }
        }
        MenuCmd::OpenDevice(i) => {
            if let Some(m) = app.mounts.get(i) {
                if m.mounted {
                    let mount = m.mount.clone();
                    app.navigate(app.active, mount);
                }
            }
        }
        MenuCmd::Settings => app.settings_open = true,
        MenuCmd::About => app.show_about(),
        MenuCmd::Compress => {
            if let Some((path, _)) = target {
                // Kompresujemy całe zaznaczenie, jeśli kliknięty element jest jego częścią.
                let mut items = app.pane().selection();
                if !items.contains(&path) {
                    items = vec![path];
                }
                let dir = app.pane().dir.path.clone();
                app.compress(items, &dir);
            }
        }
        MenuCmd::ExtractHere | MenuCmd::ExtractTo => {
            if let Some((path, _)) = target {
                app.extract(path, matches!(cmd, MenuCmd::ExtractHere));
            }
        }
    }
}

fn set_sort(app: &mut App, key: SortKey) {
    let pane = app.pane_mut();
    pane.dir.set_sort_key(key);
    pane.sync_view();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(std::env::temp_dir())
    }

    #[test]
    fn pasted_text_goes_to_active_search_field() {
        let mut app = app();
        app.panes[0].search_active = true;
        app.panes[0].search.set_focused(true);
        app.text_paste_target = Some(TextPasteTarget::Search);
        paste_text_into(&mut app, "luna");
        assert_eq!(app.panes[0].search.text(), "luna");
        assert!(app.text_paste_target.is_none());
    }

    #[test]
    fn settings_dialog_renders_panel() {
        let mut app = app();
        app.settings_open = true;
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        let mut renderer = libluna_render::SoftwareRenderer::new(1120, 720);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let panel = settings_dialog().rects(size, &theme).panel;
        let px = |x: usize, y: usize| {
            let i = (y * 1120 + x) * 4;
            renderer.data()[i..i + 4].to_vec()
        };
        // Wnętrze panelu ma inne tło niź obszar poza modalem (przygaszenie + karta).
        assert_ne!(
            px(panel.center().x as usize, panel.center().y as usize),
            px(10, 700)
        );
    }

    #[test]
    #[ignore = "generuje podgląd do docs/images (uruchom ręcznie)"]
    fn render_files_preview() {
        let icons = libluna_core::IconSettings::load();
        libluna_core::icons::set_active_icon_set(icons.external);
        libluna_core::icons::set_active_icon_theme(icons.theme);
        let mut app = app();
        app.sidebar_visible = true;
        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        let mut renderer = libluna_render::SoftwareRenderer::new(1120, 720);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/images/luna-files.png");
        std::fs::write(path, png).expect("zapis PNG");
    }

    #[test]
    #[ignore = "generuje podgląd do docs/images (uruchom ręcznie)"]
    fn render_open_with_preview() {
        let icons = libluna_core::IconSettings::load();
        libluna_core::icons::set_active_icon_set(icons.external);
        libluna_core::icons::set_active_icon_theme(icons.theme);
        let mut app = app();
        app.show_open_with(PathBuf::from("/home/luna/Dokumenty/umowa-2026.raw"));
        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        let mut renderer = libluna_render::SoftwareRenderer::new(1120, 720);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/images/luna-files-open-with.png");
        std::fs::write(path, png).expect("zapis PNG");
    }

    #[test]
    fn window_controls_track_hover() {
        let mut app = app();
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme, &text);
        let close = HeaderBar::new()
            .controls(ControlsSide::Right)
            .controls_layout(lay.top, &theme)
            .close
            .unwrap();
        handle_events(
            &mut app,
            &[Event::PointerMotion {
                position: close.center(),
                modifiers: Modifiers::default(),
            }],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.controls_hover, Some(WindowControl::Close));
        assert!(app.pointer.is_some());

        handle_events(
            &mut app,
            &[Event::PointerMotion {
                position: Point::new(close.center().x, close.center().y + 300.0),
                modifiers: Modifiers::default(),
            }],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.controls_hover, None);
    }

    #[test]
    fn middle_click_opens_folder_in_new_tab() {
        let dir = std::env::temp_dir().join("luna-files-middle-click");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("podkatalog")).unwrap();
        let mut app = App::new(dir.clone());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme, &text);
        let pr = pane_rects(&app, &lay, 0, &theme);
        let rows = file_rows(&app.panes[0], pr.items);
        let row_h = theme.metrics.itemview.row_h;
        on_middle_press(
            &mut app,
            Point::new(rows.center().x, rows.y() + row_h / 2.0),
            &lay,
            &theme,
        );
        assert_eq!(app.pane().tabs.len(), 2);
        assert_eq!(app.pane().dir.path, dir.join("podkatalog"));
        assert_eq!(app.pane().tab, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn about_dialog_can_be_opened() {
        let mut app = app();
        assert!(app.about.is_none());
        app.show_about();
        assert!(app.about.is_some());
    }

    #[test]
    fn settings_controls_stay_inside_dialog() {
        let theme = Theme::light();
        let size = Size::new(1120.0, 720.0);
        let lay = settings_layout(size, &theme);
        let dialog = settings_dialog();
        let panel = dialog.rects(size, &theme).panel;
        for row in lay.rows {
            assert!(panel.contains(row.center()), "wiersz poza panelem");
        }
        assert!(panel.contains(lay.theme_seg.center()));
        assert!(panel.contains(lay.view_seg.center()));
        assert!(panel.contains(lay.lang_seg.center()));
        assert!(panel.contains(lay.hidden_switch.center()));
        assert!(panel.contains(lay.done.center()));
        // Kontrolki nie nachodzą na przycisk zamknięcia w nagłówku.
        let close = dialog.rects(size, &theme).close;
        assert!(!lay.theme_seg.intersects(&close));
    }

    #[test]
    fn app_menu_has_settings_and_about_at_the_end() {
        let app = app();
        let (_, cmds) = menu_entries(&app, &MenuTarget::AppMenu);
        let tail: Vec<MenuCmd> = cmds.iter().rev().filter_map(|c| *c).take(2).collect();
        assert_eq!(tail.len(), 2);
        assert!(matches!(tail[0], MenuCmd::About));
        assert!(matches!(tail[1], MenuCmd::Settings));
    }

    #[test]
    fn archive_item_menu_offers_extract_actions() {
        let app = app();
        let dir = std::env::temp_dir();
        let (_, cmds) = menu_entries(&app, &MenuTarget::Item(dir.join("paczka.zip"), false));
        let has = |needle: fn(&MenuCmd) -> bool| cmds.iter().flatten().any(needle);
        assert!(has(|c| matches!(c, MenuCmd::Compress)));
        assert!(has(|c| matches!(c, MenuCmd::ExtractHere)));
        assert!(has(|c| matches!(c, MenuCmd::ExtractTo)));
    }

    #[test]
    fn regular_item_menu_has_no_extract_actions() {
        let app = app();
        let dir = std::env::temp_dir();
        let (_, cmds) = menu_entries(&app, &MenuTarget::Item(dir.join("notatka.txt"), false));
        assert!(cmds
            .iter()
            .flatten()
            .all(|c| !matches!(c, MenuCmd::ExtractHere | MenuCmd::ExtractTo)));
        assert!(cmds
            .iter()
            .flatten()
            .any(|c| matches!(c, MenuCmd::Compress)));
        // Katalog (nawet o nazwie „.zip”) nie jest archiwum w menu.
        let (_, cmds) = menu_entries(&app, &MenuTarget::Item(dir.join("folder.zip"), true));
        assert!(cmds
            .iter()
            .flatten()
            .all(|c| !matches!(c, MenuCmd::ExtractHere | MenuCmd::ExtractTo)));
    }

    #[test]
    fn item_menu_offers_preview() {
        let app = app();
        let dir = std::env::temp_dir();
        let (_, cmds) = menu_entries(&app, &MenuTarget::Item(dir.join("foto.jpg"), false));
        assert!(cmds
            .iter()
            .flatten()
            .any(|c| matches!(c, MenuCmd::TogglePreview)));
    }

    fn usb_mount() -> crate::places::Mount {
        crate::places::Mount {
            dev: "/dev/sdz1".to_owned(),
            label: "PENDRIVE".to_owned(),
            mount: std::path::PathBuf::new(),
            fs: "vfat".to_owned(),
            total: 0,
            free: 0,
            size: 16_000_000_000,
            mounted: false,
        }
    }

    #[test]
    fn unmounted_device_is_listed_and_can_be_mounted() {
        let mut app = app();
        app.mounts = vec![usb_mount()];
        app.rebuild_sidebar();
        assert!(
            app.sidebar_actions
                .iter()
                .any(|a| matches!(a, SideAction::Mount(0))),
            "niezamontowany nośnik powinien wywoływać Mount"
        );
        let (_, cmds) = menu_entries(&app, &MenuTarget::Device(0));
        assert!(cmds
            .iter()
            .flatten()
            .any(|c| matches!(c, MenuCmd::Mount(0))));
        assert!(cmds
            .iter()
            .flatten()
            .all(|c| !matches!(c, MenuCmd::OpenDevice(0))));
    }

    #[test]
    fn drag_job_handles_move_copy_self_and_trash() {
        let root = std::env::temp_dir().join("luna-files-drag-job-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("podkatalog")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        let file = root.join("a.txt");

        let moved = drag_job(
            DropTarget::Directory(root.join("podkatalog")),
            vec![file.clone()],
            false,
        );
        assert!(matches!(moved, Some(Job::Move { .. })));

        // Przenoszenie do tego samego katalogu = brak akcji; Ctrl kopiuje.
        assert!(drag_job(
            DropTarget::Directory(root.clone()),
            vec![file.clone()],
            false
        )
        .is_none());
        assert!(matches!(
            drag_job(
                DropTarget::Directory(root.clone()),
                vec![file.clone()],
                true
            ),
            Some(Job::Copy { .. })
        ));

        // Katalog nie może trafić sam do siebie.
        assert!(drag_job(
            DropTarget::Directory(root.join("podkatalog")),
            vec![root.join("podkatalog")],
            false
        )
        .is_none());

        assert!(matches!(
            drag_job(DropTarget::Trash, vec![file], false),
            Some(Job::Trash { .. })
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mouse_drag_selects_folder_as_drop_target() {
        let root = std::env::temp_dir().join("luna-files-drag-e2e-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("podkatalog")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        let mut app = App::new(root.clone());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme, &text);
        let pr = pane_rects(&app, &lay, 0, &theme);
        let rows = file_rows(&app.panes[0], pr.items);
        let entries = app.panes[0].dir.visible_entries();
        let file_index = entries.iter().position(|e| e.name == "a.txt").unwrap();
        let dir_index = entries.iter().position(|e| e.name == "podkatalog").unwrap();
        let file_pos = app.panes[0]
            .view
            .slot_rect(rows, file_index, &theme)
            .center();
        let dir_pos = app.panes[0]
            .view
            .slot_rect(rows, dir_index, &theme)
            .center();
        let press = Event::PointerButton(libluna_core::PointerEvent {
            position: file_pos,
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::default(),
        });
        let away = Event::PointerMotion {
            position: Point::new(file_pos.x + 12.0, file_pos.y + 6.0),
            modifiers: Modifiers::default(),
        };
        let over = Event::PointerMotion {
            position: dir_pos,
            modifiers: Modifiers::default(),
        };
        handle_events(&mut app, &[press, away, over], size, &theme, &text);
        let drag = app.drag.as_ref().expect("przeciąganie powinno być aktywne");
        assert!(drag.active);
        assert_eq!(
            drag.target,
            Some(DropTarget::Directory(root.join("podkatalog")))
        );
        // Sprzątanie bez wykonywania zadania.
        app.drag = None;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dropping_file_on_folder_moves_it() {
        let root = std::env::temp_dir().join("luna-files-drag-drop-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("podkatalog")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        let mut app = App::new(root.clone());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = layout(&app, size, &theme, &text);
        let pr = pane_rects(&app, &lay, 0, &theme);
        let rows = file_rows(&app.panes[0], pr.items);
        let entries = app.panes[0].dir.visible_entries();
        let file_index = entries.iter().position(|e| e.name == "a.txt").unwrap();
        let dir_index = entries.iter().position(|e| e.name == "podkatalog").unwrap();
        let file_pos = app.panes[0]
            .view
            .slot_rect(rows, file_index, &theme)
            .center();
        let dir_pos = app.panes[0]
            .view
            .slot_rect(rows, dir_index, &theme)
            .center();
        let none = Modifiers::default();
        handle_events(
            &mut app,
            &[
                Event::PointerButton(libluna_core::PointerEvent {
                    position: file_pos,
                    button: PointerButton::Left,
                    state: ButtonState::Pressed,
                    modifiers: none,
                }),
                Event::PointerMotion {
                    position: Point::new(file_pos.x + 12.0, file_pos.y + 6.0),
                    modifiers: none,
                },
                Event::PointerMotion {
                    position: dir_pos,
                    modifiers: none,
                },
                Event::PointerButton(libluna_core::PointerEvent {
                    position: dir_pos,
                    button: PointerButton::Left,
                    state: ButtonState::Released,
                    modifiers: none,
                }),
            ],
            size,
            &theme,
            &text,
        );
        let target = root.join("podkatalog/a.txt");
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && !target.exists() {
            std::thread::sleep(Duration::from_millis(10));
            app.drain_ops();
        }
        assert!(target.exists(), "plik powinien zostać przeniesiony");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn open_with_demo() -> OpenWithDialog {
        let apps = vec![
            open_with::parse_desktop_entry(
                "[Desktop Entry]\nType=Application\nName=Editor\nExec=sh %U\nIcon=org.gnome.gedit\nMimeType=text/plain;\n",
                "gedit.desktop",
            )
            .unwrap(),
            open_with::parse_desktop_entry(
                "[Desktop Entry]\nType=Application\nName=Inny program\nExec=sh %f\n",
                "inny.desktop",
            )
            .unwrap(),
        ];
        OpenWithDialog::new(
            PathBuf::from("/tmp/notatka.txt"),
            Some("text/plain".to_owned()),
            Some("gedit.desktop".to_owned()),
            apps,
        )
    }

    fn key_event(keysym: u32) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: Modifiers::default(),
            repeat: false,
        })
    }

    #[test]
    fn open_with_dialog_renders_panel_and_fits_inside() {
        let mut app = app();
        app.open_with = Some(open_with_demo());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let ow = app.open_with.as_ref().unwrap();
        let lay = open_with_layout(ow, size, &theme, &text);
        let panel = open_with_dialog(ow.apps.len(), ow.can_remember(), &theme)
            .rects(size, &theme)
            .panel;
        for rect in [lay.name, lay.list, lay.remember, lay.open, lay.cancel] {
            assert!(panel.contains(rect.center()), "kontrolka poza panelem");
        }
        let mut scene = Scene::new();
        paint(&app, &mut scene, size, &theme, &text);
        let mut renderer = libluna_render::SoftwareRenderer::new(1120, 720);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let px = |x: usize, y: usize| {
            let i = (y * 1120 + x) * 4;
            renderer.data()[i..i + 4].to_vec()
        };
        assert_ne!(
            px(panel.center().x as usize, panel.center().y as usize),
            px(10, 700)
        );
    }

    #[test]
    fn file_menu_offers_open_with_but_folder_menu_does_not() {
        let app = app();
        let dir = std::env::temp_dir();
        let (_, file_cmds) = menu_entries(&app, &MenuTarget::Item(dir.join("notatka.txt"), false));
        assert!(file_cmds
            .iter()
            .flatten()
            .any(|cmd| matches!(cmd, MenuCmd::OpenWith)));
        let (_, dir_cmds) = menu_entries(&app, &MenuTarget::Item(dir, true));
        assert!(dir_cmds
            .iter()
            .flatten()
            .all(|cmd| !matches!(cmd, MenuCmd::OpenWith)));
    }

    #[test]
    fn open_with_dialog_closes_on_escape() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);

        let mut app = app();
        app.open_with = Some(open_with_demo());
        handle_events(&mut app, &[key_event(0xff1b)], size, &theme, &text);
        assert!(app.open_with.is_none());
    }

    #[test]
    fn open_with_dialog_closes_on_cancel_click() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let mut app = app();
        app.open_with = Some(open_with_demo());
        let lay = open_with_layout(app.open_with.as_ref().unwrap(), size, &theme, &text);
        handle_events(
            &mut app,
            &[Event::PointerButton(libluna_core::PointerEvent {
                position: lay.cancel.center(),
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                modifiers: Modifiers::default(),
            })],
            size,
            &theme,
            &text,
        );
        assert!(app.open_with.is_none());
    }

    #[test]
    fn open_with_dialog_click_selects_row() {
        let mut app = app();
        app.open_with = Some(open_with_demo());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = open_with_layout(app.open_with.as_ref().unwrap(), size, &theme, &text);
        let second = Point::new(lay.list.x() + 20.0, lay.list.y() + OPEN_ROW_H * 1.5);
        handle_events(
            &mut app,
            &[Event::PointerButton(libluna_core::PointerEvent {
                position: second,
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                modifiers: Modifiers::default(),
            })],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.open_with.as_ref().unwrap().selected, Some(1));
    }

    #[test]
    fn open_with_remember_switch_is_clickable() {
        let mut app = app();
        app.open_with = Some(open_with_demo());
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1120.0, 720.0);
        let lay = open_with_layout(app.open_with.as_ref().unwrap(), size, &theme, &text);
        let before = app.open_with.as_ref().unwrap().remember;
        handle_events(
            &mut app,
            &[Event::PointerButton(libluna_core::PointerEvent {
                position: lay.remember.center(),
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                modifiers: Modifiers::default(),
            })],
            size,
            &theme,
            &text,
        );
        assert_ne!(app.open_with.as_ref().unwrap().remember, before);
    }
}
