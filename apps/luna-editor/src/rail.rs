//! Prawa szyna ikon w edytorze: wtyczki + szybkie akcje.
//!
//! Manifest wtyczki nie ma pola `icon`, więc rysujemy **monogram** — kafelek
//! z pierwszą literą nazwy (ten sam zabieg co przy ikonach tray’a w panelu).
//! Po najechaniu pokazujemy dymek z nazwą, wersją i podpowiedzią, co zrobi
//! klik; dzięki temu szyna nie wymaga legendy ani stałego miejsca na etykiety.
//!
//! Klik na wtyczkę: jeśli deklaruje ustawienia — otwiera ich formularz,
//! w przeciwnym razie panel wtyczki z listą poleceń. Na dole szyny są
//! „Wtyczki” (menedżer) i „Ustawienia” edytora.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, IconTheme, Insets, Point, PointerButton, Rect, Size, Theme,
};
use libluna_i18n::tr;
use libluna_render::TextEngine;
use libluna_widgets::{
    hover_fill, paint_icon, paint_icon_themed, IconName, Menu, MenuEntry, PaintCtx,
};

use crate::app::App;

/// Keysym `Escape` dla zamykania menu z klawiatury.
const KEY_ESCAPE: u32 = 0xff1b;

/// Szerokość szyny (ikony 20 px + marginesy).
pub const RAIL_W: f64 = 46.0;
/// Bok kafelka wtyczki.
const ITEM: f64 = 34.0;
const GAP: f64 = 4.0;
const PAD: f64 = 6.0;

/// Co trafił kliknięcie w szynie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Plugin(usize),
    Manager,
    Settings,
}

/// Pozycja menu kontekstowego kafelka wtyczki.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Settings,
    Run,
    Update,
    Manage,
}

/// Otwarte menu kontekstowe szyny (PPM na kafelku wtyczki).
#[derive(Debug, Clone)]
pub struct RailMenu {
    pub menu: Menu,
    pub anchor: Rect,
    pub plugin: usize,
    pub actions: Vec<Option<MenuAction>>,
}

/// Czy szyna ma sens przy tej szerokości okna.
pub fn visible(size: Size) -> bool {
    size.width >= 620.0
}

/// Prostokąt szyny (od nagłówka do paska stanu).
pub fn rect(size: Size, header_bottom: f64, status_top: f64) -> Rect {
    let width = if visible(size) { RAIL_W } else { 0.0 };
    Rect::from_xywh(
        size.width - width,
        header_bottom,
        width,
        (status_top - header_bottom).max(0.0),
    )
}

/// Ile kafelków wtyczek mieści się nad stopką.
fn capacity(rail: Rect) -> usize {
    let footer = ITEM * 2.0 + GAP + PAD * 2.0 + 8.0;
    (((rail.height() - footer) / (ITEM + GAP)).floor() as usize).max(0)
}

fn plugin_rect(rail: Rect, index: usize) -> Rect {
    let x = rail.x() + (rail.width() - ITEM) / 2.0;
    let y = rail.y() + PAD + index as f64 * (ITEM + GAP);
    Rect::from_xywh(x, y, ITEM, ITEM)
}

fn settings_rect(rail: Rect) -> Rect {
    let x = rail.x() + (rail.width() - ITEM) / 2.0;
    Rect::from_xywh(x, rail.bottom() - PAD - ITEM, ITEM, ITEM)
}

fn manager_rect(rail: Rect) -> Rect {
    let settings = settings_rect(rail);
    Rect::from_xywh(settings.x(), settings.y() - ITEM - GAP, ITEM, ITEM)
}

/// Trafienie w szynie (`None` poza nią).
pub fn hit(app: &App, rail: Rect, point: Point) -> Option<Hit> {
    if rail.width() <= 0.0 || !rail.contains(point) {
        return None;
    }
    let count = app.plugins.plugins.len().min(capacity(rail));
    for index in 0..count {
        if plugin_rect(rail, index).contains(point) {
            return Some(Hit::Plugin(index));
        }
    }
    if manager_rect(rail).contains(point) {
        return Some(Hit::Manager);
    }
    if settings_rect(rail).contains(point) {
        return Some(Hit::Settings);
    }
    None
}

