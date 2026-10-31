//! Interaktywne przykłady widgetów w dokumentacji.
//!
//! Każdy wpis z [`Kind`] ma prawdziwy, działający przykład: kliknięcia i klawisze
//! zmieniają stan, który rysowany jest przez widgety z `libluna-widgets`. Wpisy
//! [`Kind::Static`] są nadal rysowane na żywo z [`crate::gallery_previews`] — po
//! prostu nie mają stanu, który można zmienić.

use libluna_core::style::weight;
use libluna_core::{ButtonState, KeyEvent, Point, Rect, Theme};
use libluna_render::TextEngine;
use libluna_widgets::{
    Assistant, Banner, BannerKind, Button, ButtonVariant, Calendar, CheckButton, ColorButton,
    ColorChooser, Column, ColumnView, ControlsSide, DropDown, EmojiChooser, Entry, Expander,
    FlowBox, HeaderBar, InfoBar, InfoBarKind, Item, ItemView, LevelBar, LinkButton, MediaControls,
    Menu, MenuButton, MenuEntry, NavEntry, NavList, NavigationRail, NavigationSplitView,
    NavigationView, Orientation, OverlaySplitView, Pager, PaintCtx, Paned, PasswordEntry, PathBar,
    PopoverMenuBar, ProgressBar, RadioGroup, SearchBar, SearchEntry, Sidebar, Slider, SpinButton,
    Stack, StatusIcon, StatusPage, Stepper, StepperSide, Switch, TabBar, TextInput, TextView,
    TitleAlign, ToggleButton, ToggleGroup, ViewMode, ViewSwitcher, WindowControl,
};

/// Co robi dany widget w trybie interaktywnym.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Button,
    ButtonsExtra,
    SplitButton,
    MenuButton,
    Entry,
    Password,
    Search,
    SearchBar,
    TextView,
    Slider,
    Switch,
    Check,
    Radio,
    DropDown,
    ToggleGroup,
    Calendar,
    ColorChooser,
    Emoji,
    Spin,
    Stepper,
    ViewSwitcher,
    NavList,
    NavRail,
    TabBar,
    PathBar,
    Pager,
    NavigationView,
    NavigationSplit,
    OverlaySplit,
    Menu,
    PopoverMenuBar,
    Expander,
    FlowBox,
    Paned,
    Stack,
    ItemView,
    ColumnView,
    Media,
    Progress,
    InfoBar,
    Banner,
    StatusPage,
    HeaderBar,
    Assistant,
    /// Tylko podgląd (prawdziwy widget, ale bez stanu).
    Static,
}

/// Mapowanie nazwy wpisu katalogu na zachowanie interaktywne.
pub fn kind(name: &str) -> Kind {
    match name {
        "Button" => Kind::Button,
        "ToggleButton / LinkButton / ColorButton" => Kind::ButtonsExtra,
        "SplitButton" => Kind::SplitButton,
        "MenuButton" => Kind::MenuButton,
        "Entry" => Kind::Entry,
        "PasswordEntry" => Kind::Password,
        "SearchEntry" => Kind::Search,
        "SearchBar" => Kind::SearchBar,
        "TextView" => Kind::TextView,
        "Slider" => Kind::Slider,
        "Switch" => Kind::Switch,
        "CheckButton" => Kind::Check,
        "RadioGroup" => Kind::Radio,
        "DropDown" => Kind::DropDown,
        "ToggleGroup" => Kind::ToggleGroup,
        "Calendar" => Kind::Calendar,
        "ColorChooser" => Kind::ColorChooser,
        "EmojiChooser" => Kind::Emoji,
        "SpinButton" => Kind::Spin,
        "Stepper" => Kind::Stepper,
        "ViewSwitcher" => Kind::ViewSwitcher,
        "NavList" => Kind::NavList,
        "NavigationRail" => Kind::NavRail,
        "TabBar" => Kind::TabBar,
        "PathBar" => Kind::PathBar,
        "Pager" => Kind::Pager,
        "NavigationView" => Kind::NavigationView,
        "NavigationSplitView" => Kind::NavigationSplit,
        "OverlaySplitView" => Kind::OverlaySplit,
        "Menu" => Kind::Menu,
        "PopoverMenuBar" => Kind::PopoverMenuBar,
        "Expander" => Kind::Expander,
        "FlowBox" => Kind::FlowBox,
        "Paned" => Kind::Paned,
        "Stack" => Kind::Stack,
        "ItemView" => Kind::ItemView,
        "ColumnView" => Kind::ColumnView,
        "MediaControls" => Kind::Media,
        "ProgressBar / LevelBar" => Kind::Progress,
        "InfoBar" => Kind::InfoBar,
        "Banner" => Kind::Banner,
        "StatusPage" => Kind::StatusPage,
        "HeaderBar" => Kind::HeaderBar,
        "Assistant" => Kind::Assistant,
        _ => Kind::Static,
    }
}

/// Czy widget ma działający, klikalny przykład.
pub fn supports(name: &str) -> bool {
    kind(name) != Kind::Static
}

/// Które pole tekstowe ma fokus w przykładzie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    None,
    Entry,
    Password,
    Search,
    SearchBar,
    Note,
}

/// Stan interaktywnych przykładów (wspólny dla wszystkich widgetów).
#[derive(Debug, Clone)]
pub struct ExampleState {
    pub switch_on: bool,
    pub checked: bool,
    pub toggle_on: bool,
    pub slider: f64,
    pub level: f64,
    pub radio: usize,
    pub view: usize,
    pub tab: usize,
    pub drop_selected: usize,
    pub drop_open: bool,
    pub group: [bool; 3],
    pub expanded: bool,
    pub spin: f64,
    pub step: i64,
    pub media_playing: bool,
    pub media_pos: f64,
    pub color: usize,
    pub calendar_year: i32,
    pub calendar_month: u32,
    pub calendar_day: u32,
    pub pager: Pager,
    pub clicks: u32,
    pub last: String,
    pub entry: Entry,
    pub password: PasswordEntry,
    pub search: SearchEntry,
    pub focus: Focus,
    /// Trwa przeciąganie suwaka / szukanie w odtwarzaczu.
    pub dragging: bool,

    // ── Rozszerzone przykłady ──
    pub split_open: bool,
    pub split_clicks: u32,
    pub search_bar: bool,
    pub bar_entry: SearchEntry,
    pub note: TextView,
    pub emoji: usize,
    pub nav_selected: Option<usize>,
    pub rail_selected: usize,
    pub path: PathBar,
    pub menu_selected: Option<usize>,
    pub menu_bar: PopoverMenuBar,
    pub flow_selected: Option<usize>,
    pub paned: Paned,
    pub stack: Stack,
    pub stack_page: usize,
    pub nav_view: NavigationView,
    pub split_sidebar: bool,
    pub overlay_open: bool,
    pub item_view: ItemView,
    pub column_selected: Option<usize>,
    pub info_visible: bool,
    pub banner_visible: bool,
    pub status_clicks: u32,
    pub header_variant: usize,
    pub header_hover: Option<WindowControl>,
    pub header_view: usize,
    pub assistant: Assistant,
}

impl Default for ExampleState {
    fn default() -> Self {
        let mut paned = Paned::new(Orientation::Horizontal);
        paned.set_position(0.4);
        let mut stack = Stack::with_count(3);
        stack.set_visible_index(0);
        let mut item_view = ItemView::new(vec![
            Item::folder("Projekty"),
            Item::new("Raport").detail("PDF"),
            Item::new("Notatki").detail("TXT"),
            Item::new("Budżet").detail("XLSX"),
        ]);
        item_view.set_mode(ViewMode::List);
        item_view.set_selected(Some(1));
        let mut note = TextView::new("Napisz notatkę…");
        note.insert_str("Notatka\nDruga linia");
        Self {
            switch_on: true,
            checked: false,
            toggle_on: true,
            slider: 0.45,
            level: 0.65,
            radio: 0,
            view: 1,
            tab: 0,
            drop_selected: 0,
            drop_open: false,
            group: [true, false, false],
            expanded: true,
            spin: 5.0,
            step: 3,
            media_playing: true,
            media_pos: 0.25,
            color: 0,
            calendar_year: 2026,
            calendar_month: 10,
            calendar_day: 8,
            pager: Pager::new(5),
            clicks: 0,
            last: String::new(),
            entry: Entry::new("Wpisz tekst…").clearable(true),
            password: PasswordEntry::new("Hasło"),
            search: SearchEntry::new("Szukaj…"),
            focus: Focus::None,
            dragging: false,
            split_open: false,
            split_clicks: 0,
            search_bar: true,
            bar_entry: SearchEntry::new("Szukaj w dokumentacji…"),
            note,
            emoji: 3,
            nav_selected: Some(1),
            rail_selected: 1,
            path: PathBar::new(["Dom", "Dokumenty", "Projekty"]),
            menu_selected: None,
            menu_bar: PopoverMenuBar::new(["Plik", "Edycja", "Widok"]),
            flow_selected: Some(2),
            paned,
            stack,
            stack_page: 0,
            nav_view: NavigationView::new(),
            split_sidebar: true,
            overlay_open: false,
            item_view,
            column_selected: Some(1),
            info_visible: true,
            banner_visible: true,
            status_clicks: 0,
            header_variant: 0,
            header_hover: None,
            header_view: 0,
            assistant: Assistant::new(["Powitanie", "Szczegóły", "Gotowe"]),
        }
    }
}

