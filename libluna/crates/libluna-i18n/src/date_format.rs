//! Format daty wspólny dla zegara, kalendarza i paska Luny.
//!
//! Ustawienia mieszkają w `~/.config/luna/clock.conf` (te same, których używa
//! `luna-clock`), więc zmiana w Ustawieniach działa od razu także w panelu.
//!
//! Format opisują cztery niezależne wybory: zakres pól, cyfry/napisy,
//! kolejność dnia i miesiąca oraz separator.

use libluna_core::civil::CivilDate;
use libluna_core::config::{config_file, parse_pairs};

use crate::tr;

/// Czy pokazywać sam dzień i miesiąc, czy też rok.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateFields {
    /// Dzień i miesiąc (bez roku).
    DayMonth,
    /// Dzień, miesiąc i rok.
    #[default]
    DayMonthYear,
}

/// Czy miesiąc zapisywać cyframi, czy nazwą.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateStyle {
    /// `7.11.2026`
    #[default]
    Numeric,
    /// `7 listopada 2026`
    Words,
}

/// Kolejność dnia i miesiąca.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateOrder {
    /// Dzień → miesiąc (`7.11.2026`).
    #[default]
    DayFirst,
    /// Miesiąc → dzień, jak w zapisie angielskim (`11/7/2026`).
    MonthFirst,
}

/// Separator w formacie cyfrowym.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateSeparator {
    #[default]
    Dot,
    Dash,
    Slash,
}

impl DateSeparator {
    pub fn ch(self) -> char {
        match self {
            DateSeparator::Dot => '.',
            DateSeparator::Dash => '-',
            DateSeparator::Slash => '/',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DateSeparator::Dot => ".",
            DateSeparator::Dash => "-",
            DateSeparator::Slash => "/",
        }
    }

    pub fn next(self) -> Self {
        match self {
            DateSeparator::Dot => DateSeparator::Dash,
            DateSeparator::Dash => DateSeparator::Slash,
            DateSeparator::Slash => DateSeparator::Dot,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DateSeparator::Dot => "dot",
            DateSeparator::Dash => "dash",
            DateSeparator::Slash => "slash",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            "dot" => Some(DateSeparator::Dot),
            "dash" => Some(DateSeparator::Dash),
            "slash" => Some(DateSeparator::Slash),
            _ => None,
        }
    }
}

impl DateFields {
    pub fn label(self) -> String {
        match self {
            DateFields::DayMonth => tr("Day and month"),
            DateFields::DayMonthYear => tr("Day, month and year"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            DateFields::DayMonth => DateFields::DayMonthYear,
            DateFields::DayMonthYear => DateFields::DayMonth,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DateFields::DayMonth => "day_month",
            DateFields::DayMonthYear => "day_month_year",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            "day_month" => Some(DateFields::DayMonth),
            "day_month_year" => Some(DateFields::DayMonthYear),
            _ => None,
        }
    }
}

impl DateStyle {
    pub fn label(self) -> String {
        match self {
            DateStyle::Numeric => tr("Digits"),
            DateStyle::Words => tr("Words"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            DateStyle::Numeric => DateStyle::Words,
            DateStyle::Words => DateStyle::Numeric,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DateStyle::Numeric => "numeric",
            DateStyle::Words => "words",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            "numeric" => Some(DateStyle::Numeric),
            "words" => Some(DateStyle::Words),
            _ => None,
        }
    }
}

impl DateOrder {
    pub fn label(self) -> String {
        match self {
            DateOrder::DayFirst => tr("Day then month"),
            DateOrder::MonthFirst => tr("Month then day"),
        }
    }

    pub fn next(self) -> Self {
        match self {
            DateOrder::DayFirst => DateOrder::MonthFirst,
            DateOrder::MonthFirst => DateOrder::DayFirst,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DateOrder::DayFirst => "day_first",
            DateOrder::MonthFirst => "month_first",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            "day_first" => Some(DateOrder::DayFirst),
            "month_first" => Some(DateOrder::MonthFirst),
            _ => None,
        }
    }
}

/// Nazwa miesiąca jako klucz i18n (`tr` zamieni ją na język interfejsu).
pub fn month_name_key(month: u32) -> &'static str {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        _ => "December",
    }
}

/// Nazwa dnia tygodnia jako klucz i18n (`0` = poniedziałek).
pub fn weekday_name_key(weekday: u32) -> &'static str {
    match weekday % 7 {
        0 => "Monday",
        1 => "Tuesday",
        2 => "Wednesday",
        3 => "Thursday",
        4 => "Friday",
        5 => "Saturday",
        _ => "Sunday",
    }
}

