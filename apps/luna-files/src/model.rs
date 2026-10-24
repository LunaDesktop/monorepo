//! Model katalogu: wpisy, sortowanie, filtrowanie i formatowanie.
//!
//! Warstwa niezależna od UI — da się ją testować bez okna i bez Waylanda.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use libluna_i18n::tr;
use libluna_widgets::{Item, ItemKind};

/// Klucz sortowania listy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Kind,
}

impl SortKey {
    pub fn label(self) -> String {
        tr(match self {
            SortKey::Name => "Name",
            SortKey::Size => "Size",
            SortKey::Modified => "Modified",
            SortKey::Kind => "Type",
        })
    }

    pub fn next(self) -> Self {
        match self {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Modified,
            SortKey::Modified => SortKey::Kind,
            SortKey::Kind => SortKey::Name,
        }
    }
}

/// Rodzaj pliku (na podstawie rozszerzenia) — do ikon, podpisów i kolumny „Typ”.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Image,
    Audio,
    Video,
    Archive,
    Document,
    Code,
    Text,
    Other,
}

impl Kind {
    pub fn label(self) -> String {
        tr(match self {
            Kind::Folder => "Folder",
            Kind::Image => "Image",
            Kind::Audio => "Audio",
            Kind::Video => "Video",
            Kind::Archive => "Archive",
            Kind::Document => "Document",
            Kind::Code => "Code",
            Kind::Text => "Text file",
            Kind::Other => "File",
        })
    }

    pub fn widget_kind(self) -> ItemKind {
        match self {
            Kind::Folder => ItemKind::Folder,
            Kind::Image => ItemKind::Image,
            Kind::Audio => ItemKind::Audio,
            Kind::Video => ItemKind::Video,
            Kind::Archive => ItemKind::Archive,
            Kind::Document => ItemKind::Document,
            Kind::Code => ItemKind::Code,
            Kind::Text => ItemKind::Text,
            Kind::Other => ItemKind::File,
        }
    }

    fn from_name(name: &str, is_dir: bool) -> Self {
        if is_dir {
            return Kind::Folder;
        }
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tiff" | "avif"
            | "heic" => Kind::Image,
            "mp3" | "flac" | "ogg" | "oga" | "wav" | "opus" | "m4a" | "aac" | "wma" => Kind::Audio,
            "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" | "wmv" | "flv" => Kind::Video,
            "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "7z" | "rar" | "deb" | "rpm"
            | "iso" => Kind::Archive,
            "pdf" | "odt" | "ods" | "odp" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
            | "epub" | "rtf" => Kind::Document,
            "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "tsx" | "jsx" | "go"
            | "java" | "sh" | "toml" | "yaml" | "yml" | "json" | "xml" | "css" | "html" | "lua"
            | "rb" | "php" | "cs" | "kt" | "swift" => Kind::Code,
            "txt" | "md" | "log" | "conf" | "ini" | "cfg" | "csv" | "lang" => Kind::Text,
            _ => Kind::Other,
        }
    }
}

/// Pojedynczy wpis w katalogu.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    /// Czas modyfikacji (sekundy od epoki).
    pub modified: Option<i64>,
    pub hidden: bool,
    pub symlink: bool,
    pub kind: Kind,
}

impl Entry {
    pub fn detail(&self) -> String {
        if self.is_dir {
            Kind::Folder.label()
        } else {
            human_size(self.size)
        }
    }

    pub fn item(&self) -> Item {
        Item::new(self.name.clone())
            .detail(self.detail())
            .kind(self.kind.widget_kind())
    }
}

/// Zawartość katalogu + ustawienia prezentacji.
#[derive(Debug, Clone)]
pub struct DirView {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    sort: SortKey,
    ascending: bool,
    show_hidden: bool,
    filter: String,
}

impl DirView {
    /// Wczytuje katalog (bez podążania za `..`).
    pub fn load(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        let entries = read_entries(&path)?;
        let mut view = Self {
            path,
            entries,
            sort: SortKey::Name,
            ascending: true,
            show_hidden: false,
            filter: String::new(),
        };
        view.resort();
        Ok(view)
    }

    pub fn reload(&mut self) -> io::Result<()> {
        self.entries = read_entries(&self.path)?;
        self.resort();
        Ok(())
    }

    pub fn sort_key(&self) -> SortKey {
        self.sort
    }

    pub fn set_sort_key(&mut self, key: SortKey) {
        self.sort = key;
        self.resort();
    }

    pub fn ascending(&self) -> bool {
        self.ascending
    }

    pub fn set_ascending(&mut self, ascending: bool) {
        self.ascending = ascending;
        self.resort();
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.show_hidden = show;
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into().to_lowercase();
    }

    pub fn title(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn parent(&self) -> Option<PathBuf> {
        self.path.parent().map(Path::to_path_buf)
    }

    /// Indeksy wpisów widocznych po filtrze i ukrywaniu.
    pub fn visible(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| (self.show_hidden || !e.hidden) && self.matches(e))
            .map(|(i, _)| i)
            .collect()
    }

    fn matches(&self, e: &Entry) -> bool {
        self.filter.is_empty() || e.name.to_lowercase().contains(&self.filter)
    }

    /// Lista do `ItemView` (kolejność = `visible()`).
    pub fn items(&self) -> Vec<Item> {
        self.visible()
            .into_iter()
            .map(|i| self.entries[i].item())
            .collect()
    }

