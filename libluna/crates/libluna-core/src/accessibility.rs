//! Ustawienia dostępności wspólne dla całego środowiska.
//!
//! Trzymamy je poza motywem, bo dotyczą **prezentacji** (skalowanie tekstu,
//! kontrast, ograniczenie animacji), a nie palety akcentu. `ThemeSettings`
//! nakłada je po rozwiązaniu motywu i stylu, więc ta sama konfiguracja działa
//! w Ustawieniach, Plikach, Terminalu, powłoce i greeterze.
//!
//! Plik: `$XDG_CONFIG_HOME/luna/accessibility.conf` (`klucz = wartość`).

use std::io;
use std::path::PathBuf;

use crate::config::{config_dir, save_atomic};

/// Dopuszczalne skale tekstu (procent). Większe wartości nie mieszczą się
/// w stałych wysokościach pasków i wierszy, więc na razie kończymy na 150 %.
pub const TEXT_SCALE_CHOICES: [u8; 3] = [100, 125, 150];

/// Ustawienia dostępności.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessibilitySettings {
    /// Skala tekstu w procentach (jedna z [`TEXT_SCALE_CHOICES`]).
    pub text_scale: u8,
    /// Wysoki kontrast (mocne obramowania, czarno-biały tekst).
    pub high_contrast: bool,
    /// Ograniczenie animacji (toasty, przejścia).
    pub reduce_motion: bool,
}

impl Default for AccessibilitySettings {
    fn default() -> Self {
        Self {
            text_scale: 100,
            high_contrast: false,
            reduce_motion: false,
        }
    }
}

impl AccessibilitySettings {
    /// Najbliższa dozwolona skala (np. 130 → 125, 999 → 150).
    pub fn clamp_scale(percent: u8) -> u8 {
        TEXT_SCALE_CHOICES
            .into_iter()
            .min_by_key(|choice| (*choice as i32 - percent as i32).abs())
            .unwrap_or(100)
    }

    /// Mnożnik rozmiarów tekstu (1.0 = 100 %).
    pub fn scale_factor(&self) -> f64 {
        Self::clamp_scale(self.text_scale) as f64 / 100.0
    }

    /// Normalizuje wartości wczytane z pliku.
    pub fn normalized(mut self) -> Self {
        self.text_scale = Self::clamp_scale(self.text_scale);
        self
    }

    /// Ścieżka konfiguracji: `$XDG_CONFIG_HOME/luna/accessibility.conf`.
    pub fn config_path() -> Option<PathBuf> {
        Some(config_dir()?.join("accessibility.conf"))
    }

    /// Parsuje format `klucz = wartość` (nieznane klucze są pomijane).
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "text_scale" => {
                    if let Ok(percent) = value.parse::<u8>() {
                        settings.text_scale = Self::clamp_scale(percent);
                    }
                }
                "high_contrast" => settings.high_contrast = parse_bool(value),
                "reduce_motion" => settings.reduce_motion = parse_bool(value),
                _ => {}
            }
        }
        settings
    }

    /// Serializuje ustawienia z komentarzem nagłówkowym.
    pub fn to_config(&self) -> String {
        format!(
            "# Ustawienia dostępności Luny\ntext_scale = {}\nhigh_contrast = {}\nreduce_motion = {}\n",
            self.text_scale,
            self.high_contrast as u8,
            self.reduce_motion as u8
        )
    }

    /// Wczytuje konfigurację; przy błędzie zwraca wartości domyślne.
    pub fn load() -> Self {
        // Podglądy i testy mogą wskazać własny plik albo go wyłączyć.
        let path = match std::env::var_os("LUNA_ACCESSIBILITY_CONF") {
            Some(path) => PathBuf::from(path),
            None => match Self::config_path() {
                Some(path) => path,
                None => return Self::default(),
            },
        };
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Zapisuje konfigurację (tworzy katalog).
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "brak katalogu konfiguracji",
            ));
        };
        save_atomic(&path, &self.to_config())
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_all_fields() {
        let settings = AccessibilitySettings {
            text_scale: 150,
            high_contrast: true,
            reduce_motion: true,
        };
        assert_eq!(
            AccessibilitySettings::parse(&settings.to_config()),
            settings
        );
    }

    #[test]
    fn invalid_scale_is_clamped_to_a_choice() {
        let settings = AccessibilitySettings::parse("text_scale = 130\n");
        assert_eq!(settings.text_scale, 125);
        let settings = AccessibilitySettings::parse("text_scale = 10\n");
        assert_eq!(settings.text_scale, 100);
        // Wartość spoza zakresu `u8` jest ignorowana (zostaje domyślne 100).
        let settings = AccessibilitySettings::parse("text_scale = 9999\n");
        assert_eq!(settings.text_scale, 100);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings = AccessibilitySettings::parse("text_scale = 125\nfoo = bar\n");
        assert_eq!(settings.text_scale, 125);
        assert!(!settings.high_contrast);
    }

    #[test]
    fn scale_factor_matches_percent() {
        let mut settings = AccessibilitySettings::default();
        assert!((settings.scale_factor() - 1.0).abs() < f64::EPSILON);
        settings.text_scale = 125;
        assert!((settings.scale_factor() - 1.25).abs() < f64::EPSILON);
    }
}
