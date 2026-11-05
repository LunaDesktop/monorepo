//! Stan kreatora instalacji Luna.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use libluna_core::Point;
use libluna_widgets::{Entry, WindowControl};

use crate::backend::{self, Job, JobEvent};
use crate::checks::{self, Check};
use crate::disk::{self, Disk, Partition, MIN_FREE_BYTES};
use crate::keyboard::{self, Keyboard, XkbLists};
use crate::live::LiveEnvironment;
use crate::network::{self, NetworkState};
use crate::plan::{
    self, Account, Bootloader, Field, FileSystem, InstallOptions, InstallPlan, Issue,
    PartitionMode, RootMode, SwapChoice,
};
use crate::translations::{self, LanguageOption};

/// Krok kreatora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Language,
    Region,
    Network,
    Requirements,
    Disk,
    Account,
    Summary,
    Progress,
    Done,
}

impl Step {
    /// Kroki widoczne w nagłówku kreatora.
    pub const WIZARD: [Step; 8] = [
        Self::Welcome,
        Self::Language,
        Self::Region,
        Self::Network,
        Self::Requirements,
        Self::Disk,
        Self::Account,
        Self::Summary,
    ];

    pub fn title(self) -> String {
        match self {
            Self::Welcome => libluna_i18n::tr("Welcome"),
            Self::Language => libluna_i18n::tr("Language & keyboard"),
            Self::Region => libluna_i18n::tr("Region & timezone"),
            Self::Network => libluna_i18n::tr("Network"),
            Self::Requirements => libluna_i18n::tr("Requirements"),
            Self::Disk => libluna_i18n::tr("Disk & partitioning"),
            Self::Account => libluna_i18n::tr("Account"),
            Self::Summary => libluna_i18n::tr("Summary"),
            Self::Progress => libluna_i18n::tr("Installing Luna"),
            Self::Done => libluna_i18n::tr("Finished"),
        }
    }

    pub fn index(self) -> usize {
        Self::WIZARD
            .iter()
            .position(|step| *step == self)
            .unwrap_or(Self::WIZARD.len() - 1)
    }

    pub fn is_wizard(self) -> bool {
        Self::WIZARD.contains(&self)
    }

    pub fn previous(self) -> Self {
        let index = Self::WIZARD.iter().position(|step| *step == self);
        match index {
            Some(0) | None => self,
            Some(index) => Self::WIZARD[index - 1],
        }
    }
}

/// Element pod wskaźnikiem (hover/klik).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Next,
    Back,
    Close,
    Restart,
    PowerOff,
    SaveLog,
    Retry,
    RetryNetwork,
    RefreshChecks,
    OpenPicker(PickerKind),
    PickerRow(usize),
    PickerClose,
    PickerSearch,
    Wifi(usize),
    Language(usize),
    Layout(usize),
    Variant(usize),
    VariantNone,
    Model(usize),
    Timezone(usize),
    Disk(usize),
    Mode(PartitionMode),
    RootPartition(usize),
    EfiPartition(usize),
    SwapPartition(usize),
    SwapPartitionNone,
    Filesystem(FileSystem),
    Swap(SwapChoice),
    Bootloader(Bootloader),
    RootMode(RootMode),
    EncryptionToggle,
    AutologinToggle,
    UpdateToggle,
    ConnectWifi,
    ConfirmErase,
    ToggleLog,
    Entry(Field),
    WindowControl(WindowControl),
}

/// Typ wyboru w popupie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Language,
    Layout,
    Variant,
    Model,
    Timezone,
    Disk,
    Mode,
    RootPartition,
    EfiPartition,
    SwapPartition,
    Filesystem,
    SwapSize,
    Bootloader,
    RootMode,
}

impl PickerKind {
    pub fn title(self) -> String {
        match self {
            Self::Language => libluna_i18n::tr("Language of the installed system"),
            Self::Layout => libluna_i18n::tr("Keyboard layout"),
            Self::Variant => libluna_i18n::tr("Layout variant"),
            Self::Model => libluna_i18n::tr("Keyboard model"),
            Self::Timezone => libluna_i18n::tr("Timezone"),
            Self::Disk => libluna_i18n::tr("Disk"),
            Self::Mode => libluna_i18n::tr("How should Luna use this disk?"),
            Self::RootPartition => libluna_i18n::tr("Root partition"),
            Self::EfiPartition => libluna_i18n::tr("EFI system partition"),
            Self::SwapPartition => libluna_i18n::tr("Swap partition (optional)"),
            Self::Filesystem => libluna_i18n::tr("Filesystem"),
            Self::SwapSize => libluna_i18n::tr("Swap file"),
            Self::Bootloader => libluna_i18n::tr("Boot loader"),
            Self::RootMode => libluna_i18n::tr("Root account"),
        }
    }

    pub fn searchable(self) -> bool {
        matches!(
            self,
            Self::Language
                | Self::Layout
                | Self::Variant
                | Self::Model
                | Self::Timezone
                | Self::Disk
        )
    }
}

/// Pozycja w popupie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub label: String,
    pub value: usize,
    pub selected: bool,
    pub enabled: bool,
}

/// Otwarty popup wyboru.
pub struct Picker {
    pub kind: PickerKind,
    pub query: Entry,
    pub scroll: libluna_widgets::ScrollView,
    pub selected: usize,
}

/// Żądania okna.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pending {
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub begin_move: bool,
}

/// Pola tekstowe formularza.
pub struct Form {
    pub full_name: Entry,
    pub username: Entry,
    pub hostname: Entry,
    pub password: Entry,
    pub confirm: Entry,
    pub root_password: Entry,
    pub root_confirm: Entry,
    pub luks_password: Entry,
    pub luks_confirm: Entry,
    pub layout_search: Entry,
    pub timezone_search: Entry,
    pub wifi_password: Entry,
}

impl Form {
    fn new(account: &Account) -> Self {
        let mut full_name = Entry::new(libluna_i18n::tr("Your name"));
        full_name.set_text(account.full_name.clone());
        let mut username = Entry::new(libluna_i18n::tr("Username"));
        username.set_text(account.username.clone());
        let mut hostname = Entry::new(libluna_i18n::tr("Computer name"));
        hostname.set_text(account.hostname.clone());

        Self {
            full_name,
            username,
            hostname,
            password: Entry::new(libluna_i18n::tr("Password")).masked('•'),
            confirm: Entry::new(libluna_i18n::tr("Confirm password")).masked('•'),
            root_password: Entry::new(libluna_i18n::tr("Root password")).masked('•'),
            root_confirm: Entry::new(libluna_i18n::tr("Confirm root password")).masked('•'),
            luks_password: Entry::new(libluna_i18n::tr("Encryption passphrase")).masked('•'),
            luks_confirm: Entry::new(libluna_i18n::tr("Confirm passphrase")).masked('•'),
            layout_search: Entry::new(libluna_i18n::tr("Search keyboard layouts")),
            timezone_search: Entry::new(libluna_i18n::tr("Search timezones")),
            wifi_password: Entry::new(libluna_i18n::tr("Wi-Fi password")).masked('•'),
        }
    }

