//! Kosz zgodny ze specyfikacją XDG Trash (`.trashinfo`).
//!
//! MVP używa kosza domowego (`$XDG_DATA_HOME/Trash`). Dla plików z innego systemu
//! plików `rename` zwróci `EXDEV` — wtedy kopiujemy i usuwamy oryginał (specyfikacja
//! dopuszcza też kosz na szczycie montowania; to kolejny krok).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::model::{now_secs, trash_date};

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Katalog kosza użytkownika.
pub fn trash_root() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share"))
        .join("Trash")
}

/// Przenosi plik/katalog do kosza; zwraca ścieżkę w `Trash/files`.
pub fn move_to_trash(path: &Path) -> io::Result<PathBuf> {
    move_to_trash_in(path, &trash_root())
}

/// Jak [`move_to_trash`], ale z jawnym katalogiem kosza (testy, inne montowania).
pub fn move_to_trash_in(path: &Path, root: &Path) -> io::Result<PathBuf> {
    let files = root.join("files");
    let info = root.join("info");
    fs::create_dir_all(&files)?;
    fs::create_dir_all(&info)?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "zła nazwa pliku"))?;
    let original = absolute(path);

    // Unikalna nazwa w koszu: `raport.pdf`, `raport.2.pdf`, …
    let (stem, ext) = split_name(name);
    let mut candidate = name.to_owned();
    let mut n = 1u32;
    while files.join(&candidate).symlink_metadata().is_ok()
        || info.join(format!("{candidate}.trashinfo")).exists()
    {
        n += 1;
        candidate = if ext.is_empty() {
            format!("{stem}.{n}")
        } else {
            format!("{stem}.{n}.{ext}")
        };
    }

    let dest = files.join(&candidate);
    move_path(path, &dest)?;

    let mut f = fs::File::create(info.join(format!("{candidate}.trashinfo")))?;
    writeln!(f, "[Trash Info]")?;
    writeln!(f, "Path={}", encode_path(&original))?;
    writeln!(f, "DeletionDate={}", trash_date(now_secs()))?;
    Ok(dest)
}

/// Wpis w koszu (z `.trashinfo`).
#[derive(Debug, Clone)]
pub struct Trashed {
    pub name: String,
    /// Ścieżka w `Trash/files`.
    pub path: PathBuf,
    /// Oryginalna lokalizacja.
    pub original: PathBuf,
    pub deleted: String,
}

/// Zawartość kosza (bez sortowania — robi to UI).
pub fn list() -> Vec<Trashed> {
    list_in(&trash_root())
}

pub fn list_in(root: &Path) -> Vec<Trashed> {
    let mut out = Vec::new();
    let info = root.join("info");
    let Ok(entries) = fs::read_dir(&info) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("trashinfo") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let mut original = PathBuf::new();
        let mut deleted = String::new();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("Path=") {
                original = PathBuf::from(decode_path(v));
            } else if let Some(v) = line.strip_prefix("DeletionDate=") {
                deleted = v.to_owned();
            }
        }
        out.push(Trashed {
            path: root.join("files").join(&name),
            name,
            original,
            deleted,
        });
    }
    out
}

/// Opróżnia kosz; zwraca `(liczba plików, zwolnione bajty)`.
pub fn empty_trash() -> io::Result<(usize, u64)> {
    empty_trash_in(&trash_root())
}

pub fn empty_trash_in(root: &Path) -> io::Result<(usize, u64)> {
    let files = root.join("files");
    let info = root.join("info");
    let mut count = 0usize;
    let mut bytes = 0u64;
    if let Ok(entries) = fs::read_dir(&files) {
        for entry in entries.flatten() {
            let path = entry.path();
            bytes += path_size(&path);
            remove_path(&path)?;
            count += 1;
        }
    }
    if let Ok(entries) = fs::read_dir(&info) {
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok((count, bytes))
}

/// Ścieżka `.trashinfo` odpowiadająca plikowi w `Trash/files`.
pub fn info_for(trashed: &Path) -> Option<PathBuf> {
    let name = trashed.file_name()?.to_string_lossy();
    let root = trashed.parent()?.parent()?;
    Some(root.join("info").join(format!("{name}.trashinfo")))
}

/// Przywraca plik z kosza na jego pierwotną ścieżkę (z unikalną nazwą,
/// jeśli coś już tam jest). Usuwa wpis `.trashinfo`.
pub fn restore(trashed: &Path) -> io::Result<PathBuf> {
    let info = info_for(trashed).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "nieprawidłowa ścieżka kosza")
    })?;
    let text = fs::read_to_string(&info)?;
    let original = text
        .lines()
        .find_map(|line| line.strip_prefix("Path="))
        .map(|v| PathBuf::from(decode_path(v)))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "brak Path w .trashinfo"))?;
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent)?;
    }
    let dest = if original.symlink_metadata().is_ok() {
        let dir = original.parent().unwrap_or_else(|| Path::new("/"));
        let name = original
            .file_name()
            .map(|n| n.to_os_string())
            .unwrap_or_else(|| std::ffi::OsString::from("plik"));
        let name_str = name.to_string_lossy().into_owned();
        let (stem, ext) = split_name(&name_str);
        let mut n = 1u32;
        loop {
            let candidate = if ext.is_empty() {
                dir.join(format!("{stem} ({n})"))
            } else {
                dir.join(format!("{stem} ({n}).{ext}"))
            };
            if candidate.symlink_metadata().is_err() {
                break candidate;
            }
            n += 1;
        }
    } else {
        original
    };
    move_path(trashed, &dest)?;
    let _ = fs::remove_file(&info);
    Ok(dest)
}

