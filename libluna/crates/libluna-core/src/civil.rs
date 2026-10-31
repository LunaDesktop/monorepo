//! Data cywilna i czysta matematyka kalendarza (algorytm Howarda Hinnanta).
//!
//! Wspólna dla aplikacji kalendarza i powłoki, żeby święta i siatka miesięcy
//! liczyły się identycznie. Bez stref czasowych — te są warstwą wyżej.

/// Sekund w dobie.
pub const SECONDS_PER_DAY: i64 = 86_400;

/// Data w kalendarzu gregoriańskim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct CivilDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CivilDate {
    pub const fn from_ymd(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// Data z liczby dni od 1970-01-01.
    pub fn from_unix_days(days: i64) -> Self {
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day }
    }

    /// Liczba dni od 1970-01-01 (ujemna dla dat wcześniejszych).
    pub fn unix_days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    /// Dzień tygodnia: `0` = poniedziałek … `6` = niedziela.
    pub fn weekday(self) -> u32 {
        ((self.unix_days() + 3).rem_euclid(7)) as u32
    }

    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if Self::is_leap_year(year) {
                    29
                } else {
                    28
                }
            }
            _ => 30,
        }
    }

    pub fn days_in_its_month(self) -> u32 {
        Self::days_in_month(self.year, self.month)
    }

    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= Self::days_in_month(self.year, self.month)
    }

    pub fn add_days(self, delta: i64) -> Self {
        Self::from_unix_days(self.unix_days().saturating_add(delta))
    }

    /// Poniedziałek tygodnia zawierającego tę datę.
    pub fn start_of_week(self) -> Self {
        self.add_days(-(self.weekday() as i64))
    }

    /// ISO 8601: `RRRR-MM-DD`.
    pub fn to_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Parsuje ścisłe `RRRR-MM-DD` (bez godzin i stref).
    pub fn parse_iso(text: &str) -> Option<Self> {
        let text = text.trim();
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        if !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        {
            return None;
        }
        let year = text[0..4].parse::<i32>().ok()?;
        let month = text[5..7].parse::<u32>().ok()?;
        let day = text[8..10].parse::<u32>().ok()?;
        let date = Self { year, month, day };
        date.is_valid().then_some(date)
    }
}

/// Liczba dni od 1970-01-01.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year } as i64;
    let m = month as i64;
    let d = day as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `(rok, miesiąc, dzień)` z liczby dni od 1970-01-01.
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y as i32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_roundtrips_for_known_dates() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2024, 2, 29),
            (2026, 10, 31),
            (1900, 3, 1),
            (2038, 1, 19),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d));
            assert_eq!(
                CivilDate::from_unix_days(days).to_iso(),
                format!("{y:04}-{m:02}-{d:02}")
            );
        }
    }

    #[test]
    fn weekdays_and_iso() {
        assert_eq!(CivilDate::from_ymd(1970, 1, 1).weekday(), 3);
        assert_eq!(CivilDate::from_ymd(2024, 1, 1).weekday(), 0);
        assert_eq!(
            CivilDate::parse_iso("2026-10-31"),
            Some(CivilDate::from_ymd(2026, 10, 31))
        );
        assert_eq!(CivilDate::parse_iso("2026-02-30"), None);
        assert_eq!(CivilDate::parse_iso("31.10.2026"), None);
    }

    #[test]
    fn month_lengths() {
        assert_eq!(CivilDate::days_in_month(2024, 2), 29);
        assert_eq!(CivilDate::days_in_month(2023, 2), 28);
        assert_eq!(CivilDate::days_in_month(2026, 10), 31);
        assert_eq!(CivilDate::days_in_month(2026, 4), 30);
    }
}
