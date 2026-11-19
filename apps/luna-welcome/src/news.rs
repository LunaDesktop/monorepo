//! „Co nowego” — wczytywanie i parsowanie notatek wydania.
//!
//! Format jest celowo prosty (Markdown bez zależności): nagłówek wydania
//! `## 0.0.8 — 2026-11-05` oraz punkty zaczynające się od `- ` albo `* `.
//! Plik z dysku ma pierwszeństwo nad wbudowanym, więc wydanie systemu może
//! podmienić treść bez przebudowy aplikacji.
//!
//! **Język:** najpierw szukamy wariantu z kodem języka (`news.en.md`), potem
//! — gdy plik bazowy na dysku byłby w innym języku — sięgamy po wbudowane
//! notatki w języku sesji. Dopiero na końcu używamy pliku bazowego (polskiego).

use std::path::{Path, PathBuf};

/// Język plików bazowych w repozytorium (`data/news.md`).
const BASE_LANGUAGE: &str = "pl";

/// Wbudowane notatki — używane, gdy system nie dostarczył własnego pliku.
pub const EMBEDDED_NEWS: &str = include_str!("../data/news.md");
/// Wbudowane notatki po angielsku (`news.en.md`).
const EMBEDDED_NEWS_EN: &str = include_str!("../data/news.en.md");
/// Wbudowane notatki po niemiecku (`news.de.md`).
const EMBEDDED_NEWS_DE: &str = include_str!("../data/news.de.md");

/// Jedno wydanie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub date: String,
    pub items: Vec<String>,
}

/// Zestaw wydań, od najnowszego.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notes {
    pub releases: Vec<Release>,
    /// Skąd pochodzi treść (do diagnostyki, niewidoczne w UI).
    pub source: Option<PathBuf>,
}

impl Notes {
    /// Parsuje tekst notatek. Puste lub niepoprawne sekcje są pomijane.
    pub fn parse(src: &str) -> Self {
        let mut releases: Vec<Release> = Vec::new();
        let mut current: Option<Release> = None;

        for line in src.lines() {
            let trimmed = line.trim();
            if let Some(heading) = trimmed.strip_prefix("## ") {
                if let Some(release) = current.take() {
                    if !release.items.is_empty() {
                        releases.push(release);
                    }
                }
                let (version, date) = split_heading(heading);
                current = Some(Release {
                    version,
                    date,
                    items: Vec::new(),
                });
                continue;
            }
            let Some(item) = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))
            else {
                continue;
            };
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if let Some(release) = current.as_mut() {
                release.items.push(item.to_owned());
            } else {
                // Punkty bez nagłówka tworzą anonimowe wydanie.
                current = Some(Release {
                    version: String::new(),
                    date: String::new(),
                    items: vec![item.to_owned()],
                });
            }
        }
        if let Some(release) = current {
            if !release.items.is_empty() {
                releases.push(release);
            }
        }

        Self {
            releases,
            source: None,
        }
    }

    /// Najnowsze wydanie (pierwsze w pliku).
    pub fn latest(&self) -> Option<&Release> {
        self.releases.first()
    }

    pub fn is_empty(&self) -> bool {
        self.releases.is_empty()
    }
}

fn split_heading(heading: &str) -> (String, String) {
    for separator in [" — ", " – ", " - "] {
        if let Some((version, date)) = heading.split_once(separator) {
            return (version.trim().to_owned(), date.trim().to_owned());
        }
    }
    (heading.trim().to_owned(), String::new())
}

/// Katalogi, w których system może położyć notatki (w kolejności pierwszeństwa).
pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(custom) = std::env::var_os("LUNA_WELCOME_NEWS").filter(|v| !v.is_empty()) {
        paths.push(PathBuf::from(custom));
    }
    // Nadpisanie użytkownika ma pierwszeństwo przed systemem.
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
    {
        push_news_paths(&mut paths, &data);
    }
    push_news_paths(&mut paths, Path::new("/usr/local/share"));
    push_news_paths(&mut paths, Path::new("/usr/share"));
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty()) {
        for dir in std::env::split_paths(&dirs) {
            push_news_paths(&mut paths, &dir);
        }
    }
    paths
}

