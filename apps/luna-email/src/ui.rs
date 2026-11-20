//! Rysowanie i obsługa wejścia klienta poczty.
//!
//! Układ liczymy w jednym miejscu ([`layout`]), więc trafienia klawiszy i myszy
//! nie mogą się rozjechać z pikselami. Widok jest trójdzielny: foldery, lista
//! wiadomości, podgląd — przy węższym oknie pasek boczny znika pierwszy.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Event, IconTheme, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
    WindowEvent,
};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, paint_icon_themed, wrap_text, Avatar, Button, ButtonVariant, ControlsSide,
    HeaderBar, IconName, PaintCtx, WindowControl,
};

use crate::app::{sender_initials, AccountField, App, ComposeField, Hit, View};
use crate::model::{format_full_date, format_when, FolderKind};

/// Wysokość wiersza listy wiadomości (wspólna z przewijaniem i trafieniami).
pub const MESSAGE_ROW_HEIGHT: f64 = 74.0;
const FOLDER_ROW_HEIGHT: f64 = 34.0;
const SIDEBAR_WIDTH: f64 = 220.0;
const FOOTER_HEIGHT: f64 = 26.0;
const BUTTON: f64 = 28.0;
const PAD: f64 = 12.0;

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_TAB: u32 = 0xff09;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_DELETE: u32 = 0xffff;
const KEY_UP: u32 = 0xff52;
const KEY_DOWN: u32 = 0xff54;
const KEY_LEFT: u32 = 0xff51;
const KEY_RIGHT: u32 = 0xff53;
const KEY_HOME: u32 = 0xff50;
const KEY_END: u32 = 0xff57;
const KEY_PAGE_UP: u32 = 0xff55;
const KEY_PAGE_DOWN: u32 = 0xff56;
const KEY_F1: u32 = 0xffbe;
const KEY_F5: u32 = 0xffc2;
const KEY_A: u32 = 0x61;
const KEY_C: u32 = 0x63;
const KEY_F: u32 = 0x66;
const KEY_N: u32 = 0x6e;
const KEY_Q: u32 = 0x71;
const KEY_R: u32 = 0x72;
const KEY_S: u32 = 0x73;
const KEY_U: u32 = 0x75;
const KEY_V: u32 = 0x76;
const KEY_X: u32 = 0x78;

/// Geometria okna.
#[derive(Debug, Clone, Copy)]
struct Layout {
    header: Rect,
    sidebar: Option<Rect>,
    list: Rect,
    preview: Rect,
    footer: Rect,
    compose_btn: Rect,
    reply_btn: Rect,
    forward_btn: Rect,
    refresh_btn: Rect,
    search_btn: Rect,
    menu_btn: Rect,
    sidebar_btn: Rect,
    search_field: Option<Rect>,
    /// Obszar treści poniżej paska tytułu (formularze, podgląd).
    content: Rect,
}

fn layout(app: &App, size: Size, theme: &Theme) -> Layout {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let bar = HeaderBar::new().controls(ControlsSide::Right);
    let available = bar.controls_layout(header, theme).content;

    let mut cursor = available.right();
    let mut take = || {
        let rect = Rect::from_xywh(
            cursor - BUTTON,
            header.center().y - BUTTON / 2.0,
            BUTTON,
            BUTTON,
        );
        cursor -= BUTTON + 4.0;
        rect
    };
    let menu_btn = take();
    let search_btn = take();
    let refresh_btn = take();
    let forward_btn = take();
    let reply_btn = take();
    let compose_btn = take();
    let sidebar_btn = Rect::from_xywh(
        available.x(),
        header.center().y - BUTTON / 2.0,
        BUTTON,
        BUTTON,
    );

    let search_field = app.search_active.then(|| {
        let x = sidebar_btn.right() + 6.0;
        let width = (compose_btn.x() - 8.0 - x).max(0.0);
        Rect::from_xywh(
            x,
            header.center().y - theme.metrics.entry.height / 2.0,
            width,
            theme.metrics.entry.height,
        )
    });

    let footer = Rect::from_xywh(0.0, size.height - FOOTER_HEIGHT, size.width, FOOTER_HEIGHT);
    let content_y = header.height();
    let content_height = (footer.y() - content_y).max(0.0);
    let content = Rect::from_xywh(0.0, content_y, size.width, content_height);

    let sidebar_width = if app.sidebar_visible {
        if size.width >= 860.0 {
            SIDEBAR_WIDTH
        } else {
            // Wąskie okno: pasek boczny chowa się nad treścią (drawer),
            // bo inaczej nie byłoby jak przełączyć folderu.
            SIDEBAR_WIDTH.min((size.width * 0.72).max(180.0))
        }
    } else {
        0.0
    };
    let sidebar_overlay = app.sidebar_visible && size.width < 860.0;
    let sidebar = (sidebar_width > 0.0)
        .then(|| Rect::from_xywh(0.0, content_y, sidebar_width, content_height));
    let list_x = if sidebar_overlay { 0.0 } else { sidebar_width };
    let after_sidebar = size.width - list_x;
    let list_width = if after_sidebar < 620.0 {
        (after_sidebar * 0.52).max(200.0)
    } else {
        (after_sidebar * 0.34).clamp(300.0, 400.0)
    };
    let list = Rect::from_xywh(list_x, content_y, list_width, content_height);
    let preview = Rect::from_xywh(
        list_x + list_width,
        content_y,
        (after_sidebar - list_width).max(0.0),
        content_height,
    );

    Layout {
        header,
        sidebar,
        list,
        preview,
        footer,
        compose_btn,
        reply_btn,
        forward_btn,
        refresh_btn,
        search_btn,
        menu_btn,
        sidebar_btn,
        search_field,
        content,
    }
}

/// Rysuje całą klatkę.
pub fn paint(app: &mut App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    app.hits.clear();
    let lay = layout(app, size, theme);
    scene.fill_rect(Rect::from_size(size), theme.colors.window_bg);

    let mut ctx = PaintCtx::new(scene, theme, text);
    paint_header(app, &mut ctx, &lay, size);
    match app.view {
        View::Mail => {
            // Kolejność ma znaczenie: przy wąskim oknie pasek boczny jest
            // rysowany nad treścią (drawer), więc musi iść na końcu.
            paint_list(app, &mut ctx, &lay);
            paint_preview(app, &mut ctx, &lay, size);
            paint_sidebar(app, &mut ctx, &lay);
            paint_footer(app, &mut ctx, &lay);
        }
        View::Compose => paint_compose(app, &mut ctx, &lay, size),
        View::Account => paint_account(app, &mut ctx, &lay, size),
        View::About => paint_about(app, &mut ctx, &lay, size),
    }

    if app.menu_button.is_open() && app.view == View::Mail {
        let panel = menu_panel(app, lay.menu_btn, size, theme, text);
        app.menu.paint(&mut ctx, panel);
    }
    if app.account_menu_open && app.view == View::Mail {
        if let Some(panel) = account_menu_panel(app, &lay, size, theme, text) {
            app.account_menu.paint(&mut ctx, panel);
        }
    }
    paint_toast(app, &mut ctx, size);
}

fn menu_panel(app: &App, anchor: Rect, size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    let width = app.menu.width(text, theme).max(232.0);
    app.menu.place(anchor, width, Rect::from_size(size), theme)
}

/// Obszar kliknięcia wiersza konta w pasku bocznym.
fn account_anchor(lay: &Layout) -> Option<Rect> {
    lay.sidebar.map(|sidebar| {
        Rect::from_xywh(
            sidebar.x() + 6.0,
            sidebar.y() + PAD - 4.0,
            sidebar.width() - 12.0,
            44.0,
        )
    })
}

fn account_menu_panel(
    app: &App,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) -> Option<Rect> {
    let anchor = account_anchor(lay)?;
    let width = app.account_menu.width(text, theme).max(240.0);
    Some(
        app.account_menu
            .place(anchor, width, Rect::from_size(size), theme),
    )
}

