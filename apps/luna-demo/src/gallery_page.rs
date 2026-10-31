//! Strona „Widgety” w demo — dokumentacja biblioteki z żywymi przykładami.
//!
//! Każdy wpis ma trzy części, widoczne jednocześnie:
//!
//! 1. **Działający przykład** — prawdziwy widget z `libluna-widgets`, który można
//!    kliknąć, przeciągnąć lub obsłużyć klawiaturą (patrz
//!    [`crate::gallery_interactive`]); pozostałe wpisy są rysowane na żywo, ale bez
//!    stanu interaktywnego.
//! 2. **Kod** — kompletny fragment użycia w modelu immediate (`paint` + `*_at`).
//!    Kod jest zawsze widoczny i można go skopiować jednym kliknięciem.
//! 3. **Jak użyć** — krótka nota o podziale odpowiedzialności między widgetem
//!    a aplikacją (stan, animacja, nawigacja).
//!
//! Po lewej stronie znajduje się lista wszystkich widgetów z wyszukiwarką i
//! podziałem na kategorie; strona obsługuje też przechodzenie ←/→ między wpisami.

use libluna_core::style::weight;
use libluna_core::{ButtonState, KeyEvent, Point, Rect, Theme, ThemePreference};
use libluna_render::TextEngine;
use libluna_widgets::display::ellipsize;
use libluna_widgets::{
    Button, ControlsSide, HeaderBar, PaintCtx, ScrollView, SearchEntry, ViewSwitcher, WindowControl,
};

use crate::gallery_catalog::{notes, Category, DemoSpec, DEMOS};
use crate::gallery_interactive::{self, ExampleState};

/// Odstęp od krawędzi strony.
const PAGE_PAD: f64 = 22.0;
/// Maksymalna szerokość listy widgetów.
const SIDEBAR_MAX_W: f64 = 286.0;
/// Odstęp między listą a treścią dokumentacji.
const SIDEBAR_GAP: f64 = 16.0;
/// Wysokość wiersza widgetu.
const ROW_H: f64 = 32.0;
/// Wysokość nagłówka kategorii.
const GROUP_H: f64 = 28.0;
/// Wewnętrzny padding panelu dokumentacji.
const DETAIL_PAD: f64 = 20.0;
/// Wysokość działającego przykładu.
const PREVIEW_H: f64 = 300.0;
/// Wysokość linii kodu.
const CODE_LINE_H: f64 = 17.0;
/// Rozmiar tekstu kodu.
const CODE_SIZE: f64 = 11.5;
/// Padding bloku kodu.
const CODE_PAD: f64 = 14.0;
/// Wysokość linii opisu i noty.
const TEXT_LINE_H: f64 = 18.0;
/// Wysokość przycisku „Kopiuj kod”.
const COPY_H: f64 = 30.0;
/// Wysokość kart poprzedni/następny.
const NAV_H: f64 = 48.0;
/// Odstęp między kartami poprzedni/następny.
const NAV_GAP: f64 = 12.0;

