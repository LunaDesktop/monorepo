//! Panel górny i launcher środowiska Luna.
//!
//! Warstwa rysowania jest oddzielona od źródła danych ([`PanelState`], [`App`]), więc
//! panel można renderować headless (podglądy) oraz „na żywo” w sesji Selene.
//!
//! Panel ma po lewej **sam przycisk burger** (bez etykiety) — kliknięcie otwiera launcher.
//! Launcher ma trzy widoki: **lista**, **siatka** i **karuzela**.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Size, Theme, ThemePreference};
use libluna_render::TextEngine;
use libluna_widgets::{paint_icon, IconName, PaintCtx};

use crate::power::PowerAction;
use crate::system::{BatteryStatus, WifiNetwork};
use crate::toplevels::Task;

/// Wysokość panelu (piksele logiczne).
pub const THICKNESS: f64 = 40.0;
/// Domyślna szerokość karty launchera.
pub const LAUNCHER_W: f64 = 720.0;
/// Domyślna wysokość karty launchera.
pub const LAUNCHER_H: f64 = 600.0;
/// Bok przycisku zasilania w pasku.
pub const POWER_BTN: f64 = 24.0;
/// Szerokość rozwijanego menu zasilania.
pub const POWER_MENU_W: f64 = 176.0;
/// Wysokość pozycji menu zasilania.
pub const POWER_ITEM_H: f64 = 34.0;

const GRID_COLS: usize = 4;
const GRID_CELL_H: f64 = 76.0;
const LIST_ROW_H: f64 = 52.0;
const CARD_W: f64 = 168.0;
const CARD_H: f64 = 200.0;

/// Akcja przełącznika w pasku (kliknięcie).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToggleAction {
    #[default]
    None,
    Wifi,
    Volume,
    /// Jasność ekranu (klik cyklicznie zmienia poziom).
    Brightness,
}

/// Przełącznik szybkiego dostępu (Wi-Fi, dźwięk, bateria…).
#[derive(Debug, Clone)]
pub struct Toggle {
    /// Pełna nazwa przełącznika (tytuł w popupie, np. SSID albo „Dźwięk”).
    /// **Nie** jest rysowana na pasku — pasek pokazuje same ikony.
    pub label: String,
    /// Krótka wartość obok ikony na pasku (np. „82%”); `None` = sama ikona.
    pub value: Option<String>,
    /// Ikona z pakietu Luny.
    pub icon: IconName,
    pub active: bool,
    /// Co robi klik (jeśli cokolwiek).
    pub action: ToggleAction,
}

/// Stan panelu — czyste dane, bez logiki okna (łatwe do testów i podglądów).
#[derive(Debug, Clone)]
pub struct PanelState {
    /// Tytuł aktywnego okna lub nazwa przestrzeni.
    pub title: String,
    /// Godzina, np. „14:32”.
    pub clock: String,
    /// Data, np. „pon, 10 paź”.
    pub date: String,
    /// Liczba przestrzeni roboczych.
    pub workspaces: usize,
    /// Indeks bieżącej przestrzeni roboczej.
    pub current_ws: usize,
    /// Przełączniki szybkiego dostępu.
    pub toggles: Vec<Toggle>,
    /// Nazwa zalogowanego użytkownika.
    pub user: String,
    /// Okna z kompozytora (`zwlr_foreign_toplevel`) — taskbar na pasku.
    pub tasks: Vec<Task>,
}

impl PanelState {
    /// Przykładowy stan (podglądy, testy).
    pub fn sample() -> Self {
        Self {
            title: "Pliki — Dokumenty".to_owned(),
            clock: "14:32".to_owned(),
            date: "pon, 10 paź".to_owned(),
            workspaces: 4,
            current_ws: 1,
            toggles: vec![
                Toggle {
                    label: "Wi-Fi".to_owned(),
                    value: None,
                    icon: IconName::Wifi,
                    active: true,
                    action: ToggleAction::Wifi,
                },
                Toggle {
                    label: "Dźwięk".to_owned(),
                    value: None,
                    icon: IconName::Volume,
                    active: true,
                    action: ToggleAction::Volume,
                },
                Toggle {
                    label: "Bateria".to_owned(),
                    value: Some("82%".to_owned()),
                    icon: IconName::Battery,
                    active: false,
                    action: ToggleAction::None,
                },
            ],
            user: "patryks".to_owned(),
            tasks: vec![
                Task {
                    title: "Pliki — Dokumenty".to_owned(),
                    app_id: "files".to_owned(),
                    active: true,
                    minimized: false,
                },
                Task {
                    title: "Terminal".to_owned(),
                    app_id: "terminal".to_owned(),
                    active: false,
                    minimized: false,
                },
            ],
        }
    }
}

/// Widok launchera.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LauncherView {
    /// Pionowa lista wierszy.
    List,
    /// Siatka kafelków.
    #[default]
    Grid,
    /// Pozioma karuzela kart.
    Carousel,
}

impl LauncherView {
    /// Wszystkie widoki w kolejności przełącznika.
    pub const ALL: [LauncherView; 3] = [
        LauncherView::List,
        LauncherView::Grid,
        LauncherView::Carousel,
    ];

    /// Etykieta w przełączniku.
    pub fn label(self) -> &'static str {
        match self {
            LauncherView::List => "Lista",
            LauncherView::Grid => "Siatka",
            LauncherView::Carousel => "Karuzela",
        }
    }

    /// Następny widok (cyklicznie).
    pub fn next(self) -> Self {
        match self {
            LauncherView::List => LauncherView::Grid,
            LauncherView::Grid => LauncherView::Carousel,
            LauncherView::Carousel => LauncherView::List,
        }
    }
}

/// Stan launchera (widok + bieżąca karta karuzeli + przewinięcie + wyszukiwanie).
#[derive(Debug, Clone, Default)]
pub struct Launcher {
    pub view: LauncherView,
    pub carousel: usize,
    pub selected: Option<usize>,
    pub scroll: f64,
    /// Fraza wyszukiwania aplikacji.
    pub search: String,
}

