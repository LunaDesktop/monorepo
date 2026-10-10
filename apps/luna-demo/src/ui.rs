//! Scena demo Ustawień środowiska Luna — budowana z widgetów `libluna-widgets`
//! (`ViewSwitcher`, `NavList`, `Menu`, `Button`, `Switch`, `Slider`, `DropDown`, `TabBar`)
//! plus obsługa zdarzeń (klik, przeciąganie, skróty). Logika nie zależy od Waylanda.

use libluna_core::event::ButtonState;
use libluna_core::style::{weight, Appearance};
use libluna_core::{
    Accent, Color, Event, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
    ThemePreference, ThemeSettings,
};
use libluna_i18n::{tr, Locale};
use libluna_render::TextEngine;
use libluna_widgets::{
    ActionRow, BottomSheet, Button, Card, CheckButton, ControlsLayout, ControlsSide, DropDown,
    HeaderBar, Item, ItemView, Menu, MenuEntry, NavEntry, NavList, Pager, PaintCtx, PathBar,
    RadioGroup, ScrollView, SearchEntry, Slider, Stepper, StepperSide, Switch, Tab, TabBar,
    TabView, ViewMode, ViewSwitcher, WindowControl,
};

const STATUS_H: f64 = 28.0;
const PAD: f64 = 12.0;
/// Wysokość wiersza ustawień w demo.
const ROW_H: f64 = 56.0;
/// Indeks pozycji „O aplikacji” w menu „☰” (otwiera dolny panel).
const ABOUT_ENTRY: usize = 4;

/// Domyślna szerokość paska bocznego.
pub const DEFAULT_SIDEBAR_W: f64 = 220.0;
/// Dolny/górny limit szerokości paska bocznego przy przeciąganiu.
const MIN_SIDEBAR_W: f64 = 150.0;
const MAX_SIDEBAR_W: f64 = 460.0;
/// Szerokość strefy chwytu do przeciągania krawędzi paska bocznego.
const HANDLE_W: f64 = 6.0;

/// Który popover jest otwarty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popover {
    /// Menu „☰” pod ikoną w HeaderBarze.
    Menu,
    /// Lista wyboru języka pod kontrolką języka.
    Language,
    /// Popover opcji widoku pod przyciskiem „Opcje ▾”.
    Options,
}

/// Stan aplikacji demo, który nie należy do warstwy platformy ani do widgetów.
pub struct Demo {
    pub option_a: bool,
    pub volume: f64,
    pub volume_dragging: bool,
    pub show_bottom_bar: bool,
    pub sidebar_visible: bool,
    pub sidebar_width: f64,
    pub dragging_sidebar: bool,
    /// Wybrany widok (0 = Lista, 1 = Siatka).
    pub view_mode: usize,
    /// Zaznaczona pozycja menu nawigacji (indeks elementu) oraz podświetlenia.
    pub nav_selected: Option<usize>,
    pub nav_hover: Option<usize>,
    pub theme_hover: Option<usize>,
    pub view_hover: Option<usize>,
    pub options_hover: bool,
    pub switch_hover: bool,
    /// Podświetlona kontrolka okna i akcje do wykonania przez platformę.
    pub controls_hover: Option<WindowControl>,
    pub pending_close: bool,
    pub pending_minimize: bool,
    pub pending_maximize: bool,
    /// Otwarty popover i podświetlona pozycja.
    pub popover: Option<Popover>,
    pub popover_hover: Option<usize>,
    /// Strony zakładek (tytuły to klucze i18n) oraz podświetlona zakładka.
    pub tabs: TabView,
    pub tabs_hover: Option<usize>,
    /// Dolny panel (modalny) i karuzela stron w jego zawartości.
    pub sheet: BottomSheet,
    pub pager: Pager,
    pub pager_dragging: bool,
    /// Pasek ścieżki (składniki to klucze i18n) i pole wyszukiwania.
    pub path: PathBar,
    pub path_hover: Option<usize>,
    pub search: SearchEntry,
    /// Widok elementów (pliki/katalogi) i podświetlony element.
    pub items: ItemView,
    pub items_hover: Option<usize>,
    /// Przewijanie listy plików i stan przeciągania paska.
    pub files_scroll: ScrollView,
    pub scroll_dragging: bool,
    /// Stan popovera „Opcje” (rozmiar ikon, sortowanie, ukryte pliki) i podświetlenia.
    pub icon_size: i64,
    pub sort: usize,
    pub show_hidden: bool,
    pub options_stepper_hover: Option<StepperSide>,
    pub options_radio_hover: Option<usize>,
    pub options_check_hover: bool,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            option_a: true,
            volume: 0.65,
            volume_dragging: false,
            show_bottom_bar: true,
            sidebar_visible: true,
            sidebar_width: DEFAULT_SIDEBAR_W,
            dragging_sidebar: false,
            view_mode: 0,
            // Domyślnie zaznaczony „Wygląd" (indeks elementu w liście nawigacji).
            nav_selected: Some(2),
            nav_hover: None,
            theme_hover: None,
            view_hover: None,
            options_hover: false,
            switch_hover: false,
            controls_hover: None,
            pending_close: false,
            pending_minimize: false,
            pending_maximize: false,
            popover: None,
            popover_hover: None,
            tabs: {
                let mut tabs = TabView::new();
                tabs.add_titled("General");
                tabs.add_titled("Advanced");
                tabs.add_titled("About");
                tabs
            },
            tabs_hover: None,
            sheet: BottomSheet::new(),
            pager: Pager::new(3),
            pager_dragging: false,
            path: PathBar::new(["Home", "Documents", "Projects"]),
            path_hover: None,
            search: SearchEntry::new("Search"),
            items: ItemView::new(vec![
                Item::folder("Projects"),
                Item::new("Report").detail("PDF"),
                Item::new("Notes").detail("TXT"),
                Item::new("Budget").detail("XLSX"),
                Item::new("IMG_0001.JPG").detail("JPG"),
                Item::new("IMG_0002.JPG").detail("JPG"),
                Item::new("screenshot.png").detail("PNG"),
                Item::new("TODO.txt").detail("TXT"),
                Item::new("changelog.md").detail("MD"),
                Item::new("backup.tar").detail("TAR"),
            ]),
            items_hover: None,
            files_scroll: ScrollView::new(),
            scroll_dragging: false,
            icon_size: 32,
            sort: 0,
            show_hidden: false,
            options_stepper_hover: None,
            options_radio_hover: None,
            options_check_hover: false,
        }
    }
}

