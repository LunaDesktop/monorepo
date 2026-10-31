//! `Entry` — pole tekstowe (jednowierszowe) z kursorem, opcjonalną ikoną wiodącą i czyszczeniem.
//!
//! Widget immediate: trzyma treść i pozycję kursora (indeks bajtowy, zawsze na granicy znaku).
//! Aplikacja dostarcza zdarzenia klawiatury jako wywołania metod ([`Entry::insert_str`],
//! [`Entry::backspace`], [`Entry::move_left`]…) i sama decyduje o fokusie ([`Entry::set_focused`]).
//!
//! [`SearchEntry`](crate::SearchEntry) to cienka nakładka: `Entry` z ikoną lupy i czyszczeniem.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::icon;

/// Ikona wiodąca pola (przed tekstem).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryIcon {
    /// Lupka (pola wyszukiwania).
    Search,
    /// Kłódka (pole hasła).
    Lock,
}

/// Pole tekstowe.
#[derive(Debug, Clone, Default)]
pub struct Entry {
    text: String,
    /// Pozycja kursora jako indeks bajtowy (zawsze na granicy znaku).
    cursor: usize,
    placeholder: String,
    leading: Option<EntryIcon>,
    clearable: bool,
    focused: bool,
    hover_clear: bool,
    /// Znak maskujący (np. `•`); `None` gdy tekst widoczny.
    mask: Option<char>,
    /// Podgląd hasła („oko”): pokazuje jawny tekst mimo maski.
    peek: bool,
    /// Miejsce zarezerwowane po prawej (np. na ikonę „oka”).
    trailing: f64,
    /// Kotwica zaznaczenia (bajt); `None` = brak zaznaczenia.
    anchor: Option<usize>,
}

