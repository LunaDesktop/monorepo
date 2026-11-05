//! Wybór dysku docelowego.
//!
//! Lista pochodzi z `/sys/block` i `/proc/mounts` — bez `lsblk`, `parted` czy
//! innych narzędzi w `PATH`. Każdy dysk dostaje **powód ochrony**, jeśli zawiera
//! zamontowany system, nośnik live albo jest za mały. Instalator nigdy nie
//! proponuje takiego dysku jako celu.

use std::path::Path;
use std::process::Command;

/// Minimalny rozmiar dysku, na którym ma sens instalacja (8 GiB).
pub const MIN_DISK_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Minimalne wolne miejsce dla trybu „zainstaluj obok” (20 GiB).
pub const MIN_FREE_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Partycja na dysku (do trybu ręcznego / obok).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub fstype: String,
    pub part_type: String,
    pub mount: Option<String>,
}

impl Partition {
    pub fn size_label(&self) -> String {
        format_size(self.size_bytes)
    }

    pub fn is_esp(&self) -> bool {
        self.part_type
            .eq_ignore_ascii_case("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
            || self.fstype.eq_ignore_ascii_case("vfat")
            || self.fstype.eq_ignore_ascii_case("fat32")
    }

    pub fn is_busy(&self) -> bool {
        self.mount.is_some()
    }

    pub fn label(&self) -> String {
        let mut parts = vec![self.path.clone(), self.size_label()];
        if !self.fstype.is_empty() {
            parts.push(self.fstype.clone());
        }
        if let Some(mount) = &self.mount {
            parts.push(format!("→ {mount}"));
        }
        parts.join(" · ")
    }
}

/// Punkt montowania istotny dla bezpieczeństwa instalacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub source: String,
    pub target: String,
}

/// Dysk widziany przez instalator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub model: String,
    pub removable: bool,
    /// Największy ciągły obszar wolny (bez partycji), w bajtach.
    pub free_bytes: u64,
    /// `None` = dysk można wybrać. `Some(powód)` = cel zablokowany.
    pub protection: Option<String>,
}

impl Disk {
    pub fn can_install(&self) -> bool {
        self.protection.is_none()
    }

    pub fn size_label(&self) -> String {
        format_size(self.size_bytes)
    }

    pub fn detail(&self) -> String {
        let mut parts = vec![self.size_label()];
        if !self.model.is_empty() {
            parts.push(self.model.clone());
        }
        if self.removable {
            parts.push(libluna_i18n::tr("removable"));
        }
        parts.join(" · ")
    }
}

/// Czytelny rozmiar (GB/MB w jednostkach dziesiętnych).
pub fn format_size(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    const MB: f64 = 1_000_000.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else {
        format!("{:.0} MB", bytes / MB)
    }
}

/// Odczytuje `/proc/mounts` (pierwsze dwie kolumny).
pub fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let source = fields.next()?;
            let target = fields.next()?;
            if !source.starts_with("/dev/") {
                return None;
            }
            Some(Mount {
                source: source.to_owned(),
                target: target.to_owned(),
            })
        })
        .collect()
}

fn current_mounts() -> Vec<Mount> {
    std::fs::read_to_string("/proc/mounts")
        .map(|text| parse_mounts(&text))
        .unwrap_or_default()
}

/// Lista dysków na tym systemie.
pub fn list() -> Vec<Disk> {
    list_from(Path::new("/sys/block"), &current_mounts())
}

/// Wariant testowalny: katalog `sys_block` i lista montowań.
pub fn list_from(sys_block: &Path, mounts: &[Mount]) -> Vec<Disk> {
    let Ok(entries) = std::fs::read_dir(sys_block) else {
        return Vec::new();
    };
    let mut disks: Vec<Disk> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_candidate(&name) {
                return None;
            }
            let dir = entry.path();
            let size_sectors = read_trim(&dir.join("size"))
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            if size_sectors == 0 {
                return None;
            }
            let model = read_trim(&dir.join("device/model"))
                .or_else(|| read_trim(&dir.join("device/vendor")))
                .unwrap_or_default();
            let removable = read_trim(&dir.join("removable")).as_deref() == Some("1");
            let mut disk = build_disk(&name, size_sectors * 512, &model, removable, mounts);
            disk.free_bytes = free_space_from_sysfs(sys_block, &name);
            Some(disk)
        })
        .collect();
    disks.sort_by(|a, b| a.path.cmp(&b.path));
    disks
}

/// Buduje wpis dysku bez dotykania systemu plików (testy / import).
pub fn build_disk(
    name: &str,
    size_bytes: u64,
    model: &str,
    removable: bool,
    mounts: &[Mount],
) -> Disk {
    let protection = protection_reason(name, size_bytes, mounts);
    Disk {
        name: name.to_owned(),
        path: format!("/dev/{name}"),
        size_bytes,
        model: model.trim().to_owned(),
        removable,
        free_bytes: 0,
        protection,
    }
}

