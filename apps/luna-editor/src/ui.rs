//! Rysowanie i obsługa wejścia Luna Editor.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, IconTheme, Insets, KeyEvent, Point, PointerButton, Rect, Size,
    Theme, WindowEvent,
};
use libluna_i18n::tr;
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    clipboard_command, hover_fill, paint_icon, paint_icon_themed, sidebar_icon, Button,
    ButtonVariant, ControlsSide, HeaderBar, IconName, PaintCtx, PreferencesDialog, Switch,
    TextClipboardCommand, ViewSwitcher, WindowControl,
};

use crate::app::{App, PromptKind};
use crate::browser::{BrowserMode, FileBrowser};
use crate::prefs::TAB_WIDTHS;
use crate::syntax::Language;

const HEADER_BTN: f64 = 30.0;
const STATUS_H: f64 = 26.0;
const FIND_H: f64 = 40.0;
const SCROLL_STEP: f64 = 64.0;
const WORKSPACE_W: f64 = 236.0;
const WORKSPACE_ROW_H: f64 = 26.0;
const PREVIEW_H: f64 = 168.0;
const BROWSER_ROW_H: f64 = 28.0;

/// Geometria okna.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub header: Rect,
    pub workspace_toggle: Rect,
    pub tabs: Rect,
    pub menu: Rect,
    pub workspace: Rect,
    pub workspace_root: Rect,
    pub workspace_folder: Rect,
    pub workspace_tree: Rect,
    pub workspace_preview: Rect,
    pub find_bar: Rect,
    pub find_entry: Rect,
    pub find_close: Rect,
    pub editor: Rect,
    pub status: Rect,
    /// Panel czatu AI (pusty prostokąt, gdy panel jest schowany).
    pub chat: Rect,
    pub chat_header: Rect,
    pub chat_messages: Rect,
    pub chat_input: Rect,
    pub chat_send: Rect,
    pub chat_close: Rect,
    pub chat_clear: Rect,
    pub chat_gear: Rect,
    /// Pasek selektora modelu (pod nagłówkiem panelu).
    pub chat_model: Rect,
    /// Przycisk czatu w nagłówku (0×0, gdy nie ma wtyczki AI).
    pub chat_ai: Rect,
}

pub fn layout(
    size: Size,
    theme: &Theme,
    workspace_visible: bool,
    find_open: bool,
    chat_visible: bool,
    chat_button: bool,
) -> Layout {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let pad = theme.spacing.md.min((size.width * 0.04).max(8.0));
    let status = Rect::from_xywh(0.0, size.height - STATUS_H, size.width, STATUS_H);
    let header_content = HeaderBar::new()
        .controls(ControlsSide::Right)
        .controls_layout(header, theme)
        .content;
    let workspace_toggle = Rect::from_xywh(
        header_content.x(),
        header.center().y - HEADER_BTN / 2.0,
        HEADER_BTN,
        HEADER_BTN,
    );
    let menu = Rect::from_xywh(
        header_content.right() - HEADER_BTN,
        header.center().y - HEADER_BTN / 2.0,
        HEADER_BTN,
        HEADER_BTN,
    );
    let chat_ai = if chat_button {
        Rect::from_xywh(
            menu.x() - 4.0 - HEADER_BTN,
            menu.y(),
            HEADER_BTN,
            HEADER_BTN,
        )
    } else {
        Rect::from_xywh(menu.x(), menu.y(), 0.0, 0.0)
    };
    let tabs_right = if chat_button {
        chat_ai.x() - 4.0
    } else {
        menu.x() - 8.0
    };
    let tabs = Rect::from_xywh(
        workspace_toggle.right() + 6.0,
        header.center().y - theme.metrics.tabs.height / 2.0,
        (tabs_right - (workspace_toggle.right() + 6.0)).max(40.0),
        theme.metrics.tabs.height,
    );

    let workspace_w = if workspace_visible {
        WORKSPACE_W.min((size.width * 0.30).max(170.0))
    } else {
        0.0
    };
    let workspace = Rect::from_xywh(
        0.0,
        header.bottom(),
        workspace_w,
        (status.y() - header.bottom()).max(0.0),
    );
    let workspace_root = Rect::from_xywh(
        workspace.x() + pad,
        workspace.y() + 8.0,
        (workspace.width() - 2.0 * pad - HEADER_BTN - 6.0).max(0.0),
        26.0,
    );
    let folder_button = Rect::from_xywh(
        workspace.right() - pad - HEADER_BTN,
        workspace.y() + 8.0,
        HEADER_BTN,
        26.0,
    );
    let workspace_preview = Rect::from_xywh(
        workspace.x(),
        (workspace.bottom() - PREVIEW_H).max(workspace.y() + 42.0),
        workspace.width(),
        PREVIEW_H,
    );
    let workspace_tree = Rect::from_xywh(
        workspace.x(),
        workspace_root.bottom() + 6.0,
        workspace.width(),
        (workspace_preview.y() - 6.0 - (workspace_root.bottom() + 6.0)).max(0.0),
    );

    let editor_left = if workspace_visible {
        workspace.right() + pad
    } else {
        pad
    };
    let chat_w = if chat_visible {
        crate::chat::panel_width(size)
    } else {
        0.0
    };
    let content_right = size.width - pad - if chat_visible { chat_w + pad } else { 0.0 };
    let find_bar = Rect::from_xywh(
        editor_left,
        header.bottom() + pad,
        (content_right - editor_left).max(0.0),
        if find_open { FIND_H } else { 0.0 },
    );
    let find_entry = Rect::from_xywh(
        find_bar.x(),
        find_bar.y() + 4.0,
        (find_bar.width() - 34.0 - 90.0).max(40.0),
        (FIND_H - 8.0).max(20.0),
    );
    let find_close = Rect::from_xywh(
        find_bar.right() - 28.0,
        find_bar.center().y - 14.0,
        28.0,
        28.0,
    );

    let editor_top = if find_open {
        find_bar.bottom() + pad
    } else {
        header.bottom() + pad
    };
    let editor = Rect::from_xywh(
        editor_left,
        editor_top,
        (content_right - editor_left).max(0.0),
        (status.y() - pad - editor_top).max(0.0),
    );

    let chat = Rect::from_xywh(
        size.width - chat_w,
        header.bottom(),
        chat_w,
        (status.y() - header.bottom()).max(0.0),
    );
    let chat_pad = crate::chat::padding();
    let chat_close = Rect::from_xywh(chat.right() - chat_pad - 26.0, chat.y() + 6.0, 26.0, 26.0);
    let chat_gear = Rect::from_xywh(chat_close.x() - 4.0 - 26.0, chat.y() + 6.0, 26.0, 26.0);
    let chat_clear = Rect::from_xywh(chat_gear.x() - 6.0 - 58.0, chat.y() + 6.0, 58.0, 26.0);
    let chat_header = Rect::from_xywh(
        chat.x() + chat_pad,
        chat.y() + 6.0,
        (chat_clear.x() - 6.0 - (chat.x() + chat_pad)).max(0.0),
        26.0,
    );
    let chat_model = Rect::from_xywh(
        chat.x() + chat_pad,
        chat.y() + 38.0,
        (chat.width() - 2.0 * chat_pad).max(0.0),
        26.0,
    );
    let input_h = crate::chat::input_height();
    let chat_send = Rect::from_xywh(
        chat.right() - chat_pad - 74.0,
        chat.bottom() - chat_pad - input_h,
        74.0,
        input_h,
    );
    let chat_input = Rect::from_xywh(
        chat.x() + chat_pad,
        chat_send.y(),
        (chat_send.x() - 8.0 - (chat.x() + chat_pad)).max(0.0),
        input_h,
    );
    let chat_messages = Rect::from_xywh(
        chat.x(),
        chat_model.bottom() + 6.0,
        chat.width(),
        (chat_input.y() - 26.0 - (chat_model.bottom() + 6.0)).max(0.0),
    );

    Layout {
        header,
        workspace_toggle,
        tabs,
        menu,
        workspace,
        workspace_root,
        workspace_folder: folder_button,
        workspace_tree,
        workspace_preview,
        find_bar,
        find_entry,
        find_close,
        editor,
        status,
        chat,
        chat_header,
        chat_messages,
        chat_input,
        chat_send,
        chat_close,
        chat_clear,
        chat_gear,
        chat_model,
        chat_ai,
    }
}

pub fn paint(app: &mut App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let chat_button = crate::chat::available(app);
    let lay = layout(
        size,
        theme,
        app.workspace.visible,
        app.find.is_some(),
        app.chat.visible && chat_button,
        chat_button,
    );
    let mut ctx = PaintCtx::new(scene, theme, text);
    let colors = theme.colors;
    ctx.scene.fill_rect(Rect::from_size(size), colors.window_bg);

    HeaderBar::new()
        .controls(ControlsSide::Right)
        .paint(&mut ctx, lay.header, app.hover_control);

    paint_header_button(
        &mut ctx,
        lay.workspace_toggle,
        app.hover_workspace_toggle,
        false,
    );
    sidebar_icon(
        &mut ctx,
        colors.text,
        lay.workspace_toggle,
        app.workspace.visible,
    );

    app.tabs.paint(&mut ctx, lay.tabs);

    paint_header_button(&mut ctx, lay.menu, app.hover_menu, app.menu_open);
    paint_icon(
        &mut ctx,
        IconName::Menu,
        lay.menu.deflate(Insets::splat(6.0)),
        colors.text,
    );

    if lay.chat_ai.width() > 0.0 {
        paint_header_button(
            &mut ctx,
            lay.chat_ai,
            app.hover_chat_toggle,
            app.chat.visible,
        );
        paint_icon(
            &mut ctx,
            IconName::Chat,
            lay.chat_ai.deflate(Insets::splat(7.0)),
            if app.chat.visible {
                colors.accent
            } else {
                colors.text
            },
        );
    }

    if app.workspace.visible {
        paint_workspace(app, &mut ctx, &lay);
    }
    if app.find.is_some() {
        paint_find_bar(app, &mut ctx, &lay);
    }
    app.view_mut().paint(&mut ctx, lay.editor);
    paint_status(app, &mut ctx, &lay);
    crate::chat::paint(app, &mut ctx, &lay);

    if app.menu_open {
        if let Some(panel) = menu_panel(app, &lay, size, theme, text) {
            app.menu.paint(&mut ctx, panel);
        }
    }
    if let Some(tree) = &app.tree_menu {
        let width = tree.menu.width(text, theme);
        let panel = tree
            .menu
            .place(tree.anchor, width, Rect::from_size(size), theme);
        tree.menu.paint(&mut ctx, panel);
    }
    if app.browser.is_some() {
        paint_browser(app, &mut ctx, size);
    }
    if app.prompt.is_some() {
        paint_prompt(app, &mut ctx, size);
    }
    if app.goto.is_some() {
        paint_goto(app, &mut ctx, size);
    }
    if app.settings_open {
        paint_settings(app, &mut ctx, size);
    }
    crate::plugin_ui::paint(app, &mut ctx, size);
    if let Some(confirm) = &app.confirm {
        confirm.dialog.paint(&mut ctx, size);
    }
    if let Some((_, dialog)) = &app.pending_delete {
        dialog.paint(&mut ctx, size);
    }
    if let Some((_, dialog)) = &app.overwrite {
        dialog.paint(&mut ctx, size);
    }
    if let Some(error) = &app.error {
        error.paint(&mut ctx, size);
    }
}