impl ExampleState {
    /// Reset stanu po wybraniu innego widgetu (żeby przykłady startowały czysto).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Krótki opis ostatniej akcji — rysowany pod podglądem.
    pub fn status(&self) -> String {
        self.last.clone()
    }
}

// ── Rysowanie ───────────────────────────────────────────────────────────────

/// Rysuje interaktywny przykład. Zwraca `false` dla [`Kind::Static`].
pub fn paint(name: &str, ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) -> bool {
    let kind = kind(name);
    if kind == Kind::Static {
        return false;
    }
    let theme = *ctx.theme;
    match kind {
        Kind::Button => paint_buttons(ctx, rect, st),
        Kind::ButtonsExtra => paint_buttons_extra(ctx, rect, st),
        Kind::SplitButton => paint_split_button(ctx, rect, st),
        Kind::MenuButton => paint_menu_button(ctx, rect, st),
        Kind::Entry => {
            st.entry
                .paint(ctx, field_rect(rect, st.entry.height(&theme)));
        }
        Kind::Password => {
            st.password
                .paint(ctx, field_rect(rect, st.password.height(&theme)));
            if !st.password.text().is_empty() {
                hint(ctx, rect, "Kliknij pole i pisz — tekst jest maskowany");
            }
        }
        Kind::Search => {
            st.search
                .paint(ctx, field_rect(rect, st.search.height(&theme)));
        }
        Kind::SearchBar => paint_search_bar(ctx, rect, st),
        Kind::TextView => {
            let area = inner(rect);
            st.note.paint(ctx, area);
            hint(
                ctx,
                rect,
                if st.focus == Focus::Note {
                    "Pisanie działa — Enter dodaje linię"
                } else {
                    "Kliknij pole i pisz — Enter dodaje linię"
                },
            );
        }
        Kind::Slider => paint_slider(ctx, rect, st.slider),
        Kind::Switch => paint_switch(ctx, rect, st.switch_on),
        Kind::Check => paint_check(ctx, rect, st.checked),
        Kind::Radio => paint_radio(ctx, rect, st.radio),
        Kind::DropDown => paint_dropdown(ctx, rect, st),
        Kind::ToggleGroup => paint_toggle_group(ctx, rect, st),
        Kind::Calendar => paint_calendar(ctx, rect, st),
        Kind::ColorChooser => paint_color_chooser(ctx, rect, st),
        Kind::Emoji => paint_emoji(ctx, rect, st),
        Kind::Spin => paint_spin(ctx, rect, st.spin),
        Kind::Stepper => paint_stepper(ctx, rect, st.step),
        Kind::ViewSwitcher => paint_view_switcher(ctx, rect, st),
        Kind::NavList => paint_nav_list(ctx, rect, st),
        Kind::NavRail => paint_nav_rail(ctx, rect, st),
        Kind::TabBar => paint_tab_bar(ctx, rect, st),
        Kind::PathBar => paint_path_bar(ctx, rect, st),
        Kind::Pager => paint_pager(ctx, rect, st),
        Kind::NavigationView => paint_navigation_view(ctx, rect, st),
        Kind::NavigationSplit => paint_navigation_split(ctx, rect, st),
        Kind::OverlaySplit => paint_overlay_split(ctx, rect, st),
        Kind::Menu => paint_menu(ctx, rect, st),
        Kind::PopoverMenuBar => paint_popover_menu_bar(ctx, rect, st),
        Kind::Expander => paint_expander(ctx, rect, st.expanded),
        Kind::FlowBox => paint_flow_box(ctx, rect, st),
        Kind::Paned => paint_paned(ctx, rect, st),
        Kind::Stack => paint_stack(ctx, rect, st),
        Kind::ItemView => paint_item_view(ctx, rect, st),
        Kind::ColumnView => paint_column_view(ctx, rect, st),
        Kind::Media => paint_media(ctx, rect, st),
        Kind::Progress => paint_progress(ctx, rect, st.level),
        Kind::InfoBar => paint_info_bar(ctx, rect, st),
        Kind::Banner => paint_banner(ctx, rect, st),
        Kind::StatusPage => paint_status_page(ctx, rect, st),
        Kind::HeaderBar => paint_header_bar(ctx, rect, st),
        Kind::Assistant => paint_assistant(ctx, rect, st),
        Kind::Static => return false,
    }
    true
}

fn paint_buttons(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let h = theme.metrics.button.height;
    let buttons = [
        ("Default", ButtonVariant::Default),
        ("Suggested", ButtonVariant::Suggested),
        ("Danger", ButtonVariant::Destructive),
    ];
    let gap = 10.0;
    let widths: Vec<f64> = buttons
        .iter()
        .map(|(label, _)| Button::new(*label).width(ctx.text, &theme))
        .collect();
    let total: f64 = widths.iter().sum::<f64>() + gap * (buttons.len() as f64 - 1.0);
    let mut x = rect.center().x - total / 2.0;
    let y = rect.center().y - h / 2.0 - 16.0;
    for ((label, variant), width) in buttons.iter().zip(widths) {
        let r = Rect::from_xywh(x, y, width, h);
        Button::new(*label).variant(*variant).paint(ctx, r);
        x += width + gap;
    }
    let disabled = Rect::from_xywh(rect.center().x - 70.0, y + h + 14.0, 140.0, h);
    Button::new("Wyłączony").enabled(false).paint(ctx, disabled);
    hint(
        ctx,
        rect,
        &if st.last.is_empty() {
            "Kliknij przycisk — wynik pojawi się tutaj".to_owned()
        } else {
            format!("{} (kliknięć: {})", st.last, st.clicks)
        },
    );
}

fn paint_buttons_extra(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let h = 32.0;
    let y = rect.center().y - h / 2.0;
    let x = rect.center().x - 210.0;
    ToggleButton::new("Pogrubienie")
        .active(st.toggle_on)
        .paint(ctx, Rect::from_xywh(x, y, 120.0, h));
    LinkButton::new("Dokumentacja").paint(ctx, Rect::from_xywh(x + 132.0, y, 130.0, h));
    let palette = ColorChooser::default_palette();
    ColorButton::new(palette[st.color]).paint(ctx, Rect::from_xywh(x + 280.0, y + 2.0, 30.0, 28.0));
    hint(ctx, rect, "Przełącz „Pogrubienie”, kliknij kolor albo link");
}

fn paint_split_button(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let split = libluna_widgets::SplitButton::new("Eksportuj");
    let h = split.height(&theme);
    let r = Rect::from_xywh(rect.center().x - 150.0, rect.y() + 36.0, 300.0, h);
    split.paint(ctx, r);
    if st.split_open {
        let menu = demo_menu();
        let panel = menu.place(
            Rect::from_xywh(r.right() - 80.0, r.bottom() + 4.0, 80.0, h),
            200.0,
            rect,
            &theme,
        );
        menu.paint(ctx, panel);
    }
    let hint_text = if st.split_clicks == 0 {
        "Kliknij główną część lub strzałkę".to_owned()
    } else {
        format!(
            "Akcja główna: {} · strzałka: {}",
            st.split_clicks,
            if st.split_open {
                "otwarta"
            } else {
                "zamknięta"
            }
        )
    };
    hint(ctx, rect, &hint_text);
}

fn paint_menu_button(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let h = theme.metrics.button.height;
    let x = rect.center().x - 70.0;
    let y = rect.y() + 30.0;
    let mut button = MenuButton::new("Opcje");
    button.set_open(st.drop_open);
    button.paint(ctx, Rect::from_xywh(x, y, 140.0, h));
    if st.drop_open {
        let menu = demo_menu();
        let panel = menu.place(
            Rect::from_xywh(x, y + h + 4.0, 140.0, h),
            160.0,
            rect,
            &theme,
        );
        menu.paint(ctx, panel);
    }
    hint(
        ctx,
        rect,
        if st.drop_open {
            "Menu otwarte — kliknij przycisk, aby zamknąć"
        } else {
            "Kliknij „Opcje”, aby rozwinąć menu"
        },
    );
}

fn paint_search_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let bar = SearchBar::with_mode(st.search_bar);
    let h = bar.height(&theme);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 24.0,
        (rect.width() - 24.0).max(0.0),
        h,
    );
    bar.paint(ctx, area);
    let entry = bar.entry_rect(area, &theme);
    if entry.height() > 0.0 {
        st.bar_entry.paint(ctx, entry);
    }
    let (toggle, label) = search_bar_toggle(rect, ctx.text, &theme, st.search_bar);
    Button::new(label).paint(ctx, toggle);
    hint(
        ctx,
        rect,
        if st.search_bar {
            "Pisz w polu — „Ukryj pasek” schowa wyszukiwanie"
        } else {
            "Tryb zwykły — kliknij „Pokaż pasek”"
        },
    );
}

fn search_bar_toggle(
    rect: Rect,
    text: &TextEngine,
    theme: &Theme,
    mode: bool,
) -> (Rect, &'static str) {
    let label = if mode { "Ukryj pasek" } else { "Pokaż pasek" };
    let w = Button::new(label).width(text, theme);
    let h = theme.metrics.button.height;
    (
        Rect::from_xywh(rect.center().x - w / 2.0, rect.bottom() - 72.0, w, h),
        label,
    )
}

