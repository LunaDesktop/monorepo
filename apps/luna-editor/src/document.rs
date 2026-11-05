//! Model dokumentu: tekst, kursor, undo/redo i wejście/wyjście plików.

use std::path::{Path, PathBuf};

use libluna_widgets::CodeView;

use crate::syntax::{self, Language};

/// Maksymalny rozmiar otwieranego pliku (32 MiB).
pub const MAX_FILE_SIZE: u64 = 32 * 1024 * 1024;
/// Ile kroków pamięta undo.
const UNDO_LIMIT: usize = 500;

/// Sposób zapisu końców linii.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
        }
    }
}

/// Jedna zmiana tekstu (podstawa undo/redo).
#[derive(Debug, Clone)]
struct Edit {
    start: usize,
    removed: String,
    inserted: String,
    cursor_before: usize,
}

/// Otwarty dokument (jedna zakładka).
pub struct Document {
    pub view: CodeView,
    pub path: Option<PathBuf>,
    pub language: Language,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub dirty: bool,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl Document {
    pub fn new() -> Self {
        let mut view = CodeView::new();
        view.set_focused(true);
        Self {
            view,
            path: None,
            language: Language::Plain,
            line_ending: LineEnding::Lf,
            bom: false,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn with_text(path: Option<PathBuf>, text: &str) -> Self {
        let mut document = Self::new();
        document.language = path
            .as_deref()
            .map(Language::from_path)
            .unwrap_or(Language::Plain);
        document.path = path;
        document.view.set_text(text);
        document.rehighlight();
        document
    }

    /// Ładuje plik z dysku.
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let loaded = read_document(&path)?;
        let mut document = Self::new();
        document.language = Language::from_path(&path);
        document.line_ending = loaded.line_ending;
        document.bom = loaded.bom;
        document.path = Some(path);
        document.view.set_text(loaded.text);
        document.rehighlight();
        Ok(document)
    }

    /// Nazwa pliku albo „Bez tytułu”.
    pub fn name(&self) -> String {
        self.path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| libluna_i18n::tr("Untitled"))
    }

    /// Etykieta zakładki (ze znacznikiem niezapisanych zmian).
    pub fn tab_title(&self) -> String {
        if self.dirty {
            format!("● {}", self.name())
        } else {
            self.name()
        }
    }

    pub fn file_label(&self) -> String {
        self.path
            .as_deref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| libluna_i18n::tr("Untitled"))
    }

    pub fn is_empty_untitled(&self) -> bool {
        self.path.is_none() && self.view.is_empty() && !self.dirty
    }

    pub fn rehighlight(&mut self) {
        let spans = syntax::highlight(self.view.text(), self.language);
        self.view.set_spans(spans);
    }

