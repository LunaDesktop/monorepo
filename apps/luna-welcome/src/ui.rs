//! Rysowanie i obsługa wejścia Luna Welcome.
//!
//! Układ liczymy raz ([`layout`]) i używamy go zarówno do rysowania, jak i do
//! trafień. Dzięki temu kafelki, przycisk instalatora i checkbox nie „rozjeżdżają
//! się” względem obrazu po przewinięciu.
//!
//! Wygląd trzyma się języka libLuny: duża karta powitalna z delikatnym akcentem,
//! sekcje z nagłówkami zamiast nagłówków kart, kafelki akcji i timeline nowości.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, IconTheme, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    is_activate_key, paint_focus_ring, paint_icon, paint_icon_themed, Banner, Button,
    ButtonVariant, CheckButton, ControlsSide, HeaderBar, IconName, PaintCtx, ScrollDrag,
    WindowControl,
};

use crate::app::{Action, App};
use crate::news::{Notes, Release};

const PAD: f64 = 24.0;
const CONTENT_MAX: f64 = 920.0;
const SECTION_H: f64 = 30.0;
const SECTION_GAP: f64 = 22.0;
const HERO_H_WIDE: f64 = 196.0;
const HERO_H_NARROW: f64 = 196.0;
const FOOTER_H: f64 = 44.0;
const TILE_H_WIDE: f64 = 118.0;
const TILE_H_NARROW: f64 = 78.0;
const SCROLL_STEP: f64 = 64.0;
/// Poniżej tej szerokości treści kafelki układają się w jedną kolumnę.
const WIDE_MIN: f64 = 700.0;

/// Geometria widoku.
#[derive(Debug, Clone)]
pub struct Layout {
    pub header: Rect,
    pub viewport: Rect,
    pub footer: Rect,
    pub content: Rect,
    pub hero: Rect,
    pub banner: Option<Rect>,
    /// Nagłówek sekcji „Zacznij tutaj”.
    pub quick_heading: Rect,
    /// Nagłówek sekcji „Co nowego”.
    pub news_heading: Rect,
    pub news: Rect,
    /// Obszar siatki kafelków akcji.
    pub quick: Rect,
    /// Checkbox „Nie uruchamiaj ponownie po zalogowaniu” w stopce.
    pub autostart: Rect,
    /// Akcje klikalne (przycisk instalatora + kafelki „Zacznij tutaj”).
    pub actions: Vec<(Action, Rect)>,
    /// Wysokość całej treści (do paska przewijania).
    pub content_height: f64,
    /// Bieżące przesunięcie treści.
    pub offset: f64,
    /// Czy kafelki są w rzędzie (szerokie okno).
    pub wide: bool,
}

/// Wiersz „Co nowego” — nagłówek wydania albo zawinięty punkt.
#[derive(Debug, Clone, PartialEq)]
pub enum NewsRow {
    Heading { version: String, date: String },
    Bullet(Vec<String>),
}

