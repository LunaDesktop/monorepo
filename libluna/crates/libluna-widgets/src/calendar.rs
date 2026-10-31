//! `Calendar` — kalendarz miesięczny (GTK4): siatka tygodni + wybór dnia.
//!
//! Widget immediate: trzyma rok, miesiąc i wybrany dzień, liczy geometrię dni i obsługuje
//! trafienia. Nazwy dni tygodnia i tytuł miesiąca pochodzą z aplikacji (i18n), bo widgety
//! libLuny nie zależą od warstwy językowej.

use libluna_core::style::weight;
use libluna_core::{Point, Rect, Theme};

use crate::ctx::PaintCtx;

/// Kolumny tygodnia.
const COLS: usize = 7;
/// Stała liczba wierszy dni (miesiąc zajmuje do 6 tygodni) — stabilny układ.
const ROWS: usize = 6;

/// Kalendarz miesięczny.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calendar {
    year: i32,
    month: u32,
    selected: u32,
}

impl Calendar {
    /// Kalendarz na dany miesiąc (`month` 1..=12), z wybranym dniem 1.
    pub fn new(year: i32, month: u32) -> Self {
        Self {
            year,
            month: month.clamp(1, 12),
            selected: 1,
        }
    }

    pub fn year(&self) -> i32 {
        self.year
    }

    pub fn month(&self) -> u32 {
        self.month
    }

    pub fn selected(&self) -> u32 {
        self.selected
    }

    /// Wybiera dzień (przycięty do zakresu miesiąca).
    pub fn set_selected(&mut self, day: u32) {
        self.selected = day.clamp(1, self.days_in_month());
    }

    /// Liczba dni w bieżącym miesiącu.
    pub fn days_in_month(&self) -> u32 {
        days_in_month(self.year, self.month)
    }

    /// Dzień tygodnia pierwszego dnia miesiąca (`0` = poniedziałek … `6` = niedziela).
    pub fn first_weekday(&self) -> u32 {
        let w = days_from_civil(self.year, self.month, 1);
        ((w + 4).rem_euclid(7) as u32 + 6) % 7
    }

    /// Przechodzi do poprzedniego miesiąca.
    pub fn prev_month(&mut self) {
        if self.month == 1 {
            self.month = 12;
            self.year -= 1;
        } else {
            self.month -= 1;
        }
        self.set_selected(self.selected);
    }

    /// Przechodzi do następnego miesiąca.
    pub fn next_month(&mut self) {
        if self.month == 12 {
            self.month = 1;
            self.year += 1;
        } else {
            self.month += 1;
        }
        self.set_selected(self.selected);
    }

    pub fn title_height(&self, theme: &Theme) -> f64 {
        theme.typography.body.line_height + theme.spacing.sm * 2.0
    }

    pub fn weekday_height(&self, theme: &Theme) -> f64 {
        theme.typography.caption.line_height + theme.spacing.xs
    }

    fn grid_rect(&self, rect: Rect, theme: &Theme) -> Rect {
        let top = self.title_height(theme) + self.weekday_height(theme);
        Rect::from_xywh(
            rect.x(),
            rect.y() + top,
            rect.width(),
            (rect.height() - top).max(0.0),
        )
    }

    /// Prostokąt komórki `(row, col)` w siatce dni.
    pub fn cell_rect(&self, rect: Rect, theme: &Theme, row: usize, col: usize) -> Rect {
        let g = self.grid_rect(rect, theme);
        let cw = g.width() / COLS as f64;
        let ch = g.height() / ROWS as f64;
        Rect::from_xywh(g.x() + col as f64 * cw, g.y() + row as f64 * ch, cw, ch)
    }

    /// Prostokąt dnia miesiąca.
    pub fn day_rect(&self, rect: Rect, theme: &Theme, day: u32) -> Rect {
        let idx = self.first_weekday() + day - 1;
        let row = (idx / COLS as u32) as usize;
        let col = (idx % COLS as u32) as usize;
        self.cell_rect(rect, theme, row.min(ROWS - 1), col)
    }

