//! Pola specjalne GTK4: `PasswordEntry` i `EditableLabel`.
//!
//! Oba są cienkimi nakładkami na [`Entry`]: cała logika edycji (kursor, wstawianie,
//! usuwanie) żyje w polu bazowym, a tutaj dodajemy tylko cechy charakterystyczne.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::entry::{Entry, EntryIcon, TextInput};
use crate::icon;

/// Pole hasła: [`Entry`] z maskowaniem (`•`) i przyciskiem podglądu („oko”).
#[derive(Debug, Clone, Default)]
pub struct PasswordEntry {
    entry: Entry,
    peek_hover: bool,
}

impl PasswordEntry {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            entry: Entry::new(placeholder).masked('•').leading(EntryIcon::Lock),
            peek_hover: false,
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

    /// Czy hasło jest chwilowo widoczne (podgląd).
    pub fn is_peek(&self) -> bool {
        self.entry.is_peek()
    }

    pub fn set_peek(&mut self, peek: bool) {
        self.entry.set_peek(peek);
    }

    /// Przełącza podgląd; zwraca nowy stan.
    pub fn toggle_peek(&mut self) -> bool {
        let v = !self.entry.is_peek();
        self.entry.set_peek(v);
        v
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

    pub fn copy(&self) -> Option<String> {
        self.entry.copy()
    }

    pub fn cut(&mut self) -> Option<String> {
        self.entry.cut()
    }

    pub fn paste(&mut self, s: &str) {
        self.entry.paste(s);
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        self.entry.height(theme)
    }

    pub fn contains(&self, rect: Rect, p: Point) -> bool {
        self.entry.contains(rect, p)
    }

    /// Prostokąt przycisku podglądu („oko”) po prawej stronie pola.
    pub fn peek_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let m = theme.metrics.entry;
        Rect::from_xywh(
            rect.right() - m.pad_x - m.icon,
            rect.center().y - m.icon / 2.0,
            m.icon,
            m.icon,
        )
    }

    pub fn peek_at(&self, rect: Rect, theme: &Theme, p: Point) -> bool {
        self.peek_rect(rect, theme).contains(p)
    }

    pub fn set_peek_hover(&mut self, hovered: bool) {
        self.peek_hover = hovered;
    }

    /// Obsługuje klik: trafienie w „oko” przełącza podgląd. Zwraca `true`, gdy zmieniono stan.
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        if self.peek_at(rect, theme, p) {
            self.toggle_peek();
            true
        } else {
            false
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let m = theme.metrics.entry;
        // Zarezerwuj miejsce na „oko”, by tekst nie wchodził pod ikonę.
        let mut entry = self.entry.clone();
        entry.set_trailing_space(m.icon + 8.0);
        entry.paint(ctx, rect);

        let eye = self.peek_rect(rect, &theme);
        if self.peek_hover {
            ctx.scene.fill_rounded_rect(
                eye,
                eye.width() / 2.0,
                theme.colors.shade.scale_alpha(1.8),
            );
        }
        let color = if self.entry.is_peek() {
            theme.colors.accent
        } else {
            theme.colors.dim_text
        };
        if self.entry.is_peek() {
            icon::eye(ctx, color, eye.center(), m.icon * 0.42);
        } else {
            icon::eye_off(ctx, color, eye.center(), m.icon * 0.42);
        }
    }
}

impl TextInput for PasswordEntry {
    fn insert_str(&mut self, s: &str) {
        PasswordEntry::insert_str(self, s);
    }
    fn insert_char(&mut self, c: char) {
        PasswordEntry::insert_char(self, c);
    }
    fn backspace(&mut self) {
        PasswordEntry::backspace(self);
    }
    fn delete_forward(&mut self) {
        PasswordEntry::delete_forward(self);
    }
    fn move_left(&mut self) {
        PasswordEntry::move_left(self);
    }
    fn move_right(&mut self) {
        PasswordEntry::move_right(self);
    }
    fn home(&mut self) {
        PasswordEntry::home(self);
    }
    fn end(&mut self) {
        PasswordEntry::end(self);
    }
    fn move_left_selecting(&mut self, shift: bool) {
        self.entry.move_left_selecting(shift);
    }
    fn move_right_selecting(&mut self, shift: bool) {
        self.entry.move_right_selecting(shift);
    }
    fn home_selecting(&mut self, shift: bool) {
        self.entry.home_selecting(shift);
    }
    fn end_selecting(&mut self, shift: bool) {
        self.entry.end_selecting(shift);
    }
    fn select_all(&mut self) {
        self.entry.select_all();
    }
    fn clear_selection(&mut self) {
        self.entry.clear_selection();
    }
    fn copy(&self) -> Option<String> {
        PasswordEntry::copy(self)
    }
    fn cut(&mut self) -> Option<String> {
        PasswordEntry::cut(self)
    }
    fn paste(&mut self, s: &str) {
        PasswordEntry::paste(self, s);
    }
}