/// Prostokąty interaktywnych elementów — wspólne dla rysowania i hit-testingu.
pub struct Layout {
    pub sidebar: Rect,
    pub sidebar_handle: Rect,
    pub area: Rect,
    pub sidebar_button: Rect,
    pub sidebar_visible: bool,
    pub options_button: Rect,
    /// Rozkład paska tytułu (kontrolki okna + obszar treści).
    pub header: ControlsLayout,
    pub theme_switcher: Rect,
    pub accents: [Rect; 9],
    pub language: Rect,
    pub view_switcher: Rect,
    pub option_a: Rect,
    pub volume_track: Rect,
    pub bottom_bar: Rect,
    pub sidebar_toggle: Rect,
    /// Pasek zakładek (karta „Zakładki”) i wiersz z aktywną stroną.
    pub tabs: Rect,
    pub tabs_value: Rect,
    /// Pasek ścieżki i pole wyszukiwania (karta „Pliki”).
    pub path: Rect,
    pub search: Rect,
    /// Obszar elementów (siatka/lista) w karcie „Pliki”.
    pub items: Rect,
    /// Kotwica popoveru „☰” w HeaderBarze.
    pub burger: Rect,
}

fn segment_labels() -> [&'static str; 3] {
    ["Light", "Dark", "Auto"]
}

/// Pasek tytułu z tytułem i kontrolkami okna po prawej.
fn header_bar() -> HeaderBar {
    HeaderBar::new()
        .title(tr("Application"))
        .controls(ControlsSide::Right)
}

/// Lista nawigacji z aktualnie przetłumaczonymi etykietami (kolejność stała).
pub fn make_nav() -> NavList {
    NavList::new(vec![
        NavEntry::Item(tr("Overview")),
        NavEntry::Item(tr("Account")),
        NavEntry::Item(tr("Appearance")),
        NavEntry::Item(tr("Notifications")),
        NavEntry::Section(tr("Advanced")),
        NavEntry::Item(tr("Network")),
        NavEntry::Item(tr("System")),
    ])
}

/// Pozycje menu „☰" (aktualnie przetłumaczone).
pub fn menu_entries() -> Menu {
    Menu::new(vec![
        MenuEntry::action(tr("New Window")).shortcut("Ctrl+N"),
        MenuEntry::action(tr("Keyboard Shortcuts")),
        MenuEntry::separator(),
        MenuEntry::action(tr("Settings")),
        MenuEntry::action(tr("About")),
    ])
}

/// Pasek zakładek demo z przetłumaczonymi etykietami (tytuły w `TabView` to klucze).
pub fn tab_bar(demo: &Demo) -> TabBar {
    let tabs: Vec<Tab> = demo
        .tabs
        .tabs()
        .iter()
        .map(|t| Tab::new(tr(&t.title)).closable(t.closable))
        .collect();
    let mut bar = TabBar::with_tabs(tabs);
    bar.set_selected(demo.tabs.selected());
    bar.set_hovered(demo.tabs_hover);
    bar
}

/// Pasek ścieżki demo z przetłumaczonymi etykietami (składniki to klucze i18n).
pub fn path_bar(demo: &Demo) -> PathBar {
    let mut bar = PathBar::new(demo.path.components().iter().map(|k| tr(k)));
    bar.set_hovered(demo.path_hover);
    bar
}

/// Widok elementów demo z przetłumaczonymi etykietami (etykiety to klucze i18n).
pub fn item_view(demo: &Demo) -> ItemView {
    let items: Vec<Item> = demo
        .items
        .items()
        .iter()
        .map(|it| {
            let mut item = Item::new(tr(&it.label)).kind(it.kind);
            if let Some(detail) = &it.detail {
                item = item.detail(detail.clone());
            }
            item
        })
        .collect();
    let mut view = ItemView::new(items);
    view.set_mode(demo.items.mode());
    view.set_selected(demo.items.selected());
    view.set_hovered(demo.items_hover);
    view.set_scroll(demo.items.scroll());
    view
}

/// Kody języków i kontrolka wyboru (nazwy natywne).
pub fn language_dropdown(codes: &[String]) -> DropDown {
    let names: Vec<String> = codes
        .iter()
        .map(|c| libluna_i18n::language_name(c).unwrap_or_else(|| c.to_uppercase()))
        .collect();
    let mut dd = DropDown::new(names);
    let current = libluna_i18n::locale().language().to_owned();
    if let Some(i) = codes.iter().position(|c| *c == current) {
        dd.set_selected(i);
    }
    dd
}

fn theme_switcher(preference: ThemePreference, hovered: Option<usize>) -> ViewSwitcher {
    let mut vs = ViewSwitcher::new(segment_labels().map(tr));
    let index = ThemePreference::ALL
        .iter()
        .position(|p| *p == preference)
        .unwrap_or(0);
    vs.set_selected(index);
    vs.set_hovered(hovered);
    vs
}

fn view_switcher(view_mode: usize, hovered: Option<usize>) -> ViewSwitcher {
    let mut vs = ViewSwitcher::new([tr("List"), tr("Grid")]);
    vs.set_selected(view_mode);
    vs.set_hovered(hovered);
    vs
}

/// Panel otwartego popoveru wraz z jego widgetem.
pub fn popover_menu(
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    lay: &Layout,
    demo: &Demo,
) -> Option<(Rect, Menu)> {
    let viewport = Rect::from_xywh(0.0, 0.0, size.width, size.height);
    match demo.popover? {
        Popover::Menu => {
            let menu = menu_entries();
            let w = menu.width(text, theme);
            Some((menu.place(lay.burger, w, viewport, theme), menu))
        }
        Popover::Language => {
            let codes = libluna_i18n::languages();
            let menu = language_dropdown(&codes).menu();
            let w = menu.width(text, theme);
            Some((menu.place(lay.language, w, viewport, theme), menu))
        }
        // Popover „Opcje” rysowany osobno (własny układ).
        Popover::Options => None,
    }
}

// --- Układ ---