fn paint_header(app: &mut App, ctx: &mut PaintCtx, lay: &Layout, _size: Size) {
    let bar = HeaderBar::new().controls(ControlsSide::Right);
    bar.paint(ctx, lay.header, app.controls_hover);

    // Lewa strona: przełącznik paska bocznego + tytuł/aktywne konto.
    let colors = ctx.theme.colors;
    icon_button(ctx, lay.sidebar_btn, IconName::List, true, false);
    if lay.search_field.is_none() {
        let x = lay.sidebar_btn.right() + 8.0;
        let right = lay.compose_btn.x() - 8.0;
        let title = app
            .account()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| tr("Luna Mail").to_owned());
        ctx.label_left_fit(
            x,
            lay.header.center().y,
            &title,
            ctx.theme.typography.body.size,
            weight::BOLD,
            colors.text,
            (right - x).max(0.0),
        );
    }

    if let Some(field) = lay.search_field {
        app.search.paint(ctx, field);
    }

    let can_compose = app.view == View::Mail;
    icon_button(ctx, lay.compose_btn, IconName::Edit, can_compose, false);
    icon_button(
        ctx,
        lay.reply_btn,
        IconName::ChevronLeft,
        can_compose && app.selected_message().is_some(),
        false,
    );
    icon_button(
        ctx,
        lay.forward_btn,
        IconName::ChevronRight,
        can_compose && app.selected_message().is_some(),
        false,
    );
    icon_button(
        ctx,
        lay.refresh_btn,
        IconName::Refresh,
        app.view == View::Mail,
        false,
    );
    icon_button(
        ctx,
        lay.search_btn,
        IconName::Search,
        app.view == View::Mail,
        false,
    );
    icon_button(ctx, lay.menu_btn, IconName::Menu, true, false);
}

fn icon_button(ctx: &mut PaintCtx, rect: Rect, icon: IconName, enabled: bool, _hovered: bool) {
    let color = if enabled {
        ctx.theme.colors.text
    } else {
        ctx.theme.colors.dim_text.scale_alpha(0.45)
    };
    let inner = rect.deflate(Insets::symmetric(6.0, 6.0));
    // Pasek tytułu Luny jest monochromatyczny — pastelowe ikony zostają
    // w treści, ale kontrolki nie mogą wyglądać jak z innego systemu.
    paint_icon_themed(ctx, icon, inner, color, IconTheme::Monochrome);
}

fn paint_sidebar(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let Some(sidebar) = lay.sidebar else {
        return;
    };
    let colors = ctx.theme.colors;
    ctx.scene.fill_rect(sidebar, colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(sidebar.right() - 1.0, sidebar.y(), 1.0, sidebar.height()),
        colors.border,
    );

    // Konto na górze paska (avatar + adres). Klik otwiera listę kont.
    let account_row = account_anchor(lay).unwrap_or(Rect::ZERO);
    if app.account_menu_open {
        ctx.scene
            .fill_rounded_rect(account_row, ctx.theme.radius.small, colors.shade);
    }
    let avatar_rect = Rect::from_xywh(sidebar.x() + PAD, sidebar.y() + PAD, 32.0, 32.0);
    let label = app
        .account()
        .map(|account| account.label())
        .unwrap_or_else(|| tr("Demo mode").to_owned());
    Avatar::new(sender_initials_label(app)).paint(ctx, avatar_rect);
    let chevron = Rect::from_xywh(
        sidebar.right() - PAD - 14.0,
        avatar_rect.center().y - 7.0,
        14.0,
        14.0,
    );
    ctx.label_left_fit(
        avatar_rect.right() + 8.0,
        avatar_rect.center().y,
        &label,
        ctx.theme.typography.body.size,
        weight::NORMAL,
        colors.text,
        (chevron.x() - 6.0 - (avatar_rect.right() + 8.0)).max(0.0),
    );
    paint_icon_themed(
        ctx,
        IconName::ChevronRight,
        chevron,
        colors.dim_text,
        IconTheme::Monochrome,
    );
    app.hits.push((Hit::AccountMenu, account_row));
    let separator_y = avatar_rect.bottom() + PAD;
    ctx.scene.fill_rect(
        Rect::from_xywh(
            sidebar.x() + PAD,
            separator_y,
            sidebar.width() - 2.0 * PAD,
            1.0,
        ),
        colors.border,
    );

    let mut y = separator_y + 6.0;
    for (index, folder) in app.folders.iter().enumerate() {
        let row = Rect::from_xywh(
            sidebar.x() + 6.0,
            y,
            sidebar.width() - 12.0,
            FOLDER_ROW_HEIGHT,
        );
        let selected = index == app.folder_index;
        if selected {
            ctx.scene.fill_rounded_rect(
                row,
                ctx.theme.radius.small,
                colors.accent.scale_alpha(0.16),
            );
        }
        let fg = if selected { colors.accent } else { colors.text };
        let icon = folder_icon(folder.kind);
        let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - 8.0, 16.0, 16.0);
        paint_icon(ctx, icon, icon_rect, fg);
        let count = if index == app.folder_index {
            app.messages.iter().filter(|item| !item.seen).count()
        } else {
            folder.unread
        };
        let right = row.right() - 8.0;
        let count_w = if count > 0 {
            let text = count.to_string();
            ctx.label_right(
                right,
                row.center().y,
                &text,
                ctx.theme.typography.caption.size,
                weight::BOLD,
                if selected {
                    colors.accent
                } else {
                    colors.dim_text
                },
            );
            ctx.measure(&text, ctx.theme.typography.caption.size, weight::BOLD) + 8.0
        } else {
            0.0
        };
        ctx.label_left_fit(
            icon_rect.right() + 8.0,
            row.center().y,
            &folder.name,
            ctx.theme.typography.body.size,
            if selected {
                weight::BOLD
            } else {
                weight::NORMAL
            },
            fg,
            (right - count_w - (icon_rect.right() + 8.0)).max(0.0),
        );
        app.hits.push((Hit::Folder(index), row));
        y += FOLDER_ROW_HEIGHT;
    }
}

fn sender_initials_label(app: &App) -> String {
    app.account()
        .map(|account| {
            if account.display_name.trim().is_empty() {
                account.email.clone()
            } else {
                account.display_name.clone()
            }
        })
        .map(|name| crate::model::initials(&name))
        .unwrap_or_else(|| "DM".to_owned())
}

fn folder_icon(kind: FolderKind) -> IconName {
    match kind {
        FolderKind::Inbox => IconName::Mail,
        FolderKind::Sent => IconName::ChevronRight,
        FolderKind::Drafts => IconName::Edit,
        FolderKind::Trash => IconName::Close,
        FolderKind::Archive => IconName::Archive,
        FolderKind::Custom => IconName::Document,
    }
}