fn paint_slider(ctx: &mut PaintCtx, rect: Rect, value: f64) {
    let theme = *ctx.theme;
    let track = Rect::from_xywh(
        rect.x() + 40.0,
        rect.center().y - theme.metrics.slider.track_height / 2.0,
        (rect.width() - 80.0).max(40.0),
        theme.metrics.slider.track_height,
    );
    Slider::new(value).paint(ctx, track);
    hint(ctx, rect, &format!("Wartość: {:.0}%", value * 100.0));
}

fn paint_switch(ctx: &mut PaintCtx, rect: Rect, on: bool) {
    let theme = *ctx.theme;
    let (w, h) = Switch::new(on).size(&theme);
    let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
    Switch::new(on).paint(ctx, r);
    hint(
        ctx,
        rect,
        if on {
            "Włączone — kliknij, aby wyłączyć"
        } else {
            "Wyłączone — kliknij, aby włączyć"
        },
    );
}

fn paint_check(ctx: &mut PaintCtx, rect: Rect, checked: bool) {
    let mut check = CheckButton::new("Pokaż ukryte pliki");
    check.set_checked(checked);
    let r = Rect::from_xywh(rect.center().x - 150.0, rect.center().y - 18.0, 300.0, 36.0);
    check.paint(ctx, r);
    hint(
        ctx,
        rect,
        if checked {
            "Zaznaczone — kliknij, aby odznaczyć"
        } else {
            "Odznaczone — kliknij, aby zaznaczyć"
        },
    );
}

fn paint_radio(ctx: &mut PaintCtx, rect: Rect, selected: usize) {
    let theme = *ctx.theme;
    let mut radio = RadioGroup::new(["Luna", "Selene", "libLuna"]);
    radio.set_selected(selected);
    let row_h = theme.metrics.radio.row_h;
    let r = Rect::from_xywh(
        rect.center().x - 150.0,
        rect.center().y - row_h * 3.0 / 2.0,
        260.0,
        row_h * 3.0,
    );
    radio.paint(ctx, r);
    hint(
        ctx,
        rect,
        &format!(
            "Wybrano: {}",
            ["Luna", "Selene", "libLuna"][selected.min(2)]
        ),
    );
}

fn paint_dropdown(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let options = ["Luna", "Selene", "libLuna", "PaganOS"];
    let mut drop = DropDown::new(options);
    drop.set_selected(st.drop_selected);
    let h = drop.height(&theme);
    let w = drop.width(ctx.text, &theme).max(200.0);
    let x = rect.center().x - w / 2.0;
    let y = rect.y() + 32.0;
    drop.paint(ctx, Rect::from_xywh(x, y, w, h));
    if st.drop_open {
        let menu = drop.menu();
        let panel = menu.place(Rect::from_xywh(x, y + h + 4.0, w, h), w, rect, &theme);
        menu.paint(ctx, panel);
    }
    hint(
        ctx,
        rect,
        &format!(
            "Wybrano: {} · kliknij, aby rozwinąć",
            options[st.drop_selected]
        ),
    );
}

fn paint_toggle_group(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut group = ToggleGroup::new(["Pogrubienie", "Kursywa", "Podkreślenie"]);
    for (i, on) in st.group.iter().enumerate() {
        group.set_active(i, *on);
    }
    let r = Rect::from_xywh(rect.center().x - 200.0, rect.center().y - 16.0, 400.0, 32.0);
    group.paint(ctx, r);
    hint(
        ctx,
        rect,
        "Kliknij segmenty — mogą być aktywne jednocześnie",
    );
}

fn paint_calendar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut cal = Calendar::new(st.calendar_year, st.calendar_month);
    cal.set_selected(st.calendar_day);
    let w = 320.0_f64.min(rect.width() - 40.0);
    let h = (rect.height() - 60.0).clamp(180.0, 300.0);
    let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.y() + 24.0, w, h);
    let title = format!("{} {}", month_name(st.calendar_month), st.calendar_year);
    cal.paint(ctx, r, &title, &weekdays());
    hint(ctx, rect, "Kliknij dzień, strzałki ‹ › zmieniają miesiąc");
}

fn paint_color_chooser(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let palette = ColorChooser::default_palette();
    let mut chooser = ColorChooser::new(palette);
    chooser.set_columns(4);
    chooser.set_selected(st.color);
    let w = 220.0;
    let h = chooser.height(w, &theme);
    let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
    chooser.paint(ctx, r);
    hint(ctx, rect, &format!("Wybrany kolor: {}", st.color + 1));
}

fn paint_emoji(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut emoji = EmojiChooser::new(EmojiChooser::default_palette());
    emoji.set_columns(9);
    emoji.set_selected(st.emoji);
    let w = (rect.width() - 24.0).min(320.0);
    let h = emoji.height(w, &theme);
    emoji.paint(
        ctx,
        Rect::from_xywh(rect.x() + 12.0, rect.center().y - h / 2.0, w, h),
    );
    let selected = emoji.selected_emoji().unwrap_or("");
    hint(ctx, rect, &format!("Wybrany symbol: {selected}"));
}

fn paint_spin(ctx: &mut PaintCtx, rect: Rect, value: f64) {
    let theme = *ctx.theme;
    let mut spin = SpinButton::new(value, 0.0, 10.0).step(1.0).digits(0);
    spin.set_value(value);
    let h = spin.height(&theme);
    let w = 140.0;
    let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
    spin.paint(ctx, r);
    hint(ctx, rect, &format!("Wartość: {value:.0} (0–10)"));
}

fn paint_stepper(ctx: &mut PaintCtx, rect: Rect, value: i64) {
    let theme = *ctx.theme;
    let mut stepper = Stepper::new(value, 1, 10);
    stepper.set_value(value);
    let h = stepper.height(&theme);
    let w = stepper.width(&theme);
    let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
    stepper.paint(ctx, r);
    hint(ctx, rect, &format!("Wartość: {value} (1–10)"));
}

fn paint_view_switcher(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut switcher = ViewSwitcher::new(["Lista", "Siatka", "Karuzela"]);
    switcher.set_selected(st.view);
    let r = centered_rect(rect, 300.0, 32.0, 0.42);
    switcher.paint(ctx, r);
    hint(
        ctx,
        rect,
        &format!(
            "Wybrany widok: {}",
            ["Lista", "Siatka", "Karuzela"][st.view]
        ),
    );
}

fn paint_nav_list(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut nav = demo_nav();
    nav.set_selected(st.nav_selected);
    let r = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        220.0,
        (rect.height() - 16.0).max(0.0),
    );
    nav.paint(ctx, r);
    let selected = nav.selected_label().unwrap_or("—");
    hint(ctx, rect, &format!("Wybrano: {selected}"));
}

fn paint_nav_rail(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut rail = NavigationRail::new(["Dom", "Pliki", "Media", "Ustawienia"]);
    rail.set_selected(st.rail_selected);
    let w = rail.width(&theme);
    rail.paint(
        ctx,
        Rect::from_xywh(rect.x() + 16.0, rect.y() + 8.0, w, rect.height() - 16.0),
    );
    hint(
        ctx,
        rect,
        &format!("Wybrano: {}", rail.selected_label().unwrap_or("—")),
    );
}

fn paint_tab_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut tabs = TabBar::new(["Ogólne", "Wygląd", "Zaawansowane"]);
    tabs.set_selected(st.tab);
    let r = Rect::from_xywh(
        rect.x() + 20.0,
        rect.y() + 40.0,
        (rect.width() - 40.0).max(0.0),
        36.0,
    );
    tabs.paint(ctx, r);
    hint(
        ctx,
        rect,
        &format!(
            "Aktywna zakładka: {}",
            ["Ogólne", "Wygląd", "Zaawansowane"][st.tab]
        ),
    );
}

fn paint_path_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let h = st.path.height(&theme);
    let r = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 40.0,
        (rect.width() - 24.0).max(0.0),
        h,
    );
    st.path.paint(ctx, r);
    hint(ctx, rect, "Kliknij okruszek, aby skrócić ścieżkę");
}

fn paint_pager(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let strip = Rect::from_xywh(rect.center().x - 120.0, rect.center().y - 8.0, 240.0, 16.0);
    st.pager.paint(ctx, strip);
    hint(
        ctx,
        rect,
        &format!("Strona {} z {}", st.pager.active() + 1, st.pager.count()),
    );
}

