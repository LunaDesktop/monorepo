//! Katalog galerii deweloperskiej: każdy widget libLuny z opisem, podglądem
//! i przykładowym kodem. Dane są statyczne — kafelki rysuje `gallery_page`.

use crate::gallery_previews as pv;

/// Kategoria kafelka w galerii.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Basics,
    Forms,
    Navigation,
    Layout,
    Collections,
    Media,
    Feedback,
    Windows,
}

impl Category {
    /// Wszystkie kategorie w kolejności zakładek.
    pub const ALL: [Category; 8] = [
        Category::Basics,
        Category::Forms,
        Category::Navigation,
        Category::Layout,
        Category::Collections,
        Category::Media,
        Category::Feedback,
        Category::Windows,
    ];

    /// Etykieta zakładki.
    pub fn label(self) -> &'static str {
        match self {
            Category::Basics => "Podstawy",
            Category::Forms => "Formularze",
            Category::Navigation => "Nawigacja",
            Category::Layout => "Układ",
            Category::Collections => "Kolekcje",
            Category::Media => "Media",
            Category::Feedback => "Informacje",
            Category::Windows => "Okna",
        }
    }
}

/// Pojedynczy wpis galerii: widget (lub mała rodzina widgetów), podgląd i kod.
pub struct DemoSpec {
    /// Nazwa wyświetlana na kafelku.
    pub name: &'static str,
    /// Kategoria, do której trafia kafelek.
    pub category: Category,
    /// Krótki opis zastosowania.
    pub description: &'static str,
    /// Plik źródłowy widgetu (ścieżka względem `libluna/crates/libluna-widgets/src`).
    pub source: &'static str,
    /// Przykład użycia w modelu immediate (`stan` + `paint` + `*_at`).
    pub code: &'static str,
    /// Rysuje miniaturowy, żywy podgląd widgetu.
    pub preview: fn(&mut libluna_widgets::PaintCtx, libluna_core::Rect),
}

