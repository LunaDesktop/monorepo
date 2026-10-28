//! Strona „Widgety” w demo — galeria deweloperska libLuny.
//!
//! Układ master–detail: po lewej lista wszystkich widgetów (z wyszukiwarką,
//! grupowana po kategoriach), po prawej **działający przykład** wybranego widgetu
//! i przyciski „Pokaż przykładowy kod” oraz „Kopiuj”. Przykłady rysują prawdziwe
//! widgety z `libluna-widgets`; te, które mają stan (przyciski, suwaki, pola
//! tekstowe, zakładki…), obsługują kliknięcia i klawiaturę przez
//! [`crate::gallery_interactive`].

use libluna_core::style::weight;
use libluna_core::{ButtonState, KeyEvent, Point, Rect, Theme};
use libluna_render::TextEngine;
use libluna_widgets::display::ellipsize;
use libluna_widgets::{Button, PaintCtx, ScrollView, SearchEntry};

use crate::gallery_catalog::{Category, DemoSpec, DEMOS};
use crate::gallery_interactive::{self, ExampleState};

/// Odstęp od krawędzi strony.
const PAGE_PAD: f64 = 20.0;
/// Wysokość nagłówka strony.
const HEADER_H: f64 = 64.0;
/// Maksymalna szerokość listy widgetów.
const SIDEBAR_MAX_W: f64 = 272.0;
/// Odstęp między listą a szczegółami.
const SIDEBAR_GAP: f64 = 16.0;
/// Wysokość wiersza widgetu.
const ROW_H: f64 = 32.0;
/// Wysokość nagłówka kategorii.
const GROUP_H: f64 = 28.0;
/// Wewnętrzny padding panelu szczegółów.
const DETAIL_PAD: f64 = 18.0;
/// Wysokość działającego przykładu.
const PREVIEW_H: f64 = 300.0;
/// Wysokość linii kodu.
const CODE_LINE_H: f64 = 16.0;
/// Rozmiar tekstu kodu.
const CODE_SIZE: f64 = 11.5;
/// Padding bloku kodu.
const CODE_PAD: f64 = 12.0;
/// Wysokość linii opisu.
const DESC_LINE_H: f64 = 16.0;

/// Stan galerii: wybrany widget, przewijanie, kod i stan przykładów.
#[derive(Debug, Clone)]
pub struct GalleryState {
    /// Indeks wybranego wpisu w [`DEMOS`].
    pub selected: usize,
    /// Fraza filtrująca listę (nazwa, opis, źródło, kategoria).
    pub search: SearchEntry,
    /// Przewijanie listy widgetów.
    pub sidebar: ScrollView,
    /// Przewijanie panelu szczegółów (długi kod).
    pub detail: ScrollView,
    /// Czy pokazywać przykładowy kod.
    pub show_code: bool,
    /// Podświetlony wiersz listy (indeks w [`DEMOS`]).
    pub hover_row: Option<usize>,
    /// Podświetlenie przycisku „Pokaż przykładowy kod”.
    pub hover_button: bool,
    /// Podświetlenie przycisku „Kopiuj”.
    pub hover_copy: bool,
    /// Żądanie skopiowania kodu (odbierane przez pętlę aplikacji → schowek).
    pub copy_request: Option<String>,
    /// Krótki komunikat po skopiowaniu (rysowany obok przycisków).
    pub status: String,
    pub dragging_sidebar: bool,
    pub dragging_detail: bool,
    /// Stan działających przykładów (przełączniki, suwaki, pola…).
    pub example: ExampleState,
}

impl Default for GalleryState {
    fn default() -> Self {
        Self {
            selected: 0,
            search: SearchEntry::new("Szukaj widgetu…"),
            sidebar: ScrollView::new(),
            detail: ScrollView::new(),
            show_code: false,
            hover_row: None,
            hover_button: false,
            hover_copy: false,
            copy_request: None,
            status: String::new(),
            dragging_sidebar: false,
            dragging_detail: false,
            example: ExampleState::default(),
        }
    }
}

/// Element listy po lewej: nagłówek kategorii albo widget.
#[derive(Debug, Clone, Copy)]
pub enum SidebarRow {
    Header { category: Category, rect: Rect },
    Item { demo: usize, rect: Rect },
}

/// Geometria strony — wspólna dla rysowania, trafień i przewijania.
#[derive(Debug, Clone)]
pub struct PageLayout {
    pub header: Rect,
    pub sidebar: Rect,
    pub search: Rect,
    pub list: Rect,
    pub detail: Rect,
    pub rows: Vec<SidebarRow>,
    pub sidebar_content: f64,
    pub title: Point,
    pub meta: Point,
    pub desc: Point,
    pub preview: Rect,
    pub hint: Point,
    pub code_button: Rect,
    pub copy_button: Rect,
    pub code_block: Rect,
    pub detail_content: f64,
}

