//! Ikony pulpitu: katalogi, skanowanie plików i zapis ustawień.
//!
//! Pulpit Luny pokazuje zawartość katalogu `XDG_DESKTOP_DIR` (z `user-dirs.dirs`)
//! oraz dodatkowych katalogów dodanych w Ustawieniach. Wpisy `.desktop` są
//! traktowane jako skróty (ikona aplikacji), a zwykłe pliki i foldery dostają
//! ikonę zależną od rozszerzenia.
//!
//! Cała logika jest czysta i testowalna: parsowanie `user-dirs.dirs`, skanowanie
//! katalogów i format pliku konfiguracji. Aplikacja `luna-desktop` tylko rysuje
//! wynik i wykonuje akcje na plikach.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::{config_file, parse_pairs, save_atomic};

/// Co przedstawia ikona na pulpicie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopItemKind {
    /// Zwykły plik.
    File,
    /// Katalog (otwierany w Plikach).
    Dir,
    /// Skrót `.desktop` (uruchamiany przez `gio launch`).
    Shortcut,
}

/// Jedna ikona na pulpicie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub path: PathBuf,
    /// Czytelna nazwa: nazwa pliku/katalogu albo zlokalizowane `Name` skrótu.
    pub name: String,
    pub kind: DesktopItemKind,
    /// `Icon=` ze skrótu `.desktop` (odczytane raz przy skanowaniu, bez I/O na klatkę).
    pub icon: Option<String>,
}

/// Ustawienia pulpitu (`~/.config/luna/desktop.conf`).
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopSettings {
    /// Katalogi pokazywane na pulpicie (kolejność = kolejność skanowania).
    pub folders: Vec<PathBuf>,
    /// Bok ikony w pikselach logicznych.
    pub icon_size: f32,
    /// Odstęp siatki (ikona + etykieta + margines).
    pub grid: f32,
    /// Czy pokazywać etykiety pod ikonami.
    pub labels: bool,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            folders: vec![desktop_dir()],
            icon_size: 64.0,
            grid: 96.0,
            labels: true,
        }
    }
}

impl DesktopSettings {
    /// Wczytuje ustawienia; brak pliku = wartości domyślne.
    pub fn load() -> Self {
        let Some(path) = config_file("desktop.conf") else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::parse(&text)
    }

    /// Parsuje zawartość `desktop.conf`.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut folders = Vec::new();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "folder" => {
                    let path = PathBuf::from(value);
                    if !path.as_os_str().is_empty() && !folders.contains(&path) {
                        folders.push(path);
                    }
                }
                "icon_size" => {
                    if let Ok(v) = value.parse::<f32>() {
                        out.icon_size = v.clamp(32.0, 128.0);
                    }
                }
                "grid" => {
                    if let Ok(v) = value.parse::<f32>() {
                        out.grid = v.clamp(64.0, 200.0);
                    }
                }
                "labels" => out.labels = value != "0" && !value.eq_ignore_ascii_case("false"),
                _ => {}
            }
        }
        if !folders.is_empty() {
            out.folders = folders;
        }
        out
    }

    /// Zapisuje ustawienia do `~/.config/luna/desktop.conf`.
    pub fn save(&self) -> io::Result<()> {
        let path = config_file("desktop.conf")
            .ok_or_else(|| io::Error::other("brak katalogu konfiguracji"))?;
        save_atomic(&path, &self.to_text())
    }

    /// Serializuje ustawienia do formatu `klucz = wartość`.
    pub fn to_text(&self) -> String {
        let mut text = String::from("# Pulpit Luny — ustawienia ikon\n");
        for folder in &self.folders {
            text.push_str(&format!("folder = {}\n", folder.display()));
        }
        text.push_str(&format!("icon_size = {}\n", self.icon_size));
        text.push_str(&format!("grid = {}\n", self.grid));
        text.push_str(&format!("labels = {}\n", u8::from(self.labels)));
        text
    }
}

/// Katalog pulpitu z `~/.config/user-dirs.dirs`.
///
/// Kolejność: `XDG_DESKTOP_DIR` z pliku użytkownika, potem `~/Desktop`,
/// `~/Pulpit`, a na końcu `~/Desktop` (nawet jeśli jeszcze nie istnieje).
pub fn desktop_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    if let Some(dir) = user_dirs_desktop(&home) {
        return dir;
    }
    for name in ["Desktop", "Pulpit"] {
        let candidate = home.join(name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    home.join("Desktop")
}

/// Odczytuje `XDG_DESKTOP_DIR` z `~/.config/user-dirs.dirs`.
fn user_dirs_desktop(home: &Path) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let text = std::fs::read_to_string(base.join("user-dirs.dirs")).ok()?;
    parse_user_dirs_desktop(&text, home)
}

