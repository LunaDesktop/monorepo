//! Rysowanie i obsługa wejścia Luna Welcome.
//!
//! Układ liczymy raz ([`layout`]) i używamy go zarówno do rysowania, jak i do
//! trafień. Dzięki temu przycisk instalatora i wiersze skrótów nie „rozjeżdżają
//! się” względem obrazu po przewinięciu.

use libluna_core::style::weight;
use libluna_core::{ButtonState, Event, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, Banner, Button, ButtonVariant, Card, ControlsSide, HeaderBar, IconName, PaintCtx,
    WindowControl,
};

use crate::app::{Action, App};
use crate::news::{Notes, Release};

const PAD: f64 = 28.0;
const CONTENT_MAX: f64 = 900.0;
const HERO_H: f64 = 168.0;
const FOOTER_H: f64 = 34.0;
const ROW_H: f64 = 54.0;
const SCROLL_STEP: f64 = 64.0;
const SCROLL_UNIT: f64 = 20.0;

/// Geometria widoku.
#[derive(Debug, Clone)]
pub struct Layout {
    pub header: Rect,
    pub viewport: Rect,
    pub footer: Rect,
    pub content: Rect,
    pub hero: Rect,
    pub banner: Option<Rect>,
    pub news: Rect,
    pub quick: Rect,
    /// Akcje klikalne (przycisk instalatora + wiersze „Zacznij od…”).
    pub actions: Vec<(Action, Rect)>,
    /// Wysokość całej treści (do paska przewijania).
    pub content_height: f64,
    /// Bieżące przesunięcie treści.
    pub offset: f64,
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

    let hero = Rect::from_xywh(content.x(), content.y(), width, HERO_H);

    let mut y = hero.bottom() + 16.0;
    let banner = app.live.live.then(|| {
        let r = Rect::from_xywh(content.x(), y, width, 92.0);
        y = r.bottom() + 16.0;
        r
    });

    let news_rows = news_rows(&app.notes, theme, text, width - 2.0 * theme.spacing.lg);
    let news_h =
        theme.metrics.card.header + news_height(&news_rows, theme) + 2.0 * theme.spacing.md;

    // Najpierw skróty „Get started”, żeby były widoczne bez przewijania;
    // „What's new” zostaje pod nimi i może się przewijać.
    let quick_h = theme.metrics.card.header + 3.0 * ROW_H + theme.spacing.md;
    let quick = Rect::from_xywh(content.x(), y, width, quick_h);
    y = quick.bottom() + 16.0;

    let news = Rect::from_xywh(content.x(), y, width, news_h);
    y = news.bottom() + 16.0;

    let content_height = (y - content.y()).max(0.0);
    let max_offset = (content_height - viewport.height()).max(0.0);
    let offset = app.scroll.offset().min(max_offset);

    // Pozycje po uwzględnieniu przewinięcia.
    let shift = |r: Rect| Rect::from_xywh(r.x(), r.y() - offset, r.width(), r.height());
    let hero = shift(hero);
    let banner = banner.map(shift);
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
    let rows_top = quick.y() + theme.metrics.card.header + theme.spacing.xs;
    for (index, action) in [Action::Store, Action::Settings, Action::Docs]
        .into_iter()
        .enumerate()
    {
        let row = Rect::from_xywh(
            quick.x() + 1.0,
            rows_top + index as f64 * ROW_H,
            quick.width() - 2.0,
            ROW_H,
        );
        actions.push((action, row));
    }

    Layout {
        header,
        viewport,
        footer,
        content,
        hero,
        banner,
        news,
        quick,
        actions,
        content_height,
        offset,
    }
}

fn live_banner(app: &App) -> Banner {
    let banner = Banner::new(tr("You're running the Luna live image"))
        .description(tr(
            "Nothing is written to your disk until you confirm in the installer.",
        ))
        .action(tr("Install Luna"));
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
            NewsRow::Heading { .. } => h += body.line_height + theme.spacing.sm,
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

    paint_hero(app, &mut ctx, lay.hero);

    if let (Some(area), true) = (lay.banner, app.live.live) {
        paint_live_banner(app, &mut ctx, area);
    }

    paint_quick(app, &mut ctx, lay.quick, &lay.actions);

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

    paint_footer(app, &mut ctx, lay.footer);
}

fn paint_hero(app: &App, ctx: &mut PaintCtx, area: Rect) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let radius = theme.radius.large;
    ctx.scene.fill_rounded_rect(area, radius, colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(area, radius, 1.0, colors.border);

    let pad = theme.spacing.lg;
    let icon_size = 68.0;
    let icon = Rect::from_xywh(area.x() + pad, area.y() + pad, icon_size, icon_size);
    ctx.scene
        .fill_rounded_rect(icon, icon_size / 2.0, colors.accent.scale_alpha(0.14));
    paint_icon(
        ctx,
        IconName::App,
        icon.deflate(Insets::splat(16.0)),
        colors.accent,
    );

    let text_x = icon.right() + theme.spacing.lg;
    let max_w = (area.right() - pad - text_x).max(80.0);
    let title = tr("Welcome to Luna");
    ctx.label_left(
        text_x,
        icon.center().y - 8.0,
        &title,
        theme.typography.title.size,
        weight::BOLD,
        colors.text,
    );

    let mode = if app.live.live {
        tr("live image")
    } else {
        tr("installed system")
    };
    let subtitle = tr_args(
        "Luna {version} · {mode}",
        &[("version", &app.version), ("mode", &mode)],
    );
    ctx.label_left(
        text_x,
        icon.center().y + 22.0,
        &subtitle,
        theme.typography.body.size,
        weight::NORMAL,
        colors.dim_text,
    );

    let intro = tr("A calm, fast desktop built on libLuna. See what's new, then start exploring.");
    let lines = wrap_text(
        ctx.text,
        &intro,
        theme.typography.body.size,
        weight::NORMAL,
        max_w,
    );
    for (index, line) in lines.iter().take(3).enumerate() {
        ctx.label_left(
            area.x() + pad,
            icon.bottom() + 18.0 + index as f64 * theme.typography.body.line_height,
            line,
            theme.typography.body.size,
            weight::NORMAL,
            colors.dim_text,
        );
    }
}