fn paint_navigation_view(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    st.nav_view.paint(ctx, area);
    let content = st.nav_view.content_rect(area, &theme);
    ctx.label_center(
        content.center().x,
        content.center().y,
        &format!("Poziom {}", st.nav_view.depth()),
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    hint(
        ctx,
        rect,
        if st.nav_view.can_pop() {
            "Kliknij treść, aby wejść głębiej; „‹” wraca"
        } else {
            "Kliknij treść, aby wejść głębiej"
        },
    );
}

fn paint_navigation_split(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut view = NavigationSplitView::new().sidebar_width(140.0);
    view.set_show_sidebar(st.split_sidebar);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    if let Some(side) = view.sidebar_rect(area) {
        Sidebar::new().paint(ctx, side);
        let mut nav = demo_nav();
        nav.set_selected(Some(0));
        nav.paint(
            ctx,
            Rect::from_xywh(
                side.x() + 6.0,
                side.y() + 6.0,
                (side.width() - 12.0).max(0.0),
                (side.height() - 12.0).max(0.0),
            ),
        );
    }
    let content = view.content_rect(area);
    ctx.scene.fill_rect(content, theme.colors.view_bg);
    ctx.label_center(
        content.center().x,
        content.center().y,
        "Treść",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
    if let Some(toggle) = split_toggle(content) {
        paint_toggle_button(ctx, toggle, st.split_sidebar);
    }
    hint(
        ctx,
        rect,
        "Kliknij przycisk menu, aby schować lub pokazać panel boczny",
    );
}

fn paint_overlay_split(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut view = OverlaySplitView::new();
    view.set_open(st.overlay_open);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 6.0,
        rect.width() - 24.0,
        rect.height() - 12.0,
    );
    ctx.scene.fill_rect(area, theme.colors.view_bg);
    if st.overlay_open {
        view.paint_scrim(ctx, area);
        Sidebar::new().paint(ctx, view.sidebar_rect(area));
    }
    if let Some(toggle) = split_toggle(area) {
        paint_toggle_button(ctx, toggle, st.overlay_open);
    }
    hint(
        ctx,
        rect,
        if st.overlay_open {
            "Kliknij tło, aby zamknąć nakładany panel"
        } else {
            "Kliknij przycisk menu, aby otworzyć nakładany panel"
        },
    );
}

fn paint_menu(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let menu = demo_menu();
    let w = (rect.width() - 24.0).min(230.0);
    let h = menu.height(&theme);
    let panel = Rect::from_xywh(rect.x() + 12.0, rect.center().y - h / 2.0, w, h);
    menu.paint(ctx, panel);
    let selected = st
        .menu_selected
        .and_then(|i| menu.entries().get(i))
        .and_then(|entry| match entry {
            MenuEntry::Action { label, .. } => Some(label.clone()),
            _ => None,
        });
    hint(
        ctx,
        rect,
        &match selected {
            Some(label) => format!("Wybrano: {label}"),
            None => "Kliknij pozycję menu".to_owned(),
        },
    );
}

fn paint_popover_menu_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let h = theme.metrics.button.height;
    let bar = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 24.0,
        (rect.width() - 24.0).max(0.0),
        h,
    );
    st.menu_bar.paint(ctx, bar);
    if let Some(i) = st.menu_bar.open_index() {
        let buttons = st.menu_bar.buttons(bar, ctx.text, &theme);
        if let Some(anchor) = buttons.get(i).copied() {
            let menu = demo_menu();
            let panel = menu.place(anchor, 200.0, rect, &theme);
            menu.paint(ctx, panel);
        }
    }
    hint(
        ctx,
        rect,
        &match st.menu_bar.open_index() {
            Some(i) => format!("Otwarte menu: {}", ["Plik", "Edycja", "Widok"][i]),
            None => "Kliknij Plik / Edycję / Widok".to_owned(),
        },
    );
}

fn paint_expander(ctx: &mut PaintCtx, rect: Rect, expanded: bool) {
    let expander = Expander::new("Ustawienia zaawansowane").expanded(expanded);
    let r = Rect::from_xywh(rect.x() + 20.0, rect.y() + 24.0, rect.width() - 40.0, 160.0);
    expander.paint(ctx, r);
    if expanded {
        let content = expander.content_rect(r, ctx.theme);
        ctx.label_left(
            content.x() + 8.0,
            content.y() + 16.0,
            "Ukryta treść rozwinięta kliknięciem nagłówka.",
            ctx.theme.typography.caption.size,
            weight::NORMAL,
            ctx.theme.colors.dim_text,
        );
    }
    hint(ctx, rect, "Kliknij nagłówek, aby zwinąć lub rozwinąć");
}

fn paint_flow_box(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut flow = FlowBox::new(44.0);
    flow.set_count(10);
    flow.set_selected(st.flow_selected);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    flow.paint(ctx, area);
    for (i, cell) in flow.cells(area).iter().enumerate() {
        let bg = if st.flow_selected == Some(i) {
            theme.colors.accent.scale_alpha(0.22)
        } else {
            theme.colors.shade
        };
        ctx.scene.fill_rounded_rect(*cell, theme.radius.small, bg);
        ctx.label_center(
            cell.center().x,
            cell.center().y,
            &(i + 1).to_string(),
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.text,
        );
    }
    hint(
        ctx,
        rect,
        &format!(
            "Wybrany element: {}",
            st.flow_selected.map(|i| i + 1).unwrap_or(0)
        ),
    );
}

fn paint_paned(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    let (a, b) = st.paned.panes(area, &theme);
    ctx.scene.fill_rect(a, theme.colors.shade);
    ctx.scene.fill_rect(b, theme.colors.card_bg);
    st.paned.paint(ctx, area);
    ctx.label_center(
        a.center().x,
        a.center().y,
        "Panel A",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
    ctx.label_center(
        b.center().x,
        b.center().y,
        "Panel B",
        theme.typography.body.size,
        weight::NORMAL,
        theme.colors.text,
    );
    hint(ctx, rect, "Przeciągnij separator, aby zmienić podział");
}

fn paint_stack(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 8.0,
        rect.width() - 24.0,
        rect.height() - 16.0,
    );
    let (outgoing, incoming) = st.stack.frames(area);
    ctx.scene.fill_rounded_rect(
        outgoing.rect,
        theme.radius.small,
        theme.colors.shade.scale_alpha(outgoing.alpha as f32),
    );
    ctx.scene.fill_rounded_rect(
        incoming.rect,
        theme.radius.small,
        theme
            .colors
            .accent
            .scale_alpha(0.18 * incoming.alpha as f32),
    );
    ctx.label_center(
        incoming.rect.center().x,
        incoming.rect.center().y,
        &format!("Strona {}", st.stack_page + 1),
        theme.typography.heading.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    hint(ctx, rect, "Kliknij, aby przejść do kolejnej strony");
}

fn paint_item_view(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    st.item_view.paint(
        ctx,
        Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 6.0,
            rect.width() - 24.0,
            rect.height() - 12.0,
        ),
    );
    hint(
        ctx,
        rect,
        if st.item_view.selected_indices().is_empty() {
            "Kliknij element, aby go zaznaczyć"
        } else {
            "Kliknij element, aby przełączyć zaznaczenie"
        },
    );
}

fn paint_column_view(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let mut table = demo_table();
    table.set_selected(st.column_selected);
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 4.0,
        rect.width() - 24.0,
        rect.height() - 8.0,
    );
    table.paint(ctx, area);
    for (i, name) in ["Projekty", "Raport.pdf", "Notatki.txt", "Budżet.xlsx"]
        .iter()
        .enumerate()
    {
        let cell = table.cell_rect(area, &theme, i, 0);
        ctx.label_left(
            cell.x() + theme.spacing.md,
            cell.center().y,
            name,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.text,
        );
        let value = table.cell_rect(area, &theme, i, 1);
        ctx.label_left(
            value.x() + theme.spacing.md,
            value.center().y,
            ["12 MB", "340 KB", "2 KB", "18 KB"][i],
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    hint(
        ctx,
        rect,
        &match st.column_selected {
            Some(i) => format!("Wybrany wiersz: {}", i + 1),
            None => "Kliknij wiersz, aby go zaznaczyć".to_owned(),
        },
    );
}

fn paint_media(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let mut media = MediaControls::new("Noc nad jeziorem", "Luna Ensemble");
    media.set_playing(st.media_playing);
    media.set_position(st.media_pos);
    media.set_times("1:12", "4:48");
    let h = (media.height(ctx.theme) + 40.0).min(rect.height() - 40.0);
    let r = Rect::from_xywh(
        rect.x() + 20.0,
        rect.center().y - h / 2.0,
        rect.width() - 40.0,
        h,
    );
    media.paint(ctx, r);
    hint(
        ctx,
        rect,
        if st.media_playing {
            "Odtwarzanie — kliknij ⏸ lub pasek postępu"
        } else {
            "Pauza — kliknij ▶ lub pasek postępu"
        },
    );
}

fn paint_progress(ctx: &mut PaintCtx, rect: Rect, value: f64) {
    let w = (rect.width() - 80.0).clamp(120.0, 520.0);
    let x = rect.center().x - w / 2.0;
    let y = rect.center().y - 30.0;
    ProgressBar::new()
        .fraction(value)
        .text(format!("{:.0}%", value * 100.0))
        .paint(ctx, Rect::from_xywh(x, y, w, 10.0));
    LevelBar::new()
        .value(value)
        .segments(10)
        .paint(ctx, Rect::from_xywh(x, y + 44.0, w, 10.0));
    hint(ctx, rect, "Kliknij pasek, aby ustawić postęp");
}

fn paint_info_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    if st.info_visible {
        let bar = InfoBar::new("Mało miejsca na dysku")
            .kind(InfoBarKind::Warning)
            .action("Zarządzaj");
        let area = Rect::from_xywh(rect.x() + 12.0, rect.y() + 24.0, rect.width() - 24.0, 40.0);
        bar.paint(ctx, area);
        hint(ctx, rect, "Kliknij akcję lub ×, aby ukryć pasek");
    } else {
        centered_note(ctx, rect, "Pasek ukryty — kliknij, aby pokazać ponownie");
        hint(ctx, rect, "Kliknij gdziekolwiek, aby przywrócić InfoBar");
    }
}

fn paint_banner(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    if st.banner_visible {
        let banner = Banner::new("Nowa wersja libLuny")
            .kind(BannerKind::Info)
            .description("Wszystkie widgety są już dostępne.")
            .action("Zainstaluj");
        let area = Rect::from_xywh(rect.x() + 12.0, rect.y() + 14.0, rect.width() - 24.0, 56.0);
        banner.paint(ctx, area);
        hint(ctx, rect, "Kliknij „Zainstaluj”, aby zobaczyć akcję");
    } else {
        centered_note(ctx, rect, "Baner ukryty — kliknij, aby pokazać ponownie");
        hint(ctx, rect, "Kliknij gdziekolwiek, aby przywrócić Banner");
    }
}