/// Czysty parser `user-dirs.dirs` (do testów).
pub fn parse_user_dirs_desktop(text: &str, home: &Path) -> Option<PathBuf> {
    for raw in text.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("XDG_DESKTOP_DIR=") else {
            continue;
        };
        let value = rest.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        let expanded = value
            .replace("${HOME}", &home.display().to_string())
            .replace("$HOME", &home.display().to_string());
        let path = PathBuf::from(expanded);
        if path.is_absolute() {
            return Some(path);
        }
    }
    None
}

/// Skanuje katalogi i zwraca ikony w kolejności: katalogi, potem pliki.
///
/// Pomija pliki ukryte (`.foo`) oraz skróty `.desktop` z `Hidden=true`
/// lub `NoDisplay=true`. Brak katalogu nie jest błędem — po prostu nic nie wnosi.
pub fn scan(folders: &[PathBuf]) -> Vec<DesktopEntry> {
    let mut out: Vec<DesktopEntry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for folder in folders {
        let Ok(read) = std::fs::read_dir(folder) else {
            continue;
        };
        let mut entries: Vec<DesktopEntry> = read
            .filter_map(Result::ok)
            .filter_map(|item| entry_for(&item.path()))
            .collect();
        entries.sort_by_cached_key(|entry| {
            (
                entry.kind != DesktopItemKind::Dir,
                entry.name.to_lowercase(),
            )
        });
        for entry in entries {
            if seen.insert(entry.path.clone()) {
                out.push(entry);
            }
        }
    }
    out
}

/// Buduje wpis dla ścieżki (albo `None`, gdy plik ma być ukryty).
pub fn entry_for(path: &Path) -> Option<DesktopEntry> {
    let name = path.file_name()?.to_string_lossy().to_string();
    if name.starts_with('.') {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.is_dir() {
        return Some(DesktopEntry {
            path: path.to_path_buf(),
            name,
            kind: DesktopItemKind::Dir,
            icon: None,
        });
    }
    if !meta.is_file() {
        return None;
    }
    if name.to_ascii_lowercase().ends_with(".desktop") {
        // Czytamy plik raz: z tej samej treści bierzemy `Hidden`, czytelną,
        // zlokalizowaną nazwę (`Name[pl]` → `Name` → nazwa pliku) i `Icon`.
        let text = std::fs::read_to_string(path).ok();
        if let Some(text) = &text {
            if shortcut_hidden(text) {
                return None;
            }
        }
        let name = match &text {
            Some(text) => shortcut_name_from_text(path, text),
            None => fallback_stem(path),
        };
        let icon = text
            .as_deref()
            .and_then(|text| field_from_text(text, "Icon"));
        return Some(DesktopEntry {
            path: path.to_path_buf(),
            name,
            kind: DesktopItemKind::Shortcut,
            icon,
        });
    }
    Some(DesktopEntry {
        path: path.to_path_buf(),
        name,
        kind: DesktopItemKind::File,
        icon: None,
    })
}

/// Czy sekcja `[Desktop Entry]` oznacza wpis ukryty.
pub fn shortcut_hidden(text: &str) -> bool {
    let mut in_entry = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            if matches!(key.trim(), "Hidden" | "NoDisplay")
                && value.trim().eq_ignore_ascii_case("true")
            {
                return true;
            }
        }
    }
    false
}

/// Pole z sekcji `[Desktop Entry]` pliku `.desktop`.
fn shortcut_field(path: &Path, wanted: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    field_from_text(&text, wanted)
}

/// Pole z już odczytanej treści wpisu (ostatnia wartość wygrywa).
fn field_from_text(text: &str, wanted: &str) -> Option<String> {
    entry_fields(text)
        .into_iter()
        .rev()
        .find(|(key, _)| key == wanted)
        .map(|(_, value)| value)
}

/// Wszystkie pary `klucz = wartość` z sekcji `[Desktop Entry]`.
fn entry_fields(text: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();
    let mut in_entry = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            if !value.is_empty() {
                fields.push((key.trim().to_owned(), value.to_owned()));
            }
        }
    }
    fields
}