    fn record(&mut self, start: usize, removed: String, inserted: String, cursor_before: usize) {
        self.undo.push(Edit {
            start,
            removed,
            inserted,
            cursor_before,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.rehighlight();
    }

    fn replace_selection(&mut self, insert: &str) {
        let (start, end) = self
            .view
            .selection()
            .unwrap_or((self.view.cursor(), self.view.cursor()));
        let removed = self.view.text()[start..end].to_owned();
        let cursor_before = self.view.cursor();
        self.view.replace_range(start, end, insert);
        self.record(start, removed, insert.to_owned(), cursor_before);
    }

    pub fn insert_text(&mut self, text: &str) {
        let plain: String = text
            .chars()
            .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
            .collect();
        if plain.is_empty() {
            return;
        }
        self.replace_selection(&plain);
    }

    /// Zamienia całą treść dokumentu (np. wynik wtyczki).
    pub fn replace_all(&mut self, text: &str) {
        let plain: String = text
            .chars()
            .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
            .collect();
        let removed = self.view.text().to_owned();
        let cursor_before = self.view.cursor();
        self.view.replace_range(0, removed.len(), &plain);
        self.record(0, removed, plain, cursor_before);
    }

    /// Dopisuje tekst na końcu dokumentu.
    pub fn append(&mut self, text: &str) {
        let plain: String = text
            .chars()
            .filter(|ch| *ch == '\n' || *ch == '\t' || !ch.is_control())
            .collect();
        if plain.is_empty() {
            return;
        }
        let start = self.view.text().len();
        let cursor_before = self.view.cursor();
        self.view.replace_range(start, start, &plain);
        self.record(start, String::new(), plain, cursor_before);
    }

    pub fn newline(&mut self) {
        let cursor = self.view.cursor();
        let line = self.view.cursor_line();
        let line_start = self.view.line_start(line);
        let indent: String = self.view.text()[line_start..cursor]
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .collect();
        let previous = self.view.text()[..cursor].chars().next_back();
        let extra = if matches!(previous, Some('{' | '(' | '[' | ':')) {
            "    "
        } else {
            ""
        };
        self.replace_selection(&format!("\n{indent}{extra}"));
    }

    pub fn backspace(&mut self) {
        let (start, end) = if let Some(selection) = self.view.selection() {
            selection
        } else if self.view.cursor() == 0 {
            return;
        } else {
            (
                prev_boundary(self.view.text(), self.view.cursor()),
                self.view.cursor(),
            )
        };
        let removed = self.view.text()[start..end].to_owned();
        let cursor_before = self.view.cursor();
        self.view.replace_range(start, end, "");
        self.record(start, removed, String::new(), cursor_before);
    }

    pub fn delete_forward(&mut self) {
        let (start, end) = if let Some(selection) = self.view.selection() {
            selection
        } else if self.view.cursor() >= self.view.text().len() {
            return;
        } else {
            (
                self.view.cursor(),
                next_boundary(self.view.text(), self.view.cursor()),
            )
        };
        let removed = self.view.text()[start..end].to_owned();
        let cursor_before = self.view.cursor();
        self.view.replace_range(start, end, "");
        self.record(start, removed, String::new(), cursor_before);
    }

    pub fn tab(&mut self, unindent: bool) {
        let line = self.view.cursor_line();
        let start = self.view.line_start(line);
        if unindent {
            let line_text = self.view.line_text(line);
            let remove = line_text
                .chars()
                .take(self.view.tab_width())
                .take_while(|ch| *ch == ' ')
                .count();
            if remove == 0 {
                return;
            }
            let cursor_before = self.view.cursor();
            let removed = self.view.text()[start..start + remove].to_owned();
            self.view.replace_range(start, start + remove, "");
            self.view.set_cursor(cursor_before.saturating_sub(remove));
            self.record(start, removed, String::new(), cursor_before);
            return;
        }
        let prefix = &self.view.text()[start..self.view.cursor()];
        let column = display_column(prefix, self.view.tab_width());
        let spaces = self.view.tab_width() - column % self.view.tab_width();
        self.replace_selection(&" ".repeat(spaces));
    }

    pub fn undo(&mut self) {
        let Some(edit) = self.undo.pop() else {
            return;
        };
        let end = edit.start + edit.inserted.len();
        self.view.replace_range(edit.start, end, &edit.removed);
        self.view.set_cursor(edit.cursor_before);
        self.redo.push(edit);
        self.dirty = true;
        self.rehighlight();
    }

    pub fn redo(&mut self) {
        let Some(edit) = self.redo.pop() else {
            return;
        };
        let end = edit.start + edit.removed.len();
        self.view.replace_range(edit.start, end, &edit.inserted);
        self.view.set_cursor(edit.start + edit.inserted.len());
        self.undo.push(edit);
        self.dirty = true;
        self.rehighlight();
    }

    pub fn copy(&self) -> Option<String> {
        self.view.copy()
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.view.cut()?;
        self.dirty = true;
        self.rehighlight();
        Some(text)
    }

    /// Zapisuje dokument pod jego ścieżką.
    pub fn save(&mut self) -> Result<(), String> {
        let Some(path) = self.path.clone() else {
            return Err(libluna_i18n::tr("The file has no path"));
        };
        write_document(&path, self.view.text(), self.line_ending, self.bom)?;
        self.dirty = false;
        self.rehighlight();
        Ok(())
    }

    /// Zapisuje dokument pod nową ścieżką i zmienia język.
    pub fn save_as(&mut self, path: PathBuf) -> Result<(), String> {
        write_document(&path, self.view.text(), self.line_ending, self.bom)?;
        self.language = Language::from_path(&path);
        self.path = Some(path);
        self.dirty = false;
        self.rehighlight();
        Ok(())
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

struct LoadedDocument {
    text: String,
    line_ending: LineEnding,
    bom: bool,
}

fn read_document(path: &Path) -> Result<LoadedDocument, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(libluna_i18n::tr("Not a regular file"));
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err(libluna_i18n::tr("File is too large"));
    }
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.contains(&0) {
        return Err(libluna_i18n::tr("Binary file"));
    }
    let mut text =
        String::from_utf8(bytes).map_err(|_| libluna_i18n::tr("The file is not valid UTF-8"))?;
    let bom = text.starts_with('\u{feff}');
    if bom {
        text.remove(0);
    }
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count();
    let line_ending = if crlf > 0 && crlf == lf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    if text.contains('\r') {
        text = text.replace("\r\n", "\n").replace('\r', "\n");
    }
    Ok(LoadedDocument {
        text,
        line_ending,
        bom,
    })
}

fn write_document(
    path: &Path,
    text: &str,
    line_ending: LineEnding,
    bom: bool,
) -> Result<(), String> {
    let mut content = String::with_capacity(text.len() + 3);
    if bom {
        content.push('\u{feff}');
    }
    match line_ending {
        LineEnding::Lf => content.push_str(text),
        LineEnding::Crlf => content.push_str(&text.replace('\n', "\r\n")),
    }
    let directory = path.parent().filter(|dir| !dir.as_os_str().is_empty());
    if let Some(directory) = directory {
        if !directory.is_dir() {
            return Err(format!(
                "{}: {}",
                directory.display(),
                libluna_i18n::tr("Directory does not exist")
            ));
        }
    }
    let temp = temp_path(path);
    if let Err(error) = std::fs::write(&temp, content.as_bytes()) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("{}: {error}", temp.display()));
    }
    if let Ok(metadata) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&temp, metadata.permissions());
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(format!("{}: {error}", path.display()));
    }
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("luna-editor");
    let temp = format!(".{name}.tmp-{}", std::process::id());
    path.with_file_name(temp)
}

