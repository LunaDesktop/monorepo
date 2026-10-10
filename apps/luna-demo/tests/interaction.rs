//! Testy interakcji demo — bez okna i bez Waylanda. Przepuszczają prawdziwe zdarzenia
//! przez `ui::handle_events` i sprawdzają zmiany stanu.

use libluna_core::{
    Accent, ButtonState, Event, KeyEvent, Modifiers, Point, PointerButton, PointerEvent,
    ScrollEvent, Size, ThemePreference, ThemeSettings,
};
use libluna_render::TextEngine;
use libluna_widgets::{RadioGroup, ViewSwitcher};
use luna_demo::ui::{self, make_nav, Demo, Popover};

fn size() -> Size {
    Size::new(900.0, 1028.0)
}

fn press(p: Point) -> Event {
    Event::PointerButton(PointerEvent {
        position: p,
        button: PointerButton::Left,
        state: ButtonState::Pressed,
        modifiers: Modifiers::NONE,
    })
}

/// Wysyła jedno kliknięcie do logiki demo.
fn click(settings: &mut ThemeSettings, demo: &mut Demo, p: Point) {
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    ui::handle_events(settings, &[press(p)], size(), &theme, &text, demo);
}

fn layout(settings: &ThemeSettings, demo: &Demo) -> ui::Layout {
    ui::layout(size(), &settings.resolve(false), demo)
}

#[test]
fn clicking_dark_segment_sets_dark_theme() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let seg = ViewSwitcher::new(["a", "b", "c"]).segments(layout(&settings, &demo).theme_switcher)
        [1]
    .center();
    click(&mut settings, &mut demo, seg);
    assert_eq!(settings.preference(), ThemePreference::Dark);
}

#[test]
fn clicking_accent_dot_sets_accent() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let dot = layout(&settings, &demo).accents[7].center(); // Purple
    click(&mut settings, &mut demo, dot);
    assert_eq!(settings.accent(), Accent::Purple);
}

#[test]
fn clicking_nav_item_selects_it() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let theme = settings.resolve(false);
    let item = make_nav().rows(layout(&settings, &demo).sidebar, &theme)[5]
        .rect
        .center(); // Sieć
    click(&mut settings, &mut demo, item);
    assert_eq!(demo.nav_selected, Some(5));
}

#[test]
fn clicking_view_switcher_changes_mode() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let seg =
        ViewSwitcher::new(["a", "b"]).segments(layout(&settings, &demo).view_switcher)[1].center();
    click(&mut settings, &mut demo, seg);
    assert_eq!(demo.view_mode, 1);
}

#[test]
fn clicking_tab_selects_page() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let seg = ui::tab_bar(&demo).segments(layout(&settings, &demo).tabs, &text, &theme)[2].center();
    click(&mut settings, &mut demo, seg);
    assert_eq!(demo.tabs.selected(), 2);
}

#[test]
fn about_menu_opens_bottom_sheet_and_scrim_closes_it() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Otwórz menu „☰”.
    let burger = layout(&settings, &demo).burger.center();
    click(&mut settings, &mut demo, burger);
    assert_eq!(demo.popover, Some(Popover::Menu));

    // Klik „O aplikacji” (indeks 4) otwiera dolny panel.
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let about = ui::popover_menu(size(), &theme, &text, &layout(&settings, &demo), &demo)
        .map(|(panel, menu)| menu.rows(panel, &theme)[4].rect.center())
        .unwrap();
    click(&mut settings, &mut demo, about);
    assert!(demo.sheet.is_open());

    // Klik w tło (poza panelem) zamyka panel.
    click(&mut settings, &mut demo, Point::new(120.0, 300.0));
    assert!(!demo.sheet.is_open());
}

