//! Ustawienia panelu Luny (`~/.config/luna/panel.conf`).
//!
//! Powłoka (`luna-shell`) czyta ten plik przy rysowaniu paska, a Ustawienia
//! zapisują zmiany od razu — bez wylogowania. Format jest ten sam, co innych
//! plików Luny (`klucz = wartość`), więc plik można edytować ręcznie.

use std::path::{Path, PathBuf};

use crate::config::{config_file, parse_pairs, save_atomic};

/// Krawędź ekranu, do której przyklejony jest panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelPosition {
    /// Górna krawędź (domyślnie).
    #[default]
    Top,
    /// Dolna krawędź (jak dock w macOS).
    Bottom,
    /// Lewa krawędź — pionowy pasek.
    Left,
    /// Prawa krawędź — pionowy pasek.
    Right,
}

impl PanelPosition {
    pub const ALL: [PanelPosition; 4] = [
        PanelPosition::Top,
        PanelPosition::Bottom,
        PanelPosition::Left,
        PanelPosition::Right,
    ];

    /// Klucz w `panel.conf`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "top" | "gora" | "góra" => Some(Self::Top),
            "bottom" | "dol" | "dół" => Some(Self::Bottom),
            "left" | "lewo" | "bok-lewy" => Some(Self::Left),
            "right" | "prawo" | "bok-prawy" => Some(Self::Right),
            _ => None,
        }
    }

    /// Czy panel jest poziomy (góra/dół).
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }

    /// Czy panel jest pionowy (lewa/prawa krawędź).
    pub fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }
}

/// Wygląd panelu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelLook {
    /// Klasyczny pasek Luny: pełna szerokość, płaskie tło.
    #[default]
    Luna,
    /// Pływający, zaokrąglony pasek z marginesem (styl macOS/Dock).
    MacOs,
}

impl PanelLook {
    pub const ALL: [PanelLook; 2] = [PanelLook::Luna, PanelLook::MacOs];

    pub fn key(self) -> &'static str {
        match self {
            Self::Luna => "luna",
            Self::MacOs => "macos",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "luna" | "luna-like" => Some(Self::Luna),
            "macos" | "mac" | "macos-like" | "dock" | "floating" => Some(Self::MacOs),
            _ => None,
        }
    }

    /// Czy wygląd jest dostępny w tej wersji Luny.
    ///
    /// Pływający pasek (dawniej „macOS”) jest już zaimplementowany w powłoce,
    /// więc można go wybrać bez czekania na globalne menu.
    pub fn available(self) -> bool {
        true
    }
}

/// Sposób pokazywania pozycji system traya.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStyle {
    /// Same ikony (kompaktowo, domyślnie).
    #[default]
    Icons,
    /// Ikona + nazwa aplikacji.
    IconsAndLabels,
}

impl TrayStyle {
    pub const ALL: [TrayStyle; 2] = [TrayStyle::Icons, TrayStyle::IconsAndLabels];

    pub fn key(self) -> &'static str {
        match self {
            Self::Icons => "icons",
            Self::IconsAndLabels => "labels",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "icons" | "ikony" | "icon" => Some(Self::Icons),
            "labels" | "nazwy" | "icons-and-labels" | "ikona-nazwa" | "both" => {
                Some(Self::IconsAndLabels)
            }
            _ => None,
        }
    }

    /// Czy rysować też nazwę obok ikony.
    pub fn labels(self) -> bool {
        matches!(self, Self::IconsAndLabels)
    }
}

/// Ustawienia panelu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSettings {
    pub position: PanelPosition,
    pub look: PanelLook,
    pub tray: TrayStyle,
}

impl Default for PanelSettings {
    fn default() -> Self {
        Self {
            position: PanelPosition::Top,
            look: PanelLook::Luna,
            tray: TrayStyle::Icons,
        }
    }
}

impl PanelSettings {
    pub fn config_path() -> Option<PathBuf> {
        config_file("panel.conf")
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
                "position" => {
                    if let Some(position) = PanelPosition::from_key(&value) {
                        out.position = position;
                    }
                }
                "look" => {
                    if let Some(look) = PanelLook::from_key(&value) {
                        out.look = look;
                    }
                }
                "tray" => {
                    if let Some(tray) = TrayStyle::from_key(&value) {
                        out.tray = tray;
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn render(&self) -> String {
        format!(
            "# Luna — panel\n\
             # position: top | bottom | left | right\n\
             # look: luna | macos\n\
             # tray: icons | labels\n\
             position = {}\n\
             look = {}\n\
             tray = {}\n",
            self.position.key(),
            self.look.key(),
            self.tray.key()
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
    fn defaults_to_a_top_luna_bar() {
        let settings = PanelSettings::default();
        assert_eq!(settings.position, PanelPosition::Top);
        assert_eq!(settings.look, PanelLook::Luna);
        assert_eq!(PanelSettings::parse(""), settings);
    }

    #[test]
    fn round_trips_all_positions() {
        for position in PanelPosition::ALL {
            for tray in TrayStyle::ALL {
                let settings = PanelSettings {
                    position,
                    look: PanelLook::MacOs,
                    tray,
                };
                assert_eq!(PanelSettings::parse(&settings.render()), settings);
            }
        }
    }

    #[test]
    fn accepts_friendly_aliases() {
        assert_eq!(
            PanelSettings::parse("position = dół\nlook = dock\n").position,
            PanelPosition::Bottom
        );
        assert_eq!(
            PanelSettings::parse("position = right\n").position,
            PanelPosition::Right
        );
        assert_eq!(
            PanelPosition::from_key("bok-lewy"),
            Some(PanelPosition::Left)
        );
        assert!(PanelPosition::Left.is_vertical());
        assert!(PanelPosition::Bottom.is_horizontal());
    }

    #[test]
    fn ignores_unknown_values() {
        let settings = PanelSettings::parse("position = diagonal\nlook = glass\n");
        assert_eq!(settings, PanelSettings::default());
    }
}
