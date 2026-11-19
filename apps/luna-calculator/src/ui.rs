//! Rysowanie i obsługa zdarzeń kalkulatora.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Event, Insets, KeyEvent, PointerButton, Rect, Size, Theme, WindowEvent,
};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    BottomSheet, Button, ButtonVariant, ControlsSide, HeaderBar, MenuButton, PaintCtx,
    PreferencesDialog, TitleAlign, ViewSwitcher, WindowControl,
};

use crate::app::{Action, App, Key, MenuState, Mode};
use crate::eval::AngleMode;

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_ENTER: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_DELETE: u32 = 0xffff;
const KEY_C: u32 = 0x63;
const KEY_H: u32 = 0x68;
const KEY_V: u32 = 0x76;
const KEY_COMMA: u32 = 0x2c;
const KEY_F1: u32 = 0xffbe;

const STANDARD_ROWS: [&[Key]; 5] = [
    &[Key::Clear, Key::Backspace, Key::Percent, Key::Div],
    &[Key::Digit(7), Key::Digit(8), Key::Digit(9), Key::Mul],
    &[Key::Digit(4), Key::Digit(5), Key::Digit(6), Key::Sub],
    &[Key::Digit(1), Key::Digit(2), Key::Digit(3), Key::Add],
    &[Key::Negate, Key::Digit(0), Key::Dot, Key::Equals],
];

const SCIENTIFIC_ROWS: [&[Key]; 6] = [
    &[Key::Sin, Key::Cos, Key::Tan, Key::Pi],
    &[Key::Asin, Key::Acos, Key::Atan, Key::Euler],
    &[Key::Ln, Key::Log, Key::Sqrt, Key::Square],
    &[Key::Pow, Key::Reciprocal, Key::Factorial, Key::Exp],
    &[Key::Abs, Key::Mod, Key::TenPow, Key::Ans],
    &[Key::OpenParen, Key::CloseParen, Key::Angle, Key::Negate],
];

/// Wspólny układ okna: pasek tytułu z trybem i treść pod nim.
#[derive(Debug, Clone, Copy)]
struct Chrome {
    header: Rect,
    tabs: Rect,
    menu: Rect,
    content: Rect,
}

fn chrome(size: Size, theme: &Theme, text: &TextEngine) -> Chrome {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let header_bar = HeaderBar::new()
        .title(libluna_i18n::tr("Calculator"))
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right);
    let available = header_bar.controls_layout(header, theme).content;

    let title_width = text
        .measure(
            &libluna_i18n::tr("Calculator"),
            theme.typography.body.size,
            weight::BOLD,
        )
        .width;

    let menu_h = MenuButton::burger()
        .width(text, theme)
        .clamp(26.0, (header.height() - 8.0).max(26.0));
    let menu = Rect::from_xywh(
        available.right() - menu_h,
        header.center().y - menu_h / 2.0,
        menu_h,
        menu_h,
    );

    let title_end = available.x() + title_width + 20.0;
    let free = (menu.x() - 8.0 - title_end).max(0.0);
    let tabs_w = free.min(260.0);
    let tabs = Rect::from_xywh(
        menu.x() - 8.0 - tabs_w,
        header.center().y - 15.0,
        tabs_w,
        30.0,
    );

    let max_w = 900.0;
    let content_w = (size.width - 48.0).clamp(0.0, max_w);
    let top = header.bottom() + 16.0;
    let content = Rect::from_xywh(
        (size.width - content_w) / 2.0,
        top,
        content_w,
        (size.height - top - 16.0).max(0.0),
    );

    Chrome {
        header,
        tabs,
        menu,
        content,
    }
}

/// Prostokąt przycisku menu — używany też przez podglądy headless.
pub fn menu_anchor(size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    chrome(size, theme, text).menu
}

fn paint_card(ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.large, 1.0, theme.colors.border);
}

fn hit(app: &mut App, action: Action, rect: Rect) {
    app.hits.push((action, rect));
}