/// Kompletna lista widgetów. Każdy publiczny widget libLuny ma tu swój kafelek;
/// aliasy (`Carousel`, `Notebook`, `ScrolledWindow`, `StackSwitcher`, `ListBox`)
/// są opisane przy swoich widgetach bazowych.
pub static DEMOS: &[DemoSpec] = &[
    // ── Podstawy ────────────────────────────────────────────────────────────
    DemoSpec {
        name: "Button",
        category: Category::Basics,
        description: "Przycisk akcji: domyślny, sugerowany, destrukcyjny i wyłączony.",
        source: "button.rs",
        code: r#"let mut button = Button::new("Save");
button.paint(ctx, rect);

if button.click_at(rect, pos) {
    save();
}"#,
        preview: pv::button,
    },
    DemoSpec {
        name: "ToggleButton / LinkButton / ColorButton",
        category: Category::Basics,
        description: "Przyciski specjalne: przełączny, link i próbka koloru.",
        source: "buttons.rs",
        code: r#"let mut bold = ToggleButton::new("Bold").active(true);
bold.click_at(rect, pos);

LinkButton::new("Documentation").paint(ctx, rect);

let mut color = ColorButton::new(accent);
color.set_color(picked);"#,
        preview: pv::buttons_extra,
    },
    DemoSpec {
        name: "SplitButton",
        category: Category::Basics,
        description: "Akcja główna + osobna strzałka otwierająca menu.",
        source: "split_button.rs",
        code: r#"let split = SplitButton::new("Export");
split.paint(ctx, rect);

let r = split.rects(rect, theme);
if split.main_at(rect, theme, pos) { export(); }
if split.arrow_at(rect, theme, pos) { menu(); }"#,
        preview: pv::split_button,
    },
    DemoSpec {
        name: "MenuButton",
        category: Category::Basics,
        description: "Przycisk otwierający menu; także wariant „hamburger”.",
        source: "menu_button.rs",
        code: r#"let mut menu = MenuButton::new("Options");
menu.paint(ctx, rect);

if menu.click_at(rect, pos) {
    menu.toggle();
}"#,
        preview: pv::menu_button,
    },
    DemoSpec {
        name: "Label",
        category: Category::Basics,
        description: "Etykieta tekstowa z wyrównaniem, pogrubieniem i wyciszeniem.",
        source: "display.rs",
        code: r#"let title = Label::new("Section").bold(true);
title.paint(ctx, rect);

let hint = Label::new("Details")
    .dim(true)
    .align(LabelAlign::End);
hint.paint(ctx, rect);"#,
        preview: pv::label,
    },
    DemoSpec {
        name: "Separator",
        category: Category::Basics,
        description: "Cienka linia oddzielająca, pozioma lub pionowa.",
        source: "display.rs",
        code: r#"Separator::horizontal().paint(ctx, rect);
Separator::vertical().paint(ctx, rect);"#,
        preview: pv::separator,
    },
    DemoSpec {
        name: "Image",
        category: Category::Basics,
        description: "Ikony wektorowe dostępne w zestawie `ImageIcon`.",
        source: "image.rs",
        code: r#"Image::new(ImageIcon::Folder)
    .size(24.0)
    .paint(ctx, rect);"#,
        preview: pv::image,
    },
    DemoSpec {
        name: "Avatar",
        category: Category::Basics,
        description: "Okrągły awatar z inicjałami, skalowany do rozmiaru motywu.",
        source: "avatar.rs",
        code: r#"let avatar = Avatar::new("PL");
avatar.paint(ctx, rect);"#,
        preview: pv::avatar,
    },
    DemoSpec {
        name: "Spinner",
        category: Category::Basics,
        description: "Wskaźnik zajętości (obracający się łuk); fazę można ustawić ręcznie.",
        source: "spinner.rs",
        code: r#"let mut spinner = Spinner::new();
spinner.set_progress(0.35);
spinner.paint(ctx, rect);"#,
        preview: pv::spinner,
    },
    DemoSpec {
        name: "ProgressBar / LevelBar",
        category: Category::Basics,
        description: "Postęp liniowy z etykietą oraz poziomy w podzielonej skali.",
        source: "display.rs",
        code: r#"ProgressBar::new()
    .fraction(0.65)
    .text("65%")
    .paint(ctx, rect);

LevelBar::new()
    .value(0.72)
    .segments(5)
    .paint(ctx, rect);"#,
        preview: pv::progress_level,
    },
    DemoSpec {
        name: "ShortcutLabel",
        category: Category::Basics,
        description: "Skrót klawiszowy renderowany jako osobne klawisze.",
        source: "shortcut_label.rs",
        code: r#"let keys = ShortcutLabel::new(["Ctrl", "N"]);
keys.paint(ctx, rect);"#,
        preview: pv::shortcut,
    },
    DemoSpec {
        name: "Tooltip",
        category: Category::Basics,
        description: "Dymek podpowiedzi pozycjonowany względem kotwicy.",
        source: "tooltip.rs",
        code: r#"let tip = Tooltip::new("Reload the list");
let area = tip.rect(anchor, theme, text);
tip.paint(ctx, area);"#,
        preview: pv::tooltip,
    },
    DemoSpec {
        name: "IconName / paint_icon",
        category: Category::Basics,
        description: "Spójny zestaw 47 pastelowych ikon wektorowych (pliki, terminal, multimedia, urządzenia).",
        source: "icon.rs",
        code: r#"paint_icon(ctx, IconName::Files, rect, color);
paint_icon(ctx, IconName::Search, rect, color);
paint_icon(ctx, IconName::Moon, rect, color);"#,
        preview: pv::icons,
    },
    // ── Formularze ──────────────────────────────────────────────────────────
    DemoSpec {
        name: "Entry",
        category: Category::Forms,
        description: "Pole tekstowe: kursor, zaznaczanie, ikona wiodąca i czyszczenie.",
        source: "entry.rs",
        code: r#"let mut entry = Entry::new("Name")
    .leading(EntryIcon::Search)
    .clearable(true);
entry.set_text("Luna");
entry.paint(ctx, rect);

if entry.clear_at(rect, theme, pos) {
    entry.clear();
}"#,
        preview: pv::entry,
    },
    DemoSpec {
        name: "EditableLabel",
        category: Category::Forms,
        description: "Etykieta, która po kliknięciu staje się polem edycji.",
        source: "entries.rs",
        code: r#"let mut label = EditableLabel::new("Project name");
label.paint(ctx, rect);

if edit_requested {
    label.start_editing();
}
label.commit();"#,
        preview: pv::editable_label,
    },
    DemoSpec {
        name: "PasswordEntry",
        category: Category::Forms,
        description: "Pole hasła z maskowaniem i podglądem („oko”).",
        source: "entries.rs",
        code: r#"let mut password = PasswordEntry::new("Password");
password.insert_str("hunter2");
password.paint(ctx, rect);

if password.peek_at(rect, theme, pos) {
    password.toggle_peek();
}"#,
        preview: pv::password,
    },
    DemoSpec {
        name: "SearchEntry",
        category: Category::Forms,
        description: "Pole wyszukiwania z lupką i przyciskiem czyszczenia.",
        source: "search_entry.rs",
        code: r#"let mut search = SearchEntry::new("Search files");
search.insert_str("report");
search.paint(ctx, rect);

if search.clear_at(rect, theme, pos) {
    search.clear();
}"#,
        preview: pv::search_entry,
    },
    DemoSpec {
        name: "SearchBar",
        category: Category::Forms,
        description: "Pasek trybu wyszukiwania z przyciskiem odsłaniania.",
        source: "search_bar.rs",
        code: r#"let mut bar = SearchBar::with_mode(true);
bar.paint(ctx, rect);

if bar.hit(rect, theme, pos) {
    bar.toggle();
}"#,
        preview: pv::search_bar,
    },
    DemoSpec {
        name: "TextView",
        category: Category::Forms,
        description: "Wielolinijkowe pole tekstowe z edycją i zaznaczaniem.",
        source: "text_view.rs",
        code: r#"let mut view = TextView::new("Write a note…");
view.insert_str("Hello\nLuna");
view.paint(ctx, rect);"#,
        preview: pv::text_view,
    },
    DemoSpec {
        name: "SpinButton",
        category: Category::Forms,
        description: "Pole liczbowe ze strzałkami góra/dół i zakresem.",
        source: "spin_button.rs",
        code: r#"let mut spin = SpinButton::new(8.0, 1.0, 32.0)
    .step(2.0);
spin.paint(ctx, rect);

spin.click_at(rect, theme, pos);"#,
        preview: pv::spin_button,
    },
    DemoSpec {
        name: "Stepper",
        category: Category::Forms,
        description: "Krokowiec `−  wartość  +` dla małych zakresów.",
        source: "stepper.rs",
        code: r#"let mut step = Stepper::new(32, 8, 256)
    .step(8);
step.paint(ctx, rect);

step.click_at(rect, theme, pos);"#,
        preview: pv::stepper,
    },
    DemoSpec {
        name: "Slider",
        category: Category::Forms,
        description: "Suwak 0..1; trafienie i wartość liczone z prostokąta toru.",
        source: "slider.rs",
        code: r#"let slider = Slider::new(0.65);
slider.paint(ctx, track);

if slider.hit(track, pos, theme) {
    value = slider.value_at(track, pos);
}"#,
        preview: pv::slider,
    },
    DemoSpec {
        name: "Switch",
        category: Category::Forms,
        description: "Przełącznik wł./wył. z animowanym uchwytem.",
        source: "switch.rs",
        code: r#"let mut switch = Switch::new(true);
switch.paint(ctx, rect);

if switch.click_at(rect, pos) {
    enabled = switch.is_on();
}"#,
        preview: pv::switch,
    },
    DemoSpec {
        name: "CheckButton",
        category: Category::Forms,
        description: "Pole wyboru (checkbox) z etykietą.",
        source: "check_button.rs",
        code: r#"let mut check = CheckButton::new("Show hidden files");
check.paint(ctx, rect);

if check.click_at(rect, pos) {
    hidden = check.checked();
}"#,
        preview: pv::check_button,
    },
    DemoSpec {
        name: "RadioGroup",
        category: Category::Forms,
        description: "Lista opcji z pojedynczym wyborem.",
        source: "radio_group.rs",
        code: r#"let mut group = RadioGroup::new(["Name", "Size", "Date"]);
group.paint(ctx, rect);

if group.click_at(rect, pos, theme) {
    sort = group.selected();
}"#,
        preview: pv::radio_group,
    },
    DemoSpec {
        name: "DropDown",
        category: Category::Forms,
        description: "Lista rozwijana; rozwinięcie zwraca gotowe `Menu`.",
        source: "dropdown.rs",
        code: r#"let mut dd = DropDown::new(["Light", "Dark", "Auto"]);
dd.paint(ctx, rect);

if dd.click_at(rect, pos) {
    open_menu(dd.menu());
}"#,
        preview: pv::dropdown,
    },
    DemoSpec {
        name: "ToggleGroup",
        category: Category::Forms,
        description: "Grupa niezależnych przełączników (wielokrotny wybór).",
        source: "toggle_group.rs",
        code: r#"let mut group = ToggleGroup::new(["B", "I", "U"])
    .with_active(0, true);
group.paint(ctx, rect);

group.click_at(rect, pos);"#,
        preview: pv::toggle_group,
    },
    DemoSpec {
        name: "Calendar",
        category: Category::Forms,
        description: "Miesiąc z siatką dni, wyborem i nawigacją prev/next.",
        source: "calendar.rs",
        code: r#"let mut cal = Calendar::new(2026, 10);
cal.set_selected(25);
cal.paint(ctx, rect, "October 2026", &weekdays);

if let Some(day) = cal.day_at(rect, theme, pos) {
    cal.set_selected(day);
}"#,
        preview: pv::calendar,
    },
    DemoSpec {
        name: "ColorChooser",
        category: Category::Forms,
        description: "Paleta kolorów w siatce z zaznaczeniem i hit-testem.",
        source: "color_chooser.rs",
        code: r#"let mut chooser = ColorChooser::new(
    ColorChooser::default_palette(),
);
chooser.set_columns(6);
chooser.paint(ctx, rect);

if chooser.click_at(rect, theme, pos) {
    accent = chooser.selected_color();
}"#,
        preview: pv::color_chooser,
    },
    DemoSpec {
        name: "EmojiChooser",
        category: Category::Forms,
        description: "Siatka symboli/emoji z zaznaczeniem i podglądem wyboru.",
        source: "emoji_chooser.rs",
        code: r#"let mut emoji = EmojiChooser::new(
    EmojiChooser::default_palette(),
);
emoji.set_columns(8);
emoji.paint(ctx, rect);