/// Największy ciągły obszar wolny (w bajtach) z listy `(start, size)` w sektorach.
pub fn largest_free_bytes(total_sectors: u64, ranges: &[(u64, u64)]) -> u64 {
    const ALIGN: u64 = 2048;
    let usable_end = total_sectors.saturating_sub(ALIGN);
    let mut pos = ALIGN;
    let mut best = 0;
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    for (start, size) in sorted {
        if start > pos {
            best = best.max(start - pos);
        }
        pos = pos.max(start.saturating_add(size));
    }
    if usable_end > pos {
        best = best.max(usable_end - pos);
    }
    best.saturating_mul(512)
}

fn free_space_from_sysfs(sys_block: &Path, name: &str) -> u64 {
    let dir = sys_block.join(name);
    let total = read_trim(&dir.join("size"))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if total == 0 {
        return 0;
    }
    largest_free_bytes(total, &partition_ranges(&dir, name))
}

fn partition_ranges(dir: &Path, disk: &str) -> Vec<(u64, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_partition_name(&name, disk) {
                return None;
            }
            let start = read_trim(&entry.path().join("start"))?
                .parse::<u64>()
                .ok()?;
            let size = read_trim(&entry.path().join("size"))?.parse::<u64>().ok()?;
            Some((start, size))
        })
        .collect()
}

fn is_partition_name(name: &str, disk: &str) -> bool {
    let Some(tail) = name.strip_prefix(disk) else {
        return false;
    };
    tail.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == 'p')
}

/// Partycje danego dysku (najpierw `lsblk`, potem `/sys/block`).
pub fn partitions(disk: &str) -> Vec<Partition> {
    if let Ok(entries) = std::fs::read_dir(format!("/sys/block/{disk}")) {
        if !entries
            .flatten()
            .any(|entry| is_partition_name(&entry.file_name().to_string_lossy(), disk))
        {
            return Vec::new();
        }
    }
    if let Some(partitions) = partitions_lsblk(disk) {
        return partitions;
    }
    partitions_sysfs(disk)
}

fn partitions_lsblk(disk: &str) -> Option<Vec<Partition>> {
    let output = Command::new("lsblk")
        .args([
            "-P",
            "-b",
            "-o",
            "NAME,SIZE,FSTYPE,PARTTYPE,MOUNTPOINT",
            &format!("/dev/{disk}"),
        ])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut partitions = Vec::new();
    for line in text.lines() {
        let pairs = parse_lsblk_line(line);
        let Some(name) = pairs
            .iter()
            .find(|(key, _)| key == "NAME")
            .map(|(_, value)| value.clone())
        else {
            continue;
        };
        if name == disk || !is_partition_name(&name, disk) {
            continue;
        }
        let size = pairs
            .iter()
            .find(|(key, _)| key == "SIZE")
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .unwrap_or(0);
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        let mount = get("MOUNTPOINT");
        partitions.push(Partition {
            path: format!("/dev/{name}"),
            name,
            size_bytes: size,
            fstype: get("FSTYPE"),
            part_type: get("PARTTYPE"),
            mount: (!mount.is_empty()).then_some(mount),
        });
    }
    Some(partitions)
}

fn parse_lsblk_line(line: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let key_start = index;
        while index < bytes.len() && bytes[index] != b'=' {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let key = line[key_start..index].to_owned();
        index += 1;
        if index >= bytes.len() || bytes[index] != b'"' {
            continue;
        }
        index += 1;
        let value_start = index;
        while index < bytes.len() && bytes[index] != b'"' {
            index += 1;
        }
        let value = line[value_start..index]
            .replace("\\x20", " ")
            .replace("\\x5c", "\\");
        pairs.push((key, value));
        index += 1;
    }
    pairs
}

fn partitions_sysfs(disk: &str) -> Vec<Partition> {
    let dir = Path::new("/sys/block").join(disk);
    let mounts = current_mounts();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut partitions: Vec<Partition> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_partition_name(&name, disk) {
                return None;
            }
            let size = read_trim(&entry.path().join("size"))
                .and_then(|value| value.parse::<u64>().ok())?
                * 512;
            let path = format!("/dev/{name}");
            let mount = mounts
                .iter()
                .find(|mount| mount.source == path)
                .map(|mount| mount.target.clone());
            Some(Partition {
                name,
                path,
                size_bytes: size,
                fstype: String::new(),
                part_type: String::new(),
                mount,
            })
        })
        .collect();
    partitions.sort_by(|a, b| a.path.cmp(&b.path));
    partitions
}

fn protection_reason(name: &str, size_bytes: u64, mounts: &[Mount]) -> Option<String> {
    if size_bytes < MIN_DISK_BYTES {
        return Some(libluna_i18n::tr("Disk is smaller than 8 GB"));
    }
    let occupied: Vec<&str> = mounts
        .iter()
        .filter(|mount| disk_for_source(&mount.source, name).as_deref() == Some(name))
        .map(|mount| mount.target.as_str())
        .collect();

    for &target in &occupied {
        if is_live_mount(target) {
            return Some(libluna_i18n::tr("Contains the running live image"));
        }
    }
    for &target in &occupied {
        if target == "/" || target.starts_with("/boot") {
            return Some(libluna_i18n::tr("Contains the running system"));
        }
    }
    None
}