    /// Dzień pod punktem `p` (lub `None`).
    pub fn day_at(&self, rect: Rect, theme: &Theme, p: Point) -> Option<u32> {
        (1..=self.days_in_month()).find(|d| self.day_rect(rect, theme, *d).contains(p))
    }

    /// Rysuje kalendarz: tytuł, nagłówek dni tygodnia (`weekdays` = 7 napisów) i dni.
    pub fn paint(&self, ctx: &mut PaintCtx, rect: Rect, title: &str, weekdays: &[String]) {
        let theme = *ctx.theme;
        let body = theme.typography.body.size;
        let caption = theme.typography.caption.size;

        // Tytuł miesiąca.
        ctx.label_left(
            rect.x() + theme.spacing.sm,
            rect.y() + self.title_height(&theme) / 2.0,
            title,
            body,
            weight::MEDIUM,
            theme.colors.text,
        );

        // Nagłówek dni tygodnia.
        let g = self.grid_rect(rect, &theme);
        let wy = rect.y() + self.title_height(&theme) + self.weekday_height(&theme) / 2.0;
        let cw = g.width() / COLS as f64;
        for (i, name) in weekdays.iter().take(COLS).enumerate() {
            ctx.label_center(
                g.x() + (i as f64 + 0.5) * cw,
                wy,
                name,
                caption,
                weight::NORMAL,
                theme.colors.dim_text,
            );
        }

        // Dni.
        for day in 1..=self.days_in_month() {
            let cell = self.day_rect(rect, &theme, day);
            let c = cell.center();
            let selected = day == self.selected;
            // Wysokie komórki (np. kalendarz rozciągnięty na kartę) nie mogą
            // zamieniać podświetlenia dnia w ogromne koło.
            let r = (cell.width().min(cell.height()) / 2.0 - 2.0).clamp(6.0, 15.0);
            if selected {
                ctx.scene.fill_rounded_rect(
                    Rect::from_xywh(c.x - r, c.y - r, r * 2.0, r * 2.0),
                    r,
                    theme.colors.accent,
                );
            }
            let fg = if selected {
                theme.colors.accent_fg
            } else {
                theme.colors.text
            };
            ctx.label_center(c.x, c.y, &day.to_string(), caption, weight::NORMAL, fg);
        }
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Liczba dni od 1970-01-01 (algorytm Howarda Hinnanta). 1970-01-01 = 0 (czwartek).
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let m = m as i64;
    let d = d as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_lengths_and_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 1), 31);
    }

    #[test]
    fn first_weekday_is_monday_zero() {
        // 2024-01-01 to poniedziałek.
        assert_eq!(Calendar::new(2024, 1).first_weekday(), 0);
        // 2023-01-01 to niedziela.
        assert_eq!(Calendar::new(2023, 1).first_weekday(), 6);
    }

    #[test]
    fn month_navigation_wraps_years() {
        let mut c = Calendar::new(2024, 1);
        c.prev_month();
        assert_eq!((c.year(), c.month()), (2023, 12));
        c.next_month();
        assert_eq!((c.year(), c.month()), (2024, 1));
        let mut d = Calendar::new(2024, 12);
        d.next_month();
        assert_eq!((d.year(), d.month()), (2025, 1));
    }

    #[test]
    fn selected_day_is_clamped() {
        let mut c = Calendar::new(2023, 2);
        c.set_selected(31);
        assert_eq!(c.selected(), 28);
        c.set_selected(0);
        assert_eq!(c.selected(), 1);
    }

    #[test]
    fn day_cells_hit_testing() {
        let theme = Theme::light();
        let rect = Rect::from_xywh(0.0, 0.0, 350.0, 300.0);
        let c = Calendar::new(2024, 1);
        let jan1 = c.day_rect(rect, &theme, 1);
        assert_eq!(c.day_at(rect, &theme, jan1.center()), Some(1));
        // Punkt w wierszu tytułu nie trafia w żaden dzień.
        assert_eq!(c.day_at(rect, &theme, Point::new(5.0, 5.0)), None);
        // Dni mieszczą się w siatce.
        for day in 1..=c.days_in_month() {
            let r = c.day_rect(rect, &theme, day);
            assert!(r.bottom() <= rect.bottom() + 1e-6);
        }
    }
}
