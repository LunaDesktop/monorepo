//! Sprawdzanie wymagań przed instalacją.
//!
//! Każda kontrola ma poziom: `Ok` / `Info` / `Warning` / `Error`. Błędy blokują
//! przejście dalej; ostrzeżenia tylko informują. Kontrole są tanie i lokalne —
//! nie wymagają roota.

use std::path::Path;

use crate::plan::FileSystem;

/// Poziom kontroli.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Info,
    Warning,
    Error,
}

impl Level {
    pub fn label(self) -> String {
        match self {
            Self::Ok => libluna_i18n::tr("OK"),
            Self::Info => libluna_i18n::tr("Info"),
            Self::Warning => libluna_i18n::tr("Warning"),
            Self::Error => libluna_i18n::tr("Error"),
        }
    }
}

/// Jedna kontrola.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub title: String,
    pub detail: String,
    pub level: Level,
}

impl Check {
    fn new(title: impl Into<String>, detail: impl Into<String>, level: Level) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            level,
        }
    }
}

/// Kontekst kontroli.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub live: bool,
    pub network_connected: bool,
    pub backend_available: bool,
    pub installable_disks: usize,
    pub encryption: bool,
    pub filesystem: Option<FileSystem>,
    pub update_system: bool,
}

/// Uruchamia wszystkie kontrole.
pub fn run(context: &Context) -> Vec<Check> {
    vec![
        architecture(),
        live_image(context.live),
        firmware(),
        secure_boot(),
        memory(),
        power(),
        network(context.network_connected, context.update_system),
        backend(context.backend_available),
        disks(context.installable_disks),
        tools(context),
        network_manager(),
    ]
}

/// Czy któryś błąd blokuje instalację.
pub fn has_errors(checks: &[Check]) -> bool {
    checks.iter().any(|check| check.level == Level::Error)
}

fn architecture() -> Check {
    let arch = std::env::consts::ARCH;
    let supported = matches!(arch, "x86_64" | "aarch64");
    Check::new(
        libluna_i18n::tr("Architecture"),
        arch.to_owned(),
        if supported { Level::Ok } else { Level::Error },
    )
}

fn live_image(live: bool) -> Check {
    if live {
        Check::new(
            libluna_i18n::tr("Live image"),
            libluna_i18n::tr("Running from the Luna live image"),
            Level::Ok,
        )
    } else {
        Check::new(
            libluna_i18n::tr("Live image"),
            libluna_i18n::tr(
                "Not running from a live image — the installer will copy the running system",
            ),
            Level::Warning,
        )
    }
}

pub fn is_uefi() -> bool {
    Path::new("/sys/firmware/efi").exists()
}

fn firmware() -> Check {
    if is_uefi() {
        Check::new(
            libluna_i18n::tr("Firmware"),
            libluna_i18n::tr("UEFI"),
            Level::Ok,
        )
    } else {
        Check::new(
            libluna_i18n::tr("Firmware"),
            libluna_i18n::tr("Legacy BIOS (GRUB will use the BIOS target)"),
            Level::Info,
        )
    }
}

fn secure_boot() -> Check {
    match secure_boot_enabled() {
        Some(true) => Check::new(
            libluna_i18n::tr("Secure Boot"),
            libluna_i18n::tr("Enabled — Luna's boot loader is not signed; disable Secure Boot or enroll keys after installation"),
            Level::Warning,
        ),
        Some(false) => Check::new(
            libluna_i18n::tr("Secure Boot"),
            libluna_i18n::tr("Disabled"),
            Level::Ok,
        ),
        None => Check::new(
            libluna_i18n::tr("Secure Boot"),
            libluna_i18n::tr("Not available (BIOS or no EFI variables)"),
            Level::Info,
        ),
    }
}

/// Czy Secure Boot jest włączony (odczyt efivars albo `mokutil`).
pub fn secure_boot_enabled() -> Option<bool> {
    if let Ok(entries) = std::fs::read_dir("/sys/firmware/efi/efivars") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("SecureBoot-") {
                if let Ok(bytes) = std::fs::read(entry.path()) {
                    // 4 bajty atrybutów + wartość UINT8.
                    if let Some(value) = bytes.get(4) {
                        return Some(*value != 0);
                    }
                }
            }
        }
    }
    None
}

fn memory() -> Check {
    let bytes = memory_bytes().unwrap_or(0);
    let level = if bytes == 0 {
        Level::Info
    } else if bytes < 1024 * 1024 * 1024 {
        Level::Error
    } else if bytes < 2 * 1024 * 1024 * 1024 {
        Level::Warning
    } else {
        Level::Ok
    };
    Check::new(
        libluna_i18n::tr("Memory"),
        if bytes == 0 {
            libluna_i18n::tr("Unknown")
        } else {
            format!("{:.1} GiB", bytes as f64 / 1024.0 / 1024.0 / 1024.0)
        },
        level,
    )
}

/// Pamięć z `/proc/meminfo` (`MemTotal`).
pub fn memory_bytes_from(text: &str) -> Option<u64> {
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("MemTotal:") else {
            continue;
        };
        let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
        return Some(kb * 1024);
    }
    None
}

fn memory_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| memory_bytes_from(&text))
}