#[test]
fn search_entry_focus_and_typing() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Klik w pole nadaje fokus.
    let search = layout(&settings, &demo).search;
    click(&mut settings, &mut demo, search.center());
    assert!(demo.search.is_focused());

    // Wpisanie tekstu (platforma dostarcza `text`).
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let key = Event::Key(KeyEvent {
        keysym: 0x61,
        text: Some("a".to_owned()),
        state: ButtonState::Pressed,
        modifiers: Modifiers::NONE,
        repeat: false,
    });
    ui::handle_events(&mut settings, &[key], size(), &theme, &text, &mut demo);
    assert_eq!(demo.search.text(), "a");
}

#[test]
fn clicking_path_component_navigates_up() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    assert_eq!(demo.path.len(), 3);

    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let segs = ui::path_bar(&demo).segments(layout(&settings, &demo).path, &text, &theme);
    // Klik w element bieżący nawigację „w górę” — pierwszy widoczny składnik (np. Domowy).
    let first = segs
        .iter()
        .find(|(pick, _)| *pick == Some(0))
        .map(|(_, r)| r.center())
        .expect("pierwszy składnik ścieżki widoczny");
    click(&mut settings, &mut demo, first);
    assert_eq!(demo.path.len(), 1);
}

#[test]
fn clicking_file_item_selects_it() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let theme = settings.resolve(false);
    let slot = ui::item_view(&demo)
        .slot_rect(layout(&settings, &demo).items, 1, &theme)
        .center();
    click(&mut settings, &mut demo, slot);
    assert_eq!(demo.items.selected(), Some(1));
}

#[test]
fn scrolling_files_moves_offset() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let items = layout(&settings, &demo).items;
    let scroll = Event::Scroll(ScrollEvent {
        position: items.center(),
        dx: 0.0,
        dy: 3.0,
    });
    ui::handle_events(&mut settings, &[scroll], size(), &theme, &text, &mut demo);
    assert!(demo.items.scroll() > 0.0);
}

#[test]
fn options_popover_selects_sort() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Otwórz popover „Opcje”.
    let btn = layout(&settings, &demo).options_button.center();
    click(&mut settings, &mut demo, btn);
    assert_eq!(demo.popover, Some(Popover::Options));

    // Klik w 3. opcję sortowania (Rozmiar).
    let theme = settings.resolve(false);
    let ol = ui::options_layout(size(), &theme, &layout(&settings, &demo));
    let row = RadioGroup::new(["a", "b", "c"])
        .row_rect(ol.radios, 2, &theme)
        .center();
    click(&mut settings, &mut demo, row);
    assert_eq!(demo.sort, 2);
    // Popover zostaje otwarty po kliknięciu wewnątrz.
    assert_eq!(demo.popover, Some(Popover::Options));
}

#[test]
fn clicking_sidebar_icon_toggles_and_back() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let icon = layout(&settings, &demo).sidebar_button.center();

    click(&mut settings, &mut demo, icon);
    assert!(!demo.sidebar_visible);

    // Klik w to samo miejsce (layout liczy się na nowo) rozwija z powrotem.
    click(&mut settings, &mut demo, icon);
    assert!(demo.sidebar_visible);
}

#[test]
fn dragging_sidebar_edge_resizes_within_bounds() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    let edge = layout(&settings, &demo).sidebar_handle.center();
    click(&mut settings, &mut demo, edge);
    assert!(demo.dragging_sidebar);

    // Ruch wskaźnika mocno w prawo — szerokość przycięta do maksimum.
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    ui::handle_events(
        &mut settings,
        &[Event::PointerMotion {
            position: Point::new(5000.0, 300.0),
            modifiers: Modifiers::NONE,
        }],
        size(),
        &theme,
        &text,
        &mut demo,
    );
    assert_eq!(demo.sidebar_width, 460.0);
}