    pub fn entry(&self, field: Field) -> Option<&Entry> {
        Some(match field {
            Field::FullName => &self.full_name,
            Field::Username => &self.username,
            Field::Hostname => &self.hostname,
            Field::Password => &self.password,
            Field::ConfirmPassword => &self.confirm,
            Field::RootPassword => &self.root_password,
            Field::RootConfirm => &self.root_confirm,
            Field::LuksPassword => &self.luks_password,
            Field::LuksConfirm => &self.luks_confirm,
            Field::LayoutSearch => &self.layout_search,
            Field::TimezoneSearch => &self.timezone_search,
            Field::WifiPassword => &self.wifi_password,
            Field::Disk | Field::Confirmation => return None,
        })
    }

    pub fn entry_mut(&mut self, field: Field) -> Option<&mut Entry> {
        Some(match field {
            Field::FullName => &mut self.full_name,
            Field::Username => &mut self.username,
            Field::Hostname => &mut self.hostname,
            Field::Password => &mut self.password,
            Field::ConfirmPassword => &mut self.confirm,
            Field::RootPassword => &mut self.root_password,
            Field::RootConfirm => &mut self.root_confirm,
            Field::LuksPassword => &mut self.luks_password,
            Field::LuksConfirm => &mut self.luks_confirm,
            Field::LayoutSearch => &mut self.layout_search,
            Field::TimezoneSearch => &mut self.timezone_search,
            Field::WifiPassword => &mut self.wifi_password,
            Field::Disk | Field::Confirmation => return None,
        })
    }
}

/// Stan aplikacji.
pub struct App {
    pub live: LiveEnvironment,
    pub step: Step,
    pub languages: Vec<LanguageOption>,
    pub language_index: usize,
    pub system_locales: Vec<String>,
    pub locale: String,
    pub xkb: XkbLists,
    pub keyboard: Keyboard,
    pub timezones: Vec<String>,
    pub timezone: String,
    pub network: NetworkState,
    pub selected_wifi: Option<usize>,
    pub checks: Vec<Check>,
    pub disks: Vec<Disk>,
    pub selected_disk: Option<usize>,
    pub partitions: Vec<Partition>,
    pub options: InstallOptions,
    pub autologin: bool,
    pub root_mode: RootMode,
    pub form: Form,
    pub focus: Field,
    pub picker: Option<Picker>,
    pub issues: Vec<Issue>,
    pub confirm_erase: bool,
    pub scroll: libluna_widgets::ScrollView,
    pub pointer: Option<Point>,
    pub hover: Option<Hit>,
    pub pending: Pending,
    pub banner: Option<String>,
    pub job: Option<Job>,
    pub log: Vec<String>,
    pub progress: f64,
    pub finished: Option<Result<(), String>>,
    pub spinner: f64,
    pub backend: Option<PathBuf>,
    pub show_log: bool,
    pub started: Instant,
    pub timezone_auto: bool,
    pub layout_auto: bool,
    pub network_busy: bool,
    network_refresh_rx: Option<Receiver<NetworkState>>,
    network_connect_rx: Option<Receiver<Result<(), String>>>,
    pub wifi_message: Option<String>,
    pub log_saved: Option<PathBuf>,
}

impl App {
    /// Podstawowe pola konta (przed opcjami roota).
    pub const ACCOUNT_FIELDS: [Field; 5] = [
        Field::FullName,
        Field::Username,
        Field::Hostname,
        Field::Password,
        Field::ConfirmPassword,
    ];

    pub fn new() -> Self {
        let live = LiveEnvironment::detect();
        let system_locales = libluna_core::region::system_locales();
        let languages = translations::options();
        let language_index = current_language_index(&languages, &system_locales);
        let locale = languages
            .get(language_index)
            .map(|language| language.locale.clone())
            .unwrap_or_else(|| "en_US.UTF-8".to_owned());
        let xkb = keyboard::load();
        let language = languages
            .get(language_index)
            .map(|language| language.code.clone())
            .unwrap_or_else(|| "en".to_owned());
        let mut keyboard = Keyboard::default();
        keyboard.layout = keyboard::default_layout(&language).to_owned();
        let timezones = libluna_core::region::system_timezones();
        let timezone = detect_timezone(&timezones, &language);
        // Sieć sprawdzamy dopiero po wejściu w krok „Network” — `snapshot()`
        // może czekać na TCP, a start okna ma być natychmiastowy.
        let network = NetworkState::default();
        let disks = disk::list();
        let selected_disk = disks.iter().position(Disk::can_install);
        let partitions = selected_disk
            .and_then(|index| disks.get(index))
            .map(|disk| disk::partitions(&disk.name))
            .unwrap_or_default();
        let account = Account {
            locale: locale.clone(),
            timezone: timezone.clone(),
            keyboard_layout: keyboard.layout.clone(),
            keyboard_model: keyboard.model.clone(),
            ..Account::default()
        };
        let options = InstallOptions {
            uefi: checks::is_uefi(),
            ..InstallOptions::default()
        };
        let backend = backend::locate_backend();
        let mut app = Self {
            live,
            step: Step::Welcome,
            languages,
            language_index,
            system_locales,
            locale,
            xkb,
            keyboard,
            timezones,
            timezone,
            network,
            selected_wifi: None,
            checks: Vec::new(),
            disks,
            selected_disk,
            partitions,
            options,
            autologin: false,
            root_mode: RootMode::Locked,
            form: Form::new(&account),
            focus: Field::FullName,
            picker: None,
            issues: Vec::new(),
            confirm_erase: false,
            scroll: libluna_widgets::ScrollView::new(),
            pointer: None,
            hover: None,
            pending: Pending::default(),
            banner: if backend.is_none() {
                Some(libluna_i18n::tr(
                    "Installation backend not found (luna-install-backend). Run this on the live image.",
                ))
            } else {
                None
            },
            job: None,
            log: Vec::new(),
            progress: 0.0,
            finished: None,
            spinner: 0.0,
            backend,
            show_log: true,
            started: Instant::now(),
            timezone_auto: true,
            layout_auto: true,
            network_busy: false,
            network_refresh_rx: None,
            network_connect_rx: None,
            wifi_message: None,
            log_saved: None,
        };
        app.select_default_partitions();
        app.refresh_checks();
        app.apply_language_to_ui();
        app
    }