/// Wpisy pasując do frazy z pola wyszukiwania.
fn filtered_items(state: &GalleryState) -> Vec<usize> {
    let query = state.search.text().trim().to_lowercase();
    DEMOS
        .iter()
        .enumerate()
        .filter(|(_, spec)| {
            query.is_empty()
                || spec.name.to_lowercase().contains(&query)
                || spec.description.to_lowercase().contains(&query)
                || spec.source.to_lowercase().contains(&query)
                || spec.category.label().to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .collect()
}

fn code_button_label(show: bool) -> &'static str {
    if show {
        "Ukryj przykładowy kod"
    } else {
        "Pokaż przykładowy kod"
    }
}

/// Liczy pełny układ: nagłówek, listę widgetów i panel szczegółów.
pub fn layout(area: Rect, theme: &Theme, text: &TextEngine, state: &GalleryState) -> PageLayout {
    let header = Rect::from_xywh(area.x(), area.y(), area.width(), HEADER_H);
    let top = area.y() + HEADER_H;
    let inner_w = (area.width() - 2.0 * PAGE_PAD).max(0.0);
    let sidebar_w = SIDEBAR_MAX_W.min((inner_w * 0.34).max(190.0)).min(inner_w);
    let body_h = (area.bottom() - top - PAGE_PAD).max(0.0);
    let sidebar = Rect::from_xywh(area.x() + PAGE_PAD, top, sidebar_w, body_h);
    let detail_x = sidebar.right() + SIDEBAR_GAP;
    let detail_w = (area.right() - PAGE_PAD - detail_x).max(0.0);
    let detail = Rect::from_xywh(detail_x, top, detail_w, body_h);

    // ── Wyszukiwarka + lista widgetów ──
    let search = Rect::from_xywh(
        sidebar.x() + 10.0,
        sidebar.y() + 10.0,
        (sidebar.width() - 20.0).max(0.0),
        34.0,
    );
    let list = Rect::from_xywh(
        sidebar.x(),
        search.bottom() + 8.0,
        sidebar.width(),
        (sidebar.bottom() - search.bottom() - 12.0).max(0.0),
    );

    let items = filtered_items(state);
    let mut total = 8.0;
    for category in Category::ALL {
        let count = items
            .iter()
            .filter(|i| DEMOS[**i].category == category)
            .count();
        if count > 0 {
            total += GROUP_H + count as f64 * ROW_H;
        }
    }
    let sidebar_content = (total + 8.0).max(list.height());
    let sidebar_offset = state
        .sidebar
        .offset()
        .clamp(0.0, (sidebar_content - list.height()).max(0.0));

    let mut rows = Vec::with_capacity(items.len() + Category::ALL.len());
    let mut y = list.y() + 4.0 - sidebar_offset;
    for category in Category::ALL {
        let matching: Vec<usize> = items
            .iter()
            .copied()
            .filter(|i| DEMOS[*i].category == category)
            .collect();
        if matching.is_empty() {
            continue;
        }
        rows.push(SidebarRow::Header {
            category,
            rect: Rect::from_xywh(
                sidebar.x() + 12.0,
                y,
                (sidebar.width() - 24.0).max(0.0),
                GROUP_H,
            ),
        });
        y += GROUP_H;
        for index in matching {
            rows.push(SidebarRow::Item {
                demo: index,
                rect: Rect::from_xywh(
                    sidebar.x() + 6.0,
                    y + 1.0,
                    (sidebar.width() - 12.0).max(0.0),
                    ROW_H - 2.0,
                ),
            });
            y += ROW_H;
        }
    }

    // ── Panel szczegółów ──
    let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
    let x = detail.x() + DETAIL_PAD;
    let w = (detail.width() - 2.0 * DETAIL_PAD).max(0.0);
    let desc_lines = wrap_text(spec.description, w, theme, text).len() as f64;
    let button_h = theme.metrics.button.height.max(34.0);
    let code_button_w = Button::new(code_button_label(state.show_code)).width(text, theme);
    let copy_button_w = Button::new("Kopiuj").width(text, theme);
    let copy_on_right = code_button_w + 8.0 + copy_button_w <= w;
    let buttons_h = if copy_on_right {
        button_h
    } else {
        2.0 * button_h + 8.0
    };
    let code_lines = spec.code.lines().count().max(1) as f64;
    let code_h = if state.show_code {
        code_lines * CODE_LINE_H + 2.0 * CODE_PAD
    } else {
        0.0
    };
    // Wysokość treści nie zależy od przewinięcia — dzięki temu offset da się
    // przyciąć jeszcze przed policzeniem prostokątów.
    let detail_content = (DETAIL_PAD
        + 38.0
        + 32.0
        + desc_lines * DESC_LINE_H
        + 10.0
        + PREVIEW_H
        + 18.0
        + 26.0
        + buttons_h
        + 12.0
        + if code_h > 0.0 { code_h + 4.0 } else { 0.0 }
        + DETAIL_PAD)
        .max(detail.height());
    let detail_offset = state
        .detail
        .offset()
        .clamp(0.0, (detail_content - detail.height()).max(0.0));
    let content_top = detail.y() + DETAIL_PAD - detail_offset;
    let mut cursor = content_top;

    let title = Point::new(x, cursor + 10.0);
    cursor += 38.0;
    let meta = Point::new(x, cursor + 8.0);
    cursor += 32.0;
    let desc = Point::new(x, cursor + 8.0);
    cursor += desc_lines * DESC_LINE_H + 10.0;
    let preview = Rect::from_xywh(x, cursor, w, PREVIEW_H);
    cursor += PREVIEW_H + 18.0;
    let hint = Point::new(detail.center().x, cursor);
    cursor += 26.0;
    let code_button = Rect::from_xywh(x, cursor, code_button_w, button_h);
    let copy_button = if copy_on_right {
        Rect::from_xywh(code_button.right() + 8.0, cursor, copy_button_w, button_h)
    } else {
        Rect::from_xywh(x, cursor + button_h + 8.0, copy_button_w, button_h)
    };
    cursor += buttons_h + 12.0;
    let code_block = Rect::from_xywh(x, cursor, w, code_h);

    PageLayout {
        header,
        sidebar,
        search,
        list,
        detail,
        rows,
        sidebar_content,
        title,
        meta,
        desc,
        preview,
        hint,
        code_button,
        copy_button,
        code_block,
        detail_content,
    }
}

/// Rysuje całą stronę galerii w obszarze treści.
pub fn paint(ctx: &mut PaintCtx, area: Rect, state: &GalleryState) {
    let theme = *ctx.theme;
    ctx.scene.fill_rect(area, theme.colors.view_bg);

    let lay = layout(area, &theme, ctx.text, state);
    paint_header(ctx, area, &lay);
    paint_sidebar(ctx, &lay, state);
    paint_detail(ctx, &lay, state);

    // Paski przewijania.
    let mut sidebar = state.sidebar;
    sidebar.set_content_height(lay.sidebar_content);
    sidebar.set_offset(sidebar.offset(), lay.list);
    sidebar.paint_scrollbar(ctx, lay.list);

    let mut detail = state.detail;
    detail.set_content_height(lay.detail_content);
    detail.set_offset(detail.offset(), lay.detail);
    detail.paint_scrollbar(ctx, lay.detail);
}

fn paint_header(ctx: &mut PaintCtx, area: Rect, lay: &PageLayout) {
    let theme = *ctx.theme;
    let x = area.x() + PAGE_PAD;
    let right = lay.detail.right();

    ctx.label_left(
        x,
        area.y() + 24.0,
        "Widgety libLuny",
        theme.typography.title.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        x,
        area.y() + 46.0,
        "Lista widgetów po lewej · działający przykład i kod po prawej.",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
    ctx.label_right(
        right,
        area.y() + 24.0,
        &format!(
            "{} widgetów · {} kategorii",
            DEMOS.len(),
            Category::ALL.len()
        ),
        theme.typography.body.size,
        weight::MEDIUM,
        theme.colors.accent,
    );
    ctx.label_right(
        right,
        area.y() + 44.0,
        "↑ / ↓ wybór · PageUp / PageDown przewijanie",
        theme.typography.caption.size,
        weight::NORMAL,
        theme.colors.dim_text,
    );
}

fn paint_sidebar(ctx: &mut PaintCtx, lay: &PageLayout, state: &GalleryState) {
    let theme = *ctx.theme;
    ctx.scene
        .fill_rounded_rect(lay.sidebar, theme.radius.medium, theme.colors.window_bg);
    ctx.scene
        .stroke_rounded_rect(lay.sidebar, theme.radius.medium, 1.0, theme.colors.border);

    // Pole wyszukiwania nad listą.
    ctx.scene
        .fill_rounded_rect(lay.search, theme.radius.small, theme.colors.card_bg);
    state.search.paint(ctx, lay.search);

    ctx.scene.push_clip(lay.list);
    if !lay
        .rows
        .iter()
        .any(|row| matches!(row, SidebarRow::Item { .. }))
    {
        ctx.label_center(
            lay.list.center().x,
            lay.list.y() + 36.0,
            "Brak wyników",
            theme.typography.body.size,
            weight::MEDIUM,
            theme.colors.dim_text,
        );
        ctx.label_center(
            lay.list.center().x,
            lay.list.y() + 58.0,
            "Zmień frazę lub wyczyść pole",
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text.scale_alpha(0.8),
        );
    }
    for row in &lay.rows {
        match row {
            SidebarRow::Header { category, rect } => {
                ctx.label_left(
                    rect.x(),
                    rect.center().y,
                    &category.label().to_uppercase(),
                    theme.typography.caption.size,
                    weight::BOLD,
                    theme.colors.dim_text.scale_alpha(0.9),
                );
            }
            SidebarRow::Item { demo, rect } => {
                let spec = &DEMOS[*demo];
                let selected = *demo == state.selected;
                let hovered = state.hover_row == Some(*demo);
                if selected {
                    ctx.scene.fill_rounded_rect(
                        *rect,
                        theme.radius.small,
                        theme.colors.accent.scale_alpha(0.18),
                    );
                    let bar = Rect::from_xywh(
                        rect.x() + 2.0,
                        rect.y() + 7.0,
                        3.0,
                        (rect.height() - 14.0).max(0.0),
                    );
                    ctx.scene
                        .fill_rounded_rect(bar, bar.width() / 2.0, theme.colors.accent);
                } else if hovered {
                    ctx.scene.fill_rounded_rect(
                        *rect,
                        theme.radius.small,
                        theme.colors.shade.scale_alpha(1.8),
                    );
                }
                let name = ellipsize(
                    spec.name,
                    (rect.width() - 24.0).max(20.0),
                    theme.typography.body.size,
                    ctx.text,
                );
                ctx.label_left(
                    rect.x() + 14.0,
                    rect.center().y,
                    &name,
                    theme.typography.body.size,
                    if selected {
                        weight::MEDIUM
                    } else {
                        weight::NORMAL
                    },
                    if selected {
                        theme.colors.text
                    } else {
                        theme.colors.dim_text
                    },
                );
            }
        }
    }
    ctx.scene.pop_clip();
}

fn paint_detail(ctx: &mut PaintCtx, lay: &PageLayout, state: &GalleryState) {
    let theme = *ctx.theme;
    let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
    let width = lay.preview.width();

    ctx.scene.push_clip(lay.detail);

    // Tytuł i metadane.
    let title = ellipsize(spec.name, width, theme.typography.title.size, ctx.text);
    ctx.label_left(
        lay.title.x,
        lay.title.y,
        &title,
        theme.typography.title.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_left(
        lay.meta.x,
        lay.meta.y,
        &format!(
            "{} · libluna-widgets/src/{}",
            spec.category.label(),
            spec.source
        ),
        theme.typography.caption.size,
        weight::MEDIUM,
        theme.colors.accent,
    );

    for (i, line) in wrap_text(spec.description, width, &theme, ctx.text)
        .into_iter()
        .enumerate()
    {
        ctx.label_left(
            lay.desc.x,
            lay.desc.y + i as f64 * DESC_LINE_H,
            &line,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }

    // Działający przykład (prawdziwy widget).
    ctx.scene
        .fill_rounded_rect(lay.preview, theme.radius.medium, theme.colors.window_bg);
    ctx.scene
        .stroke_rounded_rect(lay.preview, theme.radius.medium, 1.0, theme.colors.border);
    ctx.scene.push_clip(lay.preview);
    if !gallery_interactive::paint(spec.name, ctx, lay.preview, &state.example) {
        (spec.preview)(ctx, lay.preview);
    }
    ctx.scene.pop_clip();

    // Podpowiedź / wynik ostatniej akcji.
    let status = state.example.status();
    let hint = if gallery_interactive::supports(spec.name) {
        if status.is_empty() {
            "Interaktywne — kliknij elementy w podglądzie".to_owned()
        } else {
            status
        }
    } else {
        "Podgląd na żywo — ten widget nie ma stanu interaktywnego".to_owned()
    };
    ctx.label_center(
        lay.hint.x,
        lay.hint.y,
        &hint,
        theme.typography.caption.size,
        weight::NORMAL,
        if gallery_interactive::supports(spec.name) {
            theme.colors.accent
        } else {
            theme.colors.dim_text
        },
    );

    // Przyciski: pokaż/ukryj kod i kopiowanie.
    let mut button = Button::new(code_button_label(state.show_code));
    button.set_hovered(state.hover_button);
    button.paint(ctx, lay.code_button);

    let mut copy = Button::new("Kopiuj");
    copy.set_hovered(state.hover_copy);
    copy.paint(ctx, lay.copy_button);

    // Potwierdzenie skopiowania (obok przycisków).
    if !state.status.is_empty() && lay.code_button.height() > 0.0 {
        let status_x = lay.copy_button.right() + 12.0;
        let status_y = lay.copy_button.center().y;
        if status_x < lay.detail.right() - 20.0 {
            ctx.label_left(
                status_x,
                status_y,
                &state.status,
                theme.typography.caption.size,
                weight::MEDIUM,
                theme.colors.success,
            );
        }
    }

    // Blok kodu.
    if state.show_code && lay.code_block.height() > 0.0 {
        ctx.scene.fill_rounded_rect(
            lay.code_block,
            theme.radius.small,
            theme.colors.shade.scale_alpha(0.7),
        );
        ctx.scene
            .stroke_rounded_rect(lay.code_block, theme.radius.small, 1.0, theme.colors.border);
        ctx.scene.push_clip(lay.code_block);
        for (i, line) in spec.code.lines().enumerate() {
            let cy = lay.code_block.y() + CODE_PAD + CODE_LINE_H / 2.0 + i as f64 * CODE_LINE_H;
            ctx.label_right(
                lay.code_block.x() + 22.0,
                cy,
                &(i + 1).to_string(),
                CODE_SIZE - 1.0,
                weight::NORMAL,
                theme.colors.dim_text.scale_alpha(0.55),
            );
            paint_code_line(ctx, lay.code_block.x() + 34.0, cy, line, CODE_SIZE);
        }
        ctx.scene.pop_clip();
    }

    ctx.scene.pop_clip();
}

/// Zawija opis do maksymalnie dwóch linii (z wielokropkiem).
fn wrap_text(text: &str, max_w: f64, theme: &Theme, engine: &TextEngine) -> Vec<String> {
    let size = theme.typography.caption.size;
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let probe = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if !current.is_empty() && engine.measure(&probe, size, weight::NORMAL).width > max_w {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        } else {
            current = probe;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    if lines.len() > 2 {
        let last = lines[1].clone();
        lines.truncate(2);
        lines[1] = ellipsize(&format!("{last} …"), max_w, size, engine);
    }
    lines
}

/// Prosty „highlighting”: słowa kluczowe akcentem, komentarze wyciszone.
fn paint_code_line(ctx: &mut PaintCtx, x: f64, cy: f64, line: &str, size: f64) {
    let theme = *ctx.theme;
    if line.trim_start().starts_with("//") {
        ctx.label_left(x, cy, line, size, weight::NORMAL, theme.colors.dim_text);
        return;
    }
    let mut cx = x;
    for token in line.split_inclusive(' ') {
        let trimmed = token.trim();
        let color = if is_keyword(trimmed) {
            theme.colors.accent
        } else if trimmed.starts_with('"') || trimmed.starts_with("r#") {
            theme.colors.success
        } else {
            theme.colors.text
        };
        let w = ctx.measure(token, size, weight::NORMAL);
        ctx.label_left(cx, cy, token, size, weight::NORMAL, color);
        cx += w;
    }
}

fn is_keyword(token: &str) -> bool {
    matches!(
        token,
        "let"
            | "mut"
            | "if"
            | "else"
            | "for"
            | "in"
            | "use"
            | "fn"
            | "match"
            | "impl"
            | "pub"
            | "true"
            | "false"
            | "self"
            | "Some"
            | "None"
            | "new"
    )
}

// --- Obsługa zdarzeń ---

/// Klik w stronę galerii. Zwraca `true`, jeśli zdarzenie zostało zużyte.
pub fn pointer_pressed(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    p: Point,
) -> bool {
    let lay = layout(area, theme, text, state);

    // Pole wyszukiwania ma pierwszeństwo przed listą.
    if lay.search.contains(p) {
        if state.search.clear_at(lay.search, theme, p) {
            state.search.clear();
            search_changed(state);
        }
        state.search.set_focused(true);
        return true;
    }

    // Pasek przewijania listy ma pierwszeństwo przed wierszami.
    state.sidebar.set_content_height(lay.sidebar_content);
    if state.sidebar.begin_drag(p, lay.list, theme) {
        state.dragging_sidebar = true;
        return true;
    }

    for row in &lay.rows {
        if let SidebarRow::Item { demo, rect } = row {
            if rect.contains(p) {
                state.search.set_focused(false);
                select(state, *demo);
                ensure_row_visible(state, &lay, *demo);
                return true;
            }
        }
    }
    if lay.sidebar.contains(p) {
        return true;
    }

    if lay.code_button.contains(p) {
        state.search.set_focused(false);
        state.show_code = !state.show_code;
        // Po zmianie wysokości treści przeliczamy układ, żeby offset był poprawny.
        let lay = layout(area, theme, text, state);
        state.detail.set_content_height(lay.detail_content);
        state.detail.set_offset(state.detail.offset(), lay.detail);
        return true;
    }
    if lay.copy_button.contains(p) {
        state.search.set_focused(false);
        let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
        state.copy_request = Some(spec.code.to_owned());
        state.status = "Skopiowano do schowka".to_owned();
        return true;
    }

    if lay.preview.contains(p) {
        state.search.set_focused(false);
        let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
        gallery_interactive::pressed(spec.name, lay.preview, p, theme, text, &mut state.example);
        return true;
    }

    state.detail.set_content_height(lay.detail_content);
    if state.detail.begin_drag(p, lay.detail, theme) {
        state.dragging_detail = true;
        return true;
    }
    lay.detail.contains(p)
}

/// Ruch wskaźnika: podświetlenia, przeciąganie i interakcje w podglądzie.
pub fn pointer_motion(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    p: Point,
) {
    let lay = layout(area, theme, text, state);
    state.hover_row = lay.rows.iter().find_map(|row| match row {
        SidebarRow::Item { demo, rect } if rect.contains(p) => Some(*demo),
        _ => None,
    });
    state.hover_button = lay.code_button.contains(p);
    state.hover_copy = lay.copy_button.contains(p);
    state
        .search
        .set_hover_clear(state.search.clear_at(lay.search, theme, p));

    if state.dragging_sidebar {
        state.sidebar.set_content_height(lay.sidebar_content);
        state.sidebar.drag_to(p, lay.list, theme);
    } else if state.dragging_detail {
        state.detail.set_content_height(lay.detail_content);
        state.detail.drag_to(p, lay.detail, theme);
    } else if lay.preview.contains(p) {
        let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
        gallery_interactive::motion(spec.name, lay.preview, p, theme, text, &mut state.example);
    }
}

/// Zwolnienie przycisku myszy kończy przeciąganie.
pub fn pointer_released(state: &mut GalleryState) {
    state.dragging_sidebar = false;
    state.dragging_detail = false;
    state.sidebar.end_drag();
    state.detail.end_drag();
    gallery_interactive::released(&mut state.example);
}

/// Kółko myszy nad listą albo panelem szczegółów.
pub fn scroll(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    position: Point,
    dy: f64,
) -> bool {
    let lay = layout(area, theme, text, state);
    if lay.list.contains(position) {
        state.sidebar.set_content_height(lay.sidebar_content);
        state.sidebar.on_scroll(dy, lay.list)
    } else if lay.detail.contains(position) {
        state.detail.set_content_height(lay.detail_content);
        state.detail.on_scroll(dy, lay.detail)
    } else {
        false
    }
}

/// Przewijanie panelu szczegółów klawiaturą (PageUp/PageDown).
pub fn scroll_by_page(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    direction: f64,
) {
    let lay = layout(area, theme, text, state);
    state.detail.set_content_height(lay.detail_content);
    state
        .detail
        .on_scroll(lay.detail.height() * 0.85 * direction, lay.detail);
}

/// Skok na pierwszy lub ostatni widget.
pub fn scroll_to(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    end: bool,
) {
    let demo = if end {
        DEMOS.len().saturating_sub(1)
    } else {
        0
    };
    select(state, demo);
    let lay = layout(area, theme, text, state);
    ensure_row_visible(state, &lay, demo);
}

/// Klawiatura na stronie galerii. Zwraca `true`, gdy zdarzenie zostało zużyte.
pub fn key(
    state: &mut GalleryState,
    area: Rect,
    theme: &Theme,
    text: &TextEngine,
    ke: &KeyEvent,
) -> bool {
    // Pole wyszukiwania ma pierwszeństwo — inaczej litery uruchamiałyby skróty demo.
    if state.search.is_focused() {
        if ke.state != ButtonState::Pressed {
            return false;
        }
        if ke.keysym == 0xff1b {
            if state.search.is_empty() {
                state.search.set_focused(false);
            } else {
                state.search.clear();
                search_changed(state);
            }
            return true;
        }
        if gallery_interactive::text_key(&mut state.search, ke) {
            search_changed(state);
            return true;
        }
        // Strzałki góra/dół nadal wybierają wynik z listy (obsługa niżej).
    }

    // Pole tekstowe w działającym przykładzie.
    if gallery_interactive::key(&mut state.example, ke) {
        return true;
    }
    if ke.state != ButtonState::Pressed {
        return false;
    }

    let items = filtered_items(state);
    if items.is_empty() {
        // PageUp/PageDown mogą nadal przewijać szczegóły.
        return match ke.keysym {
            0xff55 => {
                scroll_by_page(state, area, theme, text, -1.0);
                true
            }
            0xff56 => {
                scroll_by_page(state, area, theme, text, 1.0);
                true
            }
            _ => false,
        };
    }

    let position = items
        .iter()
        .position(|demo| *demo == state.selected)
        .unwrap_or(0);
    let next_index = match ke.keysym {
        0xff52 => position.saturating_sub(1),          // Up
        0xff54 => (position + 1).min(items.len() - 1), // Down
        0xff50 => 0,                                   // Home
        0xff57 => items.len() - 1,                     // End
        0xff55 => {
            scroll_by_page(state, area, theme, text, -1.0);
            return true;
        }
        0xff56 => {
            scroll_by_page(state, area, theme, text, 1.0);
            return true;
        }
        _ => return false,
    };
    let next = items[next_index];
    select(state, next);
    let lay = layout(area, theme, text, state);
    ensure_row_visible(state, &lay, next);
    true
}

/// Stan po wyjściu wskaźnika z obszaru galerii.
pub fn pointer_left(state: &mut GalleryState) {
    state.hover_row = None;
    state.hover_button = false;
    state.hover_copy = false;
    state.search.set_hover_clear(false);
    state.dragging_sidebar = false;
    state.dragging_detail = false;
    state.sidebar.end_drag();
    state.detail.end_drag();
}

fn select(state: &mut GalleryState, demo: usize) {
    if state.selected == demo {
        return;
    }
    state.selected = demo;
    state.example.reset();
    state.show_code = false;
    state.status.clear();
    state.detail = ScrollView::new();
}

/// Po zmianie frazy: przewijamy listę na górę i pilnujemy, by wybór pasował.
pub(crate) fn search_changed(state: &mut GalleryState) {
    state.sidebar = ScrollView::new();
    state.status.clear();
    let items = filtered_items(state);
    if !items.is_empty() && !items.contains(&state.selected) {
        state.selected = items[0];
        state.example.reset();
        state.detail = ScrollView::new();
    }
}

/// Przewija listę tak, aby wybrany wiersz był widoczny.
fn ensure_row_visible(state: &mut GalleryState, lay: &PageLayout, demo: usize) {
    let Some(rect) = lay.rows.iter().find_map(|row| match row {
        SidebarRow::Item { demo: d, rect } if *d == demo => Some(*rect),
        _ => None,
    }) else {
        return;
    };
    state.sidebar.set_content_height(lay.sidebar_content);
    let offset = state.sidebar.offset();
    if rect.y() < lay.list.y() {
        let delta = lay.list.y() - rect.y() + 4.0;
        state.sidebar.set_offset(offset - delta, lay.list);
    } else if rect.bottom() > lay.list.bottom() {
        let delta = rect.bottom() - lay.list.bottom() + 4.0;
        state.sidebar.set_offset(offset + delta, lay.list);
    }
}

/// Pomocniczo dla testów/podglądów: wpis katalogu dla aktualnego wyboru.
pub fn selected_spec(state: &GalleryState) -> &'static DemoSpec {
    &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))]
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_core::Size;
    use libluna_render::{Scene, SoftwareRenderer};

    fn size() -> Size {
        Size::new(1000.0, 900.0)
    }

    fn area() -> Rect {
        Rect::from_size(size())
    }

    fn demo_index(name: &str) -> usize {
        DEMOS.iter().position(|d| d.name == name).expect(name)
    }

    /// Podgląd galerii do PNG — pozwala obejrzeć stronę bez kompozytora.
    #[test]
    #[ignore]
    fn render_gallery_preview() {
        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(1200.0, 1000.0);

        let render = |state: &GalleryState, path: &str| {
            let mut scene = Scene::new();
            {
                let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
                paint(&mut ctx, Rect::from_size(size), state);
            }
            let mut renderer = SoftwareRenderer::new(size.width as u32, size.height as u32);
            renderer.draw(&scene, 1.0, theme.colors.window_bg);
            let png = renderer.encode_png().expect("PNG");
            std::fs::write(path, png).expect("zapis PNG");
        };

        let mut state = GalleryState::default();
        render(&state, "/tmp/luna-gallery-page.png");

        state.selected = demo_index("Switch");
        state.show_code = true;
        render(&state, "/tmp/luna-gallery-switch.png");

        state.selected = demo_index("Button");
        state.show_code = true;
        let lay = layout(Rect::from_size(size), &theme, &text, &state);
        state.detail.set_content_height(lay.detail_content);
        state.detail.set_offset(260.0, lay.detail);
        state.status = "Skopiowano do schowka".to_owned();
        render(&state, "/tmp/luna-gallery-code.png");

        state.selected = demo_index("Calendar");
        state.detail = ScrollView::new();
        render(&state, "/tmp/luna-gallery-calendar.png");

        state.selected = demo_index("MediaControls");
        state.show_code = false;
        render(&state, "/tmp/luna-gallery-media.png");

        // Filtr listy (np. „color” → ColorButton / ColorChooser).
        let mut filtered = GalleryState::default();
        filtered.search.set_text("color");
        filtered.search.set_focused(true);
        search_changed(&mut filtered);
        render(&filtered, "/tmp/luna-gallery-search.png");
    }

    /// Podgląd całego okna (sidebar + header + galeria) do PNG.
    #[test]
    #[ignore]
    fn render_demo_widgets_window() {
        use crate::ui::{build_scene, Demo, WIDGETS_NAV};
        use libluna_core::{Accent, ThemePreference};

        let theme = Theme::dark();
        let text = TextEngine::system();
        let size = Size::new(1200.0, 1028.0);
        let mut demo = Demo {
            nav_selected: Some(WIDGETS_NAV),
            ..Demo::default()
        };
        demo.gallery.selected = demo_index("Slider");
        demo.gallery.show_code = true;
        let mut scene = Scene::new();
        build_scene(
            &mut scene,
            &theme,
            &text,
            size,
            &demo,
            ThemePreference::Dark,
            Accent::default(),
        );
        let mut renderer = SoftwareRenderer::new(1200, 1028);
        renderer.draw(&scene, 1.0, theme.colors.window_bg);
        let png = renderer.encode_png().expect("PNG");
        std::fs::write("/tmp/luna-demo-widgets.png", png).expect("zapis PNG");
    }

    #[test]
    fn catalog_contains_every_category() {
        for category in Category::ALL {
            assert!(
                DEMOS.iter().any(|d| d.category == category),
                "brak widgetu w kategorii {:?}",
                category
            );
        }
    }

    #[test]
    fn catalog_is_unique_and_has_code_for_every_widget() {
        assert!(DEMOS.len() >= 70, "galeria powinna mieć wszystkie widgety");
        let mut names: Vec<&str> = DEMOS.iter().map(|d| d.name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "nazwy kafelków muszą być unikalne");
        for demo in DEMOS {
            assert!(
                !demo.code.trim().is_empty(),
                "{}: brak przykładu",
                demo.name
            );
            assert!(!demo.source.is_empty(), "{}: brak źródła", demo.name);
            assert!(!demo.description.is_empty(), "{}: brak opisu", demo.name);
        }
    }

    #[test]
    fn sidebar_lists_every_widget() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let lay = layout(area(), &theme, &text, &GalleryState::default());
        let items = lay
            .rows
            .iter()
            .filter(|row| matches!(row, SidebarRow::Item { .. }))
            .count();
        assert_eq!(items, DEMOS.len());
        let headers = lay
            .rows
            .iter()
            .filter(|row| matches!(row, SidebarRow::Header { .. }))
            .count();
        assert_eq!(headers, Category::ALL.len());
    }

    #[test]
    fn clicking_sidebar_selects_widget() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState::default();
        let target = demo_index("Switch");
        let lay = layout(area(), &theme, &text, &state);
        let rect = lay
            .rows
            .iter()
            .find_map(|row| match row {
                SidebarRow::Item { demo, rect } if *demo == target => Some(*rect),
                _ => None,
            })
            .expect("wiersz");
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            rect.center()
        ));
        assert_eq!(state.selected, target);
    }

    #[test]
    fn search_filters_list_and_selects_first_match() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState {
            search: SearchEntry::new("Szukaj…"),
            ..GalleryState::default()
        };
        state.search.set_text("calendar");
        state.search.set_focused(true);
        search_changed(&mut state);

        let lay = layout(area(), &theme, &text, &state);
        let items: Vec<usize> = lay
            .rows
            .iter()
            .filter_map(|row| match row {
                SidebarRow::Item { demo, .. } => Some(*demo),
                _ => None,
            })
            .collect();
        assert!(items.len() < DEMOS.len());
        assert!(items.iter().any(|i| DEMOS[*i].name == "Calendar"));
        assert_eq!(DEMOS[state.selected].name, "Calendar");

        // Brak trafień → pusta lista, ale strona nadal się rysuje.
        state.search.set_text("zzz-nie-ma");
        search_changed(&mut state);
        let lay = layout(area(), &theme, &text, &state);
        assert!(!lay
            .rows
            .iter()
            .any(|row| matches!(row, SidebarRow::Item { .. })));
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&mut ctx, area(), &state);
        assert!(!scene.is_empty());
    }

    #[test]
    fn search_field_focuses_on_click() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.search.center()
        ));
        assert!(state.search.is_focused());
    }

    #[test]
    fn copy_button_requests_widget_code() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.copy_button.center()
        ));
        assert_eq!(state.copy_request.take().as_deref(), Some(DEMOS[0].code));
        assert!(!state.status.is_empty());
    }

    #[test]
    fn code_button_toggles_code_block() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert_eq!(lay.code_block.height(), 0.0);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.code_button.center()
        ));
        assert!(state.show_code);
        let lay = layout(area(), &theme, &text, &state);
        assert!(lay.code_block.height() > 0.0);
    }

    #[test]
    fn interactive_switch_reacts_to_click() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState {
            selected: demo_index("Switch"),
            ..GalleryState::default()
        };
        let lay = layout(area(), &theme, &text, &state);
        let before = state.example.switch_on;
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.preview.center()
        ));
        assert_ne!(state.example.switch_on, before);

        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        paint(&mut ctx, area(), &state);
        assert!(!scene.is_empty());
    }

    #[test]
    fn wheel_scrolls_sidebar_and_detail_separately() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState {
            show_code: true,
            ..GalleryState::default()
        };
        // Małe okno, żeby obie kolumny miały co przewijać.
        let small = Rect::from_xywh(0.0, 0.0, 700.0, 600.0);
        let lay = layout(small, &theme, &text, &state);

        assert!(scroll(
            &mut state,
            small,
            &theme,
            &text,
            Point::new(lay.list.center().x, lay.list.y() + 40.0),
            600.0
        ));
        assert!(state.sidebar.offset() > 0.0);
        assert_eq!(state.detail.offset(), 0.0);

        assert!(scroll(
            &mut state,
            small,
            &theme,
            &text,
            lay.detail.center(),
            600.0
        ));
        assert!(state.detail.offset() > 0.0);
    }

    #[test]
    fn interactive_examples_accept_input() {
        use libluna_core::Modifiers;

        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState {
            selected: demo_index("Entry"),
            ..GalleryState::default()
        };
        let lay = layout(area(), &theme, &text, &state);
        let field = Point::new(lay.preview.center().x, lay.preview.center().y - 14.0);
        assert!(pointer_pressed(&mut state, area(), &theme, &text, field));
        let ke = KeyEvent {
            keysym: 0x61,
            keycode: 0,
            text: Some("abc".to_owned()),
            state: ButtonState::Pressed,
            modifiers: Modifiers::NONE,
            repeat: false,
        };
        assert!(key(&mut state, area(), &theme, &text, &ke));
        assert_eq!(state.example.entry.text(), "abc");

        // Slider: klik blisko prawej krawędzi toru ustawia wartość ≈ 100%.
        state.selected = demo_index("Slider");
        let lay = layout(area(), &theme, &text, &state);
        let near_end = Point::new(lay.preview.right() - 45.0, lay.preview.center().y);
        assert!(pointer_pressed(&mut state, area(), &theme, &text, near_end));
        assert!(state.example.slider > 0.9, "{}", state.example.slider);
    }

    #[test]
    fn every_demo_paints_in_detail() {
        let theme = Theme::dark();
        let text = TextEngine::none();
        let mut state = GalleryState::default();
        for (demo, spec) in DEMOS.iter().enumerate() {
            state.selected = demo;
            state.show_code = true;
            let mut scene = Scene::new();
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            paint(&mut ctx, area(), &state);
            assert!(!scene.is_empty(), "{} nie narysował się", spec.name);
        }
    }
}