fn paint_live_banner(app: &App, ctx: &mut PaintCtx, area: Rect) {
    let banner = live_banner(app);
    banner.paint(ctx, area);
    let rects = banner.rects(area, ctx.theme, ctx.text);
    if let Some(button_rect) = rects.actions.first().copied() {
        let mut button = Button::new(tr("Install Luna")).variant(ButtonVariant::Suggested);
        button.set_hovered(app.hover == Some(Action::Install));
        button.paint(ctx, button_rect);
    }
}

fn paint_news(app: &App, ctx: &mut PaintCtx, area: Rect) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let card = Card::new(tr("What's new"));
    let content = card.paint(ctx, area);
    let pad = theme.spacing.lg;
    let width = (content.width() - 2.0 * pad).max(80.0);
    let rows = news_rows(&app.notes, &theme, ctx.text, width);
    let body = theme.typography.body;
    let caption = theme.typography.caption;

    let mut y = content.y() + pad;
    for row in rows {
        match row {
            NewsRow::Heading { version, date } => {
                ctx.label_left(
                    content.x() + pad,
                    y + body.line_height / 2.0,
                    &version,
                    body.size,
                    weight::BOLD,
                    colors.text,
                );
                if !date.is_empty() {
                    ctx.label_right(
                        content.right() - pad,
                        y + body.line_height / 2.0,
                        &date,
                        caption.size,
                        weight::NORMAL,
                        colors.dim_text,
                    );
                }
                y += body.line_height + theme.spacing.sm;
            }
            NewsRow::Bullet(lines) => {
                let dot_y = y + body.line_height * 0.55;
                ctx.scene.fill_rounded_rect(
                    Rect::from_xywh(content.x() + pad + 5.0, dot_y - 2.5, 5.0, 5.0),
                    2.5,
                    colors.accent,
                );
                for line in lines {
                    ctx.label_left(
                        content.x() + pad + 20.0,
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
        if y > area.bottom() {
            break;
        }
    }
}

fn paint_quick(app: &App, ctx: &mut PaintCtx, area: Rect, actions: &[(Action, Rect)]) {
    let theme = *ctx.theme;
    let colors = theme.colors;
    let card = Card::new(tr("Get started"));
    card.paint(ctx, area);

    let rows = [
        (
            Action::Store,
            IconName::Store,
            tr("Install and update software"),
        ),
        (
            Action::Settings,
            IconName::Settings,
            tr("Themes, network and accounts"),
        ),
        (Action::Docs, IconName::Info, tr("Guides and API reference")),
    ];
    let body = theme.typography.body;
    let caption = theme.typography.caption;

    for (action, icon, subtitle) in rows {
        let Some(rect) = actions
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, rect)| *rect)
        else {
            continue;
        };
        let hovered = app.hover == Some(action);
        if hovered {
            ctx.scene.fill_rounded_rect(
                rect.deflate(Insets::symmetric(6.0, 0.0)),
                theme.radius.medium,
                colors.shade,
            );
        }
        let icon_rect = Rect::from_xywh(
            rect.x() + theme.spacing.md,
            rect.center().y - 15.0,
            30.0,
            30.0,
        );
        ctx.scene.fill_rounded_rect(
            icon_rect,
            theme.radius.medium,
            colors.accent.scale_alpha(0.12),
        );
        paint_icon(
            ctx,
            icon,
            icon_rect.deflate(Insets::splat(7.0)),
            colors.accent,
        );
        let text_x = icon_rect.right() + theme.spacing.md;
        ctx.label_left(
            text_x,
            rect.center().y - 9.0,
            &action.label(),
            body.size,
            weight::MEDIUM,
            colors.text,
        );
        ctx.label_left(
            text_x,
            rect.center().y + 12.0,
            &subtitle,
            caption.size,
            weight::NORMAL,
            colors.dim_text,
        );
        let chevron = Rect::from_xywh(rect.right() - 34.0, rect.center().y - 8.0, 16.0, 16.0);
        paint_icon(ctx, IconName::ChevronRight, chevron, colors.dim_text);
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

fn paint_footer(app: &App, ctx: &mut PaintCtx, area: Rect) {
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
    ctx.label_left(
        area.x() + PAD,
        area.center().y,
        &label,
        caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
    ctx.label_right(
        area.right() - PAD,
        area.center().y,
        &tr("Press Esc to close"),
        caption.size,
        weight::NORMAL,
        colors.dim_text,
    );
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
    app.scroll.set_content_height(lay.content_height);
    app.scroll.set_offset(lay.offset, lay.viewport);

    for event in events {
        match event {
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                app.hover = hit_action(&lay, *position);
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.hover = None;
            }
            Event::Scroll(scroll) if lay.viewport.contains(scroll.position) => {
                app.scroll
                    .on_scroll(scroll.dy / SCROLL_UNIT * SCROLL_STEP, lay.viewport);
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                handle_click(app, &lay, theme, button.position);
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
    if let Some(action) = hit_action(lay, point) {
        app.launch(action);
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
}
