//! Rysowanie i obsługa wejścia menedżera schowka.
//!
//! Układ liczymy w jednym miejscu ([`layout`]), więc trafienia klawiszy i myszy
//! nie mogą się rozjechać z pikselami. Lista jest przycinana do prostokąta,
//! dzięki czemu przewijanie nie rysuje wierszy na pasku tytułu.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, KeyEvent, Point, PointerButton, Rect, Size, Theme, WindowEvent,
};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, Button, ButtonVariant, ControlsSide, HeaderBar, IconName, PaintCtx, TitleAlign,
    WindowControl,
};

use crate::app::{App, Hit};

/// Wysokość wiersza listy (wspólna z przewijaniem i trafieniami).
pub const ROW_HEIGHT: f64 = 46.0;

const HEADER_HEIGHT: f64 = 46.0;
const FOOTER_HEIGHT: f64 = 30.0;
const PAD: f64 = 14.0;
const PIN_BUTTON: f64 = 26.0;
/// Ile znaków pierwszego wiersza bierzemy do podglądu.
///
/// Wpis może mieć megabajty (długi log, cały plik) — do wiersza i tak zmieści
/// się kilkadziesiąt znaków, więc czytanie dalej tylko marnuje pamięć.
const PREVIEW_CHARS: usize = 400;
/// Od ilu linii przestajemy liczyć (etykieta „{n} linii” jest poglądowa).
const MAX_COUNTED_LINES: usize = 10_000;

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
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
const KEY_F: u32 = 0x66;
const KEY_P: u32 = 0x70;
const KEY_Q: u32 = 0x71;

struct Layout {
    header: Rect,
    search: Rect,
    list: Rect,
    footer: Rect,
}

fn layout(app: &App, size: Size, theme: &Theme) -> Layout {
    let header = Rect::from_xywh(0.0, 0.0, size.width, HEADER_HEIGHT);
    let search_h = app.search.height(theme);
    let search = Rect::from_xywh(
        PAD,
        header.bottom() + 8.0,
        (size.width - 2.0 * PAD).max(0.0),
        search_h,
    );
    let footer = Rect::from_xywh(0.0, size.height - FOOTER_HEIGHT, size.width, FOOTER_HEIGHT);
    let top = search.bottom() + 6.0;
    let list = Rect::from_xywh(0.0, top, size.width, (footer.y() - top).max(0.0));
    Layout {
        header,
        search,
        list,
        footer,
    }
}

/// „teraz”, „5 min temu”, „1 godz. temu”, „3 dni temu” (z liczbą pojedynczą).
pub fn format_age(secs: u64) -> String {
    if secs < 60 {
        return tr("just now").to_owned();
    }
    if secs < 3_600 {
        let n = secs / 60;
        return if n == 1 {
            tr("1 min ago").to_owned()
        } else {
            tr_args("{n} min ago", &[("n", &n.to_string())])
        };
    }
    if secs < 86_400 {
        let n = secs / 3_600;
        return if n == 1 {
            tr("1 h ago").to_owned()
        } else {
            tr_args("{n} h ago", &[("n", &n.to_string())])
        };
    }
    let n = secs / 86_400;
    if n == 1 {
        tr("1 d ago").to_owned()
    } else {
        tr_args("{n} d ago", &[("n", &n.to_string())])
    }
}

/// Jedna linia podglądu: pierwsza niepusta linia, bez znaków sterujących.
///
/// Czytamy tylko pierwsze [`MAX_COUNTED_LINES`] linii i pierwsze
/// [`PREVIEW_CHARS`] znaków — reszta nie ma szans się zmieścić.
fn preview_line(text: &str) -> String {
    let line = text
        .lines()
        .take(64)
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    line.chars()
        .take(PREVIEW_CHARS)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Liczba niepustych linii, ograniczona do [`MAX_COUNTED_LINES`].
fn count_lines(text: &str) -> usize {
    text.lines()
        .take(MAX_COUNTED_LINES)
        .filter(|line| !line.trim().is_empty())
        .count()
}

pub fn paint(app: &mut App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    app.hits.clear();
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene.fill_rect(
        Rect::from_xywh(0.0, 0.0, size.width, size.height),
        theme.colors.window_bg,
    );

    let lay = layout(app, size, theme);
    app.viewport = lay.list.height();
    app.clamp_scroll();

    let header = HeaderBar::new()
        .title(tr("Clipboard"))
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right);
    header.paint(&mut ctx, lay.header, app.controls_hover);

    app.search.paint(&mut ctx, lay.search);
    paint_list(app, &mut ctx, &lay);
    paint_footer(app, &mut ctx, &lay);

    if app.about {
        paint_about(app, &mut ctx, size);
    }
}