fn is_hovered(app: &App, rect: Rect) -> bool {
    app.pointer.is_some_and(|point| rect.contains(point))
}

/// Rysuje całą klatkę i zbiera regiony kliknięć.
pub fn paint(app: &mut App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    app.hits.clear();

    let layout = chrome(size, theme, text);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene.fill_rect(
        Rect::from_xywh(0.0, 0.0, size.width, size.height),
        theme.colors.window_bg,
    );

    HeaderBar::new()
        .title(libluna_i18n::tr("Calculator"))
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right)
        .paint(&mut ctx, layout.header, app.controls_hover);

    paint_mode_switch(app, &mut ctx, layout.tabs);
    paint_menu_button(app, &mut ctx, layout.menu);
    paint_content(app, &mut ctx, layout.content);

    if let Some(state) = &mut app.menu {
        // Po zmianie rozmiaru okna menu podąża za przyciskiem w pasku tytułu.
        state.anchor = layout.menu;
    }
    if let Some(state) = &app.menu {
        let panel = menu_panel(state, size, theme, text);
        state.menu.paint(&mut ctx, panel);
    }

    if app.history_sheet.is_open() {
        app.history_sheet
            .set_title(Some(libluna_i18n::tr("History")));
        app.hits.clear();
        paint_history(app, &mut ctx, size);
    }

    if app.settings_open {
        app.hits.clear();
        paint_settings(app, &mut ctx, size);
    }
    if let Some(about) = &app.about {
        app.hits.clear();
        about.paint(&mut ctx, size);
    }
}

fn paint_menu_button(app: &mut App, ctx: &mut PaintCtx, rect: Rect) {
    let mut button = MenuButton::burger();
    button.set_open(app.menu.is_some());
    button.set_hovered(is_hovered(app, rect));
    button.paint(ctx, rect);
}

fn menu_panel(state: &MenuState, size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    let width = state.menu.width(text, theme);
    state
        .menu
        .place(state.anchor, width, Rect::from_size(size), theme)
}

fn paint_mode_switch(app: &mut App, ctx: &mut PaintCtx, rect: Rect) {
    let labels: Vec<String> = Mode::ALL
        .iter()
        .map(|mode| libluna_i18n::tr(mode.title_key()))
        .collect();
    let mut switcher = ViewSwitcher::new(labels);
    switcher.set_selected(app.mode.index());
    if let Some(point) = app.pointer {
        switcher.set_hovered(switcher.index_at(rect, point));
    }
    switcher.paint(ctx, rect);
    for (index, segment) in switcher.segments(rect).into_iter().enumerate() {
        hit(app, Action::Mode(Mode::from_index(index)), segment);
    }
}

fn paint_content(app: &mut App, ctx: &mut PaintCtx, area: Rect) {
    let display_h = 140.0_f64.min((area.height() * 0.30).max(96.0));
    let display = Rect::from_xywh(area.x(), area.y(), area.width(), display_h);
    paint_display(app, ctx, display);

    let gap = 16.0;
    let below = Rect::from_xywh(
        area.x(),
        display.bottom() + gap,
        area.width(),
        (area.height() - display_h - gap).max(0.0),
    );
    if below.height() <= 0.0 {
        return;
    }

    match app.mode {
        Mode::Standard => {
            let width = below.width().min(560.0);
            let height = below.height().min(540.0);
            let card = Rect::from_xywh(
                below.center().x - width / 2.0,
                below.y() + (below.height() - height).max(0.0) / 2.0,
                width,
                height,
            );
            paint_keypad(app, ctx, card, &STANDARD_ROWS);
        }
        Mode::Scientific if below.width() >= 760.0 => {
            let left_w = ((below.width() - gap) / 2.0).floor();
            let right_w = (below.width() - left_w - gap).max(0.0);
            let height = below.height().min(520.0);
            let y = below.y() + (below.height() - height).max(0.0) / 2.0;
            let left = Rect::from_xywh(below.x(), y, left_w, height);
            let right = Rect::from_xywh(below.x() + left_w + gap, y, right_w, height);
            paint_keypad(app, ctx, left, &SCIENTIFIC_ROWS);
            paint_keypad(app, ctx, right, &STANDARD_ROWS);
        }
        Mode::Scientific => {
            // Wąskie okno: panel naukowy nad standardowym zamiast dwóch kolumn.
            let sci_h = ((below.height() - gap) * 0.56).floor();
            let std_h = (below.height() - gap - sci_h).max(0.0);
            let upper = Rect::from_xywh(below.x(), below.y(), below.width(), sci_h);
            let lower = Rect::from_xywh(below.x(), upper.bottom() + gap, below.width(), std_h);
            paint_keypad(app, ctx, upper, &SCIENTIFIC_ROWS);
            paint_keypad(app, ctx, lower, &STANDARD_ROWS);
        }
    }
}