    /// Konstruktor dla testów i podglądów — bez sieci, dysków i backendu.
    #[allow(clippy::too_many_arguments)]
    pub fn with_data(
        live: LiveEnvironment,
        languages: Vec<LanguageOption>,
        xkb: XkbLists,
        timezones: Vec<String>,
        disks: Vec<Disk>,
        network: NetworkState,
        backend: Option<PathBuf>,
    ) -> Self {
        let language_index = current_language_index(&languages, &[]);
        let locale = languages
            .get(language_index)
            .map(|language| language.locale.clone())
            .unwrap_or_else(|| "en_US.UTF-8".to_owned());
        let language = languages
            .get(language_index)
            .map(|language| language.code.clone())
            .unwrap_or_else(|| "en".to_owned());
        let mut keyboard = Keyboard::default();
        keyboard.layout = keyboard::default_layout(&language).to_owned();
        let timezone = detect_timezone(&timezones, &language);
        let selected_disk = disks.iter().position(Disk::can_install);
        let partitions = selected_disk
            .and_then(|index| disks.get(index))
            .map(|disk| disk::partitions(&disk.name))
            .unwrap_or_default();
        let account = Account {
            locale: locale.clone(),
            timezone: timezone.clone(),
            keyboard_layout: keyboard.layout.clone(),
            keyboard_model: keyboard.model.clone(),
            ..Account::default()
        };
        let mut app = Self {
            live,
            step: Step::Welcome,
            languages,
            language_index,
            system_locales: Vec::new(),
            locale,
            xkb,
            keyboard,
            timezones,
            timezone,
            network,
            selected_wifi: None,
            checks: Vec::new(),
            disks,
            selected_disk,
            partitions,
            options: InstallOptions {
                uefi: false,
                ..InstallOptions::default()
            },
            autologin: false,
            root_mode: RootMode::Locked,
            form: Form::new(&account),
            focus: Field::FullName,
            picker: None,
            issues: Vec::new(),
            confirm_erase: false,
            scroll: libluna_widgets::ScrollView::new(),
            pointer: None,
            hover: None,
            pending: Pending::default(),
            banner: if backend.is_none() {
                Some(libluna_i18n::tr(
                    "Installation backend not found (luna-install-backend). Run this on the live image.",
                ))
            } else {
                None
            },
            job: None,
            log: Vec::new(),
            progress: 0.0,
            finished: None,
            spinner: 0.0,
            backend,
            show_log: true,
            started: Instant::now(),
            timezone_auto: true,
            layout_auto: true,
            network_busy: false,
            network_refresh_rx: None,
            network_connect_rx: None,
            wifi_message: None,
            log_saved: None,
        };
        app.select_default_partitions();
        app.refresh_checks();
        app
    }

    fn select_default_partitions(&mut self) {
        self.options.root_partition = self
            .partitions
            .iter()
            .filter(|partition| !partition.is_busy() && !partition.is_esp())
            .max_by_key(|partition| partition.size_bytes)
            .map(|partition| partition.path.clone());
        self.options.efi_partition = self
            .partitions
            .iter()
            .find(|partition| partition.is_esp() && !partition.is_busy())
            .map(|partition| partition.path.clone());
        self.options.swap_partition = None;
    }

    /// Wybór języka użytkownika.
    pub fn set_language(&mut self, index: usize) {
        if index >= self.languages.len() {
            return;
        }
        self.language_index = index;
        let language = self.languages[index].clone();
        self.locale = translations::locale_for(&language.code, &self.system_locales);
        if self.layout_auto {
            let layout = keyboard::default_layout(&language.code).to_owned();
            if self.xkb.layouts.iter().any(|option| option.code == layout) {
                self.keyboard.layout = layout;
            }
            self.keyboard.variant.clear();
        }
        if self.timezone_auto {
            let suggested = translations::suggest_timezone(&language.code);
            if self.timezone_exists(suggested) {
                self.timezone = suggested.to_owned();
            }
        }
        self.apply_language_to_ui();
        self.issues.clear();
    }

    fn apply_language_to_ui(&self) {
        let code = self
            .languages
            .get(self.language_index)
            .map(|language| language.code.clone())
            .unwrap_or_else(|| "en".to_owned());
        libluna_i18n::set_locale(libluna_i18n::Locale::new(code));
    }

    pub fn selected_language(&self) -> Option<&LanguageOption> {
        self.languages.get(self.language_index)
    }

    pub fn select_layout(&mut self, index: usize) {
        if let Some(option) = self.xkb.layouts.get(index) {
            self.keyboard.layout = option.code.clone();
            self.keyboard.variant.clear();
            self.layout_auto = false;
        }
    }

    pub fn select_variant(&mut self, index: usize) {
        let layout = self.keyboard.layout.clone();
        let variants: Vec<_> = self
            .xkb
            .variants
            .iter()
            .filter(|variant| variant.layout == layout)
            .collect();
        if let Some(variant) = variants.get(index) {
            self.keyboard.variant = variant.code.clone();
            self.layout_auto = false;
        }
    }

    pub fn clear_variant(&mut self) {
        self.keyboard.variant.clear();
        self.layout_auto = false;
    }

    pub fn select_model(&mut self, index: usize) {
        if let Some(option) = self.xkb.models.get(index) {
            self.keyboard.model = option.code.clone();
            self.layout_auto = false;
        }
    }

    pub fn select_timezone(&mut self, index: usize) {
        if let Some(timezone) = self.timezones.get(index) {
            self.timezone = timezone.clone();
            self.timezone_auto = false;
        }
    }

    fn timezone_exists(&self, timezone: &str) -> bool {
        self.timezones.iter().any(|item| item == timezone)
            || std::path::Path::new(&format!("/usr/share/zoneinfo/{timezone}")).exists()
    }

    /// Wybór dysku i odświeżenie jego partycji.
    pub fn select_disk(&mut self, index: usize) {
        if index < self.disks.len() {
            self.selected_disk = Some(index);
            self.confirm_erase = false;
            self.partitions = disk::partitions(&self.disks[index].name);
            self.select_default_partitions();
            self.refresh_checks();
        }
    }

    pub fn refresh_disks(&mut self) {
        let selected_path = self.selected_disk().map(|disk| disk.path.clone());
        self.disks = disk::list();
        self.selected_disk = selected_path
            .and_then(|path| self.disks.iter().position(|disk| disk.path == path))
            .or_else(|| self.disks.iter().position(Disk::can_install));
        self.partitions = self
            .selected_disk()
            .map(|disk| disk::partitions(&disk.name))
            .unwrap_or_default();
        self.select_default_partitions();
        self.refresh_checks();
    }

    pub fn selected_disk(&self) -> Option<&Disk> {
        self.selected_disk.and_then(|index| self.disks.get(index))
    }