fn paint_header_button(ctx: &mut PaintCtx, rect: Rect, hover: bool, active: bool) {
    if hover || active {
        ctx.scene
            .fill_rounded_rect(rect, ctx.theme.radius.small, ctx.theme.colors.shade);
    }
}

fn paint_workspace(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    ctx.scene.fill_rect(lay.workspace, colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            lay.workspace.right() - 1.0,
            lay.workspace.y(),
            1.0,
            lay.workspace.height(),
        ),
        colors.border,
    );

    let root = elide(
        ctx,
        &app.workspace.root_name(),
        lay.workspace_root.width() - 12.0,
        theme.typography.body.size,
    );
    ctx.label_left(
        lay.workspace_root.x(),
        lay.workspace_root.center().y,
        &root,
        theme.typography.body.size,
        weight::MEDIUM,
        colors.text,
    );
    paint_header_button(ctx, lay.workspace_folder, false, false);
    paint_icon_themed(
        ctx,
        IconName::Files,
        lay.workspace_folder.deflate(Insets::splat(7.0)),
        colors.dim_text,
        IconTheme::Monochrome,
    );

    let row_h = WORKSPACE_ROW_H;
    ctx.scene.push_clip(lay.workspace_tree);
    let offset = app.workspace.scroll;
    for (index, row) in app.workspace.rows.iter().enumerate() {
        let y = lay.workspace_tree.y() + index as f64 * row_h - offset;
        if y + row_h < lay.workspace_tree.y() || y > lay.workspace_tree.bottom() {
            continue;
        }
        let rect = row_rect(lay, offset, index);
        if app.hover_workspace_row == Some(index) {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, colors.shade);
        }
        if app.workspace.selected.as_ref() == Some(&row.path) {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, colors.accent.scale_alpha(0.16));
        }
        let x = rect.x() + 6.0 + row.depth as f64 * 12.0;
        let icon = if row.is_dir {
            IconName::Files
        } else {
            file_icon(&row.name)
        };
        paint_icon_themed(
            ctx,
            icon,
            Rect::from_xywh(x, y + 5.0, 16.0, 16.0),
            if row.is_dir {
                colors.accent
            } else {
                colors.dim_text
            },
            IconTheme::Monochrome,
        );
        let name = elide(
            ctx,
            &row.name,
            (rect.right() - x - 26.0).max(20.0),
            theme.typography.caption.size,
        );
        ctx.label_left(
            x + 22.0,
            y + row_h / 2.0,
            &name,
            theme.typography.caption.size,
            if row.is_dir {
                weight::MEDIUM
            } else {
                weight::NORMAL
            },
            if row.is_dir {
                colors.text
            } else {
                colors.dim_text
            },
        );
    }
    ctx.scene.pop_clip();

    paint_preview(app, ctx, lay);
}

fn row_rect(lay: &Layout, scroll: f64, index: usize) -> Rect {
    let y = lay.workspace_tree.y() + index as f64 * WORKSPACE_ROW_H - scroll;
    Rect::from_xywh(
        lay.workspace_tree.x() + 4.0,
        y,
        (lay.workspace_tree.width() - 8.0).max(0.0),
        WORKSPACE_ROW_H,
    )
}

/// Prostokąt wiersza drzewa — używany też przez podgląd w `editor-render`.
pub fn workspace_row_anchor(app: &App, lay: &Layout, index: usize) -> Option<Rect> {
    (index < app.workspace.rows.len()).then(|| row_rect(lay, app.workspace.scroll, index))
}

fn paint_preview(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let card = lay.workspace_preview;
    // Podgląd jest częścią panelu bocznego: bez własnego tła i ramki, tylko
    // delikatny separator — żadnej „wysepki” na tle sidebaru.
    ctx.scene.fill_rect(
        Rect::from_xywh(card.x(), card.y(), card.width(), 1.0),
        colors.border,
    );
    let Some(preview) = &app.workspace.preview else {
        ctx.label_center(
            card.center().x,
            card.center().y,
            &tr("Select a file to preview"),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        return;
    };
    let x = card.x() + 10.0;
    let mut y = card.y() + 16.0;
    let name = elide(
        ctx,
        &preview.name,
        card.width() - 20.0,
        theme.typography.body.size,
    );
    ctx.label_left(
        x,
        y,
        &name,
        theme.typography.body.size,
        weight::MEDIUM,
        colors.text,
    );
    y += 18.0;
    let meta = if preview.is_dir {
        tr("Folder")
    } else {
        format!("{} · {}", preview.size_label(), preview.language.name())
    };
    ctx.label_left(
        x,
        y,
        &meta,
        theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    y += 16.0;
    if let Some(error) = &preview.error {
        ctx.label_left(
            x,
            y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.warning,
        );
    } else {
        for line in &preview.lines {
            let line = elide(
                ctx,
                line,
                card.width() - 20.0,
                theme.typography.caption.size,
            );
            ctx.label_left(
                x,
                y,
                &line,
                theme.typography.caption.size,
                weight::NORMAL,
                colors.dim_text,
            );
            y += theme.typography.caption.line_height;
            if y > card.bottom() - 34.0 {
                break;
            }
        }
    }
    if !preview.is_dir {
        let button = Rect::from_xywh(card.right() - 86.0, card.bottom() - 34.0, 76.0, 26.0);
        let mut action = Button::new(tr("Open"));
        action.set_hovered(app.hover_workspace_open);
        action.paint(ctx, button);
    }
}

fn paint_find_bar(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    // Samo pole wyszukiwania jest kontrolką (zaokrągloną) — pasek nie ma
    // własnego tła, więc nie tworzy dodatkowej wysepki nad edytorem.
    if let Some(find) = &app.find {
        find.entry.paint(ctx, lay.find_entry);
        let count = if find.matches.is_empty() {
            if find.entry.is_empty() {
                String::new()
            } else {
                tr("No matches")
            }
        } else {
            format!("{}/{}", find.index + 1, find.matches.len())
        };
        if !count.is_empty() {
            ctx.label_right(
                lay.find_close.x() - 10.0,
                lay.find_bar.center().y,
                &count,
                theme.typography.caption.size,
                weight::NORMAL,
                colors.dim_text,
            );
        }
    }
    paint_header_button(ctx, lay.find_close, false, false);
    paint_icon(
        ctx,
        IconName::Close,
        lay.find_close.deflate(Insets::splat(8.0)),
        colors.dim_text,
    );
}

fn paint_status(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let caption = theme.typography.caption;
    let pad = theme.spacing.md.min((lay.status.width() * 0.04).max(8.0));
    // Płaski pasek statusu: pełna szerokość, oddzielony samą linią.
    ctx.scene.fill_rect(
        Rect::from_xywh(lay.status.x(), lay.status.y(), lay.status.width(), 1.0),
        colors.border,
    );
    let mut left = app.file_label();
    if app.dirty() {
        left.push_str("  ●");
    }
    if let Some(status) = &app.status {
        left.push_str("  ·  ");
        left.push_str(status);
    }
    let left = elide(
        ctx,
        &left,
        (lay.status.width() - 2.0 * pad) * 0.55,
        caption.size,
    );
    ctx.label_left(
        lay.status.x() + pad,
        lay.status.center().y,
        &left,
        caption.size,
        weight::NORMAL,
        colors.dim_text,
    );

    let (line, column) = app.view().line_col();
    let position = format!("{} {}, {} {}", tr("Ln"), line, tr("Col"), column);
    let mut right = lay.status.right() - pad;
    for value in ["UTF-8", app.line_ending().label()] {
        let width = ctx.measure(value, caption.size, weight::NORMAL);
        ctx.label_right(
            right,
            lay.status.center().y,
            value,
            caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        right -= width + 16.0;
    }
    let width = ctx.measure(&position, caption.size, weight::NORMAL);
    ctx.label_right(
        right,
        lay.status.center().y,
        &position,
        caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    right -= width + 20.0;
    ctx.label_right(
        right,
        lay.status.center().y,
        app.language().name(),
        caption.size,
        weight::NORMAL,
        colors.text,
    );
}

// ── Wbudowany manager plików ───────────────────────────────────────────────

struct BrowserLayout {
    card: Rect,
    close: Rect,
    up: Rect,
    path: Rect,
    places: Rect,
    list: Rect,
    filename: Rect,
    error: Rect,
    cancel: Rect,
    action: Rect,
}

fn browser_layout(size: Size) -> BrowserLayout {
    let width = (size.width - 80.0).clamp(520.0, 840.0);
    let height = (size.height - 70.0).clamp(360.0, 560.0);
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let pad = 14.0;
    let close = Rect::from_xywh(card.right() - 34.0, card.y() + 10.0, 24.0, 24.0);
    let up = Rect::from_xywh(card.x() + pad, card.y() + 44.0, 34.0, 26.0);
    let path = Rect::from_xywh(
        up.right() + 8.0,
        card.y() + 46.0,
        (card.right() - pad - (up.right() + 8.0)).max(40.0),
        20.0,
    );
    let filename_h = 34.0;
    let action_w = 136.0;
    let filename = Rect::from_xywh(
        card.x() + pad,
        card.bottom() - 14.0 - filename_h,
        (card.width() - 2.0 * pad - action_w - 104.0).max(100.0),
        filename_h,
    );
    let error = Rect::from_xywh(
        filename.x(),
        filename.y() - 18.0,
        card.width() - 2.0 * pad,
        16.0,
    );
    let action = Rect::from_xywh(
        card.right() - pad - action_w,
        filename.y(),
        action_w,
        filename_h,
    );
    let cancel = Rect::from_xywh(action.x() - 8.0 - 96.0, action.y(), 96.0, filename_h);
    let places = Rect::from_xywh(
        card.x() + pad,
        up.bottom() + 10.0,
        (card.width() * 0.24).clamp(120.0, 170.0),
        (filename.y() - 26.0 - (up.bottom() + 10.0)).max(0.0),
    );
    let list = Rect::from_xywh(
        places.right() + 10.0,
        places.y(),
        (card.right() - pad - (places.right() + 10.0)).max(0.0),
        places.height(),
    );
    BrowserLayout {
        card,
        close,
        up,
        path,
        places,
        list,
        filename,
        error,
        cancel,
        action,
    }
}

fn paint_browser(app: &App, ctx: &mut PaintCtx, size: Size) {
    let Some(browser) = &app.browser else {
        return;
    };
    let theme = *ctx.theme;
    let colors = theme.colors;
    let lay = browser_layout(size);
    let pointer = app.pointer;
    let hovered = |rect: Rect| pointer.is_some_and(|p| rect.contains(p));
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, colors.border);
    ctx.label_left(
        lay.card.x() + 18.0,
        lay.card.y() + 24.0,
        &browser.title(),
        theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    paint_header_button(ctx, lay.close, hovered(lay.close), false);
    paint_icon(
        ctx,
        IconName::Close,
        lay.close.deflate(Insets::splat(5.0)),
        colors.dim_text,
    );

    paint_header_button(ctx, lay.up, hovered(lay.up), false);
    paint_icon(
        ctx,
        IconName::ChevronLeft,
        lay.up.deflate(Insets::splat(8.0)),
        colors.dim_text,
    );
    let dir = elide(
        ctx,
        &browser.dir.display().to_string(),
        lay.path.width(),
        theme.typography.caption.size,
    );
    ctx.label_left(
        lay.path.x(),
        lay.path.center().y,
        &dir,
        theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );

    ctx.scene.fill_rect(lay.list, colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(lay.list, theme.radius.small, 1.0, colors.border);

    // Miejsca XDG.
    let mut y = lay.places.y();
    for (name, path) in crate::browser::places() {
        let rect = Rect::from_xywh(lay.places.x(), y, lay.places.width(), 26.0);
        if browser.dir == path {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, colors.shade);
        } else if hovered(rect) {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, hover_fill(&theme));
        }
        ctx.label_left(
            rect.x() + 4.0,
            rect.center().y,
            &name,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.text,
        );
        y += 28.0;
    }

    // Lista plików.
    ctx.scene.push_clip(lay.list);
    let offset = browser.scroll;
    for (index, entry) in browser.entries.iter().enumerate() {
        let row_y = lay.list.y() + 2.0 + index as f64 * BROWSER_ROW_H - offset;
        if row_y + BROWSER_ROW_H < lay.list.y() || row_y > lay.list.bottom() {
            continue;
        }
        let rect = Rect::from_xywh(
            lay.list.x() + 2.0,
            row_y,
            lay.list.width() - 4.0,
            BROWSER_ROW_H,
        );
        if browser.selected == Some(index) {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, colors.accent.scale_alpha(0.20));
        } else if hovered(rect) {
            ctx.scene
                .fill_rounded_rect(rect, theme.radius.small, hover_fill(&theme));
        }
        let icon = if entry.is_dir {
            IconName::Files
        } else {
            file_icon(&entry.name)
        };
        paint_icon(
            ctx,
            icon,
            Rect::from_xywh(rect.x() + 6.0, row_y + 6.0, 16.0, 16.0),
            if entry.is_dir {
                colors.accent
            } else {
                colors.dim_text
            },
        );
        let name = elide(
            ctx,
            &entry.name,
            rect.width() - 34.0,
            theme.typography.body.size,
        );
        ctx.label_left(
            rect.x() + 28.0,
            rect.center().y,
            &name,
            theme.typography.body.size,
            weight::NORMAL,
            colors.text,
        );
    }
    ctx.scene.pop_clip();

    if browser.mode != BrowserMode::Folder {
        let focused = browser.filename.is_focused();
        let mut entry = browser.filename.clone();
        entry.set_focused(focused);
        entry.paint(ctx, lay.filename);
    } else {
        ctx.label_left(
            lay.filename.x(),
            lay.filename.center().y,
            &elide(
                ctx,
                &browser.dir.display().to_string(),
                lay.filename.width(),
                theme.typography.caption.size,
            ),
            theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
    if let Some(error) = &browser.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            colors.warning,
        );
    }
    let mut action = Button::new(browser.action_label()).variant(ButtonVariant::Suggested);
    action.set_hovered(hovered(lay.action));
    action.paint(ctx, lay.action);
    let mut cancel = Button::new(tr("Cancel"));
    cancel.set_hovered(hovered(lay.cancel));
    cancel.paint(ctx, lay.cancel);
}

// ── Okno nazwy w panelu projektu ────────────────────────────────────────────

struct PromptLayout {
    card: Rect,
    title: Rect,
    hint: Rect,
    entry: Rect,
    error: Rect,
    submit: Rect,
    cancel: Rect,
}

fn prompt_layout(size: Size, theme: &Theme, text: &TextEngine, label: &str) -> PromptLayout {
    let width = (size.width - 160.0).clamp(380.0, 540.0);
    let height = 196.0_f64.min(size.height - 60.0);
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 26.0);
    let hint = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        18.0,
    );
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        hint.bottom() + 10.0,
        card.width() - 40.0,
        34.0,
    );
    let error = Rect::from_xywh(
        card.x() + 20.0,
        entry.bottom() + 4.0,
        card.width() - 40.0,
        16.0,
    );
    let button_h = theme.metrics.button.height;
    let submit_w = Button::new(label).width(text, theme).max(96.0);
    let submit = Rect::from_xywh(
        card.right() - 20.0 - submit_w,
        card.bottom() - 14.0 - button_h,
        submit_w,
        button_h,
    );
    let cancel_w = Button::new(tr("Cancel")).width(text, theme).max(88.0);
    let cancel = Rect::from_xywh(submit.x() - 8.0 - cancel_w, submit.y(), cancel_w, button_h);
    PromptLayout {
        card,
        title,
        hint,
        entry,
        error,
        submit,
        cancel,
    }
}

