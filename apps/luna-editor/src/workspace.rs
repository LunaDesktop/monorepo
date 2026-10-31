//! Workspace: drzewo plików projektu i podgląd zaznaczonego pliku.
//!
//! Drzewo jest celowo proste (bez watcherów): przebudowujemy je po zmianie
//! katalogu, rozwinięciu folderu albo zapisie pliku. Limit wpisów chroni UI
//! przed ogromnymi repozytoriami.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::syntax::Language;

/// Maksymalna liczba wierszy drzewa.
const MAX_ROWS: usize = 600;
/// Ile linii pokazujemy w podglądzie.
const PREVIEW_LINES: usize = 9;
/// Maksymalny rozmiar pliku do podglądu.
const PREVIEW_LIMIT: u64 = 512 * 1024;

/// Jeden wiersz drzewa workspace.
#[derive(Debug, Clone)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub open: bool,
}

/// Podgląd zaznaczonego pliku.
#[derive(Debug, Clone)]
pub struct Preview {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub language: Language,
    pub size: u64,
    pub lines: Vec<String>,
    pub error: Option<String>,
}

impl Preview {
    pub fn size_label(&self) -> String {
        if self.is_dir {
            return String::new();
        }
        let size = self.size as f64;
        if size < 1024.0 {
            format!("{} B", self.size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} kB", size / 1024.0)
        } else {
            format!("{:.1} MB", size / (1024.0 * 1024.0))
        }
    }
}

/// Stan panelu workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub visible: bool,
    pub root: PathBuf,
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub preview: Option<Preview>,
    pub scroll: f64,
    pub rows: Vec<Row>,
    /// Pokazuj pliki i katalogi rozpoczynające się od kropki.
    pub show_hidden: bool,
    /// Pomijaj `target` i `node_modules` w drzewie.
    pub skip_build_dirs: bool,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        let mut workspace = Self {
            visible: true,
            root: root.clone(),
            expanded: HashSet::new(),
            selected: None,
            preview: None,
            scroll: 0.0,
            rows: Vec::new(),
            show_hidden: false,
            skip_build_dirs: true,
        };
        workspace.expanded.insert(root);
        workspace.rebuild();
        workspace
    }

    pub fn set_root(&mut self, root: PathBuf) {
        if !root.is_dir() {
            return;
        }
        self.root = root.clone();
        self.expanded.clear();
        self.expanded.insert(root);
        self.selected = None;
        self.preview = None;
        self.scroll = 0.0;
        self.rebuild();
    }

    pub fn root_name(&self) -> String {
        self.root
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| self.root.display().to_string())
    }

    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    /// Ustawia opcje widoku drzewa i przebudowuje je, gdy coś się zmieniło.
    pub fn configure(&mut self, show_hidden: bool, skip_build_dirs: bool) {
        if self.show_hidden == show_hidden && self.skip_build_dirs == skip_build_dirs {
            return;
        }
        self.show_hidden = show_hidden;
        self.skip_build_dirs = skip_build_dirs;
        self.rebuild();
    }

    /// Przebudowuje widoczne wiersze na podstawie rozwiniętych katalogów.
    pub fn rebuild(&mut self) {
        self.rows.clear();
        let root = self.root.clone();
        self.walk(&root, 0);
        // Pozycja przewijania jest w pikselach — UI przycina ją do widoku,
        // więc zwijanie katalogu nie może zrzucać listy na samą górę.
    }

    fn walk(&mut self, dir: &Path, depth: usize) {
        if self.rows.len() >= MAX_ROWS {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut items: Vec<(PathBuf, String, bool)> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !self.show_hidden && name.starts_with('.') {
                    return None;
                }
                if self.skip_build_dirs && matches!(name.as_str(), "target" | "node_modules") {
                    return None;
                }
                let path = entry.path();
                let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
                Some((path, name, is_dir))
            })
            .collect();
        items.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
        for (path, name, is_dir) in items {
            if self.rows.len() >= MAX_ROWS {
                return;
            }
            let open = is_dir && self.expanded.contains(&path);
            self.rows.push(Row {
                path: path.clone(),
                name,
                depth,
                is_dir,
                open,
            });
            if open {
                self.walk(&path, depth + 1);
            }
        }
    }

    /// Rozwija/zwija katalog.
    pub fn toggle_dir(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
        }
        self.rebuild();
    }

    /// Zaznacza plik i buduje podgląd.
    pub fn select(&mut self, path: PathBuf) {
        self.preview = Some(preview_of(&path));
        self.selected = Some(path);
    }

    /// Katalog, w którym powstaną nowe pliki i foldery.
    ///
    /// Zaznaczony katalog jest celem, plik — jego katalogiem nadrzędnym,
    /// a brak zaznaczenia oznacza katalog główny projektu.
    pub fn target_dir(&self) -> PathBuf {
        match self.selected.as_ref() {
            Some(path) if path.is_dir() => path.clone(),
            Some(path) => path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root.clone()),
            None => self.root.clone(),
        }
    }

    /// Czy ścieżka należy do drzewa projektu.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }

    /// Rozwija katalog, zaznacza go i odświeża drzewo.
    pub fn focus_dir(&mut self, path: &Path) {
        self.expanded.insert(path.to_path_buf());
        self.rebuild();
        self.select(path.to_path_buf());
    }

    /// Rozwija katalogi nad plikiem, przewija do niego i zaznacza go.
    pub fn reveal(&mut self, path: &Path) {
        if !path.starts_with(&self.root) {
            return;
        }
        let mut current = path.parent();
        while let Some(dir) = current {
            if dir.starts_with(&self.root) {
                self.expanded.insert(dir.to_path_buf());
            }
            if dir == self.root {
                break;
            }
            current = dir.parent();
        }
        self.rebuild();
        self.select(path.to_path_buf());
    }

    /// Odświeża drzewo i podgląd (np. po zapisie pliku).
    pub fn refresh(&mut self) {
        self.rebuild();
        if let Some(selected) = self.selected.clone() {
            if selected.exists() {
                self.preview = Some(preview_of(&selected));
            } else {
                self.selected = None;
                self.preview = None;
            }
        }
    }

    pub fn row_at(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.rows.iter().position(|row| &row.path == selected)
    }
}