impl Launcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Filtruje aplikacje wg bieżącej frazy wyszukiwania.
    pub fn filter_apps<'a>(&self, apps: &'a [App]) -> Vec<&'a App> {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return apps.iter().collect();
        }
        apps.iter()
            .filter(|a| {
                a.name.to_lowercase().contains(&q)
                    || a.category.to_lowercase().contains(&q)
                    || a.exec.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Przełącza na kolejny widok.
    pub fn next_view(&mut self) {
        self.view = self.view.next();
    }

    /// Bieżący indeks zaznaczenia (domyślnie 0, o ile są pozycje).
    pub fn selected_index(&self, count: usize) -> Option<usize> {
        if count == 0 {
            None
        } else {
            Some(self.selected.unwrap_or(0).min(count - 1))
        }
    }

    /// Przesuwa zaznaczenie o krok (`dx`, `dy`) zależnie od widoku.
    ///
    /// W siatce `dx`/`dy` to ruch odpowiednio w kolumnie/wierszu; w liście
    /// i karuzeli używany jest tylko jeden z nich (`dy`/`dx`).
    pub fn navigate(&mut self, count: usize, dx: i64, dy: i64) {
        if count == 0 {
            return;
        }
        let n = count as i64;
        let cur = self.selected.unwrap_or(0).min(count - 1) as i64;
        let next = match self.view {
            LauncherView::Grid => {
                let cols = GRID_COLS as i64;
                let rows = (n + cols - 1) / cols;
                let col = (cur % cols + dx).clamp(0, cols - 1);
                let row = (cur / cols + dy).clamp(0, rows - 1);
                (row * cols + col).min(n - 1)
            }
            _ => (cur + dx + dy).rem_euclid(n),
        };
        self.selected = Some(next as usize);
        if self.view == LauncherView::Carousel {
            self.carousel = next as usize;
        }
    }

    /// Przewija zawartość listy/siatki o `notches` ząbków kółka myszy.
    ///
    /// `notches` jest w „ząbkach” — **dodatnie = w dół** (dalej w treści / w stronę
    /// końca listy), ujemne = w górę. `px_per_notch` mówi, ile pikseli przypada na
    /// jeden ząbek. Wynik jest ograniczony do `[0, max]`.
    pub fn scroll_by_notches(&mut self, notches: f64, px_per_notch: f64, max: f64) {
        if !notches.is_finite() || notches == 0.0 {
            return;
        }
        self.scroll = (self.scroll + notches * px_per_notch).clamp(0.0, max.max(0.0));
    }
}

/// Pozycja w launcherze.
#[derive(Debug, Clone)]
pub struct App {
    /// Ikona z pakietu Luny.
    pub icon: IconName,
    pub name: String,
    /// Krótki opis/kategoria (widok listy).
    pub category: String,
    /// Polecenie uruchamiające (dla prawdziwych aplikacji z plików `.desktop`).
    pub exec: Option<String>,
    /// Czy aplikacja wymaga uruchomienia w terminalu.
    pub terminal: bool,
    /// Plik `.desktop` — uruchamiamy przez `gio launch`, gdy jest dostępny.
    pub desktop_path: Option<std::path::PathBuf>,
}

impl App {
    /// Przykładowy zestaw ikon (podglądy, gdy brak `.desktop`).
    pub fn sample() -> Vec<App> {
        const APPS: [(IconName, &str, &str); 12] = [
            (IconName::Files, "Pliki", "System"),
            (IconName::Settings, "Ustawienia", "System"),
            (IconName::Edit, "Edytor", "Akcesoria"),
            (IconName::Music, "Odtwarzacz", "Multimedia"),
            (IconName::Star, "Ulubione", "System"),
            (IconName::Clock, "Zegar", "Akcesoria"),
            (IconName::Terminal, "Terminal", "System"),
            (IconName::Store, "Sklep", "System"),
            (IconName::Wifi, "Sieć", "System"),
            (IconName::Home, "Katalog", "System"),
            (IconName::User, "Konta", "System"),
            (IconName::Info, "Pomoc", "System"),
        ];
        APPS.iter()
            .map(|(i, n, c)| App {
                icon: *i,
                name: (*n).to_owned(),
                category: (*c).to_owned(),
                exec: None,
                terminal: *n == "Terminal",
                desktop_path: None,
            })
            .collect()
    }

    /// Buduje pozycje launchera z wykrytych aplikacji (plików `.desktop`).
    pub fn from_desktop(list: &[crate::apps::DesktopApp]) -> Vec<App> {
        list.iter()
            .map(|d| App {
                icon: crate::apps::icon_for(d),
                name: d.name.clone(),
                category: d.categories.first().cloned().unwrap_or_default(),
                exec: Some(d.exec.clone()),
                terminal: d.terminal,
                desktop_path: d.desktop_path.clone(),
            })
            .collect()
    }
}

/// Prostokąt przycisku burgera na panelu.
pub fn burger_rect(panel: Rect, theme: &Theme) -> Rect {
    let cy = panel.center().y;
    Rect::from_xywh(panel.x() + theme.spacing.md, cy - 14.0, 28.0, 28.0)
}

/// Prostokąt przycisku zasilania (skrajna prawa strona paska).
pub fn power_rect(bar: Rect, theme: &Theme) -> Rect {
    let cy = bar.center().y;
    let pad = theme.spacing.md;
    Rect::from_xywh(
        bar.right() - pad - POWER_BTN,
        cy - POWER_BTN / 2.0,
        POWER_BTN,
        POWER_BTN,
    )
}

/// Prostokąty pigułek przełączników (spójne z `paint_panel`), w kolejności `state.toggles`.
pub fn toggle_rects(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Vec<Rect> {
    let t = theme;
    let cy = bar.center().y;
    let cap = t.typography.caption.size;
    let pad = t.spacing.md;
    let uw = text
        .measure(&state.user, t.typography.body.size, weight::NORMAL)
        .width;
    // Za użytkownikiem i separatorem (jak w `paint_panel`); dalej przycisk zasilania.
    let mut right = bar.right() - pad - POWER_BTN - t.spacing.sm - uw - 2.0 * t.spacing.md;
    let mut rects = vec![Rect::ZERO; state.toggles.len()];
    for (i, toggle) in state.toggles.iter().enumerate().rev() {
        // Kompaktowy klaster: sama ikona (bez pigułki) i ciasne odstępy.
        let icon = 15.0;
        let vw = toggle
            .value
            .as_deref()
            .map(|v| t.spacing.xs + text.measure(v, cap, weight::MEDIUM).width)
            .unwrap_or(0.0);
        let w = 2.0 * t.spacing.xs + icon + vw;
        let pill = Rect::from_xywh(right - w, cy - 11.0, w, 22.0);
        rects[i] = pill;
        right = pill.x() - t.spacing.xs;
    }
    rects
}

/// Indeks przełącznika pod punktem `p`.
pub fn toggle_at(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    state: &PanelState,
    p: Point,
) -> Option<usize> {
    if p.y < bar.y() || p.y > bar.bottom() {
        return None;
    }
    toggle_rects(bar, theme, text, state)
        .iter()
        .position(|r| p.x >= r.x() - 4.0 && p.x <= r.right() + 4.0)
}

/// Prostokąty kropek przestrzeni roboczych (spójne z `paint_panel`), w kolejności `state.workspaces`.
pub fn workspace_rects(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    state: &PanelState,
) -> Vec<Rect> {
    let t = theme;
    let cy = bar.center().y;
    let d = 7.0;
    let toggles = toggle_rects(bar, t, text, state);
    // Kropki leżą **na lewo** od całej grupy przełączników — przełączniki są układane
    // od prawej, więc pierwszy z nich (`first()`) jest skrajnie lewy.
    let mut right = toggles
        .first()
        .map(|r| r.x() - t.spacing.xs)
        .unwrap_or(bar.right() - t.spacing.md);
    let mut rects = vec![Rect::ZERO; state.workspaces];
    for i in (0..state.workspaces).rev() {
        let dot = Rect::from_xywh(right - d, cy - d / 2.0, d, d);
        rects[i] = dot;
        right = dot.x() - t.spacing.xs;
    }
    rects
}

/// Indeks kropki przestrzeni roboczej pod punktem `p`.
pub fn workspace_at(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    state: &PanelState,
    p: Point,
) -> Option<usize> {
    if p.y < bar.y() || p.y > bar.bottom() {
        return None;
    }
    workspace_rects(bar, theme, text, state)
        .iter()
        .position(|r| p.x >= r.x() - 4.0 && p.x <= r.right() + 4.0)
}

/// Aktywny popup lub menu na panelu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivePopup {
    #[default]
    None,
    Launcher,
    Session,
    Wifi,
    Volume,
    Calendar,
    /// Menu kontekstowe przycisku okna w taskbarze (PPM).
    TaskMenu,
    /// Panel szybkich ustawień (motyw, Wi-Fi, dźwięk, jasność).
    Quick,
}

/// Prostokąt sekcji zegara na środku paska.
pub fn clock_rect(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Rect {
    let body = theme.typography.body.size;
    let clock = format!("{}   ·   {}", state.clock, state.date);
    let cw = text.measure(&clock, body, weight::MEDIUM).width;
    let pad = theme.spacing.md;
    Rect::from_xywh(
        bar.center().x - cw / 2.0 - pad,
        bar.center().y - 14.0,
        cw + 2.0 * pad,
        28.0,
    )
}

/// Prostokąt etykiety użytkownika na pasku.
pub fn user_rect(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Rect {
    let power = power_rect(bar, theme);
    let uw = text
        .measure(&state.user, theme.typography.body.size, weight::NORMAL)
        .width;
    let right = power.x() - theme.spacing.sm;
    Rect::from_xywh(
        right - uw - theme.spacing.md,
        bar.center().y - 14.0,
        uw + theme.spacing.md,
        28.0,
    )
}

/// Prostokąt rozwijanego menu zasilania (pod przyciskiem, wyrównany do prawej).
pub fn power_menu_rect(bar: Rect, theme: &Theme, items: usize) -> Rect {
    let p = power_rect(bar, theme);
    let pad = 6.0;
    let h = items as f64 * POWER_ITEM_H + 2.0 * pad;
    Rect::from_xywh(
        p.right() - POWER_MENU_W,
        bar.bottom() + pad,
        POWER_MENU_W,
        h,
    )
}

/// Indeks pozycji menu zasilania pod punktem `p`.
pub fn power_item_at(bar: Rect, theme: &Theme, items: usize, p: Point) -> Option<usize> {
    let menu = power_menu_rect(bar, theme, items);
    let inner = Rect::from_xywh(
        menu.x() + 6.0,
        menu.y() + 6.0,
        menu.width() - 12.0,
        items as f64 * POWER_ITEM_H,
    );
    if !inner.contains(p) {
        return None;
    }
    let i = ((p.y - inner.y()) / POWER_ITEM_H).floor() as usize;
    (i < items).then_some(i)
}

/// Rysuje menu zasilania z podanymi etykietami (spójne z [`power_item_at`]).
pub fn paint_power_menu(ctx: &mut PaintCtx, bar: Rect, labels: &[&str]) {
    let t = ctx.theme;
    let menu = power_menu_rect(bar, t, labels.len());
    ctx.scene
        .fill_rounded_rect(menu, t.radius.medium, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(menu, t.radius.medium, 1.0, t.colors.border);
    let x = menu.x() + 6.0;
    let y = menu.y() + 6.0;
    let w = menu.width() - 12.0;
    for (i, label) in labels.iter().enumerate() {
        let row = Rect::from_xywh(x, y + i as f64 * POWER_ITEM_H, w, POWER_ITEM_H);
        ctx.label_left(
            row.x() + 10.0,
            row.center().y,
            label,
            t.typography.body.size,
            weight::NORMAL,
            t.colors.text,
        );
    }
}

/// Prostokąt rozwijanego menu sesji (pod przyciskiem zasilania i użytkownikiem).
pub fn session_menu_rect(bar: Rect, theme: &Theme) -> Rect {
    let p = power_rect(bar, theme);
    let pad = 6.0;
    let w = 210.0;
    let h = 4.0 * POWER_ITEM_H + 2.0 * pad;
    Rect::from_xywh(p.right() - w, bar.bottom() + pad, w, h)
}

/// Wykrywa kliknięcie w pozycję menu sesji.
pub fn session_item_at(bar: Rect, theme: &Theme, p: Point) -> Option<PowerAction> {
    let items = crate::power::items();
    let menu = session_menu_rect(bar, theme);
    let inner = Rect::from_xywh(
        menu.x() + 6.0,
        menu.y() + 6.0,
        menu.width() - 12.0,
        items.len() as f64 * POWER_ITEM_H,
    );
    if !inner.contains(p) {
        return None;
    }
    let i = ((p.y - inner.y()) / POWER_ITEM_H).floor() as usize;
    items.get(i).map(|item| item.action)
}

/// Rysuje menu sesji (zablokuj, wyloguj, restart, wyłącz).
pub fn paint_session_menu(ctx: &mut PaintCtx, bar: Rect, items: &[crate::power::PowerItem]) {
    let t = ctx.theme;
    let menu = session_menu_rect(bar, t);
    ctx.scene
        .fill_rounded_rect(menu, t.radius.medium, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(menu, t.radius.medium, 1.0, t.colors.border);
    let x = menu.x() + 6.0;
    let y = menu.y() + 6.0;
    let w = menu.width() - 12.0;
    for (i, item) in items.iter().enumerate() {
        let row = Rect::from_xywh(x, y + i as f64 * POWER_ITEM_H, w, POWER_ITEM_H);
        let icon_name = match item.action {
            PowerAction::Lock => IconName::Star,
            PowerAction::Logout => IconName::User,
            PowerAction::Reboot => IconName::Clock,
            PowerAction::PowerOff => IconName::Power,
        };
        let isz = 16.0;
        let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - isz / 2.0, isz, isz);
        paint_icon(ctx, icon_name, icon_rect, t.colors.text);

        ctx.label_left(
            icon_rect.right() + 10.0,
            row.center().y,
            &item.label,
            t.typography.body.size,
            weight::NORMAL,
            t.colors.text,
        );
    }
}

/// ── Quick Panel (motyw + Wi-Fi + dźwięk + jasność) ──

/// Prostokąt przycisku otwierającego Quick Panel (na lewo od kropek i przełączników).
pub fn quick_button_rect(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Rect {
    let left = workspace_rects(bar, theme, text, state)
        .first()
        .map(|r| r.x())
        .or_else(|| toggle_rects(bar, theme, text, state).first().map(|r| r.x()))
        .unwrap_or(bar.center().x);
    Rect::from_xywh(
        left - theme.spacing.sm - 24.0,
        bar.center().y - 12.0,
        24.0,
        24.0,
    )
}

/// Szerokość panelu szybkich ustawień.
pub const QUICK_W: f64 = 400.0;
/// Wysokość panelu szybkich ustawień.
pub const QUICK_H: f64 = 492.0;

/// Prostokąt panelu szybkich ustawień.
pub fn quick_panel_rect(bar: Rect, theme: &Theme) -> Rect {
    let right = power_rect(bar, theme).right();
    Rect::from_xywh(
        (right - QUICK_W).max(bar.x() + 8.0),
        bar.bottom() + 6.0,
        QUICK_W,
        QUICK_H,
    )
}

/// Rozmieszczenie elementów Quick Panelu (spójne dla rysowania i trafień).
pub struct QuickLayout {
    pub header: Rect,
    pub avatar: Rect,
    /// `[0]` = przełącznik jasny/ciemny, `[1]` = menu zasilania.
    pub header_buttons: [Rect; 2],
    /// Wi-Fi, Bluetooth, Samolot, Nie przeszkadzać, Motyw, Zrzut ekranu.
    pub tiles: [Rect; 6],
    pub settings: Rect,
    pub mute: Rect,
    pub volume: Rect,
    pub brightness: Rect,
    pub media: Rect,
    pub media_art: Rect,
    /// Poprzedni, play/pauza, następny.
    pub media_buttons: [Rect; 3],
    /// Zablokuj ekran, wyloguj, uruchom ponownie.
    pub session: [Rect; 3],
}

pub fn quick_layout(rect: Rect, theme: &Theme) -> QuickLayout {
    let pad = theme.spacing.lg;
    let x = rect.x() + pad;
    let w = rect.width() - 2.0 * pad;
    let mut y = rect.y() + pad;

    let header = Rect::from_xywh(x, y, w, 44.0);
    let avatar = Rect::from_xywh(x, y, 44.0, 44.0);
    let power_btn = Rect::from_xywh(x + w - 36.0, y + 4.0, 36.0, 36.0);
    let theme_btn = Rect::from_xywh(power_btn.x() - 8.0 - 36.0, y + 4.0, 36.0, 36.0);
    y = header.bottom() + 14.0;

    let gap = 8.0;
    let tw = (w - 2.0 * gap) / 3.0;
    let th = 66.0;
    let mut tiles = [Rect::ZERO; 6];
    for row in 0..2 {
        for col in 0..3 {
            tiles[row * 3 + col] = Rect::from_xywh(
                x + col as f64 * (tw + gap),
                y + row as f64 * (th + gap),
                tw,
                th,
            );
        }
    }
    y += 2.0 * th + gap + 14.0;

    let settings = Rect::from_xywh(x, y, w, 108.0);
    let inset = 12.0;
    let track = Rect::from_xywh(
        settings.x() + inset,
        settings.y() + 34.0,
        settings.width() - 2.0 * inset,
        6.0,
    );
    let mute = Rect::from_xywh(
        settings.right() - inset - 18.0,
        settings.y() + 9.0,
        18.0,
        18.0,
    );
    let brightness = Rect::from_xywh(
        settings.x() + inset,
        settings.y() + 90.0,
        settings.width() - 2.0 * inset,
        6.0,
    );
    y = settings.bottom() + 12.0;

    let media = Rect::from_xywh(x, y, w, 72.0);
    let art = Rect::from_xywh(media.x() + 12.0, media.center().y - 24.0, 48.0, 48.0);
    let next = Rect::from_xywh(
        media.right() - 12.0 - 22.0,
        media.center().y - 11.0,
        22.0,
        22.0,
    );
    let play = Rect::from_xywh(next.x() - 8.0 - 32.0, media.center().y - 16.0, 32.0, 32.0);
    let prev = Rect::from_xywh(play.x() - 8.0 - 22.0, media.center().y - 11.0, 22.0, 22.0);
    y = media.bottom() + 12.0;

    let sw = (w - 2.0 * gap) / 3.0;
    let session = [
        Rect::from_xywh(x, y, sw, 40.0),
        Rect::from_xywh(x + sw + gap, y, sw, 40.0),
        Rect::from_xywh(x + 2.0 * (sw + gap), y, sw, 40.0),
    ];

    QuickLayout {
        header,
        avatar,
        header_buttons: [theme_btn, power_btn],
        tiles,
        settings,
        mute,
        volume: track,
        brightness,
        media,
        media_art: art,
        media_buttons: [prev, play, next],
        session,
    }
}

/// Trafienie w Quick Panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickHit {
    /// Przełącznik motywu w nagłówku (jasny → ciemny → auto).
    Theme,
    /// Menu zasilania w nagłówku.
    Power,
    Wifi,
    Bluetooth,
    Airplane,
    Dnd,
    /// Ciepła korekta gamma (night light).
    NightLight,
    /// Oszczędzanie energii.
    PowerSaver,
    Mute,
    Volume(u8),
    Brightness(u8),
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    Lock,
    Logout,
    Reboot,
}

fn slider_value(track: Rect, p: Point) -> Option<u8> {
    let r = Rect::from_xywh(
        track.x() - 6.0,
        track.y() - 9.0,
        track.width() + 12.0,
        track.height() + 18.0,
    );
    if !r.contains(p) {
        return None;
    }
    let f = ((p.x - track.x()) / track.width().max(1.0)).clamp(0.0, 1.0);
    Some((f * 100.0).round() as u8)
}

pub fn quick_hit(rect: Rect, theme: &Theme, p: Point) -> Option<QuickHit> {
    let l = quick_layout(rect, theme);
    if l.header_buttons[0].contains(p) {
        return Some(QuickHit::Theme);
    }
    if l.header_buttons[1].contains(p) {
        return Some(QuickHit::Power);
    }
    let tile_hits = [
        QuickHit::Wifi,
        QuickHit::Bluetooth,
        QuickHit::Airplane,
        QuickHit::Dnd,
        QuickHit::NightLight,
        QuickHit::PowerSaver,
    ];
    for (r, hit) in l.tiles.iter().zip(tile_hits) {
        if r.contains(p) {
            return Some(hit);
        }
    }
    if l.mute.contains(p) {
        return Some(QuickHit::Mute);
    }
    if let Some(v) = slider_value(l.volume, p) {
        return Some(QuickHit::Volume(v));
    }
    if let Some(v) = slider_value(l.brightness, p) {
        return Some(QuickHit::Brightness(v));
    }
    if l.media_buttons[0].contains(p) {
        return Some(QuickHit::MediaPrevious);
    }
    if l.media_buttons[1].contains(p) {
        return Some(QuickHit::MediaPlayPause);
    }
    if l.media_buttons[2].contains(p) {
        return Some(QuickHit::MediaNext);
    }
    if l.session[0].contains(p) {
        return Some(QuickHit::Lock);
    }
    if l.session[1].contains(p) {
        return Some(QuickHit::Logout);
    }
    if l.session[2].contains(p) {
        return Some(QuickHit::Reboot);
    }
    None
}

fn paint_thin_slider(ctx: &mut PaintCtx, track: Rect, pct: u8, enabled: bool) {
    let t = ctx.theme;
    let radius = track.height() / 2.0;
    ctx.scene.fill_rounded_rect(track, radius, t.colors.view_bg);
    let fill_w = (track.width() * (pct as f64 * 0.01)).clamp(0.0, track.width());
    if fill_w > 0.0 {
        let fill = Rect::from_xywh(track.x(), track.y(), fill_w, track.height());
        let col = if enabled {
            t.colors.accent
        } else {
            t.colors.dim_text
        };
        ctx.scene.fill_rounded_rect(fill, radius, col);
    }
    if !enabled {
        return;
    }
    let r = 7.0;
    let thumb_x = (track.x() + fill_w).clamp(track.x() + r, track.right() - r);
    let thumb = Rect::from_xywh(thumb_x - r, track.center().y - r, 2.0 * r, 2.0 * r);
    ctx.scene.fill_rounded_rect(thumb, r, Color::WHITE);
    ctx.scene
        .stroke_rounded_rect(thumb, r, 1.0, t.colors.border);
}

/// Największy rozmiar czcionki (od `start` w dół), przy którym tekst mieści się w `max_w`.
fn fitted_size(
    text: &str,
    max_w: f64,
    start: f64,
    min: f64,
    weight: u16,
    engine: &TextEngine,
) -> f64 {
    let mut size = start;
    while size > min && engine.measure(text, size, weight).width > max_w {
        size -= 0.5;
    }
    size
}

/// Jeden kafelek Quick Panelu (tytuł, opcjonalna druga linia, podtytuł).
fn paint_quick_tile(
    ctx: &mut PaintCtx,
    rect: Rect,
    title: &str,
    second: Option<&str>,
    subtitle: &str,
    active: bool,
) {
    let t = ctx.theme;
    let (bg, fg, sub_fg) = if active {
        (
            t.colors.accent,
            t.colors.accent_fg,
            t.colors.accent_fg.scale_alpha(0.82),
        )
    } else {
        (t.colors.shade, t.colors.text, t.colors.dim_text)
    };
    ctx.scene.fill_rounded_rect(rect, t.radius.medium, bg);
    if !active {
        ctx.scene
            .stroke_rounded_rect(rect, t.radius.medium, 1.0, t.colors.border);
    }

    let inner = Rect::from_xywh(
        rect.x() + 10.0,
        rect.y(),
        (rect.width() - 20.0).max(0.0),
        rect.height(),
    );
    ctx.scene.push_clip(inner);
    let title_sz = t.typography.body.size - 1.0;
    let sub_sz = t.typography.caption.size - 1.0;
    if let Some(second) = second {
        let size = fitted_size(
            title,
            inner.width(),
            title_sz - 1.0,
            10.0,
            weight::BOLD,
            ctx.text,
        );
        let size = fitted_size(second, inner.width(), size, 10.0, weight::BOLD, ctx.text);
        let first = ellipsize(title, inner.width(), size, weight::BOLD, ctx.text);
        let second = ellipsize(second, inner.width(), size, weight::BOLD, ctx.text);
        ctx.label_left(inner.x(), rect.y() + 16.0, &first, size, weight::BOLD, fg);
        ctx.label_left(inner.x(), rect.y() + 32.0, &second, size, weight::BOLD, fg);
        let sub_size = fitted_size(
            subtitle,
            inner.width(),
            sub_sz,
            9.5,
            weight::NORMAL,
            ctx.text,
        );
        let subtitle = ellipsize(subtitle, inner.width(), sub_size, weight::NORMAL, ctx.text);
        ctx.label_left(
            inner.x(),
            rect.y() + 52.0,
            &subtitle,
            sub_size,
            weight::NORMAL,
            sub_fg,
        );
    } else {
        let size = fitted_size(title, inner.width(), title_sz, 10.0, weight::BOLD, ctx.text);
        let sub_size = fitted_size(
            subtitle,
            inner.width(),
            sub_sz,
            9.5,
            weight::NORMAL,
            ctx.text,
        );
        let title = ellipsize(title, inner.width(), size, weight::BOLD, ctx.text);
        let subtitle = ellipsize(subtitle, inner.width(), sub_size, weight::NORMAL, ctx.text);
        ctx.label_left(inner.x(), rect.y() + 25.0, &title, size, weight::BOLD, fg);
        ctx.label_left(
            inner.x(),
            rect.y() + 46.0,
            &subtitle,
            sub_size,
            weight::NORMAL,
            sub_fg,
        );
    }
    ctx.scene.pop_clip();
}

/// Odtwarzacz pokazywany w Quick Panelu.
#[derive(Debug, Clone, Copy)]
pub struct QuickMedia<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
}

/// Dane do narysowania Quick Panelu.
pub struct QuickData<'a> {
    pub user: &'a str,
    pub preference: ThemePreference,
    /// Czy rozwiązany motyw jest ciemny (dla wyróżnienia kafelka „Motyw”).
    pub dark: bool,
    pub wifi_on: bool,
    pub ssid: Option<&'a str>,
    /// `None` = brak adaptera Bluetooth.
    pub bluetooth: Option<(bool, usize)>,
    pub airplane: bool,
    pub dnd: bool,
    /// Siła night lightu (0 = wyłączony).
    pub night_light: u8,
    /// Czy kompozytor ma sprzętową tablicę gamma.
    pub gamma_available: bool,
    pub power_saver: bool,
    /// Czy oszczędzanie działa przez limit kompozytora (brak `power-profiles-daemon`).
    pub power_fallback: bool,
    pub volume: u8,
    pub muted: bool,
    /// `None` = brak podświetlenia (np. komputer stacjonarny).
    pub brightness: Option<u8>,
    pub sink: Option<&'a str>,
    pub battery: Option<BatteryStatus>,
    pub media: Option<QuickMedia<'a>>,
}

/// Rysuje Quick Panel: profil, kafelki szybkich akcji, dźwięk/jasność, media i sesję.
pub fn paint_quick_panel(ctx: &mut PaintCtx, rect: Rect, data: &QuickData) {
    let t = ctx.theme;
    let l = quick_layout(rect, t);
    ctx.scene
        .fill_rounded_rect(rect, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.large, 1.0, t.colors.border);

    // ── Nagłówek: awatar, użytkownik, bateria, przyciski ──
    ctx.scene
        .fill_rounded_rect(l.avatar, l.avatar.width() / 2.0, t.colors.accent);
    let initial = data
        .user
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "U".to_owned());
    ctx.label_center(
        l.avatar.center().x,
        l.avatar.center().y,
        &initial,
        t.typography.heading.size,
        weight::BOLD,
        t.colors.accent_fg,
    );

    let text_x = l.avatar.right() + 12.0;
    let text_w = (l.header_buttons[0].x() - 8.0 - text_x).max(0.0);
    let name = ellipsize(
        data.user,
        text_w,
        t.typography.heading.size,
        weight::BOLD,
        ctx.text,
    );
    ctx.label_left(
        text_x,
        l.header.y() + 14.0,
        &name,
        t.typography.heading.size,
        weight::BOLD,
        t.colors.text,
    );

    let subtitle = match data.battery {
        None => "Zasilanie sieciowe".to_owned(),
        Some(b) if b.charging => format!("Bateria {}% · ładuje", b.percent),
        Some(b) => match b.minutes {
            Some(m) if m > 0 => format!("Bateria {}% · {} h {} min", b.percent, m / 60, m % 60),
            _ => format!("Bateria {}%", b.percent),
        },
    };
    let subtitle = ellipsize(
        &subtitle,
        text_w,
        t.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        text_x,
        l.header.y() + 32.0,
        &subtitle,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    for (i, (r, icon)) in l
        .header_buttons
        .iter()
        .zip([
            match data.preference {
                ThemePreference::Light => IconName::Brightness,
                ThemePreference::Dark => IconName::Moon,
                ThemePreference::System => IconName::Monitor,
            },
            IconName::Power,
        ])
        .enumerate()
    {
        ctx.scene
            .fill_rounded_rect(*r, t.radius.medium, t.colors.shade);
        let sz = 18.0;
        // Ciemny motyw zaznaczamy akcentem — od razu widać stan przełącznika.
        let fg = if i == 0 && data.dark {
            t.colors.accent
        } else {
            t.colors.text
        };
        paint_icon(
            ctx,
            icon,
            Rect::from_xywh(r.center().x - sz / 2.0, r.center().y - sz / 2.0, sz, sz),
            fg,
        );
    }

    // ── Kafelki ──
    let bt_subtitle = match data.bluetooth {
        None => "Brak adaptera".to_owned(),
        Some((false, _)) => "Wyłączony".to_owned(),
        Some((true, 0)) => "Włączony".to_owned(),
        Some((true, 1)) => "1 urządzenie".to_owned(),
        Some((true, n @ 2..=4)) => format!("{n} urządzenia"),
        Some((true, n)) => format!("{n} urządzeń"),
    };
    let bt_active = matches!(data.bluetooth, Some((true, _)));
    let night_on = data.gamma_available && data.night_light > 0;
    let night_subtitle = if !data.gamma_available {
        "Niedostępny".to_owned()
    } else if night_on {
        format!("Włączony · {}%", data.night_light)
    } else {
        "Wyłączony".to_owned()
    };
    let power_subtitle = if data.power_saver {
        if data.power_fallback {
            "Włączone · 30 FPS".to_owned()
        } else {
            "Włączone".to_owned()
        }
    } else {
        "Wyłączone".to_owned()
    };
    let tiles: [(&str, Option<&str>, String, bool); 6] = [
        (
            "Wi-Fi",
            None,
            if data.wifi_on {
                data.ssid.unwrap_or("Włączone").to_owned()
            } else {
                "Wyłączone".to_owned()
            },
            data.wifi_on,
        ),
        ("Bluetooth", None, bt_subtitle, bt_active),
        (
            "Samolot",
            None,
            if data.airplane {
                "Włączony".to_owned()
            } else {
                "Wyłączony".to_owned()
            },
            data.airplane,
        ),
        (
            "Nie",
            Some("przeszkadzać"),
            if data.dnd {
                "Włączone".to_owned()
            } else {
                "Wyłączone".to_owned()
            },
            data.dnd,
        ),
        ("Tryb nocny", None, night_subtitle, night_on),
        ("Oszczędzanie", None, power_subtitle, data.power_saver),
    ];
    for (r, (title, second, subtitle, active)) in l.tiles.iter().zip(tiles.iter()) {
        paint_quick_tile(ctx, *r, title, *second, subtitle, *active);
    }

    // ── Dźwięk i jasność ──
    ctx.scene
        .fill_rounded_rect(l.settings, t.radius.medium, t.colors.shade);
    ctx.label_left(
        l.settings.x() + 12.0,
        l.settings.y() + 18.0,
        "Dźwięk i głośność",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );
    paint_icon(
        ctx,
        IconName::Volume,
        Rect::from_xywh(l.mute.x() + 1.0, l.mute.y() + 1.0, 16.0, 16.0),
        if data.muted {
            t.colors.dim_text
        } else {
            t.colors.text
        },
    );
    ctx.label_right(
        l.mute.x() - 8.0,
        l.mute.center().y,
        &format!("{}%", data.volume),
        t.typography.caption.size,
        weight::MEDIUM,
        if data.muted {
            t.colors.dim_text
        } else {
            t.colors.text
        },
    );
    paint_thin_slider(ctx, l.volume, data.volume, !data.muted);

    let sink = match data.sink {
        Some(s) => format!("Wyjście: {s}"),
        None => "Wyjście: domyślne".to_owned(),
    };
    let sink = ellipsize(
        &sink,
        l.settings.width() - 24.0,
        t.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        l.settings.x() + 12.0,
        l.settings.y() + 58.0,
        &sink,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    let brightness = data.brightness;
    let brightness_fg = if brightness.is_some() {
        t.colors.text
    } else {
        t.colors.dim_text
    };
    ctx.label_left(
        l.settings.x() + 12.0,
        l.settings.y() + 78.0,
        "Jasność",
        t.typography.body.size,
        weight::BOLD,
        brightness_fg,
    );
    ctx.label_right(
        l.settings.right() - 12.0,
        l.settings.y() + 78.0,
        &brightness
            .map(|v| format!("{v}%"))
            .unwrap_or_else(|| "—".to_owned()),
        t.typography.caption.size,
        weight::MEDIUM,
        brightness_fg,
    );
    paint_thin_slider(
        ctx,
        l.brightness,
        brightness.unwrap_or(0),
        brightness.is_some(),
    );

    // ── Odtwarzacz ──
    ctx.scene
        .fill_rounded_rect(l.media, t.radius.medium, t.colors.shade);
    ctx.scene.fill_rounded_rect(
        l.media_art,
        t.radius.small,
        t.colors.accent.scale_alpha(0.18),
    );
    paint_icon(
        ctx,
        IconName::Music,
        Rect::from_xywh(
            l.media_art.center().x - 11.0,
            l.media_art.center().y - 11.0,
            22.0,
            22.0,
        ),
        t.colors.accent,
    );
    let has_media = data.media.is_some();
    let (title, artist) = match data.media {
        Some(m) => (m.title, m.artist),
        None => ("Nic nie jest odtwarzane", "Uruchom odtwarzacz"),
    };
    let media_text_x = l.media_art.right() + 12.0;
    let media_text_w = (l.media_buttons[0].x() - 8.0 - media_text_x).max(0.0);
    let title = ellipsize(
        title,
        media_text_w,
        t.typography.body.size,
        weight::MEDIUM,
        ctx.text,
    );
    let artist = ellipsize(
        artist,
        media_text_w,
        t.typography.caption.size,
        weight::NORMAL,
        ctx.text,
    );
    ctx.label_left(
        media_text_x,
        l.media.y() + 28.0,
        &title,
        t.typography.body.size,
        weight::MEDIUM,
        if has_media {
            t.colors.text
        } else {
            t.colors.dim_text
        },
    );
    ctx.label_left(
        media_text_x,
        l.media.y() + 47.0,
        &artist,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    let icon_fg = if has_media {
        t.colors.text
    } else {
        t.colors.dim_text
    };
    for (r, icon) in [
        (l.media_buttons[0], IconName::ChevronLeft),
        (l.media_buttons[2], IconName::ChevronRight),
    ] {
        paint_icon(
            ctx,
            icon,
            Rect::from_xywh(r.center().x - 9.0, r.center().y - 9.0, 18.0, 18.0),
            icon_fg,
        );
    }
    let play = l.media_buttons[1];
    ctx.scene.fill_rounded_rect(
        play,
        play.width() / 2.0,
        if has_media {
            t.colors.accent
        } else {
            t.colors.shade
        },
    );
    let playing = data.media.map(|m| m.playing).unwrap_or(false);
    paint_icon(
        ctx,
        if playing {
            IconName::Pause
        } else {
            IconName::Play
        },
        Rect::from_xywh(play.center().x - 8.0, play.center().y - 8.0, 16.0, 16.0),
        if has_media {
            t.colors.accent_fg
        } else {
            t.colors.dim_text
        },
    );

    // ── Sesja ──
    for (r, label) in l
        .session
        .iter()
        .zip(["Zablokuj ekran", "Wyloguj", "Uruchom ponownie"])
    {
        ctx.scene
            .fill_rounded_rect(*r, t.radius.medium, t.colors.shade);
        let size = t.typography.caption.size - 1.0;
        let label = ellipsize(label, r.width() - 8.0, size, weight::MEDIUM, ctx.text);
        ctx.label_center(
            r.center().x,
            r.center().y,
            &label,
            size,
            weight::MEDIUM,
            t.colors.text,
        );
    }
}

// ── Popup Dźwięk / Głośność ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeAction {
    SetLevel(u8),
    StepDown,
    StepUp,
    ToggleMute,
}

pub fn volume_popup_rect(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Rect {
    let w = 280.0;
    let h = 160.0;
    let pad = 6.0;
    let toggles = toggle_rects(bar, theme, text, state);
    let vol_idx = state
        .toggles
        .iter()
        .position(|t| t.action == ToggleAction::Volume);
    let right = match vol_idx.and_then(|i| toggles.get(i)) {
        Some(r) => r.right(),
        None => bar.right() - 80.0,
    };
    let x = (right - w).clamp(10.0, (bar.right() - w - 10.0).max(10.0));
    Rect::from_xywh(x, bar.bottom() + pad, w, h)
}

pub fn paint_volume_popup(ctx: &mut PaintCtx, rect: Rect, pct: u8, muted: bool) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.large, 1.0, t.colors.border);

    let pad = t.spacing.lg;
    let icon_sz = 18.0;
    let icon_rect = Rect::from_xywh(rect.x() + pad, rect.y() + pad, icon_sz, icon_sz);
    paint_icon(
        ctx,
        IconName::Volume,
        icon_rect,
        if muted {
            t.colors.dim_text
        } else {
            t.colors.accent
        },
    );

    ctx.label_left(
        icon_rect.right() + t.spacing.sm,
        icon_rect.center().y,
        "Dźwięk i głośność",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let status_str = if muted {
        format!("{pct}% (wyciszono)")
    } else {
        format!("{pct}%")
    };
    ctx.label_right(
        rect.right() - pad,
        icon_rect.center().y,
        &status_str,
        t.typography.caption.size,
        weight::MEDIUM,
        if muted {
            t.colors.dim_text
        } else {
            t.colors.text
        },
    );

    let slider_y = icon_rect.bottom() + t.spacing.lg;
    let slider_track = Rect::from_xywh(rect.x() + pad, slider_y, rect.width() - 2.0 * pad, 12.0);
    ctx.scene
        .fill_rounded_rect(slider_track, 6.0, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(slider_track, 6.0, 1.0, t.colors.border);

    let fill_w = (slider_track.width() * (pct as f64 / 100.0)).clamp(0.0, slider_track.width());
    if fill_w > 0.0 {
        let fill_rect = Rect::from_xywh(
            slider_track.x(),
            slider_track.y(),
            fill_w,
            slider_track.height(),
        );
        let fill_col = if muted {
            t.colors.dim_text
        } else {
            t.colors.accent
        };
        ctx.scene.fill_rounded_rect(fill_rect, 6.0, fill_col);
    }

    // Suwak na pasku głośności (thumb)
    let thumb_x =
        (slider_track.x() + fill_w).clamp(slider_track.x() + 8.0, slider_track.right() - 8.0);
    let thumb_rect = Rect::from_xywh(thumb_x - 8.0, slider_track.center().y - 8.0, 16.0, 16.0);
    ctx.scene.fill_rounded_rect(thumb_rect, 8.0, Color::WHITE);
    ctx.scene
        .stroke_rounded_rect(thumb_rect, 8.0, 1.5, t.colors.border);

    let btn_y = slider_track.bottom() + t.spacing.lg;
    let btn_h = 32.0;

    let minus_rect = Rect::from_xywh(rect.x() + pad, btn_y, 44.0, btn_h);
    ctx.scene
        .fill_rounded_rect(minus_rect, t.radius.small, t.colors.shade);
    ctx.scene
        .stroke_rounded_rect(minus_rect, t.radius.small, 1.0, t.colors.border);
    ctx.label_center(
        minus_rect.center().x,
        minus_rect.center().y,
        "−",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let plus_rect = Rect::from_xywh(minus_rect.right() + t.spacing.sm, btn_y, 44.0, btn_h);
    ctx.scene
        .fill_rounded_rect(plus_rect, t.radius.small, t.colors.shade);
    ctx.scene
        .stroke_rounded_rect(plus_rect, t.radius.small, 1.0, t.colors.border);
    ctx.label_center(
        plus_rect.center().x,
        plus_rect.center().y,
        "+",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let mute_w = rect.right() - pad - (plus_rect.right() + t.spacing.md);
    let mute_rect = Rect::from_xywh(plus_rect.right() + t.spacing.md, btn_y, mute_w, btn_h);
    let (mbg, mfg, mlabel) = if muted {
        (t.colors.accent, t.colors.accent_fg, "Włącz dźwięk")
    } else {
        (t.colors.shade, t.colors.text, "Wycisz")
    };
    ctx.scene.fill_rounded_rect(mute_rect, t.radius.small, mbg);
    if !muted {
        ctx.scene
            .stroke_rounded_rect(mute_rect, t.radius.small, 1.0, t.colors.border);
    }
    ctx.label_center(
        mute_rect.center().x,
        mute_rect.center().y,
        mlabel,
        t.typography.caption.size,
        weight::MEDIUM,
        mfg,
    );
}

pub fn volume_popup_hit(rect: Rect, theme: &Theme, p: Point) -> Option<VolumeAction> {
    if !rect.contains(p) {
        return None;
    }
    let pad = theme.spacing.lg;
    let icon_sz = 18.0;
    let slider_y = rect.y() + pad + icon_sz + theme.spacing.lg;
    let slider_hit = Rect::from_xywh(
        rect.x() + pad,
        slider_y - 6.0,
        rect.width() - 2.0 * pad,
        24.0,
    );
    if slider_hit.contains(p) {
        let frac = ((p.x - slider_hit.x()) / slider_hit.width()).clamp(0.0, 1.0);
        return Some(VolumeAction::SetLevel((frac * 100.0).round() as u8));
    }

    let btn_y = slider_y + 12.0 + theme.spacing.lg;
    let btn_h = 32.0;
    let minus_rect = Rect::from_xywh(rect.x() + pad, btn_y, 44.0, btn_h);
    if minus_rect.contains(p) {
        return Some(VolumeAction::StepDown);
    }

    let plus_rect = Rect::from_xywh(minus_rect.right() + theme.spacing.sm, btn_y, 44.0, btn_h);
    if plus_rect.contains(p) {
        return Some(VolumeAction::StepUp);
    }

    let mute_w = rect.right() - pad - (plus_rect.right() + theme.spacing.md);
    let mute_rect = Rect::from_xywh(plus_rect.right() + theme.spacing.md, btn_y, mute_w, btn_h);
    if mute_rect.contains(p) {
        return Some(VolumeAction::ToggleMute);
    }

    None
}

// ── Popup Sieć / Wi-Fi ──

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiAction {
    TogglePower,
    SelectNetwork(String),
}

pub fn wifi_popup_rect(bar: Rect, theme: &Theme, text: &TextEngine, state: &PanelState) -> Rect {
    let w = 290.0;
    let h = 270.0;
    let pad = 6.0;
    let toggles = toggle_rects(bar, theme, text, state);
    let wifi_idx = state
        .toggles
        .iter()
        .position(|t| t.action == ToggleAction::Wifi);
    let right = match wifi_idx.and_then(|i| toggles.get(i)) {
        Some(r) => r.right(),
        None => bar.right() - 140.0,
    };
    let x = (right - w).clamp(10.0, (bar.right() - w - 10.0).max(10.0));
    Rect::from_xywh(x, bar.bottom() + pad, w, h)
}

pub fn paint_wifi_popup(
    ctx: &mut PaintCtx,
    rect: Rect,
    wifi_enabled: bool,
    connected_ssid: Option<&str>,
    networks: &[WifiNetwork],
) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.large, 1.0, t.colors.border);

    let pad = t.spacing.lg;
    let icon_sz = 18.0;
    let icon_rect = Rect::from_xywh(rect.x() + pad, rect.y() + pad, icon_sz, icon_sz);
    paint_icon(
        ctx,
        IconName::Wifi,
        icon_rect,
        if wifi_enabled {
            t.colors.accent
        } else {
            t.colors.dim_text
        },
    );

    ctx.label_left(
        icon_rect.right() + t.spacing.sm,
        icon_rect.center().y,
        "Sieć Wi-Fi",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let sw_w = 78.0;
    let sw_h = 24.0;
    let sw_rect = Rect::from_xywh(
        rect.right() - pad - sw_w,
        icon_rect.center().y - sw_h / 2.0,
        sw_w,
        sw_h,
    );
    let (sw_bg, sw_fg, sw_lbl) = if wifi_enabled {
        (t.colors.accent, t.colors.accent_fg, "Włączone")
    } else {
        (t.colors.shade, t.colors.dim_text, "Wyłączone")
    };
    ctx.scene.fill_rounded_rect(sw_rect, sw_h / 2.0, sw_bg);
    if !wifi_enabled {
        ctx.scene
            .stroke_rounded_rect(sw_rect, sw_h / 2.0, 1.0, t.colors.border);
    }
    ctx.label_center(
        sw_rect.center().x,
        sw_rect.center().y,
        sw_lbl,
        t.typography.caption.size,
        weight::MEDIUM,
        sw_fg,
    );

    let sep_y = icon_rect.bottom() + t.spacing.md;
    ctx.scene.fill_rect(
        Rect::from_xywh(rect.x() + pad, sep_y, rect.width() - 2.0 * pad, 1.0),
        t.colors.border,
    );

    let list_y = sep_y + t.spacing.md;
    if !wifi_enabled {
        ctx.label_center(
            rect.center().x,
            list_y + 40.0,
            "Wi-Fi jest wyłączone",
            t.typography.body.size,
            weight::NORMAL,
            t.colors.dim_text,
        );
        return;
    }

    if let Some(ssid) = connected_ssid {
        let conn_rect = Rect::from_xywh(rect.x() + pad, list_y, rect.width() - 2.0 * pad, 36.0);
        ctx.scene
            .fill_rounded_rect(conn_rect, t.radius.small, t.colors.accent.scale_alpha(0.12));
        ctx.scene
            .stroke_rounded_rect(conn_rect, t.radius.small, 1.0, t.colors.accent);
        ctx.label_left(
            conn_rect.x() + 10.0,
            conn_rect.center().y,
            &format!("✓ {ssid}"),
            t.typography.body.size,
            weight::MEDIUM,
            t.colors.accent,
        );
        ctx.label_right(
            conn_rect.right() - 10.0,
            conn_rect.center().y,
            "Połączono",
            t.typography.caption.size,
            weight::NORMAL,
            t.colors.accent,
        );
    }

    let items_start_y = if connected_ssid.is_some() {
        list_y + 44.0
    } else {
        list_y
    };

    if networks.is_empty() && connected_ssid.is_none() {
        ctx.label_center(
            rect.center().x,
            items_start_y + 30.0,
            "Brak wykrytych sieci Wi-Fi",
            t.typography.body.size,
            weight::NORMAL,
            t.colors.dim_text,
        );
        return;
    }

    let item_h = 32.0;
    let mut rendered = 0;
    for net in networks.iter() {
        if connected_ssid == Some(net.ssid.as_str()) {
            continue;
        }
        if rendered >= 4 {
            break;
        }
        let row = Rect::from_xywh(
            rect.x() + pad,
            items_start_y + rendered as f64 * item_h,
            rect.width() - 2.0 * pad,
            item_h,
        );
        ctx.scene
            .fill_rounded_rect(row, t.radius.small, t.colors.shade);
        ctx.label_left(
            row.x() + 10.0,
            row.center().y,
            &net.ssid,
            t.typography.body.size,
            weight::NORMAL,
            t.colors.text,
        );
        let sig_str = format!("{}%", net.signal);
        ctx.label_right(
            row.right() - 10.0,
            row.center().y,
            &sig_str,
            t.typography.caption.size,
            weight::NORMAL,
            t.colors.dim_text,
        );
        rendered += 1;
    }
}

pub fn wifi_popup_hit(
    rect: Rect,
    theme: &Theme,
    p: Point,
    connected_ssid: Option<&str>,
    networks: &[WifiNetwork],
) -> Option<WifiAction> {
    if !rect.contains(p) {
        return None;
    }
    let pad = theme.spacing.lg;
    let icon_sz = 18.0;
    let sw_w = 78.0;
    let sw_h = 24.0;
    let sw_rect = Rect::from_xywh(
        rect.right() - pad - sw_w,
        rect.y() + pad + icon_sz / 2.0 - sw_h / 2.0,
        sw_w,
        sw_h,
    );
    if sw_rect.contains(p) {
        return Some(WifiAction::TogglePower);
    }

    let sep_y = rect.y() + pad + icon_sz + theme.spacing.md;
    let list_y = sep_y + theme.spacing.md;
    let items_start_y = if connected_ssid.is_some() {
        list_y + 44.0
    } else {
        list_y
    };
    let item_h = 32.0;
    let mut rendered = 0;
    for net in networks.iter() {
        if connected_ssid == Some(net.ssid.as_str()) {
            continue;
        }
        if rendered >= 4 {
            break;
        }
        let row = Rect::from_xywh(
            rect.x() + pad,
            items_start_y + rendered as f64 * item_h,
            rect.width() - 2.0 * pad,
            item_h,
        );
        if row.contains(p) {
            return Some(WifiAction::SelectNetwork(net.ssid.clone()));
        }
        rendered += 1;
    }

    None
}

// ── Popup Kalendarz i Zegar ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarAction {
    PrevMonth,
    NextMonth,
    SelectDay(u32),
}

pub fn calendar_popup_rect(bar: Rect, _theme: &Theme) -> Rect {
    let w = 320.0;
    let h = 350.0;
    let pad = 6.0;
    let x = (bar.center().x - w / 2.0).clamp(10.0, (bar.right() - w - 10.0).max(10.0));
    Rect::from_xywh(x, bar.bottom() + pad, w, h)
}

pub const MONTH_NAMES: [&str; 12] = [
    "Styczeń",
    "Luty",
    "Marzec",
    "Kwiecień",
    "Maj",
    "Czerwiec",
    "Lipiec",
    "Sierpień",
    "Wrzesień",
    "Październik",
    "Listopad",
    "Grudzień",
];

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

pub fn first_weekday_of_month(year: i64, month: u32) -> usize {
    let t = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let m = month as usize;
    let d = 1;
    let w = (y + y / 4 - y / 100 + y / 400 + t[m - 1] + d) % 7;
    if w == 0 {
        6
    } else {
        (w - 1) as usize
    }
}

pub fn paint_calendar_popup(
    ctx: &mut PaintCtx,
    rect: Rect,
    clock_str: &str,
    date_str: &str,
    view_year: i64,
    view_month: u32,
    today_year: i64,
    today_month: u32,
    today_day: u32,
    selected_day: Option<u32>,
) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.large, 1.0, t.colors.border);

    let pad = t.spacing.lg;
    ctx.label_center(
        rect.center().x,
        rect.y() + pad + 14.0,
        clock_str,
        24.0,
        weight::BOLD,
        t.colors.text,
    );
    ctx.label_center(
        rect.center().x,
        rect.y() + pad + 38.0,
        date_str,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    let sep_y = rect.y() + pad + 54.0;
    ctx.scene.fill_rect(
        Rect::from_xywh(rect.x() + pad, sep_y, rect.width() - 2.0 * pad, 1.0),
        t.colors.border,
    );

    let nav_y = sep_y + t.spacing.md;
    let arrow_btn_sz = 26.0;

    let prev_btn = Rect::from_xywh(rect.x() + pad, nav_y, arrow_btn_sz, arrow_btn_sz);
    ctx.scene
        .fill_rounded_rect(prev_btn, t.radius.small, t.colors.shade);
    ctx.label_center(
        prev_btn.center().x,
        prev_btn.center().y,
        "‹",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let next_btn = Rect::from_xywh(
        rect.right() - pad - arrow_btn_sz,
        nav_y,
        arrow_btn_sz,
        arrow_btn_sz,
    );
    ctx.scene
        .fill_rounded_rect(next_btn, t.radius.small, t.colors.shade);
    ctx.label_center(
        next_btn.center().x,
        next_btn.center().y,
        "›",
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let month_idx = (view_month.saturating_sub(1) as usize).min(11);
    let title = format!("{} {}", MONTH_NAMES[month_idx], view_year);
    ctx.label_center(
        rect.center().x,
        nav_y + arrow_btn_sz / 2.0,
        &title,
        t.typography.body.size,
        weight::BOLD,
        t.colors.text,
    );

    let grid_x = rect.x() + pad;
    let grid_w = rect.width() - 2.0 * pad;
    let cell_w = grid_w / 7.0;
    let weekdays = ["Pn", "Wt", "Śr", "Cz", "Pt", "So", "Nd"];
    let day_names_y = nav_y + arrow_btn_sz + t.spacing.md;

    for (col, wname) in weekdays.iter().enumerate() {
        let cx = grid_x + col as f64 * cell_w + cell_w / 2.0;
        ctx.label_center(
            cx,
            day_names_y + 8.0,
            wname,
            t.typography.caption.size,
            weight::BOLD,
            t.colors.dim_text,
        );
    }

    let grid_start_y = day_names_y + 24.0;
    let cell_h = 28.0;
    let first_weekday = first_weekday_of_month(view_year, view_month);
    let total_days = days_in_month(view_year, view_month);

    for day in 1..=total_days {
        let cell_idx = first_weekday + (day - 1) as usize;
        let col = cell_idx % 7;
        let row = cell_idx / 7;
        let cx = grid_x + col as f64 * cell_w + cell_w / 2.0;
        let cy = grid_start_y + row as f64 * cell_h + cell_h / 2.0;
        let cell_rect = Rect::from_xywh(cx - 13.0, cy - 13.0, 26.0, 26.0);

        let is_today = view_year == today_year && view_month == today_month && day == today_day;
        let is_selected = selected_day == Some(day);

        if is_today {
            ctx.scene
                .fill_rounded_rect(cell_rect, 13.0, t.colors.accent);
            ctx.label_center(
                cx,
                cy,
                &day.to_string(),
                t.typography.caption.size,
                weight::BOLD,
                t.colors.accent_fg,
            );
        } else if is_selected {
            ctx.scene.fill_rounded_rect(cell_rect, 13.0, t.colors.shade);
            ctx.scene
                .stroke_rounded_rect(cell_rect, 13.0, 1.5, t.colors.accent);
            ctx.label_center(
                cx,
                cy,
                &day.to_string(),
                t.typography.caption.size,
                weight::BOLD,
                t.colors.text,
            );
        } else {
            ctx.label_center(
                cx,
                cy,
                &day.to_string(),
                t.typography.caption.size,
                weight::NORMAL,
                t.colors.text,
            );
        }
    }
}

pub fn calendar_popup_hit(
    rect: Rect,
    theme: &Theme,
    p: Point,
    view_year: i64,
    view_month: u32,
) -> Option<CalendarAction> {
    if !rect.contains(p) {
        return None;
    }
    let pad = theme.spacing.lg;
    let sep_y = rect.y() + pad + 54.0;
    let nav_y = sep_y + theme.spacing.md;
    let arrow_btn_sz = 26.0;

    let prev_btn = Rect::from_xywh(rect.x() + pad, nav_y, arrow_btn_sz, arrow_btn_sz);
    if prev_btn.contains(p) {
        return Some(CalendarAction::PrevMonth);
    }

    let next_btn = Rect::from_xywh(
        rect.right() - pad - arrow_btn_sz,
        nav_y,
        arrow_btn_sz,
        arrow_btn_sz,
    );
    if next_btn.contains(p) {
        return Some(CalendarAction::NextMonth);
    }

    let day_names_y = nav_y + arrow_btn_sz + theme.spacing.md;
    let grid_start_y = day_names_y + 24.0;
    let grid_x = rect.x() + pad;
    let grid_w = rect.width() - 2.0 * pad;
    let cell_w = grid_w / 7.0;
    let cell_h = 28.0;

    let first_weekday = first_weekday_of_month(view_year, view_month);
    let total_days = days_in_month(view_year, view_month);

    for day in 1..=total_days {
        let cell_idx = first_weekday + (day - 1) as usize;
        let col = cell_idx % 7;
        let row = cell_idx / 7;
        let cx = grid_x + col as f64 * cell_w + cell_w / 2.0;
        let cy = grid_start_y + row as f64 * cell_h + cell_h / 2.0;
        let cell_rect = Rect::from_xywh(cx - 13.0, cy - 13.0, 26.0, 26.0);
        if cell_rect.contains(p) {
            return Some(CalendarAction::SelectDay(day));
        }
    }

    None
}

/// Zwraca prostokąt aktywnego popupu (do testowania kliknięć poza nim).
pub fn popup_bounding_rect(
    popup: ActivePopup,
    bar: Rect,
    larea: Rect,
    theme: &Theme,
    text: &TextEngine,
    state: &PanelState,
) -> Option<Rect> {
    match popup {
        ActivePopup::None => None,
        ActivePopup::Launcher => Some(larea),
        ActivePopup::Session => Some(session_menu_rect(bar, theme)),
        ActivePopup::Wifi => Some(wifi_popup_rect(bar, theme, text, state)),
        ActivePopup::Volume => Some(volume_popup_rect(bar, theme, text, state)),
        ActivePopup::Calendar => Some(calendar_popup_rect(bar, theme)),
        ActivePopup::TaskMenu => None,
        ActivePopup::Quick => Some(quick_panel_rect(bar, theme)),
    }
}

/// Prostokąt przycisku otwierającego dany popup na pasku.
pub fn popup_trigger_rect(
    popup: ActivePopup,
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    state: &PanelState,
) -> Option<Rect> {
    match popup {
        ActivePopup::None => None,
        ActivePopup::Launcher => Some(burger_rect(bar, theme)),
        ActivePopup::Session => Some(power_rect(bar, theme)),
        ActivePopup::Wifi => {
            let toggles = toggle_rects(bar, theme, text, state);
            let idx = state
                .toggles
                .iter()
                .position(|t| t.action == ToggleAction::Wifi)?;
            toggles.get(idx).copied()
        }
        ActivePopup::Volume => {
            let toggles = toggle_rects(bar, theme, text, state);
            let idx = state
                .toggles
                .iter()
                .position(|t| t.action == ToggleAction::Volume)?;
            toggles.get(idx).copied()
        }
        ActivePopup::Calendar => Some(clock_rect(bar, theme, text, state)),
        // Menu taskbara ma własny prostokąt (zależny od klikniętego przycisku).
        ActivePopup::TaskMenu => None,
        ActivePopup::Quick => Some(quick_button_rect(bar, theme, text, state)),
    }
}

/// Ikona dla okna w taskbarze (heurystyka po `app_id` i tytule).
///
/// Protokół `zwlr_foreign_toplevel` nie przekazuje ikony aplikacji, więc dobieramy
/// ją z naszego zestawu — tak jak launcher dopasowuje wpisy `.desktop`.
pub fn task_icon(task: &Task) -> IconName {
    crate::apps::icon_for_hint(&task.app_id, &task.title, None, &[], "")
}

/// Rozmieszczenie przycisków okien (taskbar) w pasku.
///
/// Najpierw próbujemy wariantu z ikoną i nazwą. Gdy okien jest dużo i nie mieszczą się
/// przed zegarem, przechodzimy na **same ikony** (kwadratowe przyciski) — dzięki temu
/// taskbar nie „przelewa się” poza pasek przy kilkunastu oknach.
pub fn task_layout(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    tasks: &[Task],
) -> (Vec<Rect>, bool) {
    let t = theme;
    let cy = bar.center().y;
    let cap = t.typography.caption.size;
    let icon = 16.0;
    let start = burger_rect(bar, t).right() + t.spacing.md + 1.0 + t.spacing.md;
    let limit = bar.center().x - t.spacing.lg;

    let layout = |icon_only: bool| {
        let mut x = start;
        let mut rects = Vec::new();
        for task in tasks {
            let w = if icon_only {
                2.0 * t.spacing.sm + icon
            } else {
                let label = task_label(task);
                (text.measure(&label, cap, weight::MEDIUM).width
                    + icon
                    + t.spacing.sm
                    + 2.0 * t.spacing.md)
                    .min(200.0)
            };
            if x + w > limit {
                break;
            }
            rects.push(Rect::from_xywh(x, cy - 12.0, w, 24.0));
            x += w + t.spacing.sm;
        }
        rects
    };

    let labeled = layout(false);
    if labeled.len() == tasks.len() {
        (labeled, false)
    } else {
        (layout(true), true)
    }
}

/// Rozmieszczenie przycisków okien (bez informacji o trybie ikon).
pub fn task_rects(bar: Rect, theme: &Theme, text: &TextEngine, tasks: &[Task]) -> Vec<Rect> {
    task_layout(bar, theme, text, tasks).0
}

/// Akcje menu kontekstowego przycisku okna (PPM).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskMenuAction {
    /// Pokaż zminimalizowane okno.
    Show,
    /// Schowaj (zminimalizuj) widoczne okno.
    Minimize,
    /// Zamknij program.
    Close,
    MoveTo(usize),
}

/// Wysokość pozycji menu taskbara.
pub const TASK_MENU_ITEM_H: f64 = 30.0;

/// Prostokąt menu kontekstowego pod przyciskiem okna.
pub fn task_menu_rect(anchor: Rect, bar: Rect, items: usize, _theme: &Theme) -> Rect {
    let pad = 6.0;
    let w = 240.0;
    let h = items as f64 * TASK_MENU_ITEM_H + 2.0 * pad;
    let min_x = bar.x() + 4.0;
    let max_x = (bar.right() - w - 4.0).max(min_x);
    let x = (anchor.center().x - w / 2.0).clamp(min_x, max_x);
    Rect::from_xywh(x, bar.bottom() + 4.0, w, h)
}

/// Indeks pozycji menu pod punktem `p`.
pub fn task_menu_item_at(rect: Rect, items: usize, p: Point) -> Option<usize> {
    let pad = 6.0;
    let inner = Rect::from_xywh(
        rect.x() + pad,
        rect.y() + pad,
        rect.width() - 2.0 * pad,
        items as f64 * TASK_MENU_ITEM_H,
    );
    if !inner.contains(p) {
        return None;
    }
    let i = ((p.y - inner.y()) / TASK_MENU_ITEM_H).floor() as usize;
    (i < items).then_some(i)
}

/// Rysuje menu kontekstowe taskbara.
pub fn paint_task_menu(
    ctx: &mut PaintCtx,
    rect: Rect,
    labels: &[String],
    actions: &[TaskMenuAction],
    hovered: Option<usize>,
) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.medium, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.medium, 1.0, t.colors.border);
    let pad = 6.0;
    let inner = Rect::from_xywh(
        rect.x() + pad,
        rect.y() + pad,
        rect.width() - 2.0 * pad,
        labels.len() as f64 * TASK_MENU_ITEM_H,
    );
    for (i, label) in labels.iter().enumerate() {
        let row = Rect::from_xywh(
            inner.x(),
            inner.y() + i as f64 * TASK_MENU_ITEM_H,
            inner.width(),
            TASK_MENU_ITEM_H,
        );
        if hovered == Some(i) {
            ctx.scene
                .fill_rounded_rect(row, t.radius.small, t.colors.shade);
        }
        let icon = match actions.get(i) {
            Some(TaskMenuAction::Show) => IconName::App,
            Some(TaskMenuAction::Minimize) => IconName::Minimize,
            Some(TaskMenuAction::Close) => IconName::Close,
            Some(TaskMenuAction::MoveTo(_)) => IconName::Grid,
            None => IconName::App,
        };
        let isz = 16.0;
        let icon_rect = Rect::from_xywh(row.x() + 8.0, row.center().y - isz / 2.0, isz, isz);
        paint_icon(ctx, icon, icon_rect, t.colors.text);
        let text_x = icon_rect.right() + 10.0;
        let avail = (row.right() - 8.0 - text_x).max(0.0);
        let label = ellipsize(
            label,
            avail,
            t.typography.body.size,
            weight::NORMAL,
            ctx.text,
        );
        ctx.label_left(
            text_x,
            row.center().y,
            &label,
            t.typography.body.size,
            weight::NORMAL,
            t.colors.text,
        );
    }
}

/// Indeks okna (przycisku) pod punktem `p`.
pub fn task_at(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    tasks: &[Task],
    p: Point,
) -> Option<usize> {
    if p.y < bar.y() || p.y > bar.bottom() {
        return None;
    }
    task_rects(bar, theme, text, tasks)
        .iter()
        .position(|r| p.x >= r.x() - 2.0 && p.x <= r.right() + 2.0)
}

#[cfg(test)]
mod taskbar_tests {
    use super::*;

    #[test]
    fn long_task_label_is_ellipsized_to_button_width() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let cap = theme.typography.caption.size;
        let long = "Bardzo długa nazwa okna — dokument bez tytułu 2026";
        let out = ellipsize(long, 90.0, cap, libluna_core::style::weight::MEDIUM, &text);
        assert!(out.ends_with('…'), "brak wielokropka: {out:?}");
        assert!(out.chars().count() < long.chars().count());
        assert!(
            text.measure(&out, cap, libluna_core::style::weight::MEDIUM)
                .width
                <= 90.5,
            "nazwa nadal wystaje: {out:?}"
        );
    }

    #[test]
    fn many_tasks_switch_to_icons() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let tasks: Vec<Task> = (0..12)
            .map(|i| Task {
                title: format!("Bardzo długa nazwa okna numer {i}"),
                app_id: "app".into(),
                active: false,
                minimized: false,
            })
            .collect();
        let (rects, icon_only) = task_layout(bar, &theme, &text, &tasks);
        assert!(icon_only, "przy 12 oknach taskbar powinien użyć ikon");
        assert_eq!(rects.len(), tasks.len());
        assert!((rects[0].width() - 28.0).abs() < 0.1);
        assert!(rects.last().unwrap().right() < bar.center().x);
    }

    #[test]
    fn task_menu_hit_test_matches_rows() {
        let theme = Theme::light();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let anchor = Rect::from_xywh(50.0, 8.0, 100.0, 24.0);
        let menu = task_menu_rect(anchor, bar, 6, &theme);
        assert!(menu.x() >= bar.x());
        assert!(menu.right() <= bar.right());
        for i in 0..6 {
            let y = menu.y() + 6.0 + (i as f64 + 0.5) * TASK_MENU_ITEM_H;
            assert_eq!(
                task_menu_item_at(menu, 6, Point::new(menu.center().x, y)),
                Some(i)
            );
        }
        assert_eq!(
            task_menu_item_at(menu, 6, Point::new(menu.center().x, menu.bottom() + 20.0)),
            None
        );
    }

    #[test]
    fn task_icons_follow_app_id() {
        assert_eq!(
            task_icon(&Task {
                title: "patryks — Thunar".into(),
                app_id: "thunar".into(),
                active: false,
                minimized: false,
            }),
            IconName::Files
        );
        assert_eq!(
            task_icon(&Task {
                title: "Terminal".into(),
                app_id: "xfce4-terminal".into(),
                active: false,
                minimized: false,
            }),
            IconName::Terminal
        );
        assert_eq!(
            task_icon(&Task {
                title: "cokolwiek".into(),
                app_id: "nieznana".into(),
                active: false,
                minimized: false,
            }),
            IconName::App
        );
    }

    #[test]
    fn task_buttons_layout_and_hit_test() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let tasks = vec![
            Task {
                title: "Pliki".into(),
                app_id: "files".into(),
                active: true,
                minimized: false,
            },
            Task {
                title: "Terminal".into(),
                app_id: "term".into(),
                active: false,
                minimized: false,
            },
        ];
        let rects = task_rects(bar, &theme, &text, &tasks);
        assert_eq!(rects.len(), 2);
        assert!(rects[1].x() > rects[0].right());
        assert_eq!(
            task_at(bar, &theme, &text, &tasks, rects[1].center()),
            Some(1)
        );
        assert_eq!(
            task_at(bar, &theme, &text, &tasks, Point::new(2.0, 2.0)),
            None
        );
    }
}

