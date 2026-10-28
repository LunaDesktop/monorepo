//! Miejsca (XDG) i urządzenia (montowania) do panelu bocznego.
//!
//! Lista montowań czytana jest z `/proc/self/mountinfo`, a rozmiary przez `statvfs`.
//! Montowanie/odmontowanie idzie przez `udisksctl` (UDisks2 po D-Busie, z polkit —
//! hasło pyta `luna-polkit`). Docelowo podmienimy to na bezpośrednie sygnały D-Bus;
//! interfejs funkcji zostaje ten sam.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use libluna_i18n::tr;

#[derive(Debug, Clone)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
}

impl Place {
    fn new(label: &str, path: PathBuf) -> Self {
        Self {
            label: tr(label),
            path,
        }
    }
}

/// Katalogi użytkownika z `~/.config/user-dirs.dirs` (z wartościami domyślnymi).
pub fn user_dirs() -> BTreeMap<String, PathBuf> {
    let home = home();
    let mut map = BTreeMap::new();
    for key in [
        "DESKTOP",
        "DOCUMENTS",
        "DOWNLOAD",
        "PICTURES",
        "MUSIC",
        "VIDEOS",
        "TEMPLATES",
        "PUBLICSHARE",
    ] {
        map.insert(key.to_owned(), home.clone());
    }
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("user-dirs.dirs");
    if let Ok(text) = fs::read_to_string(config) {
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(key) = key.trim().strip_prefix("XDG_") else {
                continue;
            };
            let Some(key) = key.strip_suffix("_DIR") else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            let path = value.replace("$HOME", &home.display().to_string());
            if !path.is_empty() {
                map.insert(key.to_owned(), PathBuf::from(path));
            }
        }
    }
    map
}

pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Miejsca w panelu bocznym (tylko istniejące katalogi).
pub fn places() -> Vec<Place> {
    let dirs = user_dirs();
    let mut out = vec![Place::new("Home", home())];
    for (key, label) in [
        ("DESKTOP", "Desktop"),
        ("DOCUMENTS", "Documents"),
        ("DOWNLOAD", "Downloads"),
        ("PICTURES", "Pictures"),
        ("MUSIC", "Music"),
        ("VIDEOS", "Videos"),
    ] {
        if let Some(path) = dirs.get(key) {
            if path.is_dir() && path != &home() {
                out.push(Place::new(label, path.clone()));
            }
        }
    }
    out.push(Place::new("File System", PathBuf::from("/")));
    out
}

/// Zamontowane urządzenie widoczne w panelu bocznym.
#[derive(Debug, Clone)]
pub struct Mount {
    pub dev: String,
    pub label: String,
    /// Punkt montowania; pusty, gdy urządzenie nie jest zamontowane.
    pub mount: PathBuf,
    pub fs: String,
    pub total: u64,
    pub free: u64,
    /// Rozmiar nośnika (z `lsblk`); dla montowań bez `lsblk` równy `total`.
    pub size: u64,
    pub mounted: bool,
}