/// Obsługuje kliknięcia w szynie. Zwraca `true`, gdy zdarzenie zostało zużyte.
pub fn handle_click(app: &mut App, event: &Event, rail: Rect) -> bool {
    let Event::PointerButton(button) = event else {
        return false;
    };
    if button.state != ButtonState::Pressed {
        return false;
    }
    match button.button {
        PointerButton::Left => {
            match hit(app, rail, button.position) {
                Some(Hit::Plugin(index)) => activate(app, index),
                Some(Hit::Manager) => app.open_plugins(),
                Some(Hit::Settings) => app.settings_open = true,
                None => {}
            }
            // Klik w puste miejsce szyny też nie może trafić do edytora.
            rail.contains(button.position)
        }
        PointerButton::Right => {
            if let Some(Hit::Plugin(index)) = hit(app, rail, button.position) {
                open_menu(app, rail, index);
            }
            rail.contains(button.position)
        }
        _ => false,
    }
}

/// Menu kontekstowe kafelka: ustawienia, uruchomienie, zarządzanie.
pub fn open_menu(app: &mut App, rail: Rect, index: usize) {
    let Some(plugin) = app.plugins.plugins.get(index) else {
        return;
    };
    let has_settings = !plugin.settings.is_empty();
    let has_commands = !plugin.commands.is_empty();
    let has_source = crate::plugins::source_of(plugin).is_some();
    let settings = MenuEntry::action(tr("Settings"));
    let run = MenuEntry::action(tr("Run command…"));
    let update = MenuEntry::action(tr("Update plugin"));
    let entries = vec![
        MenuEntry::section(format!("{} · v{}", plugin.name, plugin.version)),
        if has_settings {
            settings
        } else {
            settings.disabled()
        },
        if has_commands { run } else { run.disabled() },
        MenuEntry::separator(),
        if has_source {
            update
        } else {
            update.disabled()
        },
        MenuEntry::separator(),
        MenuEntry::action(tr("Manage plugin")),
    ];
    app.plugin_ui.rail_menu = Some(RailMenu {
        menu: Menu::new(entries),
        anchor: plugin_rect(rail, index),
        plugin: index,
        actions: vec![
            None,
            Some(MenuAction::Settings),
            Some(MenuAction::Run),
            None,
            Some(MenuAction::Update),
            None,
            Some(MenuAction::Manage),
        ],
    });
}

/// Czy otwarte jest menu szyny (blokuje wejście do edytora).
pub fn menu_open(app: &App) -> bool {
    app.plugin_ui.rail_menu.is_some()
}

/// Obsługa zdarzeń otwartego menu szyny.
pub fn handle_menu_events(
    app: &mut App,
    event: &Event,
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let Some(state) = app.plugin_ui.rail_menu.clone() else {
        return;
    };
    let width = state.menu.width(text, theme);
    let panel = state
        .menu
        .place(state.anchor, width, Rect::from_size(size), theme);
    match event {
        Event::PointerMotion { position, .. } => {
            app.pointer = Some(*position);
            if let Some(open) = app.plugin_ui.rail_menu.as_mut() {
                open.menu.hover_at(panel, Some(*position), theme);
            }
        }
        Event::PointerButton(button)
            if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
        {
            let clicked = app
                .plugin_ui
                .rail_menu
                .as_mut()
                .and_then(|open| open.menu.click_at(panel, button.position, theme));
            if let Some(index) = clicked {
                let action = state.actions.get(index).copied().flatten();
                app.plugin_ui.rail_menu = None;
                apply_menu(app, state.plugin, action);
            } else if !panel.contains(button.position) {
                app.plugin_ui.rail_menu = None;
            }
        }
        Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == KEY_ESCAPE => {
            app.plugin_ui.rail_menu = None;
        }
        Event::Scroll(scroll) => {
            if panel.contains(scroll.position) {
                if let Some(open) = app.plugin_ui.rail_menu.as_mut() {
                    open.menu
                        .scroll_by(scroll.notches_y() * 64.0, panel.height(), theme);
                }
            }
        }
        _ => {}
    }
}

