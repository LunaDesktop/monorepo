//! Trwałe preferencje `luna-tasks` (`$XDG_CONFIG_HOME/luna/tasks.conf`).
//!
//! Zapisujemy tylko sposób prezentacji listy (sortowanie i zakres). Nic o
//! procesach nie jest nigdzie zapisywane — po zamknięciu aplikacji nie zostaje
//! historia uruchomień ani nazwy poleceń.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use libluna_core::config::{config_file, parse_pairs, save_atomic};

use crate::app::SortKey;

/// Preferencje menedżera zadań.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prefs {
    pub sort: SortKey,
    pub sort_asc: bool,
    pub only_mine: bool,
    /// Czy pokazywać wykresy CPU/RAM/procesu nad listą.
    pub show_charts: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            sort: SortKey::Cpu,
            sort_asc: false,
            only_mine: false,
            show_charts: false,
        }
    }
}

impl Prefs {
    /// Ścieżka pliku konfiguracji.
    pub fn path() -> Option<PathBuf> {
        config_file("tasks.conf")
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
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "sort" => {
                    if let Some(sort) = SortKey::from_name(&value) {
                        prefs.sort = sort;
                    }
                }
                "order" => prefs.sort_asc = value.eq_ignore_ascii_case("asc"),
                "scope" => prefs.only_mine = value.eq_ignore_ascii_case("mine"),
                "charts" => prefs.show_charts = value.eq_ignore_ascii_case("on"),
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
        let text = format!(
            "# luna-tasks\nsort={}\norder={}\nscope={}\ncharts={}\n",
            self.sort.name(),
            if self.sort_asc { "asc" } else { "desc" },
            if self.only_mine { "mine" } else { "all" },
            if self.show_charts { "on" } else { "off" },
        );
        save_atomic(path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_values_and_ignores_unknown() {
        let prefs = Prefs::parse(
            "# komentarz\nsort=memory\norder=asc\nscope=mine\ncharts=on\nnieznane=x\n",
        );
        assert_eq!(prefs.sort, SortKey::Memory);
        assert!(prefs.sort_asc);
        assert!(prefs.only_mine);
        assert!(prefs.show_charts);

        let prefs = Prefs::parse("sort=cokolwiek\nscope=all\ncharts=off\n");
        assert_eq!(prefs.sort, SortKey::Cpu);
        assert!(!prefs.sort_asc);
        assert!(!prefs.only_mine);
        assert!(!prefs.show_charts);
    }

    #[test]
    fn roundtrips_through_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "luna-tasks-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("tasks.conf");
        let prefs = Prefs {
            sort: SortKey::Name,
            sort_asc: true,
            only_mine: true,
            show_charts: true,
        };
        prefs.save_to(&path).unwrap();
        assert_eq!(Prefs::load_from(&path).unwrap(), prefs);
        let _ = fs::remove_dir_all(&dir);
    }
}