    pub fn mode_available(&self, mode: PartitionMode) -> bool {
        match mode {
            PartitionMode::EraseDisk => self.selected_disk().is_some(),
            PartitionMode::FreeSpace => self
                .selected_disk()
                .is_some_and(|disk| disk.free_bytes >= MIN_FREE_BYTES),
            PartitionMode::Manual => {
                !self.partitions.is_empty()
                    && self
                        .partitions
                        .iter()
                        .any(|partition| !partition.is_busy() && !partition.is_esp())
                    && (!self.options.uefi
                        || self.partitions.iter().any(|partition| partition.is_esp()))
            }
        }
    }

    pub fn set_mode(&mut self, mode: PartitionMode) {
        if self.mode_available(mode) {
            self.options.mode = mode;
            self.confirm_erase = false;
        }
    }

    pub fn set_root_partition(&mut self, index: usize) {
        if let Some(partition) = self.partitions.get(index) {
            if partition.is_busy() || partition.is_esp() {
                return;
            }
            self.options.root_partition = Some(partition.path.clone());
            if self.options.swap_partition.as_deref() == Some(partition.path.as_str()) {
                self.options.swap_partition = None;
            }
            self.confirm_erase = false;
        }
    }

    pub fn set_efi_partition(&mut self, index: usize) {
        if let Some(partition) = self.partitions.get(index) {
            if partition.is_busy() || !partition.is_esp() {
                return;
            }
            self.options.efi_partition = Some(partition.path.clone());
        }
    }

    pub fn set_swap_partition(&mut self, index: usize) {
        if let Some(partition) = self.partitions.get(index) {
            if partition.is_busy()
                || partition.is_esp()
                || self.options.root_partition.as_deref() == Some(partition.path.as_str())
            {
                return;
            }
            self.options.swap_partition = Some(partition.path.clone());
            self.options.swap = SwapChoice::None;
        }
    }

    pub fn clear_swap_partition(&mut self) {
        self.options.swap_partition = None;
    }

    pub fn set_filesystem(&mut self, filesystem: FileSystem) {
        self.options.filesystem = filesystem;
        self.refresh_checks();
    }

    pub fn set_swap(&mut self, swap: SwapChoice) {
        self.options.swap = swap;
    }

    pub fn set_bootloader(&mut self, bootloader: Bootloader) {
        self.options.bootloader = bootloader;
        self.refresh_checks();
    }

    pub fn toggle_encryption(&mut self) {
        self.options.encrypt = !self.options.encrypt;
        if !self.options.encrypt && matches!(self.focus, Field::LuksPassword | Field::LuksConfirm) {
            self.set_focus(Field::Confirmation);
        }
        self.refresh_checks();
    }

    pub fn toggle_update(&mut self) {
        self.options.update_system = !self.options.update_system;
        self.refresh_checks();
    }

    pub fn set_root_mode(&mut self, mode: RootMode) {
        self.root_mode = mode;
        if mode != RootMode::Custom
            && matches!(self.focus, Field::RootPassword | Field::RootConfirm)
        {
            self.set_focus(Field::FullName);
        }
    }

    pub fn toggle_autologin(&mut self) {
        self.autologin = !self.autologin;
    }