impl Entry {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            ..Self::default()
        }
    }

    /// Dodaje ikonę wiodącą (np. lupkę).
    pub fn leading(mut self, icon: EntryIcon) -> Self {
        self.leading = Some(icon);
        self
    }

    /// Włącza przycisk czyszczenia („×”) widoczny, gdy pole niepuste.
    pub fn clearable(mut self, clearable: bool) -> Self {
        self.clearable = clearable;
        self
    }

    /// Maskuje tekst znakiem `mask` (np. `•`) — pola haseł.
    pub fn masked(mut self, mask: char) -> Self {
        self.mask = Some(mask);
        self
    }

    pub fn set_masked(&mut self, mask: Option<char>) {
        self.mask = mask;
    }

    pub fn is_masked(&self) -> bool {
        self.mask.is_some()
    }

    /// Pokazuje jawny tekst mimo maski (podgląd hasła).
    pub fn set_peek(&mut self, peek: bool) {
        self.peek = peek;
    }

    pub fn is_peek(&self) -> bool {
        self.peek
    }

    /// Rezerwuje miejsce po prawej stronie (np. na ikonę „oka”).
    pub fn set_trailing_space(&mut self, space: f64) {
        self.trailing = space.max(0.0);
    }

    pub fn trailing_space(&self) -> f64 {
        self.trailing
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

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.anchor = None;
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<String>) {
        self.placeholder = placeholder.into();
    }

    pub fn set_hover_clear(&mut self, hovered: bool) {
        self.hover_clear = hovered;
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

    /// Zaznaczony tekst (albo `None`).
    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|(a, b)| &self.text[a..b])
    }

    /// Ustawia zaznaczenie `start..end` (przycinane do granic znaku); kursor na `end`.
    pub fn set_selection(&mut self, start: usize, end: usize) {
        self.anchor = Some(clamp_boundary(&self.text, start));
        self.cursor = clamp_boundary(&self.text, end);
    }

    /// Zaznacza cały tekst.
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

    /// Wycina zaznaczenie (usuwa je i zwraca).
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

    /// Wstawia tekst w miejscu kursora (zastępuje zaznaczenie).
    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection();
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    /// Usuwa znak przed kursorem (albo zaznaczenie).
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

    /// Usuwa znak za kursorem (albo zaznaczenie).
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

    /// Ruch kursora w lewo; z `shift` rozciąga zaznaczenie.
    pub fn move_left_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = prev_boundary(&self.text, self.cursor);
    }

    /// Ruch kursora w prawo; z `shift` rozciąga zaznaczenie.
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
        self.cursor = 0;
    }

    pub fn end_selecting(&mut self, shift: bool) {
        self.begin_or_clear(shift);
        self.cursor = self.text.len();
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.entry.height
    }

    pub fn contains(&self, rect: Rect, p: Point) -> bool {
        rect.contains(p)
    }

    /// Prostokąt przycisku czyszczenia („×”).
    pub fn clear_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let m = theme.metrics.entry;
        Rect::from_xywh(
            rect.right() - m.pad_x - m.icon - self.trailing,
            rect.center().y - m.icon / 2.0,
            m.icon,
            m.icon,
        )
    }

    /// Czy punkt trafia w przycisk czyszczenia (tylko gdy pole czyszczalne i niepuste).
    pub fn clear_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.clearable && !self.text.is_empty() && self.clear_rect(rect, theme).contains(p)
    }

    /// Tekst do wyświetlenia (z uwzględnieniem maski hasła i podglądu).
    fn display_text(&self) -> String {
        match (self.mask, self.peek) {
            (Some(ch), false) => ch.to_string().repeat(self.text.chars().count()),
            _ => self.text.clone(),
        }
    }

    /// Prefiks tekstu (w wersji wyświetlanej) do granicy `index` — z uwzględnieniem maski.
    fn display_prefix_at(&self, index: usize) -> String {
        let upto = &self.text[..index];
        match (self.mask, self.peek) {
            (Some(ch), false) => ch.to_string().repeat(upto.chars().count()),
            _ => upto.to_string(),
        }
    }

    /// Prefiks tekstu do kursora w wersji wyświetlanej (do wyliczenia pozycji kursora).
    fn display_prefix(&self) -> String {
        self.display_prefix_at(self.cursor)
    }

    /// Pozycja (x), od której zaczyna się tekst (za ikoną wiodącą, jeśli jest).
    fn text_x(&self, rect: Rect, theme: &Theme) -> f64 {
        let m = theme.metrics.entry;
        match self.leading {
            Some(_) => rect.x() + m.pad_x + m.icon + 8.0,
            None => rect.x() + m.pad_x,
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.entry;
        let colors = theme.colors;
        let radius = theme.radius.medium;

        ctx.scene.fill_rounded_rect(rect, radius, colors.shade);
        let (border, width) = if self.focused {
            (colors.accent, 2.0)
        } else {
            (colors.border, 1.0)
        };
        ctx.scene.stroke_rounded_rect(rect, radius, width, border);

        let cy = rect.center().y;
        if let Some(icon_kind) = self.leading {
            match icon_kind {
                EntryIcon::Search => icon::search(
                    ctx,
                    if self.focused {
                        colors.accent
                    } else {
                        colors.dim_text
                    },
                    Point::new(rect.x() + m.pad_x + m.icon / 2.0, cy),
                    m.icon,
                ),
                EntryIcon::Lock => icon::lock(
                    ctx,
                    colors.dim_text,
                    Point::new(rect.x() + m.pad_x + m.icon / 2.0, cy),
                    m.icon,
                ),
            }
        }
        let text_x = self.text_x(rect, &theme);

        if self.text.is_empty() {
            ctx.label_left(
                text_x,
                cy,
                &self.placeholder,
                theme.typography.body.size,
                weight::NORMAL,
                colors.dim_text,
            );
        } else {
            let clear_space = if self.clearable { m.icon + 8.0 } else { 0.0 };
            let text_right = rect.right() - m.pad_x - clear_space - self.trailing;
            ctx.scene.push_clip(Rect::from_xywh(
                text_x,
                rect.y(),
                (text_right - text_x).max(0.0),
                rect.height(),
            ));
            // Podświetlenie zaznaczenia (pod tekstem).
            if let Some((a, b)) = self.selection() {
                let size = theme.typography.body.size;
                let x1 = text_x + ctx.measure(&self.display_prefix_at(a), size, weight::NORMAL);
                let x2 = text_x + ctx.measure(&self.display_prefix_at(b), size, weight::NORMAL);
                ctx.scene.fill_rect(
                    Rect::from_xywh(
                        x1,
                        rect.y() + 5.0,
                        (x2 - x1).max(0.0),
                        (rect.height() - 10.0).max(0.0),
                    ),
                    colors.accent.scale_alpha(0.30),
                );
            }
            let display = self.display_text();
            ctx.label_left(
                text_x,
                cy,
                &display,
                theme.typography.body.size,
                weight::NORMAL,
                colors.text,
            );
            if self.focused {
                let prefix = self.display_prefix();
                let cx = text_x + ctx.measure(&prefix, theme.typography.body.size, weight::NORMAL);
                ctx.scene.fill_rect(
                    Rect::from_xywh(cx, rect.y() + 6.0, 1.5, (rect.height() - 12.0).max(0.0)),
                    colors.text,
                );
            }
            ctx.scene.pop_clip();
        }

        if self.clearable && !self.text.is_empty() {
            let cb = self.clear_rect(rect, &theme);
            if self.hover_clear {
                ctx.scene
                    .fill_rounded_rect(cb, cb.width() / 2.0, colors.shade.scale_alpha(1.8));
            }
            icon::close(ctx, colors.dim_text, cb, 4.0);
        }
    }
}