    /// Widoczne wpisy w kolejności prezentacji.
    pub fn visible_entries(&self) -> Vec<&Entry> {
        self.visible()
            .into_iter()
            .filter_map(|i| self.entries.get(i))
            .collect()
    }

    pub fn entry(&self, visible_index: usize) -> Option<&Entry> {
        let i = *self.visible().get(visible_index)?;
        self.entries.get(i)
    }

    fn resort(&mut self) {
        let key = self.sort;
        let asc = self.ascending;
        self.entries.sort_by(|a, b| {
            // Katalogi zawsze przed plikami (też przy sortowaniu malejąco).
            b.is_dir.cmp(&a.is_dir).then_with(|| {
                let ord = match key {
                    SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortKey::Size => a.size.cmp(&b.size),
                    SortKey::Modified => a.modified.unwrap_or(0).cmp(&b.modified.unwrap_or(0)),
                    SortKey::Kind => a
                        .kind
                        .label()
                        .cmp(&b.kind.label())
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                };
                if asc {
                    ord
                } else {
                    ord.reverse()
                }
            })
        });
    }
}

fn read_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let full = entry.path();
        let file_type = entry.file_type().ok();
        let symlink = file_type.map(|t| t.is_symlink()).unwrap_or(false);
        // Dla symlinków idziemy za celem (katalog-link ma być katalogiem).
        let meta = if symlink {
            fs::metadata(&full).ok()
        } else {
            entry.metadata().ok()
        };
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = meta
            .as_ref()
            .map(|m| if m.is_dir() { 0 } else { m.len() })
            .unwrap_or(0);
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        out.push(Entry {
            hidden: name.starts_with('.'),
            kind: Kind::from_name(&name, is_dir),
            name,
            path: full,
            is_dir,
            size,
            modified,
            symlink,
        });
    }
    Ok(out)
}

/// Rozmiar w jednostkach binarnych (np. `1,4 MiB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// Data i godzina według czasu lokalnego (`YYYY-MM-DD HH:MM`).
pub fn format_mtime(secs: Option<i64>) -> String {
    let Some(secs) = secs else {
        return String::new();
    };
    let (y, m, d, h, mi, _) = local_time(secs);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

/// Format `DeletionDate` z XDG Trash Spec (`YYYY-MM-DDThh:mm:ss`), czas lokalny.
pub fn trash_date(secs: i64) -> String {
    let (y, m, d, h, mi, s) = local_time(secs);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}")
}

/// Bieżący czas w sekundach od epoki.
pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Rozbicie czasu na pola lokalne (rok, miesiąc, dzień, godzina, minuta, sekunda).
fn local_time(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    // SAFETY: `localtime_r` wypełnia strukturę `tm` i nie alokuje; przekazujemy
    // poprawny wskaźnik na `time_t`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let t = secs as libc::time_t;
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if !ok {
        return (1970, 1, 1, 0, 0, 0);
    }
    (
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_scales_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(10 * 1024), "10 KiB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn kind_is_derived_from_extension() {
        assert_eq!(Kind::from_name("a.png", false), Kind::Image);
        assert_eq!(Kind::from_name("a.MP4", false), Kind::Video);
        assert_eq!(Kind::from_name("a.tar.gz", false), Kind::Archive);
        assert_eq!(Kind::from_name("a.rs", false), Kind::Code);
        assert_eq!(Kind::from_name("a.unknownext", false), Kind::Other);
        assert_eq!(Kind::from_name("katalog", true), Kind::Folder);
    }

    fn view_with(names: &[(&str, bool)]) -> DirView {
        let mut entries = Vec::new();
        for (name, is_dir) in names {
            entries.push(Entry {
                name: (*name).to_owned(),
                path: PathBuf::from("/tmp").join(name),
                is_dir: *is_dir,
                size: name.len() as u64,
                modified: None,
                hidden: name.starts_with('.'),
                symlink: false,
                kind: Kind::from_name(name, *is_dir),
            });
        }
        DirView {
            path: PathBuf::from("/tmp"),
            entries,
            sort: SortKey::Name,
            ascending: true,
            show_hidden: false,
            filter: String::new(),
        }
    }

    #[test]
    fn directories_sort_first_and_hidden_are_filtered() {
        let mut v = view_with(&[
            ("b.txt", false),
            ("a", true),
            (".hidden", false),
            ("c", true),
        ]);
        v.resort();
        let names: Vec<&str> = v
            .visible()
            .into_iter()
            .map(|i| v.entries[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["a", "c", "b.txt"]);
        v.set_show_hidden(true);
        assert_eq!(v.visible().len(), 4);
    }

    #[test]
    fn filter_matches_case_insensitively() {
        let mut v = view_with(&[("Raport.pdf", false), ("notatki.txt", false)]);
        v.set_filter("RAP");
        let names: Vec<String> = v
            .visible()
            .into_iter()
            .map(|i| v.entries[i].name.clone())
            .collect();
        assert_eq!(names, vec!["Raport.pdf"]);
    }

    #[test]
    fn descending_keeps_directories_first() {
        let mut v = view_with(&[("b.txt", false), ("a.txt", false), ("dir", true)]);
        v.resort();
        v.set_ascending(false);
        let names: Vec<&str> = v
            .visible()
            .into_iter()
            .map(|i| v.entries[i].name.as_str())
            .collect();
        assert_eq!(names, vec!["dir", "b.txt", "a.txt"]);
    }
}