fn paint_status_page(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let title = if st.status_clicks > 0 {
        "Wyczyszczono"
    } else {
        "Brak wyników"
    };
    let page = StatusPage::new(title)
        .icon(StatusIcon::Search)
        .description("Zmień frazę i spróbuj ponownie")
        .action("Wyczyść");
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 4.0,
        rect.width() - 24.0,
        rect.height() - 8.0,
    );
    page.paint(ctx, area);
    hint(
        ctx,
        rect,
        &format!("Kliknięcia akcji: {}", st.status_clicks),
    );
}

fn paint_header_bar(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let labels = HEADER_VARIANTS;
    let variant = st.header_variant.min(labels.len() - 1);
    let mut switcher = ViewSwitcher::new(labels);
    switcher.set_selected(variant);
    switcher.paint(ctx, header_switcher_rect(rect));

    let bar = header_variant_bar(variant);
    let bar_rect = header_demo_rect(rect, &theme);
    bar.paint(ctx, bar_rect, st.header_hover);

    if variant == 3 {
        let (burger, new, sw) = header_action_rects(bar_rect, &theme, ctx.text);
        MenuButton::burger().paint(ctx, burger);
        Button::new("Nowy").paint(ctx, new);
        let mut view = ViewSwitcher::new(["Lista", "Siatka"]);
        view.set_selected(st.header_view.min(1));
        view.paint(ctx, sw);
    }

    let hint_text = if st.last.is_empty() {
        format!("Wariant: {} · kliknij kontrolki okna", labels[variant])
    } else {
        st.last.clone()
    };
    hint(ctx, rect, &hint_text);
}

const HEADER_VARIANTS: [&str; 5] = [
    "Klasyczny",
    "Tytuł z lewej",
    "Lewe kontrolki",
    "Z akcjami",
    "Bez kontrolek",
];

fn header_switcher_rect(rect: Rect) -> Rect {
    let w = (rect.width() - 40.0).min(560.0);
    Rect::from_xywh(rect.center().x - w / 2.0, rect.y() + 40.0, w, 32.0)
}

fn header_demo_rect(rect: Rect, theme: &Theme) -> Rect {
    let h = theme.metrics.headerbar.height;
    Rect::from_xywh(rect.x() + 16.0, rect.y() + 96.0, rect.width() - 32.0, h)
}

fn header_variant_bar(variant: usize) -> HeaderBar {
    match variant {
        0 => HeaderBar::new()
            .title("Pliki")
            .controls(ControlsSide::Right),
        1 => HeaderBar::new()
            .title("Dokumentacja")
            .title_align(TitleAlign::Start)
            .controls(ControlsSide::Right),
        2 => HeaderBar::new()
            .title("Ustawienia")
            .controls(ControlsSide::Left),
        3 => HeaderBar::new().controls(ControlsSide::Right),
        _ => HeaderBar::new()
            .title("Pełny ekran")
            .controls(ControlsSide::None),
    }
}

fn header_action_rects(bar_rect: Rect, theme: &Theme, text: &TextEngine) -> (Rect, Rect, Rect) {
    let content = header_variant_bar(3)
        .controls_layout(bar_rect, theme)
        .content;
    let h = theme.metrics.button.height;
    let cy = bar_rect.center().y;
    let burger = Rect::from_xywh(content.x() + 2.0, cy - h / 2.0, h, h);
    let new_w = Button::new("Nowy").width(text, theme);
    let new = Rect::from_xywh(burger.right() + 8.0, cy - h / 2.0, new_w, h);
    let sw_w = 190.0_f64.min((content.width() - 80.0).max(120.0));
    let sw = Rect::from_xywh(content.right() - sw_w - 4.0, cy - 14.0, sw_w, 28.0);
    (burger, new, sw)
}

fn window_control_label(control: WindowControl) -> &'static str {
    match control {
        WindowControl::Minimize => "Minimalizuj",
        WindowControl::Maximize => "Maksymalizuj",
        WindowControl::Close => "Zamknij",
    }
}

fn paint_assistant(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let theme = *ctx.theme;
    let area = Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 4.0,
        rect.width() - 24.0,
        rect.height() - 8.0,
    );
    st.assistant.paint(ctx, area, "Anuluj", "Dalej");
    let content = st.assistant.content_rect(area, &theme);
    ctx.label_center(
        content.center().x,
        content.center().y,
        &format!(
            "Krok {}/{}",
            st.assistant.current() + 1,
            st.assistant.count()
        ),
        theme.typography.heading.size,
        weight::MEDIUM,
        theme.colors.text,
    );
    hint(ctx, rect, "Kliknij „Dalej” lub „Anuluj” w stopce");
}

// ── Obsługa zdarzeń ─────────────────────────────────────────────────────────