fn paint_prompt(app: &App, ctx: &mut PaintCtx, size: Size) {
    let Some(prompt) = &app.prompt else {
        return;
    };
    let theme = *ctx.theme;
    let label = prompt.kind.submit_label();
    let lay = prompt_layout(size, &theme, ctx.text, &label);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &prompt.kind.title(),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let dir = if prompt.kind == PromptKind::Rename {
        prompt.target.parent()
    } else {
        Some(prompt.target.as_path())
    };
    let hint = dir
        .map(|dir| format!("{} · {}", tr("Folder"), dir.display()))
        .unwrap_or_default();
    ctx.label_left(
        lay.hint.x(),
        lay.hint.center().y,
        &elide(ctx, &hint, lay.hint.width(), theme.typography.caption.size),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    prompt.entry.paint(ctx, lay.entry);
    if let Some(error) = &prompt.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            &elide(ctx, error, lay.error.width(), theme.typography.caption.size),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    }
    let pointer = app.pointer;
    let mut submit = Button::new(label).variant(ButtonVariant::Suggested);
    submit.set_hovered(pointer.is_some_and(|p| lay.submit.contains(p)));
    submit.paint(ctx, lay.submit);
    let mut cancel = Button::new(tr("Cancel"));
    cancel.set_hovered(pointer.is_some_and(|p| lay.cancel.contains(p)));
    cancel.paint(ctx, lay.cancel);
}

// ── Ustawienia programu ─────────────────────────────────────────────

const SETTINGS_ROWS: usize = 5;
const SETTINGS_ROW_H: f64 = 40.0;
const SETTINGS_HEIGHT: f64 = 380.0;

/// Trafienie w oknie Ustawień.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsHit {
    Close,
    TabWidth(usize),
    AutoIndent,
    ShowHidden,
    SkipBuild,
    ShowWorkspace,
}

struct SettingsLayout {
    rows: [Rect; SETTINGS_ROWS],
    tab_width: Rect,
    auto_indent: Rect,
    show_hidden: Rect,
    skip_build: Rect,
    show_workspace: Rect,
    done: Rect,
}

fn settings_dialog() -> PreferencesDialog {
    PreferencesDialog::new(tr("Editor Settings"), SETTINGS_HEIGHT)
}

fn settings_layout(size: Size, theme: &Theme) -> SettingsLayout {
    let rects = settings_dialog().rects(size, theme);
    let pad = theme.spacing.lg;
    let inner = rects.content.deflate(Insets::new(pad, pad, pad, pad));
    let rows: [Rect; SETTINGS_ROWS] = std::array::from_fn(|index| {
        Rect::from_xywh(
            inner.x(),
            inner.y() + index as f64 * SETTINGS_ROW_H,
            inner.width(),
            SETTINGS_ROW_H,
        )
    });
    let seg_w = 180.0_f64.min(inner.width() * 0.5);
    let tab_width = Rect::from_xywh(
        inner.right() - seg_w,
        rows[0].center().y - 15.0,
        seg_w,
        30.0,
    );
    let toggle = theme.metrics.switch;
    let switch_rect = |row: Rect| {
        Rect::from_xywh(
            inner.right() - toggle.width,
            row.center().y - toggle.height / 2.0,
            toggle.width,
            toggle.height,
        )
    };
    let button_h = theme.metrics.button.height;
    let done = Rect::from_xywh(
        inner.right() - 120.0,
        inner.bottom() - button_h,
        120.0,
        button_h,
    );
    SettingsLayout {
        rows,
        tab_width,
        auto_indent: switch_rect(rows[1]),
        show_hidden: switch_rect(rows[2]),
        skip_build: switch_rect(rows[3]),
        show_workspace: switch_rect(rows[4]),
        done,
    }
}

