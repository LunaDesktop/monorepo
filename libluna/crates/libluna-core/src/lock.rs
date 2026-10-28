//! Wygląd ekranu blokady (`~/.config/luna/lock.conf`).
//!
//! To ustawienia **użytkownika** — `luna-lock` działa w jego sesji, więc zmiany
//! nie wymagają uprawnień roota (inaczej niż motyw ekranu logowania greetera).
//! Domyślnie ekran blokady jest ciemny i pokazuje zegar.

use std::path::{Path, PathBuf};

use crate::appearance::{Accent, ThemePreference};
use crate::config::{config_file, parse_pairs, save_atomic};

/// Skąd brać tło ekranu blokady.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LockWallpaper {
    /// Ta sama tapeta co na pulpicie (przyciemniona).
    #[default]
    Desktop,
    /// Bez tapety — jednolite, ciemne tło.
    None,
}

impl LockWallpaper {
    pub const fn name(self) -> &'static str {
        match self {
            LockWallpaper::Desktop => "desktop",
            LockWallpaper::None => "none",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "desktop" => Some(LockWallpaper::Desktop),
            "none" => Some(LockWallpaper::None),
            _ => None,
        }
    }
}

/// Ustawienia ekranu blokady.
#[derive(Debug, Clone, PartialEq)]
pub struct LockSettings {
    pub theme: ThemePreference,
    /// `None` = akcent z motywu sesji.
    pub accent: Option<Accent>,
    pub show_clock: bool,
    pub wallpaper: LockWallpaper,
}

impl Default for LockSettings {
    fn default() -> Self {
        Self {
            theme: ThemePreference::Dark,
            accent: None,
            show_clock: true,
            wallpaper: LockWallpaper::Desktop,
        }
    }
}

impl LockSettings {
    pub fn config_path() -> Option<PathBuf> {
        config_file("lock.conf")
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
                "theme" => {
                    out.theme = match value.as_str() {
                        "light" => ThemePreference::Light,
                        "dark" => ThemePreference::Dark,
                        "system" => ThemePreference::System,
                        _ => out.theme,
                    };
                }
                "accent" => out.accent = Accent::from_name(&value),
                "show_clock" => out.show_clock = !matches!(value.as_str(), "false" | "0" | "no"),
                "wallpaper" => {
                    if let Some(wallpaper) = LockWallpaper::from_name(&value) {
                        out.wallpaper = wallpaper;
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Luna Lock — wygląd ekranu blokady\n\
             # Ustawienia użytkownika; nie wymagają uprawnień administratora.\n",
        );
        out.push_str(&format!(
            "theme = {}\n",
            match self.theme {
                ThemePreference::Light => "light",
                ThemePreference::Dark => "dark",
                ThemePreference::System => "system",
            }
        ));
        if let Some(accent) = self.accent {
            out.push_str(&format!("accent = {}\n", accent.name()));
        }
        out.push_str(&format!("show_clock = {}\n", self.show_clock));
        out.push_str(&format!("wallpaper = {}\n", self.wallpaper.name()));
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_lock_settings() {
        let text = "theme = light\naccent = purple\nshow_clock = false\nwallpaper = none\n";
        let settings = LockSettings::parse(text);
        assert_eq!(settings.theme, ThemePreference::Light);
        assert_eq!(settings.accent, Some(Accent::Purple));
        assert!(!settings.show_clock);
        assert_eq!(settings.wallpaper, LockWallpaper::None);
        assert_eq!(LockSettings::parse(&settings.render()), settings);
    }

    #[test]
    fn default_is_dark_with_desktop_wallpaper() {
        let settings = LockSettings::default();
        assert_eq!(settings.theme, ThemePreference::Dark);
        assert!(settings.show_clock);
        assert_eq!(settings.wallpaper, LockWallpaper::Desktop);
    }
}