/// Ustawienia formatu daty (wspólne dla nagłówka, kalendarza i paska).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateFormat {
    pub fields: DateFields,
    pub style: DateStyle,
    pub order: DateOrder,
    pub separator: DateSeparator,
}

impl DateFormat {
    /// Wczytuje format z `~/.config/luna/clock.conf`.
    ///
    /// Brak pliku/kluczy = wartości domyślne (dzień, miesiąc, rok · cyfry · `.`).
    pub fn load() -> Self {
        let Some(path) = config_file("clock.conf") else {
            return Self::default();
        };
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Parsuje format z treści `clock.conf` (klucze `date_fields` itd.).
    pub fn parse(text: &str) -> Self {
        let mut format = Self::default();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "date_fields" => {
                    if let Some(fields) = DateFields::from_key(&value) {
                        format.fields = fields;
                    }
                }
                "date_style" => {
                    if let Some(style) = DateStyle::from_key(&value) {
                        format.style = style;
                    }
                }
                "date_order" => {
                    if let Some(order) = DateOrder::from_key(&value) {
                        format.order = order;
                    }
                }
                "date_separator" => {
                    if let Some(separator) = DateSeparator::from_key(&value) {
                        format.separator = separator;
                    }
                }
                _ => {}
            }
        }
        format
    }

    /// Data bez dnia tygodnia (np. do listy świąt).
    pub fn date(&self, date: CivilDate) -> String {
        match self.style {
            DateStyle::Numeric => {
                let day = format!("{:02}", date.day);
                let month = format!("{:02}", date.month);
                let sep = self.separator.ch();
                let body = if self.order == DateOrder::MonthFirst {
                    format!("{month}{sep}{day}")
                } else {
                    format!("{day}{sep}{month}")
                };
                if self.fields == DateFields::DayMonthYear {
                    format!("{body}{sep}{}", date.year)
                } else {
                    body
                }
            }
            DateStyle::Words => {
                let month = tr(month_name_key(date.month));
                let body = if self.order == DateOrder::MonthFirst {
                    format!("{month} {}", date.day)
                } else {
                    format!("{} {month}", date.day)
                };
                if self.fields == DateFields::DayMonthYear {
                    if self.order == DateOrder::MonthFirst {
                        format!("{body}, {}", date.year)
                    } else {
                        format!("{body} {}", date.year)
                    }
                } else {
                    body
                }
            }
        }
    }

    /// Data z dniem tygodnia (nagłówek zegara, szczegóły dnia, pasek).
    pub fn full(&self, date: CivilDate) -> String {
        format!(
            "{}, {}",
            tr(weekday_name_key(date.weekday())),
            self.date(date)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_follow_all_four_choices() {
        let date = CivilDate::from_ymd(2026, 10, 31);
        let mut format = DateFormat::default();
        assert_eq!(format.date(date), "31.10.2026");

        format.fields = DateFields::DayMonth;
        assert_eq!(format.date(date), "31.10");

        format.style = DateStyle::Words;
        let october = tr(month_name_key(10));
        assert_eq!(format.date(date), format!("31 {october}"));
        assert_eq!(format.full(date), format!("{}, 31 {october}", tr("Saturday")));

        format.fields = DateFields::DayMonthYear;
        format.order = DateOrder::MonthFirst;
        assert_eq!(format.date(date), format!("{october} 31, 2026"));

        format.style = DateStyle::Numeric;
        format.separator = DateSeparator::Slash;
        assert_eq!(format.date(date), "10/31/2026");
        format.order = DateOrder::DayFirst;
        assert_eq!(format.date(date), "31/10/2026");
        format.separator = DateSeparator::Dash;
        assert_eq!(format.date(date), "31-10-2026");
    }

    #[test]
    fn parses_clock_conf_keys_and_ignores_unknown_values() {
        let format = DateFormat::parse(
            "auto_theme = 1\ndate_fields = day_month\ndate_style = words\n\
             date_order = month_first\ndate_separator = slash\n",
        );
        assert_eq!(format.fields, DateFields::DayMonth);
        assert_eq!(format.style, DateStyle::Words);
        assert_eq!(format.order, DateOrder::MonthFirst);
        assert_eq!(format.separator, DateSeparator::Slash);

        assert_eq!(
            DateFormat::parse("date_style = xyz\ndate_separator = ??\n"),
            DateFormat::default()
        );
    }
}
