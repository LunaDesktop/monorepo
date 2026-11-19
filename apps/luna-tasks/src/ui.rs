//! Rysowanie i obsługa zdarzeń menedżera zadań.

use libluna_core::style::weight;
use libluna_core::{
    ButtonState, Color, Event, Insets, KeyEvent, Point, PointerButton, Rect, Size, Theme,
    WindowEvent,
};
use libluna_i18n::{tr, tr_args};
use libluna_render::{Scene, TextEngine};
use libluna_widgets::{
    paint_icon, AlertDialog, Button, ButtonVariant, ControlsSide, HeaderBar, IconName, MenuButton,
    PaintCtx, ScrollDrag, Switch, TitleAlign, WindowControl,
};

use crate::app::{Action, App, ChartHistory, Confirm, ContextState, MenuState, SortKey};
use crate::procs;

/// Wysokość jednego wiersza listy (wspólna z przewijaniem do wyboru).
pub const ROW_HEIGHT: f64 = 38.0;

const TABLE_HEADER_HEIGHT: f64 = 30.0;
const DETAIL_WIDTH: f64 = 272.0;

// Keysyms XKB.
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_DELETE: u32 = 0xffff;
const KEY_UP: u32 = 0xff52;
const KEY_DOWN: u32 = 0xff54;
const KEY_LEFT: u32 = 0xff51;
const KEY_RIGHT: u32 = 0xff53;
const KEY_HOME: u32 = 0xff50;
const KEY_END: u32 = 0xff57;
const KEY_PAGE_UP: u32 = 0xff55;
const KEY_PAGE_DOWN: u32 = 0xff56;
const KEY_F1: u32 = 0xffbe;
const KEY_F5: u32 = 0xffc2;
const KEY_F10: u32 = 0xffc7;
const KEY_A: u32 = 0x61;
const KEY_C: u32 = 0x63;
const KEY_F: u32 = 0x66;
const KEY_G: u32 = 0x67;
const KEY_Q: u32 = 0x71;
const KEY_R: u32 = 0x72;
const KEY_V: u32 = 0x76;
const KEY_X: u32 = 0x78;

/// Wspólny układ okna.
#[derive(Debug, Clone, Copy)]
struct Chrome {
    header: Rect,
    menu: Rect,
    /// Pasek wykresów (gdy włączony i jest dość miejsca).
    charts: Option<Rect>,
    search: Option<Rect>,
    search_button: Rect,
    /// Przełącznik „tylko moje procesy” w pasku tytułu.
    scope: Rect,
    scope_switch: Rect,
    /// Czy przy switchu jest miejsce na podpis „Moje”.
    scope_label: bool,
    list: Rect,
    list_header: Rect,
    list_body: Rect,
    detail: Option<Rect>,
    status: Rect,
}

fn chrome(
    size: Size,
    theme: &Theme,
    text: &TextEngine,
    search_active: bool,
    show_charts: bool,
) -> Chrome {
    let header = Rect::from_xywh(0.0, 0.0, size.width, theme.metrics.headerbar.height);
    let header_bar = HeaderBar::new().controls(ControlsSide::Right);
    let available = header_bar.controls_layout(header, theme).content;
    let menu_h = MenuButton::burger()
        .width(text, theme)
        .clamp(26.0, (header.height() - 8.0).max(26.0));
    let menu = Rect::from_xywh(
        available.right() - menu_h,
        header.center().y - menu_h / 2.0,
        menu_h,
        menu_h,
    );
    // Lupka stoi w pasku tytułu, tuż obok menu i kontrolek okna (jak w Luna Files).
    let search_button = Rect::from_xywh(menu.x() - 4.0 - menu_h, menu.y(), menu_h, menu_h);

    // Kompaktowy przełącznik zakresu tuż obok lupki — nie potrzebuje osobnego
    // wiersza pod nagłówkiem (wcześniej zabierał 44 px + odstęp).
    let cap = theme.typography.caption.size;
    let switch_w = theme.metrics.switch.width;
    let switch_h = theme.metrics.switch.height;
    let label = tr("Mine");
    let label_w = text.measure(&label, cap, weight::NORMAL).width;
    // W bardzo wąskim oknie zostaje sam switch — podpis ustępuje miejsca lupce.
    let scope_label = available.width() >= 480.0;
    let scope_w = switch_w + if scope_label { 6.0 + label_w } else { 0.0 };
    let scope = Rect::from_xywh(
        search_button.x() - 10.0 - scope_w,
        header.center().y - switch_h / 2.0,
        scope_w,
        switch_h,
    );
    let scope_switch = Rect::from_xywh(scope.right() - switch_w, scope.y(), switch_w, switch_h);

    let search_h = theme
        .metrics
        .entry
        .height
        .min((header.height() - 8.0).max(24.0));
    let search = search_active
        .then(|| scope.x() - 8.0 - available.x())
        .filter(|width| *width >= 80.0)
        .map(|width| {
            Rect::from_xywh(
                available.x(),
                header.center().y - search_h / 2.0,
                width,
                search_h,
            )
        });

    let margin = 16.0;
    let status_h = 22.0;
    let status = Rect::from_xywh(
        margin,
        size.height - status_h - 8.0,
        (size.width - 2.0 * margin).max(0.0),
        status_h,
    );

    let content_top = header.bottom() + 12.0;
    let bottom = (status.y() - 12.0).max(content_top);
    let available_h = (bottom - content_top).max(0.0);
    // Wykresy zabierają górny pas: pokazujemy je tylko, gdy zostaje sensowne
    // miejsce na listę (przy bardzo niskim oknie ważniejsza jest lista).
    let charts = (show_charts && available_h >= 300.0).then(|| {
        Rect::from_xywh(
            margin,
            content_top,
            (size.width - 2.0 * margin).max(0.0),
            118.0,
        )
    });
    let top = charts
        .map(|rect| rect.bottom() + 12.0)
        .unwrap_or(content_top);
    let body_h = (bottom - top).max(0.0);
    let body_w = (size.width - 2.0 * margin).max(0.0);
    let gap = 12.0;
    let wide = body_w >= 880.0;
    let (list, detail) = if wide {
        let list_w = (body_w - DETAIL_WIDTH - gap).max(0.0);
        (
            Rect::from_xywh(margin, top, list_w, body_h),
            Some(Rect::from_xywh(
                margin + list_w + gap,
                top,
                DETAIL_WIDTH,
                body_h,
            )),
        )
    } else {
        (Rect::from_xywh(margin, top, body_w, body_h), None)
    };
    let list_header = Rect::from_xywh(
        list.x(),
        list.y(),
        list.width(),
        TABLE_HEADER_HEIGHT.min(list.height()),
    );
    let list_body = Rect::from_xywh(
        list.x(),
        list_header.bottom(),
        list.width(),
        (list.bottom() - list_header.bottom()).max(0.0),
    );

    Chrome {
        header,
        menu,
        charts,
        search,
        search_button,
        scope,
        scope_switch,
        scope_label,
        list,
        list_header,
        list_body,
        detail,
        status,
    }
}

/// Prostokąt przycisku menu — używany też przez podglądy headless.
pub fn menu_anchor(size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    chrome(size, theme, text, false, false).menu
}

