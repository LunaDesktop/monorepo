//! Preferencje okna powitania.
//!
//! Plik: `$XDG_CONFIG_HOME/luna/welcome.conf` (`autostart = true|false`).
//!
//! `autostart = false` odpowiada zaznaczonemu checkboxowi „Nie uruchamiaj
//! ponownie po zalogowaniu”. Ten sam plik czyta `luna-session`, żeby przy
//! kolejnym logowaniu nie startować okna.

use std::io;
use std::path::PathBuf;

use libluna_core::config;

/// Nazwa pliku konfiguracji w katalogu `$XDG_CONFIG_HOME/luna`.
const FILE: &str = "welcome.conf";

/// Ustawienia powitania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelcomeSettings {
    /// Czy okno ma się uruchamiać przy logowaniu.
    pub autostart: bool,
}

impl Default for WelcomeSettings {
    fn default() -> Self {
        Self { autostart: true }
    }
}

impl WelcomeSettings {
    /// Ścieżka konfiguracji (`$XDG_CONFIG_HOME/luna/welcome.conf`).
    pub fn path() -> Option<PathBuf> {
        config::config_file(FILE)
    }

    /// Czyta ustawienia; brak pliku lub błąd odczytu = wartości domyślne.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    /// Parsuje format `klucz = wartość` (ignoruje komentarze i nieznane klucze).
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for (key, value) in config::parse_pairs(text) {
            if key.eq_ignore_ascii_case("autostart") {
                settings.autostart = !is_false(&value);
            }
        }
        settings
    }

    /// Zapisuje ustawienia atomowo.
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        let text = format!(
            "# Luna Welcome — ustawienia okna powitania.\n\
             # autostart = false wyłącza okno przy kolejnym logowaniu.\n\
             autostart = {}\n",
            if self.autostart { "true" } else { "false" }
        );
        config::save_atomic(&path, &text)
    }
}

/// Wartości traktowane jako „wyłączone” (te same, których używa `luna-session`).
fn is_false(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_keeps_autostart() {
        assert_eq!(WelcomeSettings::parse(""), WelcomeSettings::default());
        assert!(WelcomeSettings::default().autostart);
    }

    #[test]
    fn false_values_disable_autostart() {
        for text in [
            "autostart = false",
            "autostart = 0",
            "autostart = no",
            "autostart = off",
            "AUTOSTART = FALSE",
        ] {
            assert!(!WelcomeSettings::parse(text).autostart, "{text}");
        }
    }

    #[test]
    fn true_values_keep_autostart() {
        for text in ["autostart = true", "autostart = 1", "autostart = yes"] {
            assert!(WelcomeSettings::parse(text).autostart, "{text}");
        }
    }
}
