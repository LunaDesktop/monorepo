//! `TextView` — wielolinijkowe pole tekstowe (GTK4).
//!
//! Widget immediate: trzyma tekst (z `\n`) i indeks kursora na granicy znaku. Aplikacja
//! dostarcza zdarzenia klawiatury wywołaniami metod (`insert_str`, `backspace`, `move_up`…)
//! i sama decyduje o fokusie. Rysowanie nie zawija wierszy (brak mięk korekty) — to warstwa
//! aplikacji, gdy będzie potrzebna.
//!
//! [`Entry`](crate::Entry) to jednowierszowy odpowiednik.

use libluna_core::style::weight;
use libluna_core::{Rect, Theme};

use crate::ctx::PaintCtx;

/// Wielolinijkowe pole tekstowe.
#[derive(Debug, Clone, Default)]
pub struct TextView {
    text: String,
    /// Indeks bajtowy kursora (zawsze na granicy znaku).
    cursor: usize,
    placeholder: String,
    focused: bool,
}

impl TextView {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn line_count(&self) -> usize {
        self.line_starts().len()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.placeholder = placeholder.into();
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    /// Wstawia znak nowej linii w miejscu kursora.
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = prev_boundary(&self.text, self.cursor);
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.text.len() {
            let next = next_boundary(&self.text, self.cursor);
            self.text.replace_range(self.cursor..next, "");
        }
    }

    pub fn move_left(&mut self) {
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.cursor = next_boundary(&self.text, self.cursor);
    }

    pub fn home(&mut self) {
        self.cursor = self.line_start(self.cursor_line());
    }

    pub fn end(&mut self) {
        self.cursor = self.line_end(self.cursor_line());
    }

    /// Przechodzi wiersz w górę, zachowując kolumnę (o ile się mieści).
    pub fn move_up(&mut self) {
        let line = self.cursor_line();
        if line == 0 {
            self.cursor = 0;
            return;
        }
        let col = self.cursor_col(line);
        let start = self.line_start(line - 1);
        let end = self.line_end(line - 1);
        self.cursor = start + byte_at_char(&self.text[start..end], col);
    }

    /// Przechodzi wiersz w dół, zachowując kolumnę (o ile się mieści).
    pub fn move_down(&mut self) {
        let line = self.cursor_line();
        if line + 1 >= self.line_count() {
            self.cursor = self.text.len();
            return;
        }
        let col = self.cursor_col(line);
        let start = self.line_start(line + 1);
        let end = self.line_end(line + 1);
        self.cursor = start + byte_at_char(&self.text[start..end], col);
    }

    /// Wysokość treści (wszystkie wiersze).
    pub fn content_height(&self, theme: &Theme) -> f64 {
        self.line_count() as f64 * theme.typography.body.line_height
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.entry.height.max(self.content_height(theme))
    }