    /// Odświeża sieć w tle — bez blokowania pętli UI.
    pub fn refresh_network(&mut self) {
        if self.network_refresh_rx.is_some() {
            return;
        }
        self.network_busy = true;
        self.wifi_message = Some(libluna_i18n::tr("Checking the network…"));
        let (tx, rx) = mpsc::channel();
        self.network_refresh_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(network::snapshot());
        });
    }

    pub fn connect_selected_wifi(&mut self) {
        if self.network_connect_rx.is_some() {
            return;
        }
        let Some(index) = self.selected_wifi else {
            self.wifi_message = Some(libluna_i18n::tr("Choose a network"));
            return;
        };
        let Some(wifi) = self.network.wifi.get(index).cloned() else {
            return;
        };
        let password = self.form.wifi_password.text().to_owned();
        if !wifi.open() && password.is_empty() {
            self.wifi_message = Some(libluna_i18n::tr("Enter the Wi-Fi password"));
            self.set_focus(Field::WifiPassword);
            return;
        }
        let ssid = wifi.ssid.clone();
        let (tx, rx) = mpsc::channel();
        self.network_busy = true;
        self.network_connect_rx = Some(rx);
        self.wifi_message = Some(libluna_i18n::tr_args(
            "Connecting to {ssid}…",
            &[("ssid", &ssid)],
        ));
        std::thread::spawn(move || {
            let result = network::connect_wifi(&ssid, &password);
            let _ = tx.send(result);
        });
    }

    fn poll_network(&mut self) {
        let mut refresh = None;
        if let Some(rx) = &self.network_refresh_rx {
            match rx.try_recv() {
                Ok(snapshot) => refresh = Some(Some(snapshot)),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => refresh = Some(None),
            }
        }
        if let Some(snapshot) = refresh {
            self.network_refresh_rx = None;
            if let Some(snapshot) = snapshot {
                self.network = snapshot;
                self.selected_wifi = None;
                if self.wifi_message.as_deref()
                    == Some(libluna_i18n::tr("Checking the network…").as_str())
                {
                    self.wifi_message = None;
                }
            }
            self.network_busy = self.network_connect_rx.is_some();
            self.refresh_checks();
        }

        let mut connected = None;
        if let Some(rx) = &self.network_connect_rx {
            match rx.try_recv() {
                Ok(result) => connected = Some(Some(result)),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => connected = Some(None),
            }
        }
        if let Some(result) = connected {
            self.network_connect_rx = None;
            self.network_busy = self.network_refresh_rx.is_some();
            match result {
                Some(Ok(())) => {
                    self.wifi_message = Some(libluna_i18n::tr("Connected"));
                    self.form.wifi_password.clear();
                    self.refresh_network();
                }
                Some(Err(error)) => {
                    self.wifi_message = Some(error);
                }
                None => {}
            }
        }
    }

    /// Ponowne przeliczenie kontroli wymagań.
    pub fn refresh_checks(&mut self) {
        self.checks = checks::run(&checks::Context {
            live: self.live.live,
            network_connected: self.network.connected && self.network.internet,
            backend_available: self.backend.is_some(),
            installable_disks: self.disks.iter().filter(|disk| disk.can_install()).count(),
            encryption: self.options.encrypt,
            filesystem: Some(self.options.filesystem),
            update_system: self.options.update_system,
        });
    }

    pub fn current_account(&self) -> Account {
        Account {
            full_name: self.form.full_name.text().trim().to_owned(),
            username: self.form.username.text().trim().to_owned(),
            hostname: self.form.hostname.text().trim().to_owned(),
            password: self.form.password.text().to_owned(),
            locale: self.locale.clone(),
            timezone: self.timezone.clone(),
            keyboard_layout: self.keyboard.layout.clone(),
            keyboard_variant: self.keyboard.variant.clone(),
            keyboard_model: self.keyboard.model.clone(),
            autologin: self.autologin,
            root_mode: self.root_mode,
            root_password: self.form.root_password.text().to_owned(),
        }
    }

    pub fn plan(&self) -> Option<InstallPlan> {
        let disk = self.selected_disk()?.clone();
        Some(InstallPlan::new(
            disk,
            self.current_account(),
            self.options.clone(),
        ))
    }

    pub fn issue_for(&self, field: Field) -> Option<&str> {
        self.issues
            .iter()
            .find(|issue| issue.field == field)
            .map(|issue| issue.message.as_str())
    }

    pub fn step_title(&self) -> String {
        self.step.title()
    }

    pub fn confirm_label(&self) -> String {
        match self.options.mode {
            PartitionMode::EraseDisk => libluna_i18n::tr("Erase the disk and install Luna"),
            PartitionMode::FreeSpace => libluna_i18n::tr("Use the free space and install Luna"),
            PartitionMode::Manual => {
                libluna_i18n::tr("Format the selected partitions and install Luna")
            }
        }
    }

    pub fn summary_note(&self) -> String {
        match self.options.mode {
            PartitionMode::EraseDisk => {
                libluna_i18n::tr("The selected disk is erased completely.")
            }
            PartitionMode::FreeSpace => libluna_i18n::tr(
                "Existing systems are kept; Luna is installed into unallocated space.",
            ),
            PartitionMode::Manual => libluna_i18n::tr(
                "Only the selected root/swap partitions are formatted; the EFI partition is reused.",
            ),
        }
    }

    /// Przechodzi dalej; przy braku poprawnych danych ustawia błędy i zostaje.
    pub fn next(&mut self) {
        self.banner = None;
        match self.step {
            Step::Welcome => self.enter(Step::Language),
            Step::Language => {
                if !keyboard::valid_code(&self.keyboard.layout)
                    || !keyboard::valid_code(&self.keyboard.model)
                    || (!self.keyboard.variant.is_empty()
                        && !keyboard::valid_code(&self.keyboard.variant))
                {
                    self.banner = Some(libluna_i18n::tr("Choose a valid keyboard layout"));
                    return;
                }
                self.enter(Step::Region);
            }
            Step::Region => {
                if plan::timezone_error(&self.timezone).is_some()
                    || !self.timezone_exists(&self.timezone)
                {
                    self.banner = Some(libluna_i18n::tr("Choose a valid timezone"));
                    return;
                }
                self.refresh_network();
                self.enter(Step::Network);
            }
            Step::Network => {
                self.refresh_checks();
                self.enter(Step::Requirements);
            }
            Step::Requirements => {
                self.refresh_checks();
                if checks::has_errors(&self.checks) {
                    self.banner = Some(libluna_i18n::tr("Fix the errors below before installing"));
                    return;
                }
                self.refresh_disks();
                self.enter(Step::Disk);
            }
            Step::Disk => {
                if self.selected_disk().is_none() {
                    self.banner = Some(libluna_i18n::tr("Choose a disk to install Luna on"));
                    return;
                }
                let mut issues = plan::validate_options(&self.options);
                if self.options.mode == PartitionMode::FreeSpace {
                    if let Some(disk) = self.selected_disk() {
                        if disk.free_bytes < MIN_FREE_BYTES {
                            issues.push(Issue {
                                field: Field::Disk,
                                message: libluna_i18n::tr_args(
                                    "Not enough unallocated space on the disk ({free} free, need 20 GiB)",
                                    &[("free", &disk::format_size(disk.free_bytes))],
                                ),
                            });
                        }
                    }
                }
                if self.options.encrypt
                    && self.form.luks_password.text() != self.form.luks_confirm.text()
                {
                    issues.push(Issue {
                        field: Field::LuksConfirm,
                        message: libluna_i18n::tr("Passphrases do not match"),
                    });
                }
                if !issues.is_empty() {
                    self.issues = issues;
                    if let Some(issue) = self.issues.first() {
                        self.focus = issue.field;
                        self.banner = Some(issue.message.clone());
                    }
                    self.sync_focus();
                    return;
                }
                self.issues.clear();
                self.enter(Step::Account);
            }
            Step::Account => {
                let account = self.current_account();
                let mut issues = plan::validate_account(&account);
                if self.form.password.text() != self.form.confirm.text() {
                    issues.push(Issue {
                        field: Field::ConfirmPassword,
                        message: libluna_i18n::tr("Passwords do not match"),
                    });
                }
                if self.root_mode == RootMode::Custom {
                    if let Some(message) =
                        plan::password_error(self.form.root_password.text(), &account.username)
                    {
                        issues.push(Issue {
                            field: Field::RootPassword,
                            message,
                        });
                    }
                    if self.form.root_password.text() != self.form.root_confirm.text() {
                        issues.push(Issue {
                            field: Field::RootPassword,
                            message: libluna_i18n::tr("Root passwords do not match"),
                        });
                    }
                }
                if !issues.is_empty() {
                    self.issues = issues;
                    if let Some(issue) = self.issues.first() {
                        self.focus = issue.field;
                        self.banner = Some(issue.message.clone());
                    }
                    self.sync_focus();
                    return;
                }
                self.issues.clear();
                self.enter(Step::Summary);
            }
            Step::Summary => {
                let Some(plan) = self.plan() else {
                    self.banner = Some(libluna_i18n::tr("Choose a disk to install Luna on"));
                    return;
                };
                if self.options.update_system && !(self.network.connected && self.network.internet)
                {
                    self.banner = Some(libluna_i18n::tr(
                        "System updates require an internet connection",
                    ));
                    return;
                }
                let issues = plan::validate_with_disk(&plan);
                if !issues.is_empty() {
                    self.issues = issues;
                    self.enter(Step::Account);
                    if let Some(issue) = self.issues.first() {
                        self.focus = issue.field;
                        self.banner = Some(issue.message.clone());
                    }
                    self.sync_focus();
                    return;
                }
                if !self.confirm_erase {
                    self.banner = Some(libluna_i18n::tr(
                        "Confirm that the selected disk may be erased",
                    ));
                    return;
                }
                self.begin_install(plan);
            }
            Step::Progress | Step::Done => {}
        }
    }

    fn enter(&mut self, step: Step) {
        self.step = step;
        self.banner = None;
        self.focus_first();
    }

    fn focus_first(&mut self) {
        if let Some(field) = self.focus_order().first().copied() {
            self.focus = field;
        }
        self.sync_focus();
    }

    /// Pola tekstowe widoczne w bieżącym kroku (kolejność Tab).
    pub fn focus_order(&self) -> Vec<Field> {
        match self.step {
            Step::Welcome
            | Step::Region
            | Step::Network
            | Step::Requirements
            | Step::Disk
            | Step::Summary
            | Step::Progress
            | Step::Done => Vec::new(),
            Step::Language => Vec::new(),
            Step::Account => {
                let mut fields = Self::ACCOUNT_FIELDS.to_vec();
                if self.root_mode == RootMode::Custom {
                    fields.push(Field::RootPassword);
                    fields.push(Field::RootConfirm);
                }
                fields
            }
        }
    }

    pub fn visible_fields(&self) -> Vec<Field> {
        let mut fields = self.focus_order();
        if self.step == Step::Network {
            if self
                .selected_wifi
                .and_then(|index| self.network.wifi.get(index))
                .is_some_and(|wifi| !wifi.open())
            {
                fields.push(Field::WifiPassword);
            }
        } else if self.step == Step::Disk && self.options.encrypt {
            fields.push(Field::LuksPassword);
            fields.push(Field::LuksConfirm);
        }
        fields
    }

    /// Ustawia fokus pola i synchronizuje flagi widgetów `Entry`.
    pub fn set_focus(&mut self, field: Field) {
        self.focus = field;
        self.sync_focus();
    }

    pub fn sync_focus(&mut self) {
        for field in all_fields() {
            if let Some(entry) = self.form.entry_mut(field) {
                entry.set_focused(self.focus == field);
            }
        }
    }

    /// Otwiera popup wyboru dla danego pola.
    pub fn open_picker(&mut self, kind: PickerKind) {
        let mut query = Entry::new(libluna_i18n::tr("Search…"));
        query.set_focused(true);
        let mut picker = Picker {
            kind,
            query,
            scroll: libluna_widgets::ScrollView::new(),
            selected: 0,
        };
        let items = picker_items_for(self, kind, "");
        if let Some(position) = items.iter().position(|item| item.selected) {
            picker.selected = position;
        }
        self.picker = Some(picker);
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
    }

    /// Pozycje aktualnie otwartego popupu (z filtrem tekstowym).
    pub fn picker_items(&self) -> Vec<PickerItem> {
        let Some(picker) = &self.picker else {
            return Vec::new();
        };
        let query = if picker.kind.searchable() {
            picker.query.text().to_ascii_lowercase()
        } else {
            String::new()
        };
        picker_items_for(self, picker.kind, &query)
    }

    /// Zastosowuje zaznaczoną pozycję (Enter).
    pub fn apply_picker_selected(&mut self) {
        let Some(picker) = &self.picker else {
            return;
        };
        let selected = picker.selected;
        let kind = picker.kind;
        let items = self.picker_items();
        if let Some(item) = items.get(selected) {
            if item.enabled {
                self.apply_picker_value(kind, item.value);
            }
        }
    }

    /// Zastosowuje pozycję klikniętą w popupie.
    pub fn apply_picker_row(&mut self, row: usize) {
        let Some(picker) = &self.picker else {
            return;
        };
        let kind = picker.kind;
        let items = self.picker_items();
        if let Some(item) = items.get(row) {
            if item.enabled {
                self.apply_picker_value(kind, item.value);
            }
        }
    }

    fn apply_picker_value(&mut self, kind: PickerKind, value: usize) {
        match kind {
            PickerKind::Language => self.set_language(value),
            PickerKind::Layout => self.select_layout(value),
            PickerKind::Variant => {
                if value == usize::MAX {
                    self.clear_variant();
                } else {
                    self.select_variant(value);
                }
            }
            PickerKind::Model => self.select_model(value),
            PickerKind::Timezone => self.select_timezone(value),
            PickerKind::Disk => self.select_disk(value),
            PickerKind::Mode => self.set_mode(
                [
                    PartitionMode::EraseDisk,
                    PartitionMode::FreeSpace,
                    PartitionMode::Manual,
                ][value.min(2)],
            ),
            PickerKind::RootPartition => self.set_root_partition(value),
            PickerKind::EfiPartition => self.set_efi_partition(value),
            PickerKind::SwapPartition => {
                if value == usize::MAX {
                    self.clear_swap_partition();
                } else {
                    self.set_swap_partition(value);
                }
            }
            PickerKind::Filesystem => self.set_filesystem(FileSystem::ALL[value.min(2)]),
            PickerKind::SwapSize => self.set_swap(SwapChoice::ALL[value.min(3)]),
            PickerKind::Bootloader => {
                self.set_bootloader([Bootloader::Grub, Bootloader::None][value.min(1)])
            }
            PickerKind::RootMode => {
                self.set_root_mode(
                    [RootMode::Locked, RootMode::SamePassword, RootMode::Custom][value.min(2)],
                );
            }
        }
        self.picker = None;
    }

    /// Przechodzi do następnego/poprzedniego pola formularza.
    pub fn focus_step(&mut self, direction: i32) {
        let fields = self.visible_fields();
        if fields.is_empty() {
            return;
        }
        let current = fields
            .iter()
            .position(|field| *field == self.focus)
            .unwrap_or(0);
        let next = if direction < 0 {
            (current + fields.len() - 1) % fields.len()
        } else {
            (current + 1) % fields.len()
        };
        self.set_focus(fields[next]);
    }

    /// Cofa o krok (w kreatorze); na kroku instalacji nic nie robi.
    pub fn back(&mut self) {
        self.banner = None;
        self.step = self.step.previous();
        self.issues.clear();
        self.focus_first();
    }

    pub fn cancel(&mut self) {
        if self.step == Step::Welcome || self.step == Step::Done {
            self.pending.close = true;
        } else {
            self.back();
        }
    }

    pub fn begin_install(&mut self, plan: InstallPlan) {
        match backend::start(&plan) {
            Ok(job) => {
                self.log.clear();
                self.log.push(libluna_i18n::tr_args(
                    "Installing Luna on {disk}…",
                    &[("disk", &plan.disk.path)],
                ));
                if self.options.update_system {
                    self.log.push(libluna_i18n::tr("System update requested"));
                }
                self.finished = None;
                self.progress = 0.0;
                self.job = Some(job);
                self.banner = None;
                self.step = Step::Progress;
            }
            Err(error) => {
                self.banner = Some(error);
            }
        }
    }

    /// Odbiera zdarzenia z backendu; wywoływane raz na klatkę.
    pub fn poll_job(&mut self) {
        self.poll_network();
        let Some(job) = &self.job else {
            return;
        };
        let events = job.drain();
        let mut done = None;
        for event in events {
            match event {
                JobEvent::Line(line) => {
                    if let Some(value) = line.strip_prefix("PROGRESS:") {
                        if let Ok(percent) = value.trim().parse::<f64>() {
                            self.progress = (percent / 100.0).clamp(0.0, 1.0);
                            continue;
                        }
                    }
                    if self.log.len() < 500 {
                        self.log.push(line);
                    } else if let Some(last) = self.log.last_mut() {
                        *last = line;
                    }
                }
                JobEvent::Finished(result) => done = Some(result),
            }
        }
        if let Some(result) = done {
            self.progress = 1.0;
            self.finished = Some(result);
            self.job = None;
            self.step = Step::Done;
        }
    }

    pub fn tick(&mut self, dt: f64) {
        self.spinner = (self.spinner + dt * 0.8).rem_euclid(1.0);
    }

    pub fn needs_frames(&self) -> bool {
        self.job.is_some() || self.network_busy
    }

    pub fn apply_pending(&mut self, frame: &mut libluna_platform::Frame<'_>) {
        if self.pending.close {
            frame.close();
        }
        if self.pending.minimize {
            frame.minimize();
        }
        if self.pending.maximize {
            frame.toggle_maximize();
        }
        if self.pending.begin_move {
            frame.begin_move();
        }
        self.pending = Pending::default();
    }

    /// Zapisuje dziennik instalacji (bez haseł) do katalogu danych użytkownika.
    pub fn save_log(&mut self) {
        let path = log_path();
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            writeln!(file, "Luna Installer log")?;
            writeln!(file, "date: {:?}", std::time::SystemTime::now())?;
            if let Some(disk) = self.selected_disk() {
                writeln!(file, "disk: {}", disk.path)?;
            }
            writeln!(file, "progress: {:.0}%", self.progress * 100.0)?;
            writeln!(file)?;
            for line in &self.log {
                writeln!(file, "{line}")?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.log_saved = Some(path.clone());
                self.banner = Some(libluna_i18n::tr_args(
                    "Installation log saved to {path}",
                    &[("path", &path.display().to_string())],
                ));
            }
            Err(error) => {
                self.banner = Some(libluna_i18n::tr_args(
                    "Could not save the log: {error}",
                    &[("error", &error.to_string())],
                ));
            }
        }
    }

    pub fn restart(&mut self) {
        self.system_action("reboot");
    }

    pub fn power_off(&mut self) {
        self.system_action("poweroff");
    }

    fn system_action(&mut self, action: &str) {
        let root = unsafe { libc::geteuid() } == 0;
        let mut command = if root {
            let mut command = Command::new("systemctl");
            command.arg(action);
            command
        } else {
            let mut command = Command::new("pkexec");
            command.args(["systemctl", action]);
            command
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Err(err) = command.spawn() {
            self.banner = Some(libluna_i18n::tr_args(
                "System action failed: {error}",
                &[("error", &err.to_string())],
            ));
        }
    }

    /// Po błędzie instalacji wraca do podsumowania, żeby można było spróbować ponownie.
    pub fn retry(&mut self) {
        if matches!(self.finished, Some(Err(_))) {
            self.finished = None;
            self.banner = None;
            self.confirm_erase = false;
            self.issues.clear();
            self.step = Step::Summary;
            self.progress = 0.0;
        }
    }

    pub fn close_requested(&mut self) {
        self.pending.close = true;
    }

    pub fn log_path(&self) -> PathBuf {
        log_path()
    }
}

