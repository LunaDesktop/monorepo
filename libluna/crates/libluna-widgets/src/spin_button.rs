//! `SpinButton` — pole liczbowe z edycją tekstową i przyciskami `▲`/`▼` (GTK4, w duchu libLuny).
//!
//! Standalone wariant liczbowy: tekst edytuje [`Entry`], a strzałki po prawej zwiększają/
//! zmniejszają wartość o krok. Wartość zmienia się dopiero po zatwierdzeniu edycji
//! ([`SpinButton::commit`], np. na `Enter` albo utratę fokusu).

use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;
use crate::entry::{Entry, TextInput};
use crate::icon;

/// Strzałka pola liczbowego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinArrow {
    Up,
    Down,
}

/// Pole liczbowe.
#[derive(Debug, Clone, Default)]
pub struct SpinButton {
    entry: Entry,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    digits: usize,
    arrows_hover: Option<SpinArrow>,
}

impl SpinButton {
    /// Pole z wartością w zakresie `min..max`, krokiem 1 i bez miejsc po przecinku.
    pub fn new(value: f64, min: f64, max: f64) -> Self {
        let value = value.clamp(min, max);
        let mut entry = Entry::new("");
        entry.set_text(format_number(value, 0));
        Self {
            entry,
            value,
            min,
            max,
            step: 1.0,
            digits: 0,
            arrows_hover: None,
        }
    }

    pub fn step(mut self, step: f64) -> Self {
        self.step = step.abs().max(f64::EPSILON);
        self
    }

    /// Liczba miejsc po przecinku (formatowanie wartości).
    pub fn digits(mut self, digits: usize) -> Self {
        self.digits = digits;
        self.entry.set_text(format_number(self.value, digits));
        self
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn min(&self) -> f64 {
        self.min
    }

    pub fn max(&self) -> f64 {
        self.max
    }

    pub fn is_focused(&self) -> bool {
        self.entry.is_focused()
    }

    pub fn set_focused(&mut self, focused: bool) {
        self.entry.set_focused(focused);
    }

    /// Ustawia wartość (przyciętą do zakresu) i Odświeża tekst.
    pub fn set_value(&mut self, value: f64) {
        self.value = value.clamp(self.min, self.max);
        self.entry.set_text(format_number(self.value, self.digits));
    }

    pub fn inc(&mut self) {
        self.set_value(self.value + self.step);
    }

    pub fn dec(&mut self) {
        self.set_value(self.value - self.step);
    }

    /// Zatwierdza edycję tekstową: parsuje wpis i przycina do zakresu.
    pub fn commit(&mut self) {
        match self.entry.text().trim().parse::<f64>() {
            Ok(v) => self.set_value(v),
            Err(_) => self.entry.set_text(format_number(self.value, self.digits)),
        }
    }

    pub fn height(&self, theme: &Theme) -> f64 {
        theme.metrics.entry.height
    }

    /// Szerokość kolumny strzałek po prawej.
    pub fn arrows_width(&self, theme: &Theme, rect: Rect) -> f64 {
        theme
            .metrics
            .stepper
            .button
            .clamp(0.0, (rect.width() * 0.5).max(0.0))
    }

    /// Prostokąt strzałki `Up`/`Down`.
    pub fn arrow_rect(&self, rect: Rect, theme: &Theme, arrow: SpinArrow) -> Rect {
        let w = self.arrows_width(theme, rect);
        let half = rect.height() / 2.0;
        let y = match arrow {
            SpinArrow::Up => rect.y(),
            SpinArrow::Down => rect.y() + half,
        };
        Rect::from_xywh(rect.right() - w, y, w, half)
    }

    pub fn arrow_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<SpinArrow> {
        [SpinArrow::Up, SpinArrow::Down]
            .into_iter()
            .find(|&a| self.arrow_rect(rect, theme, a).contains(p))
    }

    pub fn set_arrows_hover(&mut self, hover: Option<SpinArrow>) {
        self.arrows_hover = hover;
    }

    /// Obsługuje klik w strzałkę: zmienia wartość. Zwraca `true`, gdy trafiono.
    pub fn click_at(&mut self, rect: Rect, theme: &Theme, p: Point) -> bool {
        match self.arrow_at(rect, theme, p) {
            Some(SpinArrow::Up) => {
                self.inc();
                true
            }
            Some(SpinArrow::Down) => {
                self.dec();
                true
            }
            None => false,
        }
    }

    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect) {
        let theme = *ctx.theme;
        let w = self.arrows_width(&theme, rect);
        let mut entry = self.entry.clone();
        entry.set_trailing_space(w + 6.0);
        entry.paint(ctx, rect);

        let m = theme.metrics.entry;
        for arrow in [SpinArrow::Up, SpinArrow::Down] {
            let ar = self.arrow_rect(rect, &theme, arrow);
            if self.arrows_hover == Some(arrow) {
                ctx.scene.fill_rounded_rect(
                    ar,
                    theme.radius.small,
                    theme.colors.shade.scale_alpha(1.8),
                );
            }
            let c = ar.center();
            let size = m.icon * 0.22;
            match arrow {
                SpinArrow::Up => icon::chevron_up(ctx, theme.colors.dim_text, c, size, 1.8),
                SpinArrow::Down => icon::chevron_down(ctx, theme.colors.dim_text, c, size, 1.8),
            }
        }
        // Cienka kreska oddzielająca strzałki od pola.
        ctx.scene.fill_rect(
            Rect::from_xywh(
                rect.right() - w - 2.0,
                rect.y() + 5.0,
                1.0,
                (rect.height() - 10.0).max(0.0),
            ),
            theme.colors.border,
        );
    }
}