/// Stan galerii: wybrany widget, przewijanie, wyszukiwanie i stan przykładów.
#[derive(Debug, Clone)]
pub struct GalleryState {
    /// Indeks wybranego wpisu w [`DEMOS`].
    pub selected: usize,
    /// Fraza filtrująca listę (nazwa, opis, źródło, kategoria).
    pub search: SearchEntry,
    /// Przewijanie listy widgetów.
    pub sidebar: ScrollView,
    /// Przewijanie panelu dokumentacji (długi kod i nota).
    pub detail: ScrollView,
    /// Podświetlony wiersz listy (indeks w [`DEMOS`]).
    pub hover_row: Option<usize>,
    /// Podświetlenie przycisku „Kopiuj kod”.
    pub hover_copy: bool,
    /// Podświetlenie karty „Poprzedni”.
    pub hover_prev: bool,
    /// Podświetlenie karty „Następny”.
    pub hover_next: bool,
    /// Podświetlona kontrolka okna w pasku tytułu.
    pub header_hover: Option<WindowControl>,
    /// Preferencja motywu pokazywana w stopce listy.
    pub theme_pref: ThemePreference,
    /// Żądanie zmiany motywu (odbierane przez pętlę aplikacji).
    pub theme_change: Option<ThemePreference>,
    /// Żądanie skopiowania kodu (odbierane przez pętlę aplikacji → schowek).
    pub copy_request: Option<String>,
    /// Krótki komunikat po skopiowaniu (rysowany obok przycisku).
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
            hover_row: None,
            hover_copy: false,
            hover_prev: false,
            hover_next: false,
            header_hover: None,
            theme_pref: ThemePreference::System,
            theme_change: None,
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
    pub detail_content: f64,
    pub crumb: Point,
    pub title: Point,
    pub meta: Point,
    pub desc: Point,
    pub preview_heading: Point,
    pub preview_badge: Rect,
    pub preview: Rect,
    pub hint: Point,
    pub code_heading: Point,
    pub copy_button: Rect,
    pub code_meta: Point,
    pub code_block: Rect,
    pub notes_heading: Point,
    pub notes: Point,
    pub prev_button: Rect,
    pub next_button: Rect,
    pub prev: Option<usize>,
    pub next: Option<usize>,
    /// Przełącznik motywu w stopce listy.
    pub theme_switch: Rect,
    /// Podpowiedź klawiaturowa pod listą widgetów.
    pub sidebar_hint: Point,
}

/// Wpisy pasujące do frazy z pola wyszukiwania.
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

/// Zawija tekst do podanej szerokości (bez limitu linii).
fn wrap_lines(text: &str, max_w: f64, size: f64, engine: &TextEngine) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
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
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Pasek tytułu używany przez dokumentację i okno demo.
///
/// Tytuł jest rysowany jako treść paska, żeby obok niego zmieściły się liczniki;
pub fn header_bar() -> HeaderBar {
    HeaderBar::new().controls(ControlsSide::Right)
}

/// Prostokąt paska tytułu na górze okna.
pub fn header_rect(area: Rect, theme: &Theme) -> Rect {
    Rect::from_xywh(
        area.x(),
        area.y(),
        area.width(),
        theme.metrics.headerbar.height,
    )
}

/// Obszar pod paskiem tytułu (cała reszta okna).
pub fn content_area(area: Rect, theme: &Theme) -> Rect {
    let header = header_rect(area, theme);
    Rect::from_xywh(
        area.x(),
        header.bottom(),
        area.width(),
        (area.bottom() - header.bottom()).max(0.0),
    )
}

