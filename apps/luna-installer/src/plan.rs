//! Plan instalacji: region, klawiatura, partycjonowanie, szyfrowanie i konto.
//!
//! Plan jest czysty (poza wykrywaniem domyślnych wartości), więc można go
//! testować bez dysków i roota. Hasła nie trafiają do `argv`: backend dostaje
//! je osobnymi liniami na `stdin`.

use crate::disk::{self, Disk, MIN_FREE_BYTES};

/// Tryb partycjonowania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionMode {
    EraseDisk,
    FreeSpace,
    Manual,
}

impl PartitionMode {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::EraseDisk => "erase",
            Self::FreeSpace => "free",
            Self::Manual => "manual",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::EraseDisk => libluna_i18n::tr("Automatic partitioning — erase the disk"),
            Self::FreeSpace => libluna_i18n::tr("Install alongside — use free space"),
            Self::Manual => libluna_i18n::tr("Manual partitioning"),
        }
    }

    pub fn description(self) -> String {
        match self {
            Self::EraseDisk => libluna_i18n::tr(
                "Creates a new partition table and removes every partition on the disk.",
            ),
            Self::FreeSpace => libluna_i18n::tr(
                "Keeps existing systems and installs into unallocated space (at least 20 GiB).",
            ),
            Self::Manual => libluna_i18n::tr(
                "Choose the root partition yourself; existing partitions are not resized.",
            ),
        }
    }
}

/// System plików dla root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystem {
    Ext4,
    Btrfs,
    Xfs,
}

impl FileSystem {
    pub const ALL: [FileSystem; 3] = [Self::Ext4, Self::Btrfs, Self::Xfs];

    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Ext4 => "ext4",
            Self::Btrfs => "btrfs",
            Self::Xfs => "xfs",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ext4 => "ext4",
            Self::Btrfs => "Btrfs",
            Self::Xfs => "XFS",
        }
    }
}

/// Swap: brak albo plik wymiany.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapChoice {
    None,
    Gb2,
    Gb4,
    Gb8,
}

impl SwapChoice {
    pub const ALL: [SwapChoice; 4] = [Self::None, Self::Gb2, Self::Gb4, Self::Gb8];

    pub fn as_arg(self) -> &'static str {
        match self {
            Self::None => "0",
            Self::Gb2 => "2",
            Self::Gb4 => "4",
            Self::Gb8 => "8",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::None => libluna_i18n::tr("No swap file"),
            Self::Gb2 => libluna_i18n::tr("2 GiB swap file"),
            Self::Gb4 => libluna_i18n::tr("4 GiB swap file"),
            Self::Gb8 => libluna_i18n::tr("8 GiB swap file"),
        }
    }
}

/// Bootloader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bootloader {
    Grub,
    None,
}

impl Bootloader {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Grub => "grub",
            Self::None => "none",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Grub => libluna_i18n::tr("GRUB"),
            Self::None => libluna_i18n::tr("Do not install a boot loader"),
        }
    }
}

/// Sposób ustawienia hasła roota.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootMode {
    Locked,
    SamePassword,
    Custom,
}

impl RootMode {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::SamePassword => "same",
            Self::Custom => "custom",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Locked => libluna_i18n::tr("Lock the root account"),
            Self::SamePassword => libluna_i18n::tr("Use the user password for root"),
            Self::Custom => libluna_i18n::tr("Set a separate root password"),
        }
    }
}

/// Dane konta, regionu i klawiatury.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub full_name: String,
    pub username: String,
    pub hostname: String,
    pub password: String,
    pub locale: String,
    pub timezone: String,
    pub keyboard_layout: String,
    pub keyboard_variant: String,
    pub keyboard_model: String,
    pub autologin: bool,
    pub root_mode: RootMode,
    pub root_password: String,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            full_name: String::new(),
            username: default_username(),
            hostname: default_hostname(),
            password: String::new(),
            locale: default_locale(),
            timezone: detect_timezone(),
            keyboard_layout: "us".to_owned(),
            keyboard_variant: String::new(),
            keyboard_model: "pc105".to_owned(),
            autologin: false,
            root_mode: RootMode::Locked,
            root_password: String::new(),
        }
    }
}