/// Oblicza układ dla bieżącego stanu i rozmiaru okna.
pub fn layout(app: &App, size: Size, theme: &Theme, text: &TextEngine) -> Layout {
    let header = Rect::from_xywh(
        0.0,
        0.0,
        size.width,
        theme.metrics.headerbar.height.min(size.height),
    );
    let footer = Rect::from_xywh(
        0.0,
        (size.height - FOOTER_H).max(header.bottom()),
        size.width,
        FOOTER_H,
    );
    let viewport = Rect::from_xywh(
        0.0,
        header.bottom(),
        size.width,
        (footer.y() - header.bottom()).max(0.0),
    );

    let width = (viewport.width() - 2.0 * PAD).min(CONTENT_MAX).max(0.0);
    let x = viewport.x() + (viewport.width() - width) / 2.0;
    let content = Rect::from_xywh(x, viewport.y() + PAD, width, 0.0);

    let wide = width >= WIDE_MIN;
    let hero_h = if wide { HERO_H_WIDE } else { HERO_H_NARROW };
    let hero = Rect::from_xywh(content.x(), content.y(), width, hero_h);

    let mut y = hero.bottom() + SECTION_GAP;
    let banner = app.live.live.then(|| {
        let r = Rect::from_xywh(content.x(), y, width, 92.0);
        y = r.bottom() + SECTION_GAP;
        r
    });

    // „Zacznij tutaj” — nagłówek sekcji, potem kafelki akcji.
    let quick_heading = Rect::from_xywh(content.x(), y, width, SECTION_H);
    y = quick_heading.bottom() + 10.0;
    let cols = if wide { 3 } else { 1 };
    let gap = if wide { 12.0 } else { 8.0 };
    let tile_h = if wide { TILE_H_WIDE } else { TILE_H_NARROW };
    let tile_w = ((width - gap * (cols as f64 - 1.0)) / cols as f64).max(0.0);
    let rows = 3_usize.div_ceil(cols);
    let quick_h = rows as f64 * tile_h + (rows as f64 - 1.0) * gap;
    let quick = Rect::from_xywh(content.x(), y, width, quick_h);
    y = quick.bottom() + SECTION_GAP;

    // „Co nowego” — nagłówek sekcji i panel z timeline.
    let news_heading = Rect::from_xywh(content.x(), y, width, SECTION_H);
    y = news_heading.bottom() + 10.0;
    let news_pad = theme.spacing.lg;
    let rows = news_rows(&app.notes, theme, text, (width - 2.0 * news_pad).max(80.0));
    let news_h = 2.0 * news_pad + news_height(&rows, theme);
    let news = Rect::from_xywh(content.x(), y, width, news_h);
    y = news.bottom() + PAD;

    let content_height = (y - content.y()).max(0.0);
    let max_offset = (content_height - viewport.height()).max(0.0);
    let offset = app.scroll.offset().min(max_offset);

    // Checkbox w stopce — zawsze z marginesem od prawej krawędzi (PAD).
    let check_label = tr("Don't show again at login");
    let check_w = theme.metrics.check.size
        + theme.metrics.check.gap
        + text
            .measure(&check_label, theme.typography.body.size, weight::NORMAL)
            .width;
    let available = (footer.width() - 2.0 * PAD).max(0.0);
    let check_w = check_w.min(available);
    let autostart = Rect::from_xywh(
        footer.right() - PAD - check_w,
        footer.y() + 6.0,
        check_w,
        (footer.height() - 12.0).max(0.0),
    );

    // Pozycje po uwzględnieniu przewinięcia.
    let shift = |r: Rect| Rect::from_xywh(r.x(), r.y() - offset, r.width(), r.height());
    let hero = shift(hero);
    let banner = banner.map(shift);
    let quick_heading = shift(quick_heading);
    let news_heading = shift(news_heading);
    let news = shift(news);
    let quick = shift(quick);

    let mut actions = Vec::new();
    if let Some(area) = banner {
        let banner_widget = live_banner(app);
        let rects = banner_widget.rects(area, theme, text);
        if let Some(install) = rects.actions.first().copied() {
            actions.push((Action::Install, install));
        }
    }
    for (index, action) in [Action::Store, Action::Settings, Action::Docs]
        .into_iter()
        .enumerate()
    {
        let col = index % cols;
        let row = index / cols;
        let rect = Rect::from_xywh(
            quick.x() + col as f64 * (tile_w + gap),
            quick.y() + row as f64 * (tile_h + gap),
            tile_w,
            tile_h,
        );
        actions.push((action, rect));
    }

    Layout {
        header,
        viewport,
        footer,
        content,
        hero,
        banner,
        quick_heading,
        news_heading,
        news,
        quick,
        autostart,
        actions,
        content_height,
        offset,
        wide,
    }
}

fn live_banner(app: &App) -> Banner {
    let system = libluna_core::os_name();
    let banner = Banner::new(tr_args(
        "You're running the {system} live image",
        &[("system", &system)],
    ))
    .description(tr(
        "Nothing is written to your disk until you confirm in the installer.",
    ))
    .action(tr_args("Install {system}", &[("system", &system)]));
    let _ = app;
    banner
}