fn preview_of(path: &Path) -> Preview {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    let metadata = std::fs::metadata(path).ok();
    let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let language = if is_dir {
        Language::Plain
    } else {
        Language::from_path(path)
    };
    if is_dir {
        return Preview {
            path: path.to_path_buf(),
            name,
            is_dir,
            language,
            size: 0,
            lines: Vec::new(),
            error: None,
        };
    }
    if size > PREVIEW_LIMIT {
        return Preview {
            path: path.to_path_buf(),
            name,
            is_dir,
            language,
            size,
            lines: Vec::new(),
            error: Some(libluna_i18n::tr("File is too large to preview")),
        };
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Preview {
                path: path.to_path_buf(),
                name,
                is_dir,
                language,
                size,
                lines: Vec::new(),
                error: Some(error.to_string()),
            }
        }
    };
    if bytes.contains(&0) {
        return Preview {
            path: path.to_path_buf(),
            name,
            is_dir,
            language,
            size,
            lines: Vec::new(),
            error: Some(libluna_i18n::tr("Binary file")),
        };
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines = text
        .lines()
        .take(PREVIEW_LINES)
        .map(|line| {
            let line: String = line.chars().take(160).collect();
            line.replace('\t', "    ")
        })
        .collect();
    Preview {
        path: path.to_path_buf(),
        name,
        is_dir,
        language,
        size,
        lines,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "luna-workspace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("README.md"), "# Test\n").unwrap();
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn tree_lists_dirs_first_and_expands() {
        let dir = temp_dir();

        let mut workspace = Workspace::new(dir.clone());
        assert!(workspace.rows[0].is_dir);
        let src = dir.join("src");
        let row = workspace
            .rows
            .iter()
            .position(|row| row.path == src)
            .unwrap();
        assert!(!workspace.rows[row].open);
        workspace.toggle_dir(&src);
        assert!(workspace
            .rows
            .iter()
            .any(|row| row.path == src.join("main.rs")));

        workspace.select(src.join("main.rs"));
        let preview = workspace.preview.as_ref().unwrap();
        assert_eq!(preview.language, Language::Rust);
        assert_eq!(preview.lines[0], "fn main() {}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collapsing_a_folder_keeps_the_scroll_position() {
        let dir = temp_dir();
        let mut workspace = Workspace::new(dir.clone());
        workspace.scroll = 120.0;
        let src = dir.join("src");
        workspace.toggle_dir(&src);
        workspace.toggle_dir(&src);
        assert_eq!(workspace.scroll, 120.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn options_hide_hidden_and_build_directories() {
        let dir = temp_dir();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::create_dir_all(dir.join(".cache")).unwrap();
        std::fs::write(dir.join(".env"), "x").unwrap();
        let mut workspace = Workspace::new(dir.clone());
        assert!(!workspace
            .rows
            .iter()
            .any(|row| row.name == ".env" || row.name == "target" || row.name == ".cache"));
        workspace.configure(true, false);
        assert!(workspace.rows.iter().any(|row| row.name == ".env"));
        assert!(workspace.rows.iter().any(|row| row.name == ".cache"));
        assert!(workspace.rows.iter().any(|row| row.name == "target"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn target_dir_follows_the_selection() {
        let dir = temp_dir();
        let mut workspace = Workspace::new(dir.clone());
        assert_eq!(workspace.target_dir(), dir);

        workspace.select(dir.join("README.md"));
        assert_eq!(workspace.target_dir(), dir);

        workspace.select(dir.join("src"));
        assert_eq!(workspace.target_dir(), dir.join("src"));

        workspace.focus_dir(&dir.join("docs"));
        assert!(workspace.expanded.contains(&dir.join("docs")));
        assert_eq!(workspace.selected, Some(dir.join("docs")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
