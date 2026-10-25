//! Archiwa: rozpoznawanie formatu, budowa poleceń i uruchamianie narzędzi systemowych.
//!
//! Świadomie korzystamy z `bsdtar`/`tar`/`zip`/`7z`/`unrar` zamiast wciągać do
//! aplikacji kilka ciężkich bibliotek: libarchive obsługuje ZIP, TAR (gz/xz/bz2),
//! 7z i RAR, a tworzenie archiwów robią natywne narzędzia. Dzięki temu menedżer
//! obsługuje też formaty, których czysto-rustowe biblioteki nie znają (RAR).

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use libluna_i18n::tr;

/// Format archiwum rozpoznawany po nazwie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Tar,
    TarGz,
    TarXz,
    TarBz2,
    SevenZ,
    Rar,
}

impl Format {
    /// Rozszerzenie używane przy tworzeniu archiwum (bez kropki).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Zip => "zip",
            Format::Tar => "tar",
            Format::TarGz => "tar.gz",
            Format::TarXz => "tar.xz",
            Format::TarBz2 => "tar.bz2",
            Format::SevenZ => "7z",
            Format::Rar => "rar",
        }
    }
}

/// Czy plik wygląda na archiwum obsługiwanego formatu.
pub fn is_archive(path: &Path) -> bool {
    format_of(path).is_some()
}

/// Format archiwum na podstawie nazwy (małe/duże litery bez znaczenia).
pub fn format_of(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    // Kolejność ma znaczenie: `.tar.gz` przed `.gz`.
    let suffixes: [(&str, Format); 7] = [
        (".tar.gz", Format::TarGz),
        (".tgz", Format::TarGz),
        (".tar.xz", Format::TarXz),
        (".txz", Format::TarXz),
        (".tar.bz2", Format::TarBz2),
        (".tbz2", Format::TarBz2),
        (".zip", Format::Zip),
    ];
    for (suffix, format) in suffixes {
        if name.ends_with(suffix) {
            return Some(format);
        }
    }
    if name.ends_with(".tar") {
        return Some(Format::Tar);
    }
    if name.ends_with(".7z") {
        return Some(Format::SevenZ);
    }
    if name.ends_with(".rar") {
        return Some(Format::Rar);
    }
    None
}

/// Nazwa archiwum bez rozszerzenia (`projekt.tar.gz` → `projekt`).
pub fn archive_stem(path: &Path) -> String {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return String::new();
    };
    let lower = name.to_ascii_lowercase();
    for suffix in [
        ".tar.gz", ".tar.xz", ".tar.bz2", ".tgz", ".txz", ".tbz2", ".zip", ".tar", ".7z", ".rar",
    ] {
        if lower.ends_with(suffix) {
            return name[..name.len() - suffix.len()].to_owned();
        }
    }
    name
}

/// Polecenie do uruchomienia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub program: String,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
}

impl Plan {
    fn new(program: impl Into<String>, args: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: None,
        }
    }
}

/// Buduje polecenie pakowania `items` do `dest` (format z rozszerzenia `dest`).
pub fn compress_plan(items: &[PathBuf], dest: &Path) -> io::Result<Plan> {
    let Some(format) = format_of(dest) else {
        return Err(invalid(tr("Unsupported archive format")));
    };
    if items.is_empty() {
        return Err(invalid(tr("Nothing selected")));
    }
    let parent = items[0]
        .parent()
        .ok_or_else(|| invalid(tr("Invalid path")))?
        .to_path_buf();
    if items.iter().any(|p| p.parent() != Some(parent.as_path())) {
        return Err(invalid(tr("Selected items must share one folder")));
    }
    // `zip`/`tar`/`7z` zapisują ścieżki względne — podajemy same nazwy, a katalog
    // roboczy ustawiamy na wspólny rodzic. Dzięki temu archiwum nie zawiera
    // `/home/u/...`, tylko wybrane pliki.
    let names: Vec<OsString> = items
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_os_string()))
        .collect();
    if names.len() != items.len() {
        return Err(invalid(tr("Invalid path")));
    }
    let dest_name = if dest.parent() == Some(parent.as_path()) {
        dest.file_name()
            .map(|n| n.to_os_string())
            .ok_or_else(|| invalid(tr("Invalid path")))?
    } else {
        dest.as_os_str().to_os_string()
    };

    let mut args: Vec<OsString> = Vec::new();
    let program = match format {
        Format::Zip => {
            args.push(OsString::from("-r"));
            args.push(OsString::from("-y")); // symlinki bez podążania
            args.push(OsString::from("-q"));
            args.push(dest_name);
            args.extend(names);
            "zip"
        }
        Format::SevenZ => {
            args.push(OsString::from("a"));
            args.push(OsString::from("-bd"));
            args.push(OsString::from("-y"));
            args.push(dest_name);
            args.push(OsString::from("--"));
            args.extend(names);
            "7z"
        }
        Format::Tar | Format::TarGz | Format::TarXz | Format::TarBz2 => {
            let flag = match format {
                Format::Tar => "-cf",
                Format::TarGz => "-czf",
                Format::TarXz => "-cJf",
                Format::TarBz2 => "-cjf",
                _ => unreachable!(),
            };
            args.push(OsString::from(flag));
            args.push(dest_name);
            args.push(OsString::from("--"));
            args.extend(names);
            "tar"
        }
        // RAR tworzymy tylko przez własnościowe `rar`; nasze archiwa tworzymy
        // w formatach otwartych — RAR obsługujemy wyłącznie do wypakowania.
        Format::Rar => return Err(invalid(tr("Unsupported archive format"))),
    };
    let mut plan = Plan::new(program, args);
    plan.cwd = Some(parent);
    Ok(plan)
}