/// Urządzenia widoczne w panelu: **wszystkie** wymienne nośniki (także
/// niezamontowane, np. świeżo podpięty pendrive) plus zamontowane wolumeny
/// użytkowe z `/proc/self/mountinfo`.
pub fn devices() -> Vec<Mount> {
    let mut out = block_devices();
    for mount in mounts() {
        let duplicate = out.iter().any(|d| {
            d.dev == mount.dev || (!mount.mount.as_os_str().is_empty() && d.mount == mount.mount)
        });
        if !duplicate {
            out.push(mount);
        }
    }
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

/// Wymienne nośniki z `lsblk` — również te bez punktu montowania.
fn block_devices() -> Vec<Mount> {
    let rows = match lsblk_rows() {
        Some(rows) => rows,
        None => return Vec::new(),
    };
    let removable_disks: std::collections::HashSet<String> = rows
        .iter()
        .filter(|row| {
            row_value(row, "TYPE") == "disk"
                && (row_value(row, "RM") == "1" || row_value(row, "TRAN") == "usb")
        })
        .map(|row| row_value(row, "NAME"))
        .collect();

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &rows {
        let kind = row_value(row, "TYPE");
        if kind != "disk" && kind != "part" {
            continue;
        }
        let name = row_value(row, "NAME");
        let fs = row_value(row, "FSTYPE");
        let removable = row_value(row, "RM") == "1"
            || row_value(row, "TRAN") == "usb"
            || removable_disks.contains(&row_value(row, "PKNAME"));
        // Niezamontowany dysk z tablicą partycji ma puste FSTYPE — pokazujemy
        // jego partycje. Dysk bez partycji („superfloppy”) ma FSTYPE i też jest wart wpisu.
        if !removable || (kind == "disk" && fs.is_empty()) {
            continue;
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        let dev = format!("/dev/{name}");
        let mut mounted_at = row_value(row, "MOUNTPOINT");
        if let Some((first, _)) = mounted_at.split_once(',') {
            mounted_at = first.to_owned();
        }
        let mounted = !mounted_at.is_empty()
            && !mounted_at.starts_with('[')
            && Path::new(&mounted_at).is_dir();
        let mount = if mounted {
            PathBuf::from(&mounted_at)
        } else {
            PathBuf::new()
        };
        let (total, free) = if mounted {
            statvfs(&mount).unwrap_or((0, 0))
        } else {
            (0, 0)
        };
        let size = row_value(row, "SIZE")
            .parse::<u64>()
            .unwrap_or(total)
            .max(total);
        let label = {
            let from_lsblk = row_value(row, "LABEL");
            if !from_lsblk.is_empty() {
                from_lsblk
            } else {
                mount_label(&dev).unwrap_or(name)
            }
        };
        out.push(Mount {
            dev,
            label,
            mount,
            fs,
            total,
            free,
            size,
            mounted,
        });
    }
    out
}

fn row_value(row: &std::collections::HashMap<String, String>, key: &str) -> String {
    row.get(key).cloned().unwrap_or_default()
}

/// Uruchamia `lsblk -P` i parsuje wiersze `KLUCZ="wartość"`.
fn lsblk_rows() -> Option<Vec<std::collections::HashMap<String, String>>> {
    let output = Command::new("lsblk")
        .args([
            "-b",
            "-P",
            "-o",
            "NAME,FSTYPE,LABEL,SIZE,MOUNTPOINT,RM,TYPE,TRAN,PKNAME",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let rows: Vec<_> = text.lines().filter_map(parse_lsblk_line).collect();
    (!rows.is_empty()).then_some(rows)
}

/// Parsuje jedną linię `lsblk -P` (wartości w cudzysłowach mogą zawierać spacje).
fn parse_lsblk_line(line: &str) -> Option<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() != Some(&'=') {
            break;
        }
        chars.next();
        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(escaped) = chars.next() {
                            value.push(escaped);
                        }
                    }
                    '"' => break,
                    _ => value.push(c),
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        map.insert(key, value);
    }
    (!map.is_empty()).then_some(map)
}

/// Lista montowań „użytkowych” (pomija pseudosystemy i montowania systemowe).
pub fn mounts() -> Vec<Mount> {
    let Ok(text) = fs::read_to_string("/proc/self/mountinfo") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let Some(m) = parse_mountinfo(line) else {
            continue;
        };
        if !user_visible_mount(&m.mount, &m.fs) {
            continue;
        }
        if !seen.insert(m.mount.clone()) {
            continue;
        }
        let (total, free) = statvfs(&m.mount).unwrap_or((0, 0));
        out.push(Mount {
            label: mount_label(&m.dev).unwrap_or_else(|| {
                m.mount
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| m.dev.clone())
            }),
            dev: m.dev,
            mount: m.mount,
            fs: m.fs,
            total,
            free,
            size: total,
            mounted: true,
        });
    }
    out.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()));
    out
}

struct MountInfo {
    dev: String,
    mount: PathBuf,
    fs: String,
}

/// Parsuje jedną linię `/proc/self/mountinfo`:
/// `36 35 98:0 /root /mnt rw - ext4 /dev/sda1 rw`.
fn parse_mountinfo(line: &str) -> Option<MountInfo> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 7 {
        return None;
    }
    let mount = PathBuf::from(unescape(parts[4]));
    let sep = parts.iter().position(|p| *p == "-")?;
    let fs = parts.get(sep + 1)?.to_string();
    let dev = parts.get(sep + 2).unwrap_or(&"").to_string();
    Some(MountInfo { dev, mount, fs })
}

