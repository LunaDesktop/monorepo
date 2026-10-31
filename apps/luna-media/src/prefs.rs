//! Trwałe preferencje `luna-media` (`$XDG_CONFIG_HOME/luna/media.conf`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Preferencje przeglądarki.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    /// Czy dźwięk filmów jest domyślnie wyciszony.
    pub muted: bool,
    /// Odstęp pokazu slajdów w sekundach.
    pub slideshow_secs: u32,
    /// Ostatnio otwarty katalog (pusty = wybierz domyślny).
    pub last_dir: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            muted: false,
            slideshow_secs: 5,
            last_dir: String::new(),
        }
    }
}

impl Prefs {
    /// Ścieżka pliku konfiguracji.
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("luna").join("media.conf"))
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
            match key.trim() {
                "muted" => prefs.muted = matches!(value.trim(), "1" | "true" | "yes"),
                "slideshow_secs" => {
                    if let Ok(secs) = value.trim().parse::<u32>() {
                        prefs.slideshow_secs = secs.clamp(1, 120);
                    }
                }
                "last_dir" => prefs.last_dir = value.trim().to_owned(),
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
            "# luna-media\nmuted={}\nslideshow_secs={}\nlast_dir={}\n",
            u8::from(self.muted),
            self.slideshow_secs.clamp(1, 120),
            self.last_dir,
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
    fn parses_and_clamps_values() {
        let prefs = Prefs::parse(
            "# komentarz\nmuted=1\nslideshow_secs=0\nlast_dir=/home/ala/Obrazy\nnieznane=x\n",
        );
        assert!(prefs.muted);
        assert_eq!(prefs.slideshow_secs, 1);
        assert_eq!(prefs.last_dir, "/home/ala/Obrazy");

        let prefs = Prefs::parse("muted=false\nslideshow_secs=999\n");
        assert!(!prefs.muted);
        assert_eq!(prefs.slideshow_secs, 120);
    }

    #[test]
    fn roundtrips_through_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "luna-media-prefs-{}-{}",
            std::process::id(),
            crate::model::now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("media.conf");
        let prefs = Prefs {
            muted: true,
            slideshow_secs: 7,
            last_dir: "/tmp/zdjecia".to_owned(),
        };
        prefs.save_to(&path).unwrap();
        let loaded = Prefs::load_from(&path).unwrap();
        assert_eq!(loaded, prefs);
        let _ = fs::remove_dir_all(&dir);
    }
}