/// Opcje partycjonowania i instalacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    pub mode: PartitionMode,
    pub filesystem: FileSystem,
    pub swap: SwapChoice,
    pub encrypt: bool,
    pub luks_passphrase: String,
    pub bootloader: Bootloader,
    pub update_system: bool,
    pub uefi: bool,
    pub root_partition: Option<String>,
    pub efi_partition: Option<String>,
    pub swap_partition: Option<String>,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            mode: PartitionMode::EraseDisk,
            filesystem: FileSystem::Ext4,
            swap: SwapChoice::Gb2,
            encrypt: false,
            luks_passphrase: String::new(),
            bootloader: Bootloader::Grub,
            update_system: false,
            uefi: crate::checks::is_uefi(),
            root_partition: None,
            efi_partition: None,
            swap_partition: None,
        }
    }
}

/// Kompletny plan przekazywany do backendu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub disk: Disk,
    pub account: Account,
    pub options: InstallOptions,
}

impl InstallPlan {
    pub fn new(disk: Disk, account: Account, options: InstallOptions) -> Self {
        Self {
            disk,
            account,
            options,
        }
    }

    /// Argumenty backendu; hasła **nie** są tu przekazywane.
    pub fn backend_args(&self) -> Vec<String> {
        let mut args = vec![
            "--mode".to_owned(),
            self.options.mode.as_arg().to_owned(),
            "--disk".to_owned(),
            self.disk.path.clone(),
            "--filesystem".to_owned(),
            self.options.filesystem.as_arg().to_owned(),
            "--swap".to_owned(),
            self.options.swap.as_arg().to_owned(),
            "--encrypt".to_owned(),
            if self.options.encrypt { "yes" } else { "no" }.to_owned(),
            "--bootloader".to_owned(),
            self.options.bootloader.as_arg().to_owned(),
            "--update".to_owned(),
            if self.options.update_system {
                "yes"
            } else {
                "no"
            }
            .to_owned(),
            "--firmware".to_owned(),
            if self.options.uefi { "uefi" } else { "bios" }.to_owned(),
            "--hostname".to_owned(),
            self.account.hostname.clone(),
            "--username".to_owned(),
            self.account.username.clone(),
            "--full-name".to_owned(),
            self.account.full_name.clone(),
            "--locale".to_owned(),
            self.account.locale.clone(),
            "--timezone".to_owned(),
            self.account.timezone.clone(),
            "--keyboard-layout".to_owned(),
            self.account.keyboard_layout.clone(),
            "--keyboard-variant".to_owned(),
            self.account.keyboard_variant.clone(),
            "--keyboard-model".to_owned(),
            self.account.keyboard_model.clone(),
            "--autologin".to_owned(),
            if self.account.autologin { "yes" } else { "no" }.to_owned(),
            "--root-mode".to_owned(),
            self.account.root_mode.as_arg().to_owned(),
        ];
        if let Some(partition) = &self.options.root_partition {
            args.push("--root-partition".to_owned());
            args.push(partition.clone());
        }
        if let Some(partition) = &self.options.efi_partition {
            args.push("--efi-partition".to_owned());
            args.push(partition.clone());
        }
        if let Some(partition) = &self.options.swap_partition {
            args.push("--swap-partition".to_owned());
            args.push(partition.clone());
        }
        args
    }

    /// Sekrety na `stdin` w kolejności: user, root (opcjonalnie), LUKS (opcjonalnie).
    pub fn secrets(&self) -> Vec<String> {
        let root = if self.account.root_mode == RootMode::Custom {
            self.account.root_password.clone()
        } else {
            String::new()
        };
        let luks = if self.options.encrypt {
            self.options.luks_passphrase.clone()
        } else {
            String::new()
        };
        vec![self.account.password.clone(), root, luks]
    }