fn paint_display(app: &mut App, ctx: &mut PaintCtx, rect: Rect) {
    paint_card(ctx, rect);
    let theme = *ctx.theme;
    let pad = 24.0;

    if app.mode == Mode::Scientific {
        ctx.label_left(
            rect.x() + pad,
            rect.y() + 26.0,
            app.angle.label(),
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
    }

    let expression = if app.expression.is_empty() {
        "0".to_owned()
    } else {
        app.expression.clone()
    };
    let mut size = 44.0;
    while ctx.measure(&expression, size, weight::BOLD) > rect.width() - pad * 2.0 && size > 18.0 {
        size -= 2.0;
    }
    let expression = ellipsize(
        ctx,
        &expression,
        rect.width() - pad * 2.0,
        size,
        weight::BOLD,
    );
    ctx.label_right(
        rect.right() - pad,
        rect.y() + rect.height() * 0.52,
        &expression,
        size,
        weight::BOLD,
        theme.colors.text,
    );

    let (message, color) = if let Some(error) = app.error {
        (libluna_i18n::tr(error), theme.colors.error)
    } else if let Some(preview) = app.preview() {
        (format!("= {preview}"), theme.colors.dim_text)
    } else if app.just_evaluated {
        (String::new(), theme.colors.dim_text)
    } else {
        (
            libluna_i18n::tr("Type an expression"),
            theme.colors.dim_text,
        )
    };

    if !message.is_empty() {
        ctx.label_right(
            rect.right() - pad,
            rect.bottom() - 28.0,
            &message,
            theme.typography.body.size,
            weight::NORMAL,
            color,
        );
    }
}

fn settings_dialog() -> PreferencesDialog {
    PreferencesDialog::new(libluna_i18n::tr("Calculator Settings"), 300.0)
}

struct SettingsLayout {
    rows: [Rect; 2],
    angle_seg: Rect,
    mode_seg: Rect,
    done: Rect,
}

fn settings_layout(size: Size, theme: &Theme) -> SettingsLayout {
    let rects = settings_dialog().rects(size, theme);
    let pad = theme.spacing.lg;
    let inner = rects.content.deflate(Insets::new(pad, pad, pad, pad));
    let row_h = 52.0;
    let rows: [Rect; 2] = std::array::from_fn(|i| {
        Rect::from_xywh(
            inner.x(),
            inner.y() + i as f64 * row_h,
            inner.width(),
            row_h,
        )
    });
    let seg_w = 220.0_f64.min(inner.width() * 0.55);
    let seg = |i: usize| {
        Rect::from_xywh(
            inner.right() - seg_w,
            rows[i].center().y - 15.0,
            seg_w,
            30.0,
        )
    };
    let bh = theme.metrics.button.height;
    let done = Rect::from_xywh(inner.right() - 120.0, inner.bottom() - bh, 120.0, bh);
    SettingsLayout {
        rows,
        angle_seg: seg(0),
        mode_seg: seg(1),
        done,
    }
}

fn paint_settings(app: &App, ctx: &mut PaintCtx, size: Size) {
    settings_dialog().paint(ctx, size);
    let theme = *ctx.theme;
    let lay = settings_layout(size, &theme);
    let body = theme.typography.body.size;
    let labels = [
        libluna_i18n::tr("Angle unit"),
        libluna_i18n::tr("Start mode"),
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

    let mut angle = ViewSwitcher::new(["DEG", "RAD"]);
    angle.set_selected(match app.angle {
        AngleMode::Degrees => 0,
        AngleMode::Radians => 1,
    });
    angle.set_hovered(
        app.pointer
            .and_then(|point| angle.index_at(lay.angle_seg, point)),
    );
    angle.paint(ctx, lay.angle_seg);

    let mut mode =
        ViewSwitcher::new([libluna_i18n::tr("Standard"), libluna_i18n::tr("Scientific")]);
    mode.set_selected(app.prefs.start_mode.index());
    mode.set_hovered(
        app.pointer
            .and_then(|point| mode.index_at(lay.mode_seg, point)),
    );
    mode.paint(ctx, lay.mode_seg);

    let mut done = Button::new(libluna_i18n::tr("Done")).variant(ButtonVariant::Suggested);
    done.set_hovered(is_hovered(app, lay.done));
    done.paint(ctx, lay.done);
}

struct HistoryLayout {
    clear: Rect,
    list: Rect,
}

fn history_layout(sheet: &BottomSheet, size: Size, theme: &Theme) -> HistoryLayout {
    let content = sheet.content_rect(size, theme);
    let pad = theme.spacing.lg.max(16.0);
    let inner = content.deflate(Insets::new(pad, pad, pad, pad));
    let bh = theme.metrics.button.height;
    let clear_w = 150.0_f64.min(inner.width() * 0.5);
    let clear = Rect::from_xywh(inner.right() - clear_w, inner.y(), clear_w, bh);
    let list_y = inner.y() + bh + 8.0;
    let list = Rect::from_xywh(
        inner.x(),
        list_y,
        inner.width(),
        (inner.bottom() - list_y).max(0.0),
    );
    HistoryLayout { clear, list }
}

fn paint_history(app: &mut App, ctx: &mut PaintCtx, size: Size) {
    app.history_sheet.paint(ctx, size);
    let theme = *ctx.theme;
    let lay = history_layout(&app.history_sheet, size, &theme);

    let mut clear = Button::new(libluna_i18n::tr("Clear history"))
        .variant(ButtonVariant::Default)
        .enabled(!app.history.is_empty());
    clear.set_hovered(is_hovered(app, lay.clear));
    clear.paint(ctx, lay.clear);
    if !app.history.is_empty() {
        hit(app, Action::ClearHistory, lay.clear);
    }

    if app.history.is_empty() {
        ctx.label_center(
            lay.list.center().x,
            lay.list.center().y,
            &libluna_i18n::tr("No calculations yet"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        return;
    }

    let entries: Vec<(String, String)> = app
        .history
        .entries()
        .iter()
        .rev()
        .map(|entry| (entry.expression.clone(), entry.result.clone()))
        .collect();
    let row_h = 36.0;

    app.history_scroll
        .set_content_height(entries.len() as f64 * row_h);
    app.history_scroll
        .set_offset(app.history_scroll.offset(), lay.list);
    let offset = app.history_scroll.offset();

    ctx.scene.push_clip(lay.list);
    for (i, (expression, result)) in entries.iter().enumerate() {
        let row = Rect::from_xywh(
            lay.list.x(),
            lay.list.y() + i as f64 * row_h - offset,
            (lay.list.width() - 12.0).max(0.0),
            row_h - 2.0,
        );
        if row.bottom() < lay.list.y() || row.y() > lay.list.bottom() {
            continue;
        }
        if is_hovered(app, row) {
            ctx.scene.fill_rounded_rect(
                row,
                theme.radius.small,
                theme.colors.shade.scale_alpha(0.8),
            );
        }

        let body = theme.typography.body.size;
        let mut result_size = body;
        while ctx.measure(result, result_size, weight::BOLD) > row.width() * 0.5
            && result_size > 11.0
        {
            result_size -= 1.0;
        }
        let result_width = ctx.measure(result, result_size, weight::BOLD);
        let expression_width = (row.width() - result_width - 30.0).max(20.0);
        let expression = ellipsize(ctx, expression, expression_width, body, weight::NORMAL);

        ctx.label_left(
            row.x() + 10.0,
            row.center().y,
            &expression,
            body,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        ctx.label_right(
            row.right() - 10.0,
            row.center().y,
            result,
            result_size,
            weight::BOLD,
            theme.colors.text,
        );

        let index = entries.len() - 1 - i;
        hit(app, Action::History(index), row);
    }
    ctx.scene.pop_clip();
    app.history_scroll.paint_scrollbar(ctx, lay.list);
}

/// Skraca tekst od lewej, zostawiając koniec i wielokropek — dla pól
/// wyrównanych do prawej i wierszy historii.
fn ellipsize(ctx: &PaintCtx, text: &str, width: f64, size: f64, font_weight: u16) -> String {
    if width <= 0.0 || text.is_empty() {
        return String::new();
    }
    if ctx.measure(text, size, font_weight) <= width {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::from("…");
    for start in (0..chars.len()).rev() {
        let mut candidate = String::from("…");
        candidate.extend(&chars[start..]);
        if ctx.measure(&candidate, size, font_weight) <= width {
            out = candidate;
        } else {
            break;
        }
    }
    out
}

fn paint_keypad(app: &mut App, ctx: &mut PaintCtx, card: Rect, rows: &[&[Key]]) {
    paint_card(ctx, card);
    if rows.is_empty() {
        return;
    }

    let pad = 12.0;
    let gap = 8.0;
    let inner = card.deflate(Insets::splat(pad));
    let columns = rows.iter().map(|row| row.len()).max().unwrap_or(0);
    if columns == 0 {
        return;
    }

    let button_w = ((inner.width() - gap * (columns - 1) as f64) / columns as f64).max(0.0);
    let button_h = ((inner.height() - gap * (rows.len() - 1) as f64) / rows.len() as f64).max(0.0);

    for (row_index, row) in rows.iter().enumerate() {
        for (column, key) in row.iter().enumerate() {
            let rect = Rect::from_xywh(
                inner.x() + column as f64 * (button_w + gap),
                inner.y() + row_index as f64 * (button_h + gap),
                button_w,
                button_h,
            );
            paint_key(app, ctx, rect, *key);
            hit(app, Action::Press(*key), rect);
        }
    }
}

fn paint_key(app: &mut App, ctx: &mut PaintCtx, rect: Rect, key: Key) {
    let variant = if key == Key::Equals {
        ButtonVariant::Suggested
    } else if matches!(
        key,
        Key::Clear
            | Key::Backspace
            | Key::Negate
            | Key::Sin
            | Key::Cos
            | Key::Tan
            | Key::Asin
            | Key::Acos
            | Key::Atan
            | Key::Ln
            | Key::Log
            | Key::Sqrt
            | Key::Abs
            | Key::Mod
            | Key::Exp
            | Key::TenPow
            | Key::Ans
    ) {
        ButtonVariant::Flat
    } else {
        ButtonVariant::Default
    };

    let label = key.label(app.angle);
    let mut button = Button::new("").variant(variant);
    button.set_hovered(is_hovered(app, rect));
    button.paint(ctx, rect);

    // `Button` rysuje etykietę rozmiarem tekstu treści. W klawiaturze
    // kalkulatora chcemy większe cyfry, więc tło rysuje widget, a napis — my.
    let theme = *ctx.theme;
    let color = match variant {
        ButtonVariant::Suggested | ButtonVariant::Destructive => theme.colors.accent_fg,
        _ => theme.colors.text,
    };
    let mut size = (rect.height() * 0.36).clamp(15.0, 28.0);
    while ctx.measure(label, size, weight::MEDIUM) > rect.width() - 12.0 && size > 11.0 {
        size -= 1.0;
    }
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        label,
        size,
        weight::MEDIUM,
        color,
    );
}

/// Obsługuje zdarzenia wejścia.
pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    // Modalne okna i menu przechwytują wejście, zanim trafi ono do klawiatury.
    if app.about.is_some() {
        handle_about_events(app, events, size, theme, text);
        return;
    }
    if app.settings_open {
        handle_settings_events(app, events, size, theme);
        return;
    }
    if app.history_sheet.is_open() {
        handle_history_events(app, events, size, theme);
        return;
    }
    if app.menu.is_some() {
        handle_menu_events(app, events, size, theme, text);
        return;
    }

    let layout = chrome(size, theme, text);
    let header = HeaderBar::new().controls(ControlsSide::Right);

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                if let Some(control) = header.control_at(layout.header, theme, button.position) {
                    match control {
                        WindowControl::Minimize => app.pending_minimize = true,
                        WindowControl::Maximize => app.pending_maximize = true,
                        WindowControl::Close => app.pending_close = true,
                    }
                    continue;
                }

                if layout.menu.contains(button.position) {
                    app.open_menu(layout.menu);
                    continue;
                }

                let action = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, rect)| rect.contains(button.position))
                    .map(|(action, _)| *action);
                if let Some(action) = action {
                    apply_action(app, action);
                } else if layout.header.contains(button.position) {
                    app.pending_move = true;
                }
            }
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                app.controls_hover = header.control_at(layout.header, theme, *position);
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.controls_hover = None;
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                handle_key(app, key);
            }
            // Wklejanie ze schowka; wpisywanie klawiszy idzie przez `handle_key`.
            Event::Paste(input) => app.insert_text(&input),
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_menu_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let anchor = menu_anchor(size, theme, text);
    if let Some(state) = &mut app.menu {
        state.anchor = anchor;
    }
    let Some(panel) = app
        .menu
        .as_ref()
        .map(|state| menu_panel(state, size, theme, text))
    else {
        return;
    };

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                let index = app
                    .menu
                    .as_ref()
                    .and_then(|state| state.menu.index_at(panel, button.position, theme));
                if let Some(index) = index {
                    app.run_menu(index);
                } else {
                    // Klik poza menu (także w przycisk burger) zamyka panel.
                    app.menu = None;
                }
                break;
            }
            Event::PointerMotion { position, .. } => {
                if let Some(state) = &mut app.menu {
                    state.menu.hover_at(panel, Some(*position), theme);
                }
                app.pointer = Some(*position);
                app.controls_hover = None;
            }
            Event::PointerLeave => {
                if let Some(state) = &mut app.menu {
                    state.menu.hover_at(panel, None, theme);
                }
                app.pointer = None;
                app.controls_hover = None;
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if key.keysym == KEY_ESCAPE {
                    app.menu = None;
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_COMMA {
                    app.open_settings();
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_H {
                    app.open_history();
                    break;
                }
                if key.keysym == KEY_F1 {
                    app.show_about();
                    break;
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_about_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                let close = app.about.as_ref().is_some_and(|about| {
                    let rects = about.rects(size, theme, text);
                    about
                        .action_at(size, theme, text, button.position)
                        .is_some()
                        || rects.dialog.close.contains(button.position)
                        || !rects.dialog.panel.contains(button.position)
                });
                if close {
                    app.about = None;
                    break;
                }
            }
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == KEY_ESCAPE => {
                app.about = None;
                break;
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_history_events(app: &mut App, events: &[Event], size: Size, theme: &Theme) {
    let lay = history_layout(&app.history_sheet, size, theme);

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                if app.history_sheet.close_at(size, theme, button.position)
                    || app.history_sheet.scrim_at(size, theme, button.position)
                {
                    app.close_history();
                    break;
                }
                let action = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, rect)| rect.contains(button.position))
                    .map(|(action, _)| *action);
                if let Some(action) = action {
                    apply_action(app, action);
                    break;
                }
            }
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            Event::Scroll(scroll) if lay.list.contains(scroll.position) => {
                app.history_scroll
                    .on_scroll(scroll.notches_y() * 36.0, lay.list);
            }
            Event::Key(key)
                if key.state == ButtonState::Pressed
                    && (key.keysym == KEY_ESCAPE
                        || (key.modifiers.ctrl && key.keysym == KEY_H)) =>
            {
                app.close_history();
                break;
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_settings_events(app: &mut App, events: &[Event], size: Size, theme: &Theme) {
    let dialog = settings_dialog();
    let lay = settings_layout(size, theme);

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                if dialog.close_at(size, theme, button.position)
                    || dialog.scrim_at(size, theme, button.position)
                    || lay.done.contains(button.position)
                {
                    app.settings_open = false;
                    app.save_prefs();
                    break;
                }

                let angle = ViewSwitcher::new(["DEG", "RAD"]);
                if let Some(index) = angle.index_at(lay.angle_seg, button.position) {
                    app.set_angle(if index == 0 {
                        AngleMode::Degrees
                    } else {
                        AngleMode::Radians
                    });
                }

                let mode = ViewSwitcher::new([
                    libluna_i18n::tr("Standard"),
                    libluna_i18n::tr("Scientific"),
                ]);
                if let Some(index) = mode.index_at(lay.mode_seg, button.position) {
                    app.set_start_mode(Mode::from_index(index));
                }
            }
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            Event::Key(key)
                if key.state == ButtonState::Pressed
                    && (key.keysym == KEY_ESCAPE
                        || (key.modifiers.ctrl && key.keysym == KEY_COMMA)) =>
            {
                app.settings_open = false;
                app.save_prefs();
                break;
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Mode(mode) => app.mode = mode,
        Action::Press(key) => app.press(key),
        Action::History(index) => app.use_history(index),
        Action::ClearHistory => app.clear_history(),
    }
}

fn handle_key(app: &mut App, key: &KeyEvent) {
    if key.modifiers.ctrl {
        match key.keysym {
            KEY_C => app.clipboard_text = Some(app.copy_text()),
            KEY_H => app.open_history(),
            KEY_V => app.pending_paste = true,
            KEY_COMMA => app.open_settings(),
            _ => {}
        }
        return;
    }

    match key.keysym {
        KEY_ESCAPE | KEY_DELETE => app.press(Key::Clear),
        KEY_ENTER | KEY_KP_ENTER => app.press(Key::Equals),
        KEY_BACKSPACE => app.press(Key::Backspace),
        KEY_F1 => app.show_about(),
        _ => {
            // Wpisywanie liczb i operatorów. Platforma dostarcza tekst w
            // `KeyEvent.text` (a nie w `Event::Text`), więc to jedyna ścieżka.
            if let Some(text) = &key.text {
                if !text.is_empty() && !text.chars().any(char::is_control) {
                    app.insert_text(text);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::{Point, PointerEvent};

    fn paint_standard(app: &mut App) -> (Size, Theme, TextEngine) {
        let size = Size::new(900.0, 680.0);
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(app, &mut scene, size, &theme, &text);
        (size, theme, text)
    }

    fn click(app: &mut App, size: Size, theme: &Theme, text: &TextEngine, point: Point) {
        let event = Event::PointerButton(PointerEvent {
            position: point,
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Default::default(),
        });
        handle_events(app, &[event], size, theme, text);
    }

    #[test]
    fn keypad_clicks_reach_the_app() {
        let mut app = App::new();
        let (size, theme, text) = paint_standard(&mut app);
        let seven = app
            .hits
            .iter()
            .find_map(|(action, rect)| (*action == Action::Press(Key::Digit(7))).then_some(*rect))
            .expect("klawisz 7 powinien mieć region kliknięcia");
        click(&mut app, size, &theme, &text, seven.center());
        assert_eq!(app.expression, "7");
    }

    #[test]
    fn burger_opens_menu() {
        let mut app = App::new();
        let (size, theme, text) = paint_standard(&mut app);
        let layout = chrome(size, &theme, &text);
        click(&mut app, size, &theme, &text, layout.menu.center());
        assert!(app.menu.is_some());
    }

    #[test]
    fn settings_dialog_changes_angle_and_start_mode() {
        let mut app = App::new();
        app.open_settings();
        let (size, theme, text) = paint_standard(&mut app);
        let lay = settings_layout(size, &theme);

        let rad = ViewSwitcher::new(["DEG", "RAD"]).segments(lay.angle_seg)[1].center();
        click(&mut app, size, &theme, &text, rad);
        assert_eq!(app.angle, AngleMode::Radians);

        let scientific =
            ViewSwitcher::new(["Standard", "Scientific"]).segments(lay.mode_seg)[1].center();
        click(&mut app, size, &theme, &text, scientific);
        assert_eq!(app.prefs.start_mode, Mode::Scientific);

        click(&mut app, size, &theme, &text, lay.done.center());
        assert!(!app.settings_open);
    }

    #[test]
    fn history_row_click_inserts_result() {
        let mut app = App::new();
        app.history.add("2+2", "4");
        app.open_history();
        let (size, theme, text) = paint_standard(&mut app);
        let row = app
            .hits
            .iter()
            .find_map(|(action, rect)| (*action == Action::History(0)).then_some(*rect))
            .expect("wiersz historii powinien mieć region kliknięcia");
        click(&mut app, size, &theme, &text, row.center());
        assert_eq!(app.expression, "4");
        assert!(!app.history_sheet.is_open());
    }

    #[test]
    fn keyboard_shortcuts_open_dialogs() {
        let mut app = App::new();
        let (size, theme, text) = paint_standard(&mut app);

        handle_events(&mut app, &[key_event(KEY_H, true)], size, &theme, &text);
        assert!(app.history_sheet.is_open());
        handle_events(
            &mut app,
            &[key_event(KEY_ESCAPE, false)],
            size,
            &theme,
            &text,
        );
        assert!(!app.history_sheet.is_open());

        handle_events(&mut app, &[key_event(KEY_COMMA, true)], size, &theme, &text);
        assert!(app.settings_open);
        handle_events(
            &mut app,
            &[key_event(KEY_ESCAPE, false)],
            size,
            &theme,
            &text,
        );
        assert!(!app.settings_open);

        handle_events(&mut app, &[key_event(KEY_F1, false)], size, &theme, &text);
        assert!(app.about.is_some());
    }

    fn key_event(keysym: u32, ctrl: bool) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: libluna_core::Modifiers {
                ctrl,
                ..Default::default()
            },
            repeat: false,
        })
    }

    #[test]
    fn mode_switch_click_changes_mode() {
        let mut app = App::new();
        let (size, theme, text) = paint_standard(&mut app);
        let scientific = app
            .hits
            .iter()
            .find_map(|(action, rect)| (*action == Action::Mode(Mode::Scientific)).then_some(*rect))
            .expect("tryb naukowy powinien mieć region kliknięcia");
        click(&mut app, size, &theme, &text, scientific.center());
        assert_eq!(app.mode, Mode::Scientific);

        paint(&mut app, &mut Scene::new(), size, &theme, &text);
        assert!(app
            .hits
            .iter()
            .any(|(action, _)| *action == Action::Press(Key::Sin)));
    }
}