fn power() -> Check {
    match power_state() {
        PowerState::Ac => Check::new(
            libluna_i18n::tr("Power"),
            libluna_i18n::tr("Connected to power"),
            Level::Ok,
        ),
        PowerState::Battery(percent) if percent < 25 => Check::new(
            libluna_i18n::tr("Power"),
            libluna_i18n::tr_args(
                "Running on battery ({percent}%) — connect the charger before installing",
                &[("percent", &percent.to_string())],
            ),
            Level::Warning,
        ),
        PowerState::Battery(percent) => Check::new(
            libluna_i18n::tr("Power"),
            libluna_i18n::tr_args(
                "Running on battery ({percent}%)",
                &[("percent", &percent.to_string())],
            ),
            Level::Warning,
        ),
        PowerState::Unknown => Check::new(
            libluna_i18n::tr("Power"),
            libluna_i18n::tr("No battery detected"),
            Level::Info,
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    Ac,
    Battery(u8),
    Unknown,
}

/// Odczyt stanu zasilania z `/sys/class/power_supply`.
pub fn power_state() -> PowerState {
    power_state_in(Path::new("/sys/class/power_supply"))
}

fn power_state_in(root: &Path) -> PowerState {
    let Ok(entries) = std::fs::read_dir(root) else {
        return PowerState::Unknown;
    };
    let mut battery = None;
    let mut ac_online = false;
    for entry in entries.flatten() {
        let dir = entry.path();
        let kind = std::fs::read_to_string(dir.join("type"))
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match kind.as_str() {
            "mains" | "usb" | "usb-c" => {
                if read_trim(&dir.join("online")).as_deref() == Some("1") {
                    ac_online = true;
                }
            }
            "battery" => {
                if let Some(capacity) =
                    read_trim(&dir.join("capacity")).and_then(|value| value.parse::<u8>().ok())
                {
                    battery = Some(capacity);
                }
            }
            _ => {}
        }
    }
    if ac_online {
        PowerState::Ac
    } else if let Some(capacity) = battery {
        PowerState::Battery(capacity)
    } else {
        PowerState::Unknown
    }
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn network(connected: bool, update: bool) -> Check {
    if connected {
        Check::new(
            libluna_i18n::tr("Network"),
            libluna_i18n::tr("Connected to the internet"),
            Level::Ok,
        )
    } else if update {
        Check::new(
            libluna_i18n::tr("Network"),
            libluna_i18n::tr("Not connected — system updates require a network connection"),
            Level::Error,
        )
    } else {
        Check::new(
            libluna_i18n::tr("Network"),
            libluna_i18n::tr("Not connected — installation can continue offline"),
            Level::Warning,
        )
    }
}

fn backend(available: bool) -> Check {
    Check::new(
        libluna_i18n::tr("Installation backend"),
        if available {
            libluna_i18n::tr("luna-install-backend found")
        } else {
            libluna_i18n::tr("luna-install-backend not found")
        },
        if available { Level::Ok } else { Level::Error },
    )
}

fn disks(count: usize) -> Check {
    Check::new(
        libluna_i18n::tr("Disks"),
        if count > 0 {
            libluna_i18n::tr_args(
                "{count} installable disk(s) found",
                &[("count", &count.to_string())],
            )
        } else {
            libluna_i18n::tr("No installable disk found")
        },
        if count > 0 { Level::Ok } else { Level::Error },
    )
}

fn tools(context: &Context) -> Check {
    let mut missing = Vec::new();
    for tool in ["rsync", "blkid"] {
        if !in_path(tool) {
            missing.push(tool);
        }
    }
    if !in_path("sgdisk") && !in_path("parted") {
        missing.push("sgdisk/parted");
    }
    if context.encryption && !in_path("cryptsetup") {
        missing.push("cryptsetup");
    }
    if missing.is_empty() {
        Check::new(
            libluna_i18n::tr("Required tools"),
            libluna_i18n::tr("All required tools are available"),
            Level::Ok,
        )
    } else {
        Check::new(
            libluna_i18n::tr("Required tools"),
            libluna_i18n::tr_args("Missing: {tools}", &[("tools", &missing.join(", "))]),
            Level::Error,
        )
    }
}

fn network_manager() -> Check {
    if in_path("nmcli") {
        Check::new(
            libluna_i18n::tr("NetworkManager"),
            libluna_i18n::tr("nmcli is available"),
            Level::Ok,
        )
    } else {
        Check::new(
            libluna_i18n::tr("NetworkManager"),
            libluna_i18n::tr("nmcli is not available — use the live session's network settings"),
            Level::Warning,
        )
    }
}

pub fn in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .any(|candidate| std::fs::metadata(candidate).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo() {
        assert_eq!(
            memory_bytes_from("MemTotal:       16384000 kB\nMemFree: 100 kB\n"),
            Some(16_384_000 * 1024)
        );
    }

    #[test]
    fn returns_errors_for_zero_disks() {
        let checks = run(&Context {
            backend_available: true,
            installable_disks: 0,
            ..Context::default()
        });
        assert!(has_errors(&checks));
    }

    #[test]
    fn unknown_power_is_info() {
        assert_eq!(
            power_state_in(Path::new("/definitely/not/here")),
            PowerState::Unknown
        );
    }

    #[test]
    fn checks_have_unique_titles() {
        let checks = run(&Context {
            backend_available: true,
            installable_disks: 1,
            ..Context::default()
        });
        let mut titles: Vec<_> = checks.iter().map(|check| &check.title).collect();
        titles.sort();
        titles.dedup();
        assert_eq!(titles.len(), checks.len());
    }
}