pub fn layout(size: Size, theme: &Theme, demo: &Demo) -> Layout {
    let lg = theme.spacing.lg;
    let md = theme.spacing.md;
    let header_h = theme.metrics.headerbar.height;
    let control = theme.metrics.headerbar.control;
    let switch_m = theme.metrics.switch;

    let status_h = if demo.show_bottom_bar { STATUS_H } else { 0.0 };
    let content_h = (size.height - header_h - status_h).max(0.0);

    let sidebar_w = if demo.sidebar_visible {
        demo.sidebar_width
    } else {
        0.0
    };
    let sidebar = Rect::from_xywh(0.0, header_h, sidebar_w, content_h);
    let sidebar_handle = if demo.sidebar_visible {
        Rect::from_xywh(sidebar_w - HANDLE_W / 2.0, header_h, HANDLE_W, content_h)
    } else {
        Rect::ZERO
    };
    let area = Rect::from_xywh(
        sidebar_w,
        header_h,
        (size.width - sidebar_w).max(0.0),
        content_h,
    );

    // Pasek tytułu jako widget: kontrolki okna + obszar na treść.
    let header_rect = Rect::from_xywh(0.0, 0.0, size.width, header_h);
    let header = header_bar().controls_layout(header_rect, theme);
    let hb_cy = header_rect.center().y;
    let content = header.content;
    let sidebar_button = Rect::from_xywh(content.x(), hb_cy - control / 2.0, control, control);
    let plus = Rect::from_xywh(
        sidebar_button.right() + PAD,
        hb_cy - control / 2.0,
        control,
        control,
    );
    let options_button = Rect::from_xywh(plus.right() + PAD, hb_cy - control / 2.0, 110.0, control);
    let burger = Rect::from_xywh(
        content.right() - control,
        hb_cy - control / 2.0,
        control,
        control,
    );

    // Karta 1: Wygląd (3 wiersze).
    let card = Rect::from_xywh(
        area.x() + lg,
        area.y() + lg,
        (area.width() - 2.0 * lg).max(0.0),
        208.0,
    );
    let header_h = theme.metrics.card.header;
    let row_h = ROW_H;
    let row = |i: usize| {
        Rect::from_xywh(
            card.x(),
            card.y() + header_h + i as f64 * row_h,
            card.width(),
            row_h,
        )
    };

    // Wiersz 0: tryb (jasny/ciemny/auto).
    let seg_total = 240.0;
    let r0 = row(0);
    let theme_switcher = Rect::from_xywh(
        r0.right() - md - seg_total,
        r0.center().y - 16.0,
        seg_total,
        32.0,
    );

    // Wiersz 1: akcent.
    let dot = 22.0;
    let gap = 10.0;
    let dots_w = Accent::ALL.len() as f64 * dot + (Accent::ALL.len() - 1) as f64 * gap;
    let r1 = row(1);
    let dots_x = r1.right() - md - dots_w;
    let accents = std::array::from_fn(|i| {
        Rect::from_xywh(
            dots_x + i as f64 * (dot + gap),
            r1.center().y - dot / 2.0,
            dot,
            dot,
        )
    });

    // Wiersz 2: język.
    let r2 = row(2);
    let dd_h = theme.metrics.dropdown.height;
    let language = Rect::from_xywh(
        r2.right() - md - 170.0,
        r2.center().y - dd_h / 2.0,
        170.0,
        dd_h,
    );

    // Karta 2: Ustawienia (5 wierszy).
    let card2 = Rect::from_xywh(area.x() + lg, card.bottom() + lg, card.width(), 320.0);
    let row2 = |i: usize| {
        Rect::from_xywh(
            card2.x(),
            card2.y() + header_h + i as f64 * row_h,
            card2.width(),
            row_h,
        )
    };
    let switch = |r: Rect| {
        Rect::from_xywh(
            r.right() - md - switch_m.width,
            r.center().y - switch_m.height / 2.0,
            switch_m.width,
            switch_m.height,
        )
    };
    let option_a = switch(row2(0));
    let srow = row2(1);
    let track_h = theme.metrics.slider.track_height;
    let volume_track = Rect::from_xywh(
        srow.right() - md - 200.0,
        srow.center().y - track_h / 2.0,
        200.0,
        track_h,
    );
    let vrow = row2(2);
    let view_switcher = Rect::from_xywh(
        vrow.right() - md - 160.0,
        vrow.center().y - 16.0,
        160.0,
        32.0,
    );
    let bottom_bar = switch(row2(3));
    let sidebar_toggle = switch(row2(4));

    // Karta 3: Zakładki (pasek + wiersz z aktywną stroną).
    let card3 = Rect::from_xywh(area.x() + lg, card2.bottom() + lg, card.width(), 152.0);
    let row3 = |i: usize| {
        Rect::from_xywh(
            card3.x(),
            card3.y() + header_h + i as f64 * row_h,
            card3.width(),
            row_h,
        )
    };
    let tabs_h = theme.metrics.tabs.height;
    let tabs_w = 420.0;
    let trow = row3(0);
    let tabs = Rect::from_xywh(
        trow.right() - md - tabs_w,
        trow.center().y - tabs_h / 2.0,
        tabs_w,
        tabs_h,
    );
    let tabs_value = row3(1);

    // Karta 4: Pliki (PathBar + SearchEntry + ItemView).
    let card4 = Rect::from_xywh(area.x() + lg, card3.bottom() + lg, card.width(), 196.0);
    let row4 = Rect::from_xywh(card4.x(), card4.y() + header_h, card4.width(), row_h);
    let path_h = theme.metrics.pathbar.height;
    let path = Rect::from_xywh(
        card4.x() + md,
        row4.center().y - path_h / 2.0,
        360.0,
        path_h,
    );
    let entry_h = theme.metrics.entry.height;
    let search = Rect::from_xywh(
        row4.right() - md - 240.0,
        row4.center().y - entry_h / 2.0,
        240.0,
        entry_h,
    );
    let items = Rect::from_xywh(
        card4.x() + md,
        card4.y() + header_h + row_h,
        (card4.width() - 2.0 * md).max(0.0),
        (card4.height() - header_h - row_h - md).max(0.0),
    );

    Layout {
        sidebar,
        sidebar_handle,
        area,
        sidebar_button,
        sidebar_visible: demo.sidebar_visible,
        options_button,
        header,
        theme_switcher,
        accents,
        language,
        view_switcher,
        option_a,
        volume_track,
        bottom_bar,
        sidebar_toggle,
        tabs,
        tabs_value,
        path,
        search,
        items,
        burger,
    }
}

