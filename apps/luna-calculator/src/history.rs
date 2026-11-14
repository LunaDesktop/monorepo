//! Lokalna historia obliczeń (`$XDG_DATA_HOME/luna/calculator/history.tsv`).

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Maksymalna liczba zapamiętanych obliczeń.
pub const MAX_ENTRIES: usize = 100;

/// Jedno obliczenie w historii.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub expression: String,
    pub result: String,
}

/// Historia obliczeń. `path = None` oznacza tryb pamięci (testy/podglądy).
#[derive(Debug, Clone, Default)]
pub struct History {
    entries: Vec<Entry>,
    path: Option<PathBuf>,
}

impl History {
    /// Historia tylko w pamięci — nic nie zapisuje.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Wczytuje historię użytkownika; brak pliku daje pustą historię.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::empty();
        };
        let entries = Self::load_from(&path).unwrap_or_default();
        Self {
            entries,
            path: Some(path),
        }
    }

    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })?;
        Some(base.join("luna").join("calculator").join("history.tsv"))
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Dopisuje obliczenie; powtórka dokładnie tego samego wyniku nie tworzy duplikatu.
    pub fn add(&mut self, expression: &str, result: &str) {
        let expression = expression.trim();
        if expression.is_empty() || result.trim().is_empty() {
            return;
        }
        if self
            .entries
            .last()
            .is_some_and(|entry| entry.expression == expression && entry.result == result)
        {
            return;
        }
        self.entries.push(Entry {
            expression: expression.to_owned(),
            result: result.to_owned(),
        });
        if self.entries.len() > MAX_ENTRIES {
            let drop = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(0..drop);
        }
        self.save();
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.save();
    }

    fn load_from(path: &Path) -> io::Result<Vec<Entry>> {
        let text = fs::read_to_string(path)?;
        let mut entries: Vec<Entry> = text
            .lines()
            .filter_map(|line| {
                let (expression, result) = line.split_once('\t')?;
                let expression = expression.trim();
                let result = result.trim();
                if expression.is_empty() || result.is_empty() {
                    return None;
                }
                Some(Entry {
                    expression: expression.to_owned(),
                    result: result.to_owned(),
                })
            })
            .collect();
        if entries.len() > MAX_ENTRIES {
            let drop = entries.len() - MAX_ENTRIES;
            entries.drain(0..drop);
        }
        Ok(entries)
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let _ = self.save_to(path);
    }

    fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut text = String::new();
        for entry in &self.entries {
            text.push_str(&entry.expression);
            text.push('\t');
            text.push_str(&entry.result);
            text.push('\n');
        }
        let tmp = path.with_extension("tsv.tmp");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            // Historia może zawierać prywatne kwoty — tylko dla właściciela.
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "luna-calculator-history-{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    #[test]
    fn adds_deduplicates_and_caps_entries() {
        let mut history = History::empty();
        history.add("2+2", "4");
        history.add("2+2", "4");
        assert_eq!(history.len(), 1);
        for i in 0..(MAX_ENTRIES + 5) {
            history.add(&format!("{i}+1"), &format!("{}", i + 1));
        }
        assert_eq!(history.len(), MAX_ENTRIES);
        assert_eq!(history.entries().last().unwrap().expression, "104+1");
    }

    #[test]
    fn roundtrips_through_a_file() {
        let path = temp_path("roundtrip");
        let mut history = History::empty();
        history.add("2+2", "4");
        history.add("sin(30)", "0.5");
        history.save_to(&path).unwrap();

        let loaded = History::load_from(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[1].expression, "sin(30)");
        assert_eq!(loaded[1].result, "0.5");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn clear_removes_entries() {
        let mut history = History::empty();
        history.add("2+2", "4");
        history.clear();
        assert!(history.is_empty());
    }
}