/// Kliknięcie w obszarze przykładu.
pub fn pressed(
    name: &str,
    rect: Rect,
    p: Point,
    theme: &Theme,
    text: &TextEngine,
    st: &mut ExampleState,
) -> bool {
    let kind = kind(name);
    match kind {
        Kind::Button => {
            let h = theme.metrics.button.height;
            let gap = 10.0;
            let labels = [
                ("Default", ButtonVariant::Default),
                ("Suggested", ButtonVariant::Suggested),
                ("Danger", ButtonVariant::Destructive),
            ];
            let widths: Vec<f64> = labels
                .iter()
                .map(|(label, _)| Button::new(*label).width(text, theme))
                .collect();
            let total: f64 = widths.iter().sum::<f64>() + gap * (labels.len() as f64 - 1.0);
            let mut x = rect.center().x - total / 2.0;
            let y = rect.center().y - h / 2.0 - 16.0;
            for ((label, _), width) in labels.iter().zip(widths) {
                if Rect::from_xywh(x, y, width, h).contains(p) {
                    st.clicks += 1;
                    st.last = format!("Kliknięto „{label}”");
                    return true;
                }
                x += width + gap;
            }
            true
        }
        Kind::ButtonsExtra => {
            let h = 32.0;
            let y = rect.center().y - h / 2.0;
            let x = rect.center().x - 210.0;
            if Rect::from_xywh(x, y, 120.0, h).contains(p) {
                st.toggle_on = !st.toggle_on;
                st.last = format!(
                    "Pogrubienie {}",
                    if st.toggle_on {
                        "włączone"
                    } else {
                        "wyłączone"
                    }
                );
            } else if Rect::from_xywh(x + 132.0, y, 130.0, h).contains(p) {
                st.last = "Otwarto dokumentację (demo)".to_owned();
            } else if Rect::from_xywh(x + 280.0, y + 2.0, 30.0, 28.0).contains(p) {
                st.color = (st.color + 1) % ColorChooser::default_palette().len();
                st.last = format!("Kolor {}", st.color + 1);
            }
            true
        }
        Kind::SplitButton => {
            let split = libluna_widgets::SplitButton::new("Eksportuj");
            let h = split.height(theme);
            let r = Rect::from_xywh(rect.center().x - 150.0, rect.y() + 36.0, 300.0, h);
            if split.arrow_at(r, theme, p) {
                st.split_open = !st.split_open;
                st.last = format!(
                    "Menu {}",
                    if st.split_open {
                        "otwarte"
                    } else {
                        "zamknięte"
                    }
                );
            } else if split.main_at(r, theme, p) {
                st.split_clicks += 1;
                st.split_open = false;
                st.last = format!("Eksport uruchomiony ({}×)", st.split_clicks);
            }
            true
        }
        Kind::MenuButton => {
            let h = theme.metrics.button.height;
            let x = rect.center().x - 70.0;
            let y = rect.y() + 30.0;
            if st.drop_open {
                let menu = demo_menu();
                let panel = menu.place(
                    Rect::from_xywh(x, y + h + 4.0, 140.0, h),
                    160.0,
                    rect,
                    theme,
                );
                if let Some(index) = menu.index_at(panel, p, theme) {
                    if let Some(MenuEntry::Action { label, .. }) = menu.entries().get(index) {
                        st.last = format!("Wybrano: {label}");
                    }
                }
                st.drop_open = false;
            } else {
                st.drop_open = true;
            }
            true
        }
        Kind::Entry => {
            let r = field_rect(rect, st.entry.height(theme));
            if r.contains(p) {
                if st.entry.clear_at(r, theme, p) {
                    st.entry.clear();
                }
                st.focus = Focus::Entry;
                st.entry.set_focused(true);
            }
            true
        }
        Kind::Password => {
            let r = field_rect(rect, st.password.height(theme));
            if r.contains(p) {
                st.password.click_at(r, theme, p);
                st.focus = Focus::Password;
                st.password.set_focused(true);
            }
            true
        }
        Kind::Search => {
            let r = field_rect(rect, st.search.height(theme));
            if r.contains(p) {
                if st.search.clear_at(r, theme, p) {
                    st.search.clear();
                }
                st.focus = Focus::Search;
                st.search.set_focused(true);
            }
            true
        }
        Kind::SearchBar => {
            let bar = SearchBar::with_mode(st.search_bar);
            let h = bar.height(theme);
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 24.0,
                (rect.width() - 24.0).max(0.0),
                h,
            );
            let (toggle, _) = search_bar_toggle(rect, text, theme, st.search_bar);
            if toggle.contains(p) {
                st.search_bar = !st.search_bar;
                st.bar_entry.set_focused(st.search_bar);
                st.focus = if st.search_bar {
                    Focus::SearchBar
                } else {
                    Focus::None
                };
                st.last = if st.search_bar {
                    "Włączono tryb wyszukiwania".to_owned()
                } else {
                    "Wyłączono tryb wyszukiwania".to_owned()
                };
            } else if bar.entry_rect(area, theme).contains(p) {
                st.focus = Focus::SearchBar;
                st.bar_entry.set_focused(true);
            }
            true
        }
        Kind::TextView => {
            if inner(rect).contains(p) {
                st.focus = Focus::Note;
                st.note.set_focused(true);
            }
            true
        }
        Kind::Slider => {
            let track = slider_track(theme, rect);
            if Slider::new(0.0).hit(track, p, theme) {
                st.slider = Slider::new(0.0).value_at(track, p);
                st.dragging = true;
                st.last = format!("{:.0}%", st.slider * 100.0);
            }
            true
        }
        Kind::Switch => {
            st.switch_on = !st.switch_on;
            true
        }
        Kind::Check => {
            st.checked = !st.checked;
            true
        }
        Kind::Radio => {
            let mut radio = RadioGroup::new(["Luna", "Selene", "libLuna"]);
            let row_h = theme.metrics.radio.row_h;
            let r = Rect::from_xywh(
                rect.center().x - 150.0,
                rect.center().y - row_h * 3.0 / 2.0,
                260.0,
                row_h * 3.0,
            );
            if radio.click_at(r, p, theme) {
                st.radio = radio.selected();
            }
            true
        }
        Kind::DropDown => {
            if st.drop_open {
                let options = ["Luna", "Selene", "libLuna", "PaganOS"];
                let menu = DropDown::new(options).menu();
                let w = 220.0;
                let h = theme.metrics.dropdown.height;
                let x = rect.center().x - w / 2.0;
                let y = rect.y() + 32.0;
                let panel = menu.place(Rect::from_xywh(x, y + h + 4.0, w, h), w, rect, theme);
                if let Some(index) = menu.index_at(panel, p, theme) {
                    if index < options.len() {
                        st.drop_selected = index;
                        st.last = format!("Wybrano {}", options[index]);
                    }
                    st.drop_open = false;
                } else if !Rect::from_xywh(x, y, w, h).contains(p) {
                    st.drop_open = false;
                }
            } else {
                st.drop_open = true;
            }
            true
        }
        Kind::ToggleGroup => {
            let mut group = ToggleGroup::new(["Pogrubienie", "Kursywa", "Podkreślenie"]);
            for (i, on) in st.group.iter().enumerate() {
                group.set_active(i, *on);
            }
            let r = Rect::from_xywh(rect.center().x - 200.0, rect.center().y - 16.0, 400.0, 32.0);
            if let Some(index) = group.click_at(r, p) {
                st.group[index] = !st.group[index];
            }
            true
        }
        Kind::Calendar => {
            let mut cal = Calendar::new(st.calendar_year, st.calendar_month);
            let w = 320.0_f64.min(rect.width() - 40.0);
            let h = (rect.height() - 60.0).clamp(180.0, 300.0);
            let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.y() + 24.0, w, h);
            let title_h = cal.title_height(theme);
            let prev = Rect::from_xywh(r.right() - 64.0, r.y(), 24.0, title_h);
            let next = Rect::from_xywh(r.right() - 32.0, r.y(), 24.0, title_h);
            if prev.contains(p) {
                cal.prev_month();
                st.calendar_year = cal.year();
                st.calendar_month = cal.month();
                st.calendar_day = cal.selected();
            } else if next.contains(p) {
                cal.next_month();
                st.calendar_year = cal.year();
                st.calendar_month = cal.month();
                st.calendar_day = cal.selected();
            } else if let Some(day) = cal.day_at(r, theme, p) {
                st.calendar_day = day;
            }
            true
        }
        Kind::ColorChooser => {
            let palette = ColorChooser::default_palette();
            let mut chooser = ColorChooser::new(palette);
            chooser.set_columns(4);
            let w = 220.0;
            let h = chooser.height(w, theme);
            let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
            if chooser.click_at(r, theme, p) {
                st.color = chooser.selected();
            }
            true
        }
        Kind::Emoji => {
            let mut emoji = EmojiChooser::new(EmojiChooser::default_palette());
            emoji.set_columns(9);
            let w = (rect.width() - 24.0).min(320.0);
            let h = emoji.height(w, theme);
            let r = Rect::from_xywh(rect.x() + 12.0, rect.center().y - h / 2.0, w, h);
            if emoji.click_at(r, theme, p) {
                st.emoji = emoji.selected();
            }
            true
        }
        Kind::Spin => {
            let mut spin = SpinButton::new(st.spin, 0.0, 10.0).step(1.0).digits(0);
            let h = spin.height(theme);
            let w = 140.0;
            let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
            if spin.click_at(r, theme, p) {
                st.spin = spin.value();
                st.last = format!("Wartość: {:.0}", st.spin);
            }
            true
        }
        Kind::Stepper => {
            let mut stepper = Stepper::new(st.step, 1, 10);
            let h = stepper.height(theme);
            let w = stepper.width(theme);
            let r = Rect::from_xywh(rect.center().x - w / 2.0, rect.center().y - h / 2.0, w, h);
            if let Some(side) = stepper.side_at(r, theme, p) {
                match side {
                    StepperSide::Minus => stepper.dec(),
                    StepperSide::Plus => stepper.inc(),
                }
                st.step = stepper.value();
                st.last = format!("Wartość: {}", st.step);
            }
            true
        }
        Kind::ViewSwitcher => {
            let switcher = ViewSwitcher::new(["Lista", "Siatka", "Karuzela"]);
            let r = centered_rect(rect, 300.0, 32.0, 0.42);
            if let Some(index) = switcher.index_at(r, p) {
                st.view = index;
            }
            true
        }
        Kind::NavList => {
            let mut nav = demo_nav();
            nav.set_selected(st.nav_selected);
            let r = Rect::from_xywh(rect.x() + 12.0, rect.y() + 8.0, 220.0, rect.height() - 16.0);
            if nav.click_at(r, p, theme) {
                st.nav_selected = nav.selected();
            }
            true
        }
        Kind::NavRail => {
            let mut rail = NavigationRail::new(["Dom", "Pliki", "Media", "Ustawienia"]);
            rail.set_selected(st.rail_selected);
            let w = rail.width(theme);
            let r = Rect::from_xywh(rect.x() + 16.0, rect.y() + 8.0, w, rect.height() - 16.0);
            if rail.click_at(r, theme, p) {
                st.rail_selected = rail.selected();
            }
            true
        }
        Kind::TabBar => {
            let mut tabs = TabBar::new(["Ogólne", "Wygląd", "Zaawansowane"]);
            let r = Rect::from_xywh(
                rect.x() + 20.0,
                rect.y() + 40.0,
                (rect.width() - 40.0).max(0.0),
                36.0,
            );
            if tabs.click_at(r, text, theme, p) {
                st.tab = tabs.selected();
            }
            true
        }
        Kind::PathBar => {
            let h = st.path.height(theme);
            let r = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 40.0,
                (rect.width() - 24.0).max(0.0),
                h,
            );
            if let Some(i) = st.path.click_at(r, text, theme, p) {
                st.path.truncate(i + 1);
                st.last = format!("Ścieżka skrócona do {}", i + 1);
            }
            true
        }
        Kind::Pager => {
            let strip =
                Rect::from_xywh(rect.center().x - 120.0, rect.center().y - 8.0, 240.0, 16.0);
            let mut pager = st.pager.clone();
            if pager.click_indicator(strip, theme, p) {
                st.pager.set_active(pager.active());
            }
            true
        }
        Kind::NavigationView => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 6.0,
                rect.width() - 24.0,
                rect.height() - 12.0,
            );
            if st.nav_view.back_at(area, theme, p) {
                st.nav_view.pop();
                st.last = format!("Poziom {}", st.nav_view.depth());
            } else if st.nav_view.content_rect(area, theme).contains(p) {
                st.nav_view.push();
                st.last = format!("Poziom {}", st.nav_view.depth());
            }
            true
        }
        Kind::NavigationSplit => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 6.0,
                rect.width() - 24.0,
                rect.height() - 12.0,
            );
            let content = NavigationSplitView::new()
                .sidebar_width(140.0)
                .content_rect(area);
            if split_toggle(content).is_some_and(|r| r.contains(p)) {
                st.split_sidebar = !st.split_sidebar;
                st.last = if st.split_sidebar {
                    "Panel boczny widoczny".to_owned()
                } else {
                    "Panel boczny schowany".to_owned()
                };
            }
            true
        }
        Kind::OverlaySplit => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 6.0,
                rect.width() - 24.0,
                rect.height() - 12.0,
            );
            if st.overlay_open {
                if OverlaySplitView::new().scrim_at(area, p) {
                    st.overlay_open = false;
                    st.last = "Panel zamknięty".to_owned();
                }
            } else if split_toggle(area).is_some_and(|r| r.contains(p)) {
                st.overlay_open = true;
                st.last = "Panel otwarty".to_owned();
            }
            true
        }
        Kind::Menu => {
            let menu = demo_menu();
            let w = (rect.width() - 24.0).min(230.0);
            let h = menu.height(theme);
            let panel = Rect::from_xywh(rect.x() + 12.0, rect.center().y - h / 2.0, w, h);
            if let Some(index) = menu.index_at(panel, p, theme) {
                st.menu_selected = Some(index);
                if let Some(MenuEntry::Action { label, .. }) = menu.entries().get(index) {
                    st.last = format!("Wybrano: {label}");
                }
            }
            true
        }
        Kind::PopoverMenuBar => {
            let h = theme.metrics.button.height;
            let bar_rect = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 24.0,
                (rect.width() - 24.0).max(0.0),
                h,
            );
            if let Some(i) = st.menu_bar.open_index() {
                let buttons = st.menu_bar.buttons(bar_rect, text, theme);
                if let Some(anchor) = buttons.get(i).copied() {
                    let menu = demo_menu();
                    let panel = menu.place(anchor, 200.0, rect, theme);
                    if let Some(index) = menu.index_at(panel, p, theme) {
                        if let Some(MenuEntry::Action { label, .. }) = menu.entries().get(index) {
                            st.last = format!("Wybrano: {label}");
                        }
                        st.menu_bar.close();
                        return true;
                    }
                }
            }
            if let Some(i) = st.menu_bar.click_at(bar_rect, text, theme, p) {
                st.last = format!("Otwarto menu: {}", st.menu_bar.labels()[i]);
            }
            true
        }
        Kind::Expander => {
            let mut expander = Expander::new("Ustawienia zaawansowane").expanded(st.expanded);
            let r = Rect::from_xywh(rect.x() + 20.0, rect.y() + 24.0, rect.width() - 40.0, 160.0);
            if expander.click_at(r, theme, p) {
                st.expanded = expander.is_expanded();
            }
            true
        }
        Kind::FlowBox => {
            let flow = FlowBox::new(44.0);
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 8.0,
                rect.width() - 24.0,
                rect.height() - 16.0,
            );
            if let Some(i) = flow.index_at(area, p) {
                st.flow_selected = Some(i);
            }
            true
        }
        Kind::Paned => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 8.0,
                rect.width() - 24.0,
                rect.height() - 16.0,
            );
            if st.paned.separator_hit(area, theme, p) {
                st.paned.begin_drag(area, theme, p);
                st.dragging = true;
            }
            true
        }
        Kind::Stack => {
            st.stack_page = (st.stack_page + 1) % st.stack.count();
            st.stack.set_visible_index(st.stack_page);
            st.stack.advance(0.0, 0.25);
            st.last = format!("Strona {}", st.stack_page + 1);
            true
        }
        Kind::ItemView => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 6.0,
                rect.width() - 24.0,
                rect.height() - 12.0,
            );
            if let Some(i) = st.item_view.index_at(area, p, theme) {
                st.item_view.toggle_selected(i);
                st.last = format!(
                    "Zaznaczono {} element(ów)",
                    st.item_view.selected_indices().len()
                );
            }
            true
        }
        Kind::ColumnView => {
            let table = demo_table();
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 4.0,
                rect.width() - 24.0,
                rect.height() - 8.0,
            );
            if let Some(row) = table.row_at(area, theme, p) {
                st.column_selected = Some(row);
                st.last = format!("Wiersz {}", row + 1);
            }
            true
        }
        Kind::Media => {
            let media = MediaControls::new("Noc nad jeziorem", "Luna Ensemble");
            let h = (media.height(theme) + 40.0).min(rect.height() - 40.0);
            let r = Rect::from_xywh(
                rect.x() + 20.0,
                rect.center().y - h / 2.0,
                rect.width() - 40.0,
                h,
            );
            if media.play_at(r, theme, text, p) {
                st.media_playing = !st.media_playing;
            } else if let Some(pos) = media.seek_at(r, theme, text, p) {
                st.media_pos = pos;
                st.dragging = true;
            }
            true
        }
        Kind::Progress => {
            let w = (rect.width() - 80.0).clamp(120.0, 520.0);
            let x = rect.center().x - w / 2.0;
            let bar = Rect::from_xywh(x, rect.center().y - 30.0, w, 10.0);
            if p.x >= bar.x()
                && p.x <= bar.right()
                && p.y >= bar.y() - 40.0
                && p.y <= bar.y() + 40.0
            {
                st.level = ((p.x - bar.x()) / bar.width()).clamp(0.0, 1.0);
            }
            true
        }
        Kind::InfoBar => {
            if st.info_visible {
                let bar = InfoBar::new("Mało miejsca na dysku")
                    .kind(InfoBarKind::Warning)
                    .action("Zarządzaj")
                    .show_close(true);
                let area =
                    Rect::from_xywh(rect.x() + 12.0, rect.y() + 24.0, rect.width() - 24.0, 40.0);
                if bar.action_at(area, theme, text, p).is_some() {
                    st.last = "Akcja: Zarządzaj".to_owned();
                    st.info_visible = false;
                } else if bar.close_at(area, theme, text, p) {
                    st.info_visible = false;
                    st.last = "Pasek zamknięty".to_owned();
                }
            } else {
                st.info_visible = true;
                st.last = "Pasek przywrócony".to_owned();
            }
            true
        }
        Kind::Banner => {
            if st.banner_visible {
                let banner = Banner::new("Nowa wersja libLuny")
                    .kind(BannerKind::Info)
                    .description("Wszystkie widgety są już dostępne.")
                    .action("Zainstaluj");
                let area =
                    Rect::from_xywh(rect.x() + 12.0, rect.y() + 14.0, rect.width() - 24.0, 56.0);
                if banner.action_at(area, theme, text, p).is_some() {
                    st.last = "Akcja: Zainstaluj".to_owned();
                    st.banner_visible = false;
                }
            } else {
                st.banner_visible = true;
                st.last = "Baner przywrócony".to_owned();
            }
            true
        }
        Kind::StatusPage => {
            let page = StatusPage::new("Brak wyników")
                .icon(StatusIcon::Search)
                .description("Zmień frazę i spróbuj ponownie")
                .action("Wyczyść");
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 4.0,
                rect.width() - 24.0,
                rect.height() - 8.0,
            );
            if page.action_at(area, theme, p) {
                st.status_clicks += 1;
                st.last = "Wyczyszczono wyszukiwanie".to_owned();
            }
            true
        }
        Kind::HeaderBar => {
            let labels = HEADER_VARIANTS;
            let switcher = ViewSwitcher::new(labels);
            if let Some(index) = switcher.index_at(header_switcher_rect(rect), p) {
                st.header_variant = index;
                st.header_hover = None;
                st.last = format!("Wariant: {}", labels[index]);
                return true;
            }
            let variant = st.header_variant.min(labels.len() - 1);
            let bar = header_variant_bar(variant);
            let bar_rect = header_demo_rect(rect, theme);
            if let Some(control) = bar.control_at(bar_rect, theme, p) {
                st.last = format!("Kontrolka okna: {}", window_control_label(control));
                return true;
            }
            if variant == 3 {
                let (burger, new, sw) = header_action_rects(bar_rect, theme, text);
                if burger.contains(p) {
                    st.last = "Menu aplikacji".to_owned();
                } else if new.contains(p) {
                    st.last = "Akcja: Nowy".to_owned();
                } else {
                    let view = ViewSwitcher::new(["Lista", "Siatka"]);
                    if let Some(index) = view.index_at(sw, p) {
                        st.header_view = index;
                        st.last = format!("Widok: {}", ["Lista", "Siatka"][index]);
                    }
                }
            }
            true
        }
        Kind::Assistant => {
            let area = Rect::from_xywh(
                rect.x() + 12.0,
                rect.y() + 4.0,
                rect.width() - 24.0,
                rect.height() - 8.0,
            );
            let [cancel, next] = st.assistant.buttons(area, text, theme, "Anuluj", "Dalej");
            if next.contains(p) {
                if st.assistant.is_last() {
                    st.last = "Kreator zakończony".to_owned();
                } else {
                    st.assistant.next();
                    st.last = format!("Krok {}", st.assistant.current() + 1);
                }
            } else if cancel.contains(p) {
                st.assistant.set_current(0);
                st.last = "Kreator anulowany".to_owned();
            }
            true
        }
        Kind::Static => false,
    }
}