/// Kandydaci w jednym katalogu danych: Luna, potem pakiet wydania („os-info”)
/// — w PaganOS notatki wydania dostarcza `pagan-release`.
fn push_news_paths(paths: &mut Vec<PathBuf>, base: &Path) {
    for relative in [
        "luna/welcome/news.md",
        "os-info/release-notes.md",
        "os-info/news.md",
        "pagan-release/release-notes.md",
        "pagan-release/news.md",
    ] {
        paths.push(base.join(relative));
    }
}

/// Wczytuje pierwszy istniejący plik; w razie braku — wbudowane notatki.
pub fn load() -> Notes {
    let language = language_code();
    pick_notes(&language, &search_paths(), |path| {
        let text = std::fs::read_to_string(path).ok()?;
        let notes = Notes::parse(&text);
        (!notes.is_empty()).then_some(notes)
    })
}

/// Wybiera notatki dla języka: wariant językowy z dysku → wbudowane w języku
/// sesji → plik bazowy z dysku → wbudowane bazowe.
///
/// `read` jest wstrzykiwane, żeby kolejność dała się przetestować bez dysku.
fn pick_notes(
    language: &str,
    paths: &[PathBuf],
    mut read: impl FnMut(&Path) -> Option<Notes>,
) -> Notes {
    for base in paths {
        if let Some(path) = localized_variant(base, language) {
            if let Some(mut notes) = read(&path) {
                notes.source = Some(path);
                return notes;
            }
        }
    }
    // Plik bazowy jest po polsku — dla innego języka lepiej pokazać wbudowane
    // tłumaczenie niż polski tekst (o ile takie mamy).
    if language != BASE_LANGUAGE {
        if let Some(text) = embedded_notes(language) {
            return Notes::parse(text);
        }
    }
    for base in paths {
        if let Some(mut notes) = read(base) {
            notes.source = Some(base.clone());
            return notes;
        }
    }
    Notes::parse(EMBEDDED_NEWS)
}

/// Kod języka wybrany w Ustawieniach (`pl`, `en`, `de`).
fn language_code() -> String {
    libluna_i18n::locale().language().to_owned()
}

/// Wbudowane notatki dla języka; `None`, gdy nie mamy tłumaczenia.
fn embedded_notes(language: &str) -> Option<&'static str> {
    match language {
        "en" => Some(EMBEDDED_NEWS_EN),
        "de" => Some(EMBEDDED_NEWS_DE),
        "pl" => Some(EMBEDDED_NEWS),
        _ => None,
    }
}

/// Ścieżka wariantu językowego: `<nazwa>.<język>.<rozszerzenie>`.
fn localized_variant(path: &Path, language: &str) -> Option<PathBuf> {
    if language.is_empty() {
        return None;
    }
    let (stem, ext) = (path.file_stem()?, path.extension()?);
    let mut name = std::ffi::OsString::from(stem);
    name.push(".");
    name.push(language);
    name.push(".");
    name.push(ext);
    let mut localized = path.to_path_buf();
    localized.set_file_name(name);
    Some(localized)
}