if emoji.click_at(rect, theme, pos) {
    insert(emoji.selected_emoji());
}"#,
        preview: pv::emoji_chooser,
    },
    // ── Nawigacja ───────────────────────────────────────────────────────────
    DemoSpec {
        name: "ViewSwitcher",
        category: Category::Navigation,
        description: "Segmentowy przełącznik widoku; alias `StackSwitcher`.",
        source: "view_switcher.rs",
        code: r#"let mut view = ViewSwitcher::new(["List", "Grid"]);
view.paint(ctx, rect);

if let Some(i) = view.index_at(rect, pos) {
    view.set_selected(i);
}"#,
        preview: pv::view_switcher,
    },
    DemoSpec {
        name: "NavList",
        category: Category::Navigation,
        description: "Menu nawigacji z sekcjami; alias `ListBox`.",
        source: "nav_list.rs",
        code: r#"let mut nav = NavList::new(vec![
    NavEntry::Section("System".into()),
    NavEntry::Item("Network".into()),
    NavEntry::Item("Display".into()),
]);
nav.paint(ctx, rect);

if let Some(i) = nav.index_at(rect, pos, theme) {
    nav.set_selected(Some(i));
}"#,
        preview: pv::nav_list,
    },
    DemoSpec {
        name: "NavigationRail",
        category: Category::Navigation,
        description: "Pionowy pasek nawigacji (ikony + etykiety).",
        source: "navigation_rail.rs",
        code: r#"let mut rail = NavigationRail::new(["Home", "Files", "Media"]);
rail.paint(ctx, rect);

if let Some(i) = rail.index_at(rect, theme, pos) {
    rail.set_selected(i);
}"#,
        preview: pv::navigation_rail,
    },
    DemoSpec {
        name: "TabBar",
        category: Category::Navigation,
        description: "Pasek zakładek z przewijaniem i zamykaniem.",
        source: "tab_bar.rs",
        code: r#"let mut tabs = TabBar::new(["General", "Advanced", "About"]);
tabs.set_selected(1);
tabs.paint(ctx, rect);

tabs.click_at(rect, text, theme, pos);"#,
        preview: pv::tab_bar,
    },
    DemoSpec {
        name: "TabView",
        category: Category::Navigation,
        description: "Model stron zakładek (dodawanie, zamykanie, wybór); alias `Notebook`.",
        source: "tab_view.rs",
        code: r#"let mut view = TabView::new();
view.add_titled("General");
view.add_titled("Advanced");
view.select(1);

view.tab_bar().paint(ctx, rect);"#,
        preview: pv::tab_view,
    },
    DemoSpec {
        name: "PathBar",
        category: Category::Navigation,
        description: "Ścieżka w postaci okruszków ze zwijaniem w wielokropek.",
        source: "path_bar.rs",
        code: r#"let mut path = PathBar::new(["Home", "Documents", "Projects"]);
path.paint(ctx, rect);

if let Some(i) = path.index_at(rect, text, theme, pos) {
    path.truncate(i + 1);
}"#,
        preview: pv::path_bar,
    },
    DemoSpec {
        name: "Pager",
        category: Category::Navigation,
        description: "Karuzela stron z przeciąganiem i kropkami; alias `Carousel`.",
        source: "pager.rs",
        code: r#"let mut pager = Pager::new(3);
pager.set_active(1);
pager.paint(ctx, rect);

for i in 0..pager.count() {
    draw_page(pager.page_rect(rect, i));
}"#,
        preview: pv::pager,
    },
    DemoSpec {
        name: "NavigationView",
        category: Category::Navigation,
        description: "Stos stron z paskiem „wstecz” i głębokością nawigacji.",
        source: "navigation.rs",
        code: r#"let mut nav = NavigationView::new();
nav.push();
nav.paint(ctx, rect);

if nav.back_at(rect, theme, pos) {
    nav.pop();
}"#,
        preview: pv::navigation_view,
    },
    DemoSpec {
        name: "NavigationSplitView",
        category: Category::Navigation,
        description: "Panel boczny + treść; panel można schować.",
        source: "navigation.rs",
        code: r#"let mut view = NavigationSplitView::new()
    .sidebar_width(220.0);

if let Some(side) = view.sidebar_rect(rect) {
    Sidebar::new().paint(ctx, side);
}
let content = view.content_rect(rect);"#,
        preview: pv::navigation_split,
    },
    DemoSpec {
        name: "OverlaySplitView",
        category: Category::Navigation,
        description: "Nakładany panel boczny (wąskie okna) z tłem przygaszającym.",
        source: "navigation.rs",
        code: r#"let mut view = OverlaySplitView::new();
view.set_open(true);

view.paint_scrim(ctx, rect);
let side = view.sidebar_rect(rect);
Sidebar::new().paint(ctx, side);"#,
        preview: pv::overlay_split,
    },
    DemoSpec {
        name: "Sidebar",
        category: Category::Navigation,
        description: "Tło i krawędź panelu bocznego; treść na `NavList`.",
        source: "navigation.rs",
        code: r#"let side = Sidebar::new();
side.paint(ctx, rect);

let inner = side.content_rect(rect, theme);
nav.paint(ctx, inner);"#,
        preview: pv::sidebar,
    },
    DemoSpec {
        name: "PopoverMenuBar",
        category: Category::Navigation,
        description: "Rząd przycisków otwierających popovery menu.",
        source: "containers.rs",
        code: r#"let mut bar = PopoverMenuBar::new(["File", "Edit", "View"]);
bar.paint(ctx, rect);

if let Some(i) = bar.click_at(rect, text, theme, pos) {
    open_popover(i);
}"#,
        preview: pv::popover_menu_bar,
    },
    DemoSpec {
        name: "Menu",
        category: Category::Navigation,
        description: "Popover z pozycjami, separatorami i skrótami.",
        source: "menu.rs",
        code: r#"let menu = Menu::new(vec![
    MenuEntry::action("New Window").shortcut("Ctrl+N"),
    MenuEntry::separator(),
    MenuEntry::action("Settings"),
]);
menu.paint(ctx, panel);"#,
        preview: pv::menu,
    },
    // ── Układ ───────────────────────────────────────────────────────────────
    DemoSpec {
        name: "Card",
        category: Category::Layout,
        description: "Karta z nagłówkiem i obszarem treści.",
        source: "card.rs",
        code: r#"let card = Card::new("Appearance");
let content = card.paint(ctx, rect);
// rysuj kontrolki wewnątrz `content`"#,
        preview: pv::card,
    },
    DemoSpec {
        name: "PreferencesGroup",
        category: Category::Layout,
        description: "Grupa ustawień: tytuł + ramka na wiersze.",
        source: "preferences.rs",
        code: r#"let group = PreferencesGroup::new("Network");
let content = group.paint(ctx, rect);

let row = PreferencesGroup::row_rect(content, 0, row_h);
row_widget.paint(ctx, row);"#,
        preview: pv::preferences_group,
    },
    DemoSpec {
        name: "PreferencesPage",
        category: Category::Layout,
        description: "Pionowy stos grup ustawień (gotowa strona preferencji).",
        source: "preferences.rs",
        code: r#"let page = PreferencesPage::new()
    .add(PreferencesGroup::new("General"), 3)
    .add(PreferencesGroup::new("Advanced"), 2);

let rows = page.paint(ctx, rect, row_h);"#,
        preview: pv::preferences_page,
    },
    DemoSpec {
        name: "ActionRow",
        category: Category::Layout,
        description: "Wiersz ustawień: tytuł/podtytuł + miejsce na kontrolkę.",
        source: "action_row.rs",
        code: r#"let row = ActionRow::new("Wi-Fi")
    .subtitle("Connected to Luna");
let suffix = row.paint(ctx, rect, 60.0);

switch.paint(ctx, suffix);"#,
        preview: pv::action_row,
    },
    DemoSpec {
        name: "Row",
        category: Category::Layout,
        description: "Rodzina „boxed list”: switch, combo, nav, expander, entry, spin…",
        source: "rows.rs",
        code: r#"let row = Row::switch("Notifications", true)
    .subtitle("Allow alerts");
row.paint(ctx, rect);

let row = Row::combo("Theme", "Dark");
row.paint(ctx, rect);"#,
        preview: pv::row,
    },
    DemoSpec {
        name: "Expander",
        category: Category::Layout,
        description: "Sekcja rozwijana z nagłówkiem i treścią.",
        source: "expander.rs",
        code: r#"let mut exp = Expander::new("Advanced");
exp.set_expanded(true);
exp.paint(ctx, rect);

if exp.click_at(rect, theme, pos) {
    exp.toggle();
}"#,
        preview: pv::expander,
    },
    DemoSpec {
        name: "FlowBox",
        category: Category::Layout,
        description: "Siatka elementów o stałym rozmiarze z zawijaniem.",
        source: "flow_box.rs",
        code: r#"let mut flow = FlowBox::new(48.0);
flow.set_count(10);
flow.paint(ctx, rect);

if let Some(i) = flow.index_at(rect, pos) {
    flow.set_selected(Some(i));
}"#,
        preview: pv::flow_box,
    },
    DemoSpec {
        name: "Paned",
        category: Category::Layout,
        description: "Dwa panele z przeciąganym separatorem.",
        source: "containers.rs",
        code: r#"let mut paned = Paned::new(Orientation::Horizontal);
paned.set_position(0.4);
paned.paint(ctx, rect);

let (left, right) = paned.panes(rect, theme);
if paned.separator_hit(rect, theme, pos) {
    paned.begin_drag(rect, theme, pos);
}"#,
        preview: pv::paned,
    },
    DemoSpec {
        name: "Stack",
        category: Category::Layout,
        description: "Stos stron z animowanym przejściem (slide, fade itd.).",
        source: "containers.rs",
        code: r#"let mut stack = Stack::with_count(3);
stack.set_transition(StackTransition::SlideHorizontal);
stack.set_visible_index(1);

stack.advance(dt, 0.25);"#,
        preview: pv::stack,
    },
    DemoSpec {
        name: "Grid / Fixed / CenterBox / Bin",
        category: Category::Layout,
        description: "Pomocniki układu: siatka, pozycja absolutna, trzy strefy i jedno dziecko.",
        source: "containers.rs, bin.rs",
        code: r#"let cells = Grid::new(2, 3).cells(rect, theme);
let child = Fixed::child(rect, 16.0, 8.0, 120.0, 32.0);

let [start, center, end] =
    CenterBox::parts(rect, 80.0, 160.0, 80.0);

let bin = Bin::new().padding(Insets::splat(8.0));
let inner = bin.content_rect(rect, child_size);"#,
        preview: pv::layout_basics,
    },
    DemoSpec {
        name: "Clamp / AspectFrame / Frame / Overlay / Revealer",
        category: Category::Layout,
        description: "Ograniczenie szerokości, proporcje, ramka, nakładka i odsłanianie.",
        source: "display.rs, helpers.rs",
        code: r#"let clamped = Clamp::new(600.0).content_rect(rect);
let framed = AspectFrame::new(16.0 / 9.0)
    .content_rect(rect);

let frame = Frame::new().label("Details");
let inner = frame.paint(ctx, rect);

let badge = Overlay::new().child_rect(
    rect, badge_size, BinAlign::End, BinAlign::Start, 12.0,
);

let mut revealer = Revealer::new();
revealer.set_fraction(0.5);
let visible = revealer.reveal_rect(rect);"#,
        preview: pv::layout_display,
    },
    DemoSpec {
        name: "WrapBox / BreakpointBin / ButtonContent",
        category: Category::Layout,
        description: "Zawijanie elementów, progi responsywności i układ ikona+etykieta.",
        source: "helpers.rs",
        code: r#"let rects = WrapBox::new()
    .spacing(8.0)
    .layout(rect, &sizes);

let bp = BreakpointBin::new(vec![600.0, 900.0]);
let active = bp.active(width);

let (icon, label) = ButtonContent::with_icon()
    .layout(rect, theme, text, "Open");"#,
        preview: pv::layout_wrap,
    },
    DemoSpec {
        name: "ToolbarView / Viewport / WindowHandle / ActionBar",
        category: Category::Layout,
        description: "Pomocniki okna: toolbar, widoczny obszar, strefa przeciągania i pasek akcji.",
        source: "helpers.rs, containers.rs",
        code: r#"let parts = ToolbarView::new()
    .top(46.0)
    .bottom(28.0)
    .rects(rect);

let visible = Viewport::content_rect(
    rect, content_size, offset,
);

let drag = WindowHandle::is_drag_region(
    bar, &controls, pos,
);

let [start, end] = ActionBar::parts(bar, theme, 100.0, 100.0);"#,
        preview: pv::layout_window,
    },
    // ── Kolekcje ────────────────────────────────────────────────────────────
    DemoSpec {
        name: "ItemView",
        category: Category::Collections,
        description: "Lista/siatka plików z ikonami, szczegółami i wielokrotnym wyborem.",
        source: "item_view.rs",
        code: r#"let mut view = ItemView::new(vec![
    Item::folder("Projects"),
    Item::new("Report").detail("PDF"),
    Item::new("Notes").detail("TXT"),
]);
view.set_mode(ViewMode::List);
view.paint(ctx, rect);