    /// Pary etykieta/wartość do ekranu podsumowania.
    pub fn summary(&self) -> Vec<(String, String)> {
        let mode = match self.options.mode {
            PartitionMode::Manual => {
                let root = self
                    .options
                    .root_partition
                    .clone()
                    .unwrap_or_else(|| "—".to_owned());
                format!("{} → {root}", self.options.mode.label())
            }
            _ => self.options.mode.label(),
        };
        let encryption = if self.options.encrypt {
            libluna_i18n::tr("LUKS2 encryption enabled")
        } else {
            libluna_i18n::tr("No encryption")
        };
        let bootloader = if self.options.bootloader == Bootloader::None {
            libluna_i18n::tr("Do not install a boot loader")
        } else if self.options.uefi {
            format!("{} (UEFI)", self.options.bootloader.label())
        } else {
            format!("{} (BIOS)", self.options.bootloader.label())
        };
        let root_mode = match self.account.root_mode {
            RootMode::Locked => RootMode::Locked.label(),
            RootMode::SamePassword => RootMode::SamePassword.label(),
            RootMode::Custom => RootMode::Custom.label(),
        };
        let swap = if let Some(partition) = &self.options.swap_partition {
            format!("{} ({partition})", libluna_i18n::tr("Swap partition"))
        } else {
            self.options.swap.label()
        };
        vec![
            (
                libluna_i18n::tr("Disk"),
                format!("{} ({})", self.disk.path, self.disk.size_label()),
            ),
            (libluna_i18n::tr("Partitioning"), mode),
            (
                libluna_i18n::tr("Filesystem"),
                self.options.filesystem.label().to_owned(),
            ),
            (libluna_i18n::tr("Swap"), swap),
            (libluna_i18n::tr("Encryption"), encryption),
            (libluna_i18n::tr("Boot loader"), bootloader),
            (
                libluna_i18n::tr("System update"),
                if self.options.update_system {
                    libluna_i18n::tr("Update during installation")
                } else {
                    libluna_i18n::tr("Skip updates")
                },
            ),
            (libluna_i18n::tr("Language"), self.account.locale.clone()),
            (
                libluna_i18n::tr("Keyboard"),
                format!(
                    "{}{}, {}",
                    self.account.keyboard_layout,
                    if self.account.keyboard_variant.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", self.account.keyboard_variant)
                    },
                    self.account.keyboard_model
                ),
            ),
            (libluna_i18n::tr("Timezone"), self.account.timezone.clone()),
            (
                libluna_i18n::tr("Full name"),
                self.account.full_name.clone(),
            ),
            (libluna_i18n::tr("Username"), self.account.username.clone()),
            (libluna_i18n::tr("Hostname"), self.account.hostname.clone()),
            (
                libluna_i18n::tr("Automatic login"),
                if self.account.autologin {
                    libluna_i18n::tr("Enabled")
                } else {
                    libluna_i18n::tr("Disabled")
                },
            ),
            (libluna_i18n::tr("Root account"), root_mode),
        ]
    }
}

/// Pola formularza i pól wyszukiwania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    FullName,
    Username,
    Hostname,
    Password,
    ConfirmPassword,
    RootPassword,
    RootConfirm,
    LuksPassword,
    LuksConfirm,
    LayoutSearch,
    TimezoneSearch,
    WifiPassword,
    Disk,
    Confirmation,
}

/// Błąd walidacji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub field: Field,
    pub message: String,
}

