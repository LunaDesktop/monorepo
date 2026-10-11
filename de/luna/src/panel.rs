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

/// Wysokość panelu (piksele logiczne).
pub const THICKNESS: f64 = 40.0;
/// Domyślna szerokość karty launchera.
pub const LAUNCHER_W: f64 = 720.0;
/// Domyślna wysokość karty launchera.
pub const LAUNCHER_H: f64 = 600.0;

const GRID_COLS: usize = 4;
const GRID_CELL_H: f64 = 76.0;
const LIST_ROW_H: f64 = 52.0;
const CARD_W: f64 = 168.0;
const CARD_H: f64 = 200.0;

/// Przełącznik szybkiego dostępu (Wi-Fi, dźwięk, bateria…).
#[derive(Debug, Clone)]
pub struct Toggle {
    pub label: String,
    /// Ikona z pakietu Luny.
    pub icon: IconName,
    pub active: bool,
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
                },
                Toggle {
                    label: "Dźwięk".to_owned(),
                    icon: IconName::Volume,
                    active: true,
                },
                Toggle {
                    label: "82%".to_owned(),
                    icon: IconName::Battery,
                    active: false,
                },
            ],
            user: "patryks".to_owned(),
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

/// Stan launchera (widok + bieżąca karta karuzeli).
#[derive(Debug, Clone, Default)]
pub struct Launcher {
    pub view: LauncherView,
    pub carousel: usize,
}

impl Launcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Przełącza na kolejny widok.
    pub fn next_view(&mut self) {
        self.view = self.view.next();
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
}

impl App {
    /// Przykładowy zestaw ikon (podglądy).
    pub fn sample() -> Vec<App> {
        const APPS: [(IconName, &str, &str); 8] = [
            (IconName::Files, "Pliki", "System"),
            (IconName::Settings, "Ustawienia", "System"),
            (IconName::Edit, "Edytor", "Akcesoria"),
            (IconName::Music, "Odtwarzacz", "Multimedia"),
            (IconName::Star, "Ulubione", "System"),
            (IconName::Clock, "Zegar", "Akcesoria"),
            (IconName::Terminal, "Terminal", "System"),
            (IconName::Store, "Sklep", "System"),
        ];
        APPS.iter()
            .map(|(i, n, c)| App {
                icon: *i,
                name: (*n).to_owned(),
                category: (*c).to_owned(),
            })
            .collect()
    }
}

/// Prostokąt przycisku burgera na panelu.
pub fn burger_rect(panel: Rect, theme: &Theme) -> Rect {
    let cy = panel.center().y;
    Rect::from_xywh(panel.x() + theme.spacing.md, cy - 14.0, 28.0, 28.0)
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
    ctx.label_left(x, cy, &panel.title, body, weight::NORMAL, t.colors.dim_text);

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

    // ── Prawa grupa (od prawej): użytkownik, przełączniki, przestrzenie ──
    let pad = t.spacing.md;
    let mut right = rect.right() - pad;
    let uw = ctx.measure(&panel.user, body, weight::NORMAL);
    ctx.label_right(right, cy, &panel.user, body, weight::NORMAL, t.colors.text);
    right -= uw + t.spacing.md;
    ctx.scene
        .fill_rect(Rect::from_xywh(right, cy - 8.0, 1.0, 16.0), t.colors.border);
    right -= t.spacing.md;

    for toggle in panel.toggles.iter().rev() {
        let cap = t.typography.caption.size;
        let lw = ctx.measure(&toggle.label, cap, weight::MEDIUM);
        let icon = 13.0;
        let w = t.spacing.sm * 2.0 + icon + t.spacing.xs + lw;
        let pill = Rect::from_xywh(right - w, cy - 12.0, w, 24.0);
        paint_toggle_pill(ctx, pill, toggle);
        right = pill.x() - t.spacing.sm;
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
    match launcher.view {
        LauncherView::Grid => (0..apps.len()).find(|&i| grid_tile(content, i).contains(p)),
        LauncherView::List => (0..apps.len()).find(|&i| list_row(content, i).contains(p)),
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

    // Liczba pozycji mieszczących się w obszarze zawartości (bez przewijania).
    let max_grid = (((r.content.height() + 12.0) / (GRID_CELL_H + 12.0))
        .floor()
        .max(0.0) as usize)
        * GRID_COLS;
    let max_list = ((r.content.height() + 8.0) / (LIST_ROW_H + 8.0))
        .floor()
        .max(0.0) as usize;

    match launcher.view {
        LauncherView::Grid => {
            for (i, app) in apps.iter().take(max_grid).enumerate() {
                paint_grid_tile(app, ctx, grid_tile(r.content, i));
            }
        }
        LauncherView::List => {
            for (i, app) in apps.iter().take(max_list).enumerate() {
                paint_list_row(app, ctx, list_row(r.content, i));
            }
        }
        LauncherView::Carousel => {
            for (i, app) in apps.iter().enumerate() {
                let card = carousel_card(r.content, i, launcher.carousel);
                // Rysujemy tylko karty mieszczące się całe w obszarze (brak obcinania tekstu).
                if card.x() < r.content.x() || card.right() > r.content.right() {
                    continue;
                }
                paint_carousel_card(app, i == launcher.carousel, ctx, card);
            }
        }
    }
}

/// Kafelek siatki: ikona + nazwa.
fn paint_grid_tile(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.medium, 1.0, t.colors.border);

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

/// Wiersz listy: ikona + nazwa + kategoria.
fn paint_list_row(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.medium, 1.0, t.colors.border);

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
        let launcher = Launcher { view, carousel: 2 };
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
            let launcher = Launcher { view, carousel: 2 };
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
}
