//! Preferencje Luna Editor.
//!
//! Plik `$XDG_CONFIG_HOME/luna/editor.conf` (albo `~/.config/luna/editor.conf`)
//! ma prosty format `klucz=wartość` — można go edytować ręcznie.

use std::path::{Path, PathBuf};

/// Domyślna szerokość tabulatora w kolumnach.
pub const DEFAULT_TAB_WIDTH: usize = 4;
/// Wartości szerokości tabulatora dostępne w Ustawieniach.
pub const TAB_WIDTHS: [usize; 3] = [2, 4, 8];

/// Preferencje edytora.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefs {
    /// Szerokość tabulatora w kolumnach.
    pub tab_width: usize,
    /// Zachowuj wcięcie po Enterze.
    pub auto_indent: bool,
    /// Pokazuj pliki i katalogi ukryte w drzewie projektu.
    pub show_hidden: bool,
    /// Pomijaj `target` i `node_modules` w drzewie projektu.
    pub skip_build_dirs: bool,
    /// Pokazuj panel projektu przy starcie.
    pub show_workspace: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tab_width: DEFAULT_TAB_WIDTH,
            auto_indent: true,
            show_hidden: false,
            skip_build_dirs: true,
            show_workspace: true,
        }
    }
}

impl Prefs {
    /// Wczytuje preferencje użytkownika (brak pliku = wartości domyślne).
    pub fn load() -> Self {
        config_path()
            .filter(|path| path.exists())
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }

    /// Zapisuje preferencje; błędy dysku są ignorowane.
    pub fn save(&self) {
        if let Some(path) = config_path() {
            let _ = self.save_to(&path);
        }
    }

    /// Najbliższa dozwolona szerokość tabulatora.
    pub fn clamp_tab_width(width: usize) -> usize {
        *TAB_WIDTHS
            .iter()
            .min_by_key(|candidate| candidate.abs_diff(width))
            .unwrap_or(&DEFAULT_TAB_WIDTH)
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
                "tab_width" => {
                    if let Ok(width) = value.parse::<usize>() {
                        prefs.tab_width = Self::clamp_tab_width(width);
                    }
                }
                "auto_indent" => prefs.auto_indent = parse_bool(value, prefs.auto_indent),
                "show_hidden" => prefs.show_hidden = parse_bool(value, prefs.show_hidden),
                "skip_build_dirs" => {
                    prefs.skip_build_dirs = parse_bool(value, prefs.skip_build_dirs)
                }
                "show_workspace" => prefs.show_workspace = parse_bool(value, prefs.show_workspace),
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
            "# luna-editor\ntab_width={}\nauto_indent={}\nshow_hidden={}\nskip_build_dirs={}\nshow_workspace={}\n",
            self.tab_width,
            u8::from(self.auto_indent),
            u8::from(self.show_hidden),
            u8::from(self.skip_build_dirs),
            u8::from(self.show_workspace),
        );
        // Zapis przez plik tymczasowy: przerwany zapis nie zostawia pustej konfiguracji.
        let tmp = path.with_extension("conf.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }
}

fn parse_bool(value: &str, fallback: bool) -> bool {
    match value {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => fallback,
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join("luna/editor.conf"));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/luna/editor.conf"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-editor-prefs-{}-{}-{}",
            std::process::id(),
            name,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("editor.conf")
    }

    #[test]
    fn defaults_are_stable() {
        let prefs = Prefs::default();
        assert_eq!(prefs.tab_width, 4);
        assert!(prefs.auto_indent);
        assert!(!prefs.show_hidden);
        assert!(prefs.skip_build_dirs);
        assert!(prefs.show_workspace);
    }

    #[test]
    fn parses_known_keys_and_clamps_tab_width() {
        let path = temp_path("parse");
        std::fs::write(
            &path,
            "# komentarz\ntab_width=7\nauto_indent=0\nshow_hidden=1\nskip_build_dirs=no\nshow_workspace=false\nnieznane=1\n",
        )
        .unwrap();
        let prefs = Prefs::load_from(&path);
        assert_eq!(prefs.tab_width, 8);
        assert!(!prefs.auto_indent);
        assert!(prefs.show_hidden);
        assert!(!prefs.skip_build_dirs);
        assert!(!prefs.show_workspace);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unknown_values_fall_back_and_roundtrip() {
        let path = temp_path("roundtrip");
        std::fs::write(&path, "tab_width=abc\nauto_indent=xx\n").unwrap();
        let mut prefs = Prefs::load_from(&path);
        assert_eq!(prefs.tab_width, DEFAULT_TAB_WIDTH);
        assert!(prefs.auto_indent);
        prefs.tab_width = 2;
        prefs.auto_indent = false;
        prefs.show_hidden = true;
        prefs.skip_build_dirs = false;
        prefs.show_workspace = false;
        prefs.save_to(&path).unwrap();
        assert_eq!(Prefs::load_from(&path), prefs);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