/// Zawija tekst do zadanej szerokości (bez dzielenia słów).
pub fn wrap_text(
    text: &TextEngine,
    input: &str,
    size: f64,
    weight: u16,
    max_width: f64,
) -> Vec<String> {
    if max_width <= 0.0 {
        return vec![input.to_owned()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in input.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if current.is_empty() || text.measure(&candidate, size, weight).width <= max_width {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_owned();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Przygotowuje wiersze „Co nowego” dla wszystkich wydań.
pub fn news_rows(notes: &Notes, theme: &Theme, text: &TextEngine, width: f64) -> Vec<NewsRow> {
    let body = theme.typography.body;
    let mut rows = Vec::new();
    for release in &notes.releases {
        rows.push(NewsRow::Heading {
            version: release.version_display(),
            date: release.date.clone(),
        });
        for item in &release.items {
            rows.push(NewsRow::Bullet(wrap_text(
                text,
                item,
                body.size,
                weight::NORMAL,
                (width - 22.0).max(40.0),
            )));
        }
    }
    rows
}

fn news_height(rows: &[NewsRow], theme: &Theme) -> f64 {
    let body = theme.typography.body;
    let mut h = 0.0;
    for row in rows {
        match row {
            NewsRow::Heading { .. } => h += 20.0 + theme.spacing.md,
            NewsRow::Bullet(lines) => {
                h += lines.len() as f64 * body.line_height + theme.spacing.xs;
            }
        }
    }
    h
}

impl Release {
    fn version_display(&self) -> String {
        if self.version.is_empty() {
            tr("Highlights").to_owned()
        } else {
            self.version.clone()
        }
    }
}

/// Rysuje całe okno.
pub fn paint(app: &App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    let lay = layout(app, size, theme, text);
    let mut ctx = PaintCtx::new(scene, theme, text);
    let colors = theme.colors;
    ctx.scene.fill_rect(Rect::from_size(size), colors.window_bg);

    let header = HeaderBar::new()
        .title(tr("Welcome to Luna"))
        .controls(ControlsSide::Right);
    let hover_control = app
        .pointer
        .and_then(|p| header.control_at(lay.header, theme, p));
    header.paint(&mut ctx, lay.header, hover_control);

    ctx.scene.push_clip(lay.viewport);

    paint_hero(app, &mut ctx, lay.hero, lay.wide);

    if let (Some(area), true) = (lay.banner, app.live.live) {
        let focused = lay
            .actions
            .iter()
            .position(|(action, _)| *action == Action::Install)
            .is_some_and(|index| app.focus.is_focused(index));
        paint_live_banner(app, &mut ctx, area, focused);
    }

    section_heading(&mut ctx, lay.quick_heading, &tr("Get started"));
    paint_tiles(app, &mut ctx, &lay);

    section_heading(&mut ctx, lay.news_heading, &tr("What's new"));
    paint_news(app, &mut ctx, lay.news);

    if let Some(error) = &app.error {
        paint_error_banner(&mut ctx, error, lay.viewport);
    }

    ctx.scene.pop_clip();

    // Pasek przewijania musi znać wysokość treści.
    let mut scroll = app.scroll;
    scroll.set_content_height(lay.content_height);
    scroll.set_offset(lay.offset, lay.viewport);
    scroll.paint_scrollbar(&mut ctx, lay.viewport);

    paint_footer(app, &mut ctx, lay.footer, lay.autostart, lay.actions.len());
}

/// Karta powitalna: ciepły akcent, księżyc w tle i wersja jako pigułki.
fn paint_hero(app: &App, ctx: &mut PaintCtx, area: Rect, wide: bool) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let radius = theme.radius.large.max(16.0);

    ctx.scene.fill_rounded_rect(area, radius, colors.card_bg);
    ctx.scene.push_clip_rounded(area, radius);

    // Delikatna „zorza” z kół akcentu — bez gradientów, które nie istnieją w scenie.
    let blob = area.width().min(460.0);
    circle(
        ctx,
        Point::new(area.right() - blob * 0.10, area.y() + blob * 0.05),
        blob * 0.52,
        colors.accent.with_alpha(0.10),
    );
    circle(
        ctx,
        Point::new(area.right() - blob * 0.62, area.bottom() + blob * 0.18),
        blob * 0.42,
        colors.accent.with_alpha(0.06),
    );

    if wide {
        let moon_size = 136.0;
        let moon = Rect::from_xywh(
            area.right() - moon_size - 26.0,
            area.y() + (area.height() - moon_size) / 2.0,
            moon_size,
            moon_size,
        );
        paint_icon_themed(
            ctx,
            IconName::Moon,
            moon,
            colors.accent.with_alpha(0.62),
            IconTheme::Monochrome,
        );
    }
    ctx.scene.pop_clip();
    ctx.scene
        .stroke_rounded_rect(area, radius, 1.0, colors.border);

    let pad = theme.spacing.xl.max(24.0);
    let text_x = area.x() + pad;
    let text_w = if wide {
        (area.width() - 2.0 * pad - 170.0).max(180.0)
    } else {
        (area.width() - 2.0 * pad).max(120.0)
    };

    let title_size = (theme.typography.title.size * 1.15).clamp(22.0, 30.0);
    let mut y = area.y() + pad + title_size * 0.55;
    ctx.label_left(
        text_x,
        y,
        &tr("Welcome to Luna"),
        title_size,
        weight::BOLD,
        colors.text,
    );

    y += title_size * 0.9 + 16.0;
    let mut x = text_x;
    x += pill(
        ctx,
        x,
        y,
        &format!("Luna {}", app.version),
        colors.accent.scale_alpha(0.15),
        colors.accent,
    ) + 8.0;
    let mode = if app.live.live {
        tr("live image")
    } else {
        tr("installed system")
    };
    let _ = pill(ctx, x, y, &mode, colors.shade, colors.dim_text);

    y += 24.0 + 12.0;
    let intro = tr("A calm, fast desktop built on libLuna. See what's new, then start exploring.");
    let lines = wrap_text(
        ctx.text,
        &intro,
        theme.typography.body.size,
        weight::NORMAL,
        text_w,
    );
    for line in lines.iter().take(3) {
        ctx.label_left(
            text_x,
            y,
            line,
            theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
        y += theme.typography.body.line_height;
    }
}

fn paint_live_banner(app: &App, ctx: &mut PaintCtx, area: Rect, focused: bool) {
    let banner = live_banner(app);
    banner.paint(ctx, area);
    let rects = banner.rects(area, ctx.theme, ctx.text);
    if let Some(button_rect) = rects.actions.first().copied() {
        let system = libluna_core::os_name();
        let mut button = Button::new(tr_args("Install {system}", &[("system", &system)]))
            .variant(ButtonVariant::Suggested);
        button.set_hovered(app.hover == Some(Action::Install));
        button.set_focused(focused);
        button.paint(ctx, button_rect);
    }
}

/// Nagłówek sekcji: mały akcentowy pasek + tytuł.
fn section_heading(ctx: &mut PaintCtx, area: Rect, title: &str) {
    let theme = *ctx.theme;
    let bar = Rect::from_xywh(area.x(), area.center().y - 8.0, 4.0, 16.0);
    ctx.scene.fill_rounded_rect(bar, 2.0, theme.colors.accent);
    ctx.label_left(
        area.x() + 14.0,
        area.center().y,
        title,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
}

/// Kafelki „Zacznij tutaj” — w rzędzie na szerokim oknie, w kolumnie na wąskim.
fn paint_tiles(app: &App, ctx: &mut PaintCtx, lay: &Layout) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let radius = theme.radius.large;
    let body = theme.typography.body;
    let caption = theme.typography.caption;

    for (action, icon) in [
        (Action::Store, IconName::Store),
        (Action::Settings, IconName::Settings),
        (Action::Docs, IconName::Info),
    ] {
        let Some(rect) = lay
            .actions
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, rect)| *rect)
        else {
            continue;
        };
        let focused = lay
            .actions
            .iter()
            .position(|(candidate, _)| *candidate == action)
            .is_some_and(|index| app.focus.is_focused(index));
        let hovered = app.hover == Some(action);
        ctx.scene.fill_rounded_rect(rect, radius, colors.card_bg);
        if hovered {
            ctx.scene.fill_rounded_rect(rect, radius, colors.shade);
            ctx.scene
                .stroke_rounded_rect(rect, radius, 1.5, colors.accent);
        } else {
            ctx.scene
                .stroke_rounded_rect(rect, radius, 1.0, colors.border);
        }

        let icon_size = if lay.wide { 38.0 } else { 32.0 };
        let icon_rect = Rect::from_xywh(
            rect.x() + 14.0,
            rect.center().y - icon_size / 2.0,
            icon_size,
            icon_size,
        );
        ctx.scene
            .fill_rounded_rect(icon_rect, icon_size * 0.32, colors.accent.scale_alpha(0.14));
        paint_icon(
            ctx,
            icon,
            icon_rect.deflate(Insets::splat(icon_size * 0.22)),
            colors.accent,
        );

        let text_x = icon_rect.right() + 12.0;
        let max_text = (rect.right() - 36.0 - text_x).max(48.0);
        let title = fit_text(ctx, &action.label(), body.size, weight::MEDIUM, max_text);
        ctx.label_left(
            text_x,
            rect.center().y - 9.0,
            &title,
            body.size,
            weight::MEDIUM,
            colors.text,
        );
        let subtitle = fit_text(
            ctx,
            &tile_subtitle(action),
            caption.size,
            weight::NORMAL,
            max_text,
        );
        ctx.label_left(
            text_x,
            rect.center().y + 12.0,
            &subtitle,
            caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        paint_icon(
            ctx,
            IconName::ChevronRight,
            Rect::from_xywh(rect.right() - 26.0, rect.center().y - 8.0, 16.0, 16.0),
            colors.dim_text,
        );
        if focused {
            paint_focus_ring(ctx, rect, radius);
        }
    }
}

fn tile_subtitle(action: Action) -> String {
    match action {
        Action::Store => tr("Install and update software"),
        Action::Settings => tr("Themes, network and accounts"),
        Action::Docs => tr("Guides and API reference"),
        _ => String::new(),
    }
}

fn paint_news(app: &App, ctx: &mut PaintCtx, area: Rect) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    panel(ctx, area);

    let pad = theme.spacing.lg;
    let content = area.deflate(Insets::splat(pad));
    let width = (content.width()).max(80.0);
    let rows = news_rows(&app.notes, &theme, ctx.text, width);
    let body = theme.typography.body;
    let caption = theme.typography.caption;

    let mut y = content.y();
    for row in rows {
        if y > content.bottom() - 4.0 {
            break;
        }
        match row {
            NewsRow::Heading { version, date } => {
                let badge_w = ctx.measure(&version, caption.size, weight::MEDIUM) + 18.0;
                let badge = Rect::from_xywh(content.x(), y, badge_w, 20.0);
                ctx.scene
                    .fill_rounded_rect(badge, 10.0, colors.accent.scale_alpha(0.16));
                ctx.label_center(
                    badge.center().x,
                    badge.center().y,
                    &version,
                    caption.size,
                    weight::MEDIUM,
                    colors.accent,
                );
                if !date.is_empty() {
                    ctx.label_right(
                        content.right(),
                        y + 10.0,
                        &date,
                        caption.size,
                        weight::NORMAL,
                        colors.dim_text,
                    );
                }
                y += 20.0 + theme.spacing.md;
            }
            NewsRow::Bullet(lines) => {
                let dot_y = y + body.line_height * 0.55;
                ctx.scene.fill_rounded_rect(
                    Rect::from_xywh(content.x() + 5.0, dot_y - 2.5, 5.0, 5.0),
                    2.5,
                    colors.accent,
                );
                for line in lines {
                    ctx.label_left(
                        content.x() + 20.0,
                        y + body.line_height / 2.0,
                        &line,
                        body.size,
                        weight::NORMAL,
                        colors.text,
                    );
                    y += body.line_height;
                }
                y += theme.spacing.xs;
            }
        }
    }
}

fn paint_error_banner(ctx: &mut PaintCtx, message: &str, viewport: Rect) {
    let width = viewport.width().min(720.0) - 32.0;
    let area = Rect::from_xywh(
        viewport.center().x - width / 2.0,
        viewport.y() + 12.0,
        width,
        52.0,
    );
    Banner::new(tr("Something went wrong"))
        .kind(libluna_widgets::BannerKind::Error)
        .description(message)
        .paint(ctx, area);
}

fn paint_footer(
    app: &App,
    ctx: &mut PaintCtx,
    area: Rect,
    autostart: Rect,
    autostart_index: usize,
) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let caption = theme.typography.caption;
    ctx.scene.fill_rect(area, colors.window_bg);
    ctx.scene.fill_rect(
        Rect::from_xywh(area.x(), area.y(), area.width(), 1.0),
        colors.border,
    );
    let mode = if app.live.live {
        tr("live image")
    } else {
        tr("installed system")
    };
    let label = tr_args(
        "Luna {version} · {mode}",
        &[("version", &app.version), ("mode", &mode)],
    );
    let available = (autostart.x() - area.x() - 2.0 * PAD - theme.spacing.lg).max(0.0);
    let label = if available < 60.0 {
        String::new()
    } else if ctx.measure(&label, caption.size, weight::NORMAL) <= available {
        label
    } else {
        fit_text(
            ctx,
            &format!("Luna {}", app.version),
            caption.size,
            weight::NORMAL,
            available,
        )
    };
    let label_w = if label.is_empty() {
        0.0
    } else {
        ctx.measure(&label, caption.size, weight::NORMAL)
    };
    if !label.is_empty() {
        ctx.label_left(
            area.x() + PAD,
            area.center().y,
            &label,
            caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }

    let mut check = CheckButton::new(tr("Don't show again at login"));
    check.set_checked(app.dont_show_again.checked());
    check.set_hovered(app.dont_show_again_hovered);
    check.set_focused(app.focus.is_focused(autostart_index));
    check.paint(ctx, autostart);

    // Podpowiedź o Esc — tylko jeśli nie nachodzi na wersję systemu.
    let hint = tr("Press Esc to close");
    let hint_w = ctx.measure(&hint, caption.size, weight::NORMAL);
    let hint_right = autostart.x() - theme.spacing.lg;
    if hint_right - hint_w > area.x() + PAD + label_w + theme.spacing.lg {
        ctx.label_right(
            hint_right,
            area.center().y,
            &hint,
            caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
}

/// Panel bez nagłówka — spójny z kartami libLuny, ale z własnym nagłówkiem sekcji.
fn panel(ctx: &mut PaintCtx, area: Rect) {
    let theme = *ctx.theme;
    ctx.scene
        .fill_rounded_rect(area, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(area, theme.radius.large, 1.0, theme.colors.border);
}

/// Okrąg z prostokąta o zaokrąglonym promieniu (scena nie ma osobnego prymitywu).
fn circle(ctx: &mut PaintCtx, center: Point, radius: f64, color: Color) {
    ctx.scene.fill_rounded_rect(
        Rect::from_xywh(
            center.x - radius,
            center.y - radius,
            radius * 2.0,
            radius * 2.0,
        ),
        radius,
        color,
    );
}

/// Pigułka z tekstem; zwraca jej szerokość (do układania w rząd).
fn pill(ctx: &mut PaintCtx, x: f64, cy: f64, label: &str, bg: Color, fg: Color) -> f64 {
    let theme = *ctx.theme;
    let size = theme.typography.caption.size;
    let h = 24.0;
    let w = ctx.measure(label, size, weight::MEDIUM) + 20.0;
    let rect = Rect::from_xywh(x, cy - h / 2.0, w, h);
    ctx.scene.fill_rounded_rect(rect, h / 2.0, bg);
    ctx.label_center(
        rect.center().x,
        rect.center().y,
        label,
        size,
        weight::MEDIUM,
        fg,
    );
    w
}

/// Skraca tekst wielokropkiem, żeby nie wychodził z kafelka.
fn fit_text(ctx: &PaintCtx, text: &str, size: f64, weight: u16, max_w: f64) -> String {
    if ctx.measure(text, size, weight) <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        end -= 1;
        let candidate: String = chars[..end].iter().collect::<String>() + "…";
        if ctx.measure(&candidate, size, weight) <= max_w {
            return candidate;
        }
    }
    String::from("…")
}

/// Obsługuje zdarzenia wejścia.
pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let lay = layout(app, size, theme, text);
    app.scroll.configure(lay.viewport, lay.content_height);
    app.focus.set_count(lay.actions.len() + 1);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                if matches!(app.scroll.handle_pointer(event, theme), ScrollDrag::Moved) {
                    continue;
                }
                app.hover = hit_action(&lay, *position);
                app.dont_show_again_hovered = lay.autostart.contains(*position);
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.hover = None;
                app.dont_show_again_hovered = false;
            }
            Event::Scroll(_) => {
                app.scroll.handle_wheel(event);
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                if !matches!(
                    app.scroll.handle_pointer(event, theme),
                    ScrollDrag::Captured
                ) {
                    handle_click(app, &lay, theme, button.position);
                }
            }
            Event::PointerButton(button) if button.state == ButtonState::Released => {
                app.scroll.handle_pointer(event, theme);
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                handle_key(app, &lay, key);
            }
            Event::Window(libluna_core::WindowEvent::CloseRequested) => {
                app.pending.close = true;
            }
            _ => {}
        }
    }
}

