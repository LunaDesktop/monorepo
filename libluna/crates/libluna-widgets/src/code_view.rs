//! `CodeView` — wielolinijkowy edytor tekstu z podświetlaniem składni.
//!
//! Uzupełnia [`TextView`](crate::TextView) o to, czego potrzebuje edytor kodu:
//! kolorowane fragmenty ([`Span`]), gutter z numerami linii, podświetlenie
//! bieżącej linii, przewijanie w obu osiach oraz trafianie kursorem w tekst.
//!
//! Widget jest „immediate”: trzyma stan (tekst, kursor, zaznaczenie, spany),
//! a aplikacja woła `paint` i metody edycyjne. Sam nie zna żadnego języka —
//! podświetlanie dostarcza aplikacja (np. `luna-editor`).

use libluna_core::style::weight;
use libluna_core::{Color, Point, Rect, Theme};
use libluna_render::TextEngine;

use crate::ctx::PaintCtx;
use crate::display::Orientation;
use crate::scrollbar::Scrollbar;

/// Rodzaj fragmentu tekstu do pokolorowania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    TypeName,
    Function,
    String,
    Number,
    Comment,
    Operator,
    Constant,
    Variable,
}

/// Zakres bajtowy w tekście i jego rodzaj.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

impl Span {
    pub fn new(start: usize, end: usize, kind: TokenKind) -> Self {
        Self { start, end, kind }
    }
}

/// Kolor tokenu dla motywu.
pub fn token_color(kind: TokenKind, theme: &Theme) -> Color {
    match kind {
        TokenKind::Plain => theme.colors.text,
        TokenKind::Keyword => theme.syntax.keyword,
        TokenKind::TypeName => theme.syntax.type_name,
        TokenKind::Function => theme.syntax.function,
        TokenKind::String => theme.syntax.string,
        TokenKind::Number => theme.syntax.number,
        TokenKind::Comment => theme.syntax.comment,
        TokenKind::Operator => theme.syntax.operator,
        TokenKind::Constant => theme.syntax.constant,
        TokenKind::Variable => theme.syntax.variable,
    }
}

/// Pole tekstowe edytora kodu.
#[derive(Debug, Clone)]
pub struct CodeView {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    focused: bool,
    spans: Vec<Span>,
    vertical: Scrollbar,
    horizontal: Scrollbar,
    /// Szerokość tabulatora w kolumnach.
    tab_width: usize,
    /// Początki linii (bajty); zawsze zawiera 0.
    starts: Vec<usize>,
    /// Najdłuższa linia w znakach (do zakresu przewijania poziomego).
    max_chars: usize,
}

impl Default for CodeView {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            anchor: None,
            focused: false,
            spans: Vec::new(),
            vertical: Scrollbar::new(Orientation::Vertical),
            horizontal: Scrollbar::new(Orientation::Horizontal),
            tab_width: 4,
            starts: vec![0],
            max_chars: 0,
        }
    }
}

/// Geometria wnętrza pola dla danego prostokąta i motywu.
#[derive(Debug, Clone, Copy)]
pub struct CodeMetrics {
    /// Obszar tekstu (bez guttera i pasków przewijania).
    pub text_area: Rect,
    /// Kolumna numerów linii.
    pub gutter: Rect,
    pub line_height: f64,
    /// Szerokość znaku monospace.
    pub char_width: f64,
}