fn paint_card(ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    ctx.scene
        .fill_rounded_rect(rect, theme.radius.large, theme.colors.card_bg);
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.large, 1.0, theme.colors.border);
}

fn hit(app: &mut App, action: Action, rect: Rect) {
    app.hits.push((action, rect));
}

fn is_hovered(app: &App, rect: Rect) -> bool {
    app.pointer.is_some_and(|point| rect.contains(point))
}

/// Rysuje całą klatkę i zbiera regiony kliknięć.
pub fn paint(app: &mut App, scene: &mut Scene, size: Size, theme: &Theme, text: &TextEngine) {
    app.hits.clear();

    let layout = chrome(size, theme, text, app.search_active, app.show_charts);
    let mut ctx = PaintCtx::new(scene, theme, text);

    ctx.scene.fill_rect(
        Rect::from_xywh(0.0, 0.0, size.width, size.height),
        theme.colors.window_bg,
    );

    let mut header_bar = HeaderBar::new()
        .title_align(TitleAlign::Start)
        .controls(ControlsSide::Right);
    if !app.search_active && size.width >= 560.0 {
        // Pole wyszukiwania zajmuje miejsce tytułu, jak w aplikacjach GTK.
        header_bar = header_bar.title(tr("Task Manager"));
    }
    header_bar.paint(&mut ctx, layout.header, app.controls_hover);

    paint_search_button(app, &mut ctx, layout.search_button);
    if let Some(search) = layout.search {
        app.search.paint(&mut ctx, search);
    }
    paint_menu_button(app, &mut ctx, layout.menu);
    paint_scope(app, &mut ctx, &layout);
    if let Some(charts) = layout.charts {
        paint_charts(app, &mut ctx, charts);
    }
    paint_list(app, &mut ctx, &layout);
    if let Some(detail) = layout.detail {
        paint_detail(app, &mut ctx, detail);
    }
    paint_status(app, &mut ctx, layout.status);

    if let Some(state) = &mut app.menu {
        // Po zmianie rozmiaru okna menu podąża za przyciskiem w pasku tytułu.
        state.anchor = layout.menu;
    }
    if let Some(state) = &app.menu {
        let panel = menu_panel(state, size, theme, text);
        state.menu.paint(&mut ctx, panel);
    }

    if let Some(state) = &mut app.context {
        let panel = context_panel(state, size, theme, text);
        state.menu.hover_at(panel, app.pointer, theme);
        state.menu.paint(&mut ctx, panel);
    }

    if let Some(confirm) = &app.confirm {
        app.hits.clear();
        confirm_dialog(confirm).paint(&mut ctx, size);
    }
    if let Some(about) = &app.about {
        app.hits.clear();
        about.paint(&mut ctx, size);
    }

    app.toast.paint(&mut ctx, size);
}

fn paint_menu_button(app: &mut App, ctx: &mut PaintCtx, rect: Rect) {
    let mut button = MenuButton::burger();
    button.set_open(app.menu.is_some());
    button.set_hovered(is_hovered(app, rect));
    button.paint(ctx, rect);
}

/// Lupka w pasku tytułu — pokazuje/chowa pole wyszukiwania.
fn paint_search_button(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    if app.search_active || is_hovered(app, rect) {
        let bg = if app.search_active {
            theme.colors.shade
        } else {
            theme.colors.shade.scale_alpha(0.6)
        };
        ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
    }
    paint_icon(
        ctx,
        IconName::Search,
        rect.deflate(Insets::splat(7.0)),
        theme.colors.text,
    );
}

fn menu_panel(state: &MenuState, size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    let width = state.menu.width(text, theme);
    state
        .menu
        .place(state.anchor, width, Rect::from_size(size), theme)
}

fn context_panel(state: &ContextState, size: Size, theme: &Theme, text: &TextEngine) -> Rect {
    let width = state.menu.width(text, theme);
    state
        .menu
        .place(state.anchor, width, Rect::from_size(size), theme)
}

/// Przełącznik „tylko moje procesy” w pasku tytułu (kompaktowo: switch + „Moje”).
fn paint_scope(app: &mut App, ctx: &mut PaintCtx, layout: &Chrome) {
    let theme = *ctx.theme;
    let mut switch = Switch::new(app.only_mine);
    switch.set_hovered(is_hovered(app, layout.scope));
    switch.paint(ctx, layout.scope_switch);
    if layout.scope_label {
        ctx.label_right(
            layout.scope_switch.x() - 6.0,
            layout.scope.center().y,
            &tr("Mine"),
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }
    hit(app, Action::ToggleOnlyMine, layout.scope);
}

/// Trzy mini-wykresy: CPU systemu, pamięć i zaznaczony proces (ostatnie 60 s).
fn paint_charts(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    let gap = 12.0;
    let card_w = ((rect.width() - 2.0 * gap) / 3.0).max(0.0);
    let memory = if app.snapshot.system.memory_total_kib == 0 {
        "—".to_owned()
    } else {
        format!(
            "{:.0}%",
            app.snapshot.system.memory_used_kib() as f64
                / app.snapshot.system.memory_total_kib as f64
                * 100.0
        )
    };
    let process = app
        .selected_process()
        .map(|process| format_cpu(process.cpu_percent))
        .unwrap_or_else(|| "—".to_owned());
    let cards = [
        (
            tr("CPU"),
            format_cpu(app.snapshot.system.cpu_percent),
            &app.charts.system_cpu,
            theme.colors.accent,
        ),
        (
            tr("Memory"),
            memory,
            &app.charts.system_memory,
            theme.colors.warning,
        ),
        (
            tr("Process"),
            process,
            &app.charts.selected_cpu,
            theme.colors.success,
        ),
    ];
    for (index, (title, value, history, color)) in cards.into_iter().enumerate() {
        let card = Rect::from_xywh(
            rect.x() + index as f64 * (card_w + gap),
            rect.y(),
            card_w,
            rect.height(),
        );
        paint_chart_card(ctx, card, &title, &value, history, color);
    }
}

fn paint_chart_card(
    ctx: &mut PaintCtx,
    card: Rect,
    title: &str,
    value: &str,
    history: &ChartHistory,
    color: Color,
) {
    let theme = *ctx.theme;
    paint_card(ctx, card);
    let pad = 12.0;
    let inner = card.deflate(Insets::new(pad, pad, pad, pad));
    let cap = theme.typography.caption.size;
    let body = theme.typography.body.size;

    ctx.label_left_fit(
        inner.x(),
        inner.y() + 8.0,
        title,
        cap,
        weight::MEDIUM,
        theme.colors.dim_text,
        inner.width() * 0.5,
    );
    ctx.label_right_fit(
        inner.right(),
        inner.y() + 8.0,
        value,
        body,
        weight::MEDIUM,
        theme.colors.text,
        inner.width() * 0.45,
    );

    let plot = Rect::from_xywh(
        inner.x(),
        inner.y() + 26.0,
        inner.width(),
        (inner.height() - 26.0).max(0.0),
    );
    if plot.height() <= 0.0 {
        return;
    }
    for fraction in [0.25, 0.5, 0.75] {
        let y = plot.bottom() - plot.height() * fraction;
        ctx.scene.fill_rect(
            Rect::from_xywh(plot.x(), y, plot.width(), 1.0),
            theme.colors.border.scale_alpha(0.6),
        );
    }
    ctx.scene.fill_rect(
        Rect::from_xywh(plot.x(), plot.bottom() - 1.0, plot.width(), 1.0),
        theme.colors.border,
    );

    let len = history.samples.len();
    if len == 0 {
        return;
    }
    let capacity = crate::app::CHART_SAMPLES as f64;
    let step = if capacity > 1.0 {
        plot.width() / (capacity - 1.0)
    } else {
        0.0
    };
    let points: Vec<Point> = history
        .samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let x = plot.right() - (len - 1 - index) as f64 * step;
            let y = plot.bottom() - plot.height() * (*sample as f64).clamp(0.0, 1.0);
            Point::new(x, y)
        })
        .collect();

    if points.len() >= 2 {
        let mut area = points.clone();
        area.push(Point::new(plot.right(), plot.bottom()));
        area.push(Point::new(points[0].x, plot.bottom()));
        ctx.scene.fill_polygon(&area, color.with_alpha(0.16));
        ctx.scene.stroke_polyline(&points, 1.8, color);
    }
    if let Some(last) = points.last() {
        ctx.scene.fill_circle(*last, 2.5, color);
    }
}