fn paint_list(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    if app.visible.is_empty() {
        paint_empty(ctx, lay.list);
        return;
    }
    ctx.scene.push_clip(lay.list);
    // Kopia indeksów: `paint_row` potrzebuje `&mut App` (gwiazdka dopisuje trafienia).
    let rows: Vec<(usize, usize)> = app.visible.iter().copied().enumerate().collect();
    for (row, entry_index) in rows {
        let top = lay.list.y() + row as f64 * ROW_HEIGHT - app.scroll;
        let rect = Rect::from_xywh(0.0, top, lay.list.width(), ROW_HEIGHT);
        if rect.bottom() < lay.list.y() || rect.y() > lay.list.bottom() {
            continue;
        }
        paint_row(app, ctx, rect, row, entry_index);
    }
    ctx.scene.pop_clip();

    paint_scrollbar(app, ctx, lay);
}

fn paint_row(app: &mut App, ctx: &mut PaintCtx, rect: Rect, row: usize, entry_index: usize) {
    let theme = *ctx.theme;
    let selected = row == app.selected;
    let hovered = app.hover == Some(row);
    // Bierzemy tylko to, czego potrzebuje wiersz — kopiowanie całego wpisu
    // na każdą klatkę przy dużym schowku było marnotrawstwem pamięci.
    let Some((pinned, preview, lines, age)) = app.history.entries.get(entry_index).map(|entry| {
        (
            entry.pinned,
            preview_line(&entry.text),
            count_lines(&entry.text),
            app.age_label(entry),
        )
    }) else {
        return;
    };

    if selected {
        ctx.scene
            .fill_rect(rect, theme.colors.accent.scale_alpha(0.16));
        // Pasek akcentu po lewej stronie — zaznaczenie widać też na jasnym tle.
        ctx.scene.fill_rect(
            Rect::from_xywh(rect.x(), rect.y(), 3.0, rect.height()),
            theme.colors.accent,
        );
    } else if hovered {
        ctx.scene
            .fill_rect(rect, theme.colors.shade.scale_alpha(1.5));
    }
    ctx.scene.fill_rect(
        Rect::from_xywh(
            rect.x() + PAD,
            rect.bottom() - 1.0,
            rect.width() - 2.0 * PAD,
            1.0,
        ),
        theme.colors.border.scale_alpha(0.45),
    );

    // Przypięty wpis dostaje gwiazdkę; klik w nią odpina/przypina.
    let pin_rect = Rect::from_xywh(
        rect.right() - PAD - PIN_BUTTON,
        rect.center().y - PIN_BUTTON / 2.0,
        PIN_BUTTON,
        PIN_BUTTON,
    );
    app.hits.push((Hit::Pin(entry_index), pin_rect));
    if pinned || hovered || selected {
        let color = if pinned {
            theme.colors.accent
        } else {
            theme.colors.dim_text.scale_alpha(0.6)
        };
        paint_icon(
            ctx,
            IconName::Star,
            Rect::from_xywh(
                pin_rect.center().x - 8.0,
                pin_rect.center().y - 8.0,
                16.0,
                16.0,
            ),
            color,
        );
    }

    let age_width = age
        .as_deref()
        .map(|label| ctx.measure(label, theme.typography.caption.size, weight::NORMAL) + 12.0)
        .unwrap_or(0.0);

    let text_left = rect.x() + PAD;
    let text_width = (pin_rect.x() - 8.0 - text_left).max(0.0);
    let line = if preview.is_empty() {
        tr("(empty)").to_owned()
    } else {
        preview
    };
    let body_size = theme.typography.body.size;

    ctx.label_left(
        text_left,
        rect.center().y - body_size * 0.42,
        &fit(
            &line,
            (text_width - age_width).max(60.0),
            body_size,
            weight::NORMAL,
            ctx.text,
        ),
        body_size,
        weight::NORMAL,
        theme.colors.text,
    );
    if let Some(age) = age {
        ctx.label_right(
            rect.right() - PAD - PIN_BUTTON - 8.0,
            rect.center().y - body_size * 0.42,
            &age,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    if lines > 1 {
        let more = tr_args("{n} lines", &[("n", &lines.to_string())]);
        ctx.label_left(
            text_left,
            rect.center().y + body_size * 0.62,
            &more,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
}

fn paint_scrollbar(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let content = app.visible.len() as f64 * ROW_HEIGHT;
    if content <= lay.list.height() {
        return;
    }
    let track = Rect::from_xywh(
        lay.list.right() - 6.0,
        lay.list.y() + 4.0,
        4.0,
        (lay.list.height() - 8.0).max(0.0),
    );
    ctx.scene
        .fill_rounded_rect(track, 2.0, ctx.theme.colors.shade.scale_alpha(1.4));
    let ratio = (lay.list.height() / content).clamp(0.08, 1.0);
    let thumb_h = (track.height() * ratio).max(20.0);
    let max_scroll = (content - lay.list.height()).max(1.0);
    let offset = (app.scroll / max_scroll) * (track.height() - thumb_h);
    let thumb = Rect::from_xywh(track.x(), track.y() + offset, track.width(), thumb_h);
    ctx.scene
        .fill_rounded_rect(thumb, 2.0, ctx.theme.colors.dim_text.scale_alpha(0.55));
}

fn paint_empty(ctx: &mut PaintCtx, list: Rect) {
    let theme = *ctx.theme;
    let center = Point::new(list.center().x, list.center().y - 18.0);
    ctx.scene
        .fill_circle(center, 26.0, theme.colors.accent.scale_alpha(0.12));
    paint_icon(
        ctx,
        IconName::Edit,
        Rect::from_xywh(center.x - 13.0, center.y - 13.0, 26.0, 26.0),
        theme.colors.accent,
    );
    ctx.label_center(
        center.x,
        center.y + 46.0,
        &fit(
            &tr("The clipboard is empty"),
            (list.width() - 32.0).max(0.0),
            theme.typography.body.size,
            weight::MEDIUM,
            ctx.text,
        ),
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    ctx.label_center(
        center.x,
        center.y + 68.0,
        &fit(
            &tr("Copy something and it will show up here"),
            (list.width() - 32.0).max(0.0),
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn paint_footer(app: &mut App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let label = match &app.toast {
        Some((message, _)) => message.clone(),
        None if app.visible.is_empty() => String::new(),
        None => tr_args("{n} entries", &[("n", &app.visible.len().to_string())]),
    };
    let color = if app.toast.is_some() {
        theme.colors.accent
    } else {
        theme.colors.dim_text
    };
    ctx.label_left(
        PAD,
        lay.footer.center().y,
        &label,
        theme.typography.caption.size,
        weight::NORMAL,
        color,
    );
    let hint = format!("{}  ·  {}", tr("Enter copies"), tr("Ctrl+P pins"));
    // Podpowiedź chowamy, gdy zabraknie miejsca — inaczej nachodzi na licznik.
    let cap = theme.typography.caption.size;
    let left_width = ctx.measure(&label, cap, weight::NORMAL);
    let hint_width = ctx.measure(&hint, cap, weight::NORMAL);
    if lay.footer.width() - 2.0 * PAD - left_width - 12.0 >= hint_width {
        ctx.label_right(
            lay.footer.right() - PAD,
            lay.footer.center().y,
            &hint,
            cap,
            weight::NORMAL,
            theme.colors.dim_text.scale_alpha(0.8),
        );
    }
}

fn about_rect(size: Size) -> Rect {
    Rect::from_xywh(
        (size.width - 360.0) / 2.0,
        (size.height - 236.0) / 2.0,
        360.0,
        236.0,
    )
}

fn paint_about(app: &mut App, ctx: &mut PaintCtx, size: Size) {
    let theme = *ctx.theme;
    ctx.scene.fill_rect(
        Rect::from_xywh(0.0, 0.0, size.width, size.height),
        Color::rgba(0.0, 0.0, 0.0, 0.35),
    );
    let card = about_rect(size);
    ctx.scene
        .fill_rounded_rect(card, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(card, theme.radius.large, 1.0, theme.colors.border);

    ctx.label_center(
        card.center().x,
        card.y() + 40.0,
        &tr("Clipboard"),
        theme.typography.title.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_center(
        card.center().x,
        card.y() + 66.0,
        &format!("{} {}", tr("Version"), env!("CARGO_PKG_VERSION")),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let max_text = (card.width() - 32.0).max(0.0);
    ctx.label_center(
        card.center().x,
        card.y() + 96.0,
        &fit(
            &tr("Clipboard history stays on this computer"),
            max_text,
            theme.typography.body.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
    ctx.label_center(
        card.center().x,
        card.y() + 118.0,
        &fit(
            &tr("Stored in ~/.config/luna/clipboard.conf (mode 0600)"),
            max_text,
            theme.typography.caption.size,
            weight::NORMAL,
            ctx.text,
        ),
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    // Przycisk dopasowuje szerokość do etykiety (tłumaczenia mają różną długość).
    let button = Button::new(tr("Close dialog")).variant(ButtonVariant::Suggested);
    let width = button.width(ctx.text, &theme).max(88.0);
    let height = button.height(&theme).max(32.0);
    let rect = Rect::from_xywh(
        card.center().x - width / 2.0,
        card.bottom() - 20.0 - height,
        width,
        height,
    );
    app.hits.push((Hit::CloseAbout, rect));
    button.paint(ctx, rect);
}

/// Skraca napis wielokropkiem do `max_width`.
fn fit(text: &str, max_width: f64, size: f64, weight: u16, engine: &TextEngine) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if engine.measure(text, size, weight).width <= max_width {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = engine.measure(dots, size, weight).width;
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if engine.measure(&probe, size, weight).width + dots_w > max_width {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

fn row_at(app: &App, lay: &Layout, point: Point) -> Option<usize> {
    if !lay.list.contains(point) {
        return None;
    }
    let offset = point.y - lay.list.y() + app.scroll;
    if offset < 0.0 {
        return None;
    }
    let index = (offset / ROW_HEIGHT).floor() as usize;
    (index < app.visible.len()).then_some(index)
}

pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme);
    app.viewport = lay.list.height();
    app.clamp_scroll();
    let header = HeaderBar::new().controls(ControlsSide::Right);
    app.search.set_focused(true);

    for event in events {
        if crate::app::handle_event(app, event) {
            continue;
        }
        match event {
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                app.hover = row_at(app, &lay, *position);
                app.controls_hover = header.control_at(lay.header, theme, *position);
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.hover = None;
                app.controls_hover = None;
            }
            Event::PointerButton(button)
                if button.state == ButtonState::Pressed && button.button == PointerButton::Left =>
            {
                if let Some(control) = header.control_at(lay.header, theme, button.position) {
                    match control {
                        WindowControl::Minimize => app.pending_minimize = true,
                        WindowControl::Maximize => app.pending_maximize = true,
                        WindowControl::Close => app.pending_close = true,
                    }
                    continue;
                }
                // Przyciski (gwiazdka, „Zamknij” w oknie O aplikacji) mają
                // pierwszeństwo przed wierszem pod nimi.
                // „×” w polu filtra: czyści frazę, nie ruszając listy.
                if !app.about && app.search.clear_at(lay.search, theme, button.position) {
                    app.search.clear();
                    app.recompute();
                    continue;
                }
                let hit = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, rect)| rect.contains(button.position))
                    .map(|(hit, _)| *hit);
                match hit {
                    Some(Hit::Pin(index)) => {
                        app.history.toggle_pin(index);
                        app.dirty = true;
                    }
                    Some(Hit::CloseAbout) => app.about = false,
                    Some(Hit::Remove(_)) => {}
                    None => {
                        if app.about {
                            // Klik poza kartą zamyka okno „O aplikacji”.
                            if !about_rect(size).contains(button.position) {
                                app.about = false;
                            }
                            continue;
                        }
                        if let Some(row) = row_at(app, &lay, button.position) {
                            app.selected = row;
                            app.ensure_visible();
                            if app.register_click(row) {
                                app.copy_selected();
                            }
                        } else if lay.header.contains(button.position) {
                            app.pending_move = true;
                        }
                    }
                }
            }
            Event::Scroll(scroll) => {
                app.scroll -= scroll.dy;
                app.clamp_scroll();
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                handle_key(app, key, &lay);
            }
            Event::Window(WindowEvent::Focused(true)) => {
                app.search.set_focused(true);
            }
            _ => {}
        }
    }
    let _ = text;
}

fn handle_key(app: &mut App, key: &KeyEvent, lay: &Layout) {
    let ctrl = key.modifiers.ctrl;
    match key.keysym {
        KEY_ESCAPE => {
            if app.about {
                app.about = false;
            } else if !app.search.is_empty() {
                app.search.clear();
                app.recompute();
            } else {
                app.pending_close = true;
            }
        }
        KEY_RETURN | KEY_KP_ENTER if !app.about => app.copy_selected(),
        KEY_F1 => app.about = !app.about,
        KEY_DELETE if !app.about => app.remove_selected(),
        KEY_UP => app.move_selection(-1),
        KEY_DOWN => app.move_selection(1),
        KEY_PAGE_UP => {
            let page = (lay.list.height() / ROW_HEIGHT).floor().max(1.0) as i32;
            app.move_selection(-page);
        }
        KEY_PAGE_DOWN => {
            let page = (lay.list.height() / ROW_HEIGHT).floor().max(1.0) as i32;
            app.move_selection(page);
        }
        // `Ctrl+Home`/`Ctrl+End` skaczą na początek/koniec listy, a samo
        // `Home`/`End` porusza kursorem w polu filtra (jak w każdym polu tekstowym).
        KEY_HOME if ctrl => app.select_first(),
        KEY_END if ctrl => app.select_last(),
        KEY_HOME => app.search.home(),
        KEY_END => app.search.end(),
        KEY_BACKSPACE => {
            app.search.backspace();
            app.recompute();
        }
        KEY_LEFT => app.search.move_left(),
        KEY_RIGHT => app.search.move_right(),
        KEY_F if ctrl => {
            app.search.set_text("");
            app.recompute();
        }
        KEY_P if ctrl => app.toggle_pin(),
        KEY_Q if ctrl => app.pending_close = true,
        _ => {
            // Wpisywanie do pola filtra. Wcześniej klawiatura obsługiwała tylko
            // strzałki i skróty, więc pisanie nie robiło nic.
            // AltGr (`level3`) bywa zgłaszany jako ctrl+alt — nie blokujemy go,
            // żeby dało się wpisać ą/ć/ę/ł/ń/ó/ś/ź/ż.
            let plain = (!ctrl && !key.modifiers.logo) || key.modifiers.level3;
            if plain {
                if let Some(text) = &key.text {
                    if !text.is_empty() && !text.chars().any(char::is_control) {
                        app.search.insert_str(text);
                        app.recompute();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::History;
    use libluna_core::{Modifiers, PointerEvent};

    fn app_with(texts: &[&str]) -> App {
        let mut app = App::launch();
        app.history = History::default();
        for (index, text) in texts.iter().enumerate() {
            app.history.push_at(text, index as u64 + 1);
        }
        app.recompute();
        app
    }

    #[test]
    fn typing_filters_the_list_through_key_events() {
        let mut app = app_with(&["cargo test", "git status"]);
        let lay = layout(&app, Size::new(460.0, 560.0), &Theme::dark());
        for ch in ["g", "i", "t"] {
            let key = KeyEvent {
                keysym: ch.chars().next().unwrap_or_default() as u32,
                keycode: 0,
                text: Some(ch.to_owned()),
                state: ButtonState::Pressed,
                modifiers: libluna_core::Modifiers::NONE,
                repeat: false,
            };
            handle_key(&mut app, &key, &lay);
        }
        assert_eq!(app.search.text(), "git");
        assert_eq!(app.visible.len(), 1, "filtr zawęża listę do jednego wpisu");
    }

    #[test]
    fn typing_with_ctrl_does_not_insert_text() {
        let mut app = app_with(&["cargo test"]);
        let lay = layout(&app, Size::new(460.0, 560.0), &Theme::dark());
        let mut modifiers = libluna_core::Modifiers::NONE;
        modifiers.ctrl = true;
        let key = KeyEvent {
            keysym: 'v' as u32,
            keycode: 0,
            text: Some("v".to_owned()),
            state: ButtonState::Pressed,
            modifiers,
            repeat: false,
        };
        handle_key(&mut app, &key, &lay);
        assert!(app.search.is_empty(), "Ctrl+V to nie tekst do filtra");
    }

    #[test]
    fn clicking_the_clear_button_empties_the_filter() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(460.0, 560.0);
        let mut app = app_with(&["cargo test", "git status"]);
        app.search.set_text("git");
        app.recompute();
        assert_eq!(app.visible.len(), 1);

        let lay = layout(&app, size, &theme);
        let clear = app.search.clear_rect(lay.search, &theme).center();
        let click = Event::PointerButton(PointerEvent {
            position: clear,
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        });
        handle_events(&mut app, &[click], size, &theme, &text);

        assert!(app.search.is_empty(), "„×” czyści frazę");
        assert_eq!(app.visible.len(), 2, "lista wraca do pełnej historii");
    }

    #[test]
    fn age_labels_switch_units() {
        assert_eq!(format_age(5), tr("just now"));
        assert!(format_age(600).contains('1'));
        assert!(format_age(7_200).contains('2'));
        assert!(format_age(3 * 86_400).contains('3'));
        // Liczba pojedyncza ma własne tłumaczenie („1 dzień temu”, nie „1 dni”).
        assert_eq!(format_age(60), tr("1 min ago"));
        assert_eq!(format_age(3_600), tr("1 h ago"));
        assert_eq!(format_age(86_400), tr("1 d ago"));
    }

    #[test]
    fn preview_uses_first_non_empty_line_without_control_characters() {
        assert_eq!(preview_line("\n\n  pierwsza \n druga"), "pierwsza");
        assert_eq!(preview_line("a\tb"), "a b");
        assert_eq!(preview_line("   "), "");
    }

    #[test]
    fn row_hit_testing_accounts_for_scrolling() {
        let app = app_with(&["a", "b", "c"]);
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app, Size::new(400.0, 600.0), &theme);
        let point = Point::new(40.0, lay.list.y() + ROW_HEIGHT * 1.5);
        assert_eq!(row_at(&app, &lay, point), Some(1));
        // Poza listą (np. na pasku tytułu) nic nie trafiamy.
        assert_eq!(row_at(&app, &lay, Point::new(40.0, 4.0)), None);
        let _ = text;
    }

    #[test]
    fn double_click_is_detected_only_on_the_same_row() {
        let mut app = app_with(&["a", "b"]);
        assert!(!app.register_click(0));
        assert!(app.register_click(0));
        assert!(!app.register_click(1));
    }
}

#[cfg(test)]
mod preview_tests {
    use super::{count_lines, preview_line, MAX_COUNTED_LINES, PREVIEW_CHARS};

    #[test]
    fn preview_is_bounded_and_strips_control_characters() {
        let long = "x".repeat(PREVIEW_CHARS * 3);
        assert_eq!(preview_line(&long).chars().count(), PREVIEW_CHARS);
        assert_eq!(preview_line("\n\n  hello\tworld  "), "hello world");
        assert_eq!(preview_line(""), "");
    }

    #[test]
    fn line_count_stops_at_the_cap() {
        let text = "a\n".repeat(MAX_COUNTED_LINES + 500);
        assert_eq!(count_lines(&text), MAX_COUNTED_LINES);
        assert_eq!(count_lines("a\n\nb\n"), 2);
    }
}

#[cfg(test)]
mod input_contract_tests {
    use super::*;
    use crate::history::History;

    #[test]
    fn event_text_is_ignored_because_keys_already_carry_text() {
        // Platforma wysyła i `KeyEvent.text`, i `Event::Text`. Schowek czyta
        // pierwsze źródło — obsługa obu dublowałaby znaki.
        let mut app = App::launch();
        app.history = History::default();
        app.history.push_at("cargo test", 1);
        app.recompute();
        let size = Size::new(460.0, 560.0);
        let theme = Theme::dark();
        let text = TextEngine::none();
        handle_events(
            &mut app,
            &[Event::Text("git".to_owned())],
            size,
            &theme,
            &text,
        );
        assert!(app.search.is_empty(), "Event::Text jest ignorowane");
    }
}