fn picker_items_for(app: &App, kind: PickerKind, query: &str) -> Vec<PickerItem> {
    let mut items = Vec::new();
    let matches = |label: &str| query.is_empty() || label.to_ascii_lowercase().contains(query);
    let mut push = |label: String, value: usize, selected: bool, enabled: bool| {
        if matches(&label) {
            items.push(PickerItem {
                label,
                value,
                selected,
                enabled,
            });
        }
    };

    match kind {
        PickerKind::Language => {
            for (index, language) in app.languages.iter().enumerate() {
                push(language.label(), index, app.language_index == index, true);
            }
        }
        PickerKind::Layout => {
            for (index, option) in app.xkb.layouts.iter().enumerate() {
                push(
                    option.label(),
                    index,
                    app.keyboard.layout == option.code,
                    true,
                );
            }
        }
        PickerKind::Variant => {
            let variants: Vec<_> = app
                .xkb
                .variants
                .iter()
                .filter(|variant| variant.layout == app.keyboard.layout)
                .collect();
            push(
                libluna_i18n::tr("No variant"),
                usize::MAX,
                app.keyboard.variant.is_empty(),
                true,
            );
            for (index, variant) in variants.iter().enumerate() {
                push(
                    format!("{} — {}", variant.code, variant.name),
                    index,
                    app.keyboard.variant == variant.code,
                    true,
                );
            }
        }
        PickerKind::Model => {
            for (index, option) in app.xkb.models.iter().enumerate() {
                push(
                    option.label(),
                    index,
                    app.keyboard.model == option.code,
                    true,
                );
            }
        }
        PickerKind::Timezone => {
            for (index, timezone) in app.timezones.iter().enumerate() {
                push(timezone.clone(), index, app.timezone == *timezone, true);
            }
        }
        PickerKind::Disk => {
            for (index, disk) in app.disks.iter().enumerate() {
                let mut label = format!("{}  ·  {}", disk.path, disk.detail());
                if let Some(protection) = &disk.protection {
                    label.push_str(&format!("  ·  {protection}"));
                }
                push(
                    label,
                    index,
                    app.selected_disk == Some(index),
                    disk.can_install(),
                );
            }
        }
        PickerKind::Mode => {
            for (index, mode) in [
                PartitionMode::EraseDisk,
                PartitionMode::FreeSpace,
                PartitionMode::Manual,
            ]
            .into_iter()
            .enumerate()
            {
                push(
                    mode.label(),
                    index,
                    app.options.mode == mode,
                    app.mode_available(mode),
                );
            }
        }
        PickerKind::RootPartition => {
            for (index, partition) in app.partitions.iter().enumerate() {
                if partition.is_esp() {
                    continue;
                }
                push(
                    partition.label(),
                    index,
                    app.options.root_partition.as_deref() == Some(partition.path.as_str()),
                    !partition.is_busy(),
                );
            }
        }
        PickerKind::EfiPartition => {
            for (index, partition) in app.partitions.iter().enumerate() {
                if !partition.is_esp() {
                    continue;
                }
                push(
                    partition.label(),
                    index,
                    app.options.efi_partition.as_deref() == Some(partition.path.as_str()),
                    !partition.is_busy(),
                );
            }
        }
        PickerKind::SwapPartition => {
            push(
                libluna_i18n::tr("No swap partition"),
                usize::MAX,
                app.options.swap_partition.is_none(),
                true,
            );
            for (index, partition) in app.partitions.iter().enumerate() {
                if partition.is_esp()
                    || app.options.root_partition.as_deref() == Some(partition.path.as_str())
                {
                    continue;
                }
                push(
                    partition.label(),
                    index,
                    app.options.swap_partition.as_deref() == Some(partition.path.as_str()),
                    !partition.is_busy(),
                );
            }
        }
        PickerKind::Filesystem => {
            for (index, filesystem) in FileSystem::ALL.into_iter().enumerate() {
                push(
                    filesystem.label().to_owned(),
                    index,
                    app.options.filesystem == filesystem,
                    true,
                );
            }
        }
        PickerKind::SwapSize => {
            for (index, swap) in SwapChoice::ALL.into_iter().enumerate() {
                push(swap.label(), index, app.options.swap == swap, true);
            }
        }
        PickerKind::Bootloader => {
            for (index, bootloader) in [Bootloader::Grub, Bootloader::None].into_iter().enumerate()
            {
                push(
                    bootloader.label(),
                    index,
                    app.options.bootloader == bootloader,
                    true,
                );
            }
        }
        PickerKind::RootMode => {
            for (index, mode) in [RootMode::Locked, RootMode::SamePassword, RootMode::Custom]
                .into_iter()
                .enumerate()
            {
                push(mode.label(), index, app.root_mode == mode, true);
            }
        }
    }
    items
}