/// Wspólny interfejs edycji tekstu dla pól ([`Entry`], [`SearchEntry`](crate::SearchEntry)).
///
/// Pozwala aplikacji obsłużyć zdarzenia klawiatury jedną funkcją, niezależnie od tego,
/// czy pole jest „gołym” `Entry`, czy wyspecjalizowanym wariantem.
pub trait TextInput {
    fn insert_str(&mut self, s: &str);
    fn insert_char(&mut self, c: char);
    fn backspace(&mut self);
    fn delete_forward(&mut self);
    fn move_left(&mut self);
    fn move_right(&mut self);
    fn home(&mut self);
    fn end(&mut self);
    /// Ruch w lewo; z `shift` rozciąga zaznaczenie.
    fn move_left_selecting(&mut self, shift: bool);
    /// Ruch w prawo; z `shift` rozciąga zaznaczenie.
    fn move_right_selecting(&mut self, shift: bool);
    fn home_selecting(&mut self, shift: bool);
    fn end_selecting(&mut self, shift: bool);
    /// Zaznacza cały tekst.
    fn select_all(&mut self);
    /// Usuwa zaznaczenie (bez zmiany tekstu).
    fn clear_selection(&mut self);
    /// Zwraca zaznaczony tekst (do systemowego schowka).
    fn copy(&self) -> Option<String>;
    /// Wycina zaznaczenie (usuwa je i zwraca).
    fn cut(&mut self) -> Option<String>;
    /// Wkleja tekst w miejsce kursora (zastępuje zaznaczenie).
    fn paste(&mut self, s: &str);
}