fn paint_settings(app: &App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    settings_dialog().paint(ctx, size);
    let lay = settings_layout(size, &theme);
    let body = theme.typography.body.size;
    let labels = [
        tr("Tab width"),
        tr("Auto-indent"),
        tr("Show hidden files"),
        tr("Skip build folders"),
        tr("Show project panel at startup"),
    ];
    for (index, label) in labels.iter().enumerate() {
        ctx.label_left(
            lay.rows[index].x() + 12.0,
            lay.rows[index].center().y,
            label,
            body,
            weight::NORMAL,
            theme.colors.text,
        );
        if index > 0 {
            ctx.scene.fill_rect(
                Rect::from_xywh(
                    lay.rows[index].x(),
                    lay.rows[index].y(),
                    lay.rows[index].width(),
                    1.0,
                ),
                theme.colors.border,
            );
        }
    }

    let pointer = app.pointer;
    let hovered = |rect: Rect| pointer.is_some_and(|p| rect.contains(p));
    let mut widths = ViewSwitcher::new(TAB_WIDTHS.iter().map(|width| width.to_string()));
    widths.set_selected(
        TAB_WIDTHS
            .iter()
            .position(|width| *width == app.prefs.tab_width)
            .unwrap_or(1),
    );
    let width_hover = pointer.and_then(|p| widths.index_at(lay.tab_width, p));
    widths.set_hovered(width_hover);
    widths.paint(ctx, lay.tab_width);

    let mut auto_indent = Switch::new(app.prefs.auto_indent);
    auto_indent.set_hovered(hovered(lay.auto_indent));
    auto_indent.paint(ctx, lay.auto_indent);
    let mut show_hidden = Switch::new(app.prefs.show_hidden);
    show_hidden.set_hovered(hovered(lay.show_hidden));
    show_hidden.paint(ctx, lay.show_hidden);
    let mut skip_build = Switch::new(app.prefs.skip_build_dirs);
    skip_build.set_hovered(hovered(lay.skip_build));
    skip_build.paint(ctx, lay.skip_build);
    let mut show_workspace = Switch::new(app.prefs.show_workspace);
    show_workspace.set_hovered(hovered(lay.show_workspace));
    show_workspace.paint(ctx, lay.show_workspace);

    let mut done = Button::new(tr("Done"));
    done.set_hovered(hovered(lay.done));
    done.paint(ctx, lay.done);
}

fn settings_hit(size: Size, theme: &Theme, point: Point) -> Option<SettingsHit> {
    let lay = settings_layout(size, theme);
    let dialog = settings_dialog();
    if dialog.close_at(size, theme, point)
        || dialog.scrim_at(size, theme, point)
        || lay.done.contains(point)
    {
        return Some(SettingsHit::Close);
    }
    let widths = ViewSwitcher::new(TAB_WIDTHS.iter().map(|width| width.to_string()));
    if let Some(index) = widths.index_at(lay.tab_width, point) {
        return Some(SettingsHit::TabWidth(index));
    }
    if lay.auto_indent.contains(point) {
        return Some(SettingsHit::AutoIndent);
    }
    if lay.show_hidden.contains(point) {
        return Some(SettingsHit::ShowHidden);
    }
    if lay.skip_build.contains(point) {
        return Some(SettingsHit::SkipBuild);
    }
    if lay.show_workspace.contains(point) {
        return Some(SettingsHit::ShowWorkspace);
    }
    None
}

fn handle_settings(app: &mut App, event: &Event, size: Size, theme: &Theme) {
    match event {
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            match settings_hit(size, theme, button.position) {
                Some(SettingsHit::Close) => app.close_settings(),
                Some(SettingsHit::TabWidth(index)) => {
                    if let Some(width) = TAB_WIDTHS.get(index) {
                        app.set_tab_width(*width);
                    }
                }
                Some(SettingsHit::AutoIndent) => app.toggle_auto_indent(),
                Some(SettingsHit::ShowHidden) => app.toggle_show_hidden(),
                Some(SettingsHit::SkipBuild) => app.toggle_skip_build_dirs(),
                Some(SettingsHit::ShowWorkspace) => app.toggle_show_workspace(),
                None => {}
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == 0xff1b => {
            app.close_settings();
        }
        _ => {}
    }
}

/// Prostokąt okna Ustawień (dla testów).
#[cfg(test)]
fn settings_panel(size: Size, theme: &Theme) -> Rect {
    settings_dialog().rects(size, theme).panel
}

// ── Dialog „Idź do linii” ──────────────────────────────────────────────────

struct GotoLayout {
    card: Rect,
    title: Rect,
    hint: Rect,
    entry: Rect,
    error: Rect,
    submit: Rect,
    cancel: Rect,
}

fn goto_layout(size: Size, theme: &Theme) -> GotoLayout {
    let width = (size.width - 160.0).clamp(340.0, 500.0);
    let height = 196.0_f64.min(size.height - 60.0);
    let card = Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    );
    let title = Rect::from_xywh(card.x() + 20.0, card.y() + 14.0, card.width() - 40.0, 26.0);
    let hint = Rect::from_xywh(
        card.x() + 20.0,
        title.bottom() + 2.0,
        card.width() - 40.0,
        18.0,
    );
    let entry = Rect::from_xywh(
        card.x() + 20.0,
        hint.bottom() + 10.0,
        card.width() - 40.0,
        34.0,
    );
    let error = Rect::from_xywh(
        card.x() + 20.0,
        entry.bottom() + 4.0,
        card.width() - 40.0,
        16.0,
    );
    let button_h = theme.metrics.button.height;
    let submit = Rect::from_xywh(
        card.right() - 20.0 - 90.0,
        card.bottom() - 14.0 - button_h,
        90.0,
        button_h,
    );
    let cancel = Rect::from_xywh(submit.x() - 8.0 - 96.0, submit.y(), 96.0, button_h);
    GotoLayout {
        card,
        title,
        hint,
        entry,
        error,
        submit,
        cancel,
    }
}

fn paint_goto(app: &App, ctx: &mut PaintCtx, size: Size) {
    let Some(goto) = &app.goto else {
        return;
    };
    let theme = *ctx.theme;
    let lay = goto_layout(size, &theme);
    ctx.scene
        .fill_rect(Rect::from_size(size), Color::rgba(0.0, 0.0, 0.0, 0.45));
    ctx.scene
        .fill_rounded_rect(lay.card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(lay.card, theme.radius.large, 1.0, theme.colors.border);
    ctx.label_left(
        lay.title.x(),
        lay.title.center().y,
        &tr("Go to line"),
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.hint.x(),
        lay.hint.center().y,
        &tr("Pass a line number and confirm with Enter"),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let mut entry = goto.entry.clone();
    entry.set_focused(true);
    entry.paint(ctx, lay.entry);
    if let Some(error) = &goto.error {
        ctx.label_left(
            lay.error.x(),
            lay.error.center().y,
            error,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.warning,
        );
    }
    let pointer = app.pointer;
    let mut submit = Button::new(tr("Go")).variant(ButtonVariant::Suggested);
    submit.set_hovered(pointer.is_some_and(|p| lay.submit.contains(p)));
    submit.paint(ctx, lay.submit);
    let mut cancel = Button::new(tr("Cancel"));
    cancel.set_hovered(pointer.is_some_and(|p| lay.cancel.contains(p)));
    cancel.paint(ctx, lay.cancel);
}

fn menu_panel(
    app: &App,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) -> Option<Rect> {
    if !app.menu_open {
        return None;
    }
    let width = app.menu.width(text, theme);
    let viewport = Rect::from_xywh(
        0.0,
        lay.header.bottom(),
        size.width,
        (size.height - lay.header.height()).max(0.0),
    );
    Some(app.menu.place(lay.menu, width, viewport, theme))
}

fn file_icon(name: &str) -> IconName {
    if Language::from_name(name) == Language::Plain {
        IconName::Document
    } else {
        IconName::Code
    }
}

fn elide(ctx: &PaintCtx, text: &str, max: f64, size: f64) -> String {
    if ctx.measure(text, size, weight::NORMAL) <= max || text.is_empty() {
        return text.to_owned();
    }
    let mut out = String::new();
    for ch in text.chars() {
        out.push(ch);
        if ctx.measure(&format!("{out}…"), size, weight::NORMAL) > max {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}

// ── Zdarzenia ──────────────────────────────────────────────────────────────

pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(
        size,
        theme,
        app.workspace.visible,
        app.find.is_some(),
        app.chat.visible && crate::chat::available(app),
        crate::chat::available(app),
    );
    for event in events {
        match event {
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            _ => {}
        }
        if let Event::Window(WindowEvent::CloseRequested) = event {
            app.request_close();
            continue;
        }
        if app.error.is_some() {
            handle_alert(app, event, size, theme, text, AlertTarget::Error);
        } else if app.confirm.is_some() {
            handle_confirm(app, event, size, theme, text);
        } else if app.overwrite.is_some() {
            handle_alert(app, event, size, theme, text, AlertTarget::Overwrite);
        } else if app.pending_delete.is_some() {
            handle_alert(app, event, size, theme, text, AlertTarget::Delete);
        } else if crate::plugin_ui::is_open(app) {
            crate::plugin_ui::handle_events(app, event, size, theme, text);
        } else if app.settings_open {
            handle_settings(app, event, size, theme);
        } else if app.browser.is_some() {
            handle_browser(app, event, size);
        } else if app.prompt.is_some() {
            handle_prompt(app, event, size, theme, text);
        } else if app.goto.is_some() {
            handle_goto(app, event, size, theme);
        } else if app.tree_menu.is_some() {
            handle_tree_menu(app, event, size, theme, text);
        } else if crate::chat::handle_events(app, event, &lay, theme, text) {
            // Zdarzenie obsłużone przez panel czatu.
        } else if app.menu_open {
            handle_menu(app, event, &lay, size, theme, text);
        } else {
            handle_normal(app, event, &lay, size, theme, text);
        }
    }
    app.tabs.scroll_into_view(app.active, lay.tabs, text, theme);
    if app.workspace.visible {
        let max = (app.workspace.rows.len() as f64 * WORKSPACE_ROW_H - lay.workspace_tree.height())
            .max(0.0);
        app.workspace.scroll = app.workspace.scroll.clamp(0.0, max);
    }
    if app.reveal_cursor {
        app.view_mut()
            .ensure_cursor_visible(lay.editor, theme, text);
        app.reveal_cursor = false;
    }
}

enum AlertTarget {
    Error,
    Overwrite,
    Delete,
}

fn handle_alert(
    app: &mut App,
    event: &Event,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    target: AlertTarget,
) {
    let result = match (&target, event) {
        (AlertTarget::Error, Event::PointerButton(button))
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            app.error.as_ref().map(|dialog| {
                let action = dialog.action_at(size, theme, text, button.position);
                let rects = dialog.rects(size, theme, text);
                let dismiss = !rects.dialog.panel.contains(button.position)
                    || rects.dialog.close.contains(button.position);
                (action, dismiss)
            })
        }
        (AlertTarget::Overwrite, Event::PointerButton(button))
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            app.overwrite.as_ref().map(|(_, dialog)| {
                let action = dialog.action_at(size, theme, text, button.position);
                let rects = dialog.rects(size, theme, text);
                let dismiss = !rects.dialog.panel.contains(button.position)
                    || rects.dialog.close.contains(button.position);
                (action, dismiss)
            })
        }
        (AlertTarget::Delete, Event::PointerButton(button))
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            app.pending_delete.as_ref().map(|(_, dialog)| {
                let action = dialog.action_at(size, theme, text, button.position);
                let rects = dialog.rects(size, theme, text);
                let dismiss = !rects.dialog.panel.contains(button.position)
                    || rects.dialog.close.contains(button.position);
                (action, dismiss)
            })
        }
        _ => None,
    };
    if let Some((action, dismiss)) = result {
        if action.is_some() || dismiss {
            match target {
                AlertTarget::Error => app.error = None,
                AlertTarget::Overwrite => {
                    let confirmed = action == Some(0) && !dismiss;
                    app.answer_overwrite(confirmed);
                }
                AlertTarget::Delete => {
                    let confirmed = action == Some(0) && !dismiss;
                    app.answer_delete(confirmed);
                }
            }
        }
        return;
    }
    if let Event::Key(key) = event {
        if key.state == ButtonState::Pressed && key.keysym == 0xff1b {
            match target {
                AlertTarget::Error => app.error = None,
                AlertTarget::Overwrite => app.answer_overwrite(false),
                AlertTarget::Delete => app.answer_delete(false),
            }
        }
    }
}