fn apply_menu(app: &mut App, plugin: usize, action: Option<MenuAction>) {
    match action {
        Some(MenuAction::Settings) => crate::plugin_ui::open_settings_for(app, plugin),
        Some(MenuAction::Run) => {
            // Prompt/panel muszą być na wierzchu, a nie pod starym formularzem.
            crate::plugin_ui::dismiss(app);
            run_command(app, plugin);
        }
        Some(MenuAction::Update) => {
            crate::plugin_ui::dismiss(app);
            app.plugin_ui.selected = plugin;
            app.plugin_ui.open = true;
            crate::plugin_ui::update_selected(app);
        }
        Some(MenuAction::Manage) => {
            crate::plugin_ui::dismiss(app);
            app.open_plugins();
            app.plugin_ui.selected = plugin;
        }
        None => {}
    }
}

/// Uruchamia jedyne polecenie bez pytania albo otwiera panel z listą poleceń.
fn run_command(app: &mut App, index: usize) {
    let Some(plugin) = app.plugins.plugins.get(index) else {
        return;
    };
    match plugin.commands.as_slice() {
        [only] if !only.prompt => crate::plugin_ui::start(app, index, 0),
        _ => {
            app.open_plugins();
            app.plugin_ui.selected = index;
        }
    }
}

/// Domyślna akcja kafelka wtyczki.
pub fn activate(app: &mut App, index: usize) {
    let Some(plugin) = app.plugins.plugins.get(index) else {
        return;
    };
    // Wtyczka czatu włącza/wyłącza panel — tak samo jak dawny przycisk w nagłówku.
    if plugin.chat {
        crate::chat::toggle(app);
        return;
    }
    if !plugin.settings.is_empty() {
        crate::plugin_ui::open_settings_for(app, index);
        return;
    }
    // Bez ustawień: jedyne polecenie bez pytania uruchamiamy od razu,
    // inaczej pokazujemy panel wtyczki z listą poleceń.
    run_command(app, index);
}

/// Rysuje szynę: kafelki wtyczek, stopkę i dymek aktywnego kafelka.
pub fn paint(app: &App, ctx: &mut PaintCtx, rail: Rect, _viewport: Size) {
    if rail.width() <= 0.0 {
        return;
    }
    let theme = *ctx.theme;
    ctx.scene.fill_rect(rail, theme.colors.sidebar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(rail.x(), rail.y(), 1.0, rail.height()),
        theme.colors.border,
    );

    let hover = app.pointer.and_then(|point| hit(app, rail, point));
    let count = app.plugins.plugins.len().min(capacity(rail));
    for index in 0..count {
        let rect = plugin_rect(rail, index);
        let Some(plugin) = app.plugins.plugins.get(index) else {
            continue;
        };
        // Wtyczka czatu jest aktywna, gdy widać panel; resztę podświetlamy,
        // gdy otwarty jest jej panel/ustawienia.
        let active = if plugin.chat {
            app.chat.visible
        } else {
            app.plugin_ui.open && app.plugin_ui.selected == index
        };
        paint_tile(ctx, rect, active, hover == Some(Hit::Plugin(index)));
        if plugin.chat {
            // Monochromatycznie jak reszta chrome’u; aktywny panel = akcent.
            let color = if app.chat.visible {
                theme.colors.accent
            } else {
                theme.colors.text
            };
            paint_icon_themed(
                ctx,
                IconName::Chat,
                rect.deflate(Insets::splat(9.0)),
                color,
                IconTheme::Monochrome,
            );
        } else {
            paint_monogram(ctx, rect, &plugin.name);
        }
    }

    // Stopka: menedżer wtyczek i ustawienia edytora.
    let manager = manager_rect(rail);
    let settings = settings_rect(rail);
    ctx.scene.fill_rect(
        Rect::from_xywh(
            rail.x() + 8.0,
            manager.y() - GAP - 2.0,
            rail.width() - 16.0,
            1.0,
        ),
        theme.colors.border,
    );
    paint_tile(ctx, manager, false, hover == Some(Hit::Manager));
    paint_icon(
        ctx,
        IconName::Grid,
        manager.deflate(Insets::splat(8.0)),
        theme.colors.text,
    );
    paint_tile(ctx, settings, false, hover == Some(Hit::Settings));
    paint_icon(
        ctx,
        IconName::Settings,
        settings.deflate(Insets::splat(8.0)),
        theme.colors.text,
    );

    // Menu kontekstowe rysuje warstwa nakładek (`paint_menu`), nad panelami
    // wtyczek — inaczej chowało się pod formularzem ustawień.

    // Dymek po lewej stronie szyny (nie zasłania edytora na dłużej niż hover).
    let (anchor, title, hint) = match hover {
        Some(Hit::Plugin(index)) => {
            let plugin = app.plugins.plugins.get(index);
            let hint = match plugin {
                Some(plugin) if plugin.chat => "Kliknij: panel czatu",
                Some(plugin) if !plugin.settings.is_empty() => "Kliknij: ustawienia",
                _ => "Kliknij: polecenia",
            };
            (
                plugin_rect(rail, index),
                plugin.map(|plugin| plugin.name.clone()).unwrap_or_default(),
                hint.to_owned(),
            )
        }
        Some(Hit::Manager) => (
            manager,
            "Wtyczki".to_owned(),
            "Kliknij: menedżer".to_owned(),
        ),
        Some(Hit::Settings) => (
            settings,
            "Ustawienia".to_owned(),
            "Kliknij: preferencje".to_owned(),
        ),
        None => return,
    };
    paint_tooltip(ctx, rail, anchor, &title, &hint);
}

