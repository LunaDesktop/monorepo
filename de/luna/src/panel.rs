//! Panel górny i launcher środowiska Luna.
//!
//! Warstwa rysowania jest oddzielona od źródła danych ([`PanelState`], [`App`]), więc
//! panel można renderować headless (podglądy) oraz „na żywo” w sesji Selene.

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Size, Theme};
use libluna_widgets::PaintCtx;

/// Wysokość panelu (piksele logiczne).
pub const THICKNESS: f64 = 40.0;

/// Przełącznik szybkiego dostępu (Wi-Fi, dźwięk, jasność…).
#[derive(Debug, Clone)]
pub struct Toggle {
    pub label: String,
    /// Symbol rysowany fontem interfejsu (może być pusty).
    pub glyph: String,
    pub active: bool,
}

/// Stan panelu — czyste dane, bez logiki okna (łatwe do testów i podglądów).
#[derive(Debug, Clone)]
pub struct PanelState {
    /// Etykieta przycisku menu (np. „Aktywności”).
    pub menu_label: String,
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
            menu_label: "Aktywności".to_owned(),
            title: "Pliki — Dokumenty".to_owned(),
            clock: "14:32".to_owned(),
            date: "pon, 10 paź".to_owned(),
            workspaces: 4,
            current_ws: 1,
            toggles: vec![
                Toggle {
                    label: "Wi-Fi".to_owned(),
                    glyph: "▲".to_owned(),
                    active: true,
                },
                Toggle {
                    label: "Dźwięk".to_owned(),
                    glyph: "◆".to_owned(),
                    active: true,
                },
                Toggle {
                    label: "82%".to_owned(),
                    glyph: "■".to_owned(),
                    active: false,
                },
            ],
            user: "patryks".to_owned(),
        }
    }
}

/// Pozycje w launcherze.
#[derive(Debug, Clone)]
pub struct App {
    /// Symbol rysowany fontem interfejsu.
    pub glyph: String,
    pub name: String,
}

impl App {
    /// Przykładowy zestaw ikon (podglądy).
    pub fn sample() -> Vec<App> {
        const APPS: [(&str, &str); 8] = [
            ("❖", "Pliki"),
            ("▦", "Ustawienia"),
            ("✎", "Edytor"),
            ("▶", "Odtwarzacz"),
            ("★", "Ulubione"),
            ("◔", "Zegar"),
            ("⌗", "Terminal"),
            ("✦", "Sklep"),
        ];
        APPS.iter()
            .map(|(g, n)| App {
                glyph: (*g).to_owned(),
                name: (*n).to_owned(),
            })
            .collect()
    }
}