impl CodeView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = 0;
        self.anchor = None;
        self.spans.clear();
        self.vertical.adjustment_mut().set_value(0.0);
        self.horizontal.adjustment_mut().set_value(0.0);
        self.reindex();
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        self.cursor = clamp_boundary(&self.text, cursor);
        self.anchor = None;
    }

    pub fn set_anchor(&mut self, anchor: Option<usize>) {
        self.anchor = anchor.map(|anchor| clamp_boundary(&self.text, anchor));
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Zaznaczenie jako `(start, end)` w kolejności tekstu.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            None
        } else if anchor < self.cursor {
            Some((anchor, self.cursor))
        } else {
            Some((self.cursor, anchor))
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    pub fn set_selection(&mut self, start: usize, end: usize) {
        let start = clamp_boundary(&self.text, start);
        let end = clamp_boundary(&self.text, end);
        self.anchor = Some(start);
        self.cursor = end;
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(start, end)| &self.text[start..end])
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_tab_width(&mut self, tab_width: usize) {
        self.tab_width = tab_width.max(1);
    }

    pub fn tab_width(&self) -> usize {
        self.tab_width
    }

    /// Ustawia spany podświetlania (sortuje, przycina do tekstu i usuwa nakładanie).
    pub fn set_spans(&mut self, spans: Vec<Span>) {
        let mut spans: Vec<Span> = spans
            .into_iter()
            .filter_map(|span| {
                let start = clamp_boundary(&self.text, span.start.min(span.end));
                let end = clamp_boundary(&self.text, span.start.max(span.end));
                (start < end).then_some(Span { start, end, ..span })
            })
            .collect();
        spans.sort_by_key(|span| (span.start, span.end));
        let mut previous_end = 0;
        spans = spans
            .into_iter()
            .filter_map(|span| {
                let start = span.start.max(previous_end);
                if start >= span.end {
                    return None;
                }
                previous_end = span.end;
                Some(Span { start, ..span })
            })
            .collect();
        self.spans = spans;
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Liczba linii (tekst kończący się `\n` ma dodatkową, pustą linię).
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// `(linia, kolumna)` 1-based do paska statusu.
    pub fn line_col(&self) -> (usize, usize) {
        let line = self.cursor_line();
        let start = self.starts[line];
        let col = self.text[start..self.cursor].chars().count() + 1;
        (line + 1, col)
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Koniec linii bez znaku `\n`.
    pub fn line_end(&self, line: usize) -> usize {
        match self.starts.get(line + 1) {
            Some(&next) => next.saturating_sub(1),
            None => self.text.len(),
        }
    }

    pub fn line_text(&self, line: usize) -> &str {
        &self.text[self.line_start(line)..self.line_end(line)]
    }

    pub fn goto_line(&mut self, line: usize) {
        let line = line.saturating_sub(1).min(self.line_count() - 1);
        self.cursor = self.starts[line];
        self.anchor = None;
    }

    /// Zamienia zakres na nowy tekst (jedna operacja — podstawa undo/redo).
    pub fn replace_range(&mut self, start: usize, end: usize, insert: &str) {
        let start = clamp_boundary(&self.text, start.min(end));
        let end = clamp_boundary(&self.text, start.max(end));
        self.text.replace_range(start..end, insert);
        self.cursor = start + insert.len();
        self.anchor = None;
        self.reindex();
    }

    pub fn insert_str(&mut self, insert: &str) {
        let (start, end) = self.selection().unwrap_or((self.cursor, self.cursor));
        self.replace_range(start, end, insert);
    }

    pub fn insert_char(&mut self, ch: char) {
        let mut buf = [0u8; 4];
        self.insert_str(ch.encode_utf8(&mut buf));
    }

    pub fn backspace(&mut self) {
        if let Some((start, end)) = self.selection() {
            self.replace_range(start, end, "");
        } else if let Some(previous) = prev_boundary(&self.text, self.cursor) {
            self.replace_range(previous, self.cursor, "");
        }
    }

    pub fn delete_forward(&mut self) {
        if let Some((start, end)) = self.selection() {
            self.replace_range(start, end, "");
        } else if let Some(next) = next_boundary(&self.text, self.cursor) {
            self.replace_range(self.cursor, next, "");
        }
    }

    pub fn copy(&self) -> Option<String> {
        self.selected_text().map(str::to_owned)
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.selected_text()?.to_owned();
        if let Some((start, end)) = self.selection() {
            self.replace_range(start, end, "");
        }
        Some(text)
    }

    pub fn paste(&mut self, text: &str) {
        let plain: String = text.chars().filter(|ch| *ch != '\r').collect();
        self.insert_str(&plain);
    }

    // ── Ruch kursora ────────────────────────────────────────────────────────

    fn move_to(&mut self, position: usize, select: bool) {
        let position = clamp_boundary(&self.text, position);
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = position;
    }

    pub fn move_left(&mut self, select: bool) {
        if !select {
            if let Some((start, _)) = self.selection() {
                self.move_to(start, false);
                return;
            }
        }
        let position = prev_boundary(&self.text, self.cursor).unwrap_or(0);
        self.move_to(position, select);
    }

    pub fn move_right(&mut self, select: bool) {
        if !select {
            if let Some((_, end)) = self.selection() {
                self.move_to(end, false);
                return;
            }
        }
        let position = next_boundary(&self.text, self.cursor).unwrap_or(self.text.len());
        self.move_to(position, select);
    }

    pub fn move_up(&mut self, select: bool) {
        let line = self.cursor_line();
        if line == 0 {
            self.move_to(0, select);
            return;
        }
        let column = self.column_chars();
        let target = self.starts[line - 1] + byte_at_char(self.line_text(line - 1), column);
        self.move_to(target, select);
    }

    pub fn move_down(&mut self, select: bool) {
        let line = self.cursor_line();
        if line + 1 >= self.line_count() {
            self.move_to(self.text.len(), select);
            return;
        }
        let column = self.column_chars();
        let target = self.starts[line + 1] + byte_at_char(self.line_text(line + 1), column);
        self.move_to(target, select);
    }

    pub fn move_page(&mut self, lines: isize, select: bool) {
        let line = self.cursor_line();
        let target_line = if lines < 0 {
            line.saturating_sub(lines.unsigned_abs())
        } else {
            (line + lines as usize).min(self.line_count() - 1)
        };
        let column = self.column_chars();
        let target = self.starts[target_line] + byte_at_char(self.line_text(target_line), column);
        self.move_to(target, select);
    }

    pub fn home(&mut self, select: bool) {
        let line = self.cursor_line();
        self.move_to(self.starts[line], select);
    }

    pub fn end(&mut self, select: bool) {
        let line = self.cursor_line();
        self.move_to(self.line_end(line), select);
    }

    pub fn document_start(&mut self, select: bool) {
        self.move_to(0, select);
    }

    pub fn document_end(&mut self, select: bool) {
        self.move_to(self.text.len(), select);
    }

    /// Indeks bajtowy początku bieżącej linii.
    pub fn cursor_line(&self) -> usize {
        match self.starts.binary_search(&self.cursor) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        }
    }

    fn column_chars(&self) -> usize {
        let start = self.starts[self.cursor_line()];
        self.text[start..self.cursor].chars().count()
    }

    // ── Geometria i przewijanie ─────────────────────────────────────────────

    pub fn metrics(&self, rect: Rect, theme: &Theme, text: &TextEngine) -> CodeMetrics {
        let pad = theme.spacing.sm;
        let line_height = theme.typography.body.line_height;
        let size = theme.typography.body.size;
        let char_width = text.measure("0", size, weight::NORMAL).width.max(1.0);
        let scroll = theme.metrics.scrollview;
        let reserve = scroll.margin + scroll.thickness;
        let digits = self.line_count().to_string().len().max(2);
        let gutter_width = char_width * digits as f64 + pad * 2.0;
        let inner = Rect::from_xywh(
            rect.x() + pad,
            rect.y() + pad,
            (rect.width() - 2.0 * pad).max(0.0),
            (rect.height() - 2.0 * pad).max(0.0),
        );
        let gutter = Rect::from_xywh(
            inner.x(),
            inner.y(),
            gutter_width.min(inner.width()),
            inner.height(),
        );
        let text_area = Rect::from_xywh(
            gutter.right() + pad,
            inner.y(),
            (inner.right() - reserve - 4.0 - (gutter.right() + pad)).max(0.0),
            (inner.bottom() - reserve - 4.0 - inner.y()).max(0.0),
        );
        CodeMetrics {
            text_area,
            gutter,
            line_height,
            char_width,
        }
    }

    fn content_size(&self, theme: &Theme, metrics: &CodeMetrics) -> (f64, f64) {
        let height = self.line_count() as f64 * metrics.line_height + theme.spacing.sm;
        let width = self.max_chars as f64 * metrics.char_width + theme.spacing.lg;
        (width, height)
    }

    fn update_bounds(&mut self, rect: Rect, theme: &Theme, text: &TextEngine) -> CodeMetrics {
        let metrics = self.metrics(rect, theme, text);
        let (width, height) = self.content_size(theme, &metrics);
        self.vertical.adjustment_mut().set_bounds(
            0.0,
            height.max(metrics.text_area.height()),
            metrics.text_area.height(),
        );
        self.horizontal.adjustment_mut().set_bounds(
            0.0,
            width.max(metrics.text_area.width()),
            metrics.text_area.width(),
        );
        metrics
    }

    pub fn vertical_offset(&self) -> f64 {
        self.vertical.adjustment().value()
    }

    pub fn horizontal_offset(&self) -> f64 {
        self.horizontal.adjustment().value()
    }

    pub fn set_vertical_offset(&mut self, offset: f64) {
        self.vertical.adjustment_mut().set_value(offset);
    }

    /// Przewija o `dx`/`dy` pikseli (rolka, Shift+rolka).
    pub fn scroll(&mut self, dx: f64, dy: f64, rect: Rect, theme: &Theme, text: &TextEngine) {
        self.update_bounds(rect, theme, text);
        if dx != 0.0 {
            self.horizontal.on_scroll(dx, rect);
        }
        if dy != 0.0 {
            self.vertical.on_scroll(dy, rect);
        }
    }

    /// Przewija tak, żeby kursor był widoczny (po edycji lub ruchu).
    pub fn ensure_cursor_visible(&mut self, rect: Rect, theme: &Theme, text: &TextEngine) {
        let metrics = self.update_bounds(rect, theme, text);
        let line = self.cursor_line();
        let top = line as f64 * metrics.line_height;
        let bottom = top + metrics.line_height;
        let value = self.vertical.adjustment().value();
        let page = metrics.text_area.height();
        if top < value {
            self.vertical.adjustment_mut().set_value(top);
        } else if bottom > value + page {
            self.vertical.adjustment_mut().set_value(bottom - page);
        }

        let start = self.starts[self.cursor_line()];
        let prefix = expand_tabs(&self.text[start..self.cursor], self.tab_width);
        let x = text
            .measure(&prefix, theme.typography.body.size, weight::NORMAL)
            .width;
        let h = self.horizontal.adjustment().value();
        let width = metrics.text_area.width();
        if x < h {
            self.horizontal.adjustment_mut().set_value(x - 4.0);
        } else if x + 4.0 > h + width {
            self.horizontal.adjustment_mut().set_value(x + 4.0 - width);
        }
    }

    /// Ustawia podświetlenie pasków, gdy kursor jest nad nimi.
    pub fn set_scroll_hover(&mut self, rect: Rect, theme: &Theme, point: Option<Point>) {
        let point = point.unwrap_or(Point::new(-1.0, -1.0));
        self.vertical
            .set_hovered(self.vertical.thumb_at(rect, theme, point));
        self.horizontal
            .set_hovered(self.horizontal.thumb_at(rect, theme, point));
    }

    /// Rozpoczyna przeciąganie paska; `true`, gdy klik trafił w pasek.
    pub fn begin_scroll_drag(&mut self, rect: Rect, theme: &Theme, point: Point) -> bool {
        if self.vertical.begin_drag(point, rect, theme) {
            return true;
        }
        self.horizontal.begin_drag(point, rect, theme)
    }

    pub fn drag_scroll(&mut self, rect: Rect, theme: &Theme, point: Point) {
        self.vertical.drag_to(point, rect, theme);
        self.horizontal.drag_to(point, rect, theme);
    }

    pub fn end_scroll_drag(&mut self) {
        self.vertical.end_drag();
        self.horizontal.end_drag();
    }

    pub fn is_scroll_dragging(&self) -> bool {
        self.vertical.is_dragging() || self.horizontal.is_dragging()
    }

    /// Indeks bajtowy pod punktem (klik myszą).
    pub fn index_at(&self, rect: Rect, theme: &Theme, text: &TextEngine, point: Point) -> usize {
        let metrics = self.metrics(rect, theme, text);
        if point.y < metrics.text_area.y() {
            return 0;
        }
        let line = (((point.y - metrics.text_area.y()) + self.vertical_offset())
            / metrics.line_height)
            .floor()
            .max(0.0) as usize;
        let line = line.min(self.line_count() - 1);
        let start = self.starts[line];
        let target = (point.x - metrics.text_area.x()) + self.horizontal_offset();
        let end = self.line_end(line);
        let line_text = &self.text[start..end];
        let size = theme.typography.body.size;
        let mut x = 0.0;
        for (index, ch) in line_text.char_indices() {
            let width = if ch == '\t' {
                let spaces =
                    self.tab_width - ((x / metrics.char_width).round() as usize) % self.tab_width;
                spaces as f64 * metrics.char_width
            } else {
                let mut buf = [0u8; 4];
                text.measure(ch.encode_utf8(&mut buf), size, weight::NORMAL)
                    .width
            };
            if x + width / 2.0 >= target {
                return start + index;
            }
            x += width;
        }
        end
    }

    /// Linia pod punktem (0-based) — przydatne do kliknięcia w gutter.
    pub fn line_at(&self, rect: Rect, theme: &Theme, text: &TextEngine, point: Point) -> usize {
        let metrics = self.metrics(rect, theme, text);
        let line = (((point.y - metrics.text_area.y()) + self.vertical_offset())
            / metrics.line_height)
            .floor()
            .max(0.0) as usize;
        line.min(self.line_count() - 1)
    }

    // ── Rysowanie ───────────────────────────────────────────────────────────

    pub fn paint(&mut self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let colors = theme.colors;
        let metrics = self.update_bounds(rect, &theme, ctx.text);
        let body = theme.typography.body;
        let caption = theme.typography.caption;

        ctx.scene
            .fill_rounded_rect(rect, theme.radius.medium, colors.view_bg);
        ctx.scene
            .stroke_rounded_rect(rect, theme.radius.medium, 1.0, colors.border);

        ctx.scene
            .fill_rect(metrics.gutter, colors.shade.scale_alpha(0.35));
        ctx.scene.fill_rect(
            Rect::from_xywh(
                metrics.gutter.right(),
                metrics.gutter.y(),
                1.0,
                metrics.gutter.height(),
            ),
            colors.border,
        );

        let first = (self.vertical_offset() / metrics.line_height)
            .floor()
            .max(0.0) as usize;
        let rows = (metrics.text_area.height() / metrics.line_height).ceil() as usize + 1;
        let last = (first + rows + 1).min(self.line_count());

        let selection = self.selection();
        let current_line = self.cursor_line();

        // Numery linii rysujemy w osobnym clipie — poziome przewijanie
        // nie może ich przesuwać ani zasłaniać.
        ctx.scene.push_clip(metrics.gutter);
        for line in first..last {
            let y =
                metrics.text_area.y() + line as f64 * metrics.line_height - self.vertical_offset();
            if line == current_line && self.focused {
                ctx.scene.fill_rect(
                    Rect::from_xywh(
                        metrics.gutter.x(),
                        y,
                        metrics.gutter.width(),
                        metrics.line_height,
                    ),
                    colors.shade.scale_alpha(0.55),
                );
            }
            let number = (line + 1).to_string();
            ctx.label_right(
                metrics.gutter.right() - theme.spacing.sm,
                y + metrics.line_height / 2.0,
                &number,
                caption.size,
                weight::NORMAL,
                if line == current_line {
                    colors.text
                } else {
                    colors.dim_text
                },
            );
        }
        ctx.scene.pop_clip();

        ctx.scene.push_clip(metrics.text_area);
        for line in first..last {
            let start = self.starts[line];
            let end = self.line_end(line);
            let y =
                metrics.text_area.y() + line as f64 * metrics.line_height - self.vertical_offset();

            if line == current_line && self.focused {
                ctx.scene.fill_rect(
                    Rect::from_xywh(
                        metrics.text_area.x() - 4.0,
                        y,
                        metrics.text_area.width() + 4.0,
                        metrics.line_height,
                    ),
                    colors.shade.scale_alpha(0.55),
                );
            }

            if let Some((sel_start, sel_end)) = selection {
                let lo = sel_start.max(start);
                let hi = sel_end.min(end);
                if lo < hi {
                    let x1 = metrics.text_area.x()
                        + ctx.measure(
                            &expand_tabs(&self.text[start..lo], self.tab_width),
                            body.size,
                            weight::NORMAL,
                        )
                        - self.horizontal_offset();
                    let x2 = metrics.text_area.x()
                        + ctx.measure(
                            &expand_tabs(&self.text[start..hi], self.tab_width),
                            body.size,
                            weight::NORMAL,
                        )
                        - self.horizontal_offset();
                    ctx.scene.fill_rect(
                        Rect::from_xywh(
                            x1,
                            y + 1.0,
                            (x2 - x1).max(0.0),
                            (metrics.line_height - 2.0).max(0.0),
                        ),
                        colors.accent.scale_alpha(0.28),
                    );
                }
            }

            self.paint_line_text(ctx, &metrics, start, end, y, body.size);
        }

        if self.focused {
            let line = current_line;
            let start = self.starts[line];
            let prefix = expand_tabs(&self.text[start..self.cursor], self.tab_width);
            let x = metrics.text_area.x() + ctx.measure(&prefix, body.size, weight::NORMAL)
                - self.horizontal_offset();
            let y =
                metrics.text_area.y() + line as f64 * metrics.line_height - self.vertical_offset();
            ctx.scene.fill_rect(
                Rect::from_xywh(x, y + 2.0, 1.6, (metrics.line_height - 4.0).max(0.0)),
                colors.text,
            );
        }
        ctx.scene.pop_clip();

        self.vertical.paint(ctx, rect);
        self.horizontal.paint(ctx, rect);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_line_text(
        &self,
        ctx: &mut PaintCtx,
        metrics: &CodeMetrics,
        start: usize,
        end: usize,
        y: f64,
        size: f64,
    ) {
        if start == end {
            return;
        }
        let theme = *ctx.theme;
        let mut position = start;
        for span in self
            .spans
            .iter()
            .filter(|span| span.start < end && span.end > start)
        {
            let span_start = span.start.max(start);
            let span_end = span.end.min(end);
            if span_start > position {
                self.paint_run(
                    ctx,
                    metrics,
                    position,
                    span_start,
                    start,
                    y,
                    size,
                    theme.colors.text,
                );
            }
            if span_end > span_start {
                self.paint_run(
                    ctx,
                    metrics,
                    span_start,
                    span_end,
                    start,
                    y,
                    size,
                    token_color(span.kind, &theme),
                );
            }
            position = position.max(span_end);
        }
        if position < end {
            self.paint_run(
                ctx,
                metrics,
                position,
                end,
                start,
                y,
                size,
                theme.colors.text,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_run(
        &self,
        ctx: &mut PaintCtx,
        metrics: &CodeMetrics,
        from: usize,
        to: usize,
        line_start: usize,
        y: f64,
        size: f64,
        color: Color,
    ) {
        let prefix = expand_tabs(&self.text[line_start..from], self.tab_width);
        let x = metrics.text_area.x() + ctx.measure(&prefix, size, weight::NORMAL)
            - self.horizontal_offset();
        let column = prefix.chars().count();
        let run = expand_tabs_from(&self.text[from..to], column, self.tab_width);
        ctx.label_left(
            x,
            y + metrics.line_height / 2.0,
            &run,
            size,
            weight::NORMAL,
            color,
        );
    }

    fn reindex(&mut self) {
        self.starts.clear();
        self.starts.push(0);
        for (index, byte) in self.text.bytes().enumerate() {
            if byte == b'\n' {
                self.starts.push(index + 1);
            }
        }
        self.max_chars = (0..self.starts.len())
            .map(|line| self.line_text(line).chars().count())
            .max()
            .unwrap_or(0);
        self.cursor = clamp_boundary(&self.text, self.cursor);
        if let Some(anchor) = self.anchor {
            self.anchor = Some(clamp_boundary(&self.text, anchor));
        }
    }
}

/// Zamienia tabulatory na spacje (do pomiaru i rysowania).
fn expand_tabs(text: &str, tab_width: usize) -> String {
    expand_tabs_from(text, 0, tab_width)
}

/// Jak [`expand_tabs`], ale liczy kolumny od `start_column` (kolejne fragmenty linii).
fn expand_tabs_from(text: &str, start_column: usize, tab_width: usize) -> String {
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(text.len());
    let mut column = start_column;
    for ch in text.chars() {
        if ch == '\t' {
            let spaces = tab_width - column % tab_width;
            for _ in 0..spaces {
                out.push(' ');
            }
            column += spaces;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

fn prev_boundary(text: &str, index: usize) -> Option<usize> {
    text[..index].char_indices().next_back().map(|(i, _)| i)
}

fn next_boundary(text: &str, index: usize) -> Option<usize> {
    text[index..].chars().next().map(|ch| index + ch.len_utf8())
}

fn byte_at_char(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn clamp_boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use libluna_render::Scene;

    fn theme() -> Theme {
        Theme::light()
    }

    #[test]
    fn insertion_and_line_breaks_keep_cursor_valid() {
        let mut view = CodeView::new();
        view.set_text("hello");
        view.set_cursor(5);
        view.insert_str("\nworld");
        assert_eq!(view.text(), "hello\nworld");
        assert_eq!(view.line_count(), 2);
        assert_eq!(view.line_col(), (2, 6));
        view.move_up(false);
        assert_eq!(view.cursor(), 5);
        view.move_down(false);
        assert_eq!(view.cursor(), 11);
    }

    #[test]
    fn selection_replaces_text_and_survives_unicode() {
        let mut view = CodeView::new();
        view.set_text("Zażółć gęślą jaźń");
        view.set_selection(0, "Zażółć".len());
        view.insert_str("Łódź");
        assert_eq!(view.text(), "Łódź gęślą jaźń");
        assert_eq!(view.cursor(), "Łódź".len());
        view.move_left(false);
        view.move_right(false);
        assert_eq!(view.line_col(), (1, 5));
    }

    #[test]
    fn shift_selection_moves_with_arrows() {
        let mut view = CodeView::new();
        view.set_text("abcdef");
        view.set_cursor(2);
        view.move_right(true);
        view.move_right(true);
        assert_eq!(view.selection(), Some((2, 4)));
        view.move_left(false);
        assert_eq!(view.cursor(), 2);
        assert!(view.selection().is_none());
    }

    #[test]
    fn hit_test_maps_clicks_to_text() {
        let mut view = CodeView::new();
        view.set_text("abc\ndef");
        let theme = theme();
        let text = TextEngine::none();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 200.0);
        let metrics = view.metrics(rect, &theme, &text);
        let point = Point::new(metrics.text_area.x() + 2.0, metrics.text_area.y() + 2.0);
        assert_eq!(view.index_at(rect, &theme, &text, point), 0);
        let second_line = Point::new(
            metrics.text_area.x() + 2.0,
            metrics.text_area.y() + metrics.line_height + 2.0,
        );
        assert_eq!(view.index_at(rect, &theme, &text, second_line), 4);
    }

    #[test]
    fn tabs_expand_and_paint_without_panicking() {
        let mut view = CodeView::new();
        view.set_text("a\tb");
        view.set_spans(vec![
            Span::new(0, 1, TokenKind::Keyword),
            Span::new(2, 3, TokenKind::String),
        ]);
        let theme = theme();
        let text = TextEngine::none();
        let mut scene = Scene::new();
        let mut ctx = PaintCtx::new(&mut scene, &theme, &text);
        view.paint(&mut ctx, Rect::from_xywh(0.0, 0.0, 500.0, 300.0));
        assert_eq!(expand_tabs("a\tb", 4), "a   b");
    }

    #[test]
    fn scrolling_bounds_follow_content() {
        let mut view = CodeView::new();
        view.set_text("line\n".repeat(200));
        let theme = theme();
        let text = TextEngine::none();
        let rect = Rect::from_xywh(0.0, 0.0, 400.0, 200.0);
        view.ensure_cursor_visible(rect, &theme, &text);
        assert_eq!(view.vertical_offset(), 0.0);
        view.document_end(false);
        view.ensure_cursor_visible(rect, &theme, &text);
        assert!(view.vertical_offset() > 0.0);
    }

    #[test]
    fn spans_outside_text_are_clipped() {
        let mut view = CodeView::new();
        view.set_text("abc");
        view.set_spans(vec![Span::new(1, 99, TokenKind::Keyword)]);
        assert_eq!(view.spans().len(), 1);
        assert_eq!(view.spans()[0].end, 3);
    }

    #[test]
    fn measurement_is_additive_for_monospace_runs() {
        let text = TextEngine::mono();
        let size = 14.0;
        let whole = text.measure("use std", size, weight::NORMAL).width;
        let parts = text.measure("use", size, weight::NORMAL).width
            + text.measure(" std", size, weight::NORMAL).width;
        assert!((whole - parts).abs() < 0.5, "{whole} vs {parts}");
    }

    #[test]
    fn size_is_stable_without_fonts() {
        let view = CodeView::new();
        let theme = theme();
        let text = TextEngine::none();
        let metrics = view.metrics(Rect::from_xywh(0.0, 0.0, 400.0, 200.0), &theme, &text);
        assert!(metrics.text_area.width() > 0.0);
        assert!(metrics.gutter.width() > 0.0);
        assert!(metrics.text_area.height() > 0.0);
    }
}