fn paint_list(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let list = lay.list;
    if list.width() <= 0.0 {
        return;
    }
    let colors = ctx.theme.colors;
    ctx.scene.fill_rect(list, colors.view_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(list.right() - 1.0, list.y(), 1.0, list.height()),
        colors.border,
    );

    let rows_h = app.visible.len() as f64 * MESSAGE_ROW_HEIGHT;
    let viewport_h = (list.height() - 12.0).max(0.0);
    app.list_viewport = viewport_h;
    let max_scroll = (rows_h - viewport_h).max(0.0);
    app.list_scroll = app.list_scroll.clamp(0.0, max_scroll);

    if app.visible.is_empty() {
        let title = if app.loading {
            tr("Loading messages…")
        } else if app.search.is_empty() {
            tr("No messages in this folder")
        } else {
            tr("Nothing matches your search")
        };
        ctx.scene.push_clip(list);
        let center = Point::new(list.center().x, list.center().y - 10.0);
        paint_icon(
            ctx,
            IconName::Mail,
            Rect::from_xywh(center.x - 18.0, center.y - 44.0, 36.0, 36.0),
            colors.dim_text.scale_alpha(0.6),
        );
        ctx.label_center(
            center.x,
            center.y,
            &title,
            ctx.theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
        ctx.scene.pop_clip();
        return;
    }

    ctx.scene.push_clip(list);
    let top = list.y() + 6.0;
    for (visible_index, message_index) in app.visible.iter().enumerate() {
        let y = top + visible_index as f64 * MESSAGE_ROW_HEIGHT - app.list_scroll;
        let row = Rect::from_xywh(list.x(), y, list.width(), MESSAGE_ROW_HEIGHT);
        if row.bottom() < list.y() || row.y() > list.bottom() {
            continue;
        }
        let Some(message) = app.messages.get(*message_index) else {
            continue;
        };
        let selected = visible_index == app.selected;
        let hovered = app
            .pointer
            .is_some_and(|point| row.contains(point) && point.x < list.right() - 8.0);

        if selected {
            ctx.scene.fill_rect(row, colors.accent.scale_alpha(0.16));
            ctx.scene.fill_rect(
                Rect::from_xywh(row.x(), row.y(), 3.0, row.height()),
                colors.accent,
            );
        } else if hovered {
            ctx.scene.fill_rect(row, colors.shade);
        }

        // Kropka nieprzeczytanej wiadomości.
        if !message.seen {
            ctx.scene.fill_circle(
                Point::new(row.x() + 12.0, row.y() + 24.0),
                3.5,
                colors.accent,
            );
        }

        let avatar_rect = Rect::from_xywh(row.x() + 22.0, row.y() + 14.0, 30.0, 30.0);
        Avatar::new(sender_initials(message)).paint(ctx, avatar_rect);

        let text_x = avatar_rect.right() + 10.0;
        let right = row.right() - 10.0;
        let when = format_when(message.date, crate::model::now_secs());
        let when_w = ctx.measure(&when, ctx.theme.typography.caption.size, weight::NORMAL);
        ctx.label_right(
            right,
            row.y() + 18.0,
            &when,
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        ctx.label_left_fit(
            text_x,
            row.y() + 18.0,
            message.sender(),
            ctx.theme.typography.body.size,
            if message.seen {
                weight::NORMAL
            } else {
                weight::BOLD
            },
            if message.seen {
                colors.text
            } else {
                colors.text
            },
            (right - when_w - 8.0 - text_x).max(0.0),
        );

        // Ikony po prawej stronie tematu: gwiazdka i spinacz.
        let mut icon_right = right;
        let caption = ctx.theme.typography.caption.size;
        if message.has_attachment {
            let rect = Rect::from_xywh(icon_right - 14.0, row.y() + 34.0, 14.0, 14.0);
            paint_icon(ctx, IconName::Document, rect, colors.dim_text);
            icon_right -= 18.0;
        }
        if message.flagged {
            let rect = Rect::from_xywh(icon_right - 14.0, row.y() + 34.0, 14.0, 14.0);
            paint_icon(ctx, IconName::Star, rect, colors.warning);
            icon_right -= 18.0;
        }

        ctx.label_left_fit(
            text_x,
            row.y() + 39.0,
            message.subject_or_placeholder(),
            ctx.theme.typography.body.size,
            if message.seen {
                weight::NORMAL
            } else {
                weight::BOLD
            },
            colors.text,
            (icon_right - text_x).max(0.0),
        );
        let snippet = if message.snippet.is_empty() {
            String::new()
        } else {
            message.snippet.clone()
        };
        ctx.label_left_fit(
            text_x,
            row.y() + 58.0,
            &snippet,
            caption,
            weight::NORMAL,
            colors.dim_text,
            (right - text_x).max(0.0),
        );

        ctx.scene.fill_rect(
            Rect::from_xywh(row.x() + 10.0, row.bottom() - 1.0, row.width() - 20.0, 1.0),
            colors.border.scale_alpha(0.6),
        );
        app.hits.push((Hit::Message(visible_index), row));
    }
    ctx.scene.pop_clip();
    paint_scrollbar(ctx, list, app.list_scroll, rows_h, viewport_h);
}

fn paint_preview(app: &mut App, ctx: &mut PaintCtx, lay: &Layout, _size: Size) {
    let preview = lay.preview;
    if preview.width() <= 40.0 {
        return;
    }
    let colors = ctx.theme.colors;
    ctx.scene.fill_rect(preview, colors.view_bg);

    let Some(message) = app.selected_message().cloned() else {
        let center = preview.center();
        ctx.scene.push_clip(preview);
        paint_icon(
            ctx,
            IconName::Mail,
            Rect::from_xywh(center.x - 22.0, center.y - 70.0, 44.0, 44.0),
            colors.dim_text.scale_alpha(0.5),
        );
        ctx.label_center(
            center.x,
            center.y - 20.0,
            &tr("Select a message"),
            ctx.theme.typography.heading.size,
            weight::BOLD,
            colors.dim_text,
        );
        ctx.label_center(
            center.x,
            center.y + 4.0,
            &tr("The message body will appear here"),
            ctx.theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
        if app.demo {
            let label = tr("Configure an account to use real mail");
            let button = Button::new(label).variant(ButtonVariant::Suggested);
            let w = button.width(ctx.text, ctx.theme);
            let h = button.height(ctx.theme);
            let rect = Rect::from_xywh(center.x - w / 2.0, center.y + 34.0, w, h);
            button.paint(ctx, rect);
            app.hits.push((Hit::Configure, rect));
        }
        ctx.scene.pop_clip();
        return;
    };

    // Nagłówek podglądu.
    let inner_x = preview.x() + PAD + 4.0;
    let inner_right = preview.right() - PAD - 4.0;
    let mut y = preview.y() + PAD;
    ctx.label_left_fit(
        inner_x,
        y + 10.0,
        message.subject_or_placeholder(),
        ctx.theme.typography.heading.size,
        weight::BOLD,
        colors.text,
        (inner_right - inner_x).max(0.0),
    );
    y += 26.0;

    let avatar_rect = Rect::from_xywh(inner_x, y, 30.0, 30.0);
    Avatar::new(sender_initials(&message)).paint(ctx, avatar_rect);
    let sender = if message.from_addr.is_empty() {
        message.sender().to_owned()
    } else {
        format!("{} <{}>", message.sender(), message.from_addr)
    };
    // Data po prawej zabiera miejsce — nadawca nie może wejść jej pod spód.
    let when = format_full_date(message.date);
    let when_w = ctx.measure(&when, ctx.theme.typography.caption.size, weight::NORMAL);
    let text_width = (inner_right - when_w - 10.0 - (avatar_rect.right() + 8.0)).max(0.0);
    ctx.label_left_fit(
        avatar_rect.right() + 8.0,
        y + 9.0,
        &sender,
        ctx.theme.typography.body.size,
        weight::BOLD,
        colors.text,
        text_width,
    );
    let to = if message.to_addr.is_empty() {
        String::new()
    } else {
        format!("{}: {}", tr("To"), message.to_addr)
    };
    ctx.label_left_fit(
        avatar_rect.right() + 8.0,
        y + 24.0,
        &to,
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
        text_width,
    );
    ctx.label_right(
        inner_right,
        y + 9.0,
        &when,
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    y += 38.0;

    // Przyciski odpowiedzi i przekazania.
    let reply = Button::new(tr("Reply")).variant(ButtonVariant::Flat);
    let forward = Button::new(tr("Forward")).variant(ButtonVariant::Flat);
    let h = reply.height(ctx.theme);
    let forward_w = forward.width(ctx.text, ctx.theme);
    let reply_w = reply.width(ctx.text, ctx.theme);
    let forward_rect = Rect::from_xywh(inner_right - forward_w, y, forward_w, h);
    let reply_rect = Rect::from_xywh(forward_rect.x() - 6.0 - reply_w, y, reply_w, h);
    reply.paint(ctx, reply_rect);
    forward.paint(ctx, forward_rect);
    app.hits.push((Hit::Reply, reply_rect));
    app.hits.push((Hit::Forward, forward_rect));
    y += h + 10.0;

    ctx.scene.fill_rect(
        Rect::from_xywh(inner_x, y, (inner_right - inner_x).max(0.0), 1.0),
        colors.border,
    );
    y += 10.0;

    let body_rect = Rect::from_xywh(
        inner_x,
        y,
        (inner_right - inner_x).max(0.0),
        (preview.bottom() - 10.0 - y).max(0.0),
    );
    app.body_viewport = body_rect.height();

    if app.body_loading {
        ctx.label_left(
            body_rect.x(),
            body_rect.y() + 10.0,
            &tr("Loading message…"),
            ctx.theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
        return;
    }

    let body = app.open_body.clone().unwrap_or_default();
    let body_size = ctx.theme.typography.body.size;
    let line_height = ctx.theme.typography.body.line_height;
    let text_content = if body.text.trim().is_empty() {
        tr("This message has no plain-text content.").to_owned()
    } else {
        body.text.clone()
    };
    let lines = wrap_text(
        &text_content,
        body_rect.width(),
        body_size,
        weight::NORMAL,
        ctx.text,
    );
    let attachments_h = if body.attachments.is_empty() {
        0.0
    } else {
        26.0 + body.attachments.len() as f64 * 26.0
    };
    let content_h = lines.len() as f64 * line_height + attachments_h + 12.0;
    let max_scroll = (content_h - body_rect.height()).max(0.0);
    app.body_scroll = app.body_scroll.clamp(0.0, max_scroll);

    ctx.scene.push_clip(body_rect);
    let mut line_y = body_rect.y() - app.body_scroll;
    for line in &lines {
        if line_y + line_height >= body_rect.y() && line_y <= body_rect.bottom() {
            ctx.label_left(
                body_rect.x(),
                line_y + line_height * 0.5,
                line,
                body_size,
                weight::NORMAL,
                colors.text,
            );
        }
        line_y += line_height;
    }
    if !body.attachments.is_empty() {
        let mut attach_y = line_y + 8.0;
        ctx.label_left(
            body_rect.x(),
            attach_y + 6.0,
            &tr("Attachments"),
            ctx.theme.typography.caption.size,
            weight::BOLD,
            colors.dim_text,
        );
        attach_y += 24.0;
        for (index, attachment) in body.attachments.iter().enumerate() {
            let row = Rect::from_xywh(body_rect.x(), attach_y, body_rect.width(), 24.0);
            let icon = Rect::from_xywh(row.x(), row.y() + 5.0, 14.0, 14.0);
            paint_icon(ctx, IconName::Document, icon, colors.accent);
            let label = format!("{}  ·  {}", attachment.name, attachment.size_label());
            ctx.label_left_fit(
                icon.right() + 6.0,
                row.center().y,
                &label,
                ctx.theme.typography.caption.size,
                weight::NORMAL,
                colors.text,
                (row.right() - icon.right() - 6.0).max(0.0),
            );
            app.hits.push((Hit::Attachment(index), row));
            attach_y += 26.0;
        }
    }
    ctx.scene.pop_clip();
    paint_scrollbar(
        ctx,
        body_rect,
        app.body_scroll,
        content_h,
        body_rect.height(),
    );
}

fn paint_scrollbar(
    ctx: &mut PaintCtx,
    area: Rect,
    offset: f64,
    content_height: f64,
    viewport: f64,
) {
    if content_height <= viewport + 1.0 || viewport <= 0.0 {
        return;
    }
    let thickness = 6.0;
    let track = Rect::from_xywh(
        area.right() - thickness - 2.0,
        area.y() + 2.0,
        thickness,
        (area.height() - 4.0).max(0.0),
    );
    let ratio = (viewport / content_height).clamp(0.0, 1.0);
    let thumb_h = (track.height() * ratio).max(24.0);
    let max_offset = (content_height - viewport).max(1.0);
    let progress = (offset / max_offset).clamp(0.0, 1.0);
    let thumb_y = track.y() + (track.height() - thumb_h) * progress;
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(track.x(), thumb_y, thickness, thumb_h),
        thickness / 2.0,
        ctx.theme.colors.dim_text.scale_alpha(0.35),
    );
}

fn paint_footer(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let footer = lay.footer;
    let colors = ctx.theme.colors;
    ctx.scene.fill_rect(footer, colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(footer.x(), footer.y(), footer.width(), 1.0),
        colors.border,
    );
    let count = app.visible.len();
    let total = format!("{count} {}", tr("messages"));
    let status = if app.demo {
        format!(
            "{}  ·  {}  ·  {}",
            tr("Demo mode"),
            total,
            tr("Configure an account")
        )
    } else {
        let unread = app
            .messages
            .iter()
            .filter(|item| !item.seen)
            .count()
            .to_string();
        format!(
            "{}  ·  {}  ·  {} {}",
            app.account()
                .map(|account| account.email.clone())
                .unwrap_or_default(),
            total,
            unread,
            tr("unread")
        )
    };
    ctx.label_left_fit(
        footer.x() + PAD,
        footer.center().y,
        &status,
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
        (footer.width() - 2.0 * PAD).max(0.0),
    );
}

fn paint_compose(app: &mut App, ctx: &mut PaintCtx, lay: &Layout, size: Size) {
    let colors = ctx.theme.colors;
    let card = centered_card(lay.content, size, 700.0, 30.0);
    ctx.scene
        .fill_rounded_rect(card, ctx.theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, ctx.theme.radius.medium, 1.0, colors.border);

    let inner = card.deflate(Insets::symmetric(20.0, 16.0));
    ctx.label_left(
        inner.x(),
        inner.y() + 6.0,
        &tr("New message"),
        ctx.theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );

    let mut y = inner.y() + 30.0;
    let label_w = 66.0;
    let field_h = ctx.theme.metrics.entry.height.max(32.0);
    let input_x = inner.x() + label_w;
    let input_w = (inner.right() - input_x).max(0.0);

    let rows = [
        (tr("To:"), 0usize),
        (tr("Cc:"), 1usize),
        (tr("Subject:"), 2usize),
    ];
    for (label, index) in rows {
        ctx.label_left(
            inner.x(),
            y + field_h / 2.0,
            &label,
            ctx.theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
        let rect = Rect::from_xywh(input_x, y, input_w, field_h);
        match index {
            0 => app.compose.to.paint(ctx, rect),
            1 => app.compose.cc.paint(ctx, rect),
            _ => app.compose.subject.paint(ctx, rect),
        }
        let field = match index {
            0 => ComposeField::To,
            1 => ComposeField::Cc,
            _ => ComposeField::Subject,
        };
        app.hits.push((Hit::ComposeField(field), rect));
        y += field_h + 8.0;
    }

    if let Some(error) = app.compose.error {
        ctx.label_left(
            inner.x(),
            y + 4.0,
            &tr(error),
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.error,
        );
        y += 18.0;
    }

    y += 4.0;
    ctx.scene.fill_rect(
        Rect::from_xywh(inner.x(), y, inner.width(), 1.0),
        colors.border,
    );
    y += 8.0;

    // Pasek załączników i przyciski na dole.
    let buttons_h = ctx.theme.metrics.button.height;
    let attachments_h = if app.compose.attachments.is_empty() {
        0.0
    } else {
        24.0 + app.compose.attachments.len().min(4) as f64 * 22.0
    };
    let body_bottom = (inner.bottom() - buttons_h - 12.0 - attachments_h).max(y + 40.0);
    let body_rect = Rect::from_xywh(inner.x(), y, inner.width(), (body_bottom - y).max(0.0));
    app.compose_body_width = (body_rect.width() - 2.0 * ctx.theme.spacing.sm).max(0.0);
    app.compose.body.paint(ctx, body_rect);
    app.hits
        .push((Hit::ComposeField(ComposeField::Body), body_rect));
    y = body_bottom + 8.0;

    for (index, attachment) in app.compose.attachments.iter().take(4).enumerate() {
        let row = Rect::from_xywh(inner.x(), y, inner.width(), 20.0);
        let icon = Rect::from_xywh(row.x(), row.y() + 3.0, 14.0, 14.0);
        paint_icon(ctx, IconName::Document, icon, colors.dim_text);
        let label = format!("{}  ·  {}", attachment.name, attachment.size_label());
        ctx.label_left_fit(
            icon.right() + 6.0,
            row.center().y,
            &label,
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.text,
            (row.right() - 40.0 - icon.right()).max(0.0),
        );
        // „×” usuwa załącznik.
        let remove = Rect::from_xywh(row.right() - 18.0, row.y() + 2.0, 16.0, 16.0);
        paint_icon(
            ctx,
            IconName::Close,
            remove.deflate(Insets::symmetric(2.0, 2.0)),
            colors.dim_text,
        );
        app.hits.push((Hit::RemoveAttachment(index), remove));
        y += 22.0;
    }
    if app.compose.attachments.len() > 4 {
        ctx.label_left(
            inner.x(),
            y + 8.0,
            &tr_args(
                "+{n} more",
                &[("n", &(app.compose.attachments.len() - 4).to_string())],
            ),
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }

    // Przyciski.
    let send = Button::new(if app.compose.sending {
        tr("Sending…")
    } else {
        tr("Send")
    })
    .variant(ButtonVariant::Suggested)
    .enabled(!app.compose.sending);
    let cancel = Button::new(tr("Cancel"));
    let attach = Button::new(tr("Attach"));
    let y = inner.bottom() - buttons_h;
    let send_w = send.width(ctx.text, ctx.theme);
    let cancel_w = cancel.width(ctx.text, ctx.theme);
    let attach_w = attach.width(ctx.text, ctx.theme);
    let send_rect = Rect::from_xywh(inner.right() - send_w, y, send_w, buttons_h);
    let cancel_rect = Rect::from_xywh(send_rect.x() - 8.0 - cancel_w, y, cancel_w, buttons_h);
    let attach_rect = Rect::from_xywh(inner.x(), y, attach_w, buttons_h);
    attach.paint(ctx, attach_rect);
    cancel.paint(ctx, cancel_rect);
    send.paint(ctx, send_rect);
    app.hits.push((Hit::ComposeAttach, attach_rect));
    app.hits.push((Hit::ComposeCancel, cancel_rect));
    app.hits.push((Hit::ComposeSend, send_rect));

    ctx.label_left(
        attach_rect.right() + 10.0,
        y + buttons_h / 2.0,
        &tr("Ctrl+Enter sends · Esc returns"),
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
}

fn paint_account(app: &mut App, ctx: &mut PaintCtx, lay: &Layout, size: Size) {
    let colors = ctx.theme.colors;
    let card = centered_card(lay.content, size, 560.0, 24.0);
    // Formularz jest krótszy niż okno — nie rozciągamy karty na całą wysokość.
    let card = Rect::from_xywh(card.x(), card.y(), card.width(), card.height().min(470.0));
    ctx.scene
        .fill_rounded_rect(card, ctx.theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, ctx.theme.radius.medium, 1.0, colors.border);

    let inner = card.deflate(Insets::symmetric(20.0, 16.0));
    ctx.label_left(
        inner.x(),
        inner.y() + 6.0,
        &tr("Account settings"),
        ctx.theme.typography.heading.size,
        weight::BOLD,
        colors.text,
    );
    ctx.label_left(
        inner.x(),
        inner.y() + 28.0,
        &tr("Use an app password if your provider requires it"),
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );

    let field_h = ctx.theme.metrics.entry.height.max(32.0);
    let label_w = 142.0;
    let port_w = 64.0;
    let input_x = inner.x() + label_w;
    let input_w = (inner.right() - input_x).max(0.0);
    let mut y = inner.y() + 50.0;

    let row = |ctx: &mut PaintCtx, y: f64, label: &str| {
        ctx.label_left(
            inner.x(),
            y + field_h / 2.0,
            label,
            ctx.theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
    };

    row(ctx, y, &tr("E-mail address"));
    let email_rect = Rect::from_xywh(input_x, y, input_w, field_h);
    app.account_form.email.paint(ctx, email_rect);
    app.hits
        .push((Hit::AccountField(AccountField::Email), email_rect));
    y += field_h + 8.0;

    row(ctx, y, &tr("Display name"));
    let display_rect = Rect::from_xywh(input_x, y, input_w, field_h);
    app.account_form.display.paint(ctx, display_rect);
    app.hits
        .push((Hit::AccountField(AccountField::Display), display_rect));
    y += field_h + 8.0;

    row(ctx, y, &tr("Login"));
    let login_rect = Rect::from_xywh(input_x, y, input_w, field_h);
    app.account_form.login.paint(ctx, login_rect);
    app.hits
        .push((Hit::AccountField(AccountField::Login), login_rect));
    y += field_h + 8.0;

    row(ctx, y, &tr("Password"));
    let password_rect = Rect::from_xywh(input_x, y, input_w, field_h);
    app.account_form.password.paint(ctx, password_rect);
    app.hits
        .push((Hit::AccountField(AccountField::Password), password_rect));
    app.hits.push((
        Hit::PasswordPeek,
        app.account_form
            .password
            .peek_rect(password_rect, ctx.theme),
    ));
    y += field_h + 8.0;

    // IMAP: host + port.
    row(ctx, y, &tr("IMAP server"));
    let imap_host_rect = Rect::from_xywh(input_x, y, input_w - port_w - 8.0, field_h);
    let imap_port_rect = Rect::from_xywh(inner.right() - port_w, y, port_w, field_h);
    app.account_form.imap_host.paint(ctx, imap_host_rect);
    app.account_form.imap_port.paint(ctx, imap_port_rect);
    app.hits
        .push((Hit::AccountField(AccountField::ImapHost), imap_host_rect));
    app.hits
        .push((Hit::AccountField(AccountField::ImapPort), imap_port_rect));
    y += field_h + 8.0;

    row(ctx, y, &tr("SMTP server"));
    let smtp_host_rect = Rect::from_xywh(input_x, y, input_w - port_w - 8.0, field_h);
    let smtp_port_rect = Rect::from_xywh(inner.right() - port_w, y, port_w, field_h);
    app.account_form.smtp_host.paint(ctx, smtp_host_rect);
    app.account_form.smtp_port.paint(ctx, smtp_port_rect);
    app.hits
        .push((Hit::AccountField(AccountField::SmtpHost), smtp_host_rect));
    app.hits
        .push((Hit::AccountField(AccountField::SmtpPort), smtp_port_rect));
    y += field_h + 8.0;

    if let Some(message) = &app.account_form.message {
        ctx.label_left_fit(
            inner.x(),
            y + 4.0,
            message,
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            colors.warning,
            inner.width(),
        );
    }

    let buttons_h = ctx.theme.metrics.button.height;
    let y = inner.bottom() - buttons_h;
    let save = Button::new(tr("Save")).variant(ButtonVariant::Suggested);
    let cancel = Button::new(tr("Cancel"));
    let detect = Button::new(tr("Detect settings"));
    let save_w = save.width(ctx.text, ctx.theme);
    let cancel_w = cancel.width(ctx.text, ctx.theme);
    let detect_w = detect.width(ctx.text, ctx.theme);
    let save_rect = Rect::from_xywh(inner.right() - save_w, y, save_w, buttons_h);
    let cancel_rect = Rect::from_xywh(save_rect.x() - 8.0 - cancel_w, y, cancel_w, buttons_h);

    let mut left = inner.x();
    if app.store.active_account().is_some() {
        let remove = Button::new(if app.account_form.confirm_remove {
            tr("Confirm removal")
        } else {
            tr("Remove account")
        })
        .variant(ButtonVariant::Destructive);
        let remove_w = remove.width(ctx.text, ctx.theme);
        let remove_rect = Rect::from_xywh(left, y, remove_w, buttons_h);
        remove.paint(ctx, remove_rect);
        app.hits.push((Hit::AccountRemove, remove_rect));
        left = remove_rect.right() + 8.0;
    }

    let detect_rect = Rect::from_xywh(left, y, detect_w, buttons_h);
    detect.paint(ctx, detect_rect);
    cancel.paint(ctx, cancel_rect);
    save.paint(ctx, save_rect);
    app.hits.push((Hit::AccountDetect, detect_rect));
    app.hits.push((Hit::AccountCancel, cancel_rect));
    app.hits.push((Hit::AccountSave, save_rect));
}

fn paint_about(app: &mut App, ctx: &mut PaintCtx, lay: &Layout, size: Size) {
    let colors = ctx.theme.colors;
    let card = centered_card(lay.content, size, 420.0, 20.0);
    // Karta „O aplikacji” ma stałą wysokość — nie rozciągamy jej na całe okno.
    let card = Rect::from_xywh(
        card.x(),
        card.y() + (card.height() - 300.0).max(0.0) / 2.0,
        card.width(),
        300.0f64.min(card.height()),
    );
    ctx.scene
        .fill_rounded_rect(card, ctx.theme.radius.medium, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, ctx.theme.radius.medium, 1.0, colors.border);

    let center = card.center();
    let icon_rect = Rect::from_xywh(center.x - 26.0, card.y() + 30.0, 52.0, 52.0);
    ctx.scene.fill_circle(
        Point::new(icon_rect.center().x, icon_rect.center().y),
        26.0,
        colors.accent.scale_alpha(0.16),
    );
    paint_icon(
        ctx,
        IconName::Mail,
        icon_rect.deflate(Insets::symmetric(12.0, 12.0)),
        colors.accent,
    );

    ctx.label_center(
        center.x,
        card.y() + 106.0,
        "Luna Mail",
        ctx.theme.typography.title.size,
        weight::BOLD,
        colors.text,
    );
    ctx.label_center(
        center.x,
        card.y() + 130.0,
        "0.1.0",
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    ctx.label_center(
        center.x,
        card.y() + 162.0,
        &tr("A classic mail client for the Luna desktop"),
        ctx.theme.typography.body.size,
        weight::NORMAL,
        colors.text,
    );
    ctx.label_center(
        center.x,
        card.y() + 184.0,
        "IMAP · SMTP · email-lib · libLuna",
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    ctx.label_center(
        center.x,
        card.y() + 210.0,
        "paganlinux.org",
        ctx.theme.typography.caption.size,
        weight::NORMAL,
        colors.accent,
    );

    let close = Button::new(tr("Close dialog")).variant(ButtonVariant::Suggested);
    let w = close.width(ctx.text, ctx.theme);
    let h = close.height(ctx.theme);
    let rect = Rect::from_xywh(center.x - w / 2.0, card.bottom() - h - 20.0, w, h);
    close.paint(ctx, rect);
    app.hits.push((Hit::AboutClose, rect));
}

fn paint_toast(app: &App, ctx: &mut PaintCtx, size: Size) {
    let Some((text, _)) = &app.toast else {
        return;
    };
    let caption = ctx.theme.typography.body.size;
    let width = (ctx.measure(text, caption, weight::NORMAL) + 28.0).min(size.width - 32.0);
    let height = 34.0;
    let rect = Rect::from_xywh(
        (size.width - width) / 2.0,
        size.height - FOOTER_HEIGHT - height - 12.0,
        width,
        height,
    );
    ctx.scene
        .fill_rounded_rect(rect, height / 2.0, ctx.theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, height / 2.0, 1.0, ctx.theme.colors.border);
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        text,
        caption,
        weight::NORMAL,
        ctx.theme.colors.text,
    );
}

fn centered_card(content: Rect, size: Size, max_width: f64, margin: f64) -> Rect {
    let width = (size.width - 2.0 * margin).min(max_width).max(240.0);
    Rect::from_xywh(
        (size.width - width) / 2.0,
        content.y() + margin,
        width,
        (content.height() - 2.0 * margin).max(220.0),
    )
}

// ——— obsługa zdarzeń ———

/// Obsługuje wszystkie zdarzenia klatki.
pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme);
    for event in events {
        match event {
            Event::Scroll(scroll) => handle_scroll(app, scroll.dy, scroll.position, &lay),
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                let bar = HeaderBar::new().controls(ControlsSide::Right);
                app.controls_hover = bar.control_at(lay.header, theme, *position);
                if app.menu_button.is_open() {
                    let panel = menu_panel(app, lay.menu_btn, size, theme, text);
                    app.menu.hover_at(panel, Some(*position), theme);
                }
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.controls_hover = None;
                app.menu.set_hovered(None);
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Middle
                    && button.state == ButtonState::Pressed
                    && app.wants_text_input() =>
            {
                app.pending_paste_primary = true;
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                handle_click(
                    app,
                    button.position,
                    &lay,
                    size,
                    theme,
                    text,
                    button.modifiers.shift,
                );
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                handle_key(app, key, theme, text);
            }
            Event::Text(typed) => handle_text(app, typed),
            Event::Paste(text) => {
                app.paste_text(text);
            }
            Event::Clipboard(data) if data.mime == "text/plain" => {
                if let Ok(text) = std::str::from_utf8(&data.bytes) {
                    app.paste_text(text);
                }
            }
            Event::Drop { paths, .. } => {
                if app.view == View::Compose {
                    app.attach_paths(paths);
                } else if app.view == View::Mail {
                    app.compose_new();
                    app.attach_paths(paths);
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            Event::Window(WindowEvent::Focused(false)) => app.search.set_focused(false),
            _ => {}
        }
    }
}

fn handle_scroll(app: &mut App, dy: f64, position: Point, lay: &Layout) {
    match app.view {
        View::Mail => {
            if lay.list.contains(position) {
                app.list_scroll -= dy;
                app.clamp_list_scroll();
            } else if lay.preview.contains(position) {
                // Górna granica jest pilnowana przy rysowaniu (treść bywa długa).
                app.body_scroll = (app.body_scroll - dy).max(0.0);
            }
        }
        View::Compose => {
            // Treść edytora przewija się sama za kursorem.
        }
        _ => {}
    }
}

fn handle_click(
    app: &mut App,
    position: Point,
    lay: &Layout,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    extend_selection: bool,
) {
    // Kontrolki okna mają zawsze pierwszeństwo.
    let bar = HeaderBar::new().controls(ControlsSide::Right);
    if let Some(control) = bar.control_at(lay.header, theme, position) {
        match control {
            WindowControl::Minimize => app.pending_minimize = true,
            WindowControl::Maximize => app.pending_maximize = true,
            WindowControl::Close => app.pending_close = true,
        }
        return;
    }

    // Menu kont z paska bocznego jest nad treścią, ale pod kontrolkami okna.
    if app.account_menu_open {
        if let Some(panel) = account_menu_panel(app, lay, size, theme, text) {
            if panel.contains(position) {
                if let Some(index) = app.account_menu.click_at(panel, position, theme) {
                    app.account_menu_open = false;
                    let accounts = app.store.accounts.len();
                    if (1..=accounts).contains(&index) {
                        app.switch_account(index - 1);
                        // Szuflada w wąskim oknie nie może zasłaniać treści.
                        if size.width < 860.0 {
                            app.sidebar_visible = false;
                        }
                    } else if index == accounts + 2 {
                        app.open_account_settings_new();
                    } else if index == accounts + 3 {
                        app.open_account_settings();
                    }
                }
            } else {
                app.account_menu_open = false;
            }
        } else {
            app.account_menu_open = false;
        }
        return;
    }

    // Menu jest nad wszystkim.
    if app.menu_button.is_open() {
        let panel = menu_panel(app, lay.menu_btn, size, theme, text);
        if panel.contains(position) {
            if let Some(index) = app.menu.click_at(panel, position, theme) {
                app.menu_button.set_open(false);
                menu_action(app, index);
            }
        } else {
            app.menu_button.set_open(false);
        }
        return;
    }

    if app.view == View::Mail {
        if lay.menu_btn.contains(position) {
            app.account_menu_open = false;
            app.menu_button.toggle();
            return;
        }
        if lay.search_btn.contains(position) {
            app.search_active = !app.search_active;
            app.search.set_focused(app.search_active);
            if !app.search_active {
                app.search.clear();
                app.recompute();
            }
            return;
        }
        if lay.refresh_btn.contains(position) {
            app.refresh();
            return;
        }
        if lay.compose_btn.contains(position) {
            app.compose_new();
            return;
        }
        if lay.reply_btn.contains(position) {
            app.compose_reply();
            return;
        }
        if lay.forward_btn.contains(position) {
            app.compose_forward();
            return;
        }
        if lay.sidebar_btn.contains(position) {
            app.sidebar_visible = !app.sidebar_visible;
            return;
        }
        if let Some(field) = lay.search_field {
            if app.search.clear_at(field, theme, position) {
                app.search.clear();
                app.recompute();
                return;
            }
            if field.contains(position) {
                app.search
                    .place_cursor(field, theme, text, position, extend_selection);
                app.search.set_focused(true);
                return;
            }
        }
    }

    // Trafienia z poprzedniej klatki (foldery, wiadomości, przyciski kart).
    if let Some((hit, rect)) = app
        .hits
        .iter()
        .rev()
        .find(|(_, rect)| !rect.is_empty() && rect.contains(position))
        .copied()
    {
        match hit {
            Hit::Folder(index) => {
                app.select_folder(index);
                app.search.set_focused(false);
                // W wąskim oknie pasek jest szufladą — po wyborze wraca treść.
                if size.width < 860.0 {
                    app.sidebar_visible = false;
                }
            }
            Hit::Message(index) => {
                app.search.set_focused(false);
                app.select(index);
                app.register_click(index);
            }
            Hit::Attachment(index) => {
                if let Some(attachment) = app
                    .open_body
                    .as_ref()
                    .and_then(|body| body.attachments.get(index))
                    .cloned()
                {
                    match crate::app::save_attachment(&attachment) {
                        Ok(path) => {
                            app.notify(format!("{}: {}", tr("Saved attachment"), path.display()))
                        }
                        Err(message) => app.notify(message),
                    }
                }
            }
            Hit::RemoveAttachment(index) => {
                if index < app.compose.attachments.len() {
                    app.compose.attachments.remove(index);
                }
            }
            Hit::ComposeField(field) => {
                app.compose.field = field;
                match field {
                    ComposeField::To => {
                        app.compose
                            .to
                            .place_cursor(rect, theme, text, position, extend_selection);
                    }
                    ComposeField::Cc => {
                        app.compose
                            .cc
                            .place_cursor(rect, theme, text, position, extend_selection);
                    }
                    ComposeField::Subject => {
                        app.compose.subject.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    ComposeField::Body => {
                        app.compose.body.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                }
                app.compose.focus();
            }
            Hit::ComposeSend => app.send_compose(),
            Hit::ComposeCancel => app.close_compose(),
            Hit::ComposeAttach => app.notify(tr("Drag files into the window to attach them")),
            Hit::AccountField(field) => {
                app.account_form.field = field;
                app.account_form.confirm_remove = false;
                match field {
                    AccountField::Email => {
                        app.account_form.email.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::Display => {
                        app.account_form.display.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::Login => {
                        app.account_form.login.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::Password => {
                        app.account_form.password.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::ImapHost => {
                        app.account_form.imap_host.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::ImapPort => {
                        app.account_form.imap_port.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::SmtpHost => {
                        app.account_form.smtp_host.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                    AccountField::SmtpPort => {
                        app.account_form.smtp_port.place_cursor(
                            rect,
                            theme,
                            text,
                            position,
                            extend_selection,
                        );
                    }
                }
                app.account_form.focus();
            }
            Hit::AccountMenu => {
                app.menu_button.set_open(false);
                app.open_account_menu();
            }
            Hit::AccountRemove => {
                if app.account_form.confirm_remove {
                    app.remove_active_account();
                } else {
                    app.account_form.confirm_remove = true;
                    app.account_form.message =
                        Some(tr("Click again to remove this account").to_owned());
                }
            }
            Hit::AccountSave => app.save_account(),
            Hit::AccountCancel => {
                app.view = View::Mail;
                app.account_form.message = None;
            }
            Hit::AccountDetect => app.account_form.detect(),
            Hit::PasswordPeek => {
                app.account_form.password.toggle_peek();
            }
            Hit::AboutClose => app.view = View::Mail,
            Hit::SidebarToggle => app.sidebar_visible = !app.sidebar_visible,
            Hit::RetryConnect | Hit::Configure => app.open_account_settings(),
            Hit::Reply => app.compose_reply(),
            Hit::Forward => app.compose_forward(),
        }
        return;
    }

    // Przeciąganie okna za pasek tytułu (dwuklik obsługuje platforma).
    if lay.header.contains(position) && app.view != View::Account {
        app.pending_move = true;
    }
}

fn menu_action(app: &mut App, index: usize) {
    match index {
        1 => app.compose_new(),
        2 => app.compose_reply(),
        3 => app.compose_forward(),
        6 => app.refresh(),
        7 => app.mark_seen(true, true),
        8 => app.mark_seen(false, true),
        9 => app.toggle_flag(),
        10 => app.delete_selected(),
        13 => app.open_account_menu(),
        14 => app.open_account_settings_new(),
        15 => app.open_account_settings(),
        16 => app.view = View::About,
        17 => app.pending_close = true,
        _ => {}
    }
}

fn handle_text(app: &mut App, typed: &str) {
    if app.view == View::About {
        return;
    }
    if app.view == View::Mail && !app.search.is_focused() {
        return;
    }
    let cleaned: String = typed.chars().filter(|ch| !ch.is_control()).collect();
    if !cleaned.is_empty() {
        app.insert_text(&cleaned);
    }
}

fn handle_key(app: &mut App, key: &KeyEvent, theme: &Theme, text: &TextEngine) {
    let ctrl = key.modifiers.ctrl;
    let shift = key.modifiers.shift;

    if ctrl && key.keysym == KEY_Q {
        app.pending_close = true;
        return;
    }
    if key.keysym == KEY_F1 {
        app.view = View::About;
        return;
    }
    if ctrl {
        match key.keysym {
            KEY_N => {
                app.compose_new();
                return;
            }
            KEY_R if shift => {
                app.mark_seen(true, true);
                return;
            }
            KEY_R => {
                app.compose_reply();
                return;
            }
            KEY_U if shift => {
                app.mark_seen(false, true);
                return;
            }
            KEY_F if shift => {
                app.compose_forward();
                return;
            }
            KEY_F => {
                if app.view == View::Mail {
                    app.search_active = true;
                    app.search.set_focused(true);
                }
                return;
            }
            KEY_A => {
                app.select_all();
                return;
            }
            KEY_C => {
                if let Some(text) = app.copy() {
                    app.pending_copy = Some(text);
                }
                return;
            }
            KEY_X => {
                if let Some(text) = app.cut() {
                    app.pending_copy = Some(text);
                }
                return;
            }
            KEY_V => {
                app.pending_paste = true;
                return;
            }
            KEY_S if app.view == View::Account => {
                app.save_account();
                return;
            }
            KEY_RETURN | KEY_KP_ENTER => {
                match app.view {
                    View::Compose => app.send_compose(),
                    View::Account => app.save_account(),
                    _ => {}
                }
                return;
            }
            _ => {}
        }
    }
    if key.keysym == KEY_F5 {
        app.refresh();
        return;
    }

    match app.view {
        View::About => {
            if matches!(key.keysym, KEY_ESCAPE | KEY_RETURN | KEY_KP_ENTER) {
                app.view = View::Mail;
            }
        }
        View::Compose => handle_compose_key(app, key, theme, text),
        View::Account => handle_account_key(app, key),
        View::Mail => handle_mail_key(app, key),
    }
}

fn handle_compose_key(app: &mut App, key: &KeyEvent, theme: &Theme, text: &TextEngine) {
    let shift = key.modifiers.shift;
    match key.keysym {
        KEY_ESCAPE => app.close_compose(),
        KEY_TAB => {
            if shift {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KEY_RETURN | KEY_KP_ENTER if app.compose.field != ComposeField::Body => {
            app.focus_next();
        }
        KEY_RETURN | KEY_KP_ENTER => app.compose.body.insert_newline(),
        KEY_BACKSPACE => app.backspace(),
        KEY_DELETE => app.delete_forward(),
        KEY_LEFT => app.move_cursor(-1, 0, shift),
        KEY_RIGHT => app.move_cursor(1, 0, shift),
        KEY_UP if app.compose.field == ComposeField::Body && app.compose_body_width > 0.0 => app
            .compose
            .body
            .move_visual(-1, app.compose_body_width, theme, text),
        KEY_DOWN if app.compose.field == ComposeField::Body && app.compose_body_width > 0.0 => app
            .compose
            .body
            .move_visual(1, app.compose_body_width, theme, text),
        KEY_UP => app.move_cursor(0, -1, shift),
        KEY_DOWN => app.move_cursor(0, 1, shift),
        KEY_HOME => app.cursor_home(shift),
        KEY_END => app.cursor_end(shift),
        _ => {}
    }
}

fn handle_account_key(app: &mut App, key: &KeyEvent) {
    let shift = key.modifiers.shift;
    match key.keysym {
        KEY_ESCAPE => {
            app.view = View::Mail;
            app.account_form.message = None;
        }
        KEY_TAB => {
            if shift {
                app.focus_previous();
            } else {
                app.focus_next();
            }
        }
        KEY_RETURN | KEY_KP_ENTER => app.save_account(),
        KEY_BACKSPACE => app.backspace(),
        KEY_DELETE => app.delete_forward(),
        KEY_LEFT => app.move_cursor(-1, 0, shift),
        KEY_RIGHT => app.move_cursor(1, 0, shift),
        KEY_HOME => app.cursor_home(shift),
        KEY_END => app.cursor_end(shift),
        _ => {}
    }
}

fn handle_mail_key(app: &mut App, key: &KeyEvent) {
    if app.search.is_focused() {
        match key.keysym {
            KEY_ESCAPE => {
                app.search.set_focused(false);
                app.search_active = false;
                app.search.clear();
                app.recompute();
            }
            KEY_BACKSPACE => {
                app.search.backspace();
                app.recompute();
            }
            KEY_DELETE => {
                app.search.delete_forward();
                app.recompute();
            }
            KEY_LEFT => app.search.move_left_selecting(key.modifiers.shift),
            KEY_RIGHT => app.search.move_right_selecting(key.modifiers.shift),
            KEY_HOME => app.search.home_selecting(key.modifiers.shift),
            KEY_END => app.search.end_selecting(key.modifiers.shift),
            KEY_UP => {
                app.search.set_focused(false);
                app.move_selection(-1);
            }
            KEY_DOWN => {
                app.search.set_focused(false);
                app.move_selection(1);
            }
            KEY_RETURN | KEY_KP_ENTER => app.search.set_focused(false),
            _ => {}
        }
        return;
    }

    match key.keysym {
        KEY_ESCAPE => {
            if app.menu_button.is_open() {
                app.menu_button.set_open(false);
            } else if app.search_active || !app.search.is_empty() {
                app.search_active = false;
                app.search.clear();
                app.recompute();
            }
        }
        KEY_UP => app.move_selection(-1),
        KEY_DOWN => app.move_selection(1),
        KEY_PAGE_UP => app.move_selection(-8),
        KEY_PAGE_DOWN => app.move_selection(8),
        KEY_HOME => app.select_first(),
        KEY_END => app.select_last(),
        KEY_RETURN | KEY_KP_ENTER => {
            // Enter „otwiera” wiadomość — podgląd jest już widoczny, więc
            // przenosimy fokus z listy i oznaczamy jako przeczytane.
            app.mark_seen(true, false);
        }
        KEY_DELETE => app.delete_selected(),
        KEY_S => app.toggle_flag(),
        KEY_N => app.compose_new(),
        KEY_R => app.compose_reply(),
        KEY_F => app.compose_forward(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use libluna_i18n::set_locale;
    use libluna_i18n::Locale;

    fn setup() -> (App, Theme) {
        set_locale(Locale::new("pl"));
        // `App::demo()` nie czyta `~/.config` i nie uruchamia wątku sieciowego.
        let mut app = App::demo();
        app.load_demo();
        (app, Theme::dark())
    }

    #[test]
    fn layout_turns_sidebar_into_a_drawer_in_narrow_windows() {
        let (mut app, theme) = setup();
        app.sidebar_visible = true;
        let narrow = layout(&app, Size::new(600.0, 600.0), &theme);
        // Szuflada nad treścią: lista zaczyna się od krawędzi okna.
        assert!(narrow.sidebar.is_some());
        assert!(narrow.list.x() < 1.0);
        let wide = layout(&app, Size::new(1100.0, 700.0), &theme);
        assert!(wide.sidebar.is_some());
        assert!(wide.list.x() >= wide.sidebar.unwrap().width());
    }

    #[test]
    fn sidebar_toggle_hides_even_on_wide_windows() {
        let (mut app, theme) = setup();
        app.sidebar_visible = false;
        let wide = layout(&app, Size::new(1100.0, 700.0), &theme);
        assert!(wide.sidebar.is_none());
    }

    #[test]
    fn header_buttons_do_not_overlap_window_controls() {
        let (app, theme) = setup();
        let lay = layout(&app, Size::new(900.0, 700.0), &theme);
        let bar = HeaderBar::new().controls(ControlsSide::Right);
        let controls = bar.controls_layout(lay.header, &theme);
        let minimize = controls.minimize.expect("kontrolka");
        assert!(lay.menu_btn.right() <= minimize.x());
    }

    #[test]
    fn previews_stay_inside_the_window() {
        let (app, theme) = setup();
        for width in [480.0, 700.0, 1000.0, 1600.0] {
            let lay = layout(&app, Size::new(width, 720.0), &theme);
            assert!(lay.list.right() <= lay.preview.right() + 0.5);
            assert!(lay.preview.right() <= width + 0.5);
            assert!(lay.list.width() > 0.0);
        }
    }

    fn click_hit(app: &mut App, theme: &Theme, size: Size, want: Hit) {
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(app, &mut scene, size, theme, &text);
        let (_, rect) = app
            .hits
            .iter()
            .find(|(hit, _)| *hit == want)
            .copied()
            .unwrap_or_else(|| panic!("brak trafienia {want:?}"));
        let lay = layout(app, size, theme);
        handle_click(app, rect.center(), &lay, size, theme, &text, false);
    }

    #[test]
    fn clicking_an_account_field_focuses_it_and_paste_lands_there() {
        let (mut app, theme) = setup();
        let size = Size::new(1080.0, 720.0);
        app.open_account_settings();
        click_hit(
            &mut app,
            &theme,
            size,
            Hit::AccountField(AccountField::Email),
        );
        assert_eq!(app.account_form.field, AccountField::Email);
        assert!(app.account_form.email.is_focused());

        let text = TextEngine::system();
        handle_events(
            &mut app,
            &[Event::Paste("ala@example.org".into())],
            size,
            &theme,
            &text,
        );
        assert_eq!(app.account_form.email.text(), "ala@example.org");
    }

    #[test]
    fn clicking_compose_body_focuses_it_and_paste_lands_there() {
        let (mut app, theme) = setup();
        let size = Size::new(1080.0, 720.0);
        app.compose_new();
        click_hit(
            &mut app,
            &theme,
            size,
            Hit::ComposeField(ComposeField::Body),
        );
        assert_eq!(app.compose.field, ComposeField::Body);
        assert!(app.compose.body.is_focused());

        let text = TextEngine::system();
        handle_events(
            &mut app,
            &[Event::Paste("Treść wklejona".into())],
            size,
            &theme,
            &text,
        );
        assert!(app.compose.body.text().contains("Treść wklejona"));
    }

    #[test]
    fn click_inside_an_existing_field_sets_the_exact_cursor() {
        let (mut app, theme) = setup();
        let size = Size::new(1080.0, 720.0);
        app.open_account_settings();
        app.account_form.email.set_text("abcdef@example.org");

        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size, &theme, &text);
        let (_, rect) = app
            .hits
            .iter()
            .find(|(hit, _)| *hit == Hit::AccountField(AccountField::Email))
            .copied()
            .expect("pole e-mail");
        let lay = layout(&app, size, &theme);

        // Klik przy lewej krawędzi pola → kursor na początku.
        handle_click(
            &mut app,
            Point::new(rect.x() + 2.0, rect.center().y),
            &lay,
            size,
            &theme,
            &text,
            false,
        );
        app.insert_text("X");
        assert_eq!(app.account_form.email.text(), "Xabcdef@example.org");

        // Klik przy prawej krawędzi → kursor na końcu (pole się przewija).
        handle_click(
            &mut app,
            Point::new(rect.right() - 2.0, rect.center().y),
            &lay,
            size,
            &theme,
            &text,
            false,
        );
        app.insert_text("Y");
        assert_eq!(app.account_form.email.text(), "Xabcdef@example.orgY");
    }
}