/// Liczy pełny układ: pasek tytułu, listę widgetów i panel dokumentacji.
pub fn layout(area: Rect, theme: &Theme, text: &TextEngine, state: &GalleryState) -> PageLayout {
    let header = header_rect(area, theme);
    let top = header.bottom();
    let inner_w = (area.width() - 2.0 * PAGE_PAD).max(0.0);
    let sidebar_w = SIDEBAR_MAX_W.min((inner_w * 0.34).max(200.0)).min(inner_w);
    let body_h = (area.bottom() - top - PAGE_PAD).max(0.0);
    let sidebar = Rect::from_xywh(area.x() + PAGE_PAD, top, sidebar_w, body_h);
    let detail_x = sidebar.right() + SIDEBAR_GAP;
    let detail_w = (area.right() - PAGE_PAD - detail_x).max(0.0);
    let detail = Rect::from_xywh(detail_x, top, detail_w, body_h);

    // ── Wyszukiwarka + lista widgetów + stopka z podpowiedzią ──
    let search = Rect::from_xywh(
        sidebar.x() + 10.0,
        sidebar.y() + 10.0,
        (sidebar.width() - 20.0).max(0.0),
        34.0,
    );
    const SIDEBAR_FOOTER_H: f64 = 66.0;
    let list = Rect::from_xywh(
        sidebar.x(),
        search.bottom() + 8.0,
        sidebar.width(),
        (sidebar.bottom() - search.bottom() - 12.0 - SIDEBAR_FOOTER_H).max(0.0),
    );
    let theme_switch = Rect::from_xywh(
        sidebar.x() + 10.0,
        sidebar.bottom() - 54.0,
        (sidebar.width() - 20.0).max(0.0),
        28.0,
    );
    let sidebar_hint = Point::new(sidebar.center().x, sidebar.bottom() - 14.0);

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

    // ── Panel dokumentacji ──
    let selected = state.selected.min(DEMOS.len().saturating_sub(1));
    let spec = &DEMOS[selected];
    let x = detail.x() + DETAIL_PAD;
    let w = (detail.width() - 2.0 * DETAIL_PAD).max(0.0);

    let caption = theme.typography.caption.size;
    let desc_lines = wrap_lines(spec.description, w, caption, text);
    let note_lines = wrap_lines(notes(spec.name), w - 18.0, caption, text);
    let code_lines = spec.code.lines().count().max(1) as f64;
    let code_h = code_lines * CODE_LINE_H + 2.0 * CODE_PAD;
    let desc_h = desc_lines.len() as f64 * TEXT_LINE_H;
    let notes_h = note_lines.len() as f64 * TEXT_LINE_H;

    // Pozycje liczone względem góry treści (offset dodajemy na końcu).
    let mut cursor = 0.0;
    let crumb_y = cursor + 8.0;
    cursor += 20.0;
    let title_y = cursor + 20.0;
    cursor += 32.0;
    let meta_y = cursor + 10.0;
    cursor += 22.0;
    let desc_y = cursor + 9.0;
    cursor += desc_h + 16.0;
    let preview_heading_y = cursor + 10.0;
    let preview_badge = Rect::from_xywh(x + w - 108.0, cursor + 10.0 - 10.0, 108.0, 20.0);
    cursor += 28.0;
    let preview = Rect::from_xywh(x, cursor, w, PREVIEW_H);
    cursor += PREVIEW_H + 18.0;
    let hint_y = cursor;
    cursor += 22.0;
    let code_heading_y = cursor + 10.0;
    let copy_w = Button::new("Kopiuj kod").width(text, theme).max(96.0);
    let copy_button = Rect::from_xywh(
        x + w - copy_w,
        code_heading_y - COPY_H / 2.0,
        copy_w,
        COPY_H,
    );
    cursor += 30.0;
    let code_meta_y = cursor + 9.0;
    cursor += 20.0;
    let code_block = Rect::from_xywh(x, cursor, w, code_h);
    cursor += code_h + 20.0;
    let notes_heading_y = cursor + 10.0;
    cursor += 28.0;
    let notes_y = cursor + 9.0;
    cursor += notes_h + 18.0;
    let prev_button = Rect::from_xywh(x, cursor, (w - NAV_GAP) / 2.0, NAV_H);
    let next_button = Rect::from_xywh(
        prev_button.right() + NAV_GAP,
        cursor,
        (w - NAV_GAP) / 2.0,
        NAV_H,
    );
    cursor += NAV_H + DETAIL_PAD;

    let detail_content = (cursor + DETAIL_PAD).max(detail.height());
    let detail_offset = state
        .detail
        .offset()
        .clamp(0.0, (detail_content - detail.height()).max(0.0));
    let content_top = detail.y() + DETAIL_PAD - detail_offset;

    let point = |offset_y: f64| Point::new(x, content_top + offset_y);

    PageLayout {
        header,
        sidebar,
        search,
        list,
        detail,
        rows,
        sidebar_content,
        detail_content,
        crumb: point(crumb_y),
        title: point(title_y),
        meta: point(meta_y),
        desc: point(desc_y),
        preview_heading: point(preview_heading_y),
        preview_badge: Rect::from_xywh(
            preview_badge.x(),
            content_top + preview_badge.y(),
            preview_badge.width(),
            preview_badge.height(),
        ),
        preview: Rect::from_xywh(
            preview.x(),
            content_top + preview.y(),
            preview.width(),
            preview.height(),
        ),
        hint: Point::new(detail.center().x, content_top + hint_y),
        code_heading: point(code_heading_y),
        copy_button: Rect::from_xywh(
            copy_button.x(),
            content_top + copy_button.y(),
            copy_button.width(),
            copy_button.height(),
        ),
        code_meta: point(code_meta_y),
        code_block: Rect::from_xywh(
            code_block.x(),
            content_top + code_block.y(),
            code_block.width(),
            code_block.height(),
        ),
        notes_heading: point(notes_heading_y),
        notes: point(notes_y),
        prev_button: Rect::from_xywh(
            prev_button.x(),
            content_top + prev_button.y(),
            prev_button.width(),
            prev_button.height(),
        ),
        next_button: Rect::from_xywh(
            next_button.x(),
            content_top + next_button.y(),
            next_button.width(),
            next_button.height(),
        ),
        prev: if selected > 0 {
            Some(selected - 1)
        } else {
            None
        },
        next: if selected + 1 < DEMOS.len() {
            Some(selected + 1)
        } else {
            None
        },
        theme_switch,
        sidebar_hint,
    }
}