/// Buduje polecenie wypakowania `archive` do katalogu `dest`.
pub fn extract_plan(archive: &Path, dest: &Path) -> io::Result<Plan> {
    let Some(format) = format_of(archive) else {
        return Err(invalid(tr("Unsupported archive format")));
    };
    let mut plan = if on_path("bsdtar") {
        // libarchive: ZIP, TAR (gz/xz/bz2), 7z, RAR — jedna ścieżka dla wszystkiego.
        Plan::new(
            "bsdtar",
            vec![
                OsString::from("-xf"),
                archive.as_os_str().to_os_string(),
                OsString::from("-C"),
                dest.as_os_str().to_os_string(),
            ],
        )
    } else {
        match format {
            Format::Zip => Plan::new(
                "unzip",
                vec![
                    OsString::from("-q"),
                    OsString::from("-o"),
                    archive.as_os_str().to_os_string(),
                    OsString::from("-d"),
                    dest.as_os_str().to_os_string(),
                ],
            ),
            Format::SevenZ => Plan::new(
                "7z",
                vec![
                    OsString::from("x"),
                    OsString::from("-y"),
                    OsString::from("-bd"),
                    OsString::from(format!("-o{}", dest.display())),
                    archive.as_os_str().to_os_string(),
                ],
            ),
            Format::Rar => Plan::new(
                "unrar",
                vec![
                    OsString::from("x"),
                    OsString::from("-o+"),
                    archive.as_os_str().to_os_string(),
                    dest.as_os_str().to_os_string(),
                ],
            ),
            Format::Tar | Format::TarGz | Format::TarXz | Format::TarBz2 => Plan::new(
                "tar",
                vec![
                    OsString::from("-xf"),
                    archive.as_os_str().to_os_string(),
                    OsString::from("-C"),
                    dest.as_os_str().to_os_string(),
                ],
            ),
        }
    };
    // `unrar`/`7z` przyjmują katalog docelowy jako argument; tar/bsdtar przez `-C`.
    plan.cwd = None;
    Ok(plan)
}

/// Pakuje `items` do `dest`. Blokuje wątek operacji; `cancel` zatrzymuje proces.
pub fn compress(items: &[PathBuf], dest: &Path, cancel: &AtomicBool) -> io::Result<()> {
    let plan = compress_plan(items, dest)?;
    run(&plan, cancel)
}

/// Wypakowuje `archive` do `dest` (katalog tworzony, jeśli nie istnieje).
pub fn extract(archive: &Path, dest: &Path, cancel: &AtomicBool) -> io::Result<()> {
    std::fs::create_dir_all(dest)?;
    let plan = extract_plan(archive, dest)?;
    run(&plan, cancel)
}

/// Wolna nazwa w `dir`: `base.zip`, `base (2).zip`, …
pub fn unique_path(dir: &Path, base: &str, extension: &str) -> PathBuf {
    for n in 1.. {
        let name = if n == 1 {
            format!("{base}.{extension}")
        } else {
            format!("{base} ({n}).{extension}")
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn run(plan: &Plan, cancel: &AtomicBool) -> io::Result<()> {
    let mut cmd = Command::new(&plan.program);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(cwd) = &plan.cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd.spawn().map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("{}: {}", tr("Archive tool not found"), plan.program),
        )
    })?;
    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::Interrupted, tr("Cancelled")));
        }
        match child.try_wait()? {
            Some(status) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                if status.success() {
                    return Ok(());
                }
                let message = stderr.trim();
                return Err(io::Error::other(if message.is_empty() {
                    format!("{}: {status}", plan.program)
                } else {
                    message.to_owned()
                }));
            }
            None => std::thread::sleep(Duration::from_millis(60)),
        }
    }
}

