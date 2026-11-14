//! Rysowanie i obsługa zdarzeń kalkulatora.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Event, Insets, KeyEvent, PointerButton, Rect, Size, Theme, WindowEvent,
};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    Button, ButtonVariant, ControlsSide, HeaderBar, PaintCtx, TitleAlign, ViewSwitcher,
    WindowControl,
};

use crate::app::{Action, App, Key, Mode};

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_ENTER: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_DELETE: u32 = 0xffff;
const KEY_C: u32 = 0x63;
const KEY_V: u32 = 0x76;

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
    let title_end = available.x() + title_width + 20.0;
    let free = (available.right() - title_end).max(0.0);
    let tabs_w = free.min(260.0);
    let tabs = Rect::from_xywh(
        title_end + (free - tabs_w).max(0.0),
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
        content,
    }
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
    paint_content(app, &mut ctx, layout.content);
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
        Mode::Scientific => {
            let left_w = ((below.width() - gap) / 2.0).floor();
            let right_w = (below.width() - left_w - gap).max(0.0);
            let height = below.height().min(520.0);
            let y = below.y() + (below.height() - height).max(0.0) / 2.0;
            let left = Rect::from_xywh(below.x(), y, left_w, height);
            let right = Rect::from_xywh(below.x() + left_w + gap, y, right_w, height);
            paint_keypad(app, ctx, left, &SCIENTIFIC_ROWS);
            paint_keypad(app, ctx, right, &STANDARD_ROWS);
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
            | Key::Angle
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
            Event::Text(input) | Event::Paste(input) => app.insert_text(input),
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Mode(mode) => app.mode = mode,
        Action::Press(key) => app.press(key),
    }
}

fn handle_key(app: &mut App, key: &KeyEvent) {
    if key.modifiers.ctrl {
        match key.keysym {
            KEY_C => app.clipboard_text = Some(app.copy_text()),
            KEY_V => app.pending_paste = true,
            _ => {}
        }
        return;
    }

    match key.keysym {
        KEY_ESCAPE | KEY_DELETE => app.press(Key::Clear),
        KEY_ENTER | KEY_KP_ENTER => app.press(Key::Equals),
        KEY_BACKSPACE => app.press(Key::Backspace),
        _ => {}
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
