//! Wbudowany manager plików edytora (otwieranie, zapis, wybór katalogu workspace).
//!
//! To nie jest osobna aplikacja — lekki modal na `std::fs`, z listą katalogów,
//! miejscami XDG i polem nazwy pliku. Dzięki temu „Otwórz” i „Zapisz jako”
//! nie wymagają przepisywania ścieżki z pamięci.

use std::path::{Path, PathBuf};

use libluna_i18n::tr;
use libluna_widgets::Entry;

/// Maksymalna liczba pozycji w jednym katalogu.
const MAX_ENTRIES: usize = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserMode {
    Open,
    Save,
    Folder,
}

#[derive(Debug, Clone)]
pub struct BrowserEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Wynik zamknięcia przeglądarki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserOutcome {
    Open(PathBuf),
    Save(PathBuf),
    Folder(PathBuf),
}

pub struct FileBrowser {
    pub mode: BrowserMode,
    pub dir: PathBuf,
    pub entries: Vec<BrowserEntry>,
    pub selected: Option<usize>,
    pub scroll: f64,
    pub filename: Entry,
    pub error: Option<String>,
}

impl FileBrowser {
    pub fn new(mode: BrowserMode, start: PathBuf) -> Self {
        let dir = if start.is_dir() {
            start.clone()
        } else if let Some(parent) = start.parent().filter(|parent| parent.is_dir()) {
            parent.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        };
        let mut filename = Entry::new(tr("File name"));
        if mode == BrowserMode::Save {
            if let Some(name) = start.file_name().and_then(|name| name.to_str()) {
                filename.set_text(name);
            }
            filename.set_focused(true);
        }
        let mut browser = Self {
            mode,
            dir,
            entries: Vec::new(),
            selected: None,
            scroll: 0.0,
            filename,
            error: None,
        };
        browser.reload();
        if mode == BrowserMode::Open && start.is_file() {
            if let Some(name) = start.file_name().and_then(|name| name.to_str()) {
                browser.filename.set_text(name);
            }
            browser.selected = browser.entries.iter().position(|entry| entry.path == start);
        }
        browser
    }

    pub fn title(&self) -> String {
        match self.mode {
            BrowserMode::Open => tr("Open file"),
            BrowserMode::Save => tr("Save as…"),
            BrowserMode::Folder => tr("Open folder"),
        }
    }

    pub fn action_label(&self) -> String {
        match self.mode {
            BrowserMode::Open => tr("Open"),
            BrowserMode::Save => tr("Save"),
            BrowserMode::Folder => tr("Choose folder"),
        }
    }

    pub fn reload(&mut self) {
        self.entries.clear();
        self.error = None;
        match std::fs::read_dir(&self.dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let path = entry.path();
                    let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                    self.entries.push(BrowserEntry { name, path, is_dir });
                }
                self.entries.sort_by(|a, b| {
                    b.is_dir
                        .cmp(&a.is_dir)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                self.entries.truncate(MAX_ENTRIES);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        if let Some(index) = self.selected.filter(|index| *index < self.entries.len()) {
            self.selected = Some(index);
        } else {
            self.selected = None;
        }
        self.scroll = 0.0;
    }

    pub fn navigate(&mut self, dir: PathBuf) {
        if !dir.is_dir() {
            return;
        }
        self.dir = dir;
        self.selected = None;
        self.filename.clear();
        self.reload();
    }

    pub fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            self.navigate(parent);
        }
    }

    pub fn select(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        self.selected = Some(index);
        self.filename.set_text(entry.name.clone());
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.entries.len() as isize - 1) as usize;
        self.select(next);
    }

    /// Klik/Enter na zaznaczonej pozycji: katalog otwiera, plik wybiera.
    pub fn activate_selected(&mut self) -> Option<BrowserOutcome> {
        let index = self.selected?;
        let entry = self.entries.get(index)?.clone();
        if entry.is_dir {
            self.navigate(entry.path);
            return None;
        }
        self.filename.set_text(entry.name);
        match self.mode {
            BrowserMode::Open => Some(BrowserOutcome::Open(entry.path)),
            BrowserMode::Save => None,
            BrowserMode::Folder => None,
        }
    }

    /// Zatwierdzenie przyciskiem (pole nazwy ma pierwszeństwo).
    pub fn submit(&mut self) -> Option<BrowserOutcome> {
        self.error = None;
        match self.mode {
            BrowserMode::Folder => Some(BrowserOutcome::Folder(self.dir.clone())),
            BrowserMode::Open => {
                let value = self.filename.text().trim().to_owned();
                let path = if value.is_empty() {
                    let index = self.selected?;
                    let entry = self.entries.get(index)?;
                    if entry.is_dir {
                        self.navigate(entry.path.clone());
                        return None;
                    }
                    entry.path.clone()
                } else {
                    self.resolve(&value)
                };
                if path.is_dir() {
                    self.navigate(path);
                    None
                } else if path.is_file() {
                    Some(BrowserOutcome::Open(path))
                } else {
                    self.error = Some(tr("File does not exist"));
                    None
                }
            }
            BrowserMode::Save => {
                let value = self.filename.text().trim().to_owned();
                if value.is_empty() {
                    self.error = Some(tr("Enter a file name"));
                    return None;
                }
                let path = self.resolve(&value);
                if path.is_dir() {
                    self.error = Some(tr("This is a directory"));
                    return None;
                }
                Some(BrowserOutcome::Save(path))
            }
        }
    }

    fn resolve(&self, value: &str) -> PathBuf {
        if value.starts_with('~') || Path::new(value).is_absolute() {
            crate::document::expand_path(value)
        } else {
            self.dir.join(value)
        }
    }

    pub fn selected_is_file(&self) -> bool {
        self.selected
            .and_then(|index| self.entries.get(index))
            .is_some_and(|entry| !entry.is_dir)
    }
}

/// Miejsca XDG pokazywane w lewej kolumnie przeglądarki.
pub fn places() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return vec![("/".to_owned(), PathBuf::from("/"))];
    };
    out.push((tr("Home"), home.clone()));
    for (key, sub) in [
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
        ("Pictures", "Pictures"),
        ("Desktop", "Desktop"),
    ] {
        let path = home.join(sub);
        if path.is_dir() {
            out.push((tr(key), path));
        }
    }
    out.push(("/".to_owned(), PathBuf::from("/")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-browser-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        dir
    }

    #[test]
    fn lists_directories_first_and_navigates() {
        let dir = temp_dir();
        let mut browser = FileBrowser::new(BrowserMode::Open, dir.join("a.txt"));
        assert!(browser.entries[0].is_dir);
        assert!(browser.selected_is_file());
        browser.navigate(dir.join("sub"));
        assert_eq!(browser.dir, dir.join("sub"));
        browser.up();
        assert_eq!(browser.dir, dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_requires_a_name_and_returns_path() {
        let dir = temp_dir();
        let mut browser = FileBrowser::new(BrowserMode::Save, dir.join("new.rs"));
        browser.filename.set_text("out.rs");
        assert_eq!(
            browser.submit(),
            Some(BrowserOutcome::Save(dir.join("out.rs")))
        );
        browser.filename.set_text("");
        assert!(browser.submit().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_stops_at_missing_file() {
        let dir = temp_dir();
        let mut browser = FileBrowser::new(BrowserMode::Open, dir.clone());
        browser.filename.set_text("nie-ma.txt");
        assert!(browser.submit().is_none());
        assert!(browser.error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