/// Ruch wskaźnika (przeciąganie suwaka / separatora / paska odtwarzacza).
pub fn motion(
    name: &str,
    rect: Rect,
    p: Point,
    theme: &Theme,
    text: &TextEngine,
    st: &mut ExampleState,
) -> bool {
    let kind = kind(name);
    if kind == Kind::HeaderBar {
        let bar = header_variant_bar(st.header_variant);
        let bar_rect = header_demo_rect(rect, theme);
        st.header_hover = bar.control_at(bar_rect, theme, p);
        return false;
    }
    if kind == Kind::Paned {
        let area = Rect::from_xywh(
            rect.x() + 12.0,
            rect.y() + 8.0,
            rect.width() - 24.0,
            rect.height() - 16.0,
        );
        if st.paned.is_dragging() {
            st.paned.drag_to(area, theme, p);
            return true;
        }
        return false;
    }
    if !st.dragging {
        return false;
    }
    match kind {
        Kind::Slider => {
            st.slider = Slider::new(0.0).value_at(slider_track(theme, rect), p);
            true
        }
        Kind::Media => {
            let media = MediaControls::new("Noc nad jeziorem", "Luna Ensemble");
            let h = (media.height(theme) + 40.0).min(rect.height() - 40.0);
            let r = Rect::from_xywh(
                rect.x() + 20.0,
                rect.center().y - h / 2.0,
                rect.width() - 40.0,
                h,
            );
            if let Some(pos) = media.seek_at(r, theme, text, p) {
                st.media_pos = pos;
            }
            true
        }
        _ => false,
    }
}

