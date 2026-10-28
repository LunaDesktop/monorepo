//! `SearchEntry` — pole wyszukiwania (lupka, tekst, kursor, „×” czyszczenia).
//!
//! Cienka nakładka na [`Entry`]: pole tekstowe z ikoną lupy i przyciskiem czyszczenia.
//! Cała logika edycji żyje w [`Entry`]; tutaj delegujemy, zachowując dawny interfejs.

use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::entry::{Entry, EntryIcon};

/// Pole wyszukiwania.
#[derive(Debug, Clone, Default)]
pub struct SearchEntry {
    entry: Entry,
}

impl SearchEntry {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            entry: Entry::new(placeholder)
                .leading(EntryIcon::Search)
                .clearable(true),
        }
    }

    pub fn text(&self) -> &str {
        self.entry.text()
    }

    pub fn is_empty(&self) -> bool {
        self.entry.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.entry.cursor()
    }

    pub fn is_focused(&self) -> bool {
        self.entry.is_focused()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.entry.set_text(text);
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.entry.set_focused(focused);
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.entry.set_placeholder(placeholder);
    }

    pub fn set_hover_clear(&mut self, hovered: bool) {
        self.entry.set_hover_clear(hovered);
    }

    pub fn clear(&mut self) {
        self.entry.clear();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.entry.insert_str(s);
    }

    pub fn insert_char(&mut self, c: char) {
        self.entry.insert_char(c);
    }

    pub fn backspace(&mut self) {
        self.entry.backspace();
    }

    pub fn delete_forward(&mut self) {
        self.entry.delete_forward();
    }

    pub fn move_left(&mut self) {
        self.entry.move_left();
    }

    pub fn move_right(&mut self) {
        self.entry.move_right();
    }

    pub fn home(&mut self) {
        self.entry.home();
    }

    pub fn end(&mut self) {
        self.entry.end();
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        self.entry.height(theme)
    }

    pub fn contains(&self, rect: Rect, p: Point) -> bool {
        self.entry.contains(rect, p)
    }

    pub fn clear_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        self.entry.clear_rect(rect, theme)
    }

    pub fn clear_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.entry.clear_at(rect, theme, p)
    }

    /// Zakres zaznaczenia `(start, end)` w bajtach (albo `None`).
    pub fn selection(&self) -> Option<(usize, usize)> {
        self.entry.selection()
    }

    pub fn has_selection(&self) -> bool {
        self.entry.has_selection()
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.entry.selected_text()
    }

    pub fn set_selection(&mut self, start: usize, end: usize) {
        self.entry.set_selection(start, end);
    }

    pub fn select_all(&mut self) {
        self.entry.select_all();
    }

    pub fn clear_selection(&mut self) {
        self.entry.clear_selection();
    }

    /// Zwraca zaznaczenie do schowka aplikacji.
    pub fn copy(&self) -> Option<String> {
        self.entry.copy()
    }

    /// Wycina zaznaczenie.
    pub fn cut(&mut self) -> Option<String> {
        self.entry.cut()
    }

    /// Wkleja tekst (zastępuje zaznaczenie).
    pub fn paste(&mut self, s: &str) {
        self.entry.paste(s);
    }

    pub fn move_left_selecting(&mut self, shift: bool) {
        self.entry.move_left_selecting(shift);
    }

    pub fn move_right_selecting(&mut self, shift: bool) {
        self.entry.move_right_selecting(shift);
    }

    pub fn home_selecting(&mut self, shift: bool) {
        self.entry.home_selecting(shift);
    }

    pub fn end_selecting(&mut self, shift: bool) {
        self.entry.end_selecting(shift);
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        self.entry.paint(ctx, rect);
    }
}

impl crate::entry::TextInput for SearchEntry {
    fn insert_str(&mut self, s: &str) {
        SearchEntry::insert_str(self, s);
    }
    fn insert_char(&mut self, c: char) {
        SearchEntry::insert_char(self, c);
    }
    fn backspace(&mut self) {
        SearchEntry::backspace(self);
    }
    fn delete_forward(&mut self) {
        SearchEntry::delete_forward(self);
    }
    fn move_left(&mut self) {
        SearchEntry::move_left(self);
    }
    fn move_right(&mut self) {
        SearchEntry::move_right(self);
    }
    fn home(&mut self) {
        SearchEntry::home(self);
    }
    fn end(&mut self) {
        SearchEntry::end(self);
    }
    fn move_left_selecting(&mut self, shift: bool) {
        SearchEntry::move_left_selecting(self, shift);
    }
    fn move_right_selecting(&mut self, shift: bool) {
        SearchEntry::move_right_selecting(self, shift);
    }
    fn home_selecting(&mut self, shift: bool) {
        SearchEntry::home_selecting(self, shift);
    }
    fn end_selecting(&mut self, shift: bool) {
        SearchEntry::end_selecting(self, shift);
    }
    fn select_all(&mut self) {
        SearchEntry::select_all(self);
    }
    fn clear_selection(&mut self) {
        SearchEntry::clear_selection(self);
    }
    fn copy(&self) -> Option<String> {
        SearchEntry::copy(self)
    }
    fn cut(&mut self) -> Option<String> {
        SearchEntry::cut(self)
    }
    fn paste(&mut self, s: &str) {
        SearchEntry::paste(self, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_cursor() {
        let mut e = SearchEntry::new("Szukaj");
        e.insert_str("abc");
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 3);
        e.move_left();
        e.insert_char('X');
        assert_eq!(e.text(), "abXc");
        e.backspace();
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 2);
        e.delete_forward(); // usuwa 'c' za kursorem
        assert_eq!(e.text(), "ab");
        e.home();
        e.delete_forward(); // usuwa 'a'
        assert_eq!(e.text(), "b");
    }

    #[test]
    fn multibyte_is_char_aware() {
        let mut e = SearchEntry::new("");
        e.insert_str("ząb");
        assert_eq!(e.text(), "ząb");
        e.backspace();
        assert_eq!(e.text(), "zą");
        e.move_left();
        e.backspace();
        assert_eq!(e.text(), "ą");
    }

    #[test]
    fn clear_resets_state() {
        let mut e = SearchEntry::new("");
        e.insert_str("x");
        e.clear();
        assert!(e.is_empty());
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn clear_hit_only_when_non_empty() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 34.0);
        let mut e = SearchEntry::new("");
        let c = e.clear_rect(rect, &theme).center();
        assert!(!e.clear_at(rect, &theme, c));
        e.insert_str("abc");
        assert!(e.clear_at(rect, &theme, c));
    }

    #[test]
    fn height_from_metrics() {
        let theme = Theme::light();
        assert_eq!(
            SearchEntry::new("").height(&theme),
            theme.metrics.entry.height
        );
    }
}
