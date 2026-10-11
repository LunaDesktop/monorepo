//! Wykrywanie zainstalowanych aplikacji na podstawie plików `.desktop` (XDG).
//!
//! Skanujemy katalogi `applications/` z `XDG_DATA_HOME` i `XDG_DATA_DIRS`, parsujemy
//! sekcję `[Desktop Entry]` i zwracamy listę aplikacji do pokazania w launcherze.
//! Kolejność katalogów = priorytet (użytkownik nad systemem); duplikaty po `id` wygrywa
//! pierwszy napotkany.
//!
//! Parsowanie jest czyste ([`parse`]) — łatwe do testów bez dostępu do systemu plików.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use libluna_widgets::IconName;

/// Aplikacja z pliku `.desktop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub comment: Option<String>,
    pub terminal: bool,
    pub categories: Vec<String>,
}

/// Język z otoczenia (`LANG`/`LC_MESSAGES`), np. `pl` z `pl_PL.UTF-8`.
pub fn current_lang() -> Option<String> {
    let raw = std::env::var("LC_MESSAGES")
        .or_else(|_| std::env::var("LANG"))
        .ok()?;
    let lang = raw.split(['_', '.', '@']).next()?.trim();
    if lang.is_empty() || lang == "C" || lang == "POSIX" {
        None
    } else {
        Some(lang.to_ascii_lowercase())
    }
}

/// Usuwa kody pól (`%U`, `%f`, `%i`…) z polecenia `Exec`.
fn strip_field_codes(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|tok| !(tok.starts_with('%') && tok.len() == 2))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parsuje treść pliku `.desktop`. Zwraca `None`, gdy to nie jest wyświetlana aplikacja.
pub fn parse(content: &str, lang: Option<&str>) -> Option<DesktopApp> {
    let mut in_entry = false;
    let mut name = None;
    let mut name_loc = None;
    let mut comment = None;
    let mut exec = None;
    let mut icon = None;
    let mut terminal = false;
    let mut ty = None;
    let mut nodisplay = false;
    let mut hidden = false;
    let mut categories = Vec::new();

    let name_key = lang.map(|l| format!("Name[{l}]"));
    let comment_key = lang.map(|l| format!("Comment[{l}]"));

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "Type" => ty = Some(v.to_owned()),
            "Name" => name = Some(v.to_owned()),
            "Comment" => comment = Some(v.to_owned()),
            "Exec" => exec = Some(v.to_owned()),
            "Icon" => icon = Some(v.to_owned()),
            "Terminal" => terminal = v.eq_ignore_ascii_case("true"),
            "NoDisplay" => nodisplay = v.eq_ignore_ascii_case("true"),
            "Hidden" => hidden = v.eq_ignore_ascii_case("true"),
            "Categories" => {
                categories = v
                    .split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_owned())
                    .collect();
            }
            _ => {
                if name_key.as_deref() == Some(k) {
                    name_loc = Some(v.to_owned());
                } else if comment_key.as_deref() == Some(k) {
                    comment = Some(v.to_owned());
                }
            }
        }
    }

    if ty.as_deref() != Some("Application") || nodisplay || hidden {
        return None;
    }
    Some(DesktopApp {
        name: name_loc.or(name)?,
        exec: strip_field_codes(&exec?),
        icon,
        comment,
        terminal,
        categories,
    })
}

/// Skanuje system w poszukiwaniu aplikacji (posortowane po nazwie).
pub fn scan() -> Vec<DesktopApp> {
    let lang = current_lang();
    let mut seen: HashSet<String> = HashSet::new();
    let mut apps = Vec::new();

    for dir in application_dirs() {
        for path in desktop_files(&dir, 0) {
            let id = path
                .strip_prefix(&dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('/', "-");
            if !seen.insert(id) {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(app) = parse(&content, lang.as_deref()) {
                    apps.push(app);
                }
            }
        }
    }

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Katalogi `applications/` w kolejności priorytetu (użytkownik pierwszy).
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(d).join("applications"));
    }
    dirs
}