/// Czy program jest w `PATH` (żeby wybrać `bsdtar` zamiast `unzip`/`tar`).
fn on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(program);
        candidate.is_file()
    })
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("luna-files-archive-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn zip_roundtrip_with_real_tool() {
        if !on_path("zip") || !on_path("bsdtar") {
            return; // środowisko bez narzędzi — reszta testów pokrywa plany
        }
        let dir = temp_dir("zip-roundtrip");
        let src = dir.join("hello.txt");
        std::fs::write(&src, b"luna").unwrap();
        let dest = dir.join("pack.zip");
        let cancel = AtomicBool::new(false);
        compress(std::slice::from_ref(&src), &dest, &cancel).unwrap();
        assert!(dest.is_file());
        std::fs::remove_file(&src).unwrap();
        let out = dir.join("out");
        extract(&dest, &out, &cancel).unwrap();
        assert_eq!(std::fs::read(out.join("hello.txt")).unwrap(), b"luna");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tar_gz_roundtrip_with_real_tool() {
        if !on_path("tar") || !on_path("bsdtar") {
            return;
        }
        let dir = temp_dir("tar-roundtrip");
        let src = dir.join("nested.txt");
        std::fs::write(&src, b"selene").unwrap();
        let dest = dir.join("pack.tar.gz");
        let cancel = AtomicBool::new(false);
        compress(std::slice::from_ref(&src), &dest, &cancel).unwrap();
        std::fs::remove_file(&src).unwrap();
        let out = dir.join("out");
        extract(&dest, &out, &cancel).unwrap();
        assert_eq!(std::fs::read(out.join("nested.txt")).unwrap(), b"selene");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_archive_formats_case_insensitively() {
        assert_eq!(format_of(&p("a.zip")), Some(Format::Zip));
        assert_eq!(format_of(&p("A.TAR.GZ")), Some(Format::TarGz));
        assert_eq!(format_of(&p("x.tgz")), Some(Format::TarGz));
        assert_eq!(format_of(&p("x.tar.xz")), Some(Format::TarXz));
        assert_eq!(format_of(&p("x.tbz2")), Some(Format::TarBz2));
        assert_eq!(format_of(&p("x.7z")), Some(Format::SevenZ));
        assert_eq!(format_of(&p("x.rar")), Some(Format::Rar));
        assert_eq!(format_of(&p("x.txt")), None);
    }

    #[test]
    fn archive_stem_strips_double_extension() {
        assert_eq!(archive_stem(&p("projekt.tar.gz")), "projekt");
        assert_eq!(archive_stem(&p("projekt.ZIP")), "projekt");
        assert_eq!(archive_stem(&p("bez-rozszerzenia")), "bez-rozszerzenia");
    }

    #[test]
    fn zip_plan_uses_relative_names_and_parent_cwd() {
        let items = vec![p("/home/u/dir/a.txt"), p("/home/u/dir/b")];
        let plan = compress_plan(&items, &p("/home/u/dir/pack.zip")).unwrap();
        assert_eq!(plan.program, "zip");
        assert_eq!(plan.cwd.as_deref(), Some(Path::new("/home/u/dir")));
        let args: Vec<String> = plan
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"pack.zip".to_owned()));
        assert!(args.contains(&"a.txt".to_owned()));
        assert!(args.contains(&"b".to_owned()));
        assert!(!args.iter().any(|a| a.starts_with("/home/u")));
    }

    #[test]
    fn tar_plan_picks_compression_flag() {
        let items = vec![p("/tmp/x")];
        let plan = compress_plan(&items, &p("/tmp/x.tar.xz")).unwrap();
        assert_eq!(plan.program, "tar");
        assert!(plan.args[0].to_string_lossy().contains("J"));
    }

    #[test]
    fn extract_plan_contains_archive_and_destination() {
        let plan = extract_plan(&p("/tmp/a.zip"), &p("/tmp/out")).unwrap();
        let args: Vec<String> = plan
            .args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a == "/tmp/a.zip"));
        assert!(args.iter().any(|a| a == "/tmp/out"));
    }

    #[test]
    fn rejects_items_from_different_folders() {
        let items = vec![p("/tmp/a/one"), p("/tmp/b/two")];
        assert!(compress_plan(&items, &p("/tmp/a/x.zip")).is_err());
    }

    #[test]
    fn unique_path_numbers_collisions() {
        let dir = std::env::temp_dir().join("luna-files-archive-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Paczka.zip"), b"x").unwrap();
        let next = unique_path(&dir, "Paczka", "zip");
        assert_eq!(next.file_name().unwrap(), "Paczka (2).zip");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