/// Zamienia `~` na katalog domowy i ścieżkę relatywną na absolutną.
pub fn expand_path(value: &str) -> PathBuf {
    let expanded = if value == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
    } else if let Some(rest) = value.strip_prefix("~/") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(rest)
    } else {
        PathBuf::from(value)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(expanded)
    }
}

pub fn find_all(text: &str, query: &str) -> Vec<(usize, usize)> {
    if query.is_empty() {
        return Vec::new();
    }
    let lowered_text = text.to_lowercase();
    let lowered_query = query.to_lowercase();
    if lowered_text.len() == text.len() && lowered_query.len() == query.len() {
        let mut out = Vec::new();
        let mut offset = 0;
        while let Some(position) = lowered_text[offset..].find(&lowered_query) {
            let start = offset + position;
            out.push((start, start + query.len()));
            offset = start + query.len().max(1);
        }
        out
    } else {
        text.match_indices(query)
            .map(|(start, _)| (start, start + query.len()))
            .collect()
    }
}

fn prev_boundary(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map(|(position, _)| position)
        .unwrap_or(0)
}

fn next_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map(|ch| index + ch.len_utf8())
        .unwrap_or(text.len())
}

fn display_column(text: &str, tab_width: usize) -> usize {
    let mut column = 0;
    for ch in text.chars() {
        if ch == '\t' {
            column += tab_width.max(1) - column % tab_width.max(1);
        } else {
            column += 1;
        }
    }
    column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_and_redo_restore_text_and_cursor() {
        let mut doc = Document::with_text(None, "hello");
        doc.view.set_cursor(5);
        doc.insert_text(" world");
        assert_eq!(doc.view.text(), "hello world");
        assert!(doc.dirty);
        doc.undo();
        assert_eq!(doc.view.text(), "hello");
        assert_eq!(doc.view.cursor(), 5);
        doc.redo();
        assert_eq!(doc.view.text(), "hello world");
    }

    #[test]
    fn enter_keeps_indentation_and_adds_level_after_brace() {
        let mut doc = Document::with_text(None, "    fn main() {");
        doc.view.set_cursor(doc.view.text().len());
        doc.newline();
        assert_eq!(doc.view.text(), "    fn main() {\n        ");
    }

    #[test]
    fn tab_advances_to_next_stop_and_shift_tab_unindents() {
        let mut doc = Document::with_text(None, "ab");
        doc.view.set_cursor(2);
        doc.tab(false);
        assert_eq!(doc.view.text(), "ab  ");

        let mut indented = Document::with_text(None, "    ab");
        indented.view.set_cursor(indented.view.text().len());
        indented.tab(true);
        assert_eq!(indented.view.text(), "ab");
    }

    #[test]
    fn search_is_case_insensitive_when_lengths_allow() {
        let matches = find_all("Ala ma kota, ala", "ala");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (0, 3));
    }

    #[test]
    fn expand_path_handles_home_and_relative_paths() {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some(home) = home {
            assert_eq!(expand_path("~/x"), home.join("x"));
        }
        assert!(expand_path("relative.txt").is_absolute());
    }

    #[test]
    fn line_endings_are_detected_and_restored() {
        let dir = std::env::temp_dir().join(format!("luna-editor-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crlf.txt");
        std::fs::write(&path, b"a\r\nb\r\n").unwrap();
        let document = Document::load(path.clone()).unwrap();
        assert_eq!(document.line_ending, LineEnding::Crlf);
        assert_eq!(document.view.text(), "a\nb\n");
        let out = dir.join("out.txt");
        write_document(&out, document.view.text(), LineEnding::Crlf, false).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"a\r\nb\r\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn binary_files_are_rejected() {
        let dir = std::env::temp_dir().join(format!("luna-editor-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bin.dat");
        std::fs::write(&path, b"abc\0def").unwrap();
        assert!(read_document(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