#[test]
fn burger_opens_menu_and_clicking_action_closes_it() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Klik „☰” otwiera popover.
    let burger = layout(&settings, &demo).burger.center();
    click(&mut settings, &mut demo, burger);
    assert_eq!(demo.popover, Some(Popover::Menu));

    // Klik w pierwszą pozycję menu zamyka je.
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let (panel, menu) =
        ui::popover_menu(size(), &theme, &text, &layout(&settings, &demo), &demo).unwrap();
    let first = menu.rows(panel, &theme)[0].rect.center();
    click(&mut settings, &mut demo, first);
    assert!(demo.popover.is_none());
}

#[test]
fn clicking_outside_closes_menu() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let burger = layout(&settings, &demo).burger.center();
    click(&mut settings, &mut demo, burger);
    assert_eq!(demo.popover, Some(Popover::Menu));
    // Klik daleko od panelu (środek treści) zamyka menu.
    click(&mut settings, &mut demo, Point::new(400.0, 500.0));
    assert!(demo.popover.is_none());
}

#[test]
fn window_controls_are_widget_managed() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    let header = layout(&settings, &demo).header;

    let close = header.close.expect("kontrolki po prawej").center();
    click(&mut settings, &mut demo, close);
    assert!(demo.pending_close);

    let minimize = header.minimize.unwrap().center();
    click(&mut settings, &mut demo, minimize);
    assert!(demo.pending_minimize);
}

#[test]
fn language_dropdown_opens_and_selects() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Otwarcie listy języków.
    let dd = layout(&settings, &demo).language.center();
    click(&mut settings, &mut demo, dd);
    assert_eq!(demo.popover, Some(Popover::Language));

    // Wybór ostatniej pozycji ustawia język i zamyka popover.
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let (panel, menu) =
        ui::popover_menu(size(), &theme, &text, &layout(&settings, &demo), &demo).unwrap();
    let last = menu.rows(panel, &theme).last().unwrap().rect.center();
    click(&mut settings, &mut demo, last);
    assert!(demo.popover.is_none());
}

#[test]
fn accent_change_shows_toast_and_undo_reverts() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();

    // Zmiana akcentu pokazuje powiadomienie.
    let dot = layout(&settings, &demo).accents[2].center();
    click(&mut settings, &mut demo, dot);
    assert_eq!(settings.accent(), Accent::ALL[2]);
    assert!(demo.toast.is_open());

    // Klik „Cofnij” przywraca poprzedni akcent i chowa powiadomienie.
    let theme = settings.resolve(false);
    let text = TextEngine::system();
    let undo = demo
        .toast
        .action_rect(size(), &theme, &text)
        .expect("akcja powiadomienia")
        .center();
    click(&mut settings, &mut demo, undo);
    assert_eq!(settings.accent(), Accent::default());
    assert!(!demo.toast.is_open());
}

#[test]
fn search_without_results_shows_empty_state_that_clears() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    demo.search.set_text("zzz");

    let theme = settings.resolve(false);
    let action = ui::empty_state(&demo)
        .expect("ekran stanu dla braku wyników")
        .rects(layout(&settings, &demo).items, &theme)
        .action
        .expect("przycisk akcji")
        .center();
    click(&mut settings, &mut demo, action);
    assert!(demo.search.is_empty());
    assert!(ui::empty_state(&demo).is_none());
}

#[test]
fn search_filters_items_and_selection_maps_to_full_list() {
    let mut settings = ThemeSettings::default();
    let mut demo = Demo::default();
    demo.search.set_text("proj");

    let visible = ui::visible_indices(&demo);
    assert_eq!(
        visible.len(),
        1,
        "filtr powinien zawęzić listę do jednego elementu"
    );
    let view = ui::item_view(&demo);
    assert_eq!(view.len(), 1);

    // Klik widocznego elementu zaznacza go w pełnej liście.
    let theme = settings.resolve(false);
    let slot = view
        .slot_rect(layout(&settings, &demo).items, 0, &theme)
        .center();
    click(&mut settings, &mut demo, slot);
    assert_eq!(demo.items.selected(), Some(visible[0]));
}
