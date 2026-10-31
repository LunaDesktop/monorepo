//! Wspólne pomocniki plików konfiguracyjnych Luny (`$XDG_CONFIG_HOME/luna`).
//!
//! Wszystkie ustawienia per-użytkownik mieszkają w jednym katalogu i używają
//! prostego formatu `klucz = wartość`. Zapis jest atomowy, żeby przerwany proces
//! nie zostawił pustego pliku, a uprawnienia są czytelne dla innych procesów sesji.

use std::io;
use std::path::{Path, PathBuf};

/// Katalog konfiguracji użytkownika (`$XDG_CONFIG_HOME/luna`).
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("luna"))
}

/// Ścieżka pliku konfiguracji o nazwie `name` w katalogu Luny.
pub fn config_file(name: &str) -> Option<PathBuf> {
    Some(config_dir()?.join(name))
}

/// Rozbija tekst `klucz = wartość` na pary (komentarze i puste linie pomijane).
pub fn parse_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            out.push((key.trim().to_owned(), value.trim().to_owned()));
        }
    }
    out
}

/// Zapisuje tekst atomowo (plik tymczasowy + `rename`), z prawami 0644.
pub fn save_atomic(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        let mut file = std::fs::File::create(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Zapisuje tekst atomowo z prawami `0600` (dane prywatne użytkownika).
pub fn save_atomic_private(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(text.as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Bezpieczna nazwa/tekst z konfiguracji (bez znaków kontrolnych).
pub fn clean_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 || value.contains(['\n', '\r', '\0']) {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_pairs() {
        let pairs = parse_pairs("# komentarz\n[a]\nfoo = bar\nbaz=qux\n");
        assert_eq!(
            pairs,
            vec![
                ("foo".to_owned(), "bar".to_owned()),
                ("baz".to_owned(), "qux".to_owned())
            ]
        );
    }

    #[test]
    fn atomic_save_roundtrips() {
        let dir = std::env::temp_dir().join(format!("luna-config-{}", std::process::id()));
        let path = dir.join("test.conf");
        save_atomic(&path, "a = 1\n").expect("zapis");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a = 1\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_control_characters() {
        assert!(clean_value("ok").is_some());
        assert!(clean_value("a\nb").is_none());
        assert!(clean_value("   ").is_none());
    }
}
