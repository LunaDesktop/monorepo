//! Interaktywne przykłady widgetów w galerii deweloperskiej.
//!
//! Galeria pokazuje prawdziwe widgety z `libluna-widgets` — ten moduł dodaje im
//! **stan**, żeby kliknięcia i klawisze naprawdę coś robiły (przełącznik się
//! przełącza, suwak przesuwa, pole przyjmuje tekst itd.). Widgety, których nie ma
//! na liście [`kind`], są rysowane przez swój statyczny podgląd z
//! [`crate::gallery_previews`] — nadal są to prawdziwe widgety, tylko bez stanu.

use libluna_core::style::weight;
use libluna_core::{ButtonState, KeyEvent, Point, Rect, Theme};
use libluna_render::TextEngine;
use libluna_widgets::{
    Button, ButtonVariant, Calendar, CheckButton, ColorButton, ColorChooser, DropDown, Entry,
    Expander, LevelBar, LinkButton, MediaControls, Menu, MenuButton, MenuEntry, Pager, PaintCtx,
    PasswordEntry, ProgressBar, RadioGroup, SearchEntry, Slider, SpinButton, Stepper, StepperSide,
    Switch, TabBar, TextInput, ToggleButton, ToggleGroup, ViewSwitcher,
};

/// Co robi dany widget w trybie interaktywnym.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Button,
    ButtonsExtra,
    MenuButton,
    Entry,
    Password,
    Search,
    Slider,
    Switch,
    Check,
    Radio,
    DropDown,
    ToggleGroup,
    Calendar,
    ColorChooser,
    ViewSwitcher,
    TabBar,
    Expander,
    Spin,
    Stepper,
    Pager,
    Media,
    Progress,
    /// Tylko podgląd (prawdziwy widget, ale bez stanu).
    Static,
}

/// Mapowanie nazwy wpisu katalogu na zachowanie interaktywne.
pub fn kind(name: &str) -> Kind {
    match name {
        "Button" => Kind::Button,
        "ToggleButton / LinkButton / ColorButton" => Kind::ButtonsExtra,
        "MenuButton" => Kind::MenuButton,
        "Entry" => Kind::Entry,
        "PasswordEntry" => Kind::Password,
        "SearchEntry" => Kind::Search,
        "Slider" => Kind::Slider,
        "Switch" => Kind::Switch,
        "CheckButton" => Kind::Check,
        "RadioGroup" => Kind::Radio,
        "DropDown" => Kind::DropDown,
        "ToggleGroup" => Kind::ToggleGroup,
        "Calendar" => Kind::Calendar,
        "ColorChooser" => Kind::ColorChooser,
        "ViewSwitcher" => Kind::ViewSwitcher,
        "TabBar" => Kind::TabBar,
        "Expander" => Kind::Expander,
        "SpinButton" => Kind::Spin,
        "Stepper" => Kind::Stepper,
        "Pager" => Kind::Pager,
        "MediaControls" => Kind::Media,
        "ProgressBar / LevelBar" => Kind::Progress,
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
}

impl Default for ExampleState {
    fn default() -> Self {
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
        if self.last.is_empty() {
            String::new()
        } else {
            self.last.clone()
        }
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
        Kind::Slider => paint_slider(ctx, rect, st.slider),
        Kind::Switch => paint_switch(ctx, rect, st.switch_on),
        Kind::Check => paint_check(ctx, rect, st.checked),
        Kind::Radio => paint_radio(ctx, rect, st.radio),
        Kind::DropDown => paint_dropdown(ctx, rect, st),
        Kind::ToggleGroup => paint_toggle_group(ctx, rect, st),
        Kind::Calendar => paint_calendar(ctx, rect, st),
        Kind::ColorChooser => paint_color_chooser(ctx, rect, st),
        Kind::ViewSwitcher => {
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
        Kind::TabBar => {
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
        Kind::Expander => paint_expander(ctx, rect, st.expanded),
        Kind::Spin => paint_spin(ctx, rect, st.spin),
        Kind::Stepper => paint_stepper(ctx, rect, st.step),
        Kind::Pager => paint_pager(ctx, rect, st),
        Kind::Media => paint_media(ctx, rect, st),
        Kind::Progress => paint_progress(ctx, rect, st.level),
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
        let menu = demo_menu();
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

fn paint_pager(ctx: &mut PaintCtx, rect: Rect, st: &ExampleState) {
    let strip = Rect::from_xywh(rect.center().x - 120.0, rect.center().y - 8.0, 240.0, 16.0);
    st.pager.paint(ctx, strip);
    hint(
        ctx,
        rect,
        &format!("Strona {} z {}", st.pager.active() + 1, st.pager.count()),
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
                let menu = demo_menu();
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
        Kind::ViewSwitcher => {
            let switcher = ViewSwitcher::new(["Lista", "Siatka", "Karuzela"]);
            let r = centered_rect(rect, 300.0, 32.0, 0.42);
            if let Some(index) = switcher.index_at(r, p) {
                st.view = index;
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
        Kind::Expander => {
            let mut expander = Expander::new("Ustawienia zaawansowane").expanded(st.expanded);
            let r = Rect::from_xywh(rect.x() + 20.0, rect.y() + 24.0, rect.width() - 40.0, 160.0);
            if expander.click_at(r, theme, p) {
                st.expanded = expander.is_expanded();
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
        Kind::Pager => {
            let strip =
                Rect::from_xywh(rect.center().x - 120.0, rect.center().y - 8.0, 240.0, 16.0);
            let mut pager = st.pager.clone();
            if pager.click_indicator(strip, theme, p) {
                st.pager.set_active(pager.active());
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
        Kind::Static => false,
    }
}

/// Ruch wskaźnika (przeciąganie suwaka / paska odtwarzacza).
pub fn motion(
    name: &str,
    rect: Rect,
    p: Point,
    theme: &Theme,
    text: &TextEngine,
    st: &mut ExampleState,
) -> bool {
    if !st.dragging {
        return false;
    }
    match kind(name) {
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