fn handle_click(app: &mut App, lay: &Layout, theme: &Theme, point: Point) {
    let header = HeaderBar::new().controls(ControlsSide::Right);
    if let Some(control) = header.control_at(lay.header, theme, point) {
        match control {
            WindowControl::Minimize => app.pending.minimize = true,
            WindowControl::Maximize => app.pending.maximize = true,
            WindowControl::Close => app.pending.close = true,
        }
        return;
    }
    if lay.header.contains(point) {
        app.pending.begin_move = true;
        return;
    }
    if lay.autostart.contains(point) {
        app.focus.focus(lay.actions.len());
        app.toggle_dont_show_again();
        return;
    }
    if let Some(index) = lay
        .actions
        .iter()
        .position(|(_, rect)| rect.contains(point))
    {
        app.focus.focus(index);
        app.launch(lay.actions[index].0);
    }
}

fn hit_action(lay: &Layout, point: Point) -> Option<Action> {
    lay.actions
        .iter()
        .find(|(_, rect)| rect.contains(point))
        .map(|(action, _)| *action)
}

fn handle_key(app: &mut App, lay: &Layout, key: &KeyEvent) {
    if key.modifiers.ctrl && key.keysym == 0x77 {
        app.pending.close = true;
        return;
    }
    if app.focus.handle_key(key) {
        reveal_focus(app, lay);
        return;
    }
    if app.scroll.handle_key(key.keysym) {
        return;
    }
    if app.focus.current().is_some() && is_activate_key(key) {
        match focus_target(lay, app.focus.current()) {
            Some(FocusTarget::Action(action)) => app.launch(action),
            Some(FocusTarget::Autostart) => app.toggle_dont_show_again(),
            None => {}
        }
        return;
    }
    match key.keysym {
        0xff1b => app.pending.close = true,
        0xff0d | 0xff8d => {
            if let Some(action) = app.primary_action() {
                app.launch(action);
            }
        }
        0x3f => app.launch(Action::Docs),
        0xff52 => {
            app.scroll.on_scroll(-SCROLL_STEP, lay.viewport);
        }
        0xff54 => {
            app.scroll.on_scroll(SCROLL_STEP, lay.viewport);
        }
        _ => {}
    }
}