// --- Obsługa zdarzeń ---

/// Reaguje na zdarzenia: kliknięcia, przeciąganie (pasek boczny, suwak) i skróty.
///
/// Nie zależy od Waylanda — bierze zdarzenia i ustawienia, więc da się ją wywołać
/// także w testach i w headlessowym „driverze” (bez okna).
pub fn handle_events(
    settings: &mut ThemeSettings,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    demo: &mut Demo,
) {
    let lay = layout(size, theme, demo);
    let theme_vs = ViewSwitcher::new(segment_labels().map(tr));
    let view_vs = ViewSwitcher::new([tr("List"), tr("Grid")]);
    let tabs_bar = tab_bar(demo);
    let path = path_bar(demo);
    let items = item_view(demo);
    let nav = make_nav();
    let slider = Slider::new(demo.volume);
    let popover = popover_menu(size, theme, text, &lay, demo);
    let header_rect = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let header = header_bar();

    demo.files_scroll
        .set_content_height(demo.items.content_height(lay.items, theme));

    for event in events {
        match event {
            Event::PointerButton(pe) if pe.button == PointerButton::Left => {
                let p = pe.position;
                match pe.state {
                    ButtonState::Pressed => {
                        // Dolny panel (modalny) ma pierwszeństwo nad resztą UI.
                        if demo.sheet.is_open() {
                            let area = demo.sheet.content_rect(size, theme);
                            if demo.sheet.close_at(size, theme, p) {
                                demo.sheet.set_open(false);
                            } else if demo.sheet.drag_zone_at(size, theme, p) {
                                demo.sheet.begin_drag(p.y);
                            } else if demo.sheet.scrim_at(size, theme, p) {
                                demo.sheet.set_open(false);
                            } else if area.contains(p)
                                && !demo.pager.click_indicator(area, theme, p)
                            {
                                demo.pager_dragging = true;
                                demo.pager.begin_drag(p.x);
                            }
                            continue;
                        }

                        // Popover „Opcje” obsługuje własne kontrolki.
                        if demo.popover == Some(Popover::Options) {
                            let ol = options_layout(size, theme, &lay);
                            let mut stepper =
                                Stepper::new(demo.icon_size, ICON_MIN, ICON_MAX).step(8);
                            let mut radios = sort_options();
                            radios.set_selected(demo.sort);
                            let mut check = CheckButton::new(tr("Show hidden files"));
                            check.set_checked(demo.show_hidden);
                            if stepper.click_at(ol.stepper, theme, p) {
                                demo.icon_size = stepper.value();
                            } else if radios.click_at(ol.radios, p, theme) {
                                demo.sort = radios.selected();
                            } else if check.click_at(ol.check, p) {
                                demo.show_hidden = check.checked();
                            } else if !ol.panel.contains(p) {
                                demo.popover = None;
                            }
                            continue;
                        }

                        // Gdy popover otwarty, klik wybiera pozycję albo zamyka.
                        if demo.popover.is_some() {
                            if let (Some((panel, menu)), Some(kind)) = (&popover, demo.popover) {
                                if let Some(entry) = menu.index_at(*panel, p, theme) {
                                    if kind == Popover::Language {
                                        let codes = libluna_i18n::languages();
                                        if let Some(code) = codes.get(entry) {
                                            libluna_i18n::set_locale(Locale::new(code));
                                        }
                                    } else if entry == ABOUT_ENTRY {
                                        // „O aplikacji” otwiera dolny panel.
                                        demo.sheet.set_open(true);
                                    }
                                }
                            }
                            demo.popover = None;
                            demo.popover_hover = None;
                            continue;
                        }

                        // Klik poza polem wyszukiwania odbiera mu fokus.
                        if !demo.search.contains(lay.search, p) {
                            demo.search.set_focused(false);
                        }

                        if let Some(control) = header.control_at(header_rect, theme, p) {
                            match control {
                                WindowControl::Minimize => demo.pending_minimize = true,
                                WindowControl::Maximize => demo.pending_maximize = true,
                                WindowControl::Close => demo.pending_close = true,
                            }
                        } else if lay.burger.contains(p) {
                            demo.popover = Some(Popover::Menu);
                        } else if lay.options_button.contains(p) {
                            demo.popover = Some(Popover::Options);
                        } else if lay.language.contains(p) {
                            demo.popover = Some(Popover::Language);
                        } else if lay.sidebar_handle.contains(p) {
                            demo.dragging_sidebar = true;
                        } else if let Some(i) = theme_vs.index_at(lay.theme_switcher, p) {
                            settings.set_preference(ThemePreference::ALL[i]);
                        } else if let Some(i) = view_vs.index_at(lay.view_switcher, p) {
                            demo.view_mode = i;
                            demo.items.set_mode(if i == 1 {
                                ViewMode::Grid
                            } else {
                                ViewMode::List
                            });
                            demo.files_scroll
                                .set_content_height(demo.items.content_height(lay.items, theme));
                            demo.items.clamp_scroll(lay.items, theme);
                            demo.files_scroll.set_offset(demo.items.scroll(), lay.items);
                        } else if let Some(i) = tabs_bar.index_at(lay.tabs, text, theme, p) {
                            demo.tabs.select(i);
                        } else if demo.search.contains(lay.search, p) {
                            if demo.search.clear_at(lay.search, theme, p) {
                                demo.search.clear();
                            }
                            demo.search.set_focused(true);
                        } else if let Some(i) = path.index_at(lay.path, text, theme, p) {
                            demo.path.truncate(i + 1);
                        } else if demo.files_scroll.begin_drag(p, lay.items, theme) {
                            demo.scroll_dragging = true;
                        } else if let Some(i) = items.index_at(lay.items, p, theme) {
                            demo.items.set_selected(Some(i));
                        } else if let Some(i) = nav.index_at(lay.sidebar, p, theme) {
                            demo.nav_selected = Some(i);
                        } else if lay.option_a.contains(p) {
                            demo.option_a = !demo.option_a;
                        } else if slider.hit(lay.volume_track, p, theme) {
                            demo.volume_dragging = true;
                            demo.volume = slider.value_at(lay.volume_track, p);
                        } else if lay.bottom_bar.contains(p) {
                            demo.show_bottom_bar = !demo.show_bottom_bar;
                        } else if lay.sidebar_toggle.contains(p) || lay.sidebar_button.contains(p) {
                            demo.sidebar_visible = !demo.sidebar_visible;
                        } else {
                            for (i, r) in lay.accents.iter().enumerate() {
                                if r.contains(p) {
                                    settings.set_accent(Accent::ALL[i]);
                                }
                            }
                        }
                    }
                    ButtonState::Released => {
                        demo.dragging_sidebar = false;
                        demo.volume_dragging = false;
                        if demo.pager_dragging {
                            demo.pager.end_drag();
                            demo.pager_dragging = false;
                        }
                        if demo.scroll_dragging {
                            demo.files_scroll.end_drag();
                            demo.scroll_dragging = false;
                        }
                        if demo.sheet.is_open() {
                            demo.sheet.end_drag(theme);
                        }
                    }
                }
            }
            Event::PointerMotion { position, .. } => {
                let position = *position;
                if demo.sheet.is_open() {
                    let close = demo.sheet.close_at(size, theme, position);
                    demo.sheet.set_hover_close(close);
                    demo.sheet.update_drag(position.y);
                    if demo.pager_dragging {
                        let area = demo.sheet.content_rect(size, theme);
                        demo.pager.update_drag(position.x, area);
                    }
                    continue;
                }
                if demo.popover == Some(Popover::Options) {
                    let ol = options_layout(size, theme, &lay);
                    demo.options_stepper_hover = Stepper::new(demo.icon_size, ICON_MIN, ICON_MAX)
                        .side_at(ol.stepper, theme, position);
                    demo.options_radio_hover = sort_options().index_at(ol.radios, position, theme);
                    demo.options_check_hover = CheckButton::new("").hit(ol.check, position);
                    continue;
                }
                if demo.dragging_sidebar && demo.sidebar_visible {
                    demo.sidebar_width = position.x.clamp(MIN_SIDEBAR_W, MAX_SIDEBAR_W);
                }
                if demo.volume_dragging {
                    demo.volume = slider.value_at(lay.volume_track, position);
                }

                if demo.popover.is_some() {
                    demo.popover_hover = popover
                        .as_ref()
                        .and_then(|(panel, menu)| menu.index_at(*panel, position, theme));
                } else {
                    demo.nav_hover = nav.index_at(lay.sidebar, position, theme);
                    demo.theme_hover = theme_vs.index_at(lay.theme_switcher, position);
                    demo.view_hover = view_vs.index_at(lay.view_switcher, position);
                    demo.options_hover = lay.options_button.contains(position);
                    demo.switch_hover = lay.option_a.contains(position);
                    demo.tabs_hover = tabs_bar.index_at(lay.tabs, text, theme, position);
                    demo.path_hover = path.index_at(lay.path, text, theme, position);
                    demo.items_hover = items.index_at(lay.items, position, theme);
                    if demo.scroll_dragging {
                        demo.files_scroll.drag_to(position, lay.items, theme);
                        demo.items.set_scroll(demo.files_scroll.offset());
                    }
                    demo.search
                        .set_hover_clear(demo.search.clear_at(lay.search, theme, position));
                    demo.controls_hover = header.control_at(header_rect, theme, position);
                }
            }
            Event::PointerLeave => {
                demo.dragging_sidebar = false;
                demo.volume_dragging = false;
                demo.pager_dragging = false;
                demo.nav_hover = None;
                demo.theme_hover = None;
                demo.view_hover = None;
                demo.options_hover = false;
                demo.switch_hover = false;
                demo.tabs_hover = None;
                demo.path_hover = None;
                demo.items_hover = None;
                demo.files_scroll.end_drag();
                demo.scroll_dragging = false;
                demo.search.set_hover_clear(false);
                demo.sheet.set_hover_close(false);
                demo.controls_hover = None;
            }
            Event::Scroll(se) => {
                if !demo.sheet.is_open() && lay.items.contains(se.position) {
                    let step = theme.metrics.itemview.row_h;
                    if demo.files_scroll.on_scroll(se.dy * step, lay.items) {
                        demo.items.set_scroll(demo.files_scroll.offset());
                    }
                }
            }
            Event::Key(ke) if ke.state == ButtonState::Pressed => {
                // Gdy pole wyszukiwania ma fokus, klawisze idą do niego.
                if demo.search.is_focused() && handle_search_key(&mut demo.search, ke) {
                    continue;
                }
                match ke.keysym {
                    // `l` — przełącz jasny/ciemny
                    0x6c => {
                        let target = if theme.appearance == Appearance::Dark {
                            ThemePreference::Light
                        } else {
                            ThemePreference::Dark
                        };
                        settings.set_preference(target);
                    }
                    // `a` — następny akcent
                    0x61 => {
                        let idx = Accent::ALL
                            .iter()
                            .position(|a| *a == settings.accent())
                            .unwrap_or(0);
                        settings.set_accent(Accent::ALL[(idx + 1) % Accent::ALL.len()]);
                    }
                    // `g` — następny język
                    0x67 => cycle_language(),
                    // `b` — pokaż/ukryj pasek dolny
                    0x62 => demo.show_bottom_bar = !demo.show_bottom_bar,
                    // `s` — pokaż/ukryj pasek boczny
                    0x73 => demo.sidebar_visible = !demo.sidebar_visible,
                    // `v` — przełącz widok (Lista/Siatka)
                    0x76 => demo.view_mode = (demo.view_mode + 1) % 2,
                    // `o` — pokaż/ukryj dolny panel („O aplikacji”)
                    0x6f => demo.sheet.set_open(!demo.sheet.is_open()),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn cycle_language() {
    let languages = libluna_i18n::languages();
    if languages.is_empty() {
        return;
    }
    let current = libluna_i18n::locale().language().to_owned();
    let idx = languages.iter().position(|l| *l == current).unwrap_or(0);
    let next = languages[(idx + 1) % languages.len()].clone();
    libluna_i18n::set_locale(Locale::new(&next));
}

/// Kieruje pojedyncze zdarzenie klawiatury do pola wyszukiwania.
///
/// Zwraca `true`, jeśli klawisz został zużyty (wtedy skróty demo są pomijane).
fn handle_search_key(entry: &mut SearchEntry, ke: &KeyEvent) -> bool {
    // Tekst złożony przez platformę (obsługuje też znaki spoza ASCII).
    if let Some(t) = &ke.text {
        if !t.is_empty() && !t.chars().all(char::is_control) {
            entry.insert_str(t);
            return true;
        }
    }
    match ke.keysym {
        0xff08 => entry.backspace(),      // BackSpace
        0xffff => entry.delete_forward(), // Delete
        0xff51 => entry.move_left(),      // Left
        0xff53 => entry.move_right(),     // Right
        0xff50 => entry.home(),           // Home
        0xff57 => entry.end(),            // End
        0x20..=0x7e => {
            if let Some(c) = char::from_u32(ke.keysym) {
                entry.insert_char(c);
            }
        }
        _ => return false,
    }
    true
}

// --- Budowa sceny ---

/// Buduje scenę makiety Ustawień o zadanym rozmiarze (piksele logiczne).
pub fn build_scene(
    scene: &mut libluna_render::Scene,
    theme: &Theme,
    text: &TextEngine,
    size: Size,
    demo: &Demo,
    preference: ThemePreference,
    accent: Accent,
) {
    let lay = layout(size, theme, demo);
    let mut paint = PaintCtx { scene, theme, text };

    let (w, h) = (size.width, size.height);
    let bg = theme.colors.window_bg;
    paint.scene.fill_rect(Rect::from_xywh(0.0, 0.0, w, h), bg);

    draw_headerbar(
        &mut paint,
        Rect::from_xywh(0.0, 0.0, w, theme.metrics.headerbar.height),
        &lay,
        demo,
    );
    if lay.sidebar.width() > 0.0 {
        draw_sidebar(&mut paint, &lay, demo);
    }
    draw_content(&mut paint, lay.area, &lay, demo, preference, accent);
    if demo.show_bottom_bar {
        draw_statusbar(&mut paint, Rect::from_xywh(0.0, h - STATUS_H, w, STATUS_H));
    }

    // Popover („☰" lub wybór języka) rysowany na wierzchu.
    if let Some((panel, mut menu)) = popover_menu(size, theme, text, &lay, demo) {
        menu.set_hovered(demo.popover_hover);
        menu.paint(&mut paint, panel);
    }

    // Popover „Opcje” (rozmiar ikon / sortowanie / ukryte pliki).
    if demo.popover == Some(Popover::Options) {
        draw_options(&mut paint, size, &lay, demo);
    }

    // Dolny panel (modalny) rysowany na samym wierzchu.
    if demo.sheet.is_open() {
        let mut sheet = demo.sheet.clone();
        sheet.set_title(Some(tr("About")));
        sheet.paint(&mut paint, size);
        draw_sheet_content(&mut paint, demo, size);
    }
}

/// Zawartość dolnego panelu: karuzela stron (`Pager`) z kropkami-wskaźnikami.
fn draw_sheet_content(paint: &mut PaintCtx, demo: &Demo, size: Size) {
    let theme = *paint.theme;
    let content = demo.sheet.content_rect(size, &theme);
    let pager = &demo.pager;

    // Obszar stron (nad kropkami).
    let dots_h = theme.metrics.pager.dot + theme.metrics.pager.pad;
    let pages = Rect::from_xywh(
        content.x(),
        content.y(),
        content.width(),
        (content.height() - dots_h).max(0.0),
    );

    paint.scene.push_clip(pages);
    for i in 0..pager.count() {
        draw_about_page(paint, pager.page_rect(pages, i), i);
    }
    paint.scene.pop_clip();

    pager.paint(paint, content);
}

/// Pojedyncza strona karuzeli w panelu „O aplikacji”.
fn draw_about_page(paint: &mut PaintCtx, rect: Rect, index: usize) {
    let theme = *paint.theme;
    let (heading, body) = match index {
        0 => (tr("About"), "libLuna 0.0.1".to_owned()),
        1 => (tr("Keyboard Shortcuts"), "l · a · g · b · s · v".to_owned()),
        _ => (tr("License"), "MPL-2.0".to_owned()),
    };
    let cx = rect.center().x;
    let cy = rect.center().y;
    paint.label_center(
        cx,
        cy - 14.0,
        &heading,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    paint.label_center(
        cx,
        cy + 14.0,
        &body,
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn draw_headerbar(paint: &mut PaintCtx, bar: Rect, lay: &Layout, demo: &Demo) {
    let theme = *paint.theme;

    // Tło, tytuł i kontrolki okna rysuje widget.
    header_bar().paint(paint, bar, demo.controls_hover);

    let cy = bar.center().y;

    // Ikona-przełącznik paska bocznego (◧ / ▭).
    let sidebar_button = lay.sidebar_button;
    paint
        .scene
        .fill_rounded_rect(sidebar_button, theme.radius.small, theme.colors.shade);
    draw_sidebar_icon(
        paint,
        theme.colors.text,
        sidebar_button,
        lay.sidebar_visible,
    );

    let control = theme.metrics.headerbar.control;
    let plus = Rect::from_xywh(
        sidebar_button.right() + PAD,
        cy - control / 2.0,
        control,
        control,
    );
    paint
        .scene
        .fill_rounded_rect(plus, theme.radius.small, theme.colors.shade);
    draw_plus(paint, theme.colors.text, plus, 5.0);

    // Przycisk „Opcje ▾” jako widget.
    let mut options = Button::new(tr("Options")).dropdown(true);
    options.set_hovered(demo.options_hover);
    options.paint(paint, lay.options_button);

    // Menu „☰”.
    let burger = lay.burger;
    paint
        .scene
        .fill_rounded_rect(burger, theme.radius.small, theme.colors.shade);
    draw_burger(paint, theme.colors.text, burger, 8.0);
}

fn draw_sidebar(paint: &mut PaintCtx, lay: &Layout, demo: &Demo) {
    let sb = lay.sidebar;
    paint.scene.fill_rect(sb, paint.theme.colors.sidebar_bg);
    paint.scene.fill_rect(
        Rect::from_xywh(sb.right() - 1.0, sb.y(), 1.0, sb.height()),
        paint.theme.colors.border,
    );

    let mut nav = make_nav();
    nav.set_selected(demo.nav_selected);
    nav.set_hovered(demo.nav_hover);
    nav.paint(paint, sb);
}

fn draw_content(
    paint: &mut PaintCtx,
    area: Rect,
    lay: &Layout,
    demo: &Demo,
    preference: ThemePreference,
    accent: Accent,
) {
    let theme = *paint.theme;
    paint.scene.fill_rect(area, theme.colors.view_bg);

    let lg = theme.spacing.lg;
    let md = theme.spacing.md;
    let body = theme.typography.body.size;

    // --- Karta 1: Wygląd (3 wiersze) ---
    let card = Rect::from_xywh(
        area.x() + lg,
        area.y() + lg,
        (area.width() - 2.0 * lg).max(0.0),
        208.0,
    );
    Card::new(tr("Appearance")).paint(paint, card);

    draw_row_label(paint, card, lay.theme_switcher, &tr("Theme"));
    theme_switcher(preference, demo.theme_hover).paint(paint, lay.theme_switcher);

    draw_row_label(paint, card, lay.accents[0], &tr("Accent color"));
    draw_accent_dots(paint, &lay.accents, accent);

    draw_row_label(paint, card, lay.language, &tr("Language"));
    let codes = libluna_i18n::languages();
    language_dropdown(&codes).paint(paint, lay.language);

    // --- Karta 2: Ustawienia ---
    let card2 = Rect::from_xywh(card.x(), card.bottom() + lg, card.width(), 320.0);
    Card::new(tr("Settings")).paint(paint, card2);

    draw_row_label(paint, card2, lay.option_a, &tr("Option A"));
    let mut sw = Switch::new(demo.option_a);
    sw.set_hovered(demo.switch_hover);
    sw.paint(paint, lay.option_a);

    draw_row_label(paint, card2, lay.volume_track, &tr("Volume level"));
    Slider::new(demo.volume).paint(paint, lay.volume_track);
    paint.label_right(
        lay.volume_track.x() - 12.0,
        lay.volume_track.center().y,
        &format!("{}%", (demo.volume * 100.0).round() as i32),
        body,
        weight::NORMAL,
        theme.colors.text,
    );

    draw_row_label(paint, card2, lay.view_switcher, &tr("View"));
    view_switcher(demo.view_mode, demo.view_hover).paint(paint, lay.view_switcher);

    draw_row_label(paint, card2, lay.bottom_bar, &tr("Bottom bar"));
    Switch::new(demo.show_bottom_bar).paint(paint, lay.bottom_bar);

    draw_row_label(paint, card2, lay.sidebar_toggle, &tr("Sidebar"));
    Switch::new(demo.sidebar_visible).paint(paint, lay.sidebar_toggle);

    // --- Karta 3: Zakładki ---
    let card3 = Rect::from_xywh(card.x(), card2.bottom() + lg, card.width(), 152.0);
    Card::new(tr("Tabs")).paint(paint, card3);

    // Wiersz 0: pasek zakładek (widget TabBar).
    tab_bar(demo).paint(paint, lay.tabs);

    // Wiersz 1: aktywna strona (model TabView).
    paint.label_left(
        card3.x() + md,
        lay.tabs_value.center().y,
        &tr("Active tab"),
        body,
        weight::NORMAL,
        theme.colors.text,
    );
    let active = demo.tabs.selected_title().map(tr).unwrap_or_default();
    paint.label_right(
        lay.tabs_value.right() - md,
        lay.tabs_value.center().y,
        &active,
        body,
        weight::MEDIUM,
        theme.colors.text,
    );

    // --- Karta 4: Pliki (PathBar + SearchEntry + ItemView) ---
    let card4 = Rect::from_xywh(card.x(), card3.bottom() + lg, card.width(), 196.0);
    Card::new(tr("Files")).paint(paint, card4);
    path_bar(demo).paint(paint, lay.path);

    let mut search = demo.search.clone();
    search.set_placeholder(tr("Search"));
    search.paint(paint, lay.search);

    item_view(demo).paint(paint, lay.items);

    let mut scrollbar = demo.files_scroll;
    scrollbar.set_content_height(demo.items.content_height(lay.items, &theme));
    scrollbar.paint_scrollbar(paint, lay.items);
}

/// Rysuje etykietę wiersza ustawień (`ActionRow`) z miejscem na kontrolkę po prawej.
fn draw_row_label(paint: &mut PaintCtx, container: Rect, control: Rect, title: &str) {
    let theme = *paint.theme;
    let md = theme.spacing.md;
    let row = Rect::from_xywh(
        container.x(),
        control.center().y - ROW_H / 2.0,
        container.width(),
        ROW_H,
    );
    let suffix = (container.width() - (control.x() - container.x()) - md).max(0.0);
    ActionRow::new(title).paint(paint, row, suffix);
}

fn draw_accent_dots(paint: &mut PaintCtx, dots: &[Rect; 9], accent: Accent) {
    let theme = *paint.theme;
    for (i, r) in dots.iter().enumerate() {
        let a = Accent::ALL[i];
        paint
            .scene
            .fill_rounded_rect(*r, r.width() / 2.0, a.color());
        if a == accent {
            let ring = r.inflate(Insets::splat(3.0));
            paint
                .scene
                .stroke_rounded_rect(ring, ring.width() / 2.0, 2.0, theme.colors.text);
        }
    }
}

fn draw_statusbar(paint: &mut PaintCtx, bar: Rect) {
    let theme = *paint.theme;
    let body = theme.typography.body.size;
    paint.scene.fill_rect(bar, theme.colors.headerbar_bg);
    paint.scene.fill_rect(
        Rect::from_xywh(bar.x(), bar.y(), bar.width(), 1.0),
        theme.colors.border,
    );
    paint.label_left(
        bar.x() + PAD,
        bar.center().y,
        &tr("Status: Ready"),
        body,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    let version = format!("{} 1.0.0", tr("Version"));
    paint.label_right(
        bar.right() - PAD,
        bar.center().y,
        &version,
        body,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

// --- Ikony rysowane wektorowo (bez zależności od fontu) ---

fn draw_plus(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    paint.scene.stroke_line(
        Point::new(c.x - arm, c.y),
        Point::new(c.x + arm, c.y),
        2.0,
        color,
    );
    paint.scene.stroke_line(
        Point::new(c.x, c.y - arm),
        Point::new(c.x, c.y + arm),
        2.0,
        color,
    );
}

fn draw_burger(paint: &mut PaintCtx, color: Color, box_: Rect, arm: f64) {
    let c = box_.center();
    for dy in [-6.0, 0.0, 6.0] {
        paint.scene.stroke_line(
            Point::new(c.x - arm, c.y + dy),
            Point::new(c.x + arm, c.y + dy),
            2.0,
            color,
        );
    }
}

/// Ikona paska bocznego: `◧` gdy widoczny, `▭` (sam kontur) gdy ukryty.
fn draw_sidebar_icon(paint: &mut PaintCtx, color: Color, box_: Rect, filled: bool) {
    let c = box_.center();
    let w = 18.0;
    let h = 14.0;
    let outer = Rect::from_xywh(c.x - w / 2.0, c.y - h / 2.0, w, h);
    paint.scene.stroke_rounded_rect(outer, 3.0, 1.5, color);
    if filled {
        let inner = Rect::from_xywh(outer.x() + 2.0, outer.y() + 2.0, w * 0.36, h - 4.0);
        paint.scene.fill_rounded_rect(inner, 2.0, color);
    }
}

// --- Popover „Opcje” (rozmiar ikon, sortowanie, ukryte pliki) ---

const ICON_MIN: i64 = 16;
const ICON_MAX: i64 = 64;
const OPTIONS_W: f64 = 260.0;

/// Rozkład popovera opcji.
pub struct OptionsLayout {
    pub panel: Rect,
    pub stepper: Rect,
    pub radios: Rect,
    pub check: Rect,
    pub icon_label_cy: f64,
    pub sort_label_cy: f64,
    pub sep1_y: f64,
    pub sep2_y: f64,
}

fn sort_options() -> RadioGroup {
    RadioGroup::new([tr("Name"), tr("Date modified"), tr("Size")])
}

/// Liczy rozkład popovera opcji (pod przyciskiem „Opcje ▾”).
pub fn options_layout(size: Size, theme: &Theme, lay: &Layout) -> OptionsLayout {
    let md = theme.spacing.md;
    let stepper_h = theme.metrics.stepper.height;
    let stepper_w = theme.metrics.stepper.width;
    let radio_h = 3.0 * theme.metrics.radio.row_h;
    let check_h = theme.metrics.check.size.max(24.0);
    let row = stepper_h;
    let cap = 20.0;
    let sep = 1.0;

    let h = md + row + md + sep + md + cap + radio_h + md + sep + md + check_h + md;
    let anchor = lay.options_button;
    let mut x = anchor.x();
    if x + OPTIONS_W > size.width - 6.0 {
        x = size.width - 6.0 - OPTIONS_W;
    }
    x = x.max(6.0);
    let mut y = anchor.bottom() + 6.0;
    if y + h > size.height - 6.0 {
        y = anchor.y() - h - 6.0;
    }
    y = y.max(6.0);
    let panel = Rect::from_xywh(x, y, OPTIONS_W, h);

    let icon_label_cy = panel.y() + md + row / 2.0;
    let stepper = Rect::from_xywh(
        panel.right() - md - stepper_w,
        panel.y() + md + (row - stepper_h) / 2.0,
        stepper_w,
        stepper_h,
    );
    let sep1_y = panel.y() + md + row + md;
    let sort_label_cy = sep1_y + md + cap / 2.0;
    let radios = Rect::from_xywh(
        panel.x() + md,
        sep1_y + md + cap,
        panel.width() - 2.0 * md,
        radio_h,
    );
    let sep2_y = sep1_y + md + cap + radio_h + md;
    let check = Rect::from_xywh(
        panel.x() + md,
        sep2_y + md,
        panel.width() - 2.0 * md,
        check_h,
    );
    OptionsLayout {
        panel,
        stepper,
        radios,
        check,
        icon_label_cy,
        sort_label_cy,
        sep1_y,
        sep2_y,
    }
}

fn draw_options(paint: &mut PaintCtx, size: Size, lay: &Layout, demo: &Demo) {
    let theme = *paint.theme;
    let md = theme.spacing.md;
    let body = theme.typography.body.size;
    let ol = options_layout(size, &theme, lay);

    paint
        .scene
        .fill_rounded_rect(ol.panel, theme.radius.medium, theme.colors.card_bg);
    paint
        .scene
        .stroke_rounded_rect(ol.panel, theme.radius.medium, 1.0, theme.colors.border);

    paint.label_left(
        ol.panel.x() + md,
        ol.icon_label_cy,
        &tr("Icon size"),
        body,
        weight::NORMAL,
        theme.colors.text,
    );
    let mut stepper = Stepper::new(demo.icon_size, ICON_MIN, ICON_MAX).step(8);
    stepper.set_hovered(demo.options_stepper_hover);
    stepper.paint(paint, ol.stepper);

    paint.scene.fill_rect(
        Rect::from_xywh(
            ol.panel.x() + md,
            ol.sep1_y,
            (ol.panel.width() - 2.0 * md).max(0.0),
            1.0,
        ),
        theme.colors.border,
    );

    paint.label_left(
        ol.panel.x() + md,
        ol.sort_label_cy,
        &tr("Sort by"),
        body,
        weight::NORMAL,
        theme.colors.text,
    );
    let mut radios = sort_options();
    radios.set_selected(demo.sort);
    radios.set_hovered(demo.options_radio_hover);
    radios.paint(paint, ol.radios);

    paint.scene.fill_rect(
        Rect::from_xywh(
            ol.panel.x() + md,
            ol.sep2_y,
            (ol.panel.width() - 2.0 * md).max(0.0),
            1.0,
        ),
        theme.colors.border,
    );

    let mut check = CheckButton::new(tr("Show hidden files"));
    check.set_checked(demo.show_hidden);
    check.set_hovered(demo.options_check_hover);
    check.paint(paint, ol.check);
}