fn all_fields() -> [Field; 13] {
    [
        Field::FullName,
        Field::Username,
        Field::Hostname,
        Field::Password,
        Field::ConfirmPassword,
        Field::RootPassword,
        Field::RootConfirm,
        Field::LuksPassword,
        Field::LuksConfirm,
        Field::LayoutSearch,
        Field::TimezoneSearch,
        Field::WifiPassword,
        Field::Confirmation,
    ]
}

fn current_language_index(languages: &[LanguageOption], _system_locales: &[String]) -> usize {
    let current = libluna_i18n::locale().language().to_owned();
    languages
        .iter()
        .position(|language| language.code == current)
        .unwrap_or(0)
}

fn detect_timezone(timezones: &[String], language: &str) -> String {
    let current = plan::detect_timezone();
    if timezones.iter().any(|zone| zone == &current) {
        return current;
    }
    let suggested = translations::suggest_timezone(language);
    if timezones.iter().any(|zone| zone == suggested) {
        return suggested.to_owned();
    }
    timezones
        .first()
        .cloned()
        .unwrap_or_else(|| "UTC".to_owned())
}

fn log_path() -> PathBuf {
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(data).join("luna/installer/install.log");
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".local/share/luna/installer/install.log"))
        .unwrap_or_else(|| PathBuf::from("/tmp/luna-installer.log"))
}

