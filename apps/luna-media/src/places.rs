//! Miejsca w panelu bocznym: katalog domowy i katalogi XDG użytkownika.

use std::fs;
use std::path::{Path, PathBuf};

use libluna_i18n::tr;
use libluna_widgets::IconName;

/// Skrót do katalogu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
    pub icon: IconName,
}

/// Katalog domowy użytkownika.
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Ścieżka z `user-dirs.dirs` (np. `XDG_PICTURES_DIR`).
pub fn xdg_dir(key: &str) -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    let text = fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = rest.trim().trim_matches('"');
        let value = value.replace("$HOME", &home().display().to_string());
        let path = PathBuf::from(value);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// Lista miejsc do panelu bocznego (bez duplikatów, tylko istniejące katalogi).
pub fn list() -> Vec<Place> {
    let home = home();
    let mut places = Vec::new();
    let mut push = |label: &str, path: Option<PathBuf>, icon: IconName| {
        let path = path.unwrap_or_else(|| home.join(label));
        if path.is_dir() && !places.iter().any(|p: &Place| p.path == path) {
            places.push(Place {
                label: tr(label),
                path,
                icon,
            });
        }
    };
    push("Home", Some(home.clone()), IconName::Home);
    push(
        "Pictures",
        xdg_dir("XDG_PICTURES_DIR").or_else(|| Some(home.join("Pictures"))),
        IconName::Image,
    );
    push(
        "Videos",
        xdg_dir("XDG_VIDEOS_DIR").or_else(|| Some(home.join("Videos"))),
        IconName::Video,
    );
    push(
        "Downloads",
        xdg_dir("XDG_DOWNLOAD_DIR").or_else(|| Some(home.join("Downloads"))),
        IconName::Document,
    );
    push(
        "Desktop",
        xdg_dir("XDG_DESKTOP_DIR").or_else(|| Some(home.join("Desktop"))),
        IconName::Monitor,
    );
    places
}

/// Pierwszy sensowny katalog startowy: Obrazy, potem Wideo, potem domowy.
pub fn start_dir() -> PathBuf {
    for key in ["XDG_PICTURES_DIR", "XDG_VIDEOS_DIR"] {
        if let Some(path) = xdg_dir(key) {
            return path;
        }
    }
    home()
}

/// Czy `path` jest w katalogu domowym (dla skracania etykiet ścieżki).
pub fn in_home(path: &Path) -> bool {
    path.starts_with(home())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_has_unique_existing_paths() {
        let places = list();
        let mut paths: Vec<&Path> = places.iter().map(|p| p.path.as_path()).collect();
        let len = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), len);
        assert!(places.iter().all(|p| p.path.is_dir()));
    }
}