/// Wersja systemu: `LUNA_VERSION`, pliki pakietu wydania (`/etc/luna-release`,
/// `/etc/pagan-release`, `/etc/os-release`), a na końcu notatki.
pub fn system_version(notes: &Notes) -> String {
    if let Ok(version) = std::env::var("LUNA_VERSION") {
        if !version.trim().is_empty() {
            return version.trim().to_owned();
        }
    }
    if let Some(version) = crate::os_info::version() {
        return version;
    }
    notes
        .latest()
        .map(|release| release.version.clone())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "0.0.8".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_and_bullets() {
        let notes = Notes::parse(
            "# komentarz\n## 0.0.9 — 2026-12-01\n- Pierwsza rzecz\n* Druga rzecz\n## 0.0.8\n- Starsza\n",
        );
        assert_eq!(notes.releases.len(), 2);
        let latest = notes.latest().unwrap();
        assert_eq!(latest.version, "0.0.9");
        assert_eq!(latest.date, "2026-12-01");
        assert_eq!(latest.items, vec!["Pierwsza rzecz", "Druga rzecz"]);
        assert_eq!(notes.releases[1].version, "0.0.8");
    }

    #[test]
    fn skips_empty_releases() {
        let notes = Notes::parse("## 1.0\n## 0.9 — 2026-01-01\n- Coś\n");
        assert_eq!(notes.releases.len(), 1);
        assert_eq!(notes.releases[0].version, "0.9");
    }

    #[test]
    fn bullets_without_heading_still_render() {
        let notes = Notes::parse("- Pierwsza wiadomość\n");
        assert_eq!(notes.releases.len(), 1);
        assert_eq!(notes.releases[0].items.len(), 1);
    }

    #[test]
    fn search_paths_include_os_info() {
        let paths = search_paths();
        let joined: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        assert!(joined
            .iter()
            .any(|p| p.ends_with("os-info/release-notes.md")));
        assert!(joined.iter().any(|p| p.ends_with("pagan-release/news.md")));
        assert!(joined.iter().any(|p| p.ends_with("luna/welcome/news.md")));
    }

    #[test]
    fn embedded_notes_are_available() {
        let notes = Notes::parse(EMBEDDED_NEWS);
        assert!(!notes.is_empty());
    }

    #[test]
    fn localized_variant_uses_language_suffix() {
        let path = localized_variant(Path::new("/usr/share/luna/welcome/news.md"), "en");
        assert_eq!(
            path,
            Some(PathBuf::from("/usr/share/luna/welcome/news.en.md"))
        );
        // Brak języka → brak wariantu (zostaje plik bazowy).
        assert_eq!(
            localized_variant(Path::new("/usr/share/luna/welcome/news.md"), ""),
            None
        );
    }

    #[test]
    fn embedded_notes_follow_the_language() {
        assert!(embedded_notes("en")
            .unwrap()
            .contains("installs and updates from pacman"));
        assert!(embedded_notes("de")
            .unwrap()
            .contains("Installation und Aktualisierungen"));
        assert!(embedded_notes("pl")
            .unwrap()
            .contains("instalacja i aktualizacje"));
        // Nieznany język → brak tłumaczenia (zostaje plik bazowy z dysku).
        assert!(embedded_notes("xx").is_none());
        for language in ["pl", "en", "de"] {
            assert!(
                !Notes::parse(embedded_notes(language).unwrap()).is_empty(),
                "brak notatek dla {language}"
            );
        }
    }

    #[test]
    fn disk_variant_wins_over_base_file() {
        let dir = std::env::temp_dir().join(format!("luna-news-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("news.md");
        std::fs::write(&base, "## 1.0 — 2026-01-01\n- Wersja polska\n").unwrap();
        let english = dir.join("news.en.md");
        std::fs::write(&english, "## 1.0 — 2026-01-01\n- English version\n").unwrap();

        let notes = pick_notes("en", std::slice::from_ref(&base), |path| {
            let text = std::fs::read_to_string(path).ok()?;
            Some(Notes::parse(&text)).filter(|n| !n.is_empty())
        });
        assert_eq!(notes.latest().unwrap().items[0], "English version");
        assert_eq!(notes.source.as_deref(), Some(english.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn embedded_translation_beats_foreign_base_file() {
        // Dyskowy plik jest po polsku, a sesja mówi po angielsku: lepiej pokazać
        // wbudowane angielskie notatki niż polski tekst z pakietu.
        let paths = vec![PathBuf::from("/usr/share/luna/welcome/news.md")];
        let notes = pick_notes("en", &paths, |_| None);
        assert!(notes.latest().unwrap().items[0].contains("installs and updates"));

        // Dla polskiego plik bazowy z dysku nadal wygrywa z wbudowanym.
        let read = |path: &Path| {
            (path.file_name() == Some(std::ffi::OsStr::new("news.md")))
                .then(|| Notes::parse("## 1.0 — 2026-01-01\n- z dysku\n"))
        };
        let notes = pick_notes("pl", &paths, read);
        assert_eq!(notes.latest().unwrap().items[0], "z dysku");
    }
}