/// Rysuje menu kontekstowe szyny **nad** panelami wtyczek i edytora.
pub fn paint_menu(app: &App, ctx: &mut PaintCtx, viewport: Size) {
    let Some(open) = &app.plugin_ui.rail_menu else {
        return;
    };
    let width = open.menu.width(ctx.text, ctx.theme);
    let panel = open
        .menu
        .place(open.anchor, width, Rect::from_size(viewport), ctx.theme);
    open.menu.paint(ctx, panel);
}

fn paint_tile(ctx: &mut PaintCtx, rect: Rect, active: bool, hovered: bool) {
    let theme = *ctx.theme;
    if active {
        ctx.scene.fill_rounded_rect(
            rect,
            theme.radius.small,
            theme.colors.accent.with_alpha(0.18),
        );
        // Wskaźnik po stronie treści (lewa krawędź kafelka).
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(
                rect.x() - 3.0,
                rect.y() + 8.0,
                3.0,
                (rect.height() - 16.0).max(1.0),
            ),
            1.5,
            theme.colors.accent,
        );
    } else if hovered {
        ctx.scene
            .fill_rounded_rect(rect, theme.radius.small, hover_fill(&theme));
    }
}

fn paint_monogram(ctx: &mut PaintCtx, rect: Rect, name: &str) {
    let theme = *ctx.theme;
    let inner = rect.deflate(Insets::splat(3.0));
    ctx.scene
        .fill_rounded_rect(inner, theme.radius.small, theme.colors.shade);
    let initial = name
        .chars()
        .find(|c| !c.is_whitespace())
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .unwrap_or('?');
    ctx.label_center(
        inner.center().x,
        inner.center().y,
        &initial.to_string(),
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
}

fn paint_tooltip(ctx: &mut PaintCtx, rail: Rect, anchor: Rect, title: &str, hint: &str) {
    let theme = *ctx.theme;
    let cap = theme.typography.caption.size;
    let body = theme.typography.body.size;
    let title_w = ctx.text.measure(title, body, weight::MEDIUM).width;
    let hint_w = ctx.text.measure(hint, cap, weight::NORMAL).width;
    let width = title_w.max(hint_w) + 20.0;
    let height = 42.0;
    let max_x = (rail.x() - 8.0 - width).max(4.0);
    let x = max_x;
    let y = (anchor.center().y - height / 2.0).clamp(
        rail.y() + 4.0,
        (rail.bottom() - height - 4.0).max(rail.y() + 4.0),
    );
    let rect = Rect::from_xywh(x, y, width, height);
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(rect.x() + 1.0, rect.y() + 2.0, rect.width(), rect.height()),
        theme.radius.small,
        Color::rgba(0.0, 0.0, 0.0, 0.18),
    );
    ctx.scene
        .fill_rounded_rect(rect, theme.radius.small, theme.colors.headerbar_bg);
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.small, 1.0, theme.colors.border);
    ctx.label_left(
        rect.x() + 10.0,
        rect.y() + 14.0,
        title,
        body,
        weight::MEDIUM,
        theme.colors.text,
    );
    ctx.label_left(
        rect.x() + 10.0,
        rect.y() + 30.0,
        hint,
        cap,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{
        Context as PluginContext, Output, Plugin, PluginCommand, PluginSetting, SettingKind,
    };
    use libluna_core::Modifiers;

    fn plugin(name: &str, settings: bool, commands: usize) -> Plugin {
        Plugin {
            dir: std::path::PathBuf::from("/tmp/rail-test"),
            id: name.to_ascii_lowercase(),
            name: name.to_owned(),
            version: "1.0.0".to_owned(),
            author: "test".to_owned(),
            description: String::new(),
            entry: "main.py".to_owned(),
            interpreter: None,
            commands: (0..commands)
                .map(|index| PluginCommand {
                    id: format!("cmd{index}"),
                    title: format!("Polecenie {index}"),
                    prompt: false,
                    context: PluginContext::None,
                    output: Output::Insert,
                })
                .collect(),
            settings: if settings {
                vec![PluginSetting {
                    key: "url".to_owned(),
                    label: "Adres".to_owned(),
                    kind: SettingKind::Text,
                    default: String::new(),
                }]
            } else {
                Vec::new()
            },
            chat: false,
        }
    }

    fn app_with(plugins: Vec<Plugin>) -> App {
        let mut app = App::new();
        app.plugins.plugins = plugins;
        app
    }

    fn rail() -> Rect {
        Rect::from_xywh(1000.0, 40.0, RAIL_W, 600.0)
    }

    fn click(position: Point, button: PointerButton) -> Event {
        Event::PointerButton(libluna_core::PointerEvent {
            position,
            button,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        })
    }

    #[test]
    fn hit_maps_tiles_and_footer() {
        let app = app_with(vec![
            plugin("Dokumentacja", true, 1),
            plugin("Skroty", false, 1),
        ]);
        let rail = rail();
        assert_eq!(
            hit(&app, rail, plugin_rect(rail, 0).center()),
            Some(Hit::Plugin(0))
        );
        assert_eq!(
            hit(&app, rail, plugin_rect(rail, 1).center()),
            Some(Hit::Plugin(1))
        );
        assert_eq!(
            hit(&app, rail, manager_rect(rail).center()),
            Some(Hit::Manager)
        );
        assert_eq!(
            hit(&app, rail, settings_rect(rail).center()),
            Some(Hit::Settings)
        );
        assert_eq!(hit(&app, rail, Point::new(10.0, 10.0)), None);
    }

    #[test]
    fn right_click_opens_the_plugin_menu() {
        let mut app = app_with(vec![plugin("Dokumentacja", true, 1)]);
        let rail = rail();
        let position = plugin_rect(rail, 0).center();
        assert!(handle_click(
            &mut app,
            &click(position, PointerButton::Right),
            rail
        ));
        let menu = app.plugin_ui.rail_menu.as_ref().expect("menu szyny");
        assert_eq!(menu.plugin, 0);
        assert_eq!(
            menu.actions.iter().flatten().copied().collect::<Vec<_>>(),
            vec![
                MenuAction::Settings,
                MenuAction::Run,
                MenuAction::Update,
                MenuAction::Manage
            ]
        );
    }

    #[test]
    fn left_click_opens_settings_when_the_plugin_has_them() {
        let mut app = app_with(vec![plugin("Dokumentacja", true, 1)]);
        let rail = rail();
        let position = plugin_rect(rail, 0).center();
        assert!(handle_click(
            &mut app,
            &click(position, PointerButton::Left),
            rail
        ));
        assert!(app.plugin_ui.form.is_some(), "formularz ustawień wtyczki");
    }

    #[test]
    fn settings_click_opens_the_editor_settings() {
        let mut app = app_with(vec![]);
        let rail = rail();
        let position = settings_rect(rail).center();
        assert!(handle_click(
            &mut app,
            &click(position, PointerButton::Left),
            rail
        ));
        assert!(app.settings_open);
    }
}