/// Etykieta edytowalna: pokazuje tekst, a po wejściu w edycję zamienia się w [`Entry`].
#[derive(Debug, Clone, Default)]
pub struct EditableLabel {
    text: String,
    editing: bool,
    entry: Entry,
}

impl EditableLabel {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut entry = Entry::new("");
        entry.set_text(text.clone());
        Self {
            text,
            editing: false,
            entry,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.entry.set_text(self.text.clone());
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    /// Wchodzi w tryb edycji (kursor na końcu tekstu).
    pub fn start_editing(&mut self) {
        self.editing = true;
        self.entry.set_text(self.text.clone());
        self.entry.set_focused(true);
    }

    /// Zatwierdza edycję: tekst pola staje się tekstem etykiety.
    pub fn commit(&mut self) {
        if self.editing {
            self.text = self.entry.text().to_string();
            self.editing = false;
            self.entry.set_focused(false);
        }
    }

    /// Anuluje edycję (przywraca poprzedni tekst).
    pub fn cancel(&mut self) {
        if self.editing {
            self.editing = false;
            self.entry.set_text(self.text.clone());
            self.entry.set_focused(false);
        }
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.entry.height
    }

    /// Deleguje edycję do pola bazowego (gdy trwa edycja).
    pub fn entry_mut(&mut self) -> &mut Entry {
        &mut self.entry
    }

    pub fn insert_str(&mut self, s: &str) {
        if self.editing {
            self.entry.insert_str(s);
        }
    }

    pub fn backspace(&mut self) {
        if self.editing {
            self.entry.backspace();
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        if self.editing {
            let mut entry = self.entry.clone();
            entry.set_focused(true);
            entry.paint(ctx, rect);
        } else {
            ctx.label_left(
                rect.x(),
                rect.center().y,
                &self.text,
                theme.typography.body.size,
                weight::NORMAL,
                theme.colors.text,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 200.0, 34.0)
    }

    #[test]
    fn password_masks_but_keeps_text() {
        let theme = Theme::light();
        let mut p = PasswordEntry::new("Hasło");
        p.insert_str("abc123");
        assert_eq!(p.text(), "abc123");
        assert!(p.entry.is_masked());
        assert!(!p.is_peek());
        // Klik w „oko” włącza podgląd.
        let eye = p.peek_rect(rect(), &theme).center();
        assert!(p.click_at(rect(), &theme, eye));
        assert!(p.is_peek());
        assert!(p.click_at(rect(), &theme, eye));
        assert!(!p.is_peek());
        // Klik poza „okiem” nic nie zmienia.
        assert!(!p.click_at(rect(), &theme, Point::new(5.0, 5.0)));
    }

    #[test]
    fn password_editing_delegates() {
        let mut p = PasswordEntry::new("");
        p.insert_str("ząb");
        assert_eq!(p.text(), "ząb");
        p.backspace();
        assert_eq!(p.text(), "zą");
    }

    #[test]
    fn editable_label_commit_and_cancel() {
        let mut l = EditableLabel::new("Stara");
        assert_eq!(l.text(), "Stara");
        assert!(!l.is_editing());

        l.start_editing();
        assert!(l.is_editing());
        l.entry_mut().set_text("Nowa");
        l.commit();
        assert!(!l.is_editing());
        assert_eq!(l.text(), "Nowa");

        l.start_editing();
        l.entry_mut().set_text("Porzucona");
        l.cancel();
        assert_eq!(l.text(), "Nowa");
    }

    #[test]
    fn editable_label_ignores_typing_when_not_editing() {
        let mut l = EditableLabel::new("X");
        l.insert_str("Y");
        assert_eq!(l.text(), "X");
    }
}