if let Some(i) = view.index_at(rect, pos, theme) {
    view.toggle_selected(i);
}"#,
        preview: pv::item_view,
    },
    DemoSpec {
        name: "ColumnView",
        category: Category::Collections,
        description: "Tabela kolumnowa z nagłówkiem, wagami i zaznaczeniem wiersza.",
        source: "column_view.rs",
        code: r#"let mut table = ColumnView::new(vec![
    Column::new("Name").weight(2.0),
    Column::new("Size"),
]);
table.set_rows(5);
table.paint(ctx, rect);"#,
        preview: pv::column_view,
    },
    DemoSpec {
        name: "ScrollView",
        category: Category::Collections,
        description: "Przewijanie pionowe z „kciukiem”; alias `ScrolledWindow`.",
        source: "scroll_view.rs",
        code: r#"let mut view = ScrollView::new();
view.set_content_height(content_height);
view.on_scroll(dy, viewport);
view.paint_scrollbar(ctx, viewport);"#,
        preview: pv::scroll_view,
    },
    DemoSpec {
        name: "Scrollbar / Adjustment",
        category: Category::Collections,
        description: "Pasek przewijania obu osi oraz model zakresu (`Adjustment`).",
        source: "scrollbar.rs",
        code: r#"let mut adj = Adjustment::new(0.0, 200.0, 60.0);
adj.set_fraction(0.35);

let mut bar = Scrollbar::new(Orientation::Vertical);
bar.set_adjustment(adj);
bar.paint(ctx, viewport);"#,
        preview: pv::scrollbar,
    },
    // ── Media ───────────────────────────────────────────────────────────────
    DemoSpec {
        name: "MediaControls",
        category: Category::Media,
        description: "Odtwarzacz: tytuł, wykonawca, postęp, czas i sterowanie.",
        source: "media_controls.rs",
        code: r#"let mut media = MediaControls::new("Aurora", "Luna Ensemble");
media.set_times("1:42", "4:07");
media.set_position(0.41);
media.paint(ctx, rect);

if media.play_at(rect, theme, text, pos) {
    media.toggle();
}"#,
        preview: pv::media_controls,
    },
    DemoSpec {
        name: "Picture",
        category: Category::Media,
        description: "Obraz z dopasowaniem: Contain, Cover, Fill, FitWidth, FitHeight.",
        source: "picture.rs",
        code: r#"let mut pic = Picture::with_image(image);
pic.set_fit(ContentFit::Cover);
pic.paint(ctx, rect);"#,
        preview: pv::picture,
    },
    // ── Informacje ──────────────────────────────────────────────────────────
    DemoSpec {
        name: "InfoBar",
        category: Category::Feedback,
        description: "Pasek komunikatu: info, pytanie, ostrzeżenie, błąd + akcja.",
        source: "infobar.rs",
        code: r#"let bar = InfoBar::new("Storage almost full")
    .kind(InfoBarKind::Warning)
    .action("Manage");
bar.paint(ctx, rect);"#,
        preview: pv::info_bar,
    },
    DemoSpec {
        name: "Banner",
        category: Category::Feedback,
        description: "Wyróżniony komunikat z opisem i akcją.",
        source: "banner.rs",
        code: r#"let banner = Banner::new("Update available")
    .kind(BannerKind::Info)
    .description("libLuna 0.0.2 is ready.")
    .action("Install");
banner.paint(ctx, rect);"#,
        preview: pv::banner,
    },
    DemoSpec {
        name: "Toast",
        category: Category::Feedback,
        description: "Krótkie powiadomienie-nakładka z akcją (np. „Cofnij”).",
        source: "toast.rs",
        code: r#"let mut toast = Toast::new("Accent changed")
    .action("Undo");
toast.show_animated();
toast.paint(ctx, window_size);"#,
        preview: pv::toast,
    },
    DemoSpec {
        name: "StatusPage",
        category: Category::Feedback,
        description: "Ekran stanu: ikona, tytuł, opis i akcja (pusty widok, brak wyników).",
        source: "status_page.rs",
        code: r#"let page = StatusPage::new("No results")
    .icon(StatusIcon::Search)
    .description("Try another phrase")
    .action("Clear");
page.paint(ctx, rect);"#,
        preview: pv::status_page,
    },
    DemoSpec {
        name: "Statusbar",
        category: Category::Feedback,
        description: "Dolny pasek stanu: komunikat po lewej, licznik po prawej.",
        source: "statusbar.rs",
        code: r#"let mut status = Statusbar::new();
status.set_message("Ready");
status.set_right("8 items");
status.paint(ctx, rect);"#,
        preview: pv::statusbar,
    },
    DemoSpec {
        name: "Popover",
        category: Category::Feedback,
        description: "Panel podpowiedziany do kotwicy (nad/pod/obok) z zamykaniem.",
        source: "popover.rs",
        code: r#"let popover = Popover::new(PopoverPlacement::Above);
let panel = popover.place(anchor, size, viewport, theme);
popover.paint(ctx, panel);"#,
        preview: pv::popover,
    },
    // ── Okna i dialogi ──────────────────────────────────────────────────────
    DemoSpec {
        name: "HeaderBar",
        category: Category::Windows,
        description: "Pasek tytułu CSD: tytuł, kontrolki okna i obszar na zawartość.",
        source: "header_bar.rs",
        code: r#"let bar = HeaderBar::new()
    .title("Files")
    .controls(ControlsSide::Right);
bar.paint(ctx, rect, hovered_control);"#,
        preview: pv::header_bar,
    },
    DemoSpec {
        name: "WindowTitle",
        category: Category::Windows,
        description: "Tytuł i podtytuł okna z układem pod kontrolki.",
        source: "helpers.rs",
        code: r#"let title = WindowTitle::new("Documents")
    .subtitle("12 items");
title.paint(ctx, rect);"#,
        preview: pv::window_title,
    },
    DemoSpec {
        name: "Dialog",
        category: Category::Windows,
        description: "Bazowe okno modalne: przygaszenie, panel, nagłówek i zamknięcie.",
        source: "dialogs.rs",
        code: r#"let dialog = Dialog::new("Preferences")
    .size(400.0, 280.0);
let rects = dialog.paint(ctx, window_size);

if dialog.close_at(window_size, theme, pos) {
    close_dialog();
}"#,
        preview: pv::dialog,
    },
    DemoSpec {
        name: "AlertDialog",
        category: Category::Windows,
        description: "Alert z treścią i przyciskami akcji (wynik przez `action_at`).",
        source: "dialogs.rs",
        code: r#"let alert = AlertDialog::new(
    "Delete file?",
    "This cannot be undone.",
)
.button("Cancel")
.button("Delete");
alert.paint(ctx, window_size);