    /// Iteruje po wierszach tekstu.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.split('\n')
    }

    fn line_starts(&self) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, b) in self.text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    fn cursor_line(&self) -> usize {
        let starts = self.line_starts();
        starts
            .partition_point(|&s| s <= self.cursor)
            .saturating_sub(1)
    }

    fn line_start(&self, line: usize) -> usize {
        *self.line_starts().get(line).unwrap_or(&self.text.len())
    }

    fn line_end(&self, line: usize) -> usize {
        let starts = self.line_starts();
        match starts.get(line + 1) {
            Some(&next) => next - 1, // bez znaku '\n'
            None => self.text.len(),
        }
    }

    /// Kolumna kursora w danym wierszu (licząc znaki).
    fn cursor_col(&self, line: usize) -> usize {
        let start = self.line_start(line);
        self.text[start..self.cursor].chars().count()
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let radius = theme.radius.medium;
        let colors = theme.colors;
        ctx.scene.fill_rounded_rect(rect, radius, colors.shade);
        let (border, width) = if self.focused {
            (colors.accent, 2.0)
        } else {
            (colors.border, 1.0)
        };
        ctx.scene.stroke_rounded_rect(rect, radius, width, border);

        let pad = theme.spacing.sm;
        let inner = Rect::from_xywh(
            rect.x() + pad,
            rect.y() + pad,
            (rect.width() - 2.0 * pad).max(0.0),
            (rect.height() - 2.0 * pad).max(0.0),
        );
        let body = theme.typography.body.size;
        let lh = theme.typography.body.line_height;

        ctx.scene.push_clip(inner);
        if self.text.is_empty() {
            ctx.label_left(
                inner.x(),
                inner.y() + lh / 2.0,
                &self.placeholder,
                body,
                weight::NORMAL,
                colors.dim_text,
            );
        } else {
            for (i, line) in self.lines().enumerate() {
                let y = inner.y() + i as f64 * lh + lh / 2.0;
                if y - lh / 2.0 > inner.bottom() {
                    break;
                }
                ctx.label_left(inner.x(), y, line, body, weight::NORMAL, colors.text);
            }
            if self.focused {
                let line = self.cursor_line();
                let start = self.line_start(line);
                let col_x = ctx.measure(&self.text[start..self.cursor], body, weight::NORMAL);
                let cx = inner.x() + col_x;
                let cy = inner.y() + line as f64 * lh;
                ctx.scene.fill_rect(
                    Rect::from_xywh(cx, cy + 2.0, 1.5, (lh - 4.0).max(0.0)),
                    colors.text,
                );
            }
        }
        ctx.scene.pop_clip();
    }
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i]
        .char_indices()
        .next_back()
        .map(|(j, _)| j)
        .unwrap_or(0)
}

fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(i)
}

/// Indeks bajtowy `col`-tego znaku w napisie (lub długość, gdy za krótki).
fn byte_at_char(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newline_and_line_count() {
        let mut t = TextView::new("");
        t.insert_str("ab\ncd");
        assert_eq!(t.line_count(), 2);
        assert_eq!(t.text(), "ab\ncd");
        assert_eq!(t.cursor(), 5);
    }

    #[test]
    fn backspace_removes_newline() {
        let mut t = TextView::new("");
        t.insert_str("a\nb");
        t.move_left(); // przed 'b'
        t.backspace(); // usuwa '\n'
        assert_eq!(t.text(), "ab");
    }

    #[test]
    fn vertical_navigation_keeps_column() {
        let mut t = TextView::new("");
        t.set_text("hello\nhi\nworld");
        t.home(); // początek ostatniego wiersza ("world")
        t.move_up(); // wiersz "hi", kolumna 0
        t.move_right();
        t.move_right(); // kolumna 2 w "hi" (koniec wiersza)
        assert_eq!(t.cursor(), "hello\nhi".len());
        t.move_down(); // wiersz "world", kolumna 2
        assert_eq!(t.cursor(), "hello\nhi\nwo".len());
    }

    #[test]
    fn home_and_end_stay_in_line() {
        let mut t = TextView::new("");
        t.set_text("one\ntwo\nthree");
        t.end();
        assert_eq!(t.cursor(), t.text().len());
        t.home();
        assert_eq!(t.cursor(), "one\ntwo\n".len());
        t.move_up();
        assert_eq!(t.cursor(), "one\n".len());
        t.home();
        assert_eq!(t.cursor(), "one\n".len()); // już na początku wiersza
        t.move_up();
        assert_eq!(t.cursor(), 0);
        t.move_up(); // brak wiersza wyżej
        assert_eq!(t.cursor(), 0);
        t.end(); // koniec wiersza "one"
        assert_eq!(t.cursor(), "one".len());
    }

    #[test]
    fn placeholder_and_content_height() {
        let theme = Theme::light();
        let mut t = TextView::new("Notatka…");
        assert_eq!(t.line_count(), 1);
        assert!(t.content_height(&theme) > 0.0);
        t.insert_str("a\nb\nc");
        assert_eq!(
            t.content_height(&theme),
            3.0 * theme.typography.body.line_height
        );
    }
}
