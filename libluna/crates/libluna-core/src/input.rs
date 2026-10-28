//! Ustawienia wejścia (`~/.config/luna/input.conf`).
//!
//! Kompozytor Selene czyta ten plik przy starcie, a Ustawienia mogą zmienić
//! większość wartości na żywo przez `luna-control` (`input repeat …`,
//! `input natural-scroll …`). Trzymamy tu tylko wartości, które realnie
//! obsługuje wejście evdev — reszta (tap-to-click, akceleracja) jest nadal
//! oznaczona jako „w trakcie rozwoju” w interfejsie.

use std::path::{Path, PathBuf};

use crate::config::{config_file, parse_pairs, save_atomic};

/// Minimalne i maksymalne opóźnienie powtarzania klawiszy (ms).
pub const MIN_REPEAT_DELAY: u32 = 100;
pub const MAX_REPEAT_DELAY: u32 = 2000;
/// Minimalna i maksymalna częstość powtarzania (klawisze/s).
pub const MIN_REPEAT_RATE: u32 = 5;
pub const MAX_REPEAT_RATE: u32 = 60;

/// Ustawienia klawiatury i myszy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSettings {
    /// Opóźnienie przed rozpoczęciem powtarzania (ms).
    pub repeat_delay: u32,
    /// Częstość powtarzania (klawisze na sekundę).
    pub repeat_rate: u32,
    /// Odwrócony kierunek przewijania („naturalne przewijanie”).
    pub natural_scroll: bool,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            repeat_delay: 600,
            repeat_rate: 25,
            natural_scroll: false,
        }
    }
}

impl InputSettings {
    pub fn repeat_delay(&self) -> u32 {
        self.repeat_delay
    }

    pub fn repeat_rate(&self) -> u32 {
        self.repeat_rate
    }

    pub fn natural_scroll(&self) -> bool {
        self.natural_scroll
    }

    pub fn set_repeat_delay(&mut self, delay: u32) {
        self.repeat_delay = delay.clamp(MIN_REPEAT_DELAY, MAX_REPEAT_DELAY);
    }

    pub fn set_repeat_rate(&mut self, rate: u32) {
        self.repeat_rate = rate.clamp(MIN_REPEAT_RATE, MAX_REPEAT_RATE);
    }

    pub fn set_natural_scroll(&mut self, enabled: bool) {
        self.natural_scroll = enabled;
    }

    pub fn config_path() -> Option<PathBuf> {
        config_file("input.conf")
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "repeat_delay" => {
                    if let Ok(delay) = value.parse::<u32>() {
                        settings.set_repeat_delay(delay);
                    }
                }
                "repeat_rate" => {
                    if let Ok(rate) = value.parse::<u32>() {
                        settings.set_repeat_rate(rate);
                    }
                }
                "natural_scroll" => {
                    settings.set_natural_scroll(!matches!(value.as_str(), "false" | "0" | "no"));
                }
                _ => {}
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        format!(
            "# Luna — klawiatura i mysz\n\
             # repeat_delay: {MIN_REPEAT_DELAY}..{MAX_REPEAT_DELAY} ms\n\
             # repeat_rate: {MIN_REPEAT_RATE}..{MAX_REPEAT_RATE} klawiszy/s\n\
             repeat_delay = {}\n\
             repeat_rate = {}\n\
             natural_scroll = {}\n",
            self.repeat_delay, self.repeat_rate, self.natural_scroll
        )
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_all_keys() {
        let settings =
            InputSettings::parse("repeat_delay = 250\nrepeat_rate = 40\nnatural_scroll = true\n");
        assert_eq!(settings.repeat_delay(), 250);
        assert_eq!(settings.repeat_rate(), 40);
        assert!(settings.natural_scroll());
        assert_eq!(InputSettings::parse(&settings.render()), settings);
    }

    #[test]
    fn values_are_clamped() {
        let mut settings = InputSettings::parse("repeat_delay = 1\nrepeat_rate = 999\n");
        assert_eq!(settings.repeat_delay(), MIN_REPEAT_DELAY);
        assert_eq!(settings.repeat_rate(), MAX_REPEAT_RATE);
        settings.set_repeat_delay(99999);
        assert_eq!(settings.repeat_delay(), MAX_REPEAT_DELAY);
    }
}