struct Columns {
    left: f64,
    name_left: f64,
    name_right: f64,
    cpu_left: f64,
    cpu_right: f64,
    mem_right: f64,
    user_left: f64,
    right: f64,
    mem_visible: bool,
    user_visible: bool,
}

fn columns(area: Rect) -> Columns {
    let left = area.x() + 12.0;
    let right = area.right() - 12.0;
    let user_w = if area.width() >= 600.0 { 108.0 } else { 0.0 };
    let mem_w = if area.width() >= 500.0 { 92.0 } else { 0.0 };
    let cpu_w = 88.0;
    let pid_w = 62.0;
    let user_left = right - user_w;
    // Odstęp między pamięcią a użytkownikiem — bez niego nagłówki i wartości się zlewają.
    let mem_right = if user_w > 0.0 {
        user_left - 10.0
    } else {
        right
    };
    let cpu_right = mem_right - mem_w;
    let cpu_left = cpu_right - cpu_w;
    let name_left = left + pid_w;
    let name_right = (cpu_left - 10.0).max(name_left);
    Columns {
        left,
        name_left,
        name_right,
        cpu_left,
        cpu_right,
        mem_right,
        user_left,
        right,
        mem_visible: mem_w > 0.0,
        user_visible: user_w > 0.0,
    }
}

fn heading_label(app: &App, key: SortKey) -> String {
    let label = tr(key.title_key());
    if app.sort == key {
        format!("{label} {}", if app.sort_asc { "↑" } else { "↓" })
    } else {
        label
    }
}