/// Krótka etykieta okna (tytuł, a gdy pusty — `app_id`), przycięta do ~28 znaków.
fn task_label(task: &Task) -> String {
    let s = if !task.title.is_empty() {
        task.title.clone()
    } else {
        task.app_id.clone()
    };
    let max = 28;
    if s.chars().count() > max {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    } else {
        s
    }
}

/// Skraca tekst tak, żeby **w pikselach** zmieścił się w `max_w` (z „…” na końcu).
///
/// Wcześniej nazwa była ucinana po liczbie znaków, ale przy szerokich literach
/// (albo wąskiej pigułce) i tak wystawała poza przycisk taskbara.
fn ellipsize(text: &str, max_w: f64, size: f64, weight: u16, engine: &TextEngine) -> String {
    if max_w <= 0.0 || engine.measure(text, size, weight).width <= max_w {
        return text.to_owned();
    }
    let dots = "…";
    let dots_w = engine.measure(dots, size, weight).width;
    let mut out = String::new();
    for ch in text.chars() {
        let mut probe = out.clone();
        probe.push(ch);
        if engine.measure(&probe, size, weight).width + dots_w > max_w {
            break;
        }
        out = probe;
    }
    out.push('…');
    out
}

/// Rysuje przycisk okna w taskbarze (aktywny — akcent).
fn paint_task_button(ctx: &mut PaintCtx, rect: Rect, task: &Task, icon_only: bool) {
    let t = ctx.theme;
    let cap = t.typography.caption.size;
    let (bg, fg) = if task.active {
        (t.colors.accent, t.colors.accent_fg)
    } else {
        (t.colors.card_bg, t.colors.text)
    };
    ctx.scene.fill_rounded_rect(rect, rect.height() / 2.0, bg);
    if !task.active {
        ctx.scene
            .stroke_rounded_rect(rect, rect.height() / 2.0, 1.0, t.colors.border);
    }
    let icon = task_icon(task);
    let isz = 16.0;
    if icon_only {
        let icon_rect = Rect::from_xywh(
            rect.center().x - isz / 2.0,
            rect.center().y - isz / 2.0,
            isz,
            isz,
        );
        paint_icon(ctx, icon, icon_rect, fg);
        return;
    }
    let icon_rect = Rect::from_xywh(
        rect.x() + t.spacing.sm,
        rect.center().y - isz / 2.0,
        isz,
        isz,
    );
    paint_icon(ctx, icon, icon_rect, fg);
    let text_x = icon_rect.right() + t.spacing.sm;
    let avail = (rect.right() - t.spacing.sm - text_x).max(0.0);
    let label = ellipsize(&task_label(task), avail, cap, weight::MEDIUM, ctx.text);
    ctx.label_left(text_x, rect.center().y, &label, cap, weight::MEDIUM, fg);
}

