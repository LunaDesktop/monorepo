//! Preferencje wyświetlaczy: tryb (rozdzielczość + odświeżanie) na złącze.
//!
//! Plik `~/.config/luna/displays.conf` czyta Selene przy starcie (wybiera tryb
//! z modesetu) oraz Ustawienia (pokazuje, co jest zapisane). Nie wymaga roota:
//! tryby są per użytkownik, a sam modeset robi kompozytor.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Tryb wyświetlania: rozdzielczość + odświeżanie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutputMode {
    pub width: u16,
    pub height: u16,
    pub refresh: u16,
}

impl OutputMode {
    pub fn new(width: u16, height: u16, refresh: u16) -> Self {
        Self {
            width,
            height,
            refresh,
        }
    }

    /// Czyta zapis `1920x1080@60` (albo bez `@Hz` — wtedy 0 = dowolne).
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (size, refresh) = match text.split_once('@') {
            Some((size, refresh)) => (size, refresh.trim_end_matches("Hz").parse::<u16>().ok()?),
            None => (text, 0),
        };
        let (width, height) = size.split_once(['x', 'X'])?;
        Some(Self {
            width: width.trim().parse().ok()?,
            height: height.trim().parse().ok()?,
            refresh,
        })
    }

    /// Zapis w pliku konfiguracji (`1920x1080@60`; bez `@` przy `refresh == 0`).
    pub fn config_value(self) -> String {
        if self.refresh == 0 {
            format!("{}x{}", self.width, self.height)
        } else {
            format!("{}x{}@{}", self.width, self.height, self.refresh)
        }
    }

    pub fn resolution(self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Etykieta dla UI, np. `1920 × 1080 @ 60 Hz`.
    pub fn label(self) -> String {
        if self.refresh == 0 {
            format!("{} × {}", self.width, self.height)
        } else {
            format!("{} × {} @ {} Hz", self.width, self.height, self.refresh)
        }
    }

    /// Czy tryb pasuje do zapisu użytkownika (`refresh == 0` = dowolne odświeżanie).
    pub fn matches(self, wanted: Self) -> bool {
        self.width == wanted.width
            && self.height == wanted.height
            && (wanted.refresh == 0 || self.refresh == wanted.refresh)
    }
}

/// Preferencje jednego złącza: tryb i pozycja w układzie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OutputPreference {
    /// Zapisany tryb (`None` = automatyczny/natywny).
    pub mode: Option<OutputMode>,
    /// Pozycja logiczna (lewy górny róg) w globalnej przestrzeni układu.
    pub position: Option<(i32, i32)>,
}

/// Preferencje wyświetlaczy (wszystkie pola opcjonalne).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DisplaySettings {
    outputs: BTreeMap<String, OutputPreference>,
    /// Siła ciepłej korekty (night light) 0–100, stosowana przy starcie sesji.
    pub night_light: u8,
}

impl DisplaySettings {
    pub fn path() -> Option<PathBuf> {
        // Nadpisanie dla testów/CI (nie wpływa na użytkownika).
        if let Some(path) = std::env::var_os("LUNA_DISPLAYS_CONF").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(path));
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("luna").join("displays.conf"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::other("brak katalogu konfiguracji"));
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_string())
    }

    /// Tryb zapisany dla złącza (`None` = automatyczny).
    pub fn mode_for(&self, output: &str) -> Option<OutputMode> {
        self.outputs
            .get(output)
            .and_then(|preference| preference.mode)
    }

    /// Zapisuje tryb (albo `None` = automatyczny) dla złącza.
    pub fn set_mode(&mut self, output: &str, mode: Option<OutputMode>) {
        self.outputs.entry(output.to_owned()).or_default().mode = mode;
    }

    /// Pozycja zapisana dla złącza (lewy górny róg w układzie).
    pub fn position_for(&self, output: &str) -> Option<(i32, i32)> {
        self.outputs
            .get(output)
            .and_then(|preference| preference.position)
    }

    /// Zapisuje pozycję złącza (`None` = automatyczny układ poziomy).
    pub fn set_position(&mut self, output: &str, position: Option<(i32, i32)>) {
        self.outputs.entry(output.to_owned()).or_default().position = position;
    }

    /// Złącza z jawną preferencją (także „auto”).
    pub fn outputs(&self) -> impl Iterator<Item = (&str, OutputPreference)> {
        self.outputs
            .iter()
            .map(|(name, preference)| (name.as_str(), *preference))
    }

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
            let (key, value) = (key.trim(), value.trim());
            if key == "night_light" {
                if let Ok(percent) = value.parse::<u8>() {
                    settings.night_light = percent.min(100);
                }
                continue;
            }
            let Some(rest) = key.strip_prefix("output ") else {
                continue;
            };
            let Some((name, field)) = rest.rsplit_once(' ') else {
                continue;
            };
            let name = name.trim().trim_matches('"');
            if name.is_empty() {
                continue;
            }
            match field.trim() {
                "mode" => {
                    let mode = if value.eq_ignore_ascii_case("auto") || value.is_empty() {
                        None
                    } else {
                        let Some(mode) = OutputMode::parse(value) else {
                            continue;
                        };
                        Some(mode)
                    };
                    settings.outputs.entry(name.to_owned()).or_default().mode = mode;
                }
                "position" => {
                    let Some(position) = parse_position(value) else {
                        continue;
                    };
                    settings
                        .outputs
                        .entry(name.to_owned())
                        .or_default()
                        .position = Some(position);
                }
                _ => {}
            }
        }
        settings
    }
}