fn paint_list(app: &mut App, ctx: &mut PaintCtx, layout: &Chrome) {
    let theme = *ctx.theme;
    paint_card(ctx, layout.list);
    ctx.scene.push_clip_rounded(layout.list, theme.radius.large);

    // Nagłówek kolumn (klik przełącza sortowanie).
    ctx.scene.fill_rect(layout.list_header, theme.colors.shade);
    let cols = columns(layout.list);
    let cap = theme.typography.caption.size;
    let cy = layout.list_header.center().y;
    ctx.label_left(
        cols.left,
        cy,
        &heading_label(app, SortKey::Pid),
        cap,
        weight::MEDIUM,
        theme.colors.dim_text,
    );
    ctx.label_left(
        cols.name_left,
        cy,
        &heading_label(app, SortKey::Name),
        cap,
        weight::MEDIUM,
        theme.colors.dim_text,
    );
    ctx.label_right(
        cols.cpu_right,
        cy,
        &heading_label(app, SortKey::Cpu),
        cap,
        weight::MEDIUM,
        theme.colors.dim_text,
    );
    if cols.mem_visible {
        ctx.label_right(
            cols.mem_right,
            cy,
            &heading_label(app, SortKey::Memory),
            cap,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
    }
    if cols.user_visible {
        ctx.label_left(
            cols.user_left,
            cy,
            &heading_label(app, SortKey::User),
            cap,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
    }
    ctx.scene.fill_rect(
        Rect::from_xywh(
            layout.list_header.x(),
            layout.list_header.bottom() - 1.0,
            layout.list_header.width(),
            1.0,
        ),
        theme.colors.border,
    );

    // Kliknięcia w nagłówki: cała wysokość paska, podział na kolumny.
    let header = layout.list_header;
    hit(
        app,
        Action::Sort(SortKey::Pid),
        Rect::from_xywh(
            cols.left,
            header.y(),
            cols.name_left - cols.left,
            header.height(),
        ),
    );
    hit(
        app,
        Action::Sort(SortKey::Name),
        Rect::from_xywh(
            cols.name_left,
            header.y(),
            (cols.name_right - cols.name_left).max(0.0),
            header.height(),
        ),
    );
    hit(
        app,
        Action::Sort(SortKey::Cpu),
        Rect::from_xywh(
            cols.cpu_left,
            header.y(),
            cols.cpu_right - cols.cpu_left,
            header.height(),
        ),
    );
    if cols.mem_visible {
        hit(
            app,
            Action::Sort(SortKey::Memory),
            Rect::from_xywh(
                cols.cpu_right,
                header.y(),
                cols.mem_right - cols.cpu_right,
                header.height(),
            ),
        );
    }
    if cols.user_visible {
        hit(
            app,
            Action::Sort(SortKey::User),
            Rect::from_xywh(
                cols.user_left,
                header.y(),
                cols.right - cols.user_left,
                header.height(),
            ),
        );
    }

    app.list_viewport = layout.list_body;
    let count = app.filtered.len();
    app.scroll
        .configure(layout.list_body, count as f64 * ROW_HEIGHT);
    let offset = app.scroll.offset();

    for visible in 0..count {
        let index = app.filtered[visible];
        let y = layout.list_body.y() + visible as f64 * ROW_HEIGHT - offset;
        let row = Rect::from_xywh(
            layout.list_body.x(),
            y,
            layout.list_body.width(),
            ROW_HEIGHT,
        );
        if row.bottom() < layout.list_body.y() || row.y() > layout.list_body.bottom() {
            continue;
        }
        paint_row(app, ctx, layout, &cols, row, index);
    }

    if count == 0 {
        let message = if app.snapshot.processes.is_empty() {
            tr("No processes")
        } else {
            tr("No processes match the filter")
        };
        ctx.label_center(
            layout.list_body.center().x,
            layout.list_body.center().y,
            &message,
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }

    app.scroll.paint_scrollbar(ctx, layout.list_body);
    ctx.scene.pop_clip();
    ctx.scene
        .stroke_rounded_rect(layout.list, theme.radius.large, 1.0, theme.colors.border);
}

fn paint_row(
    app: &mut App,
    ctx: &mut PaintCtx,
    layout: &Chrome,
    cols: &Columns,
    row: Rect,
    index: usize,
) {
    let theme = *ctx.theme;
    let pid = app.snapshot.processes[index].pid;
    let selected = app.selected == Some(pid);
    let pointer = app.pointer;
    let hovered =
        pointer.is_some_and(|point| row.contains(point) && layout.list_body.contains(point));

    let inset = Rect::from_xywh(
        row.x() + 4.0,
        row.y() + 2.0,
        (row.width() - 8.0).max(0.0),
        (row.height() - 4.0).max(0.0),
    );
    if selected {
        ctx.scene.fill_rounded_rect(
            inset,
            theme.radius.small,
            theme.colors.accent.with_alpha(0.16),
        );
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(
                row.x() + 5.0,
                row.y() + 7.0,
                3.0,
                (row.height() - 14.0).max(0.0),
            ),
            2.0,
            theme.colors.accent,
        );
    } else if hovered {
        ctx.scene
            .fill_rounded_rect(inset, theme.radius.small, theme.colors.shade);
    }

    {
        let process = &app.snapshot.processes[index];
        let total_memory = app.snapshot.system.memory_total_kib;
        let body = theme.typography.body.size;
        let cap = theme.typography.caption.size;
        let cy = row.center().y;
        let name_weight = if selected {
            weight::MEDIUM
        } else {
            weight::NORMAL
        };
        let state_tint = state_color(&theme, process.state);

        // Zablokowane/zombie procesy mają delikatne tło, żeby nie umknęły w tłumie.
        if process.state == 'Z' && !selected {
            ctx.scene.fill_rounded_rect(
                inset,
                theme.radius.small,
                theme.colors.error.with_alpha(0.09),
            );
        }
        // Kropka stanu w kolumnie PID: zielona = działa, szara = śpi, …
        ctx.scene.fill_rounded_rect(
            Rect::from_xywh(cols.name_left - 12.0, cy - 3.0, 6.0, 6.0),
            3.0,
            state_tint,
        );

        ctx.label_left(
            cols.left,
            cy,
            &process.pid.to_string(),
            cap,
            weight::NORMAL,
            theme.colors.dim_text,
        );

        let name_width = (cols.name_right - cols.name_left).max(0.0);
        let name = ellipsize(ctx, &process.name, name_width, body, name_weight);
        let name_color = match process.state {
            'Z' => theme.colors.error,
            'T' | 't' => theme.colors.warning,
            _ => theme.colors.text,
        };
        ctx.label_left(cols.name_left, cy, &name, body, name_weight, name_color);

        let cpu = format_cpu(process.cpu_percent);
        let cpu_color = cpu_color(&theme, process.cpu_percent);
        let bar_w = 34.0;
        let bar = Rect::from_xywh(cols.cpu_left, cy - 2.0, bar_w, 4.0);
        ctx.scene
            .fill_rounded_rect(bar, 2.0, theme.colors.dim_text.with_alpha(0.28));
        let fraction = (process.cpu_percent as f64 / 100.0).clamp(0.0, 1.0);
        let fill_w = if fraction > 0.0 {
            (bar_w * fraction).max(2.0)
        } else {
            0.0
        };
        if fill_w > 0.0 {
            ctx.scene.fill_rounded_rect(
                Rect::from_xywh(bar.x(), bar.y(), fill_w, bar.height()),
                2.0,
                cpu_color,
            );
        }
        ctx.label_right(cols.cpu_right, cy, &cpu, body, weight::NORMAL, cpu_color);

        if cols.mem_visible {
            ctx.label_right(
                cols.mem_right,
                cy,
                &format_memory(process.rss_kib),
                cap,
                weight::NORMAL,
                memory_color(&theme, process.rss_kib, total_memory),
            );
        }
        if cols.user_visible {
            let user_width = (cols.right - cols.user_left).max(0.0);
            let user = ellipsize(ctx, &process.user, user_width, cap, weight::NORMAL);
            ctx.label_left(
                cols.user_left,
                cy,
                &user,
                cap,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        }
    }

    ctx.scene.fill_rect(
        Rect::from_xywh(
            row.x() + 12.0,
            row.bottom() - 1.0,
            (row.width() - 24.0).max(0.0),
            1.0,
        ),
        theme.colors.border.scale_alpha(0.6),
    );

    let clickable = row.intersection(&layout.list_body).unwrap_or(row);
    hit(app, Action::Row(pid), clickable);
}

fn paint_detail(app: &mut App, ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    paint_card(ctx, rect);
    let pad = 16.0;
    let inner = rect.deflate(Insets::new(pad, pad, pad, pad));
    let Some(pid) = app.selected else {
        ctx.label_center(
            inner.center().x,
            inner.center().y,
            &tr("Select a process first"),
            theme.typography.body.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        return;
    };
    // Jednorazowa kopia pól — dalej mutujemy `app` (trafienia przycisków).
    let Some(process) = app.snapshot.processes.iter().find(|p| p.pid == pid) else {
        return;
    };
    let info = DetailInfo {
        pid,
        ppid: process.ppid,
        name: process.name.clone(),
        cmdline: process.cmdline.clone(),
        state: process.state,
        uid: process.uid,
        user: process.user.clone(),
        cpu_percent: process.cpu_percent,
        rss_kib: process.rss_kib,
        threads: process.threads,
        start_jiffies: process.start_jiffies,
    };

    let body = theme.typography.body.size;
    let cap = theme.typography.caption.size;
    let heading_h = theme.typography.heading.line_height;

    let name = ellipsize(
        ctx,
        &info.name,
        inner.width(),
        theme.typography.heading.size,
        weight::BOLD,
    );
    ctx.label_left(
        inner.x(),
        inner.y() + heading_h / 2.0,
        &name,
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        inner.x(),
        inner.y() + heading_h + 10.0,
        &format!("{} · PID {}", tr(procs::state_key(info.state)), info.pid),
        cap,
        weight::NORMAL,
        theme.colors.dim_text,
    );

    let mut y = inner.y() + heading_h + 26.0;
    ctx.scene.fill_rect(
        Rect::from_xywh(inner.x(), y, inner.width(), 1.0),
        theme.colors.border,
    );
    y += 10.0;

    let age = {
        let seconds = info.start_jiffies as f64 / procs::clock_ticks().max(1) as f64;
        let age = app.snapshot.system.uptime.as_secs_f64() - seconds;
        format_age(age)
    };
    let rows = [
        (tr("Parent"), info.ppid.to_string()),
        (tr("User"), info.user.clone()),
        (tr("CPU"), format_cpu(info.cpu_percent)),
        (tr("Memory"), format_memory(info.rss_kib)),
        (tr("Threads"), info.threads.to_string()),
        (tr("Uptime"), age),
    ];
    let label_w = rows
        .iter()
        .map(|(label, _)| ctx.measure(label, cap, weight::NORMAL))
        .fold(0.0_f64, f64::max)
        + 12.0;
    let row_h = theme.typography.body.line_height + 4.0;
    for (label, value) in rows {
        ctx.label_left(
            inner.x(),
            y,
            &label,
            cap,
            weight::NORMAL,
            theme.colors.dim_text,
        );
        let values_x = inner.x() + label_w;
        let value = ellipsize(
            ctx,
            &value,
            (inner.right() - values_x).max(0.0),
            body,
            weight::NORMAL,
        );
        ctx.label_left(values_x, y, &value, body, weight::NORMAL, theme.colors.text);
        y += row_h;
    }

    let button_h = theme.metrics.button.height;
    let gap = 6.0;
    // Przyciski jeden nad drugim: pełna szerokość panelu mieści długie
    // tłumaczenia („Wymuś zamknięcie”) bez przycinania.
    let buttons_top = inner.bottom() - button_h * 2.0 - gap;
    if y + row_h < buttons_top {
        y += 6.0;
        ctx.label_left(
            inner.x(),
            y,
            &tr("Command line"),
            cap,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
        y += theme.typography.caption.line_height + 4.0;
        let max_y = (buttons_top - 6.0).max(y);
        // Wspólny helper libLuny: zawija **i** przycina, więc długie ścieżki
        // (`/home/…/zed-editor`) nie wychodzą za kartę.
        ctx.paragraph(
            Rect::from_xywh(inner.x(), y, inner.width(), (max_y - y).max(0.0)),
            &info.cmdline,
            cap,
            theme.typography.caption.line_height,
            weight::NORMAL,
            theme.colors.text,
            8,
        );
    }

    let can_kill = (info.uid == app.uid || app.uid == 0) && info.pid > 1;
    let end_rect = Rect::from_xywh(inner.x(), buttons_top, inner.width(), button_h);
    let force_rect = Rect::from_xywh(inner.x(), end_rect.bottom() + gap, inner.width(), button_h);

    let mut end = Button::new(tr("End process")).enabled(can_kill);
    end.set_hovered(can_kill && is_hovered(app, end_rect));
    end.paint(ctx, end_rect);
    hit(app, Action::EndSelected, end_rect);

    let mut force = Button::new(tr("Force kill"))
        .variant(ButtonVariant::Destructive)
        .enabled(can_kill);
    force.set_hovered(can_kill && is_hovered(app, force_rect));
    force.paint(ctx, force_rect);
    hit(app, Action::ForceKillSelected, force_rect);
}

/// Pola procesu potrzebne w panelu szczegółów (kopiowane, by nie trzymać
/// pożyczenia `app.snapshot` podczas rysowania i zbierania trafień).
struct DetailInfo {
    pid: u32,
    ppid: u32,
    name: String,
    cmdline: String,
    state: char,
    uid: u32,
    user: String,
    cpu_percent: f32,
    rss_kib: u64,
    threads: u32,
    start_jiffies: u64,
}

fn paint_status(app: &App, ctx: &mut PaintCtx, rect: Rect) {
    let theme = *ctx.theme;
    let cap = theme.typography.caption.size;
    let total = app.snapshot.processes.len();
    let shown = app.filtered.len();
    let mut parts = Vec::new();
    if shown == total {
        parts.push(tr_args(
            "{count} processes",
            &[("count", &total.to_string())],
        ));
    } else {
        parts.push(tr_args(
            "{shown} of {total} processes",
            &[("shown", &shown.to_string()), ("total", &total.to_string())],
        ));
    }
    parts.push(tr_args(
        "CPU {percent}",
        &[("percent", &format_cpu(app.snapshot.system.cpu_percent))],
    ));
    let system = app.snapshot.system;
    if system.memory_total_kib > 0 {
        parts.push(tr_args(
            "Memory {used} of {total}",
            &[
                ("used", &format_memory(system.memory_used_kib())),
                ("total", &format_memory(system.memory_total_kib)),
            ],
        ));
    }
    parts.push(tr_args(
        "Load average {load}",
        &[("load", &format!("{:.2}", system.load[0]))],
    ));
    ctx.label_left(
        rect.x() + 2.0,
        rect.center().y,
        &parts.join("  ·  "),
        cap,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn confirm_dialog(confirm: &Confirm) -> AlertDialog {
    let pid = confirm.pid.to_string();
    let (title, body, action) = if confirm.force {
        (
            tr("Force kill"),
            tr_args(
                "Force-kill \"{name}\" (PID {pid})? Unsaved work may be lost.",
                &[("name", &confirm.name), ("pid", &pid)],
            ),
            tr("Force kill"),
        )
    } else {
        (
            tr("End process"),
            tr_args(
                "End \"{name}\" (PID {pid})? Unsaved work may be lost.",
                &[("name", &confirm.name), ("pid", &pid)],
            ),
            tr("End process"),
        )
    };
    AlertDialog::new(title, body)
        .width(460.0)
        .button(tr("Cancel"))
        .button(action)
}

fn ellipsize(ctx: &PaintCtx, text: &str, width: f64, size: f64, font_weight: u16) -> String {
    if width <= 0.0 {
        return String::new();
    }
    if ctx.measure(text, size, font_weight) <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}…");
        if ctx.measure(&candidate, size, font_weight) > width {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// Procent CPU z jednym miejscem po przecinku tylko dla małych wartości.
pub fn format_cpu(percent: f32) -> String {
    let percent = percent.max(0.0);
    if percent < 0.05 {
        "0%".to_owned()
    } else if percent < 10.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.0}%")
    }
}

/// Pamięć w KiB → czytelny zapis (MiB/GiB).
pub fn format_memory(kib: u64) -> String {
    let mib = kib as f64 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.1} GiB", mib / 1024.0)
    } else if mib >= 100.0 || mib == 0.0 {
        format!("{mib:.0} MiB")
    } else {
        format!("{mib:.1} MiB")
    }
}

fn format_age(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m")
    } else {
        format!("{seconds}s")
    }
}

fn cpu_color(theme: &Theme, percent: f32) -> Color {
    if percent >= 50.0 {
        theme.colors.error
    } else if percent >= 20.0 {
        theme.colors.warning
    } else {
        theme.colors.dim_text
    }
}

/// Kolor kropki stanu procesu.
fn state_color(theme: &Theme, state: char) -> Color {
    match state {
        'R' => theme.colors.success,
        'D' | 'T' | 't' => theme.colors.warning,
        'Z' => theme.colors.error,
        'I' => theme.colors.dim_text.scale_alpha(0.45),
        _ => theme.colors.dim_text.scale_alpha(0.7),
    }
}

/// Pamięć kolorujemy dopiero, gdy proces naprawdę zajmuje dużo RAM-u.
fn memory_color(theme: &Theme, rss_kib: u64, total_kib: u64) -> Color {
    if total_kib == 0 {
        return theme.colors.dim_text;
    }
    let share = rss_kib as f64 / total_kib as f64;
    if share >= 0.25 {
        theme.colors.error
    } else if share >= 0.10 {
        theme.colors.warning
    } else {
        theme.colors.dim_text
    }
}

/// Punkt kotwicy dla menu kontekstowego otwieranego z klawiatury.
fn selected_row_anchor(app: &App) -> Rect {
    let x = app.list_viewport.center().x;
    let y = app
        .selected_row()
        .map(|row| {
            app.list_viewport.y() + row as f64 * ROW_HEIGHT - app.scroll.offset() + ROW_HEIGHT / 2.0
        })
        .unwrap_or(app.list_viewport.y() + 20.0);
    Rect::from_xywh(x, y, 1.0, 1.0)
}

/// Obsługuje zdarzenia wejścia.
pub fn handle_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    for event in events {
        if let Event::Window(WindowEvent::Focused(focused)) = event {
            app.focused = *focused;
        }
    }

    // Modalne okna przechwytują wejście, zanim trafi ono do listy.
    if app.about.is_some() {
        handle_about_events(app, events, size, theme, text);
        return;
    }
    if app.confirm.is_some() {
        handle_confirm_events(app, events, size, theme, text);
        return;
    }
    if app.menu.is_some() {
        handle_menu_events(app, events, size, theme, text);
        return;
    }
    if app.context.is_some() {
        handle_context_events(app, events, size, theme, text);
        return;
    }

    let layout = chrome(size, theme, text, app.search_active, app.show_charts);
    app.list_viewport = layout.list_body;
    app.scroll
        .configure(layout.list_body, app.filtered.len() as f64 * ROW_HEIGHT);
    let header = HeaderBar::new().controls(ControlsSide::Right);

    for event in events {
        // Przeciąganie paska przewijania ma pierwszeństwo przed klikaniem treści.
        if matches!(event, Event::PointerButton(_) | Event::PointerMotion { .. })
            && app.scroll.handle_pointer(event, theme) != ScrollDrag::Ignored
        {
            continue;
        }

        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                if let Some(control) = header.control_at(layout.header, theme, button.position) {
                    match control {
                        WindowControl::Minimize => app.pending_minimize = true,
                        WindowControl::Maximize => app.pending_maximize = true,
                        WindowControl::Close => app.pending_close = true,
                    }
                    continue;
                }

                if layout.menu.contains(button.position) {
                    app.open_menu(layout.menu);
                    continue;
                }

                if layout.search_button.contains(button.position) {
                    app.toggle_search();
                    continue;
                }

                if let Some(search) = layout.search {
                    if search.contains(button.position) {
                        if app.search.clear_at(search, theme, button.position) {
                            app.search.clear();
                            app.recompute();
                        }
                        app.search.set_focused(true);
                        continue;
                    }
                }

                let action = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, rect)| rect.contains(button.position))
                    .map(|(action, _)| *action);
                if let Some(action) = action {
                    apply_action(app, action);
                } else if layout.header.contains(button.position) {
                    app.pending_move = true;
                } else {
                    // Klik poza listą i polem tekstowym zabiera fokus wyszukiwaniu.
                    app.search.set_focused(false);
                }
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Right
                    && button.state == ButtonState::Pressed =>
            {
                // PPM na wierszu: zaznacz proces i otwórz menu kontekstowe.
                if layout.list_body.contains(button.position) {
                    let pid = app
                        .hits
                        .iter()
                        .rev()
                        .find(|(_, rect)| rect.contains(button.position))
                        .and_then(|(action, _)| match action {
                            Action::Row(pid) => Some(*pid),
                            _ => None,
                        });
                    if let Some(pid) = pid {
                        app.open_context(
                            pid,
                            Rect::from_xywh(button.position.x, button.position.y, 1.0, 1.0),
                        );
                    }
                }
            }
            Event::PointerMotion { position, .. } => {
                app.pointer = Some(*position);
                app.controls_hover = header.control_at(layout.header, theme, *position);
                if let Some(search) = layout.search {
                    app.search
                        .set_hover_clear(app.search.clear_at(search, theme, *position));
                } else {
                    app.search.set_hover_clear(false);
                }
            }
            Event::PointerLeave => {
                app.pointer = None;
                app.controls_hover = None;
                app.search.set_hover_clear(false);
            }
            Event::Scroll(scroll) => {
                if layout.list_body.contains(scroll.position) {
                    app.scroll
                        .on_scroll(scroll.notches_y() * ROW_HEIGHT * 3.0, layout.list_body);
                }
            }
            Event::Key(key) if key.state == ButtonState::Pressed => handle_key(app, key),
            Event::Text(input) | Event::Paste(input) => {
                if app.search.is_focused() {
                    app.search.insert_str(input);
                    app.recompute();
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_menu_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let anchor = menu_anchor(size, theme, text);
    if let Some(state) = &mut app.menu {
        state.anchor = anchor;
    }
    let Some(panel) = app
        .menu
        .as_ref()
        .map(|state| menu_panel(state, size, theme, text))
    else {
        return;
    };

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                let index = app
                    .menu
                    .as_ref()
                    .and_then(|state| state.menu.index_at(panel, button.position, theme));
                if let Some(index) = index {
                    app.run_menu(index);
                } else {
                    // Klik poza menu (także w przycisk burger) zamyka panel.
                    app.menu = None;
                }
                break;
            }
            Event::PointerMotion { position, .. } => {
                if let Some(state) = &mut app.menu {
                    state.menu.hover_at(panel, Some(*position), theme);
                }
                app.pointer = Some(*position);
                app.controls_hover = None;
            }
            Event::PointerLeave => {
                if let Some(state) = &mut app.menu {
                    state.menu.hover_at(panel, None, theme);
                }
                app.pointer = None;
                app.controls_hover = None;
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if key.keysym == KEY_ESCAPE {
                    app.menu = None;
                    break;
                }
                if key.keysym == KEY_F1 {
                    app.show_about();
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_R {
                    app.menu = None;
                    app.refresh();
                    app.show_toast(tr("Process list refreshed"));
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_G {
                    app.menu = None;
                    app.toggle_charts();
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_Q {
                    app.pending_close = true;
                    break;
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_context_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let Some(panel) = app
        .context
        .as_ref()
        .map(|state| context_panel(state, size, theme, text))
    else {
        return;
    };

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                let index = app
                    .context
                    .as_ref()
                    .and_then(|state| state.menu.index_at(panel, button.position, theme));
                if let Some(index) = index {
                    app.run_context(index);
                } else {
                    app.context = None;
                }
                break;
            }
            Event::PointerButton(button)
                if button.button == PointerButton::Right
                    && button.state == ButtonState::Pressed =>
            {
                // PPM na innym procesie przestawia menu; poza wierszem zamyka je.
                let pid = app
                    .hits
                    .iter()
                    .rev()
                    .find(|(_, rect)| rect.contains(button.position))
                    .and_then(|(action, _)| match action {
                        Action::Row(pid) => Some(*pid),
                        _ => None,
                    });
                match pid {
                    Some(pid) => app.open_context(
                        pid,
                        Rect::from_xywh(button.position.x, button.position.y, 1.0, 1.0),
                    ),
                    None => app.context = None,
                }
                break;
            }
            Event::PointerMotion { position, .. } => {
                if let Some(state) = &mut app.context {
                    state.menu.hover_at(panel, Some(*position), theme);
                }
                app.pointer = Some(*position);
            }
            Event::PointerLeave => {
                if let Some(state) = &mut app.context {
                    state.menu.hover_at(panel, None, theme);
                }
                app.pointer = None;
            }
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if key.keysym == KEY_ESCAPE {
                    app.context = None;
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_R {
                    app.context = None;
                    app.refresh();
                    app.show_toast(tr("Process list refreshed"));
                    break;
                }
                if key.modifiers.ctrl && key.keysym == KEY_Q {
                    app.pending_close = true;
                    break;
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_confirm_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    let Some(confirm) = app.confirm.clone() else {
        return;
    };
    let dialog = confirm_dialog(&confirm);

    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                match dialog.action_at(size, theme, text, button.position) {
                    // 0 = anuluj, 1 = potwierdź.
                    Some(0) => app.confirm = None,
                    Some(_) => app.execute_confirm(),
                    None => {
                        let rects = dialog.rects(size, theme, text);
                        if !rects.dialog.panel.contains(button.position)
                            || rects.dialog.close.contains(button.position)
                        {
                            app.confirm = None;
                        }
                    }
                }
                break;
            }
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            Event::Key(key) if key.state == ButtonState::Pressed => {
                if key.keysym == KEY_ESCAPE {
                    app.confirm = None;
                    break;
                }
                if matches!(key.keysym, KEY_RETURN | KEY_KP_ENTER) {
                    app.execute_confirm();
                    break;
                }
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn handle_about_events(
    app: &mut App,
    events: &[Event],
    size: Size,
    theme: &Theme,
    text: &TextEngine,
) {
    for event in events {
        match event {
            Event::PointerButton(button)
                if button.button == PointerButton::Left && button.state == ButtonState::Pressed =>
            {
                let close = app.about.as_ref().is_some_and(|about| {
                    let rects = about.rects(size, theme, text);
                    about
                        .action_at(size, theme, text, button.position)
                        .is_some()
                        || rects.dialog.close.contains(button.position)
                        || !rects.dialog.panel.contains(button.position)
                });
                if close {
                    app.about = None;
                    break;
                }
            }
            Event::PointerMotion { position, .. } => app.pointer = Some(*position),
            Event::PointerLeave => app.pointer = None,
            Event::Key(key) if key.state == ButtonState::Pressed && key.keysym == KEY_ESCAPE => {
                app.about = None;
                break;
            }
            Event::Window(WindowEvent::CloseRequested) => app.pending_close = true,
            _ => {}
        }
    }
}

fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Sort(key) => app.set_sort(key),
        Action::Row(pid) => {
            app.selected = Some(pid);
            app.search.set_focused(false);
        }
        Action::ToggleOnlyMine => app.toggle_only_mine(),
        Action::EndSelected => app.ask_end(false),
        Action::ForceKillSelected => app.ask_end(true),
    }
}

fn handle_key(app: &mut App, key: &KeyEvent) {
    if key.modifiers.ctrl {
        match key.keysym {
            KEY_Q => {
                app.pending_close = true;
                return;
            }
            KEY_R => {
                app.refresh();
                app.show_toast(tr("Process list refreshed"));
                return;
            }
            KEY_F => {
                app.activate_search();
                app.search.select_all();
                return;
            }
            KEY_G => {
                app.toggle_charts();
                return;
            }
            _ => {}
        }
        if app.search.is_focused() {
            match key.keysym {
                KEY_A => app.search.select_all(),
                KEY_C => {
                    if let Some(text) = app.search.copy() {
                        app.clipboard_text = Some(text);
                    }
                }
                KEY_X => {
                    if let Some(text) = app.search.cut() {
                        app.clipboard_text = Some(text);
                        app.recompute();
                    }
                }
                KEY_V => app.pending_paste = true,
                _ => {}
            }
        }
        return;
    }

    if app.search.is_focused() {
        match key.keysym {
            KEY_ESCAPE => {
                app.close_search();
            }
            KEY_BACKSPACE => {
                app.search.backspace();
                app.recompute();
            }
            KEY_DELETE => {
                app.search.delete_forward();
                app.recompute();
            }
            KEY_LEFT => app.search.move_left_selecting(key.modifiers.shift),
            KEY_RIGHT => app.search.move_right_selecting(key.modifiers.shift),
            KEY_HOME => app.search.home_selecting(key.modifiers.shift),
            KEY_END => app.search.end_selecting(key.modifiers.shift),
            KEY_UP => {
                app.search.set_focused(false);
                app.move_selection(-1);
            }
            KEY_DOWN => {
                app.search.set_focused(false);
                app.move_selection(1);
            }
            KEY_RETURN | KEY_KP_ENTER => app.search.set_focused(false),
            KEY_F1 => app.show_about(),
            _ => {
                if let Some(text) = &key.text {
                    if !text.is_empty() && !text.chars().any(char::is_control) {
                        app.search.insert_str(text);
                        app.recompute();
                    }
                }
            }
        }
        return;
    }

    match key.keysym {
        KEY_UP => app.move_selection(-1),
        KEY_DOWN => app.move_selection(1),
        KEY_PAGE_UP => app.move_selection(-10),
        KEY_PAGE_DOWN => app.move_selection(10),
        KEY_HOME => app.select_first(),
        KEY_END => app.select_last(),
        KEY_F10 if key.modifiers.shift => {
            if app.selected.is_some() {
                let anchor = selected_row_anchor(app);
                app.open_context_for_selected(anchor);
            }
        }
        KEY_DELETE => app.ask_end(key.modifiers.shift),
        KEY_RETURN | KEY_KP_ENTER => app.ask_end(false),
        KEY_ESCAPE => {
            if app.selected.take().is_none() && (app.search_active || !app.search.is_empty()) {
                app.close_search();
            }
        }
        KEY_F1 => app.show_about(),
        KEY_F5 => {
            app.refresh();
            app.show_toast(tr("Process list refreshed"));
        }
        _ => {
            if key.modifiers.logo {
                return;
            }
            // Pisanie bez fokusu w polu wyszukiwania zaczyna nowy filtr
            // (platforma dostarcza wpisany znak w `KeyEvent::text`).
            if let Some(text) = &key.text {
                if !text.is_empty() && !text.chars().any(char::is_control) {
                    app.activate_search();
                    app.search.set_text(text);
                    app.recompute();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Action;
    use crate::prefs::Prefs;
    use crate::procs::{Process, Snapshot, SystemInfo};
    use libluna_core::{Modifiers, Point, PointerEvent};

    fn process(pid: u32, name: &str, uid: u32, cpu: f32, rss: u64) -> Process {
        Process {
            pid,
            ppid: 1,
            name: name.to_owned(),
            cmdline: format!("/usr/bin/{name} --flag"),
            state: 'S',
            uid,
            user: if uid == 1000 {
                "patryk".into()
            } else {
                "root".into()
            },
            cpu_jiffies: 0,
            start_jiffies: 900,
            rss_kib: rss,
            threads: 4,
            cpu_percent: cpu,
        }
    }

    fn demo_app() -> App {
        let snapshot = Snapshot {
            processes: vec![
                process(1, "init", 0, 0.0, 4_000),
                process(20, "firefox", 1000, 25.0, 512_000),
                process(30, "bash", 1000, 5.0, 12_000),
            ],
            system: SystemInfo {
                cpu_percent: 12.0,
                cpu_count: 8,
                memory_total_kib: 16_000_000,
                memory_available_kib: 8_000_000,
                uptime: std::time::Duration::from_secs(3600),
                load: [0.42, 0.5, 0.6],
            },
            ..Snapshot::default()
        };
        App::with_snapshot(snapshot, Prefs::default())
    }

    fn key(keysym: u32) -> KeyEvent {
        KeyEvent {
            keysym,
            keycode: 0,
            text: None,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        }
    }

    fn text_key(text: &str) -> KeyEvent {
        KeyEvent {
            text: Some(text.to_owned()),
            ..key(0)
        }
    }

    fn paint_once(app: &mut App, size: Size) {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(app, &mut scene, size, &theme, &text);
    }

    #[test]
    fn paint_never_panics_in_common_states() {
        let mut app = demo_app();
        paint_once(&mut app, Size::new(980.0, 700.0));
        paint_once(&mut app, Size::new(620.0, 460.0));
        paint_once(&mut app, Size::new(360.0, 300.0));

        app.selected = Some(20);
        paint_once(&mut app, Size::new(980.0, 700.0));

        app.search.set_text("nie ma");
        app.recompute();
        app.activate_search();
        paint_once(&mut app, Size::new(980.0, 700.0));

        app.close_search();
        paint_once(&mut app, Size::new(980.0, 700.0));
        app.open_menu(menu_anchor(
            Size::new(980.0, 700.0),
            &Theme::dark(),
            &TextEngine::system(),
        ));
        paint_once(&mut app, Size::new(980.0, 700.0));

        app.menu = None;
        app.selected = Some(20);
        app.ask_end(true);
        paint_once(&mut app, Size::new(980.0, 700.0));

        app.confirm = None;
        app.open_context(20, Rect::from_xywh(300.0, 200.0, 1.0, 1.0));
        paint_once(&mut app, Size::new(980.0, 700.0));

        app.context = None;
        app.show_about();
        paint_once(&mut app, Size::new(980.0, 700.0));
    }

    #[test]
    fn list_pushes_row_and_header_hits() {
        let mut app = demo_app();
        let size = Size::new(980.0, 700.0);
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size, &theme, &text);

        assert!(app
            .hits
            .iter()
            .any(|(action, _)| *action == Action::Row(20)));
        assert!(app
            .hits
            .iter()
            .any(|(action, _)| *action == Action::Sort(SortKey::Cpu)));
    }

    #[test]
    fn clicking_a_row_selects_it() {
        let mut app = demo_app();
        let size = Size::new(980.0, 700.0);
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size, &theme, &text);

        let (_, rect) = app
            .hits
            .iter()
            .find(|(action, _)| *action == Action::Row(20))
            .expect("row hit");
        let position = Point::new(rect.x() + 20.0, rect.center().y);
        let events = vec![Event::PointerButton(PointerEvent {
            position,
            button: PointerButton::Left,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        })];
        handle_events(&mut app, &events, size, &theme, &text);
        assert_eq!(app.selected, Some(20));
    }

    #[test]
    fn keyboard_moves_selection_and_opens_confirmation() {
        let mut app = demo_app();
        app.uid = 1000;
        handle_key(&mut app, &key(KEY_DOWN));
        assert_eq!(app.selected, Some(20));
        handle_key(&mut app, &key(KEY_DELETE));
        assert!(app.confirm.is_some());
        assert_eq!(app.confirm.as_ref().map(|c| c.pid), Some(20));
    }

    #[test]
    fn physical_keyboard_starts_and_extends_the_filter() {
        let mut app = demo_app();
        handle_key(&mut app, &text_key("f"));
        assert!(app.search_active);
        assert!(app.search.is_focused());
        handle_key(&mut app, &text_key("i"));
        handle_key(&mut app, &text_key("re"));
        assert_eq!(app.search.text(), "fire");
        assert_eq!(app.filtered.len(), 1);
        assert_eq!(app.snapshot.processes[app.filtered[0]].pid, 20);
    }

    #[test]
    fn ctrl_f_focuses_search_and_escape_clears_it() {
        let mut app = demo_app();
        let mut event = key(KEY_F);
        event.modifiers.ctrl = true;
        handle_key(&mut app, &event);
        assert!(app.search_active);
        assert!(app.search.is_focused());
        app.search.set_text("bash");
        handle_key(&mut app, &key(KEY_ESCAPE));
        assert!(!app.search_active);
        assert!(app.search.is_empty());
        assert!(!app.search.is_focused());
        assert_eq!(app.filtered.len(), 3);
    }

    #[test]
    fn right_click_on_a_row_opens_the_context_menu() {
        let mut app = demo_app();
        app.uid = 1000;
        let size = Size::new(980.0, 700.0);
        let theme = Theme::dark();
        let text = TextEngine::system();
        let mut scene = Scene::new();
        paint(&mut app, &mut scene, size, &theme, &text);

        let (_, rect) = app
            .hits
            .iter()
            .find(|(action, _)| *action == Action::Row(20))
            .expect("row hit");
        let events = vec![Event::PointerButton(PointerEvent {
            position: Point::new(rect.x() + 40.0, rect.center().y),
            button: PointerButton::Right,
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
        })];
        handle_events(&mut app, &events, size, &theme, &text);
        assert!(app.context.is_some());
        assert_eq!(app.selected, Some(20));
    }

    #[test]
    fn shift_f10_opens_the_context_menu_for_the_selection() {
        let mut app = demo_app();
        app.uid = 1000;
        app.selected = Some(20);
        app.list_viewport = Rect::from_xywh(16.0, 120.0, 600.0, 400.0);
        let mut event = key(KEY_F10);
        event.modifiers.shift = true;
        handle_key(&mut app, &event);
        assert!(app.context.is_some());
        assert_eq!(app.context.as_ref().and_then(|state| state.pid), Some(20));
    }

    #[test]
    fn colors_follow_process_state_and_memory_share() {
        let theme = Theme::dark();
        assert_eq!(state_color(&theme, 'Z'), theme.colors.error);
        assert_eq!(state_color(&theme, 'R'), theme.colors.success);
        assert_eq!(state_color(&theme, 'T'), theme.colors.warning);
        assert_eq!(memory_color(&theme, 300, 1000), theme.colors.error);
        assert_eq!(memory_color(&theme, 150, 1000), theme.colors.warning);
        assert_eq!(memory_color(&theme, 50, 1000), theme.colors.dim_text);
        assert_eq!(memory_color(&theme, 50, 0), theme.colors.dim_text);
    }

    #[test]
    fn formats_cpu_memory_and_age() {
        assert_eq!(format_cpu(0.0), "0%");
        assert_eq!(format_cpu(4.25), "4.2%");
        assert_eq!(format_cpu(25.4), "25%");
        assert_eq!(format_memory(0), "0 MiB");
        assert_eq!(format_memory(512 * 1024), "512 MiB");
        assert_eq!(format_memory(3 * 1024 * 1024), "3.0 GiB");
        assert_eq!(format_age(30.0), "30s");
        assert_eq!(format_age(125.0), "2m");
        assert_eq!(format_age(3_725.0), "1h 2m");
        assert_eq!(format_age(90_000.0), "1d 1h");
    }
}