/// Zbiera pliki `.desktop` z katalogu (rekurencyjnie do głębokości 2).
fn desktop_files(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 2 {
                out.extend(desktop_files(&path, depth + 1));
            }
        } else if path.extension().map(|e| e == "desktop").unwrap_or(false) {
            out.push(path);
        }
    }
    out
}

/// Dobiera ikonę z pakietu Luny na podstawie kategorii/nazwy (heurystyka).
pub fn icon_for(app: &DesktopApp) -> IconName {
    let cats = app.categories.join(" ").to_lowercase();
    let name = app.name.to_lowercase();
    let has = |needle: &str| cats.contains(needle) || name.contains(needle);

    if has("filemanager") || has("files") || name.contains("pliki") {
        IconName::Files
    } else if has("terminal") || has("console") || name.contains("terminal") {
        IconName::Terminal
    } else if has("audio") || has("video") || has("player") || has("music") {
        IconName::Music
    } else if has("browser") || has("network") || has("wifi") {
        IconName::Wifi
    } else if has("settings") || has("preferences") || has("system") {
        IconName::Settings
    } else if has("office") || has("texteditor") || has("editor") {
        IconName::Edit
    } else if has("game") {
        IconName::Star
    } else if has("graphics") || has("photography") {
        IconName::Edit
    } else {
        IconName::App
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIREFOX: &str = "\
[Desktop Entry]
Type=Application
Name=Firefox
Name[pl]=Firefox
GenericName=Web Browser
Comment=Browse the Web
Exec=firefox %u
Icon=firefox
Terminal=false
Categories=Network;WebBrowser;
";

    #[test]
    fn parses_application_entry() {
        let app = parse(FIREFOX, Some("pl")).unwrap();
        assert_eq!(app.name, "Firefox");
        assert_eq!(app.exec, "firefox"); // %u usunięte
        assert!(!app.terminal);
        assert!(app.categories.contains(&"WebBrowser".to_owned()));
        assert_eq!(icon_for(&app), IconName::Wifi);
    }

    #[test]
    fn prefers_localized_name() {
        let content =
            "[Desktop Entry]\nType=Application\nName=Files\nName[pl]=Pliki\nExec=nautilus\n";
        assert_eq!(parse(content, Some("pl")).unwrap().name, "Pliki");
        // Bez dopasowania języka → nazwa bazowa.
        assert_eq!(parse(content, Some("de")).unwrap().name, "Files");
    }

    #[test]
    fn skips_non_application_and_hidden() {
        assert!(parse("[Desktop Entry]\nType=Link\nName=x\nExec=x\n", None).is_none());
        assert!(parse(
            "[Desktop Entry]\nType=Application\nName=x\nExec=x\nNoDisplay=true\n",
            None
        )
        .is_none());
        assert!(parse(
            "[Desktop Entry]\nType=Application\nName=x\nExec=x\nHidden=true\n",
            None
        )
        .is_none());
        assert!(parse("[Desktop Entry]\nType=Application\nName=x\n", None).is_none());
    }

    #[test]
    fn ignores_unrelated_sections() {
        let content = "[Desktop Action New]\nName=Nowy\nExec=touch\n\n[Desktop Entry]\nType=Application\nName=Pliki\nExec=nautilus\n";
        assert_eq!(parse(content, None).unwrap().name, "Pliki");
    }

    #[test]
    fn icon_heuristics_cover_common_categories() {
        let mk = |cat: &str, name: &str| DesktopApp {
            name: name.into(),
            exec: "x".into(),
            icon: None,
            comment: None,
            terminal: false,
            categories: cat
                .split(';')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect(),
        };
        assert_eq!(
            icon_for(&mk("System;FileManager;", "Pliki")),
            IconName::Files
        );
        assert_eq!(
            icon_for(&mk("System;TerminalEmulator;", "Terminal")),
            IconName::Terminal
        );
        assert_eq!(
            icon_for(&mk("AudioVideo;Player;", "Odtwarzacz")),
            IconName::Music
        );
        assert_eq!(icon_for(&mk("Utility;", "Costam")), IconName::App);
    }
}