/// Rysuje panel górny w zadanym prostokącie.
pub fn paint_panel(panel: &PanelState, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    let cy = rect.center().y;
    let body = t.typography.body.size;

    // Tło + dolna krawędź oddzielająca od pulpitu.
    ctx.scene.fill_rect(rect, t.colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(rect.x(), rect.bottom() - 1.0, rect.width(), 1.0),
        t.colors.border,
    );

    // ── Lewa grupa: sam burger (ikona Menu) + separator + tytuł okna ──
    let burger = burger_rect(rect, t);
    ctx.scene
        .fill_rounded_rect(burger, t.radius.small, t.colors.shade);
    let isz = burger.width() * 0.64;
    paint_icon(
        ctx,
        IconName::Menu,
        Rect::from_xywh(
            burger.center().x - isz / 2.0,
            burger.center().y - isz / 2.0,
            isz,
            isz,
        ),
        t.colors.text,
    );
    let mut x = burger.right() + t.spacing.md;
    ctx.scene
        .fill_rect(Rect::from_xywh(x, cy - 8.0, 1.0, 16.0), t.colors.border);
    x += t.spacing.md;
    // Taskbar: przyciski okien; przy braku okien — tytuł (podglądy headless).
    if panel.tasks.is_empty() {
        let max = (clock_rect(rect, t, ctx.text, panel).x() - t.spacing.md - x).max(0.0);
        let title = ellipsize(&panel.title, max, body, weight::NORMAL, ctx.text);
        ctx.label_left(x, cy, &title, body, weight::NORMAL, t.colors.dim_text);
    } else {
        let (rects, icon_only) = task_layout(rect, t, ctx.text, &panel.tasks);
        for (r, task) in rects.iter().zip(panel.tasks.iter()) {
            paint_task_button(ctx, *r, task, icon_only);
        }
    }

    // ── Środek: zegar + data ──
    let clock = format!("{}   ·   {}", panel.clock, panel.date);
    ctx.label_center(
        rect.center().x,
        cy,
        &clock,
        body,
        weight::MEDIUM,
        t.colors.text,
    );

    // ── Prawa grupa (od prawej): zasilanie, użytkownik, przełączniki, przestrzenie ──
    // Przycisk zasilania (skrajna prawa strona).
    let power = power_rect(rect, t);
    ctx.scene
        .fill_rounded_rect(power, t.radius.small, t.colors.shade);
    let pisz = POWER_BTN * 0.62;
    paint_icon(
        ctx,
        IconName::Power,
        Rect::from_xywh(
            power.center().x - pisz / 2.0,
            power.center().y - pisz / 2.0,
            pisz,
            pisz,
        ),
        t.colors.text,
    );
    let mut right = power.x() - t.spacing.sm;
    let uw = ctx.measure(&panel.user, body, weight::NORMAL);
    ctx.label_right(right, cy, &panel.user, body, weight::NORMAL, t.colors.text);
    right -= uw + t.spacing.md;
    ctx.scene
        .fill_rect(Rect::from_xywh(right, cy - 8.0, 1.0, 16.0), t.colors.border);

    // Przełączniki szybkiego dostępu (pigułki).
    let toggles = toggle_rects(rect, t, ctx.text, panel);
    for (r, toggle) in toggles.iter().zip(panel.toggles.iter()) {
        paint_toggle_pill(ctx, *r, toggle);
    }

    // Kropki przestrzeni roboczych (spójne z `workspace_rects`).
    for (i, dot) in workspace_rects(rect, t, ctx.text, panel).iter().enumerate() {
        let color = if i == panel.current_ws {
            t.colors.accent
        } else {
            t.colors.dim_text
        };
        ctx.scene.fill_rounded_rect(*dot, dot.height() / 2.0, color);
    }

    // Przycisk Quick Panelu (motyw, Wi-Fi, dźwięk, jasność).
    let qb = quick_button_rect(rect, t, ctx.text, panel);
    ctx.scene
        .fill_rounded_rect(qb, t.radius.small, t.colors.shade);
    paint_icon(
        ctx,
        IconName::Sliders,
        Rect::from_xywh(qb.center().x - 7.5, qb.center().y - 7.5, 15.0, 15.0),
        t.colors.text,
    );
}