impl Issue {
    fn new(field: Field, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

/// Waliduje cały plan.
pub fn validate(plan: &InstallPlan) -> Vec<Issue> {
    let mut issues = Vec::new();
    if !plan.disk.can_install() {
        issues.push(Issue::new(
            Field::Disk,
            plan.disk
                .protection
                .clone()
                .unwrap_or_else(|| libluna_i18n::tr("This disk cannot be used")),
        ));
    }
    issues.extend(validate_options(&plan.options));
    issues.extend(validate_account(&plan.account));
    issues
}

/// Waliduje opcje partycjonowania.
pub fn validate_options(options: &InstallOptions) -> Vec<Issue> {
    let mut issues = Vec::new();
    if options.mode == PartitionMode::Manual {
        if options.root_partition.as_deref().unwrap_or("").is_empty() {
            issues.push(Issue::new(
                Field::Disk,
                libluna_i18n::tr("Choose a root partition"),
            ));
        }
        if options.uefi && options.efi_partition.as_deref().unwrap_or("").is_empty() {
            issues.push(Issue::new(
                Field::Disk,
                libluna_i18n::tr("Choose the EFI system partition"),
            ));
        }
    }
    if options.mode == PartitionMode::FreeSpace && !crate::checks::in_path("parted") {
        issues.push(Issue::new(
            Field::Disk,
            libluna_i18n::tr("parted is required to install into free space"),
        ));
    }
    for partition in [
        options.root_partition.as_deref(),
        options.efi_partition.as_deref(),
        options.swap_partition.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !partition.starts_with("/dev/") || partition.contains("..") {
            issues.push(Issue::new(
                Field::Disk,
                libluna_i18n::tr("Invalid partition path"),
            ));
        }
    }
    if options.encrypt {
        if let Some(message) = luks_error(&options.luks_passphrase) {
            issues.push(Issue::new(Field::LuksPassword, message));
        }
        if !crate::checks::in_path("cryptsetup") {
            issues.push(Issue::new(
                Field::LuksPassword,
                libluna_i18n::tr("cryptsetup is not available on this live image"),
            ));
        }
    }
    match options.filesystem {
        FileSystem::Btrfs if !crate::checks::in_path("mkfs.btrfs") => issues.push(Issue::new(
            Field::Disk,
            libluna_i18n::tr("mkfs.btrfs is not available on this live image"),
        )),
        FileSystem::Xfs if !crate::checks::in_path("mkfs.xfs") => issues.push(Issue::new(
            Field::Disk,
            libluna_i18n::tr("mkfs.xfs is not available on this live image"),
        )),
        _ => {}
    }
    if options.bootloader == Bootloader::Grub && !crate::checks::in_path("grub-install") {
        issues.push(Issue::new(
            Field::Disk,
            libluna_i18n::tr("grub-install is not available on this live image"),
        ));
    }
    issues
}

/// Waliduje plan z uwzględnieniem wolnego miejsca na dysku.
pub fn validate_with_disk(plan: &InstallPlan) -> Vec<Issue> {
    let mut issues = validate(plan);
    if plan.options.mode == PartitionMode::FreeSpace && plan.disk.free_bytes < MIN_FREE_BYTES {
        issues.push(Issue::new(
            Field::Disk,
            libluna_i18n::tr_args(
                "Not enough unallocated space on {disk} (need at least 20 GiB)",
                &[("disk", &disk::format_size(plan.disk.free_bytes))],
            ),
        ));
    }
    if plan.account.root_mode == RootMode::Custom {
        if let Some(message) = password_error(&plan.account.root_password, &plan.account.username) {
            issues.push(Issue::new(Field::RootPassword, message));
        }
    }
    issues
}

/// Waliduje konto (bez opcji dysku).
pub fn validate_account(account: &Account) -> Vec<Issue> {
    let mut issues = Vec::new();
    if let Some(message) = full_name_error(&account.full_name) {
        issues.push(Issue::new(Field::FullName, message));
    }
    if let Some(message) = username_error(&account.username) {
        issues.push(Issue::new(Field::Username, message));
    }
    if let Some(message) = hostname_error(&account.hostname) {
        issues.push(Issue::new(Field::Hostname, message));
    }
    if let Some(message) = password_error(&account.password, &account.username) {
        issues.push(Issue::new(Field::Password, message));
    }
    if let Some(message) = locale_error(&account.locale) {
        issues.push(Issue::new(Field::Password, message));
    }
    if let Some(message) = timezone_error(&account.timezone) {
        issues.push(Issue::new(Field::Password, message));
    }
    if !crate::keyboard::valid_code(&account.keyboard_layout) {
        issues.push(Issue::new(
            Field::Password,
            libluna_i18n::tr("Invalid keyboard layout"),
        ));
    }
    if !account.keyboard_variant.is_empty()
        && !crate::keyboard::valid_code(&account.keyboard_variant)
    {
        issues.push(Issue::new(
            Field::Password,
            libluna_i18n::tr("Invalid keyboard variant"),
        ));
    }
    if !crate::keyboard::valid_code(&account.keyboard_model) {
        issues.push(Issue::new(
            Field::Password,
            libluna_i18n::tr("Invalid keyboard model"),
        ));
    }
    issues
}

pub fn full_name_error(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some(libluna_i18n::tr("Enter your name"));
    }
    if trimmed.chars().count() > 64 {
        return Some(libluna_i18n::tr("Name is too long"));
    }
    if trimmed.chars().any(char::is_control) {
        return Some(libluna_i18n::tr("Name contains invalid characters"));
    }
    None
}

pub fn username_error(value: &str) -> Option<String> {
    if value.len() < 3 {
        return Some(libluna_i18n::tr("Use at least 3 characters"));
    }
    if value.len() > 32 {
        return Some(libluna_i18n::tr("Use at most 32 characters"));
    }
    let mut chars = value.chars();
    let first = chars.next().unwrap_or('_');
    if !(first.is_ascii_lowercase() || first == '_') {
        return Some(libluna_i18n::tr("Start with a lowercase letter or _"));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Some(libluna_i18n::tr("Use lowercase letters, digits, - and _"));
    }
    const RESERVED: &[&str] = &[
        "root",
        "daemon",
        "bin",
        "sys",
        "sync",
        "games",
        "man",
        "lp",
        "mail",
        "news",
        "uucp",
        "proxy",
        "www-data",
        "backup",
        "list",
        "irc",
        "gnats",
        "nobody",
        "systemd-network",
        "systemd-resolve",
        "messagebus",
        "polkitd",
        "sshd",
    ];
    if RESERVED.contains(&value) {
        return Some(libluna_i18n::tr("This username is reserved"));
    }
    None
}

pub fn hostname_error(value: &str) -> Option<String> {
    if value.is_empty() {
        return Some(libluna_i18n::tr("Enter a computer name"));
    }
    if value.len() > 63 {
        return Some(libluna_i18n::tr("Use at most 63 characters"));
    }
    if value.starts_with('-') || value.ends_with('-') {
        return Some(libluna_i18n::tr("Cannot start or end with -"));
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Some(libluna_i18n::tr("Use letters, digits and -"));
    }
    if value.chars().all(|c| c.is_ascii_digit()) {
        return Some(libluna_i18n::tr("Computer name cannot be only digits"));
    }
    None
}

pub fn password_error(value: &str, username: &str) -> Option<String> {
    if value.chars().count() < 8 {
        return Some(libluna_i18n::tr("Use at least 8 characters"));
    }
    if value.chars().any(char::is_control) {
        return Some(libluna_i18n::tr("Password contains invalid characters"));
    }
    if !username.is_empty() && value.eq_ignore_ascii_case(username) {
        return Some(libluna_i18n::tr("Password cannot be the username"));
    }
    None
}

pub fn luks_error(value: &str) -> Option<String> {
    if value.chars().count() < 8 {
        return Some(libluna_i18n::tr("Use at least 8 characters"));
    }
    if value.chars().any(char::is_control) {
        return Some(libluna_i18n::tr("Passphrase contains invalid characters"));
    }
    None
}

pub fn locale_error(value: &str) -> Option<String> {
    let valid = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'));
    (!valid).then(|| libluna_i18n::tr("Invalid language code"))
}

pub fn timezone_error(value: &str) -> Option<String> {
    let valid = !value.is_empty()
        && !value.contains("..")
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '+'));
    (!valid).then(|| libluna_i18n::tr("Invalid timezone"))
}

