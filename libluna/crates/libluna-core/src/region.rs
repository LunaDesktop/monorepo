//! Region, formaty i strefa czasowa (`~/.config/luna/region.conf`).
//!
//! Listy są pobierane z systemu: locale z `/usr/share/i18n/SUPPORTED` (albo
//! `locale -a`), strefy z `/usr/share/zoneinfo/zone.tab` (albo
//! `timedatectl list-timezones`). Wybór użytkownika jest **per sesja** —
//! `luna-session` eksportuje z niego `LC_TIME` i `TZ`, bez potrzeby roota.
//! Zmiana strefy systemowej to osobna, uprzywilejowana operacja.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{clean_value, config_file, parse_pairs, save_atomic};

/// Ustawienia regionu użytkownika.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegionSettings {
    /// Format locale, np. `pl_PL.UTF-8`; `None` = systemowy.
    pub locale: Option<String>,
    /// Strefa czasowa IANA, np. `Europe/Warsaw`; `None` = systemowa.
    pub timezone: Option<String>,
}

impl RegionSettings {
    pub fn config_path() -> Option<PathBuf> {
        config_file("region.conf")
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "locale" => out.locale = valid_locale(&value),
                "timezone" => out.timezone = valid_timezone(&value),
                _ => {}
            }
        }
        out
    }

    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Luna — region i strefa czasowa\n\
             # Wygenerowane przez Ustawienia Luny. Puste = wartość systemowa.\n",
        );
        if let Some(locale) = &self.locale {
            out.push_str(&format!("locale = {locale}\n"));
        }
        if let Some(timezone) = &self.timezone {
            out.push_str(&format!("timezone = {timezone}\n"));
        }
        out
    }

    pub fn save_local(&self, path: &Path) -> std::io::Result<()> {
        save_atomic(path, &self.render())
    }

    pub fn save(&self) -> std::io::Result<()> {
        match Self::config_path() {
            Some(path) => self.save_local(&path),
            None => Ok(()),
        }
    }

    /// Locale faktycznie używany przez Lunę (zapisany albo systemowy).
    pub fn effective_locale(&self) -> String {
        self.locale.clone().unwrap_or_else(current_locale)
    }

    /// Strefa faktycznie używana przez Lunę (zapisana albo systemowa).
    pub fn effective_timezone(&self) -> String {
        self.timezone.clone().unwrap_or_else(current_timezone)
    }
}

fn valid_locale(value: &str) -> Option<String> {
    clean_value(value).filter(|v| {
        v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'))
    })
}

fn valid_timezone(value: &str) -> Option<String> {
    clean_value(value).filter(|v| {
        !v.starts_with('/')
            && !v.contains("..")
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'))
    })
}

/// Locale systemowy (do wyświetlenia i wartości domyślnej).
pub fn current_locale() -> String {
    for key in ["LC_ALL", "LC_TIME", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return value;
            }
        }
    }
    "C".to_owned()
}

/// Strefa systemowa: `/etc/timezone`, dowiązanie `/etc/localtime`, `timedatectl`.
pub fn current_timezone() -> String {
    if let Ok(text) = std::fs::read_to_string("/etc/timezone") {
        if let Some(tz) = valid_timezone(text.lines().next().unwrap_or("")) {
            return tz;
        }
    }
    if let Ok(target) = std::fs::canonicalize("/etc/localtime") {
        let text = target.to_string_lossy();
        if let Some((_, tz)) = text.split_once("/zoneinfo/") {
            if let Some(tz) = valid_timezone(tz) {
                return tz;
            }
        }
    }
    if let Ok(output) = Command::new("timedatectl")
        .args(["show", "-p", "Timezone", "--value"])
        .output()
    {
        if output.status.success() {
            if let Some(tz) = valid_timezone(&String::from_utf8_lossy(&output.stdout)) {
                return tz;
            }
        }
    }
    "UTC".to_owned()
}

/// Locale dostępne w systemie (posortowane, bez duplikatów).
pub fn system_locales() -> Vec<String> {
    let mut locales = Vec::new();
    if let Ok(text) = std::fs::read_to_string("/usr/share/i18n/SUPPORTED") {
        for line in text.lines() {
            let Some(code) = line.split_whitespace().next() else {
                continue;
            };
            if looks_like_locale(code) {
                push_unique(&mut locales, code.to_owned());
            }
        }
    }
    if locales.is_empty() {
        if let Ok(output) = Command::new("locale").arg("-a").output() {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let code = line.trim();
                    if looks_like_locale(code) {
                        push_unique(&mut locales, code.to_owned());
                    }
                }
            }
        }
    }
    // Język interfejsu i systemowy zawsze muszą być na liście, nawet na minimalnym systemie.
    push_unique(&mut locales, current_locale());
    locales.sort_by_cached_key(|code| code.to_lowercase());
    locales
}

fn looks_like_locale(code: &str) -> bool {
    code.contains('_')
        && !code.starts_with('.')
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'))
}

/// Strefy czasowe dostępne w systemie (posortowane, bez duplikatów).
pub fn system_timezones() -> Vec<String> {
    let mut zones = Vec::new();
    if let Ok(text) = std::fs::read_to_string("/usr/share/zoneinfo/zone.tab") {
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut fields = line.split('\t');
            let _country = fields.next();
            let _coords = fields.next();
            if let Some(zone) = fields.next().and_then(valid_timezone) {
                push_unique(&mut zones, zone);
            }
        }
    }
    if zones.is_empty() {
        if let Ok(output) = Command::new("timedatectl").arg("list-timezones").output() {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some(zone) = valid_timezone(line) {
                        push_unique(&mut zones, zone);
                    }
                }
            }
        }
    }
    push_unique(&mut zones, "UTC".to_owned());
    push_unique(&mut zones, current_timezone());
    zones.sort_by_cached_key(|zone| zone.to_lowercase());
    zones
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if value.is_empty() {
        return;
    }
    if list
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    list.push(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_region() {
        let text = "# region\nlocale = pl_PL.UTF-8\ntimezone = Europe/Warsaw\n";
        let settings = RegionSettings::parse(text);
        assert_eq!(settings.locale.as_deref(), Some("pl_PL.UTF-8"));
        assert_eq!(settings.timezone.as_deref(), Some("Europe/Warsaw"));
        assert_eq!(RegionSettings::parse(&settings.render()), settings);
    }

    #[test]
    fn rejects_escaping_timezone_paths() {
        let settings = RegionSettings::parse("timezone = ../../etc/passwd\n");
        assert!(settings.timezone.is_none());
        let settings = RegionSettings::parse("timezone = /etc/localtime\n");
        assert!(settings.timezone.is_none());
    }

    #[test]
    fn system_lists_are_never_empty_and_include_current_values() {
        let locales = system_locales();
        assert!(!locales.is_empty());
        assert!(locales.iter().any(|l| l == &current_locale()));

        let zones = system_timezones();
        assert!(!zones.is_empty());
        assert!(zones.iter().any(|z| z == &current_timezone()));
    }
}