impl TextInput for Entry {
    fn insert_str(&mut self, s: &str) {
        Entry::insert_str(self, s);
    }
    fn insert_char(&mut self, c: char) {
        Entry::insert_char(self, c);
    }
    fn backspace(&mut self) {
        Entry::backspace(self);
    }
    fn delete_forward(&mut self) {
        Entry::delete_forward(self);
    }
    fn move_left(&mut self) {
        Entry::move_left(self);
    }
    fn move_right(&mut self) {
        Entry::move_right(self);
    }
    fn home(&mut self) {
        Entry::home(self);
    }
    fn end(&mut self) {
        Entry::end(self);
    }
    fn move_left_selecting(&mut self, shift: bool) {
        Entry::move_left_selecting(self, shift);
    }
    fn move_right_selecting(&mut self, shift: bool) {
        Entry::move_right_selecting(self, shift);
    }
    fn home_selecting(&mut self, shift: bool) {
        Entry::home_selecting(self, shift);
    }
    fn end_selecting(&mut self, shift: bool) {
        Entry::end_selecting(self, shift);
    }
    fn select_all(&mut self) {
        Entry::select_all(self);
    }
    fn clear_selection(&mut self) {
        Entry::clear_selection(self);
    }
    fn copy(&self) -> Option<String> {
        Entry::copy(self)
    }
    fn cut(&mut self) -> Option<String> {
        Entry::cut(self)
    }
    fn paste(&mut self, s: &str) {
        Entry::paste(self, s);
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

/// Przybliża indeks do najbliższej granicy znaku (w dół) i do długości napisu.
fn clamp_boundary(s: &str, i: usize) -> usize {
    let mut b = i.min(s.len());
    while b > 0 && !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_cursor() {
        let mut e = Entry::new("Nazwa");
        e.insert_str("abc");
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 3);
        e.move_left();
        e.insert_char('X');
        assert_eq!(e.text(), "abXc");
        e.backspace();
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 2);
        e.delete_forward();
        assert_eq!(e.text(), "ab");
        e.home();
        e.delete_forward();
        assert_eq!(e.text(), "b");
    }

    #[test]
    fn multibyte_is_char_aware() {
        let mut e = Entry::new("");
        e.insert_str("ząb");
        e.backspace();
        assert_eq!(e.text(), "zą");
        e.move_left();
        e.backspace();
        assert_eq!(e.text(), "ą");
    }

    #[test]
    fn clear_resets_state() {
        let mut e = Entry::new("");
        e.insert_str("x");
        e.clear();
        assert!(e.is_empty());
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn clear_hit_requires_clearable_and_text() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 200.0, 34.0);
        let mut plain = Entry::new("");
        plain.insert_str("abc");
        let c = plain.clear_rect(rect, &theme).center();
        // Pole nieczyszczalne: brak trafienia nawet z tekstem.
        assert!(!plain.clear_at(rect, &theme, c));

        let mut e = Entry::new("").clearable(true);
        assert!(!e.clear_at(rect, &theme, c));
        e.insert_str("abc");
        assert!(e.clear_at(rect, &theme, c));
    }

    #[test]
    fn height_from_metrics() {
        let theme = Theme::light();
        assert_eq!(Entry::new("").height(&theme), theme.metrics.entry.height);
    }

    #[test]
    fn selection_copy_cut_paste() {
        let mut e = Entry::new("");
        e.set_text("hello world");
        e.set_selection(0, 5);
        assert_eq!(e.selected_text(), Some("hello"));
        assert_eq!(e.copy(), Some("hello".to_owned()));
        // Wklejenie zastępuje zaznaczenie.
        e.paste("HELLO");
        assert_eq!(e.text(), "HELLO world");
        assert!(!e.has_selection());
        // Wycinanie.
        e.set_selection(0, 6);
        assert_eq!(e.cut(), Some("HELLO ".to_owned()));
        assert_eq!(e.text(), "world");
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn shift_extends_selection_and_typing_replaces() {
        let mut e = Entry::new("");
        e.set_text("abc");
        e.home();
        e.move_right_selecting(true);
        e.move_right_selecting(true);
        assert_eq!(e.selected_text(), Some("ab"));
        e.insert_char('X');
        assert_eq!(e.text(), "Xc");
    }

    #[test]
    fn backspace_deletes_selection() {
        let mut e = Entry::new("");
        e.set_text("abcdef");
        e.set_selection(1, 4);
        e.backspace();
        assert_eq!(e.text(), "aef");
        assert_eq!(e.cursor(), 1);
    }

    #[test]
    fn select_all_clamps_to_char_boundary() {
        let mut e = Entry::new("");
        e.set_text("jąbko");
        e.select_all();
        assert_eq!(e.selected_text(), Some("jąbko"));
        e.set_selection(2, 2); // środek znaku → przycięte
        assert!(!e.has_selection());
        e.clear_selection();
        assert_eq!(e.selection(), None);
    }
}