/// Przełącznik statusu w pasku: sama ikona (opcjonalnie krótka wartość), bez pigułki.
fn paint_toggle_pill(ctx: &mut PaintCtx, rect: Rect, toggle: &Toggle) {
    let t = ctx.theme;
    let fg = if toggle.active {
        t.colors.accent
    } else {
        t.colors.dim_text
    };
    let isz = 15.0;
    let ix = rect.x() + t.spacing.xs;
    paint_icon(
        ctx,
        toggle.icon,
        Rect::from_xywh(ix, rect.center().y - isz / 2.0, isz, isz),
        fg,
    );
    if let Some(value) = &toggle.value {
        ctx.label_left(
            ix + isz + t.spacing.xs,
            rect.center().y,
            value,
            t.typography.caption.size,
            weight::MEDIUM,
            fg,
        );
    }
}

/// Pigułka segmentu (np. przełącznik widoku launchera).
fn paint_pill(ctx: &mut PaintCtx, rect: Rect, label: &str, active: bool) {
    let t = ctx.theme;
    let bg = if active {
        t.colors.accent
    } else {
        t.colors.card_bg
    };
    let fg = if active {
        t.colors.accent_fg
    } else {
        t.colors.text
    };
    ctx.scene.fill_rounded_rect(rect, rect.height() / 2.0, bg);
    if !active {
        ctx.scene
            .stroke_rounded_rect(rect, rect.height() / 2.0, 1.0, t.colors.border);
    }
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        label,
        t.typography.caption.size,
        weight::MEDIUM,
        fg,
    );
}

