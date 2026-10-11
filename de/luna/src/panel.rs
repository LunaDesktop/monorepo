//! Panel górny i launcher środowiska Luna.
//!
//! Warstwa rysowania jest oddzielona od źródła danych ([`PanelState`], [`App`]), więc
//! panel można renderować headless (podglądy) oraz „na żywo” w sesji Selene.
//!
//! Panel ma po lewej **sam przycisk burger** (bez etykiety) — kliknięcie otwiera launcher.
//! Launcher ma trzy widoki: **lista**, **siatka** i **karuzela**.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Size, Theme};
use libluna_render::TextEngine;
use libluna_widgets::{paint_icon, IconName, PaintCtx};

use crate::toplevels::Task;

/// Wysokość panelu (piksele logiczne).
pub const THICKNESS: f64 = 40.0;
/// Domyślna szerokość karty launchera.
pub const LAUNCHER_W: f64 = 720.0;
/// Domyślna wysokość karty launchera.
pub const LAUNCHER_H: f64 = 600.0;
/// Bok przycisku zasilania w pasku.
pub const POWER_BTN: f64 = 26.0;
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
}

/// Przełącznik szybkiego dostępu (Wi-Fi, dźwięk, bateria…).
#[derive(Debug, Clone)]
pub struct Toggle {
    pub label: String,
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
                    icon: IconName::Wifi,
                    active: true,
                    action: ToggleAction::Wifi,
                },
                Toggle {
                    label: "Dźwięk".to_owned(),
                    icon: IconName::Volume,
                    active: true,
                    action: ToggleAction::Volume,
                },
                Toggle {
                    label: "82%".to_owned(),
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
                },
                Task {
                    title: "Terminal".to_owned(),
                    app_id: "terminal".to_owned(),
                    active: false,
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

/// Stan launchera (widok + bieżąca karta karuzeli + przewinięcie).
#[derive(Debug, Clone, Default)]
pub struct Launcher {
    pub view: LauncherView,
    pub carousel: usize,
    /// Zaznaczona pozycja (nawigacja klawiaturą); `None` = brak zaznaczenia.
    pub selected: Option<usize>,
    /// Pionowe przewinięcie zawartości (Lista/Siatka), w pikselach.
    pub scroll: f64,
}

impl Launcher {
    pub fn new() -> Self {
        Self::default()
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
        let icon = 13.0;
        let lw = text.measure(&toggle.label, cap, weight::MEDIUM).width;
        let w = t.spacing.sm * 2.0 + icon + t.spacing.xs + lw;
        let pill = Rect::from_xywh(right - w, cy - 12.0, w, 24.0);
        rects[i] = pill;
        right = pill.x() - t.spacing.sm;
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
    toggle_rects(bar, theme, text, state)
        .iter()
        .position(|r| r.contains(p))
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

/// Rozmieszczenie przycisków okien (taskbar) w pasku.
pub fn task_rects(bar: Rect, theme: &Theme, text: &TextEngine, tasks: &[Task]) -> Vec<Rect> {
    let t = theme;
    let cy = bar.center().y;
    let cap = t.typography.caption.size;
    // Start: za burgerem + separator + odstęp (spójne z `paint_panel`).
    let mut x = burger_rect(bar, t).right() + t.spacing.md + 1.0 + t.spacing.md;
    let limit = bar.center().x - t.spacing.lg;
    let mut rects = Vec::new();
    for task in tasks {
        let label = task_label(task);
        let w = (text.measure(&label, cap, weight::MEDIUM).width + 2.0 * t.spacing.md).min(200.0);
        if x + w > limit {
            break;
        }
        rects.push(Rect::from_xywh(x, cy - 12.0, w, 24.0));
        x += w + t.spacing.sm;
    }
    rects
}

/// Indeks okna (przycisku) pod punktem `p`.
pub fn task_at(
    bar: Rect,
    theme: &Theme,
    text: &TextEngine,
    tasks: &[Task],
    p: Point,
) -> Option<usize> {
    task_rects(bar, theme, text, tasks)
        .iter()
        .position(|r| r.contains(p))
}

#[cfg(test)]
mod taskbar_tests {
    use super::*;

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
            },
            Task {
                title: "Terminal".into(),
                app_id: "term".into(),
                active: false,
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

/// Rysuje przycisk okna w taskbarze (aktywny — akcent).
fn paint_task_button(ctx: &mut PaintCtx, rect: Rect, task: &Task) {
    let t = ctx.theme;
    let label = task_label(task);
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
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        &label,
        cap,
        weight::MEDIUM,
        fg,
    );
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
        ctx.label_left(x, cy, &panel.title, body, weight::NORMAL, t.colors.dim_text);
    } else {
        let rects = task_rects(rect, t, ctx.text, &panel.tasks);
        for (r, task) in rects.iter().zip(panel.tasks.iter()) {
            paint_task_button(ctx, *r, task);
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
    let pisz = POWER_BTN * 0.6;
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
    right -= t.spacing.md;

    // Przełączniki szybkiego dostępu (pigułki).
    let toggles = toggle_rects(rect, t, ctx.text, panel);
    for (r, toggle) in toggles.iter().zip(panel.toggles.iter()) {
        paint_toggle_pill(ctx, *r, toggle);
    }
    if let Some(last) = toggles.last() {
        right = last.x() - t.spacing.sm;
    }

    for i in (0..panel.workspaces).rev() {
        let d = 8.0;
        let dot = Rect::from_xywh(right - d, cy - d / 2.0, d, d);
        let color = if i == panel.current_ws {
            t.colors.accent
        } else {
            t.colors.dim_text
        };
        ctx.scene.fill_rounded_rect(dot, d / 2.0, color);
        right = dot.x() - t.spacing.sm;
    }
}

/// Pigułka przełącznika szybkiego dostępu (ikona + etykieta).
fn paint_toggle_pill(ctx: &mut PaintCtx, rect: Rect, toggle: &Toggle) {
    let t = ctx.theme;
    let bg = if toggle.active {
        t.colors.accent
    } else {
        t.colors.card_bg
    };
    let fg = if toggle.active {
        t.colors.accent_fg
    } else {
        t.colors.text
    };
    ctx.scene.fill_rounded_rect(rect, rect.height() / 2.0, bg);
    if !toggle.active {
        ctx.scene
            .stroke_rounded_rect(rect, rect.height() / 2.0, 1.0, t.colors.border);
    }
    let isz = 13.0;
    let ix = rect.x() + t.spacing.sm;
    paint_icon(
        ctx,
        toggle.icon,
        Rect::from_xywh(ix, rect.center().y - isz / 2.0, isz, isz),
        fg,
    );
    ctx.label_left(
        ix + isz + t.spacing.xs,
        rect.center().y,
        &toggle.label,
        t.typography.caption.size,
        weight::MEDIUM,
        fg,
    );
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
    let content = launcher_rects(area, theme, text).content;
    let sy = launcher.scroll;
    match launcher.view {
        LauncherView::Grid => (0..apps.len()).find(|&i| {
            let t = grid_tile(content, i);
            Rect::from_xywh(t.x(), t.y() - sy, t.width(), t.height()).contains(p)
        }),
        LauncherView::List => (0..apps.len()).find(|&i| {
            let t = list_row(content, i);
            Rect::from_xywh(t.x(), t.y() - sy, t.width(), t.height()).contains(p)
        }),
        LauncherView::Carousel => {
            (0..apps.len()).find(|&i| carousel_card(content, i, launcher.carousel).contains(p))
        }
    }
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
    let content = launcher_rects(area, theme, text).content;
    let gap_list = 8.0;
    let gap_grid = 12.0;
    let total = match launcher.view {
        LauncherView::List => (apps.len() as f64) * (LIST_ROW_H + gap_list) - gap_list,
        LauncherView::Grid => {
            let rows = apps.len().div_ceil(GRID_COLS) as f64;
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
    let Some(i) = launcher.selected else {
        return;
    };
    if i >= apps.len() {
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
    let max = launcher_scroll_max(apps, launcher, area, theme, text);
    launcher.scroll = launcher.scroll.clamp(0.0, max);
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

    // Pole wyszukiwania (makieta).
    ctx.scene
        .fill_rounded_rect(r.search, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(r.search, t.radius.medium, 1.0, t.colors.border);
    ctx.label_left(
        r.search.x() + t.spacing.md,
        r.search.center().y,
        "Szukaj…",
        t.typography.body.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    // Zawartość przewijana pionowo (Lista/Siatka) — obcinana do karty.
    let scroll = launcher.scroll;
    ctx.scene.push_clip(r.content);
    match launcher.view {
        LauncherView::Grid => {
            for (i, app) in apps.iter().enumerate() {
                let t = grid_tile(r.content, i);
                paint_grid_tile(
                    app,
                    launcher.selected_index(apps.len()) == Some(i),
                    ctx,
                    Rect::from_xywh(t.x(), t.y() - scroll, t.width(), t.height()),
                );
            }
        }
        LauncherView::List => {
            for (i, app) in apps.iter().enumerate() {
                let t = list_row(r.content, i);
                paint_list_row(
                    app,
                    launcher.selected_index(apps.len()) == Some(i),
                    ctx,
                    Rect::from_xywh(t.x(), t.y() - scroll, t.width(), t.height()),
                );
            }
        }
        LauncherView::Carousel => {
            for (i, app) in apps.iter().enumerate() {
                paint_carousel_card(
                    app,
                    i == launcher.carousel,
                    ctx,
                    carousel_card(r.content, i, launcher.carousel),
                );
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
}