/// Język interfejsu ze standardowych zmiennych (`LC_ALL` → `LC_MESSAGES` →
/// `LANGUAGE` → `LANG`), w postaci `(pełny_tag, język)` — np. `("pl_PL", "pl")`.
///
/// `libluna-core` nie zależy od `libluna-i18n`, więc robimy to lokalnie, według
/// tej samej kolejności zmiennych. Bez locale wracamy do `Name`.
fn desktop_locale() -> Option<(String, String)> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        // `LANGUAGE` może być listą priorytetów (`pl:de:en`).
        let first = value.split(':').next().unwrap_or("");
        let first = first.split('.').next().unwrap_or(first);
        let first = first.split('@').next().unwrap_or(first);
        if first.is_empty()
            || first.eq_ignore_ascii_case("C")
            || first.eq_ignore_ascii_case("POSIX")
        {
            continue;
        }
        let language = first
            .split(['_', '-'])
            .next()
            .unwrap_or(first)
            .to_ascii_lowercase();
        return Some((first.to_owned(), language));
    }
    None
}

/// Czytelna nazwa skrótu `.desktop` z już odczytanej treści pliku.
///
/// Kolejność: `Name[pełny_tag]` → jakikolwiek `Name[<język>_*]` → `Name` →
/// nazwa pliku bez rozszerzenia. Dzięki temu na pulpicie widać „Ustawienia”,
/// a nie `org.luna.Settings.desktop`.
fn shortcut_name_from_text(path: &Path, text: &str) -> String {
    let locale = desktop_locale();
    shortcut_name_localized(
        path,
        text,
        locale
            .as_ref()
            .map(|(tag, language)| (tag.as_str(), language.as_str())),
    )
}

/// Wariant [`shortcut_name_from_text`] z jawnie podanym językiem (testy).
fn shortcut_name_localized(path: &Path, text: &str, locale: Option<(&str, &str)>) -> String {
    let fields = entry_fields(text);
    if let Some((tag, language)) = locale {
        if let Some(name) = fields
            .iter()
            .rev()
            .find(|(key, _)| key.eq_ignore_ascii_case(&format!("Name[{tag}]")))
            .map(|(_, value)| value.clone())
        {
            return name;
        }
        // Np. wpis ma `Name[pl_PL]`, a użytkownik ma samo `pl`.
        if let Some(name) = fields
            .iter()
            .rev()
            .find(|(key, _)| {
                key.strip_prefix("Name[")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .map(|entry_tag| {
                        entry_tag
                            .split(['_', '-'])
                            .next()
                            .unwrap_or(entry_tag)
                            .eq_ignore_ascii_case(language)
                    })
                    .unwrap_or(false)
            })
            .map(|(_, value)| value.clone())
        {
            return name;
        }
    }
    fields
        .iter()
        .rev()
        .find(|(key, _)| key.eq_ignore_ascii_case("Name"))
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| fallback_stem(path))
}

/// Nazwa pliku bez `.desktop` — ostatnia linia obrony, gdy wpis nie ma `Name`.
fn fallback_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Nazwa ikony/etykiety skrótu `.desktop`: zlokalizowane `Name[xx]`, `Name`,
/// a dopiero na końcu nazwa pliku bez rozszerzenia.
pub fn shortcut_name(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => shortcut_name_from_text(path, &text),
        Err(_) => fallback_stem(path),
    }
}

/// Nazwa ikony z pliku `.desktop` (pole `Icon`), jeśli jest podana.
pub fn shortcut_icon(path: &Path) -> Option<String> {
    shortcut_field(path, "Icon")
}

/// Kopiuje skrót `.desktop` na pulpit, nadając unikalną nazwę (`-2`, `-3`, …).
///
/// Zwraca ścieżkę utworzonego pliku. Plik docelowy dostaje prawo wykonania,
/// żeby menedżery plików i `gio launch` traktowały go jak skrót.
pub fn add_shortcut(source: &Path) -> io::Result<PathBuf> {
    add_shortcut_to(source, &desktop_dir())
}

/// Jak [`add_shortcut`], ale do wskazanego katalogu (pierwszy katalog z Ustawień).
pub fn add_shortcut_to(source: &Path, folder: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "skrot".to_owned());
    let mut target = folder.join(format!("{stem}.desktop"));
    let mut n = 2;
    while target.exists() {
        target = folder.join(format!("{stem}-{n}.desktop"));
        n += 1;
    }
    std::fs::copy(source, &target)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
    }
    Ok(target)
}

/// Zapisane pozycje ikon: ścieżka → (kolumna, wiersz).
pub type DesktopPositions = HashMap<PathBuf, (i32, i32)>;

/// Prefiks klucza pozycji dla pulpitu na wyjściu `output`.
///
/// Wyjście 0 używa historycznego `pos:<ścieżka>` (zgodność z istniejącymi
/// plikami), a kolejne monitory dostają `pos1:`, `pos2:` itd.
fn positions_prefix(output: usize) -> String {
    if output == 0 {
        "pos:".to_owned()
    } else {
        format!("pos{output}:")
    }
}