/// Czyta `1920,0` / `-1600, 200` (pozycja logiczna).
fn parse_position(value: &str) -> Option<(i32, i32)> {
    let (x, y) = value.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

impl std::fmt::Display for DisplaySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "# Wyświetlacze Luny — zapisuje centrum ustawień")?;
        writeln!(f, "night_light = {}", self.night_light)?;
        for (name, preference) in &self.outputs {
            let mode = preference
                .mode
                .map(|mode| mode.config_value())
                .unwrap_or_else(|| "auto".to_owned());
            writeln!(f, "output {name} mode = {mode}")?;
            if let Some((x, y)) = preference.position {
                writeln!(f, "output {name} position = {x},{y}")?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing_and_round_trip() {
        assert_eq!(
            OutputMode::parse("1920x1080@60"),
            Some(OutputMode::new(1920, 1080, 60))
        );
        assert_eq!(
            OutputMode::parse("2560X1440@144Hz"),
            Some(OutputMode::new(2560, 1440, 144))
        );
        assert_eq!(
            OutputMode::parse("1600x900"),
            Some(OutputMode::new(1600, 900, 0))
        );
        assert!(OutputMode::parse("natywny").is_none());
        assert!(OutputMode::parse("1920x").is_none());
        assert_eq!(
            OutputMode::new(1920, 1080, 60).config_value(),
            "1920x1080@60"
        );
        assert_eq!(OutputMode::new(1920, 1080, 0).config_value(), "1920x1080");
        assert_eq!(
            OutputMode::new(1920, 1080, 60).label(),
            "1920 × 1080 @ 60 Hz"
        );
    }

    #[test]
    fn mode_matching_ignores_refresh_when_zero() {
        let wanted = OutputMode::new(1920, 1080, 0);
        assert!(OutputMode::new(1920, 1080, 60).matches(wanted));
        assert!(OutputMode::new(1920, 1080, 60).matches(OutputMode::new(1920, 1080, 60)));
        assert!(!OutputMode::new(1920, 1080, 60).matches(OutputMode::new(1920, 1080, 144)));
        assert!(!OutputMode::new(2560, 1440, 60).matches(wanted));
    }

    #[test]
    fn position_round_trip() {
        let text = "\
output HDMI-A-1 position = 1920,0
output DP-1 position = -1600, 200
output eDP-1 position = nonsense
";
        let settings = DisplaySettings::parse(text);
        assert_eq!(settings.position_for("HDMI-A-1"), Some((1920, 0)));
        assert_eq!(settings.position_for("DP-1"), Some((-1600, 200)));
        assert_eq!(settings.position_for("eDP-1"), None);

        let round_trip = DisplaySettings::parse(&settings.to_string());
        assert_eq!(round_trip, settings);

        let mut manual = DisplaySettings::default();
        manual.set_mode("X", Some(OutputMode::new(1920, 1080, 60)));
        manual.set_position("X", Some((0, 0)));
        let text = manual.to_string();
        assert!(text.contains("output X mode = 1920x1080@60"), "{text}");
        assert!(text.contains("output X position = 0,0"), "{text}");
    }

    #[test]
    fn parse_and_display_round_trip() {
        let text = "\
# komentarz
night_light = 25
output HDMI-A-1 mode = 1920x1080@60
output eDP-1 mode = auto
output \"DP-2\" mode = 2560x1440@144
";
        let settings = DisplaySettings::parse(text);
        assert_eq!(settings.night_light, 25);
        assert_eq!(
            settings.mode_for("HDMI-A-1"),
            Some(OutputMode::new(1920, 1080, 60))
        );
        assert_eq!(settings.mode_for("eDP-1"), None);
        assert_eq!(
            settings.mode_for("DP-2"),
            Some(OutputMode::new(2560, 1440, 144))
        );

        let round_trip = DisplaySettings::parse(&settings.to_string());
        assert_eq!(round_trip, settings);
    }

    #[test]
    fn invalid_lines_are_ignored() {
        let settings = DisplaySettings::parse(
            "night_light = 250\noutput  mode = 1920x1080@60\noutput X mode = nonsense\n",
        );
        assert_eq!(settings.night_light, 100);
        assert_eq!(settings.outputs().count(), 0);
    }

    #[test]
    fn set_mode_overrides_and_clears() {
        let mut settings = DisplaySettings::default();
        settings.set_mode("eDP-1", Some(OutputMode::new(1920, 1200, 60)));
        assert!(settings.mode_for("eDP-1").is_some());
        settings.set_mode("eDP-1", None);
        assert!(settings.mode_for("eDP-1").is_none());
        assert_eq!(settings.outputs().count(), 1);
    }

    #[test]
    fn save_and_load_round_trip_under_override() {
        let path = std::env::temp_dir().join(format!("luna-displays-{}.conf", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::env::set_var("LUNA_DISPLAYS_CONF", &path);

        let mut settings = DisplaySettings::default();
        settings.set_mode("HDMI-A-1", Some(OutputMode::new(2560, 1440, 144)));
        settings.night_light = 35;
        settings.save().expect("save");

        let loaded = DisplaySettings::load();
        assert_eq!(loaded, settings);

        std::env::remove_var("LUNA_DISPLAYS_CONF");
        let _ = std::fs::remove_file(&path);
    }
}