fn handle_confirm(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    let result = match event {
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            app.confirm.as_ref().map(|confirm| {
                let action = confirm.dialog.action_at(size, theme, text, button.position);
                let rects = confirm.dialog.rects(size, theme, text);
                let dismiss = !rects.dialog.panel.contains(button.position)
                    || rects.dialog.close.contains(button.position);
                (action, dismiss)
            })
        }
        _ => None,
    };
    if let Some((action, dismiss)) = result {
        match action {
            Some(action) => app.answer_confirm(action),
            None if dismiss => app.confirm = None,
            None => {}
        }
        return;
    }
    if let Event::Key(key) = event {
        if key.state == ButtonState::Pressed && key.keysym == 0xff1b {
            app.confirm = None;
        }
    }
}

fn handle_goto(app: &mut App, event: &Event, size: Size, theme: &Theme) {
    let lay = goto_layout(size, theme);
    match event {
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            if lay.submit.contains(button.position) {
                app.submit_goto();
            } else if lay.cancel.contains(button.position) || !lay.card.contains(button.position) {
                app.goto = None;
                app.focus_editor();
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            let command = app
                .goto
                .as_mut()
                .and_then(|dialog| clipboard_command(&mut dialog.entry, key));
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return;
            }
            match key.keysym {
                0xff1b => {
                    app.goto = None;
                    app.focus_editor();
                }
                0xff0d | 0xff8d => app.submit_goto(),
                _ => {
                    if let Some(dialog) = app.goto.as_mut() {
                        match key.keysym {
                            0xff08 => dialog.entry.backspace(),
                            0xffff => dialog.entry.delete_forward(),
                            0xff51 => dialog.entry.move_left(),
                            0xff53 => dialog.entry.move_right(),
                            0xff50 => dialog.entry.home(),
                            0xff57 => dialog.entry.end(),
                            0x61 if key.modifiers.ctrl => dialog.entry.select_all(),
                            _ => {
                                if !key.modifiers.ctrl {
                                    if let Some(value) = &key.text {
                                        let plain: String =
                                            value.chars().filter(|ch| !ch.is_control()).collect();
                                        if !plain.is_empty() {
                                            dialog.entry.insert_str(&plain);
                                            dialog.error = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Event::Text(value) => {
            let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
            if !plain.is_empty() {
                if let Some(dialog) = app.goto.as_mut() {
                    dialog.entry.insert_str(&plain);
                    dialog.error = None;
                }
            }
        }
        Event::Paste(value) => {
            if let Some(dialog) = app.goto.as_mut() {
                dialog.entry.insert_str(value);
                dialog.error = None;
            }
        }
        _ => {}
    }
}

fn handle_prompt(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    let Some(kind) = app.prompt.as_ref().map(|prompt| prompt.kind) else {
        return;
    };
    let label = kind.submit_label();
    let lay = prompt_layout(size, theme, text, &label);
    match event {
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            if lay.submit.contains(button.position) {
                app.submit_prompt();
            } else if lay.cancel.contains(button.position) || !lay.card.contains(button.position) {
                app.cancel_prompt();
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            let command = app
                .prompt
                .as_mut()
                .and_then(|prompt| clipboard_command(&mut prompt.entry, key));
            if let Some(command) = command {
                match command {
                    TextClipboardCommand::Copy(Some(value))
                    | TextClipboardCommand::Cut(Some(value)) => app.clipboard_text = Some(value),
                    TextClipboardCommand::Paste => app.paste_requested = true,
                    _ => {}
                }
                return;
            }
            match key.keysym {
                0xff1b => app.cancel_prompt(),
                0xff0d | 0xff8d => app.submit_prompt(),
                _ => {
                    if let Some(prompt) = app.prompt.as_mut() {
                        match key.keysym {
                            0xff08 => prompt.entry.backspace(),
                            0xffff => prompt.entry.delete_forward(),
                            0xff51 => prompt.entry.move_left(),
                            0xff53 => prompt.entry.move_right(),
                            0xff50 => prompt.entry.home(),
                            0xff57 => prompt.entry.end(),
                            0x61 if key.modifiers.ctrl => prompt.entry.select_all(),
                            _ => {
                                if !key.modifiers.ctrl {
                                    if let Some(value) = &key.text {
                                        let plain: String =
                                            value.chars().filter(|ch| !ch.is_control()).collect();
                                        if !plain.is_empty() {
                                            prompt.entry.insert_str(&plain);
                                            prompt.error = None;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Event::Text(value) => {
            let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
            if !plain.is_empty() {
                if let Some(prompt) = app.prompt.as_mut() {
                    prompt.entry.insert_str(&plain);
                    prompt.error = None;
                }
            }
        }
        Event::Paste(value) => {
            if let Some(prompt) = app.prompt.as_mut() {
                prompt.entry.insert_str(value);
                prompt.error = None;
            }
        }
        _ => {}
    }
}

fn handle_tree_menu(app: &mut App, event: &Event, size: Size, theme: &Theme, text: &TextEngine) {
    let Some(tree) = app.tree_menu.as_ref() else {
        return;
    };
    let width = tree.menu.width(text, theme);
    let panel = tree
        .menu
        .place(tree.anchor, width, Rect::from_size(size), theme);
    match event {
        Event::PointerButton(button) if button.state == ButtonState::Pressed => {
            if button.button == PointerButton::Left {
                let action = app
                    .tree_menu
                    .as_mut()
                    .and_then(|tree| tree.menu.click_at(panel, button.position, theme));
                if let Some(action) = action {
                    app.tree_menu_action(action);
                } else if !panel.contains(button.position) {
                    app.tree_menu = None;
                }
            } else {
                app.tree_menu = None;
            }
        }
        Event::PointerMotion { position, .. } => {
            if let Some(tree) = app.tree_menu.as_mut() {
                tree.menu.hover_at(panel, Some(*position), theme);
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == 0xff1b => {
            app.tree_menu = None;
        }
        _ => {}
    }
}

fn handle_right_click(app: &mut App, lay: &Layout, point: Point) {
    app.tree_menu = None;
    if !app.workspace.visible || !lay.workspace_tree.contains(point) {
        return;
    }
    let index = ((point.y - lay.workspace_tree.y() + app.workspace.scroll) / WORKSPACE_ROW_H)
        .floor()
        .max(0.0) as usize;
    if index >= app.workspace.rows.len() {
        return;
    }
    if let Some(anchor) = workspace_row_anchor(app, lay, index) {
        app.open_tree_menu(index, anchor);
    }
}

/// Klawiatura drzewa projektu: strzałki, Enter, F2 i Delete.
fn handle_workspace_key(app: &mut App, key: &KeyEvent, lay: &Layout) -> bool {
    if key.modifiers.ctrl || key.modifiers.alt {
        return false;
    }
    match key.keysym {
        0xff52 => app.workspace_move(-1),
        0xff54 => app.workspace_move(1),
        0xff50 => app.workspace_select_edge(false),
        0xff57 => app.workspace_select_edge(true),
        0xff51 => app.workspace_collapse_or_parent(),
        0xff53 => app.workspace_expand_or_child(),
        0xff0d | 0xff8d => app.workspace_activate_selected(),
        0xffbf => app.rename_selected(),
        0xffff => app.delete_selected(),
        0xff1b => app.workspace_focused = false,
        _ => {}
    }
    scroll_workspace_to(app, lay);
    true
}

fn scroll_workspace_to(app: &mut App, lay: &Layout) {
    let Some(index) = app.workspace.selected_index() else {
        return;
    };
    let top = index as f64 * WORKSPACE_ROW_H;
    let bottom = top + WORKSPACE_ROW_H;
    let view = lay.workspace_tree.height();
    if top < app.workspace.scroll {
        app.workspace.scroll = top;
    } else if bottom > app.workspace.scroll + view {
        app.workspace.scroll = bottom - view;
    }
    let max = (app.workspace.rows.len() as f64 * WORKSPACE_ROW_H - view).max(0.0);
    app.workspace.scroll = app.workspace.scroll.clamp(0.0, max);
}

fn handle_browser(app: &mut App, event: &Event, size: Size) {
    let Some(browser) = app.browser.as_ref() else {
        return;
    };
    let lay = browser_layout(size);
    let save_mode = browser.mode == BrowserMode::Save;
    let entry_focused = browser.filename.is_focused();
    match event {
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let point = button.position;
            if lay.close.contains(point) || !lay.card.contains(point) {
                app.browser = None;
                return;
            }
            if lay.up.contains(point) {
                if let Some(browser) = app.browser.as_mut() {
                    browser.up();
                }
                return;
            }
            if lay.action.contains(point) {
                app.submit_browser();
                return;
            }
            if lay.cancel.contains(point) {
                app.browser = None;
                return;
            }
            if lay.filename.contains(point) && save_mode {
                if let Some(browser) = app.browser.as_mut() {
                    browser.filename.set_focused(true);
                }
                return;
            }
            // Miejsca XDG.
            let mut y = lay.places.y();
            for (_, path) in crate::browser::places() {
                let rect = Rect::from_xywh(lay.places.x(), y, lay.places.width(), 26.0);
                if rect.contains(point) {
                    if let Some(browser) = app.browser.as_mut() {
                        browser.navigate(path);
                    }
                    return;
                }
                y += 28.0;
            }
            // Lista plików.
            if lay.list.contains(point) {
                if let Some(browser) = app.browser.as_mut() {
                    let row = ((point.y - lay.list.y() - 2.0 + browser.scroll) / BROWSER_ROW_H)
                        .floor()
                        .max(0.0) as usize;
                    if row < browser.entries.len() {
                        let entry = browser.entries[row].clone();
                        let already = browser.selected == Some(row);
                        browser.select(row);
                        if entry.is_dir {
                            browser.navigate(entry.path);
                        } else if already || !save_mode {
                            app.submit_browser();
                        }
                    }
                }
            }
        }
        Event::PointerMotion { .. } => {}
        Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
            if let Some(browser) = app.browser.as_mut() {
                let max =
                    (browser.entries.len() as f64 * BROWSER_ROW_H - lay.list.height()).max(0.0);
                browser.scroll =
                    (browser.scroll + scroll.notches_y() * BROWSER_ROW_H).clamp(0.0, max);
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if key.keysym == 0xff1b {
                app.browser = None;
                return;
            }
            if save_mode && entry_focused {
                let command = app
                    .browser
                    .as_mut()
                    .and_then(|browser| clipboard_command(&mut browser.filename, key));
                if let Some(command) = command {
                    match command {
                        TextClipboardCommand::Copy(Some(value))
                        | TextClipboardCommand::Cut(Some(value)) => {
                            app.clipboard_text = Some(value)
                        }
                        TextClipboardCommand::Paste => app.paste_requested = true,
                        _ => {}
                    }
                    return;
                }
                match key.keysym {
                    0xff0d | 0xff8d => app.submit_browser(),
                    _ => {
                        if let Some(browser) = app.browser.as_mut() {
                            match key.keysym {
                                0xff08 => browser.filename.backspace(),
                                0xffff => browser.filename.delete_forward(),
                                0xff51 => browser.filename.move_left(),
                                0xff53 => browser.filename.move_right(),
                                0xff50 => browser.filename.home(),
                                0xff57 => browser.filename.end(),
                                0x61 if key.modifiers.ctrl => browser.filename.select_all(),
                                _ => {
                                    if !key.modifiers.ctrl {
                                        if let Some(value) = &key.text {
                                            let plain: String = value
                                                .chars()
                                                .filter(|ch| !ch.is_control())
                                                .collect();
                                            if !plain.is_empty() {
                                                browser.filename.insert_str(&plain);
                                                browser.error = None;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return;
            }
            match key.keysym {
                0xff52 => {
                    if let Some(browser) = app.browser.as_mut() {
                        browser.move_selection(-1);
                    }
                }
                0xff54 => {
                    if let Some(browser) = app.browser.as_mut() {
                        browser.move_selection(1);
                    }
                }
                0xff0d | 0xff8d => {
                    let outcome = app
                        .browser
                        .as_mut()
                        .and_then(FileBrowser::activate_selected);
                    if let Some(outcome) = outcome {
                        app.apply_browser_outcome(outcome);
                    }
                }
                0xff08 => {
                    if let Some(browser) = app.browser.as_mut() {
                        browser.up();
                    }
                }
                _ => {}
            }
        }
        Event::Text(value) if save_mode => {
            let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
            if !plain.is_empty() {
                if let Some(browser) = app.browser.as_mut() {
                    browser.filename.insert_str(&plain);
                    browser.error = None;
                }
            }
        }
        Event::Paste(value) if save_mode => {
            if let Some(browser) = app.browser.as_mut() {
                browser.filename.insert_str(value);
                browser.error = None;
            }
        }
        _ => {}
    }
}

fn handle_menu(
    app: &mut App,
    event: &Event,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let panel = menu_panel(app, lay, size, theme, text);
    match event {
        Event::PointerMotion { position, .. } => {
            app.hover_menu = lay.menu.contains(*position);
            if let Some(panel) = panel {
                app.menu.hover_at(panel, Some(*position), theme);
            }
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            if let Some(panel) = panel {
                if panel.contains(button.position) {
                    if let Some(index) = app.menu.click_at(panel, button.position, theme) {
                        app.menu_open = false;
                        app.menu_action(index);
                    }
                    return;
                }
            }
            app.menu_open = false;
        }
        Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == 0xff1b => {
            app.menu_open = false;
        }
        _ => {}
    }
}

fn handle_normal(
    app: &mut App,
    event: &Event,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    match event {
        Event::PointerMotion { position, .. } => {
            app.hover_control = HeaderBar::new()
                .controls(ControlsSide::Right)
                .control_at(lay.header, theme, *position);
            app.hover_menu = lay.menu.contains(*position);
            app.hover_chat_toggle = lay.chat_ai.width() > 0.0 && lay.chat_ai.contains(*position);
            app.hover_workspace_toggle = lay.workspace_toggle.contains(*position);
            app.hover_workspace_row =
                if app.workspace.visible && lay.workspace_tree.contains(*position) {
                    let row = ((position.y - lay.workspace_tree.y() + app.workspace.scroll)
                        / WORKSPACE_ROW_H)
                        .floor()
                        .max(0.0) as usize;
                    (row < app.workspace.rows.len()).then_some(row)
                } else {
                    None
                };
            app.hover_workspace_open = app.workspace.visible
                && app
                    .workspace
                    .preview
                    .as_ref()
                    .is_some_and(|preview| !preview.is_dir)
                && Rect::from_xywh(
                    lay.workspace_preview.right() - 86.0,
                    lay.workspace_preview.bottom() - 34.0,
                    76.0,
                    26.0,
                )
                .contains(*position);
            app.tabs
                .set_hovered(app.tabs.index_at(lay.tabs, text, theme, *position));
            app.view_mut()
                .set_scroll_hover(lay.editor, theme, Some(*position));
            if app.dragging_selection {
                let index = app.view().index_at(lay.editor, theme, text, *position);
                let anchor = app.drag_anchor;
                app.view_mut().set_selection(anchor, index);
            }
            if app.view().is_scroll_dragging() {
                app.view_mut().drag_scroll(lay.editor, theme, *position);
            }
        }
        Event::PointerLeave => {
            app.hover_control = None;
            app.hover_menu = false;
            app.hover_chat_toggle = false;
            app.hover_workspace_toggle = false;
            app.hover_workspace_row = None;
            app.hover_workspace_open = false;
            app.tabs.set_hovered(None);
            app.view_mut().set_scroll_hover(lay.editor, theme, None);
        }
        Event::PointerButton(button) if button.state == ButtonState::Pressed => {
            match button.button {
                PointerButton::Left => handle_click(app, lay, size, theme, text, button.position),
                PointerButton::Right => {
                    handle_right_click(app, lay, button.position);
                }
                _ => {}
            }
        }
        Event::PointerButton(button) if button.state == ButtonState::Released => {
            app.dragging_selection = false;
            app.view_mut().end_scroll_drag();
        }
        Event::Scroll(scroll) => {
            if lay.tabs.contains(scroll.position) {
                app.tabs
                    .on_scroll(scroll.notches_x() * SCROLL_STEP, lay.tabs, text, theme);
            } else if app.workspace.visible && lay.workspace_tree.contains(scroll.position) {
                let max = (app.workspace.rows.len() as f64 * WORKSPACE_ROW_H
                    - lay.workspace_tree.height())
                .max(0.0);
                app.workspace.scroll =
                    (app.workspace.scroll + scroll.notches_y() * SCROLL_STEP).clamp(0.0, max);
            } else if lay.editor.contains(scroll.position) {
                app.view_mut().scroll(
                    scroll.notches_x() * SCROLL_STEP,
                    scroll.notches_y() * SCROLL_STEP,
                    lay.editor,
                    theme,
                    text,
                );
                app.view_mut()
                    .set_scroll_hover(lay.editor, theme, Some(scroll.position));
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed => {
            if app.find_focused() {
                handle_find_key(app, key);
            } else if app.workspace_focused && handle_workspace_key(app, key, lay) {
                // Klawiatura drzewa projektu.
            } else {
                handle_editor_key(app, key, lay, theme);
            }
        }
        Event::Text(value) => {
            if app.find_focused() {
                insert_find_text(app, value);
            } else if !app.workspace_focused {
                app.insert_text(value);
            }
        }
        Event::Paste(value) => {
            if app.find_focused() {
                insert_find_text(app, value);
            } else if !app.workspace_focused {
                app.paste_text(value);
            }
        }
        Event::Drop { paths, .. } => {
            // Przeciągnięcie plików z menedżera: otwórz je w zakładkach,
            // a katalog ustaw jako projekt.
            let mut opened = false;
            for path in paths {
                if path.is_dir() {
                    app.workspace.set_root(path.clone());
                } else if path.is_file() {
                    app.open_path(path.clone());
                    opened = true;
                }
            }
            if opened {
                app.focus_editor();
            }
        }
        _ => {}
    }
}

fn handle_click(
    app: &mut App,
    lay: &Layout,
    _size: Size,
    theme: &Theme,
    text: &TextEngine,
    point: Point,
) {
    let header = HeaderBar::new().controls(ControlsSide::Right);
    app.workspace_focused = false;
    if let Some(control) = header.control_at(lay.header, theme, point) {
        match control {
            WindowControl::Minimize => app.pending.minimize = true,
            WindowControl::Maximize => app.pending.maximize = true,
            WindowControl::Close => app.request_close(),
        }
        return;
    }
    if lay.menu.contains(point) {
        app.menu_open = true;
        app.menu.set_hovered(None);
        return;
    }
    if lay.chat_ai.width() > 0.0 && lay.chat_ai.contains(point) {
        crate::chat::toggle(app);
        return;
    }
    if lay.workspace_toggle.contains(point) {
        app.toggle_workspace();
        return;
    }
    // Zakładki w headerbarze.
    if lay.tabs.contains(point) {
        if let Some(dir) = app.tabs.scroll_dir_at(lay.tabs, text, theme, point) {
            let delta = match dir {
                libluna_widgets::ScrollDir::Left => -SCROLL_STEP,
                libluna_widgets::ScrollDir::Right => SCROLL_STEP,
            };
            app.tabs.on_scroll(delta, lay.tabs, text, theme);
            return;
        }
        if let Some(index) = app.tabs.close_at(lay.tabs, text, theme, point) {
            app.close_tab(index);
            return;
        }
        if let Some(index) = app.tabs.index_at(lay.tabs, text, theme, point) {
            app.select_tab(index);
            // Przeciągnięcie zakładki przenosi całe okno (brak przestawiania kart).
            app.pending.begin_move = true;
        }
        // Puste miejsce paska zakładek również chwyta okno.
        if lay.header.contains(point) {
            app.pending.begin_move = true;
        }
        return;
    }
    if lay.header.contains(point) {
        app.pending.begin_move = true;
        return;
    }
    // Workspace.
    if app.workspace.visible && lay.workspace.contains(point) {
        app.workspace_focused = true;
        if lay.workspace_folder.contains(point) || lay.workspace_root.contains(point) {
            app.open_folder_browser();
            return;
        }
        if lay.workspace_tree.contains(point) {
            let row = ((point.y - lay.workspace_tree.y() + app.workspace.scroll) / WORKSPACE_ROW_H)
                .floor()
                .max(0.0) as usize;
            if row < app.workspace.rows.len() {
                app.workspace_row_clicked(row);
            }
            return;
        }
        if app
            .workspace
            .preview
            .as_ref()
            .is_some_and(|preview| !preview.is_dir)
            && Rect::from_xywh(
                lay.workspace_preview.right() - 86.0,
                lay.workspace_preview.bottom() - 34.0,
                76.0,
                26.0,
            )
            .contains(point)
        {
            app.workspace_open_selected();
        }
        return;
    }
    if app.find.is_some() && lay.find_bar.contains(point) {
        if lay.find_close.contains(point) {
            app.close_find();
            return;
        }
        if lay.find_entry.contains(point) {
            let clear = app
                .find
                .as_mut()
                .is_some_and(|find| find.entry.clear_at(lay.find_entry, theme, point));
            app.set_find_focus(true);
            if clear {
                app.refresh_find();
            }
            return;
        }
        return;
    }
    if !lay.editor.contains(point) {
        return;
    }
    if app.view_mut().begin_scroll_drag(lay.editor, theme, point) {
        return;
    }
    let metrics = app.view().metrics(lay.editor, theme, text);
    let index = app.view().index_at(lay.editor, theme, text, point);
    app.view_mut().set_focused(true);
    app.set_find_focus(false);
    if metrics.gutter.contains(point) {
        let line = app.view().line_at(lay.editor, theme, text, point);
        let start = app.view().line_start(line);
        let mut end = app.view().line_end(line);
        if end < app.view().text().len() {
            end += 1;
        }
        app.view_mut().set_selection(start, end);
        app.reveal_cursor = true;
        return;
    }
    if point.x > metrics.gutter.right() && metrics.text_area.contains(point) {
        if app.dragging_selection {
            let anchor = app.drag_anchor;
            app.view_mut().set_selection(anchor, index);
        } else {
            app.view_mut().set_cursor(index);
            app.drag_anchor = index;
            app.dragging_selection = true;
        }
        app.reveal_cursor = true;
    }
}

fn handle_editor_key(app: &mut App, key: &KeyEvent, lay: &Layout, theme: &Theme) {
    let shift = key.modifiers.shift;
    if key.modifiers.alt {
        match key.keysym {
            0xff51 => {
                app.view_mut().move_left(shift);
                app.reveal_cursor = true;
                return;
            }
            0xff53 => {
                app.view_mut().move_right(shift);
                app.reveal_cursor = true;
                return;
            }
            _ => {}
        }
    }
    if key.modifiers.ctrl {
        match key.keysym {
            0x6f => app.open_open_browser(),
            0x73 if shift => app.open_save_browser(),
            0x73 => app.save(),
            0x6e => app.new_file(),
            0x77 => app.close_tab(app.active),
            0x66 => app.toggle_find(),
            0x67 => app.open_goto_dialog(),
            0x2c => app.open_settings(),
            0x70 if shift => app.open_plugins(),
            0x61 if shift => crate::chat::toggle(app),
            0x61 => {
                app.view_mut().select_all();
                app.focus_editor();
            }
            0x63 => app.copy(),
            0x78 => app.cut(),
            0x76 => app.paste_requested = true,
            0x7a if shift => app.redo(),
            0x7a => app.undo(),
            0x79 => app.redo(),
            0x62 => app.toggle_workspace(),
            0x09 if shift => app.select_prev_tab(),
            0x09 => app.select_next_tab(),
            0xff50 => {
                app.view_mut().document_start(shift);
                app.reveal_cursor = true;
            }
            0xff57 => {
                app.view_mut().document_end(shift);
                app.reveal_cursor = true;
            }
            _ => {}
        }
        return;
    }
    match key.keysym {
        0xff51 => {
            app.view_mut().move_left(shift);
            app.reveal_cursor = true;
        }
        0xff53 => {
            app.view_mut().move_right(shift);
            app.reveal_cursor = true;
        }
        0xff52 => {
            app.view_mut().move_up(shift);
            app.reveal_cursor = true;
        }
        0xff54 => {
            app.view_mut().move_down(shift);
            app.reveal_cursor = true;
        }
        0xff50 => {
            app.view_mut().home(shift);
            app.reveal_cursor = true;
        }
        0xff57 => {
            app.view_mut().end(shift);
            app.reveal_cursor = true;
        }
        0xff55 => {
            let rows = (lay.editor.height() / theme.typography.body.line_height).floor() as isize;
            app.view_mut().move_page(-rows.max(1), shift);
            app.reveal_cursor = true;
        }
        0xff56 => {
            let rows = (lay.editor.height() / theme.typography.body.line_height).floor() as isize;
            app.view_mut().move_page(rows.max(1), shift);
            app.reveal_cursor = true;
        }
        0xff08 => app.backspace(),
        0xffff => app.delete_forward(),
        0xff0d | 0xff8d => app.newline(),
        0xff09 => app.tab(shift),
        0xff1b => {
            if app.find.is_some() {
                app.close_find();
            } else {
                app.view_mut().clear_selection();
            }
        }
        _ => {
            if let Some(value) = &key.text {
                let plain: String = value
                    .chars()
                    .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
                    .collect();
                if !plain.is_empty() {
                    app.insert_text(&plain);
                }
            }
        }
    }
}

fn handle_find_key(app: &mut App, key: &KeyEvent) {
    let command = app
        .find
        .as_mut()
        .and_then(|find| clipboard_command(&mut find.entry, key));
    if let Some(command) = command {
        match command {
            TextClipboardCommand::Copy(Some(text)) | TextClipboardCommand::Cut(Some(text)) => {
                app.clipboard_text = Some(text)
            }
            TextClipboardCommand::Paste => app.paste_requested = true,
            _ => {}
        }
        app.refresh_find();
        return;
    }
    match key.keysym {
        0xff1b => {
            app.close_find();
            return;
        }
        0xff0d | 0xff8d => {
            app.find_step(!key.modifiers.shift);
            return;
        }
        _ => {}
    }
    let changed = if let Some(find) = app.find.as_mut() {
        match key.keysym {
            0xff08 => {
                find.entry.backspace();
                true
            }
            0xffff => {
                find.entry.delete_forward();
                true
            }
            0xff51 => {
                find.entry.move_left();
                false
            }
            0xff53 => {
                find.entry.move_right();
                false
            }
            0xff50 => {
                find.entry.home();
                false
            }
            0xff57 => {
                find.entry.end();
                false
            }
            0x61 if key.modifiers.ctrl => {
                find.entry.select_all();
                false
            }
            _ => {
                if key.modifiers.ctrl {
                    false
                } else if let Some(value) = &key.text {
                    let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
                    if plain.is_empty() {
                        false
                    } else {
                        find.entry.insert_str(&plain);
                        true
                    }
                } else {
                    false
                }
            }
        }
    } else {
        false
    };
    if changed {
        app.refresh_find();
    }
}

fn insert_find_text(app: &mut App, value: &str) {
    let plain: String = value.chars().filter(|ch| !ch.is_control()).collect();
    if plain.is_empty() {
        return;
    }
    if let Some(find) = app.find.as_mut() {
        find.entry.insert_str(&plain);
    }
    app.refresh_find();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{TREE_DELETE, TREE_NEW_FILE};
    use libluna_core::{ButtonState, PointerButton, PointerEvent, ScrollEvent};
    use libluna_render::Scene;

    /// Ten sam układ, jaki widzi obsługa zdarzeń (zależny od wtyczek w systemie).
    fn test_layout(app: &App, theme: &Theme) -> Layout {
        let chat = crate::chat::available(app);
        layout(
            size(),
            theme,
            app.workspace.visible,
            false,
            chat && app.chat.visible,
            chat,
        )
    }

    fn theme() -> Theme {
        Theme::light()
    }

    fn size() -> Size {
        Size::new(1120.0, 760.0)
    }

    fn click(app: &mut App, point: Point) {
        let theme = theme();
        let text = TextEngine::none();
        handle_events(
            app,
            &[Event::PointerButton(PointerEvent {
                position: point,
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                modifiers: Default::default(),
            })],
            size(),
            &theme,
            &text,
        );
    }

    fn right_click(app: &mut App, point: Point) {
        let theme = theme();
        let text = TextEngine::none();
        handle_events(
            app,
            &[Event::PointerButton(PointerEvent {
                position: point,
                button: PointerButton::Right,
                state: ButtonState::Pressed,
                modifiers: Default::default(),
            })],
            size(),
            &theme,
            &text,
        );
    }

    fn tree_app() -> (std::path::PathBuf, App) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-editor-tree-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Test\n").unwrap();
        let dir = dir.canonicalize().unwrap();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        (dir, app)
    }

    fn row_index(app: &App, name: &str) -> usize {
        app.workspace
            .rows
            .iter()
            .position(|row| row.name == name)
            .expect("row in tree")
    }

    fn menu_panel(app: &App) -> Rect {
        let theme = theme();
        let text = TextEngine::none();
        let tree = app.tree_menu.as_ref().unwrap();
        let width = tree.menu.width(&text, &theme);
        tree.menu
            .place(tree.anchor, width, Rect::from_size(size()), &theme)
    }

    fn menu_item(app: &App, entry: usize) -> Rect {
        let theme = theme();
        app.tree_menu
            .as_ref()
            .unwrap()
            .menu
            .rows(menu_panel(app), &theme)
            .into_iter()
            .find(|row| row.entry == entry)
            .unwrap()
            .rect
    }

    fn two_docs() -> App {
        let mut app = App::new();
        app.new_file();
        app
    }

    #[test]
    fn clicking_a_tab_switches_document() {
        let mut app = two_docs();
        app.select_tab(0);
        let theme = theme();
        let text = TextEngine::none();
        let lay = test_layout(&app, &theme);
        let segments = app.tabs.segments(lay.tabs, &text, &theme);
        assert_eq!(segments.len(), 2);
        click(&mut app, segments[1].center());
        assert_eq!(app.active, 1);
        assert_eq!(app.tabs.selected(), 1);
    }

    #[test]
    fn clicking_a_tab_close_button_closes_it() {
        let mut app = two_docs();
        let theme = theme();
        let text = TextEngine::none();
        let lay = test_layout(&app, &theme);
        let segments = app.tabs.segments(lay.tabs, &text, &theme);
        let metrics = theme.metrics.tabs;
        let seg = segments[1];
        let close = Point::new(
            seg.right() - metrics.pad_x / 2.0 - metrics.close / 2.0,
            seg.center().y,
        );
        click(&mut app, close);
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.active, 0);
    }

    #[test]
    fn header_drag_starts_window_move_on_tabs_and_empty_space() {
        let mut app = App::new();
        let theme = theme();
        let text = TextEngine::none();
        let lay = test_layout(&app, &theme);
        let seg = app.tabs.segments(lay.tabs, &text, &theme)[0];
        click(&mut app, seg.center());
        assert!(app.pending.begin_move);

        app.pending.begin_move = false;
        click(
            &mut app,
            Point::new(lay.tabs.right() - 4.0, lay.tabs.center().y),
        );
        assert!(app.pending.begin_move);
    }

    fn chat_app() -> App {
        let mut app = App::new();
        app.plugins.plugins = vec![crate::plugins::Plugin {
            dir: std::path::PathBuf::from("/tmp/ai-test"),
            id: "ai-test".into(),
            name: "AI Test".into(),
            version: "1".into(),
            author: String::new(),
            description: String::new(),
            entry: "run.sh".into(),
            interpreter: Some("sh".into()),
            commands: Vec::new(),
            settings: Vec::new(),
            chat: true,
        }];
        app.chat.visible = true;
        app
    }

    #[test]
    fn dropping_a_file_opens_it_in_a_tab() {
        let dir = std::env::temp_dir().join("luna-editor-drop-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("upuszczony.txt");
        std::fs::write(&path, "treść\n").unwrap();

        let mut app = App::new();
        let theme = theme();
        let text = TextEngine::none();
        handle_events(
            &mut app,
            &[Event::Drop {
                position: Point::new(10.0, 10.0),
                paths: vec![path.clone()],
                copy: true,
            }],
            size(),
            &theme,
            &text,
        );
        assert!(app
            .docs
            .iter()
            .any(|doc| doc.path.as_deref() == Some(path.as_path())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chat_panel_shrinks_the_editor_and_opens_settings() {
        let mut app = chat_app();
        let theme = theme();
        let lay = test_layout(&app, &theme);
        assert!(lay.chat.width() > 0.0);
        assert!(lay.editor.right() <= lay.chat.x());
        assert!(lay.chat_ai.width() > 0.0);
        let mut scene = Scene::new();
        let text = TextEngine::none();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        crate::chat::paint(&mut app, &mut ctx, &lay);
        click(&mut app, lay.chat_gear.center());
        assert!(app.plugin_ui.form.is_some());
    }

    #[test]
    fn header_button_toggles_workspace() {
        let mut app = App::new();
        let theme = theme();
        let lay = test_layout(&app, &theme);
        assert!(app.workspace.visible);
        click(&mut app, lay.workspace_toggle.center());
        assert!(!app.workspace.visible);
        let hidden = test_layout(&app, &theme);
        assert_eq!(hidden.workspace.width(), 0.0);
        let pad = theme.spacing.md.min((size().width * 0.04).max(8.0));
        assert_eq!(hidden.editor.x(), pad);
        assert_eq!(hidden.editor.right(), size().width - pad);
        click(&mut app, lay.workspace_toggle.center());
        assert!(app.workspace.visible);
    }

    #[test]
    fn layout_does_not_overlap_at_common_sizes() {
        for size in [Size::new(1120.0, 760.0), Size::new(700.0, 500.0)] {
            for workspace_visible in [false, true] {
                for find_open in [false, true] {
                    let theme = theme();
                    let lay = layout(size, &theme, workspace_visible, find_open, false, false);
                    assert!(lay.header.bottom() <= lay.editor.y());
                    assert!(lay.editor.bottom() <= lay.status.y());
                    assert!(lay.status.bottom() <= size.height);
                    assert!(lay.editor.right() <= size.width);
                    assert!(lay.tabs.right() <= lay.menu.x());
                    assert!(lay.workspace_toggle.right() <= lay.tabs.x());
                    if workspace_visible {
                        assert!(lay.workspace.right() <= lay.editor.x());
                        assert!(lay.workspace.bottom() <= lay.status.y());
                        assert!(lay.workspace_tree.bottom() <= lay.workspace_preview.y());
                    } else {
                        assert_eq!(lay.workspace.width(), 0.0);
                    }
                    if find_open {
                        assert!(lay.find_bar.bottom() <= lay.editor.y());
                        assert!(lay.find_bar.x() >= lay.workspace.right());
                        assert!(lay.find_entry.right() <= lay.find_close.x());
                    } else {
                        assert_eq!(lay.find_bar.height(), 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn wheel_direction_matches_the_file_manager() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-editor-wheel-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for index in 0..60 {
            std::fs::write(dir.join(format!("file-{index:02}.txt")), "x").unwrap();
        }
        let theme = theme();
        let text = TextEngine::none();
        let mut app = App::new();
        app.workspace.set_root(dir.clone());
        let lay = test_layout(&app, &theme);
        let tree_point = Point::new(lay.workspace_tree.x() + 10.0, lay.workspace_tree.y() + 40.0);
        handle_events(
            &mut app,
            &[Event::Scroll(ScrollEvent {
                position: tree_point,
                dx: 0.0,
                dy: 10.0,
            })],
            size(),
            &theme,
            &text,
        );
        let down = app.workspace.scroll;
        assert!(down > 0.0, "rolka w dół przewija drzewo w dół");
        handle_events(
            &mut app,
            &[Event::Scroll(ScrollEvent {
                position: tree_point,
                dx: 0.0,
                dy: -10.0,
            })],
            size(),
            &theme,
            &text,
        );
        assert!(app.workspace.scroll < down);

        let long = "linia\n".repeat(200);
        let mut app = App::with_text(None, &long);
        app.reveal_cursor = false;
        let lay = test_layout(&app, &theme);
        handle_events(
            &mut app,
            &[Event::Scroll(ScrollEvent {
                position: lay.editor.center(),
                dx: 0.0,
                dy: 10.0,
            })],
            size(),
            &theme,
            &text,
        );
        assert!(app.view().vertical_offset() > 0.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_dialog_applies_changes() {
        use crate::app::MENU_SETTINGS;
        let mut app = App::new();
        app.menu_action(MENU_SETTINGS);
        assert!(app.settings_open);
        let theme = theme();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size(), &theme, &TextEngine::none());
        let lay = settings_layout(size(), &theme);
        click(
            &mut app,
            Point::new(
                lay.tab_width.x() + lay.tab_width.width() / 6.0,
                lay.tab_width.center().y,
            ),
        );
        assert_eq!(app.prefs.tab_width, 2);
        assert_eq!(app.doc().view.tab_width(), 2);

        click(&mut app, lay.auto_indent.center());
        assert!(!app.prefs.auto_indent);
        click(&mut app, lay.show_hidden.center());
        assert!(app.prefs.show_hidden);
        click(&mut app, lay.skip_build.center());
        assert!(!app.prefs.skip_build_dirs);
        click(&mut app, lay.show_workspace.center());
        assert!(!app.prefs.show_workspace);
        assert!(!app.workspace.visible);

        click(&mut app, lay.done.center());
        assert!(!app.settings_open);
        assert!(settings_panel(size(), &theme).width() > 0.0);
    }

    #[test]
    fn paint_handles_all_panels() {
        let mut app = App::new();
        let theme = theme();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size(), &theme, &text);
        app.toggle_workspace();
        app.toggle_workspace();
        app.toggle_find();
        paint(&mut app, &mut scene, size(), &theme, &text);
        app.show_about();
        paint(&mut app, &mut scene, size(), &theme, &text);
    }

    #[test]
    fn right_click_on_a_tree_row_opens_the_context_menu() {
        let (dir, mut app) = tree_app();
        let theme = theme();
        let lay = test_layout(&app, &theme);
        let index = row_index(&app, "src");
        let anchor = workspace_row_anchor(&app, &lay, index).unwrap();
        right_click(&mut app, anchor.center());
        assert!(app.tree_menu.is_some());
        assert_eq!(app.workspace.selected, Some(dir.join("src")));
        assert!(app.workspace_focused);

        // Klik poza panelem zamyka menu bez akcji.
        right_click(
            &mut app,
            Point::new(size().width - 3.0, size().height - 3.0),
        );
        assert!(app.tree_menu.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_menu_new_file_starts_the_name_prompt() {
        let (dir, mut app) = tree_app();
        let theme = theme();
        let lay = test_layout(&app, &theme);
        let index = row_index(&app, "README.md");
        let anchor = workspace_row_anchor(&app, &lay, index).unwrap();
        right_click(&mut app, anchor.center());

        let item = menu_item(&app, TREE_NEW_FILE);
        click(&mut app, item.center());
        assert!(app.tree_menu.is_none());
        assert_eq!(app.prompt.as_ref().unwrap().kind, PromptKind::NewFile);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn context_menu_delete_asks_before_removing() {
        let (dir, mut app) = tree_app();
        let theme = theme();
        let text = TextEngine::none();
        let lay = test_layout(&app, &theme);
        let index = row_index(&app, "README.md");
        let anchor = workspace_row_anchor(&app, &lay, index).unwrap();
        right_click(&mut app, anchor.center());

        let item = menu_item(&app, TREE_DELETE);
        click(&mut app, item.center());
        assert!(app.pending_delete.is_some());

        let confirm = {
            let (_, dialog) = app.pending_delete.as_ref().unwrap();
            dialog.rects(size(), &theme, &text).buttons[0].center()
        };
        click(&mut app, confirm);
        assert!(app.pending_delete.is_none());
        assert!(!dir.join("README.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_can_be_cancelled_with_the_button() {
        let (dir, mut app) = tree_app();
        let theme = theme();
        let text = TextEngine::none();
        app.begin_prompt(PromptKind::NewFile, dir.clone());
        let label = app.prompt.as_ref().unwrap().kind.submit_label();
        let lay = prompt_layout(size(), &theme, &text, &label);
        click(&mut app, lay.cancel.center());
        assert!(app.prompt.is_none());
        assert!(dir.join("README.md").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paint_handles_tree_menu_and_prompt() {
        let (dir, mut app) = tree_app();
        let theme = theme();
        let text = TextEngine::none();
        let lay = test_layout(&app, &theme);
        let index = row_index(&app, "src");
        let anchor = workspace_row_anchor(&app, &lay, index).unwrap();
        app.open_tree_menu(index, anchor);
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size(), &theme, &text);
        app.tree_menu = None;
        app.begin_prompt(PromptKind::NewFolder, dir.clone());
        paint(&mut app, &mut scene, size(), &theme, &text);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