/// Proponowana nazwa użytkownika z `$USER`/`$LOGNAME`.
pub fn default_username() -> String {
    let raw = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    let mut out: String = raw
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
        .take(32)
        .collect();
    if out.len() < 3 {
        out = "luna".to_owned();
    }
    if !out
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
    {
        out.insert(0, 'l');
    }
    out
}

/// Nazwa komputera z `/etc/hostname` lub „luna”.
pub fn default_hostname() -> String {
    let raw = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let candidate: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if hostname_error(&candidate).is_none() {
        candidate
    } else {
        "luna".to_owned()
    }
}

/// Język systemu w formacie locale (np. `pl_PL.UTF-8`).
pub fn default_locale() -> String {
    let language = libluna_i18n::locale().language().to_owned();
    crate::translations::locale_for(&language, &libluna_core::region::system_locales())
}

/// Strefa czasowa: `/etc/timezone` albo dowiązanie `/etc/localtime`.
pub fn detect_timezone() -> String {
    if let Ok(value) = std::fs::read_to_string("/etc/timezone") {
        let value = value.trim();
        if timezone_error(value).is_none() {
            return value.to_owned();
        }
    }
    if let Ok(link) = std::fs::read_link("/etc/localtime") {
        let text = link.to_string_lossy();
        if let Some((_, zone)) = text.split_once("zoneinfo/") {
            if timezone_error(zone).is_none() {
                return zone.to_owned();
            }
        }
    }
    "UTC".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(username: &str, password: &str) -> Account {
        Account {
            full_name: "Luna Tester".to_owned(),
            username: username.to_owned(),
            hostname: "luna-pc".to_owned(),
            password: password.to_owned(),
            locale: "pl_PL.UTF-8".to_owned(),
            timezone: "Europe/Warsaw".to_owned(),
            keyboard_layout: "pl".to_owned(),
            keyboard_variant: String::new(),
            keyboard_model: "pc105".to_owned(),
            autologin: false,
            root_mode: RootMode::Locked,
            root_password: String::new(),
        }
    }

    fn disk() -> Disk {
        let mut disk = Disk {
            name: "sda".to_owned(),
            path: "/dev/sda".to_owned(),
            size_bytes: 64_000_000_000,
            model: String::new(),
            removable: false,
            free_bytes: 30 * 1024 * 1024 * 1024,
            protection: None,
        };
        disk.free_bytes = MIN_FREE_BYTES;
        disk
    }

    #[test]
    fn valid_account_passes() {
        assert!(validate_account(&account("luna", "bardzo-tajne")).is_empty());
    }

    #[test]
    fn username_rules_are_enforced() {
        assert!(username_error("ab").is_some());
        assert!(username_error("Luna").is_some());
        assert!(username_error("root").is_some());
        assert!(username_error("luna user").is_some());
        assert!(username_error("luna").is_none());
    }

    #[test]
    fn password_needs_eight_characters_and_no_controls() {
        assert!(password_error("short", "luna").is_some());
        assert!(password_error("luna", "luna").is_some());
        assert!(password_error("dobre-haslo", "luna").is_none());
        assert!(password_error("zle\nhaslo", "luna").is_some());
    }

    #[test]
    fn hostname_rules_are_enforced() {
        assert!(hostname_error("-luna").is_some());
        assert!(hostname_error("luna_pc").is_some());
        assert!(hostname_error("1234").is_some());
        assert!(hostname_error("luna-pc").is_none());
    }

    #[test]
    fn timezone_rejects_path_traversal() {
        assert!(timezone_error("../../etc").is_some());
        assert!(timezone_error("Europe/Warsaw").is_none());
    }

    #[test]
    fn backend_arguments_never_contain_secrets() {
        let plan = InstallPlan::new(
            disk(),
            account("luna", "tajne-haslo"),
            InstallOptions::default(),
        );
        let args = plan.backend_args().join(" ");
        assert!(args.contains("--disk /dev/sda"));
        assert!(!args.contains("tajne-haslo"));
        assert_eq!(plan.secrets()[0], "tajne-haslo");
    }

    #[test]
    fn options_are_passed_to_the_backend() {
        let mut options = InstallOptions::default();
        options.mode = PartitionMode::FreeSpace;
        options.filesystem = FileSystem::Btrfs;
        options.swap = SwapChoice::Gb4;
        options.encrypt = true;
        options.luks_passphrase = "bardzo-tajne".to_owned();
        options.bootloader = Bootloader::None;
        options.update_system = true;
        options.uefi = true;
        let plan = InstallPlan::new(disk(), account("luna", "tajne-haslo"), options);
        let args = plan.backend_args();
        assert!(args.windows(2).any(|pair| pair == ["--mode", "free"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["--filesystem", "btrfs"]));
        assert!(args.windows(2).any(|pair| pair == ["--swap", "4"]));
        assert!(args.windows(2).any(|pair| pair == ["--encrypt", "yes"]));
        assert!(args.windows(2).any(|pair| pair == ["--bootloader", "none"]));
        assert!(args.windows(2).any(|pair| pair == ["--update", "yes"]));
        assert!(args.windows(2).any(|pair| pair == ["--firmware", "uefi"]));
        assert!(!args.join(" ").contains("bardzo-tajne"));
        assert_eq!(plan.secrets()[2], "bardzo-tajne");
    }

    #[test]
    fn protected_disk_blocks_plan() {
        let mut disk = disk();
        disk.protection = Some("Contains the running system".to_owned());
        let plan = InstallPlan::new(
            disk,
            account("luna", "tajne-haslo"),
            InstallOptions::default(),
        );
        assert!(validate(&plan)
            .iter()
            .any(|issue| issue.field == Field::Disk));
    }

    #[test]
    fn free_space_mode_requires_enough_space() {
        let mut disk = disk();
        disk.free_bytes = 10 * 1024 * 1024 * 1024;
        let mut options = InstallOptions::default();
        options.mode = PartitionMode::FreeSpace;
        let plan = InstallPlan::new(disk, account("luna", "tajne-haslo"), options);
        assert!(validate_with_disk(&plan)
            .iter()
            .any(|issue| issue.field == Field::Disk));
    }

    #[test]
    fn manual_mode_requires_root_partition() {
        let mut options = InstallOptions::default();
        options.mode = PartitionMode::Manual;
        options.uefi = false;
        let issues = validate_options(&options);
        assert!(issues.iter().any(|issue| issue.field == Field::Disk));
    }

    #[test]
    fn custom_root_password_is_validated() {
        let mut account = account("luna", "tajne-haslo");
        account.root_mode = RootMode::Custom;
        account.root_password = "krotkie".to_owned();
        let plan = InstallPlan::new(disk(), account, InstallOptions::default());
        assert!(validate_with_disk(&plan)
            .iter()
            .any(|issue| issue.field == Field::RootPassword));
    }
}