/// Rysuje całą stronę dokumentacji widgetów.
pub fn paint(ctx: &mut PaintCtx, area: Rect, state: &GalleryState) {
    let theme = *ctx.theme;
    ctx.scene.fill_rect(area, theme.colors.view_bg);

    let lay = layout(area, &theme, ctx.text, state);
    paint_header(ctx, &lay, state);
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

fn paint_header(ctx: &mut PaintCtx, lay: &PageLayout, state: &GalleryState) {
    let theme = *ctx.theme;
    let bar = header_bar();
    bar.paint(ctx, lay.header, state.header_hover);
    let content = bar.controls_layout(lay.header, &theme).content;
    let cy = lay.header.center().y;

    ctx.label_left(
        content.x() + 2.0,
        cy,
        "Dokumentacja widgetów",
        theme.typography.body.size,
        weight::BOLD,
        theme.colors.text,
    );
    ctx.label_right(
        content.right() - 6.0,
        cy,
        &format!(
            "{} widgetów · {} kategorii",
            DEMOS.len(),
            Category::ALL.len()
        ),
        theme.typography.caption.size,
        weight::MEDIUM,
        theme.colors.accent,
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

    let mut theme_switcher = ViewSwitcher::new(["Jasny", "Ciemny", "Auto"]);
    theme_switcher.set_selected(theme_index(state.theme_pref));
    theme_switcher.paint(ctx, lay.theme_switch);

    ctx.label_center(
        lay.sidebar_hint.x,
        lay.sidebar_hint.y,
        "↑/↓ wybór · ←/→ poprzedni/następny",
        theme.typography.caption.size - 0.5,
        weight::NORMAL,
        theme.colors.dim_text.scale_alpha(0.85),
    );
}

fn theme_index(preference: ThemePreference) -> usize {
    match preference {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    }
}

fn theme_preference(index: usize) -> ThemePreference {
    match index {
        0 => ThemePreference::Light,
        1 => ThemePreference::Dark,
        _ => ThemePreference::System,
    }
}

fn paint_detail(ctx: &mut PaintCtx, lay: &PageLayout, state: &GalleryState) {
    let theme = *ctx.theme;
    let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
    let width = lay.preview.width();

    // Panel dokumentacji.
    ctx.scene
        .fill_rounded_rect(lay.detail, theme.radius.medium, theme.colors.window_bg);
    ctx.scene
        .stroke_rounded_rect(lay.detail, theme.radius.medium, 1.0, theme.colors.border);

    ctx.scene.push_clip(lay.detail);

    // Kategoria (okruszek) i tytuł.
    ctx.label_left(
        lay.crumb.x,
        lay.crumb.y,
        &spec.category.label().to_uppercase(),
        theme.typography.caption.size,
        weight::BOLD,
        theme.colors.accent,
    );
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
        &format!("libluna-widgets/src/{} · model immediate", spec.source),
        theme.typography.caption.size,
        weight::MEDIUM,
        theme.colors.dim_text,
    );

    let desc = wrap_lines(
        spec.description,
        width,
        theme.typography.caption.size,
        ctx.text,
    );
    for (i, line) in desc.iter().enumerate() {
        ctx.label_left(
            lay.desc.x,
            lay.desc.y + i as f64 * TEXT_LINE_H,
            line,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }

    // ── Działający przykład ──
    ctx.label_left(
        lay.preview_heading.x,
        lay.preview_heading.y,
        if gallery_interactive::supports(spec.name) {
            "Działający przykład"
        } else {
            "Podgląd widgetu"
        },
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let interactive = gallery_interactive::supports(spec.name);
    let (badge_label, badge_fg) = if interactive {
        ("INTERAKTYWNY", theme.colors.accent)
    } else {
        ("PODGLĄD", theme.colors.dim_text)
    };
    ctx.scene.fill_rounded_rect(
        lay.preview_badge,
        lay.preview_badge.height() / 2.0,
        badge_fg.scale_alpha(0.16),
    );
    ctx.label_center(
        lay.preview_badge.center().x,
        lay.preview_badge.center().y,
        badge_label,
        theme.typography.caption.size - 1.0,
        weight::BOLD,
        badge_fg,
    );

    ctx.scene
        .fill_rounded_rect(lay.preview, theme.radius.medium, theme.colors.view_bg);
    ctx.scene
        .stroke_rounded_rect(lay.preview, theme.radius.medium, 1.0, theme.colors.border);
    ctx.scene.push_clip(lay.preview);
    if !gallery_interactive::paint(spec.name, ctx, lay.preview, &state.example) {
        (spec.preview)(ctx, lay.preview);
    }
    ctx.scene.pop_clip();

    // Podpowiedź / wynik ostatniej akcji. Same widgety rysują własną podpowiedź
    // w podglądzie, więc tutaj pokazujemy tylko wynik akcji (albo opis trybu
    // statycznego, który nie ma wewnętrznej podpowiedzi).
    let status = state.example.status();
    let hint = if interactive {
        status
    } else {
        "Podgląd na żywo — ten widget nie ma stanu interaktywnego".to_owned()
    };
    if !hint.is_empty() {
        ctx.label_center(
            lay.hint.x,
            lay.hint.y,
            &hint,
            theme.typography.caption.size,
            weight::NORMAL,
            if interactive {
                theme.colors.accent
            } else {
                theme.colors.dim_text
            },
        );
    }

    // ── Kod ──
    ctx.label_left(
        lay.code_heading.x,
        lay.code_heading.y,
        "Kod",
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let mut copy = Button::new("Kopiuj kod");
    copy.set_hovered(state.hover_copy);
    copy.paint(ctx, lay.copy_button);

    if !state.status.is_empty() {
        let status_x = lay.copy_button.x() - 12.0;
        ctx.label_right(
            status_x,
            lay.code_heading.y,
            &state.status,
            theme.typography.caption.size,
            weight::MEDIUM,
            theme.colors.success,
        );
    }

    ctx.label_left(
        lay.code_meta.x,
        lay.code_meta.y,
        "use libluna::prelude::*; · ctx: &mut PaintCtx · rect: Rect · pos: Point",
        theme.typography.caption.size - 0.5,
        weight::NORMAL,
        theme.colors.dim_text.scale_alpha(0.9),
    );

    if lay.code_block.height() > 0.0 {
        ctx.scene.fill_rounded_rect(
            lay.code_block,
            theme.radius.small,
            theme.colors.shade.scale_alpha(0.7),
        );
        ctx.scene
            .stroke_rounded_rect(lay.code_block, theme.radius.small, 1.0, theme.colors.border);
        ctx.scene.push_clip(lay.code_block);
        let number_x = lay.code_block.x() + 12.0;
        let code_x = lay.code_block.x() + 34.0;
        let max_w = (lay.code_block.right() - 14.0 - code_x).max(20.0);
        for (i, line) in spec.code.lines().enumerate() {
            let cy = lay.code_block.y() + CODE_PAD + CODE_LINE_H / 2.0 + i as f64 * CODE_LINE_H;
            ctx.label_right(
                number_x + 14.0,
                cy,
                &(i + 1).to_string(),
                CODE_SIZE - 1.0,
                weight::NORMAL,
                theme.colors.dim_text.scale_alpha(0.55),
            );
            paint_code_line(ctx, code_x, cy, line, CODE_SIZE, max_w);
        }
        ctx.scene.pop_clip();
    }

    // ── Jak użyć ──
    ctx.label_left(
        lay.notes_heading.x,
        lay.notes_heading.y,
        "Jak użyć",
        theme.typography.heading.size,
        weight::BOLD,
        theme.colors.text,
    );
    let note_lines = wrap_lines(
        notes(spec.name),
        width - 18.0,
        theme.typography.caption.size,
        ctx.text,
    );
    for (i, line) in note_lines.iter().enumerate() {
        let y = lay.notes.y + i as f64 * TEXT_LINE_H;
        ctx.label_left(
            lay.notes.x,
            y,
            "•",
            theme.typography.caption.size,
            weight::BOLD,
            theme.colors.accent,
        );
        ctx.label_left(
            lay.notes.x + 14.0,
            y,
            line,
            theme.typography.caption.size,
            weight::NORMAL,
            theme.colors.dim_text,
        );
    }

    // ── Poprzedni / następny widget ──
    paint_nav_card(
        ctx,
        lay.prev_button,
        lay.prev.map(|i| DEMOS[i].name),
        "← Poprzedni",
        state.hover_prev,
    );
    paint_nav_card(
        ctx,
        lay.next_button,
        lay.next.map(|i| DEMOS[i].name),
        "Następny →",
        state.hover_next,
    );

    ctx.scene.pop_clip();
}

fn paint_nav_card(
    ctx: &mut PaintCtx,
    rect: Rect,
    name: Option<&str>,
    caption: &str,
    hovered: bool,
) {
    let theme = *ctx.theme;
    let enabled = name.is_some();
    let bg = if hovered && enabled {
        theme.colors.shade.scale_alpha(1.8)
    } else {
        theme.colors.card_bg
    };
    ctx.scene.fill_rounded_rect(rect, theme.radius.small, bg);
    ctx.scene
        .stroke_rounded_rect(rect, theme.radius.small, 1.0, theme.colors.border);

    ctx.label_left(
        rect.x() + 14.0,
        rect.y() + 16.0,
        caption,
        theme.typography.caption.size - 1.0,
        weight::MEDIUM,
        if enabled {
            theme.colors.accent
        } else {
            theme.colors.dim_text.scale_alpha(0.6)
        },
    );
    let label = name.unwrap_or("—");
    let label = ellipsize(
        label,
        (rect.width() - 28.0).max(20.0),
        theme.typography.body.size,
        ctx.text,
    );
    ctx.label_left(
        rect.x() + 14.0,
        rect.y() + 34.0,
        &label,
        theme.typography.body.size,
        weight::MEDIUM,
        if enabled {
            theme.colors.text
        } else {
            theme.colors.dim_text.scale_alpha(0.6)
        },
    );
}

/// Prosty „highlighting”: słowa kluczowe akcentem, komentarze i łańcuchy inaczej.
fn paint_code_line(ctx: &mut PaintCtx, x: f64, cy: f64, line: &str, size: f64, max_w: f64) {
    let theme = *ctx.theme;
    if line.trim_start().starts_with("//") {
        let text = ellipsize(line, max_w, size, ctx.text);
        ctx.label_left(x, cy, &text, size, weight::NORMAL, theme.colors.dim_text);
        return;
    }
    let mut cx = x;
    for token in line.split_inclusive(' ') {
        if cx >= x + max_w {
            break;
        }
        let trimmed = token.trim_end();
        let color = if is_keyword(trimmed.trim()) {
            theme.colors.accent
        } else if trimmed.trim().starts_with('"') {
            theme.colors.success
        } else {
            theme.colors.text
        };
        let remaining = x + max_w - cx;
        let shown = ellipsize(token, remaining, size, ctx.text);
        let w = ctx.measure(&shown, size, weight::NORMAL);
        ctx.label_left(cx, cy, &shown, size, weight::NORMAL, color);
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
            | "return"
            | "struct"
            | "enum"
    )
}

// --- Obsługa zdarzeń ---

/// Klik w stronę dokumentacji. Zwraca `true`, jeśli zdarzenie zostało zużyte.
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
        if !lay.list.contains(p) {
            break;
        }
        if let SidebarRow::Item { demo, rect } = row {
            if rect.contains(p) {
                state.search.set_focused(false);
                select(state, *demo);
                ensure_row_visible(state, &lay, *demo);
                return true;
            }
        }
    }
    if lay.theme_switch.contains(p) {
        let switcher = ViewSwitcher::new(["Jasny", "Ciemny", "Auto"]);
        if let Some(index) = switcher.index_at(lay.theme_switch, p) {
            let pref = theme_preference(index);
            state.theme_pref = pref;
            state.theme_change = Some(pref);
        }
        return true;
    }
    if lay.sidebar.contains(p) {
        return true;
    }

    if lay.copy_button.contains(p) {
        state.search.set_focused(false);
        let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
        state.copy_request = Some(spec.code.to_owned());
        state.status = "Skopiowano kod".to_owned();
        return true;
    }

    if lay.preview.contains(p) {
        state.search.set_focused(false);
        let spec = &DEMOS[state.selected.min(DEMOS.len().saturating_sub(1))];
        gallery_interactive::pressed(spec.name, lay.preview, p, theme, text, &mut state.example);
        return true;
    }

    if lay.prev_button.contains(p) && lay.prev.is_some() {
        state.search.set_focused(false);
        if let Some(prev) = lay.prev {
            select(state, prev);
            let lay = layout(area, theme, text, state);
            ensure_row_visible(state, &lay, prev);
        }
        return true;
    }
    if lay.next_button.contains(p) && lay.next.is_some() {
        state.search.set_focused(false);
        if let Some(next) = lay.next {
            select(state, next);
            let lay = layout(area, theme, text, state);
            ensure_row_visible(state, &lay, next);
        }
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
    state.hover_row = if lay.list.contains(p) {
        lay.rows.iter().find_map(|row| match row {
            SidebarRow::Item { demo, rect } if rect.contains(p) => Some(*demo),
            _ => None,
        })
    } else {
        None
    };
    state.hover_copy = lay.copy_button.contains(p);
    state.hover_prev = lay.prev_button.contains(p) && lay.prev.is_some();
    state.hover_next = lay.next_button.contains(p) && lay.next.is_some();
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

/// Kółko myszy nad listą albo panelem dokumentacji.
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

/// Przewijanie panelu dokumentacji klawiaturą (PageUp/PageDown).
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

/// Klawiatura na stronie dokumentacji. Zwraca `true`, gdy zdarzenie zostało zużyte.
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
        0xff51 => {
            if let Some(prev) = state.selected.checked_sub(1) {
                select(state, prev);
                let lay = layout(area, theme, text, state);
                ensure_row_visible(state, &lay, prev);
            }
            return true;
        }
        0xff53 => {
            let next = (state.selected + 1).min(DEMOS.len() - 1);
            select(state, next);
            let lay = layout(area, theme, text, state);
            ensure_row_visible(state, &lay, next);
            return true;
        }
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
    state.hover_copy = false;
    state.hover_prev = false;
    state.hover_next = false;
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

    /// Podgląd strony dokumentacji do PNG — pozwala obejrzeć ją bez kompozytora.
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
        render(&state, "/tmp/luna-gallery-switch.png");

        state.selected = demo_index("Button");
        state.status = "Skopiowano kod".to_owned();
        render(&state, "/tmp/luna-gallery-code.png");

        state.selected = demo_index("Calendar");
        render(&state, "/tmp/luna-gallery-calendar.png");

        state.selected = demo_index("HeaderBar");
        state.example.header_variant = 3;
        render(&state, "/tmp/luna-gallery-headerbar.png");

        // Filtr listy (np. „color” → ColorButton / ColorChooser).
        let mut filtered = GalleryState::default();
        filtered.search.set_text("color");
        filtered.search.set_focused(true);
        search_changed(&mut filtered);
        render(&filtered, "/tmp/luna-gallery-search.png");
    }

    /// Podgląd całego okna (sidebar + header + dokumentacja) do PNG.
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
        let mut names = std::collections::BTreeSet::new();
        for demo in DEMOS {
            assert!(names.insert(demo.name), "zduplikowana nazwa: {}", demo.name);
            assert!(
                !demo.code.trim().is_empty(),
                "{} nie ma przykładu kodu",
                demo.name
            );
            assert!(
                !notes(demo.name).trim().is_empty(),
                "{} nie ma noty „jak użyć”",
                demo.name
            );
        }
    }

    #[test]
    fn every_demo_paints_in_detail() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        for (demo, spec) in DEMOS.iter().enumerate() {
            state.selected = demo;
            let mut scene = Scene::new();
            let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
            paint(&mut ctx, area(), &state);
            assert!(!scene.is_empty(), "{} nie narysował się", spec.name);
            for command in scene.commands() {
                let rect = match command {
                    libluna_render::DrawCommand::FillRect { rect, .. }
                    | libluna_render::DrawCommand::FillRoundedRect { rect, .. }
                    | libluna_render::DrawCommand::StrokeRoundedRect { rect, .. } => Some(*rect),
                    _ => None,
                };
                if let Some(rect) = rect {
                    assert!(
                        rect.width() > 0.0 && rect.height() > 0.0,
                        "{} narysował pusty prostokąt {rect:?}",
                        spec.name
                    );
                }
            }
        }
    }

    #[test]
    fn every_demo_renders_without_panicking() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let size = Size::new(1200.0, 1000.0);
        let area = Rect::from_size(size);
        let mut state = GalleryState::default();
        for demo in 0..DEMOS.len() {
            state.selected = demo;
            let mut scene = Scene::new();
            {
                let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
                paint(&mut ctx, area, &state);
            }
            let mut renderer = SoftwareRenderer::new(size.width as u32, size.height as u32);
            renderer.draw(&scene, 1.0, theme.colors.window_bg);
        }
    }

    #[test]
    fn code_and_notes_are_always_visible() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert!(lay.code_block.height() > 0.0);
        assert!(lay.code_block.bottom() <= lay.detail_content + lay.detail.y());
        assert!(lay.notes.y > lay.code_block.y());
    }

    #[test]
    fn copy_button_requests_the_visible_code() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.copy_button.center()
        ));
        assert_eq!(state.copy_request.as_deref(), Some(DEMOS[0].code));
    }

    #[test]
    fn next_and_previous_cards_select_widgets() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.next_button.center()
        ));
        assert_eq!(state.selected, 1);

        let lay = layout(area(), &theme, &text, &state);
        assert!(pointer_pressed(
            &mut state,
            area(),
            &theme,
            &text,
            lay.prev_button.center()
        ));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn keyboard_moves_between_widgets() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        let down = KeyEvent {
            keysym: 0xff54,
            keycode: 0,
            state: ButtonState::Pressed,
            modifiers: libluna_core::Modifiers::NONE,
            text: None,
            repeat: false,
        };
        assert!(key(&mut state, area(), &theme, &text, &down));
        assert_eq!(state.selected, 1);

        let right = KeyEvent {
            keysym: 0xff53,
            keycode: 0,
            state: ButtonState::Pressed,
            modifiers: libluna_core::Modifiers::NONE,
            text: None,
            repeat: false,
        };
        assert!(key(&mut state, area(), &theme, &text, &right));
        assert_eq!(state.selected, 2);
    }

    #[test]
    fn theme_switch_requests_a_theme_change() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        let lay = layout(area(), &theme, &text, &state);
        let dark =
            ViewSwitcher::new(["Jasny", "Ciemny", "Auto"]).segments(lay.theme_switch)[1].center();
        assert!(pointer_pressed(&mut state, area(), &theme, &text, dark));
        assert_eq!(state.theme_pref, ThemePreference::Dark);
        assert_eq!(state.theme_change, Some(ThemePreference::Dark));
    }

    #[test]
    fn searching_selects_first_match_and_scrolls_back() {
        let mut state = GalleryState::default();
        state.search.set_text("calendar");
        search_changed(&mut state);
        assert_eq!(DEMOS[state.selected].name, "Calendar");
        assert_eq!(state.sidebar.offset(), 0.0);
    }

    #[test]
    fn wheel_scrolls_sidebar_and_detail_separately() {
        let theme = Theme::light();
        let text = TextEngine::system();
        let mut state = GalleryState::default();
        state.selected = demo_index("Assistant");
        let lay = layout(area(), &theme, &text, &state);
        assert!(lay.detail_content > lay.detail.height());
        let detail_y = state.detail.offset();
        assert!(scroll(
            &mut state,
            area(),
            &theme,
            &text,
            lay.detail.center(),
            120.0
        ));
        assert!(state.detail.offset() > detail_y);
        assert_eq!(state.sidebar.offset(), 0.0);
    }
}
