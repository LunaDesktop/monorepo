//! Trwałe preferencje `luna-calculator` (`$XDG_CONFIG_HOME/luna/calculator.conf`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::app::Mode;
use crate::eval::AngleMode;

/// Preferencje kalkulatora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    /// Jednostka kąta dla funkcji trygonometrycznych.
    pub angle: AngleMode,
    /// Tryb, w którym aplikacja startuje.
    pub start_mode: Mode,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            angle: AngleMode::Degrees,
            start_mode: Mode::Standard,
        }
    }
}

impl Prefs {
    /// Ścieżka pliku konfiguracji.
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("luna").join("calculator.conf"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| Self::load_from(&path).ok())
            .unwrap_or_default()
    }

    fn load_from(path: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(path)?;
        Ok(Self::parse(&text))
    }

    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_ascii_lowercase();
            match key.trim() {
                "angle" => {
                    prefs.angle = match value.as_str() {
                        "radians" | "rad" => AngleMode::Radians,
                        _ => AngleMode::Degrees,
                    };
                }
                "mode" => {
                    prefs.start_mode = match value.as_str() {
                        "scientific" | "naukowy" => Mode::Scientific,
                        _ => Mode::Standard,
                    };
                }
                _ => {}
            }
        }
        prefs
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else {
            return;
        };
        let _ = self.save_to(&path);
    }

    fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = format!(
            "# luna-calculator\nangle={}\nmode={}\n",
            match self.angle {
                AngleMode::Degrees => "degrees",
                AngleMode::Radians => "radians",
            },
            match self.start_mode {
                Mode::Standard => "standard",
                Mode::Scientific => "scientific",
            }
        );
        // Zapis przez plik tymczasowy — przerwanie nie zostawia pustej konfiguracji.
        let tmp = path.with_extension("conf.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_values_and_ignores_unknown() {
        let prefs = Prefs::parse("# komentarz\nangle=radians\nmode=scientific\nnieznane=wartosc\n");
        assert_eq!(prefs.angle, AngleMode::Radians);
        assert_eq!(prefs.start_mode, Mode::Scientific);

        let prefs = Prefs::parse("angle=stopnie\nmode=cos\n");
        assert_eq!(prefs.angle, AngleMode::Degrees);
        assert_eq!(prefs.start_mode, Mode::Standard);
    }

    #[test]
    fn roundtrips_through_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "luna-calculator-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("calculator.conf");
        let prefs = Prefs {
            angle: AngleMode::Radians,
            start_mode: Mode::Scientific,
        };
        prefs.save_to(&path).unwrap();
        assert_eq!(Prefs::load_from(&path).unwrap(), prefs);
        let _ = fs::remove_dir_all(&dir);
    }
}