impl TextInput for SpinButton {
    fn insert_str(&mut self, s: &str) {
        self.entry.insert_str(s);
    }
    fn insert_char(&mut self, c: char) {
        self.entry.insert_char(c);
    }
    fn backspace(&mut self) {
        self.entry.backspace();
    }
    fn delete_forward(&mut self) {
        self.entry.delete_forward();
    }
    fn move_left(&mut self) {
        self.entry.move_left();
    }
    fn move_right(&mut self) {
        self.entry.move_right();
    }
    fn home(&mut self) {
        self.entry.home();
    }
    fn end(&mut self) {
        self.entry.end();
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
        self.entry.copy()
    }
    fn cut(&mut self) -> Option<String> {
        self.entry.cut()
    }
    fn paste(&mut self, s: &str) {
        self.entry.paste(s);
    }
}

fn format_number(value: f64, digits: usize) -> String {
    if digits == 0 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.digits$}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect::from_xywh(0.0, 0.0, 120.0, 34.0)
    }

    #[test]
    fn value_is_clamped_and_formatted() {
        let mut s = SpinButton::new(5.0, 0.0, 10.0);
        assert_eq!(s.value(), 5.0);
        s.set_value(99.0);
        assert_eq!(s.value(), 10.0);
        s.set_value(-3.0);
        assert_eq!(s.value(), 0.0);
        assert_eq!(s.entry.text(), "0");
    }

    #[test]
    fn arrows_change_value() {
        let theme = Theme::light();
        let mut s = SpinButton::new(2.0, 0.0, 10.0).step(3.0);
        let up = s.arrow_rect(rect(), &theme, SpinArrow::Up).center();
        let down = s.arrow_rect(rect(), &theme, SpinArrow::Down).center();
        assert!(s.click_at(rect(), &theme, up));
        assert_eq!(s.value(), 5.0);
        assert!(s.click_at(rect(), &theme, down));
        assert_eq!(s.value(), 2.0);
        // Klik w pole (nie w strzałkę) nie zmienia wartości.
        assert!(!s.click_at(rect(), &theme, Point::new(10.0, 17.0)));
    }

    #[test]
    fn commit_parses_and_clamps() {
        let mut s = SpinButton::new(1.0, 0.0, 10.0);
        s.entry.set_text("7");
        s.commit();
        assert_eq!(s.value(), 7.0);
        s.entry.set_text("bzdury");
        s.commit();
        assert_eq!(s.value(), 7.0); // przywrócone formatowanie
        assert_eq!(s.entry.text(), "7");
        s.entry.set_text("999");
        s.commit();
        assert_eq!(s.value(), 10.0);
    }

    #[test]
    fn digits_formatting() {
        let s = SpinButton::new(2.5, 0.0, 10.0).digits(2);
        assert_eq!(s.entry.text(), "2.50");
    }
}