/// Rozmieszczenie elementów launchera.
#[derive(Debug, Clone)]
pub struct LauncherRects {
    pub header: Rect,
    pub search: Rect,
    /// Całe segmentowe przełączniki widoku.
    pub switcher: Rect,
    /// Trzy przyciski widoku (kolejność jak [`LauncherView::ALL`]).
    pub views: [Rect; 3],
    /// Obszar na zawartość (lista/siatka/karuzela).
    pub content: Rect,
}

/// Liczy rozmieszczenie launchera w obszarze `area`.
pub fn launcher_rects(area: Rect, theme: &Theme, text: &TextEngine) -> LauncherRects {
    let pad = theme.spacing.lg;
    let header_h = 30.0;
    let header = Rect::from_xywh(
        area.x() + pad,
        area.y() + pad,
        area.width() - 2.0 * pad,
        header_h,
    );

    // Przełącznik widoku po prawej nagłówka.
    let cap = theme.typography.caption.size;
    let gap = theme.spacing.xs;
    let widths: Vec<f64> = LauncherView::ALL
        .iter()
        .map(|v| text.measure(v.label(), cap, weight::MEDIUM).width + 2.0 * theme.spacing.md)
        .collect();
    let total: f64 = widths.iter().sum::<f64>() + gap * 2.0;
    let mut views = [Rect::ZERO; 3];
    let mut x = header.right() - total;
    for (i, w) in widths.iter().enumerate() {
        views[i] = Rect::from_xywh(x, header.y() + (header_h - 26.0) / 2.0, *w, 26.0);
        x += w + gap;
    }
    let switcher = Rect::from_xywh(header.right() - total, header.y(), total, 26.0);

    let search = Rect::from_xywh(
        area.x() + pad,
        header.bottom() + theme.spacing.sm,
        area.width() - 2.0 * pad,
        32.0,
    );
    let content_y = search.bottom() + theme.spacing.lg;
    let content = Rect::from_xywh(
        area.x() + pad,
        content_y,
        area.width() - 2.0 * pad,
        (area.bottom() - pad - content_y).max(0.0),
    );

    LauncherRects {
        header,
        search,
        switcher,
        views,
        content,
    }
}

/// Widok odpowiadający kliknięciu w przełącznik (jeśli trafiono).
pub fn launcher_view_at(
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    p: Point,
) -> Option<LauncherView> {
    let r = launcher_rects(area, theme, text);
    r.views
        .iter()
        .position(|v| v.contains(p))
        .map(|i| LauncherView::ALL[i])
}

/// Indeks aplikacji pod punktem `p` (wg aktualnego widoku).
pub fn launcher_app_at(
    apps: &[App],
    launcher: &Launcher,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    p: Point,
) -> Option<usize> {
    let filtered = launcher.filter_apps(apps);
    let content = launcher_rects(area, theme, text).content;
    let sy = launcher.scroll;
    let filtered_idx = match launcher.view {
        LauncherView::Grid => (0..filtered.len()).find(|&i| {
            let t = grid_tile(content, i);
            Rect::from_xywh(t.x(), t.y() - sy, t.width(), t.height()).contains(p)
        }),
        LauncherView::List => (0..filtered.len()).find(|&i| {
            let t = list_row(content, i);
            Rect::from_xywh(t.x(), t.y() - sy, t.width(), t.height()).contains(p)
        }),
        LauncherView::Carousel => {
            (0..filtered.len()).find(|&i| carousel_card(content, i, launcher.carousel).contains(p))
        }
    }?;
    let target_app = filtered[filtered_idx];
    apps.iter().position(|a| std::ptr::eq(a, target_app))
}

fn grid_tile(content: Rect, i: usize) -> Rect {
    let gap = 12.0;
    let cell_w = (content.width() - gap * (GRID_COLS as f64 - 1.0)) / GRID_COLS as f64;
    let c = (i % GRID_COLS) as f64;
    let r = (i / GRID_COLS) as f64;
    Rect::from_xywh(
        content.x() + c * (cell_w + gap),
        content.y() + r * (GRID_CELL_H + gap),
        cell_w,
        GRID_CELL_H,
    )
}

fn list_row(content: Rect, i: usize) -> Rect {
    let gap = 8.0;
    Rect::from_xywh(
        content.x(),
        content.y() + i as f64 * (LIST_ROW_H + gap),
        content.width(),
        LIST_ROW_H,
    )
}

fn carousel_card(content: Rect, i: usize, selected: usize) -> Rect {
    let gap = 16.0;
    let x = content.center().x - CARD_W / 2.0 + (i as f64 - selected as f64) * (CARD_W + gap);
    Rect::from_xywh(
        x,
        content.y() + (content.height() - CARD_H) / 2.0,
        CARD_W,
        CARD_H,
    )
}

/// Maksymalne pionowe przewinięcie dla danego widoku.
pub fn launcher_scroll_max(
    apps: &[App],
    launcher: &Launcher,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
) -> f64 {
    let filtered_len = launcher.filter_apps(apps).len();
    let content = launcher_rects(area, theme, text).content;
    let gap_list = 8.0;
    let gap_grid = 12.0;
    let total = match launcher.view {
        LauncherView::List => (filtered_len as f64) * (LIST_ROW_H + gap_list) - gap_list,
        LauncherView::Grid => {
            let rows = filtered_len.div_ceil(GRID_COLS) as f64;
            rows * (GRID_CELL_H + gap_grid) - gap_grid
        }
        LauncherView::Carousel => 0.0,
    };
    (total - content.height()).max(0.0)
}

/// Dopasowuje przewinięcie tak, aby zaznaczona pozycja była widoczna.
pub fn ensure_selection_visible(
    apps: &[App],
    launcher: &mut Launcher,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
) {
    let filtered_len = launcher.filter_apps(apps).len();
    ensure_selection_visible_count(filtered_len, launcher, area, theme, text);
}

/// Dopasowuje przewinięcie dla zadanej liczby pozycji.
pub fn ensure_selection_visible_count(
    count: usize,
    launcher: &mut Launcher,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
) {
    let Some(i) = launcher.selected else {
        return;
    };
    if i >= count {
        return;
    }
    let content = launcher_rects(area, theme, text).content;
    let (top, bottom) = match launcher.view {
        LauncherView::Grid => {
            let t = grid_tile(content, i);
            (t.y() - content.y(), t.bottom() - content.y())
        }
        LauncherView::List => {
            let t = list_row(content, i);
            (t.y() - content.y(), t.bottom() - content.y())
        }
        LauncherView::Carousel => return,
    };
    let view_h = content.height();
    if top < launcher.scroll {
        launcher.scroll = top;
    } else if bottom > launcher.scroll + view_h {
        launcher.scroll = bottom - view_h;
    }
    let max = launcher_scroll_max_count(count, launcher, area, theme, text);
    launcher.scroll = launcher.scroll.clamp(0.0, max);
}

/// Liczy maksymalne przewinięcie na podstawie liczby pozycji.
pub fn launcher_scroll_max_count(
    count: usize,
    launcher: &Launcher,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
) -> f64 {
    let content = launcher_rects(area, theme, text).content;
    let gap_list = 8.0;
    let gap_grid = 12.0;
    let total = match launcher.view {
        LauncherView::List => (count as f64) * (LIST_ROW_H + gap_list) - gap_list,
        LauncherView::Grid => {
            let rows = count.div_ceil(GRID_COLS) as f64;
            rows * (GRID_CELL_H + gap_grid) - gap_grid
        }
        LauncherView::Carousel => 0.0,
    };
    (total - content.height()).max(0.0)
}