/// Element w kolejności `Tab`: akcje ekranu, potem checkbox w stopce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusTarget {
    Action(Action),
    Autostart,
}

fn focus_target(lay: &Layout, index: Option<usize>) -> Option<FocusTarget> {
    let index = index?;
    match lay.actions.get(index) {
        Some((action, _)) => Some(FocusTarget::Action(*action)),
        None if index == lay.actions.len() => Some(FocusTarget::Autostart),
        None => None,
    }
}

/// Dosuwa treść tak, by fokusowany kafelek był widoczny (stopka jest zawsze widoczna).
fn reveal_focus(app: &mut App, lay: &Layout) {
    let Some(index) = app.focus.current() else {
        return;
    };
    let Some((_, rect)) = lay.actions.get(index) else {
        return;
    };
    let viewport = lay.viewport;
    let margin = 8.0;
    if rect.y() < viewport.y() + margin {
        app.scroll
            .on_scroll(rect.y() - viewport.y() - margin, viewport);
    } else if rect.bottom() > viewport.bottom() - margin {
        app.scroll
            .on_scroll(rect.bottom() - viewport.bottom() + margin, viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LiveEnvironment;
    use crate::news::Notes;

    fn app(live: bool) -> App {
        let notes = Notes::parse("## 0.0.8 — 2026-11-05\n- Nowość\n");
        App::with_environment(
            LiveEnvironment {
                live,
                medium: None,
                reason: String::new(),
            },
            notes,
            "0.0.8".to_owned(),
        )
    }

    fn rect_for(lay: &Layout, action: Action) -> Rect {
        lay.actions
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, rect)| *rect)
            .expect("akcja w układzie")
    }

    #[test]
    fn live_session_has_installer_action() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(true), Size::new(980.0, 760.0), &theme, &text);
        assert!(lay
            .actions
            .iter()
            .any(|(action, _)| *action == Action::Install));
    }

    #[test]
    fn installed_system_hides_installer_action() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(false), Size::new(980.0, 760.0), &theme, &text);
        assert!(!lay
            .actions
            .iter()
            .any(|(action, _)| *action == Action::Install));
    }

    fn key(keysym: u32, shift: bool) -> Event {
        Event::Key(KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: libluna_core::Modifiers {
                shift,
                ..libluna_core::Modifiers::NONE
            },
            repeat: false,
        })
    }

    fn press(app: &mut App, keysym: u32, shift: bool) {
        handle_events(
            app,
            &[key(keysym, shift)],
            Size::new(980.0, 760.0),
            &Theme::light(),
            &TextEngine::system(),
        );
    }

    #[test]
    fn tab_cycles_actions_and_wraps_to_the_checkbox() {
        let mut app = app(false); // Store, Settings, Docs + checkbox
        press(&mut app, 0xff09, false);
        assert!(app.focus.is_focused(0));
        press(&mut app, 0xff09, false);
        press(&mut app, 0xff09, false);
        press(&mut app, 0xff09, false);
        assert!(app.focus.is_focused(3)); // checkbox
        press(&mut app, 0xff09, false);
        assert!(app.focus.is_focused(0));
    }

    #[test]
    fn shift_tab_starts_at_the_checkbox() {
        let mut app = app(false);
        press(&mut app, 0xff09, true);
        assert!(app.focus.is_focused(3));
    }

    #[test]
    fn enter_activates_the_focused_control() {
        let mut app = app(false);
        let before = app.dont_show_again.checked();
        press(&mut app, 0xff09, true); // fokus na checkboxie
        press(&mut app, 0xff0d, false);
        assert_ne!(app.dont_show_again.checked(), before);
    }

    #[test]
    fn losing_the_installer_clamps_focus() {
        let mut app = app(true); // Install, Store, Settings, Docs + checkbox
        press(&mut app, 0xff09, true);
        assert!(app.focus.is_focused(4));
        app.live.live = false;
        handle_events(
            &mut app,
            &[],
            Size::new(980.0, 760.0),
            &Theme::light(),
            &TextEngine::system(),
        );
        assert!(app.focus.is_focused(3));
        assert_eq!(app.focus.current(), Some(3));
    }

    #[test]
    fn wrapped_lines_respect_width() {
        let text = TextEngine::system();
        let lines = wrap_text(
            &text,
            "Luna is a calm, fast desktop built on a small immediate-mode toolkit.",
            14.0,
            400,
            120.0,
        );
        assert!(lines.len() > 1);
        for line in lines {
            assert!(text.measure(&line, 14.0, 400).width <= 121.0);
        }
    }

    #[test]
    fn quick_rows_have_actions() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(false), Size::new(980.0, 760.0), &theme, &text);
        for action in [Action::Store, Action::Settings, Action::Docs] {
            assert!(lay
                .actions
                .iter()
                .any(|(candidate, _)| *candidate == action));
        }
    }

    #[test]
    fn wide_layout_puts_tiles_in_one_row() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(false), Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.wide);
        let store = rect_for(&lay, Action::Store);
        let settings = rect_for(&lay, Action::Settings);
        let docs = rect_for(&lay, Action::Docs);
        assert!((store.y() - settings.y()).abs() < 0.5);
        assert!((store.y() - docs.y()).abs() < 0.5);
        assert!(store.x() < settings.x() && settings.x() < docs.x());
        assert!(store.width() > 200.0);
    }

    #[test]
    fn narrow_layout_stacks_tiles() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(false), Size::new(600.0, 760.0), &theme, &text);
        assert!(!lay.wide);
        let store = rect_for(&lay, Action::Store);
        let settings = rect_for(&lay, Action::Settings);
        let docs = rect_for(&lay, Action::Docs);
        assert!(settings.y() >= store.bottom() - 0.5);
        assert!(docs.y() >= settings.bottom() - 0.5);
        assert!((store.x() - settings.x()).abs() < 0.5);
    }

    #[test]
    fn autostart_checkbox_sits_in_footer_with_margin() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let lay = layout(&app(false), Size::new(980.0, 760.0), &theme, &text);
        assert!(lay.autostart.width() > 0.0);
        assert!(lay.autostart.y() >= lay.footer.y());
        assert!(lay.autostart.bottom() <= lay.footer.bottom());
        // Checkbox nigdy nie dotyka prawej krawędzi okna.
        assert!(lay.autostart.right() <= lay.footer.right() - PAD + 0.5);
        assert!(lay.autostart.right() > lay.footer.right() - PAD - 600.0);
    }

    #[test]
    fn clicking_checkbox_toggles_dont_show_again() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut app = app(false);
        let lay = layout(&app, Size::new(980.0, 760.0), &theme, &text);
        let point = lay.autostart.center();
        handle_click(&mut app, &lay, &theme, point);
        assert!(app.dont_show_again.checked());
        handle_click(&mut app, &lay, &theme, point);
        assert!(!app.dont_show_again.checked());
    }
}