if let Some(i) = alert.action_at(window_size, theme, text, pos) {
    run_action(i);
}"#,
        preview: pv::alert_dialog,
    },
    DemoSpec {
        name: "AboutDialog",
        category: Category::Windows,
        description: "Okno „O aplikacji”: nazwa, wersja, autor i strona.",
        source: "dialogs.rs",
        code: r#"let about = AboutDialog::new("Luna Files", "0.0.1")
    .developer("libLuna contributors")
    .website("https://example.org");
about.paint(ctx, window_size);"#,
        preview: pv::about_dialog,
    },
    DemoSpec {
        name: "ShortcutsDialog",
        category: Category::Windows,
        description: "Lista skrótów klawiszowych z etykietami akcji.",
        source: "dialogs.rs",
        code: r#"let dialog = ShortcutsDialog::new()
    .entry("New window", ["Ctrl", "N"])
    .entry("Search", ["Ctrl", "F"]);
dialog.paint(ctx, window_size);"#,
        preview: pv::shortcuts_dialog,
    },
    DemoSpec {
        name: "PreferencesDialog",
        category: Category::Windows,
        description: "Okno ustawień o zadanej wysokości, gotowe na `PreferencesPage`.",
        source: "dialogs.rs",
        code: r#"let dialog = PreferencesDialog::new("Preferences", 420.0);
let rects = dialog.paint(ctx, window_size);
// treść rysuj w `rects.content`"#,
        preview: pv::preferences_dialog,
    },
    DemoSpec {
        name: "BottomSheet",
        category: Category::Windows,
        description: "Dolny panel modalny: grabber, zamykanie, animacja i przeciąganie.",
        source: "bottom_sheet.rs",
        code: r#"let mut sheet = BottomSheet::new().title("About");
sheet.set_open_animated(true);
sheet.paint(ctx, window_size);

if sheet.close_at(window_size, theme, pos) {
    sheet.set_open_animated(false);
}"#,
        preview: pv::bottom_sheet,
    },
    DemoSpec {
        name: "Assistant",
        category: Category::Windows,
        description: "Kreator kroków z nagłówkiem, wskaźnikiem i przyciskami Nawigacji.",
        source: "assistant.rs",
        code: r#"let mut wizard = Assistant::new([
    "Welcome", "Details", "Done",
]);
wizard.paint(ctx, rect, "Cancel", "Next");

if wizard.is_last() {
    finish();
}"#,
        preview: pv::assistant,
    },
];

/// Liczba widgetów w katalogu (do nagłówka galerii).
pub fn widget_count() -> usize {
    DEMOS.len()
}