/// Rysuje launcher w zadanym prostokącie.
pub fn paint_launcher(apps: &[App], launcher: &Launcher, ctx: &mut PaintCtx, area: Rect) {
    let t = *ctx.theme;
    let r = launcher_rects(area, &t, ctx.text);

    ctx.scene
        .fill_rounded_rect(area, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(area, t.radius.large, 1.0, t.colors.border);

    ctx.label_left(
        r.header.x(),
        r.header.center().y,
        "Aplikacje",
        t.typography.heading.size,
        weight::BOLD,
        t.colors.text,
    );

    // Przełącznik widoku.
    for (i, view) in LauncherView::ALL.iter().enumerate() {
        paint_pill(ctx, r.views[i], view.label(), *view == launcher.view);
    }

    // Pole wyszukiwania z aktywnym tekstem i kursorem
    ctx.scene
        .fill_rounded_rect(r.search, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(r.search, t.radius.medium, 1.0, t.colors.border);

    let s_icon_sz = 16.0;
    let s_icon_rect = Rect::from_xywh(
        r.search.x() + t.spacing.sm,
        r.search.center().y - s_icon_sz / 2.0,
        s_icon_sz,
        s_icon_sz,
    );
    paint_icon(ctx, IconName::Search, s_icon_rect, t.colors.dim_text);

    let search_x = s_icon_rect.right() + t.spacing.sm;
    if launcher.search.is_empty() {
        ctx.label_left(
            search_x,
            r.search.center().y,
            "Szukaj aplikacji…",
            t.typography.body.size,
            weight::NORMAL,
            t.colors.dim_text,
        );
    } else {
        ctx.label_left(
            search_x,
            r.search.center().y,
            &launcher.search,
            t.typography.body.size,
            weight::NORMAL,
            t.colors.text,
        );
        let tw = ctx.measure(&launcher.search, t.typography.body.size, weight::NORMAL);
        let cursor_x = search_x + tw + 2.0;
        ctx.scene.fill_rect(
            Rect::from_xywh(cursor_x, r.search.center().y - 8.0, 2.0, 16.0),
            t.colors.accent,
        );
    }

    // Filtrowana lista aplikacji
    let filtered = launcher.filter_apps(apps);
    let scroll = launcher.scroll;
    ctx.scene.push_clip(r.content);

    if filtered.is_empty() {
        ctx.label_center(
            r.content.center().x,
            r.content.center().y,
            "Nie znaleziono aplikacji",
            t.typography.body.size,
            weight::NORMAL,
            t.colors.dim_text,
        );
    } else {
        match launcher.view {
            LauncherView::Grid => {
                for (i, app) in filtered.iter().enumerate() {
                    let t = grid_tile(r.content, i);
                    paint_grid_tile(
                        app,
                        launcher.selected_index(filtered.len()) == Some(i),
                        ctx,
                        Rect::from_xywh(t.x(), t.y() - scroll, t.width(), t.height()),
                    );
                }
            }
            LauncherView::List => {
                for (i, app) in filtered.iter().enumerate() {
                    let t = list_row(r.content, i);
                    paint_list_row(
                        app,
                        launcher.selected_index(filtered.len()) == Some(i),
                        ctx,
                        Rect::from_xywh(t.x(), t.y() - scroll, t.width(), t.height()),
                    );
                }
            }
            LauncherView::Carousel => {
                for (i, app) in filtered.iter().enumerate() {
                    paint_carousel_card(
                        app,
                        i == launcher.carousel,
                        ctx,
                        carousel_card(r.content, i, launcher.carousel),
                    );
                }
            }
        }
    }
    ctx.scene.pop_clip();
}

/// Kafelek siatki: ikona + nazwa; zaznaczony wyróżniony akcentem.
fn paint_grid_tile(app: &App, selected: bool, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    let bg = if selected {
        t.colors.accent.scale_alpha(0.12)
    } else {
        t.colors.view_bg
    };
    ctx.scene.fill_rounded_rect(rect, t.radius.medium, bg);
    let border = if selected {
        t.colors.accent
    } else {
        t.colors.border
    };
    ctx.scene.stroke_rounded_rect(
        rect,
        t.radius.medium,
        if selected { 2.0 } else { 1.0 },
        border,
    );

    let icon = Rect::from_xywh(rect.center().x - 18.0, rect.y() + 12.0, 36.0, 36.0);
    ctx.scene
        .fill_rounded_rect(icon, t.radius.small, t.colors.accent.scale_alpha(0.16));
    let inner = Rect::from_xywh(icon.x() + 9.0, icon.y() + 9.0, 18.0, 18.0);
    paint_icon(ctx, app.icon, inner, t.colors.accent);
    ctx.label_center(
        rect.center().x,
        icon.bottom() + 12.0,
        &app.name,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.text,
    );
}

/// Wiersz listy: ikona + nazwa + kategoria; zaznaczony wyróżniony akcentem.
fn paint_list_row(app: &App, selected: bool, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    let bg = if selected {
        t.colors.accent.scale_alpha(0.12)
    } else {
        t.colors.view_bg
    };
    ctx.scene.fill_rounded_rect(rect, t.radius.medium, bg);
    let border = if selected {
        t.colors.accent
    } else {
        t.colors.border
    };
    ctx.scene.stroke_rounded_rect(
        rect,
        t.radius.medium,
        if selected { 2.0 } else { 1.0 },
        border,
    );

    let icon = Rect::from_xywh(rect.x() + t.spacing.md, rect.center().y - 16.0, 32.0, 32.0);
    ctx.scene
        .fill_rounded_rect(icon, t.radius.small, t.colors.accent.scale_alpha(0.16));
    let inner = Rect::from_xywh(icon.x() + 7.0, icon.y() + 7.0, 18.0, 18.0);
    paint_icon(ctx, app.icon, inner, t.colors.accent);
    ctx.label_left(
        icon.right() + t.spacing.md,
        rect.center().y,
        &app.name,
        t.typography.body.size,
        weight::MEDIUM,
        t.colors.text,
    );
    ctx.label_right(
        rect.right() - t.spacing.md,
        rect.center().y,
        &app.category,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );
}

/// Karta karuzeli: duża ikona + nazwa; wybrana wyróżniona akcentem.
fn paint_carousel_card(app: &App, selected: bool, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    let bg = if selected {
        t.colors.accent.scale_alpha(0.12)
    } else {
        t.colors.view_bg
    };
    ctx.scene.fill_rounded_rect(rect, t.radius.large, bg);
    let border = if selected {
        t.colors.accent
    } else {
        t.colors.border
    };
    ctx.scene.stroke_rounded_rect(
        rect,
        t.radius.large,
        if selected { 2.0 } else { 1.0 },
        border,
    );

    let icon = Rect::from_xywh(rect.center().x - 34.0, rect.y() + 36.0, 68.0, 68.0);
    ctx.scene
        .fill_rounded_rect(icon, t.radius.medium, t.colors.accent.scale_alpha(0.18));
    let inner = Rect::from_xywh(icon.x() + 16.0, icon.y() + 16.0, 36.0, 36.0);
    paint_icon(ctx, app.icon, inner, t.colors.accent);
    ctx.label_center(
        rect.center().x,
        icon.bottom() + 24.0,
        &app.name,
        t.typography.body.size,
        if selected {
            weight::BOLD
        } else {
            weight::MEDIUM
        },
        t.colors.text,
    );
    ctx.label_center(
        rect.center().x,
        icon.bottom() + 44.0,
        &app.category,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.dim_text,
    );
}

/// Pełny podgląd pulpitu: tapeta, panel górny i launcher.
pub fn paint_desktop(
    panel: &PanelState,
    apps: &[App],
    launcher: &Launcher,
    ctx: &mut PaintCtx,
    size: Size,
) {
    let accent = ctx.theme.colors.accent;

    ctx.scene
        .fill_rect(Rect::from_size(size), ctx.theme.colors.window_bg);
    let blob = Point::new(size.width * 0.78, size.height * 0.74);
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(blob.x - 220.0, blob.y - 220.0, 440.0, 440.0),
        220.0,
        accent.scale_alpha(0.10),
    );

    paint_panel(panel, ctx, Rect::from_xywh(0.0, 0.0, size.width, THICKNESS));

    let lw = LAUNCHER_W;
    let lh = LAUNCHER_H;
    let lrect = Rect::from_xywh(
        (size.width - lw) / 2.0,
        (size.height - lh) / 2.0 + 16.0,
        lw,
        lh,
    );
    paint_launcher(apps, launcher, ctx, lrect);
}

/// Wyciszony kolor akcentu (pomocniczo dla tapet/podświetleń).
pub fn accent_soft(theme: &Theme) -> Color {
    theme.colors.accent.scale_alpha(0.12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::{Scene, TextEngine};

    fn area() -> Rect {
        Rect::from_xywh(100.0, 60.0, LAUNCHER_W, LAUNCHER_H)
    }

    #[test]
    fn workspace_dots_hit_test_matches_layout() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let mut st = PanelState::sample();
        st.workspaces = 4;
        st.current_ws = 2;
        let rects = workspace_rects(bar, &theme, &text, &st);
        assert_eq!(rects.len(), 4);
        for (i, r) in rects.iter().enumerate() {
            assert_eq!(workspace_at(bar, &theme, &text, &st, r.center()), Some(i));
        }
        // Kropki idą w lewo (wyższy indeks bliżej prawej) i nie zachodzą na siebie.
        assert!(rects[3].x() > rects[0].x());
        assert!(rects[0].right() <= rects[1].x());
    }

    #[test]
    fn quick_panel_hit_tests_match_layout() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let st = PanelState::sample();
        let btn = quick_button_rect(bar, &theme, &text, &st);
        assert!(
            bar.contains(btn.center()),
            "przycisk Quick musi być w pasku"
        );
        let panel = quick_panel_rect(bar, &theme);
        assert!(panel.y() >= bar.bottom());
        assert!(panel.right() <= bar.right());
        let l = quick_layout(panel, &theme);
        assert_eq!(
            quick_hit(panel, &theme, l.header_buttons[0].center()),
            Some(QuickHit::Theme)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.header_buttons[1].center()),
            Some(QuickHit::Power)
        );
        let tile_hits = [
            QuickHit::Wifi,
            QuickHit::Bluetooth,
            QuickHit::Airplane,
            QuickHit::Dnd,
            QuickHit::NightLight,
            QuickHit::PowerSaver,
        ];
        for (r, hit) in l.tiles.iter().zip(tile_hits) {
            assert_eq!(quick_hit(panel, &theme, r.center()), Some(hit));
        }
        assert_eq!(
            quick_hit(panel, &theme, l.mute.center()),
            Some(QuickHit::Mute)
        );
        let p = Point::new(l.volume.x() + l.volume.width() * 0.25, l.volume.center().y);
        match quick_hit(panel, &theme, p) {
            Some(QuickHit::Volume(v)) => assert!((23..=27).contains(&v), "{v}"),
            other => panic!("oczekiwano Volume, jest {other:?}"),
        }
        let p = Point::new(
            l.brightness.x() + l.brightness.width() * 0.75,
            l.brightness.center().y,
        );
        match quick_hit(panel, &theme, p) {
            Some(QuickHit::Brightness(v)) => assert!((73..=77).contains(&v), "{v}"),
            other => panic!("oczekiwano Brightness, jest {other:?}"),
        }
        assert_eq!(
            quick_hit(panel, &theme, l.media_buttons[0].center()),
            Some(QuickHit::MediaPrevious)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.media_buttons[1].center()),
            Some(QuickHit::MediaPlayPause)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.media_buttons[2].center()),
            Some(QuickHit::MediaNext)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.session[0].center()),
            Some(QuickHit::Lock)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.session[1].center()),
            Some(QuickHit::Logout)
        );
        assert_eq!(
            quick_hit(panel, &theme, l.session[2].center()),
            Some(QuickHit::Reboot)
        );
        assert_eq!(
            quick_hit(
                panel,
                &theme,
                Point::new(panel.right() + 40.0, panel.center().y)
            ),
            None
        );
    }

    #[test]
    #[ignore = "generuje podgląd do docs/images (uruchom recznie)"]
    fn render_quick_panel_preview() {
        use libluna_render::{Scene, SoftwareRenderer};
        let text = TextEngine::system();
        let width = 1920.0;
        let section = QUICK_H + THICKNESS + 28.0;
        let size = Size::new(width, section * 2.0);
        let mut scene = Scene::new();
        let state = PanelState::sample();
        for (i, theme) in [Theme::light(), Theme::dark()].into_iter().enumerate() {
            let top = i as f64 * section;
            scene.fill_rect(
                Rect::from_xywh(0.0, top, width, section),
                theme.colors.window_bg,
            );
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            let bar = Rect::from_xywh(0.0, top, width, THICKNESS);
            paint_panel(&state, &mut ctx, bar);
            let q = quick_panel_rect(bar, &theme);
            let data = QuickData {
                user: "patryks",
                preference: ThemePreference::System,
                dark: i == 1,
                wifi_on: true,
                ssid: Some("Dom_5G"),
                bluetooth: Some((true, 2)),
                airplane: false,
                dnd: false,
                night_light: 60,
                gamma_available: true,
                power_saver: false,
                power_fallback: true,
                volume: 65,
                muted: false,
                brightness: Some(80),
                sink: Some("Głośniki wbudowane"),
                battery: Some(BatteryStatus {
                    percent: 82,
                    charging: false,
                    minutes: Some(200),
                }),
                media: Some(QuickMedia {
                    title: "Nazwa utworu",
                    artist: "Wykonawca",
                    playing: true,
                }),
            };
            paint_quick_panel(
                &mut ctx,
                Rect::from_xywh(q.x(), bar.bottom() + 12.0, q.width(), q.height()),
                &data,
            );
        }
        let mut renderer = SoftwareRenderer::new(size.width as u32, size.height as u32);
        renderer.draw(&scene, 1.0, Theme::light().colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../docs/images/quick-panel.png"
        );
        std::fs::write(path, png).expect("zapis PNG");
        println!("saved {path}");
    }

    #[test]
    fn power_menu_items_hit_test() {
        let theme = Theme::dark();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let n = 3;
        let menu = power_menu_rect(bar, &theme, n);
        let row = |i: usize| {
            Rect::from_xywh(
                menu.x() + 6.0,
                menu.y() + 6.0 + i as f64 * POWER_ITEM_H,
                menu.width() - 12.0,
                POWER_ITEM_H,
            )
        };
        for i in 0..n {
            assert_eq!(power_item_at(bar, &theme, n, row(i).center()), Some(i));
        }
        // Poza pozycjami (róg menu) brak trafienia.
        assert_eq!(
            power_item_at(bar, &theme, n, Point::new(menu.x() + 1.0, menu.y() + 1.0)),
            None
        );
        // Przycisk zasilania mieści się w pasku, przy prawej krawędzi.
        let p = power_rect(bar, &theme);
        assert!(p.right() <= bar.right());
        assert!(bar.contains(p.center()));
    }

    fn panel_commands(view: LauncherView) -> usize {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint_panel(
            &PanelState::sample(),
            &mut ctx,
            Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS),
        );
        let launcher = Launcher {
            view,
            carousel: 2,
            selected: None,
            scroll: 0.0,
            search: String::new(),
        };
        paint_launcher(&App::sample(), &launcher, &mut ctx, area());
        scene.len()
    }

    #[test]
    fn panel_has_no_activities_label_but_burger_button() {
        // Panel ma burger po lewej; kropki przestrzeni i tytuł po prawej od niego.
        let theme = Theme::light();
        let panel = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let burger = burger_rect(panel, &theme);
        assert!(panel.contains(burger.center()));
        assert_eq!(burger.width(), 28.0);
    }

    #[test]
    fn view_switcher_cycles_and_hit_tests() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let r = launcher_rects(area(), &theme, &text);
        for (i, view) in LauncherView::ALL.iter().enumerate() {
            assert_eq!(
                launcher_view_at(area(), &theme, &text, r.views[i].center()),
                Some(*view)
            );
        }
        assert_eq!(LauncherView::List.next(), LauncherView::Grid);
        assert_eq!(LauncherView::Carousel.next(), LauncherView::List);
    }

    #[test]
    fn list_scroll_is_clamped_and_offsets_hit_test() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let apps = App::sample();
        let mut launcher = Launcher {
            view: LauncherView::List,
            carousel: 0,
            selected: None,
            scroll: 0.0,
            search: String::new(),
        };
        let max = launcher_scroll_max(&apps, &launcher, area(), &theme, &text);
        assert!(max > 0.0, "lista 12 pozycji powinna się przewijać");

        // Po przewinięciu o jeden wiersz pierwsza pozycja jest nad obszarem → trafiamy w drugą.
        launcher.scroll = (LIST_ROW_H + 8.0).min(max);
        let r = launcher_rects(area(), &theme, &text);
        let top = Point::new(r.content.x() + 5.0, r.content.y() + 5.0);
        assert_eq!(
            launcher_app_at(&apps, &launcher, area(), &theme, &text, top),
            Some(1)
        );
    }

    #[test]
    fn all_three_views_paint() {
        assert!(panel_commands(LauncherView::List) > 20);
        assert!(panel_commands(LauncherView::Grid) > 20);
        assert!(panel_commands(LauncherView::Carousel) > 20);
    }

    #[test]
    fn app_hit_test_finds_tiles_in_each_view() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let apps = App::sample();
        for view in LauncherView::ALL {
            let launcher = Launcher {
                view,
                carousel: 2,
                selected: None,
                scroll: 0.0,
                search: String::new(),
            };
            let r = launcher_rects(area(), &theme, &text);
            // Punkt wnętrza znanej pozycji (pierwszy kafel/wiersz, albo środek wybranej karty).
            let (p, expected) = match view {
                LauncherView::Grid | LauncherView::List => {
                    (Point::new(r.content.x() + 5.0, r.content.y() + 5.0), 0)
                }
                LauncherView::Carousel => (r.content.center(), 2),
            };
            assert_eq!(
                launcher_app_at(&apps, &launcher, area(), &theme, &text, p),
                Some(expected),
                "widok {view:?}"
            );
        }
    }

    #[test]
    fn keyboard_navigation_moves_selection_per_view() {
        // Lista: +1/-1 po indeksie, z zawijaniem.
        let mut list = Launcher {
            view: LauncherView::List,
            ..Launcher::default()
        };
        assert_eq!(list.selected_index(0), None);
        list.navigate(5, 0, 1);
        assert_eq!(list.selected_index(5), Some(1));
        list.navigate(5, 0, -1);
        list.navigate(5, 0, -1);
        assert_eq!(list.selected_index(5), Some(4)); // zawija

        // Siatka: pion = ±GRID_COLS, poziom = ±1.
        let mut grid = Launcher {
            view: LauncherView::Grid,
            ..Launcher::default()
        };
        grid.navigate(24, 0, 1);
        assert_eq!(grid.selected_index(24), Some(GRID_COLS));
        grid.navigate(24, 1, 0);
        assert_eq!(grid.selected_index(24), Some(GRID_COLS + 1));
        grid.navigate(24, 0, -5);
        assert_eq!(grid.selected_index(24), Some(1)); // nie wychodzi nad wiersz 0

        // Karuzela: steruje indeksem karty.
        let mut car = Launcher {
            view: LauncherView::Carousel,
            ..Launcher::default()
        };
        car.navigate(6, 1, 0);
        assert_eq!(car.selected_index(6), Some(1));
        assert_eq!(car.carousel, 1);
    }

    #[test]
    fn selection_scrolls_into_view() {
        let theme = Theme::light();
        let text = TextEngine::none();
        let apps = App::sample();
        let mut launcher = Launcher {
            view: LauncherView::List,
            ..Launcher::default()
        };
        // Wybierz ostatnią pozycję i upewnij się, że lista się przewinęła.
        launcher.navigate(apps.len(), 0, -1);
        ensure_selection_visible(&apps, &mut launcher, area(), &theme, &text);
        assert!(
            launcher.scroll > 0.0,
            "zaznaczenie poza ekranem ma przewijać"
        );
    }

    #[test]
    fn clock_and_user_rects_and_session_hit() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let st = PanelState::sample();

        let cr = clock_rect(bar, &theme, &text, &st);
        assert!(bar.contains(cr.center()));

        let ur = user_rect(bar, &theme, &text, &st);
        assert!(bar.contains(ur.center()));

        let items = crate::power::items();
        let srect = session_menu_rect(bar, &theme);
        assert_eq!(
            session_item_at(bar, &theme, Point::new(srect.x() + 10.0, srect.y() + 20.0)),
            Some(items[0].action)
        );
    }

    #[test]
    fn volume_and_wifi_popup_hit_tests() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let st = PanelState::sample();

        let vrect = volume_popup_rect(bar, &theme, &text, &st);
        assert!(vrect.width() > 100.0);
        let vhit = volume_popup_hit(
            vrect,
            &theme,
            Point::new(vrect.center().x, vrect.y() + 56.0),
        );
        assert!(matches!(vhit, Some(VolumeAction::SetLevel(_))));

        let wrect = wifi_popup_rect(bar, &theme, &text, &st);
        assert!(wrect.width() > 100.0);
        let networks = vec![crate::system::WifiNetwork {
            ssid: "TestNet".into(),
            signal: 80,
            active: false,
        }];
        let whit = wifi_popup_hit(
            wrect,
            &theme,
            Point::new(wrect.x() + 20.0, wrect.y() + 90.0),
            None,
            &networks,
        );
        assert_eq!(whit, Some(WifiAction::SelectNetwork("TestNet".into())));
    }

    #[test]
    fn calendar_days_and_weekdays_calculation() {
        assert_eq!(days_in_month(2026, 10), 31);
        assert_eq!(days_in_month(2024, 2), 29); // leap year
        assert_eq!(days_in_month(2025, 2), 28); // non-leap year

        // 2026-10-01 to czwartek -> indeks 3 (0=Pn, 1=Wt, 2=Śr, 3=Cz)
        assert_eq!(first_weekday_of_month(2026, 10), 3);

        let theme = Theme::dark();
        let bar = Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS);
        let cal_rect = calendar_popup_rect(bar, &theme);
        assert!(cal_rect.width() > 200.0);

        let prev_hit = calendar_popup_hit(
            cal_rect,
            &theme,
            Point::new(cal_rect.x() + 20.0, cal_rect.y() + 85.0),
            2026,
            10,
        );
        assert_eq!(prev_hit, Some(CalendarAction::PrevMonth));
    }

    /// Benchmark powłoki: pełny koszt klatki `chrome` (1920x680) — render sceny plus
    /// konwersja RGBA→BGRA do `wl_shm` (ta sama co w `libluna-platform::present`).
    ///
    /// Uruchom: `cargo test --release -p luna bench_shell_chrome -- --nocapture --ignored`
    #[test]
    #[ignore = "benchmark reczny"]
    fn bench_shell_chrome_1920x680() {
        use libluna_core::{Color, Size};
        use libluna_render::SoftwareRenderer;
        use libluna_widgets::PaintCtx;
        use std::time::Instant;

        let (w, h) = (1920u32, 680u32);
        let theme = Theme::dark();
        let text = TextEngine::system();
        let apps = App::sample();
        let mut launcher = Launcher::new();
        launcher.view = LauncherView::Grid;
        let mut st = PanelState::sample();
        st.tasks.clear();

        let bar = Rect::from_xywh(0.0, 0.0, w as f64, THICKNESS);
        let larea = Rect::from_xywh(
            (w as f64 - LAUNCHER_W) / 2.0,
            THICKNESS + 24.0,
            LAUNCHER_W,
            LAUNCHER_H,
        );

        let mut renderer = SoftwareRenderer::new(w, h);
        let mut scene = Scene::new();
        let n = 60u32;

        // 1) Render sceny (to, co robi `luna-shell` w każdej klatce animacji).
        let t = Instant::now();
        for _ in 0..n {
            scene.clear();
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            ctx.scene.fill_rect(
                Rect::from_size(Size::new(w as f64, h as f64)),
                Color::from_rgba8(0, 0, 0, 75),
            );
            paint_panel(&st, &mut ctx, bar);
            paint_launcher(&apps, &launcher, &mut ctx, larea);
            renderer.draw(&scene, 1.0, Color::rgba(0.0, 0.0, 0.0, 0.0));
        }
        let ms_render = t.elapsed().as_secs_f64() * 1000.0 / n as f64;

        // 2) Konwersja do BGRA (jak `present()` w `libluna-platform`).
        let mut dst = vec![0u8; w as usize * h as usize * 4];
        let pixels = renderer.data();
        let t = Instant::now();
        for _ in 0..n {
            for (d, s) in dst.chunks_exact_mut(4).zip(pixels.chunks_exact(4)) {
                d[0] = s[2];
                d[1] = s[1];
                d[2] = s[0];
                d[3] = s[3];
            }
        }
        let ms_swizzle = t.elapsed().as_secs_f64() * 1000.0 / n as f64;

        // 3) Dowód regresji: ta sama klatka **bez czyszczenia sceny** — komendy sumują
        //    się i koszt rośnie liniowo (tak powłoka spadała do 1–5 FPS po sekundzie).
        scene.clear();
        let mut no_clear_ms = 0.0;
        for frame in 0..120u32 {
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            ctx.scene.fill_rect(
                Rect::from_size(Size::new(w as f64, h as f64)),
                Color::from_rgba8(0, 0, 0, 75),
            );
            paint_panel(&st, &mut ctx, bar);
            paint_launcher(&apps, &launcher, &mut ctx, larea);
            let t = Instant::now();
            renderer.draw(&scene, 1.0, Color::rgba(0.0, 0.0, 0.0, 0.0));
            if frame == 119 {
                no_clear_ms = t.elapsed().as_secs_f64() * 1000.0;
            }
        }

        println!(
            "shell chrome 1920x680: render {ms_render:.2} ms + swizzle {ms_swizzle:.2} ms = {:.2} ms/klatke ({} fps)",
            ms_render + ms_swizzle,
            if ms_render + ms_swizzle > 0.0 {
                1000.0 / (ms_render + ms_swizzle)
            } else {
                0.0
            }
        );
        println!(
            "  [regresja] 120. klatka BEZ clear(): {no_clear_ms:.1} ms ({:.1} fps)",
            if no_clear_ms > 0.0 {
                1000.0 / no_clear_ms
            } else {
                0.0
            }
        );
    }

    // ── Regresje układu paska ───────────────────────────────────────────────────

    /// Pigułki przełączników muszą iść jedna za drugą (bez nachodzenia) i kończyć się
    /// przed przyciskiem zasilania. Pigułka bez wartości tekstowej mieści samą ikonę.
    #[test]
    fn toggle_pills_never_overlap() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let bar = Rect::from_xywh(0.0, 0.0, 1920.0, THICKNESS);
        let st = PanelState::sample();
        let rects = toggle_rects(bar, &theme, &text, &st);
        assert_eq!(rects.len(), st.toggles.len());
        for w in rects.windows(2) {
            assert!(
                w[0].right() <= w[1].x(),
                "pigułki nachodzą: {:?} / {:?}",
                w[0],
                w[1]
            );
        }
        let power = power_rect(bar, &theme);
        assert!(rects.last().unwrap().right() <= power.x());
        // Sieć i dźwięk: sama ikona (bez nazwy sieci / „Dźwięk”).
        let wifi = &st.toggles[0];
        assert!(wifi.value.is_none());
        assert!(rects[0].width() < 40.0, "pigułka Wi-Fi powinna być wąska");
    }

    /// Kropki przestrzeni roboczych leżą **na lewo** od całej grupy pigułek i nie
    /// nachodzą na siebie ani na pigułki (wcześniej rysowały się na pigułce dźwięku).
    #[test]
    fn workspace_dots_sit_left_of_toggles() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let bar = Rect::from_xywh(0.0, 0.0, 1920.0, THICKNESS);
        let mut st = PanelState::sample();
        st.workspaces = 4;
        st.current_ws = 2;
        let toggles = toggle_rects(bar, &theme, &text, &st);
        let dots = workspace_rects(bar, &theme, &text, &st);
        assert_eq!(dots.len(), 4);
        assert!(dots.last().unwrap().right() <= toggles[0].x());
        for w in dots.windows(2) {
            assert!(w[0].right() <= w[1].x());
        }
        for (i, d) in dots.iter().enumerate() {
            assert_eq!(workspace_at(bar, &theme, &text, &st, d.center()), Some(i));
        }
    }

    /// Kliknięcie w listę/siatkę trafia we właściwą aplikację także po przewinięciu.
    #[test]
    fn launcher_click_respects_scroll() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let area = Rect::from_xywh(100.0, 60.0, LAUNCHER_W, LAUNCHER_H);
        let apps = App::sample();
        let content = launcher_rects(area, &theme, &text).content;
        let probe = Rect::from_xywh(content.x() + 8.0, content.y() + 8.0, 8.0, 8.0).center();

        let mut l = Launcher::new();
        l.view = LauncherView::List;
        assert_eq!(
            launcher_app_at(&apps, &l, area, &theme, &text, probe),
            Some(0)
        );
        l.scroll = LIST_ROW_H + 8.0;
        assert_eq!(
            launcher_app_at(&apps, &l, area, &theme, &text, probe),
            Some(1)
        );
    }

    /// Nawigacja strzałkami w siatce: w bok po kolumnach, w dół po wierszach,
    /// bez wychodzenia poza zakres.
    #[test]
    fn launcher_grid_navigation() {
        let mut l = Launcher::new();
        l.view = LauncherView::Grid;
        l.selected = Some(0);
        l.navigate(12, 1, 0);
        assert_eq!(l.selected, Some(1));
        l.navigate(12, -1, 0);
        assert_eq!(l.selected, Some(0));
        l.navigate(12, 0, 1);
        assert_eq!(l.selected, Some(GRID_COLS));
        l.selected = Some(GRID_COLS);
        l.navigate(12, -1, 0);
        assert_eq!(l.selected, Some(GRID_COLS));
    }

    /// Przewijanie kółkiem: jeden ząbek ≈ jeden wiersz, ograniczone do zakresu.
    #[test]
    fn launcher_wheel_scroll_is_bounded() {
        let mut l = Launcher::new();
        l.view = LauncherView::List;
        l.scroll_by_notches(1.0, 64.0, 500.0);
        assert_eq!(l.scroll, 64.0);
        l.scroll_by_notches(1.0, 64.0, 500.0);
        assert_eq!(l.scroll, 128.0);
        l.scroll_by_notches(-5.0, 64.0, 500.0);
        assert_eq!(l.scroll, 0.0);
        l.scroll_by_notches(100.0, 64.0, 500.0);
        assert_eq!(l.scroll, 500.0);
    }
}