/// Rysuje panel górny w zadanym prostokącie.
pub fn paint_panel(panel: &PanelState, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    let cy = rect.center().y;
    let body = t.typography.body.size;
    let pad = t.spacing.md;

    // Tło + dolna krawędź oddzielająca od pulpitu.
    ctx.scene.fill_rect(rect, t.colors.headerbar_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(rect.x(), rect.bottom() - 1.0, rect.width(), 1.0),
        t.colors.border,
    );

    // ── Lewa grupa: przycisk menu + tytuł + separator ──
    let menu = Rect::from_xywh(rect.x() + pad, cy - 12.0, 24.0, 24.0);
    ctx.scene
        .fill_rounded_rect(menu, t.radius.small, t.colors.shade);
    ctx.label_center(
        menu.center().x,
        menu.center().y,
        "☰",
        body,
        weight::BOLD,
        t.colors.text,
    );
    let mut x = menu.right() + t.spacing.sm;
    ctx.label_left(
        x,
        cy,
        &panel.menu_label,
        body,
        weight::MEDIUM,
        t.colors.text,
    );
    x += ctx.measure(&panel.menu_label, body, weight::MEDIUM) + t.spacing.md;
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
    let mut right = rect.right() - pad;
    let uw = ctx.measure(&panel.user, body, weight::NORMAL);
    ctx.label_right(right, cy, &panel.user, body, weight::NORMAL, t.colors.text);
    right -= uw + t.spacing.md;
    ctx.scene
        .fill_rect(Rect::from_xywh(right, cy - 8.0, 1.0, 16.0), t.colors.border);
    right -= t.spacing.md;

    for toggle in panel.toggles.iter().rev() {
        let label = if toggle.glyph.is_empty() {
            toggle.label.clone()
        } else {
            format!("{} {}", toggle.glyph, toggle.label)
        };
        let cap = t.typography.caption.size;
        let w = ctx.measure(&label, cap, weight::MEDIUM) + t.spacing.lg;
        let pill = Rect::from_xywh(right - w, cy - 12.0, w, 24.0);
        paint_pill(ctx, pill, &label, toggle.active);
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

/// Pigułka przełącznika (aktywna = akcent).
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

/// Rysuje launcher (menu aplikacji) w zadanym prostokącie.
pub fn paint_launcher(apps: &[App], ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.large, t.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.large, 1.0, t.colors.border);

    let pad = t.spacing.lg;
    ctx.label_left(
        rect.x() + pad,
        rect.y() + pad + 6.0,
        "Aplikacje",
        t.typography.heading.size,
        weight::BOLD,
        t.colors.text,
    );

    // Pole wyszukiwania (makieta).
    let search = Rect::from_xywh(
        rect.x() + pad,
        rect.y() + pad + 28.0,
        rect.width() - 2.0 * pad,
        32.0,
    );
    ctx.scene
        .fill_rounded_rect(search, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(search, t.radius.medium, 1.0, t.colors.border);
    ctx.label_left(
        search.x() + t.spacing.md,
        search.center().y,
        "Szukaj…",
        t.typography.body.size,
        weight::NORMAL,
        t.colors.dim_text,
    );

    // Siatka ikon (4 kolumny).
    let cols = 4usize;
    let gap = t.spacing.md;
    let gx = rect.x() + pad;
    let gy = search.bottom() + t.spacing.lg;
    let cell_w = (rect.width() - 2.0 * pad - gap * (cols as f64 - 1.0)) / cols as f64;
    let cell_h = 76.0;
    for (i, app) in apps.iter().enumerate() {
        let c = (i % cols) as f64;
        let r = (i / cols) as f64;
        let tile = Rect::from_xywh(
            gx + c * (cell_w + gap),
            gy + r * (cell_h + gap),
            cell_w,
            cell_h,
        );
        paint_app_tile(app, ctx, tile);
    }
}

/// Kafelek aplikacji: ikona + nazwa.
fn paint_app_tile(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let t = ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, t.radius.medium, t.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(rect, t.radius.medium, 1.0, t.colors.border);

    // Kwadratowa ikona w akcencie (delikatnym), z symbolem.
    let icon = Rect::from_xywh(rect.center().x - 18.0, rect.y() + 12.0, 36.0, 36.0);
    ctx.scene
        .fill_rounded_rect(icon, t.radius.small, t.colors.accent.scale_alpha(0.16));
    ctx.label_center(
        icon.center().x,
        icon.center().y,
        &app.glyph,
        18.0,
        weight::MEDIUM,
        t.colors.accent,
    );
    ctx.label_center(
        rect.center().x,
        icon.bottom() + 12.0,
        &app.name,
        t.typography.caption.size,
        weight::NORMAL,
        t.colors.text,
    );
}

/// Pełny podgląd pulpitu: tapeta, panel górny i launcher.
pub fn paint_desktop(panel: &PanelState, apps: &[App], ctx: &mut PaintCtx, size: Size) {
    let accent = ctx.theme.colors.accent;

    // Tapeta: tło + delikatny motyw z akcentu.
    ctx.scene
        .fill_rect(Rect::from_size(size), ctx.theme.colors.window_bg);
    let blob = Point::new(size.width * 0.78, size.height * 0.74);
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(blob.x - 220.0, blob.y - 220.0, 440.0, 440.0),
        220.0,
        accent.scale_alpha(0.10),
    );

    paint_panel(panel, ctx, Rect::from_xywh(0.0, 0.0, size.width, THICKNESS));

    let lw = 620.0;
    let lh = 360.0;
    let lrect = Rect::from_xywh(
        (size.width - lw) / 2.0,
        (size.height - lh) / 2.0 + 16.0,
        lw,
        lh,
    );
    paint_launcher(apps, ctx, lrect);
}

/// Wyciszony kolor akcentu (pomocniczo dla tapet/podświetleń).
pub fn accent_soft(theme: &Theme) -> Color {
    theme.colors.accent.scale_alpha(0.12)
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::{Scene, TextEngine};

    fn panel_commands(apps: usize) -> usize {
        let theme = Theme::light();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        let panel = PanelState::sample();
        paint_panel(
            &panel,
            &mut ctx,
            Rect::from_xywh(0.0, 0.0, 1280.0, THICKNESS),
        );
        if apps > 0 {
            paint_launcher(
                &App::sample()[..apps],
                &mut ctx,
                Rect::from_xywh(0.0, 0.0, 620.0, 360.0),
            );
        }
        scene.len()
    }

    #[test]
    fn sample_state_is_consistent() {
        let s = PanelState::sample();
        assert!(s.workspaces > 0);
        assert!(s.current_ws < s.workspaces);
        assert!(!s.toggles.is_empty());
    }

    #[test]
    fn panel_emits_draw_commands() {
        assert!(panel_commands(0) > 10);
    }

    #[test]
    fn launcher_draws_more_with_more_apps() {
        assert!(panel_commands(8) > panel_commands(4));
    }
}
