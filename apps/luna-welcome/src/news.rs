//! „Co nowego” — wczytywanie i parsowanie notatek wydania.
//!
//! Format jest celowo prosty (Markdown bez zależności): nagłówek wydania
//! `## 0.0.8 — 2026-11-05` oraz punkty zaczynające się od `- ` albo `* `.
//! Plik z dysku ma pierwszeństwo nad wbudowanym, więc wydanie systemu może
//! podmienić treść bez przebudowy aplikacji.

use std::path::{Path, PathBuf};

/// Wbudowane notatki — używane, gdy system nie dostarczył własnego pliku.
pub const EMBEDDED_NEWS: &str = include_str!("../data/news.md");

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
    for path in search_paths() {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut notes = Notes::parse(&text);
        if !notes.is_empty() {
            notes.source = Some(path);
            return notes;
        }
    }
    Notes::parse(EMBEDDED_NEWS)
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
}
