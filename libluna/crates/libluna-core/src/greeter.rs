//! Konfiguracja ekranu logowania (`luna-greeter`).
//!
//! Plik jest **systemowy** (`/etc/luna/greeter.conf`, prawa 0644), bo greeter
//! działa jako osobny użytkownik `greeter` i nie może czytać ustawień z katalogu
//! domowego osoby logującej się. Do zmiany ścieżki tapety wystarczy jeden klucz;
//! nie ma tu żadnych poleceń do wykonania.
//!
//! `LUNA_GREETER_CONFIG` pozwala wskazać inny plik (np. w `--preview` albo testach).
//! Ustawienia zapisuje `luna-greeter-config` przez `pkexec` (patrz Ustawienia).

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::{Accent, ThemePreference};

/// Systemowa lokalizacja konfiguracji ekranu logowania.
pub const SYSTEM_PATH: &str = "/etc/luna/greeter.conf";

/// Ustawienia ekranu logowania.
#[derive(Debug, Clone, PartialEq)]
pub struct GreeterSettings {
    /// Tapeta PNG (ścieżka absolutna); `None` = gradient akcentu.
    pub wallpaper: Option<PathBuf>,
    /// Motyw karty logowania.
    pub theme: ThemePreference,
    /// Akcent; `None` = akcent z ustawień motywu greetera.
    pub accent: Option<Accent>,
    /// Czy pokazywać zegar i datę w prawym górnym rogu.
    pub show_clock: bool,
}

impl Default for GreeterSettings {
    fn default() -> Self {
        Self {
            wallpaper: None,
            theme: ThemePreference::Dark,
            accent: None,
            show_clock: true,
        }
    }
}

impl GreeterSettings {
    /// Ścieżka konfiguracji: `LUNA_GREETER_CONFIG` albo systemowa.
    pub fn config_path() -> PathBuf {
        match std::env::var("LUNA_GREETER_CONFIG") {
            Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
            _ => PathBuf::from(SYSTEM_PATH),
        }
    }

    /// Wczytuje konfigurację (brak pliku / błędy → wartości domyślne).
    pub fn load() -> Self {
        Self::load_from(&Self::config_path())
    }

    /// Wczytuje konfigurację z jawnie podanej ścieżki.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Parsuje format `klucz=wartość` (nieznane klucze są ignorowane).
    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "wallpaper" if valid_wallpaper(value) => {
                    settings.wallpaper = Some(PathBuf::from(value));
                }
                "theme" => {
                    settings.theme = match value {
                        "light" => ThemePreference::Light,
                        "dark" => ThemePreference::Dark,
                        "system" => ThemePreference::System,
                        _ => settings.theme,
                    };
                }
                "accent" => {
                    if let Some(accent) = Accent::from_name(value) {
                        settings.accent = Some(accent);
                    }
                }
                "show_clock" => {
                    settings.show_clock = !matches!(value, "false" | "0" | "no");
                }
                _ => {}
            }
        }
        settings
    }

    /// Serializuje konfigurację do formatu akceptowanego przez [`Self::parse`].
    pub fn render(&self) -> String {
        let mut out = String::from(
            "# Luna Greeter — ekran logowania\n\
             # Wygenerowane przez Ustawienia Luny. Ścieżka tapety musi być absolutna.\n",
        );
        out.push_str(&format!(
            "theme={}\n",
            match self.theme {
                ThemePreference::Light => "light",
                ThemePreference::Dark => "dark",
                ThemePreference::System => "system",
            }
        ));
        if let Some(accent) = self.accent {
            out.push_str(&format!("accent={}\n", accent.name()));
        }
        out.push_str(&format!("show_clock={}\n", self.show_clock));
        if let Some(wallpaper) = &self.wallpaper {
            out.push_str(&format!("wallpaper={}\n", wallpaper.display()));
        }
        out
    }

    /// Zapisuje konfigurację atomowo (0664 — czytelna dla greetera).
    ///
    /// Wymaga uprawnień do katalogu docelowego; w normalnej sesji ustawienia
    /// wołają przez `pkexec` pomocnik `luna-greeter-config`.
    pub fn save_local(&self, path: &Path) -> std::io::Result<()> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".greeter.conf.{}.tmp", std::process::id()));
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
            file.write_all(self.render().as_bytes())?;
            file.sync_all().ok();
        }
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

fn valid_wallpaper(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value.contains(['\n', '\r', '\0'])
        && Path::new(value).is_absolute()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_all_keys() {
        let text = "\
# komentarz
theme=light
accent=purple
show_clock=false
wallpaper=/usr/share/wallpapers/luna.png
";
        let settings = GreeterSettings::parse(text);
        assert_eq!(settings.theme, ThemePreference::Light);
        assert_eq!(settings.accent, Some(Accent::Purple));
        assert!(!settings.show_clock);
        assert_eq!(
            settings.wallpaper.as_deref(),
            Some(Path::new("/usr/share/wallpapers/luna.png"))
        );
        let round = GreeterSettings::parse(&settings.render());
        assert_eq!(round, settings);
    }

    #[test]
    fn rejects_relative_and_control_wallpaper_paths() {
        for bad in ["wallpaper=relative.png", "wallpaper=a\nb.png", "wallpaper="] {
            assert!(GreeterSettings::parse(bad).wallpaper.is_none(), "{bad}");
        }
        assert!(GreeterSettings::parse("wallpaper=/ok.png")
            .wallpaper
            .is_some());
    }

    #[test]
    fn saves_atomically_with_readable_mode() {
        let dir = std::env::temp_dir().join(format!(
            "luna-greeter-settings-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path = dir.join("greeter.conf");
        let settings = GreeterSettings {
            wallpaper: Some(PathBuf::from("/usr/share/wallpapers/luna.png")),
            theme: ThemePreference::Dark,
            accent: Some(Accent::Teal),
            show_clock: true,
        };
        settings.save_local(&path).expect("zapis");
        assert_eq!(GreeterSettings::load_from(&path), settings);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        let _ = std::fs::remove_dir_all(dir);
    }
}