/// Zwolnienie przycisku — kończy przeciąganie.
pub fn released(st: &mut ExampleState) {
    st.dragging = false;
    st.paned.end_drag();
}

/// Klawiatura w przykładzie. Zwraca `true`, gdy zdarzenie zostało zużyte.
pub fn key(st: &mut ExampleState, ke: &KeyEvent) -> bool {
    // Escape zawsze zdejmuje fokus z pól tekstowych.
    if ke.keysym == 0xff1b {
        if st.focus != Focus::None {
            st.focus = Focus::None;
            st.entry.set_focused(false);
            st.password.set_focused(false);
            st.search.set_focused(false);
            st.bar_entry.set_focused(false);
            st.note.set_focused(false);
            return true;
        }
        return false;
    }

    if ke.state != ButtonState::Pressed {
        return false;
    }

    match st.focus {
        Focus::Entry => {
            st.entry.set_focused(true);
            text_key(&mut st.entry, ke)
        }
        Focus::Password => {
            st.password.set_focused(true);
            text_key(&mut st.password, ke)
        }
        Focus::Search => {
            st.search.set_focused(true);
            text_key(&mut st.search, ke)
        }
        Focus::SearchBar => {
            st.bar_entry.set_focused(true);
            text_key(&mut st.bar_entry, ke)
        }
        Focus::Note => {
            st.note.set_focused(true);
            text_view_key(&mut st.note, ke)
        }
        Focus::None => false,
    }
}

pub(crate) fn text_key<T: TextInput>(entry: &mut T, ke: &KeyEvent) -> bool {
    let shift = ke.modifiers.shift;
    let ctrl = ke.modifiers.ctrl;
    if ctrl && ke.keysym == 0x61 {
        entry.select_all();
        return true;
    }
    if !ctrl {
        if let Some(text) = &ke.text {
            if !text.is_empty() && !text.chars().all(char::is_control) {
                entry.insert_str(text);
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
        0x20..=0x7e if !ctrl => {
            if let Some(ch) = char::from_u32(ke.keysym) {
                entry.insert_char(ch);
            }
        }
        _ => return false,
    }
    true
}

fn text_view_key(view: &mut TextView, ke: &KeyEvent) -> bool {
    let shift = ke.modifiers.shift;
    let ctrl = ke.modifiers.ctrl;
    if ctrl && ke.keysym == 0x61 {
        view.select_all();
        return true;
    }
    if !ctrl {
        if let Some(text) = &ke.text {
            if !text.is_empty() && !text.chars().all(char::is_control) {
                view.insert_str(text);
                return true;
            }
        }
    }
    match ke.keysym {
        0xff0d => view.insert_newline(),
        0xff08 => view.backspace(),
        0xffff => view.delete_forward(),
        0xff51 => {
            if shift {
                view.move_left_selecting(true)
            } else {
                view.move_left()
            }
        }
        0xff53 => {
            if shift {
                view.move_right_selecting(true)
            } else {
                view.move_right()
            }
        }
        0xff50 => {
            if shift {
                view.home_selecting(true)
            } else {
                view.home()
            }
        }
        0xff57 => {
            if shift {
                view.end_selecting(true)
            } else {
                view.end()
            }
        }
        0xff52 => {
            if shift {
                view.move_up_selecting(true)
            } else {
                view.move_up()
            }
        }
        0xff54 => {
            if shift {
                view.move_down_selecting(true)
            } else {
                view.move_down()
            }
        }
        0x20..=0x7e if !ctrl => {
            if let Some(ch) = char::from_u32(ke.keysym) {
                view.insert_char(ch);
            }
        }
        _ => return false,
    }
    true
}

// ── Pomocnicze ──────────────────────────────────────────────────────────────

fn demo_menu() -> Menu {
    Menu::new(vec![
        MenuEntry::action("Nowy"),
        MenuEntry::action("Otwórz"),
        MenuEntry::action("Zapisz").shortcut("Ctrl+S"),
        MenuEntry::separator(),
        MenuEntry::action("Zamknij"),
    ])
}

fn demo_nav() -> NavList {
    NavList::new(vec![
        NavEntry::Section("System".into()),
        NavEntry::Item("Sieć".into()),
        NavEntry::Item("Ekran".into()),
        NavEntry::Item("Dźwięk".into()),
    ])
}

fn demo_table() -> ColumnView {
    let mut table = ColumnView::new(vec![
        Column::new("Nazwa").weight(2.0),
        Column::new("Rozmiar"),
    ]);
    table.set_rows(4);
    table
}

fn inner(rect: Rect) -> Rect {
    Rect::from_xywh(
        rect.x() + 12.0,
        rect.y() + 10.0,
        (rect.width() - 24.0).max(0.0),
        (rect.height() - 20.0).max(0.0),
    )
}

fn split_toggle(content: Rect) -> Option<Rect> {
    if content.width() < 48.0 || content.height() < 48.0 {
        None
    } else {
        Some(Rect::from_xywh(
            content.x() + 8.0,
            content.y() + 8.0,
            30.0,
            30.0,
        ))
    }
}

fn paint_toggle_button(ctx: &mut PaintCtx, rect: Rect, active: bool) {
    let theme = *ctx.theme;
    let bg = if active {
        theme.colors.accent.scale_alpha(0.2)
    } else {
        theme.colors.card_bg
    };
    ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.small, 1.0, theme.colors.border);
    let color = if active {
        theme.colors.accent
    } else {
        theme.colors.dim_text
    };
    let cx = rect.center().x;
    let cy = rect.center().y;
    for i in -1..=1 {
        let bar = Rect::from_xywh(cx - 6.0, cy + i as f64 * 4.5 - 0.75, 12.0, 1.5);
        ctx.scene.fill_rounded_rect(bar, 0.75, color);
    }
}

fn field_rect(rect: Rect, height: f64) -> Rect {
    let w = (rect.width() - 80.0).clamp(220.0, 420.0);
    Rect::from_xywh(
        rect.center().x - w / 2.0,
        rect.center().y - height / 2.0 - 14.0,
        w,
        height,
    )
}

fn slider_track(theme: &Theme, rect: Rect) -> Rect {
    Rect::from_xywh(
        rect.x() + 40.0,
        rect.center().y - theme.metrics.slider.track_height / 2.0,
        (rect.width() - 80.0).max(40.0),
        theme.metrics.slider.track_height,
    )
}

fn centered_rect(rect: Rect, w: f64, h: f64, ratio: f64) -> Rect {
    Rect::from_xywh(
        rect.center().x - w / 2.0,
        rect.y() + rect.height() * ratio - h / 2.0,
        w,
        h,
    )
}

fn hint(ctx: &mut PaintCtx, rect: Rect, text: &str) {
    let theme = *ctx.theme;
    ctx.label_center(
        rect.center().x,
        rect.bottom() - 16.0,
        text,
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn centered_note(ctx: &mut PaintCtx, rect: Rect, text: &str) {
    let theme = *ctx.theme;
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        text,
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.dim_text,
    );
}

fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Styczeń",
        2 => "Luty",
        3 => "Marzec",
        4 => "Kwiecień",
        5 => "Maj",
        6 => "Czerwiec",
        7 => "Lipiec",
        8 => "Sierpień",
        9 => "Wrzesień",
        10 => "Październik",
        11 => "Listopad",
        _ => "Grudzień",
    }
}

fn weekdays() -> Vec<String> {
    ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery_catalog::DEMOS;
    use libluna_render::Scene;

    #[test]
    fn every_interactive_demo_accepts_clicks_and_paints() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let rect = Rect::from_xywh(0.0, 0.0, 800.0, 300.0);
        let points = [
            rect.center(),
            Point::new(rect.x() + 20.0, rect.y() + 20.0),
            Point::new(rect.right() - 20.0, rect.bottom() - 20.0),
        ];
        for demo in DEMOS {
            if !supports(demo.name) {
                continue;
            }
            let mut state = ExampleState::default();
            for p in points {
                assert!(
                    pressed(demo.name, rect, p, &theme, &text, &mut state),
                    "{} nie przyjął kliknięcia",
                    demo.name
                );
            }
            let mut scene = Scene::new();
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            assert!(
                paint(demo.name, &mut ctx, rect, &state),
                "{} nie narysował się",
                demo.name
            );
            assert!(!scene.is_empty(), "{} nie dodał nic do sceny", demo.name);
        }
    }

    #[test]
    fn headerbar_variants_switch_and_report_window_controls() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let rect = Rect::from_xywh(0.0, 0.0, 800.0, 300.0);
        let mut state = ExampleState::default();
        let labels = HEADER_VARIANTS;
        let segments = ViewSwitcher::new(labels).segments(header_switcher_rect(rect));
        assert!(pressed(
            "HeaderBar",
            rect,
            segments[2].center(),
            &theme,
            &text,
            &mut state
        ));
        assert_eq!(state.header_variant, 2);

        let bar = header_variant_bar(2);
        let close = bar
            .controls_layout(header_demo_rect(rect, &theme), &theme)
            .close
            .expect("kontrolka zamknięcia");
        assert!(pressed(
            "HeaderBar",
            rect,
            close.center(),
            &theme,
            &text,
            &mut state
        ));
        assert!(state.last.contains("Zamknij"));
    }
}