/// Usuwa wpis z kosza trwale (plik + `.trashinfo`).
pub fn purge(trashed: &Path) -> io::Result<()> {
    remove_path(trashed)?;
    if let Some(info) = info_for(trashed) {
        let _ = fs::remove_file(info);
    }
    Ok(())
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn move_path(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        // Inny system plików (pendrive, inna partycja): kopiujemy i usuwamy oryginał.
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_path(src, dst)?;
            remove_path(src)
        }
        Err(e) => Err(e),
    }
}

fn copy_path(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_path(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dst)
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

fn remove_path(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn path_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_dir() {
        let mut sum = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                sum += path_size(&entry.path());
            }
        }
        sum
    } else {
        meta.len()
    }
}

/// Procentowe kodowanie ścieżki zgodnie z XDG Trash Spec (bez kodowania `/`).
pub fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    for &b in path.as_os_str().as_encoded_bytes() {
        match b {
            b'/' | b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decode_path(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "luna-files-trash-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn trashing_keeps_content_and_writes_info() {
        let base = temp_dir("basic");
        let datadir = base.join("Trash");
        let file = base.join("notatka.txt");
        fs::write(&file, "treść").unwrap();

        let dest = move_to_trash_in(&file, &datadir).unwrap();
        assert!(!file.exists());
        assert_eq!(fs::read_to_string(&dest).unwrap(), "treść");

        let info = fs::read_to_string(datadir.join("info/notatka.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\n"));
        assert!(info.contains("Path="));
        assert!(info.contains("DeletionDate="));
        assert!(!info.contains("Path=%2F")); // slashe nie są kodowane
    }

    #[test]
    fn name_collisions_get_a_suffix() {
        let base = temp_dir("collision");
        let root = base.join("Trash");
        let a = base.join("dokument.txt");
        let b = base.join("dokument.txt");
        fs::write(&a, "a").unwrap();
        move_to_trash_in(&a, &root).unwrap();
        fs::write(&b, "b").unwrap();
        let dest = move_to_trash_in(&b, &root).unwrap();
        assert_eq!(dest.file_name().unwrap(), "dokument.2.txt");
        assert!(root.join("info/dokument.2.txt.trashinfo").exists());
    }

    #[test]
    fn listing_reads_original_path() {
        let base = temp_dir("list");
        let root = base.join("Trash");
        let file = base.join("plik.bin");
        fs::write(&file, [1u8, 2, 3]).unwrap();
        move_to_trash_in(&file, &root).unwrap();
        let items = list_in(&root);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "plik.bin");
        assert_eq!(items[0].original, file);
    }

    #[test]
    fn emptying_clears_files_and_info() {
        let base = temp_dir("empty");
        let root = base.join("Trash");
        let a = base.join("a.txt");
        fs::write(&a, "12345").unwrap();
        move_to_trash_in(&a, &root).unwrap();
        let (count, bytes) = empty_trash_in(&root).unwrap();
        assert_eq!(count, 1);
        assert_eq!(bytes, 5);
        assert!(list_in(&root).is_empty());
    }

    #[test]
    fn restore_puts_file_back() {
        let base = temp_dir("restore");
        let root = base.join("Trash");
        let original = base.join("dane.txt");
        fs::write(&original, "abc").unwrap();
        move_to_trash_in(&original, &root).unwrap();
        assert!(!original.exists());
        let restored = restore(&root.join("files/dane.txt")).unwrap();
        assert_eq!(restored, original);
        assert_eq!(fs::read_to_string(&restored).unwrap(), "abc");
        assert!(!root.join("info/dane.txt.trashinfo").exists());
    }

    #[test]
    fn purge_removes_file_and_info() {
        let base = temp_dir("purge");
        let root = base.join("Trash");
        let file = base.join("x.bin");
        fs::write(&file, [0u8; 4]).unwrap();
        let dest = move_to_trash_in(&file, &root).unwrap();
        purge(&dest).unwrap();
        assert!(!dest.exists());
        assert!(!root.join("info/x.bin.trashinfo").exists());
    }

    #[test]
    fn path_encoding_escapes_spaces() {
        assert_eq!(encode_path(Path::new("/a b/c.txt")), "/a%20b/c.txt");
        assert_eq!(decode_path("/a%20b/c.txt"), "/a b/c.txt");
    }
}
