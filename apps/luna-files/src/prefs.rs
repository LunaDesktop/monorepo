//! Preferencje `luna-files` (język, domyślny widok, ukryte pliki).
//!
//! Trzymamy je w `$XDG_CONFIG_HOME/luna/files.conf` (albo `~/.config/...`).
//! Format jest celowo prosty — `klucz=wartość`, jedna para na linię — żeby plik
//! dało się edytować ręcznie i żeby nie ciągnąć do aplikacji formatu serializacji.

use std::path::{Path, PathBuf};

use libluna_widgets::ViewMode;

/// Języki, które aplikacja potrafi wybrać w Ustawieniach.
pub const LANGUAGES: [&str; 3] = ["pl", "en", "de"];

/// Preferencje aplikacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    pub language: String,
    pub view: ViewMode,
    pub show_hidden: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            language: libluna_i18n::locale().language().to_owned(),
            view: ViewMode::List,
            show_hidden: false,
        }
    }
}

impl Prefs {
    /// Wczytuje preferencje użytkownika (brak pliku = wartości domyślne).
    pub fn load() -> Self {
        config_path()
            .filter(|p| p.exists())
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    /// Zapisuje preferencje; błędy (np. brak miejsca) są ignorowane — aplikacja
    /// nie może się wywrócić tylko dlatego, że nie da się zapisać konfiguracji.
    pub fn save(&self) {
        if let Some(path) = config_path() {
            let _ = self.save_to(&path);
        }
    }

    fn load_from(path: &Path) -> Self {
        let mut prefs = Self::default();
        let Ok(text) = std::fs::read_to_string(path) else {
            return prefs;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "language" => {
                    if LANGUAGES.contains(&value) {
                        prefs.language = value.to_owned();
                    }
                }
                "view" => {
                    prefs.view = if value.eq_ignore_ascii_case("grid") {
                        ViewMode::Grid
                    } else {
                        ViewMode::List
                    };
                }
                "show_hidden" => prefs.show_hidden = matches!(value, "1" | "true" | "yes"),
                _ => {}
            }
        }
        prefs
    }

    fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = format!(
            "# luna-files\nlanguage={}\nview={}\nshow_hidden={}\n",
            self.language,
            match self.view {
                ViewMode::List => "list",
                ViewMode::Grid => "grid",
            },
            if self.show_hidden { "true" } else { "false" }
        );
        // Zapis przez plik tymczasowy: przerwany zapis nie zostawia pustej konfiguracji.
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

/// Indeks języka w [`LANGUAGES`] (dla kontrolki w Ustawieniach).
pub fn language_index(code: &str) -> usize {
    LANGUAGES.iter().position(|l| *l == code).unwrap_or(0)
}

/// Ścieżka pliku konfiguracji.
fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join("luna/files.conf"));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/luna/files.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_known_keys() {
        let dir = std::env::temp_dir().join("luna-files-prefs-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("files.conf");
        std::fs::write(
            &path,
            "# komentarz\nlanguage=de\nview=grid\nshow_hidden=true\nnieznane=1\n",
        )
        .unwrap();
        let prefs = Prefs::load_from(&path);
        assert_eq!(prefs.language, "de");
        assert_eq!(prefs.view, ViewMode::Grid);
        assert!(prefs.show_hidden);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unsupported_language_and_roundtrips() {
        let dir = std::env::temp_dir().join("luna-files-prefs-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("files.conf");
        std::fs::write(&path, "language=xx\nview=grid\n").unwrap();
        let mut prefs = Prefs::load_from(&path);
        assert!(LANGUAGES.contains(&prefs.language.as_str()));
        prefs.language = "pl".to_owned();
        prefs.show_hidden = true;
        prefs.save_to(&path).unwrap();
        let again = Prefs::load_from(&path);
        assert_eq!(again, prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_index_maps_codes() {
        assert_eq!(language_index("pl"), 0);
        assert_eq!(language_index("en"), 1);
        assert_eq!(language_index("de"), 2);
        assert_eq!(language_index("nieznany"), 0);
    }
}