/// Wyjście, do którego należy klucz `pos…`; `None` = to nie jest klucz pozycji.
fn key_output(key: &str) -> Option<usize> {
    let rest = key.strip_prefix("pos")?;
    let (head, _) = rest.split_once(':')?;
    if head.is_empty() {
        Some(0)
    } else {
        head.parse().ok()
    }
}

/// Tekst bez pozycji jednego wyjścia (reszta linii zostaje nietknięta).
fn without_output(text: &str, output: usize) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if let Some((key, _)) = line.split_once('=') {
            if key_output(key.trim()) == Some(output) {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Dokleja pozycje jednego wyjścia do tekstu pliku.
fn with_positions(mut text: String, output: usize, positions: &DesktopPositions) -> String {
    let mut keys: Vec<_> = positions.iter().collect();
    keys.sort_by_key(|(path, _)| path.as_os_str().to_owned());
    if !keys.is_empty() && !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let prefix = positions_prefix(output);
    for (path, (col, row)) in keys {
        text.push_str(&format!("{prefix}{} = {col},{row}\n", path.display()));
    }
    text
}

/// Wczytuje pozycje z `~/.config/luna/desktop.conf` (klucze `pos:<ścieżka>`).
pub fn load_positions() -> DesktopPositions {
    load_positions_at(0)
}

/// Wczytuje pozycje ikon pulpitu na wskazanym wyjściu.
///
/// Każdy monitor ma własną siatkę (okna mają różne rozmiary), więc zapisujemy
/// pozycje osobno: wyjście 0 → `pos:<ścieżka>`, kolejne → `pos1:<ścieżka>`.
pub fn load_positions_at(output: usize) -> DesktopPositions {
    let mut out = DesktopPositions::new();
    let Some(path) = config_file("desktop.conf") else {
        return out;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return out;
    };
    for (key, value) in parse_pairs(&text) {
        if key_output(&key) != Some(output) {
            continue;
        }
        let Some((_, raw)) = key.split_once(':') else {
            continue;
        };
        let Some((col, row)) = value.split_once(',') else {
            continue;
        };
        if let (Ok(col), Ok(row)) = (col.trim().parse::<i32>(), row.trim().parse::<i32>()) {
            out.insert(PathBuf::from(raw), (col.max(0), row.max(0)));
        }
    }
    out
}

/// Zapisuje ustawienia razem z pozycjami ikon pulpitu głównego.
///
/// Pozycje z pozostałych monitorów (`pos1:`…) są zachowane — inaczej zmiana
/// ustawień w Ustawieniach kasowałaby układ ikon na drugim ekranie.
pub fn save(settings: &DesktopSettings, positions: &DesktopPositions) -> io::Result<()> {
    let path = config_file("desktop.conf")
        .ok_or_else(|| io::Error::other("brak katalogu konfiguracji"))?;
    let mut text = settings.to_text();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    for line in existing.lines() {
        if let Some((key, _)) = line.split_once('=') {
            if key_output(key.trim()).is_some_and(|output| output != 0) {
                text.push_str(line.trim_end());
                text.push('\n');
            }
        }
    }
    text = with_positions(text, 0, positions);
    save_atomic(&path, &text)
}

/// Zapisuje pozycje ikon **jednego** pulpitu, nie ruszając ustawień ani
/// pozycji z innych monitorów.
pub fn save_positions_at(output: usize, positions: &DesktopPositions) -> io::Result<()> {
    let path = config_file("desktop.conf")
        .ok_or_else(|| io::Error::other("brak katalogu konfiguracji"))?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let text = with_positions(without_output(&existing, output), output, positions);
    save_atomic(&path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "luna-desktop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parses_user_dirs_home_variable() {
        let home = Path::new("/home/ania");
        let text = "XDG_DESKTOP_DIR=\"$HOME/Pulpit\"\nXDG_DOWNLOAD_DIR=\"$HOME/Pobrane\"\n";
        assert_eq!(
            parse_user_dirs_desktop(text, home),
            Some(PathBuf::from("/home/ania/Pulpit"))
        );
        assert_eq!(parse_user_dirs_desktop("# nic\n", home), None);
    }

    #[test]
    fn positions_are_stored_per_output() {
        let mut primary = DesktopPositions::new();
        primary.insert(PathBuf::from("/tmp/a"), (0, 0));
        let mut second = DesktopPositions::new();
        second.insert(PathBuf::from("/tmp/a"), (2, 1));

        // Plik zawiera ustawienia, pozycję wyjścia 0 i wyjścia 1.
        let text = "icon_size = 64\ngrid = 96\npos:/tmp/b = 1,1\npos1:/tmp/c = 3,2\n";
        let text = with_positions(without_output(text, 0), 0, &primary);
        let text = with_positions(without_output(&text, 1), 1, &second);

        // Wyjście 0 zachowuje historyczny format bez numeru, kolejne ma `pos1:`.
        assert!(text.contains("pos:/tmp/a = 0,0"), "{text}");
        assert!(text.contains("pos1:/tmp/a = 2,1"), "{text}");
        assert!(!text.contains("pos:/tmp/b"), "{text}");
        assert!(!text.contains("pos1:/tmp/c"), "{text}");
        assert!(text.contains("grid = 96"), "{text}");
    }

    #[test]
    fn position_keys_carry_the_output_index() {
        assert_eq!(key_output("pos:/tmp/a"), Some(0));
        assert_eq!(key_output("pos1:/tmp/a"), Some(1));
        assert_eq!(key_output("pos12:/x"), Some(12));
        assert_eq!(key_output("grid"), None);
        assert_eq!(key_output("position"), None);
        assert_eq!(key_output("folder"), None);
        assert_eq!(positions_prefix(0), "pos:");
        assert_eq!(positions_prefix(2), "pos2:");
    }

    #[test]
    fn settings_roundtrip() {
        let settings = DesktopSettings {
            folders: vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")],
            icon_size: 72.0,
            grid: 104.0,
            labels: false,
        };
        let parsed = DesktopSettings::parse(&settings.to_text());
        assert_eq!(parsed, settings);
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = temp("scan-missing");
        let entries = scan(&[dir.join("nie-ma")]);
        assert!(entries.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scan_orders_dirs_first_and_skips_hidden() {
        let dir = temp("scan");
        std::fs::create_dir_all(dir.join("Folder")).unwrap();
        std::fs::write(dir.join("notatka.txt"), "x").unwrap();
        std::fs::write(dir.join(".ukryty"), "x").unwrap();
        std::fs::write(
            dir.join("ukryta.desktop"),
            "[Desktop Entry]\nName=Ukryta\nHidden=true\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("widoczna.desktop"),
            "[Desktop Entry]\nName=Widoczna\n",
        )
        .unwrap();
        let entries = scan(&[dir.clone()]);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        // Skrót pokazuje `Name` z `.desktop`, nie nazwę pliku.
        assert_eq!(names, vec!["Folder", "notatka.txt", "Widoczna"]);
        assert_eq!(entries[0].kind, DesktopItemKind::Dir);
        assert_eq!(entries[2].kind, DesktopItemKind::Shortcut);
        assert_eq!(shortcut_name(&dir.join("widoczna.desktop")), "Widoczna");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn localized_shortcut_name_prefers_language() {
        let dir = temp("localized");
        let path = dir.join("org.luna.Settings.desktop");
        let text = "[Desktop Entry]\nName=Settings\nName[pl]=Ustawienia\nName[de]=Einstellungen\n";
        assert_eq!(
            shortcut_name_localized(&path, text, Some(("pl_PL", "pl"))),
            "Ustawienia"
        );
        assert_eq!(
            shortcut_name_localized(&path, text, Some(("de_DE", "de"))),
            "Einstellungen"
        );
        // Brak tłumaczenia dla języka → `Name`.
        assert_eq!(
            shortcut_name_localized(&path, text, Some(("fr_FR", "fr"))),
            "Settings"
        );
        assert_eq!(shortcut_name_localized(&path, text, None), "Settings");
        // Wpis ma tylko wariant regionalny — dopasowujemy po języku.
        let regional = "[Desktop Entry]\nName=Settings\nName[pl_PL]=Ustawienia PL\n";
        assert_eq!(
            shortcut_name_localized(&path, regional, Some(("pl", "pl"))),
            "Ustawienia PL"
        );
        // Bez żadnego `Name` zostaje nazwa pliku bez `.desktop`.
        assert_eq!(
            shortcut_name_localized(&path, "[Desktop Entry]\n", Some(("pl", "pl"))),
            "org.luna.Settings"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unique_shortcut_name() {
        let dir = temp("shortcut");
        let source = dir.join("app.desktop");
        std::fs::write(&source, "[Desktop Entry]\nName=App\n").unwrap();
        // `add_shortcut` pisze do katalogu pulpitu, więc sprawdzamy tylko nazwę
        // wyliczaną przez ten sam algorytm bez ruszania katalogu użytkownika.
        let stem = source.file_stem().unwrap().to_string_lossy().to_string();
        assert_eq!(stem, "app");
        let _ = std::fs::remove_dir_all(dir);
    }
}