/// Kontrolka okna pod punktem (używana przez UI).
pub fn window_control_at(
    header: libluna_core::Rect,
    theme: &libluna_core::Theme,
    point: Point,
) -> Option<WindowControl> {
    libluna_widgets::HeaderBar::new()
        .controls(libluna_widgets::ControlsSide::Right)
        .control_at(header, theme, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::XkbLists;

    fn disk(protected: bool, free: u64) -> Disk {
        Disk {
            name: "sda".to_owned(),
            path: "/dev/sda".to_owned(),
            size_bytes: 64_000_000_000,
            model: "Test".to_owned(),
            removable: false,
            free_bytes: free,
            protection: protected.then(|| "Contains the running system".to_owned()),
        }
    }

    fn language(code: &str, coverage: u8) -> LanguageOption {
        LanguageOption {
            code: code.to_owned(),
            name: translations::language_name(code),
            locale: format!("{code}_{code}.UTF-8"),
            coverage,
        }
    }

    fn app() -> App {
        let xkb = XkbLists {
            layouts: vec![
                keyboard::XkbOption {
                    code: "pl".to_owned(),
                    name: "Polish".to_owned(),
                },
                keyboard::XkbOption {
                    code: "us".to_owned(),
                    name: "English".to_owned(),
                },
            ],
            models: vec![keyboard::XkbOption {
                code: "pc105".to_owned(),
                name: "Generic".to_owned(),
            }],
            variants: vec![],
        };
        App::with_data(
            LiveEnvironment {
                live: true,
                medium: None,
                reason: String::new(),
            },
            vec![language("pl", 98), language("en", 100)],
            xkb,
            vec!["Europe/Warsaw".to_owned(), "UTC".to_owned()],
            vec![disk(false, MIN_FREE_BYTES)],
            NetworkState {
                connected: true,
                internet: true,
                ..NetworkState::default()
            },
            Some(PathBuf::from("/bin/true")),
        )
    }

    fn fill_account(app: &mut App) {
        app.form.full_name.set_text("Luna Tester");
        app.form.username.set_text("luna");
        app.form.hostname.set_text("luna-pc");
        app.form.password.set_text("dobre-haslo");
        app.form.confirm.set_text("dobre-haslo");
    }

    #[test]
    fn selects_first_installable_disk() {
        let app = app();
        assert_eq!(
            app.selected_disk().map(|disk| disk.name.as_str()),
            Some("sda")
        );
    }

    #[test]
    fn welcome_advances_to_language() {
        let mut app = app();
        app.next();
        assert_eq!(app.step, Step::Language);
    }

    #[test]
    fn language_switch_updates_locale_and_keyboard() {
        let mut app = app();
        app.set_language(0);
        assert!(app.locale.to_lowercase().contains("pl"));
        assert_eq!(app.keyboard.layout, "pl");
    }

    #[test]
    fn timezone_selection_disables_auto() {
        let mut app = app();
        app.select_timezone(1);
        assert_eq!(app.timezone, "UTC");
        assert!(!app.timezone_auto);
    }

    #[test]
    fn free_space_mode_requires_enough_space() {
        let mut app = app();
        app.disks[0].free_bytes = MIN_FREE_BYTES;
        assert!(app.mode_available(PartitionMode::FreeSpace));
        app.disks[0].free_bytes = 1024;
        assert!(!app.mode_available(PartitionMode::FreeSpace));
    }

    #[test]
    fn account_requires_matching_passwords() {
        let mut app = app();
        app.step = Step::Account;
        fill_account(&mut app);
        app.form.confirm.set_text("inne-haslo");
        app.next();
        assert_eq!(app.step, Step::Account);
    }

    #[test]
    fn valid_account_advances_to_summary() {
        let mut app = app();
        app.step = Step::Account;
        fill_account(&mut app);
        app.next();
        assert_eq!(app.step, Step::Summary);
    }

    #[test]
    fn summary_requires_confirmation() {
        let mut app = app();
        app.step = Step::Summary;
        fill_account(&mut app);
        app.next();
        assert_eq!(app.step, Step::Summary);
        assert!(app.banner.is_some());
    }

    #[test]
    fn plan_uses_form_values_and_options() {
        let mut app = app();
        fill_account(&mut app);
        app.options.filesystem = FileSystem::Btrfs;
        let plan = app.plan().unwrap();
        assert_eq!(plan.account.username, "luna");
        assert_eq!(plan.options.filesystem, FileSystem::Btrfs);
        assert_eq!(plan.disk.path, "/dev/sda");
    }

    #[test]
    fn back_steps_through_the_wizard() {
        let mut app = app();
        app.step = Step::Summary;
        app.back();
        assert_eq!(app.step, Step::Account);
        app.back();
        assert_eq!(app.step, Step::Disk);
    }
}