fn user_visible_mount(mount: &Path, fs: &str) -> bool {
    const FS: [&str; 18] = [
        "ext2", "ext3", "ext4", "btrfs", "xfs", "f2fs", "vfat", "exfat", "ntfs", "ntfs3",
        "fuseblk", "iso9660", "udf", "hfsplus", "hfs", "msdos", "jfs", "reiserfs",
    ];
    if !FS.contains(&fs) {
        return false;
    }
    const SYSTEM: [&str; 9] = [
        "/",
        "/boot",
        "/boot/efi",
        "/home",
        "/tmp",
        "/var",
        "/usr",
        "/opt",
        "/srv",
    ];
    if SYSTEM.iter().any(|s| mount == Path::new(s)) {
        return false;
    }
    true
}

/// Odkodowuje oktalne sekwencje z mountinfo (`\040` = spacja).
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&text[i + 1..i + 4], 8) {
                out.push(v);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Etykieta wolumenu z `/dev/disk/by-label` (np. „PENDRIVE”).
pub fn mount_label(dev: &str) -> Option<String> {
    let dir = Path::new("/dev/disk/by-label");
    let target = fs::canonicalize(dev).ok()?;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        if fs::canonicalize(entry.path()).ok().as_ref() == Some(&target) {
            return Some(entry.file_name().to_string_lossy().replace("\\x20", " "));
        }
    }
    None
}

/// `(rozmiar, wolne)` w bajtach.
pub fn statvfs(path: &Path) -> Option<(u64, u64)> {
    let st = rustix::fs::statvfs(path).ok()?;
    let unit = if st.f_frsize != 0 {
        st.f_frsize
    } else {
        st.f_bsize
    };
    Some((st.f_blocks * unit, st.f_bavail * unit))
}

/// Montuje urządzenie przez UDisks2 (polkit pyta o hasło, gdy trzeba).
pub fn mount_device(dev: &str) {
    spawn_udisks(&["mount", "-b", dev]);
}

/// Odmontowuje urządzenie przez UDisks2.
pub fn unmount_device(dev: &str) {
    spawn_udisks(&["unmount", "-b", dev]);
}

fn spawn_udisks(args: &[&str]) {
    if let Err(e) = Command::new("udisksctl").args(args).spawn() {
        eprintln!("luna-files: udisksctl {:?}: {e}", args);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mountinfo_line_is_parsed() {
        let line =
            "36 35 98:0 /mnt1 /run/media/patryk/PEN\\040DRIVE rw,noatime - vfat /dev/sdb1 rw";
        let m = parse_mountinfo(line).unwrap();
        assert_eq!(m.mount, PathBuf::from("/run/media/patryk/PEN DRIVE"));
        assert_eq!(m.fs, "vfat");
        assert_eq!(m.dev, "/dev/sdb1");
    }

    #[test]
    fn system_mounts_are_hidden() {
        assert!(user_visible_mount(Path::new("/run/media/x/USB"), "vfat"));
        assert!(!user_visible_mount(Path::new("/"), "ext4"));
        assert!(!user_visible_mount(Path::new("/home"), "ext4"));
        assert!(!user_visible_mount(Path::new("/mnt/x"), "proc"));
    }

    #[test]
    fn octal_sequences_are_unescaped() {
        assert_eq!(unescape("a\\040b\\134c"), "a b\\c");
    }

    #[test]
    fn lsblk_line_parses_quoted_values() {
        let line = "NAME=\"sdb1\" FSTYPE=\"vfat\" LABEL=\"PEN DRIVE\" SIZE=\"15600000000\" MOUNTPOINT=\"/run/media/x/PEN DRIVE\" RM=\"1\" TYPE=\"part\" TRAN=\"\" PKNAME=\"sdb\"";
        let row = parse_lsblk_line(line).expect("row");
        assert_eq!(row["NAME"], "sdb1");
        assert_eq!(row["LABEL"], "PEN DRIVE");
        assert_eq!(row["MOUNTPOINT"], "/run/media/x/PEN DRIVE");
        assert_eq!(row["RM"], "1");
        assert_eq!(row["PKNAME"], "sdb");
    }

    #[test]
    fn lsblk_parser_handles_missing_and_escaped_values() {
        let row = parse_lsblk_line("NAME=\"sda\" FSTYPE=\"\" LABEL=\"A\\\\B\"").expect("row");
        assert_eq!(row["NAME"], "sda");
        assert_eq!(row["FSTYPE"], "");
        assert_eq!(row["LABEL"], "A\\B");
    }
}
