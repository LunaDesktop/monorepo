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
    /// Kotwica zaznaczenia (bajt); `None` = brak zaznaczenia.
    anchor: Option<usize>,
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
        self.anchor = None;
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
        self.anchor = None;
    }

    /// Zakres zaznaczenia `(start, end)` w bajtach (albo `None`).
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.cursor {
            None
        } else {
            Some((a.min(self.cursor), a.max(self.cursor)))
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(a, b)| &self.text[a..b])
    }

    /// Ustawia zaznaczenie `start..end` (przycinane do granic znaku).
    pub fn set_selection(&mut self, start: usize, end: usize) {
        self.anchor = Some(clamp_boundary(&self.text, start));
        self.cursor = clamp_boundary(&self.text, end);
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Zwraca zaznaczenie do schowka aplikacji.
    pub fn copy(&self) -> Option<String> {
        self.selected_text().map(str::to_owned)
    }

    /// Wycina zaznaczenie.
    pub fn cut(&mut self) -> Option<String> {
        let s = self.copy()?;
        self.delete_selection();
        Some(s)
    }

    /// Wkleja tekst (zastępuje zaznaczenie).
    pub fn paste(&mut self, s: &str) {
        self.insert_str(s);
    }

    fn delete_selection(&mut self) {
        if let Some((a, b)) = self.selection() {
            self.text.replace_range(a..b, "");
            self.cursor = a;
        }
        self.anchor = None;
    }

    fn begin_or_clear(&mut self, shift: bool) {
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection();
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
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor > 0 {
            let prev = prev_boundary(&self.text, self.cursor);
            self.text.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    pub fn delete_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
            return;
        }
        if self.cursor < self.text.len() {
            let next = next_boundary(&self.text, self.cursor);
            self.text.replace_range(self.cursor..next, "");
        }
    }

    pub fn move_left(&mut self) {
        self.move_left_selecting(false);
    }

    pub fn move_right(&mut self) {
        self.move_right_selecting(false);
    }

    pub fn move_left_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    pub fn move_right_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = next_boundary(&self.text, self.cursor);
    }

    pub fn home(&mut self) {
        self.home_selecting(false);
    }

    pub fn end(&mut self) {
        self.end_selecting(false);
    }

    pub fn home_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = self.line_start(self.cursor_line());
    }

    pub fn end_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = self.line_end(self.cursor_line());
    }

    /// Przechodzi wiersz w górę, zachowując kolumnę (o ile się mieści).
    pub fn move_up(&mut self) {
        self.move_up_selecting(false);
    }

    /// Przechodzi wiersz w dół, zachowując kolumnę (o ile się mieści).
    pub fn move_down(&mut self) {
        self.move_down_selecting(false);
    }

    pub fn move_up_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
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

    pub fn move_down_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
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
            // Podświetlenie zaznaczenia (per wiersz).
            if let Some((a, b)) = self.selection() {
                let starts = self.line_starts();
                for (i, &start) in starts.iter().enumerate() {
                    let end = if i + 1 < starts.len() {
                        starts[i + 1] - 1
                    } else {
                        self.text.len()
                    };
                    let lo = a.max(start);
                    let hi = b.min(end);
                    if lo < hi {
                        let y = inner.y() + i as f64 * lh;
                        let x1 =
                            inner.x() + ctx.measure(&self.text[start..lo], body, weight::NORMAL);
                        let x2 =
                            inner.x() + ctx.measure(&self.text[start..hi], body, weight::NORMAL);
                        ctx.scene.fill_rect(
                            Rect::from_xywh(x1, y + 2.0, (x2 - x1).max(0.0), (lh - 4.0).max(0.0)),
                            colors.accent.scale_alpha(0.30),
                        );
                    }
                }
            }
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

/// Przybliża indeks do najbliższej granicy znaku (w dół) i do długości napisu.
fn clamp_boundary(s: &str, i: usize) -> usize {
    let mut b = i.min(s.len());
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

impl crate::entry::TextInput for TextView {
    fn insert_str(&mut self, s: &str) {
        TextView::insert_str(self, s);
    }
    fn insert_char(&mut self, c: char) {
        TextView::insert_char(self, c);
    }
    fn backspace(&mut self) {
        TextView::backspace(self);
    }
    fn delete_forward(&mut self) {
        TextView::delete_forward(self);
    }
    fn move_left(&mut self) {
        TextView::move_left(self);
    }
    fn move_right(&mut self) {
        TextView::move_right(self);
    }
    fn home(&mut self) {
        TextView::home(self);
    }
    fn end(&mut self) {
        TextView::end(self);
    }
    fn move_left_selecting(&mut self, shift: bool) {
        TextView::move_left_selecting(self, shift);
    }
    fn move_right_selecting(&mut self, shift: bool) {
        TextView::move_right_selecting(self, shift);
    }
    fn home_selecting(&mut self, shift: bool) {
        TextView::home_selecting(self, shift);
    }
    fn end_selecting(&mut self, shift: bool) {
        TextView::end_selecting(self, shift);
    }
    fn select_all(&mut self) {
        TextView::select_all(self);
    }
    fn clear_selection(&mut self) {
        TextView::clear_selection(self);
    }
    fn copy(&self) -> Option<String> {
        TextView::copy(self)
    }
    fn cut(&mut self) -> Option<String> {
        TextView::cut(self)
    }
    fn paste(&mut self, s: &str) {
        TextView::paste(self, s);
    }
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

    #[test]
    fn multiline_selection_and_editing() {
        let mut t = TextView::new("");
        t.set_text("one\ntwo\nthree");
        // Zaznacz od końca "one" do początku "three".
        t.set_selection(3, 8);
        assert_eq!(t.selected_text(), Some("\ntwo\n"));
        assert_eq!(t.copy(), Some("\ntwo\n".to_owned()));
        t.paste(" | ");
        assert_eq!(t.text(), "one | three");
        assert!(!t.has_selection());
    }

    #[test]
    fn shift_selects_by_lines() {
        let mut t = TextView::new("");
        t.set_text("ab\ncd");
        t.home();
        t.move_up_selecting(true); // brak wiersza wyżej → kursor na 0, kotwica wcześniej
        t.move_right_selecting(true);
        assert!(t.has_selection());
    }

    #[test]
    fn backspace_removes_selection() {
        let mut t = TextView::new("");
        t.set_text("abcdef");
        t.set_selection(1, 4);
        t.backspace();
        assert_eq!(t.text(), "aef");
        assert_eq!(t.cursor(), 1);
    }
}