fn is_live_mount(target: &str) -> bool {
    [
        "/run/live",
        "/lib/live",
        "/cdrom",
        "/live",
        "/run/initramfs/live",
    ]
    .iter()
    .any(|prefix| target == *prefix || target.starts_with(&format!("{prefix}/")))
}

/// Czy źródło montowania (`/dev/nvme0n1p2`) należy do dysku `name`.
pub fn disk_for_source(source: &str, name: &str) -> Option<String> {
    let rest = source.strip_prefix("/dev/")?;
    if rest == name {
        return Some(name.to_owned());
    }
    let tail = rest.strip_prefix(name)?;
    let starts_partition = tail
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == 'p');
    starts_partition.then(|| name.to_owned())
}

fn is_candidate(name: &str) -> bool {
    const SKIP: &[&str] = &["loop", "ram", "zram", "sr", "fd", "dm-", "md", "nbd"];
    if SKIP.iter().any(|prefix| name.starts_with(prefix)) {
        return false;
    }
    ["sd", "hd", "vd", "xvd", "nvme", "mmcblk"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(source: &str, target: &str) -> Mount {
        Mount {
            source: source.to_owned(),
            target: target.to_owned(),
        }
    }

    #[test]
    fn parses_mounts_and_skips_pseudo_sources() {
        let mounts = parse_mounts(
            "/dev/sda1 / ext4 rw 0 0\nproc /proc proc rw 0 0\noverlay / overlay rw 0 0\n",
        );
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source, "/dev/sda1");
        assert_eq!(mounts[0].target, "/");
    }

    #[test]
    fn maps_partitions_to_whole_disks() {
        assert_eq!(disk_for_source("/dev/sda1", "sda").as_deref(), Some("sda"));
        assert_eq!(
            disk_for_source("/dev/nvme0n1p2", "nvme0n1").as_deref(),
            Some("nvme0n1")
        );
        assert_eq!(
            disk_for_source("/dev/mmcblk0p1", "mmcblk0").as_deref(),
            Some("mmcblk0")
        );
        assert!(disk_for_source("/dev/sdb1", "sda").is_none());
    }

    #[test]
    fn system_disk_is_protected() {
        let disk = build_disk(
            "nvme0n1",
            500 * 1_000_000_000,
            "Fast SSD",
            false,
            &[mount("/dev/nvme0n1p2", "/")],
        );
        assert!(!disk.can_install());
        assert!(disk.protection.unwrap().contains("system"));
    }

    #[test]
    fn live_medium_is_protected() {
        let disk = build_disk(
            "sdb",
            16 * 1_000_000_000,
            "USB",
            true,
            &[mount("/dev/sdb1", "/run/live/medium")],
        );
        assert!(!disk.can_install());
        assert!(disk.protection.unwrap().contains("live"));
    }

    #[test]
    fn empty_disk_can_be_selected() {
        let disk = build_disk(
            "sdc",
            64 * 1_000_000_000,
            "USB",
            true,
            &[mount("/dev/sdc1", "/media/user/USB")],
        );
        assert!(disk.can_install());
    }

    #[test]
    fn small_disk_is_rejected() {
        let disk = build_disk("sdd", 4 * 1024 * 1024 * 1024, "", false, &[]);
        assert!(!disk.can_install());
    }

    #[test]
    fn size_is_human_readable() {
        assert_eq!(format_size(500_000_000_000), "500.0 GB");
        assert_eq!(format_size(8_000_000_000), "8.0 GB");
        assert_eq!(format_size(512_000_000), "512 MB");
    }

    #[test]
    fn free_space_is_the_largest_gap() {
        let ranges = [(2048, 2048), (50_000, 10_000)];
        assert_eq!(largest_free_bytes(100_000, &ranges), (50_000 - 4096) * 512);
    }

    #[test]
    fn parses_lsblk_properties() {
        let pairs = parse_lsblk_line(
            "NAME=\"sda1\" SIZE=\"1073741824\" FSTYPE=\"vfat\" PARTTYPE=\"c12a7328-f81f-11d2-ba4b-00a0c93ec93b\" MOUNTPOINT=\"/boot/efi\"",
        );
        assert_eq!(pairs.len(), 5);
        assert_eq!(pairs[0], ("NAME".to_owned(), "sda1".to_owned()));
    }

    #[test]
    fn esp_is_detected() {
        let partition = Partition {
            name: "sda1".to_owned(),
            path: "/dev/sda1".to_owned(),
            size_bytes: 512_000_000,
            fstype: "vfat".to_owned(),
            part_type: String::new(),
            mount: None,
        };
        assert!(partition.is_esp());
    }
}
