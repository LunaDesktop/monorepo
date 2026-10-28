//! Stan aplikacji i pętla klatki `luna-settings`.
//!
//! Podział jest ten sam co w powłoce: dane i akcje ([`SettingsApp`]) są oddzielone
//! od rysowania ([`crate::ui`]). Dzięki temu strony da się testować bez okna, a te
//! same funkcje systemowe (`luna::system`) obsługują panel i ustawienia.

use std::error::Error;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use libluna_core::theme_file::StyledTheme;
use libluna_core::{
    Accent, GreeterSettings, IconSet, IconSettings, LockSettings, Rect, RegionSettings,
    ThemePreference, ThemeSettings, WorkspaceLayout,
};
use libluna_i18n::{set_locale, tr, tr_args, Locale};
use libluna_platform::{run as run_app, AppConfig, Frame};
use libluna_widgets::{
    Entry, EntryIcon, NavEntry, NavList, PasswordEntry, ScrollView, Toast, WindowControl,
};
use luna::notifications::{self, Placement};
use luna::system::{self, BatteryStatus};

use crate::pages::{self, Action, Page, PickerKind, SliderKind};
use crate::ui;

/// Zmiany, które trzeba zastosować do `Frame` po obsłudze zdarzeń.
#[derive(Debug, Default)]
pub struct Pending {
    pub preference: Option<ThemePreference>,
    pub accent: Option<Accent>,
    /// `Some(None)` = wróć do stylu domyślnego.
    pub style: Option<Option<String>>,
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub begin_move: bool,
}

/// Wyszukiwany wybór (język, locale, strefa).
pub struct Picker {
    pub kind: PickerKind,
    pub query: Entry,
    pub selected: usize,
    pub scroll: f64,
}

/// Modalny formularz hasła administratora (np. zapis ekranu logowania).
pub struct AdminPrompt {
    pub password: PasswordEntry,
    pub error: Option<String>,
    /// Wynik operacji z wątku tła; `None`, gdy nic nie trwa.
    pub pending: Option<mpsc::Receiver<Result<(), String>>>,
}

impl AdminPrompt {
    pub fn new() -> Self {
        let mut password = PasswordEntry::new(tr("Administrator password"));
        password.set_focused(true);
        Self {
            password,
            error: None,
            pending: None,
        }
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Picker {
    fn new(kind: PickerKind) -> Self {
        let mut query = Entry::new(tr("Search")).leading(EntryIcon::Search);
        query.set_focused(true);
        Self {
            kind,
            query,
            selected: 0,
            scroll: 0.0,
        }
    }
}

/// Pozycja wyszukiwanego wyboru.
#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub value: String,
}

/// Mały edytor tekstu (np. nazwa przestrzeni roboczej).
pub struct TextEditor {
    pub title: String,
    pub entry: Entry,
    pub workspace: usize,
}

/// Stan aplikacji.
pub struct SettingsApp {
    pub page: Page,
    pub nav: NavList,
    pub nav_pages: Vec<Option<Page>>,
    pub scroll: ScrollView,

    /// Stan systemu odpytywany w tle (ta sama implementacja co w pasku Luny).
    pub system: system::Shared,
    pub system_ready: bool,
    pub user: String,
    pub volume: Option<(u8, bool)>,
    pub audio_sink: Option<String>,
    pub brightness: Option<u8>,
    pub battery: Option<BatteryStatus>,
    pub power_profile: Option<String>,

    pub theme_pref: ThemePreference,
    pub accent: Accent,
    pub styles: Vec<StyledTheme>,
    pub style: Option<String>,
    pub language: String,
    pub languages: Vec<(String, String)>,

    pub icons: IconSettings,
    /// Zestawy ikon freedesktop wykryte w systemie.
    pub icon_sets: Vec<IconSet>,
    pub lock: LockSettings,
    pub region: RegionSettings,
    pub workspace_layout: WorkspaceLayout,
    pub notification_placement: Placement,
    /// Locale i strefy wykryte w systemie (dla wyszukiwanego wyboru).
    pub locales: Vec<String>,
    pub timezones: Vec<String>,

    pub picker: Option<Picker>,
    pub picker_hover: Option<usize>,
    pub editor: Option<TextEditor>,
    /// Formularz hasła do zmian wymagających uprawnień administratora.
    pub admin: Option<AdminPrompt>,
    /// Czy ekran logowania ma niezapisane zmiany (wybory są szkicem).
    pub greeter_dirty: bool,

    pub dnd: bool,
    pub power_saver: bool,

    /// Ustawienia ekranu logowania (`/etc/luna/greeter.conf`).
    pub greeter: GreeterSettings,
    /// Wykryte tapety PNG proponowane dla ekranu logowania.
    pub wallpapers: Vec<PathBuf>,

    pub status: Toast,
    pub status_until: Option<std::time::Instant>,
    pub pending: Pending,
    pub hover_control: Option<WindowControl>,
    /// Aktywny suwak: rodzaj i prostokąt toru (do przeciągania).
    pub drag: Option<(SliderKind, Rect)>,
    /// Tekst do systemowego schowka (odbierany przez pętlę klatki).
    pub clipboard_text: Option<String>,
    /// Prośba o odczyt schowka; wynik wróci jako `Event::Paste`.
    pub paste_requested: bool,
    /// Pole, do którego trafi wklejony tekst.
    pub paste_target: Option<TextPasteTarget>,
}

/// Pole tekstowe obsługujące systemowy schowek.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPasteTarget {
    PickerQuery,
    EditorEntry,
    AdminPassword,
}

impl SettingsApp {
    pub fn new(settings: &ThemeSettings) -> Self {
        let language = libluna_i18n::locale().language().to_owned();
        let mut languages: Vec<(String, String)> = libluna_i18n::languages()
            .into_iter()
            .map(|code| {
                let name = libluna_i18n::language_name(&code).unwrap_or_else(|| code.clone());
                (code, name)
            })
            .collect();
        languages.sort_by_cached_key(|(_, name)| name.to_lowercase());

        // Wybrany wcześniej zestaw ikon ma obowiązywać także aplikacje GTK/Qt/KDE;
        // ponawiamy zapis przy starcie, gdyby inny program nadpisał te pliki.
        let mut icons = IconSettings::load();
        libluna_core::icons::set_active_icon_theme(icons.theme);
        // Domyślnie używamy dołączonego pakietu Luna-Icons. Zapamiętujemy
        // poprzedni motyw GTK, żeby powrót do ikon wektorowych mógł go przywrócić.
        if icons.external.is_some() && icons.previous_gtk_theme.is_none() {
            icons.previous_gtk_theme = libluna_core::icons::gtk_icon_theme()
                .filter(|name| Some(name.as_str()) != icons.external.as_deref())
                .or_else(|| Some("Adwaita".to_owned()));
        }

        let mut app = Self {
            page: Page::default(),
            nav: NavList::new(Vec::new()),
            nav_pages: Vec::new(),
            scroll: ScrollView::new(),
            system: system::spawn_light(),
            system_ready: false,
            user: std::env::var("USER")
                .or_else(|_| std::env::var("LOGNAME"))
                .unwrap_or_else(|_| "?".to_owned()),
            volume: system::read_volume(),
            audio_sink: None,
            brightness: system::read_brightness(),
            battery: None,
            power_profile: None,
            theme_pref: settings.preference(),
            accent: settings.accent(),
            styles: libluna_core::theme_file::available(),
            style: settings.style().map(str::to_owned),
            language,
            languages,
            icons,
            icon_sets: libluna_core::icons::external_icon_sets(),
            lock: LockSettings::load(),
            region: RegionSettings::load(),
            workspace_layout: WorkspaceLayout::load(),
            notification_placement: notifications::load_placement(),
            locales: libluna_core::region::system_locales(),
            timezones: libluna_core::region::system_timezones(),
            picker: None,
            picker_hover: None,
            editor: None,
            admin: None,
            greeter_dirty: false,
            dnd: system::dnd(),
            power_saver: system::power_saver(),
            greeter: GreeterSettings::load(),
            wallpapers: pages::wallpapers(),
            status: Toast::default(),
            status_until: None,
            pending: Pending::default(),
            hover_control: None,
            drag: None,
            clipboard_text: None,
            paste_requested: false,
            paste_target: None,
        };
        app.rebuild_nav();
        app
    }

    /// Pokazuje krótki komunikat nad treścią.
    pub fn show_status(&mut self, message: impl Into<String>) {
        self.status = Toast::new(message);
        self.status.show_animated();
        self.status_until = Some(std::time::Instant::now() + Duration::from_secs(3));
    }

    /// Odbudowuje etykiety sidebaru (np. po zmianie języka), zachowując stronę.
    pub fn rebuild_nav(&mut self) {
        let (mut nav, pages) = nav_entries();
        let selected = pages
            .iter()
            .position(|p| *p == Some(self.page))
            .unwrap_or(0);
        nav.set_selected(Some(selected));
        self.nav = nav;
        self.nav_pages = pages;
    }

    /// Wybiera stronę i wraca na górę przewijania.
    pub fn go_to(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.scroll = ScrollView::new();
            let selected = self.nav_pages.iter().position(|p| *p == Some(page));
            self.nav.set_selected(selected);
        }
    }

    /// Odświeża dane z wątku systemowego i plików powłoki.
    pub fn sync_system(&mut self) {
        if let Ok(sys) = self.system.lock() {
            self.system_ready = true;
            if !sys.user.is_empty() {
                self.user = sys.user.clone();
            }
            if !matches!(self.drag.map(|(kind, _)| kind), Some(SliderKind::Volume))
                && sys.volume.is_some()
            {
                self.volume = sys.volume;
            }
            self.audio_sink = sys.audio_sink.clone();
            if !matches!(
                self.drag.map(|(kind, _)| kind),
                Some(SliderKind::Brightness)
            ) {
                self.brightness = sys.brightness;
            }
            self.battery = sys.battery;
            self.power_profile = sys.power_profile.clone();
        }
        self.dnd = system::dnd();
        self.power_saver = system::power_saver();
    }

    /// Otwiera wyszukiwany wybór dla języka, locale albo strefy.
    pub fn open_picker(&mut self, kind: PickerKind) {
        let mut picker = Picker::new(kind);
        picker.selected = self.picker_current_index(kind, &picker);
        self.picker = Some(picker);
        self.picker_hover = None;
    }

    /// Wszystkie pozycje wybranego rodzaju (przed filtrowaniem).
    pub fn picker_items(&self, kind: PickerKind) -> Vec<PickerItem> {
        match kind {
            PickerKind::Language => self
                .languages
                .iter()
                .map(|(code, name)| PickerItem {
                    label: format!("{name} ({code})"),
                    value: code.clone(),
                })
                .collect(),
            PickerKind::Locale => {
                let system = libluna_core::region::current_locale();
                let mut items = vec![PickerItem {
                    label: format!("{}: {system}", tr("System")),
                    value: String::new(),
                }];
                items.extend(self.locales.iter().map(|locale| PickerItem {
                    label: locale.clone(),
                    value: locale.clone(),
                }));
                items
            }
            PickerKind::Timezone => {
                let system = libluna_core::region::current_timezone();
                let mut items = vec![PickerItem {
                    label: format!("{}: {system}", tr("System")),
                    value: String::new(),
                }];
                items.extend(self.timezones.iter().map(|zone| PickerItem {
                    label: zone.clone(),
                    value: zone.clone(),
                }));
                items
            }
            PickerKind::IconSet => {
                let mut items = vec![PickerItem {
                    label: tr("Vector icons (Luna)"),
                    value: String::new(),
                }];
                items.extend(self.icon_sets.iter().map(|set| {
                    let display = libluna_core::icons::icon_set_display_name(set);
                    let label = if display == set.name {
                        set.name.clone()
                    } else {
                        format!("{} ({})", display, set.name)
                    };
                    PickerItem {
                        label,
                        value: set.name.clone(),
                    }
                }));
                items
            }
        }
    }

    /// Pozycje przefiltrowane frazą z pola wyszukiwania.
    pub fn picker_filtered(&self) -> Vec<PickerItem> {
        let Some(picker) = &self.picker else {
            return Vec::new();
        };
        let query = picker.query.text().trim().to_lowercase();
        self.picker_items(picker.kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn picker_current_index(&self, kind: PickerKind, picker: &Picker) -> usize {
        let query = picker.query.text().to_lowercase();
        let current = match kind {
            PickerKind::Language => self.language.clone(),
            PickerKind::Locale => self.region.locale.clone().unwrap_or_default(),
            PickerKind::Timezone => self.region.timezone.clone().unwrap_or_default(),
            PickerKind::IconSet => self.icons.external.clone().unwrap_or_default(),
        };
        self.picker_items(kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .position(|item| item.value == current)
            .unwrap_or(0)
    }

    /// Zatwierdza aktualną pozycję pickera.
    pub fn apply_picker(&mut self) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let items = self.picker_filtered_for(&picker);
        let Some(item) = items.get(picker.selected.clamp(0, items.len().saturating_sub(1))) else {
            return;
        };
        match picker.kind {
            PickerKind::Language => {
                if let Some(index) = self
                    .languages
                    .iter()
                    .position(|(code, _)| *code == item.value)
                {
                    self.on_action(Action::SetLanguage(index));
                }
            }
            PickerKind::Locale => {
                self.region.locale = (!item.value.is_empty()).then(|| item.value.clone());
                self.save_region();
            }
            PickerKind::Timezone => {
                self.region.timezone = (!item.value.is_empty()).then(|| item.value.clone());
                self.save_region();
            }
            PickerKind::IconSet => {
                self.on_action(Action::SetExternalIconSet(
                    (!item.value.is_empty()).then(|| item.value.clone()),
                ));
            }
        }
    }

    fn picker_filtered_for(&self, picker: &Picker) -> Vec<PickerItem> {
        let query = picker.query.text().trim().to_lowercase();
        self.picker_items(picker.kind)
            .into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.label.to_lowercase().contains(&query)
                    || item.value.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Otwiera edytor nazwy przestrzeni.
    pub fn open_workspace_editor(&mut self, index: usize) {
        if index >= self.workspace_layout.count() {
            return;
        }
        let mut entry = Entry::new(tr("Workspace name"));
        entry.set_text(self.workspace_layout.name(index));
        entry.set_focused(true);
        self.editor = Some(TextEditor {
            title: tr_args("Workspace {n}", &[("n", &(index + 1).to_string())]),
            entry,
            workspace: index,
        });
    }

    /// Zapisuje nazwę przestrzeni i od razu wysyła ją do Selene.
    pub fn save_workspace_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let value = editor.entry.text().trim().to_owned();
        self.workspace_layout.set_name(
            editor.workspace,
            (!value.is_empty()).then_some(value.as_str()),
        );
        if let Err(err) = self.workspace_layout.save() {
            self.show_status(format!("{}: {err}", tr("Could not save workspaces")));
            return;
        }
        let name = one_line(&self.workspace_layout.name(editor.workspace));
        control_send(&format!("workspace-name {} {name}", editor.workspace));
        self.show_status(tr("Workspace renamed"));
    }

    fn save_region(&mut self) {
        match self.region.save() {
            Ok(()) => self.show_status(tr("Region settings saved")),
            Err(err) => self.show_status(format!("{}: {err}", tr("Could not save region"))),
        }
    }

    /// Stosuje odłożone zmiany do okna (motyw, kontrolki okna).
    pub fn apply_pending(&mut self, frame: &mut Frame) {
        if let Some(preference) = self.pending.preference.take() {
            frame.set_preference(preference);
        }
        if let Some(accent) = self.pending.accent.take() {
            frame.set_accent(accent);
        }
        if let Some(style) = self.pending.style.take() {
            frame.set_style(style);
        }
        if self.pending.close {
            frame.close();
            self.pending.close = false;
        }
        if self.pending.minimize {
            frame.minimize();
            self.pending.minimize = false;
        }
        if self.pending.maximize {
            frame.toggle_maximize();
            self.pending.maximize = false;
        }
        if self.pending.begin_move {
            frame.begin_move();
            self.pending.begin_move = false;
        }
    }

    /// Wykonuje akcję wybranej kontrolki.
    /// Ponawia globalny motyw ikon dla GTK/Qt/KDE po starcie aplikacji.
    ///
    /// Celowo nie robimy tego w [`SettingsApp::new`] — konstruktor jest używany
    /// przez testy i podglądy, a nie powinien pisać do `~/.config` użytkownika.
    pub fn apply_icon_theme_globally(&self) {
        let Some(name) = &self.icons.external else {
            return;
        };
        if libluna_core::icons::find_icon_set(name).is_some() {
            libluna_core::icons::apply_icon_theme_globally(name);
        }
    }

    pub fn on_action(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::SetPreference(preference) => {
                self.theme_pref = preference;
                self.pending.preference = Some(preference);
            }
            Action::SetAccent(accent) => {
                self.accent = accent;
                self.pending.accent = Some(accent);
            }
            Action::SetStyle(index) => {
                let style = if index == 0 {
                    None
                } else {
                    self.styles.get(index - 1).map(|s| s.id.clone())
                };
                self.style = style.clone();
                self.pending.style = Some(style);
            }
            Action::SetLanguage(index) => {
                let Some((code, _)) = self.languages.get(index).cloned() else {
                    return;
                };
                self.language = code.clone();
                let locale = Locale::new(&code);
                set_locale(locale.clone());
                libluna_i18n::load_catalogs_from_dirs();
                let _ = libluna_i18n::save_preferred_locale(&locale);
                self.rebuild_nav();
                self.show_status(tr("Language changed — all Luna apps switch automatically"));
            }
            Action::OpenPicker(kind) => self.open_picker(kind),
            Action::SetIconTheme(theme) => {
                if self.icons.theme == theme {
                    return;
                }
                self.icons.theme = theme;
                libluna_core::icons::set_active_icon_theme(theme);
                match self.icons.save() {
                    Ok(()) => self.show_status(tr("Icon style updated")),
                    Err(err) => {
                        self.show_status(format!("{}: {err}", tr("Could not save icon style")))
                    }
                }
            }
            Action::SetExternalIconSet(name) => {
                if self.icons.external == name {
                    return;
                }
                let previous = self.icons.clone();
                self.icons.set_external(name);
                match self.icons.save() {
                    Ok(()) => {
                        // Globalnie dla wszystkich procesów libLuny (platforma wykrywa
                        // też zmianę pliku, ale ta aplikacja ma odświeżyć się od razu).
                        libluna_core::icons::set_active_icon_set(self.icons.external.clone());
                        self.show_status(tr("Icon set updated"));
                    }
                    Err(err) => {
                        self.icons = previous;
                        self.icons.apply_current_global();
                        self.show_status(format!("{}: {err}", tr("Could not save icon set")))
                    }
                }
            }
            Action::SetLockTheme(preference) => {
                if self.lock.theme == preference {
                    return;
                }
                self.lock.theme = preference;
                self.save_lock();
            }
            Action::SetLockAccent(accent) => {
                if self.lock.accent == Some(accent) {
                    return;
                }
                self.lock.accent = Some(accent);
                self.save_lock();
            }
            Action::ToggleLockClock => {
                self.lock.show_clock = !self.lock.show_clock;
                self.save_lock();
            }
            Action::SetLockWallpaper(wallpaper) => {
                if self.lock.wallpaper == wallpaper {
                    return;
                }
                self.lock.wallpaper = wallpaper;
                self.save_lock();
            }
            Action::SetNotificationPlacement(placement) => {
                if self.notification_placement == placement {
                    return;
                }
                self.notification_placement = placement;
                match notifications::save_placement(placement) {
                    Ok(()) => self.show_status(tr("Notification position updated")),
                    Err(err) => self.show_status(format!(
                        "{}: {err}",
                        tr("Could not save notification position")
                    )),
                }
            }
            Action::SetWorkspaceCount(count) => {
                if self.workspace_layout.count() == count {
                    return;
                }
                self.workspace_layout.set_count(count);
                match self.workspace_layout.save() {
                    Ok(()) => {
                        // Selene dodaje/usuwa uchwyty `ext_workspace_v1` od razu,
                        // więc panel i podgląd `Super+Tab` widzą nowy zestaw.
                        control_send(&format!("workspace-count {count}"));
                        self.show_status(tr("Workspace count updated"));
                    }
                    Err(err) => {
                        self.show_status(format!("{}: {err}", tr("Could not save workspaces")))
                    }
                }
            }
            Action::EditWorkspaceName(index) => self.open_workspace_editor(index),
            Action::ApplySystemTimezone => self.apply_system_timezone(),
            Action::SetVolume(percent) => {
                let muted = self.volume.map(|(_, m)| m).unwrap_or(false);
                self.volume = Some((percent, muted));
                system::set_volume(percent);
            }
            Action::ToggleMute => {
                system::toggle_mute();
                if let Some((volume, muted)) = self.volume {
                    self.volume = Some((volume, !muted));
                }
            }
            Action::SetBrightness(percent) => {
                self.brightness = Some(percent);
                system::set_brightness(percent);
            }
            Action::TogglePowerSaver => {
                self.power_saver = !self.power_saver;
                system::set_power_saver(self.power_saver);
                if system::ppd_available() {
                    let profile = if self.power_saver {
                        "power-saver"
                    } else {
                        "balanced"
                    };
                    system::set_power_profile(profile);
                    self.power_profile = Some(profile.to_owned());
                } else {
                    control_send(&format!("power-saver {}", u8::from(self.power_saver)));
                }
            }
            Action::ToggleDnd => {
                self.dnd = !self.dnd;
                system::set_dnd(self.dnd);
            }
            Action::SetGreeterWallpaper(index) => {
                let Some(path) = self.wallpapers.get(index).cloned() else {
                    return;
                };
                if self.greeter.wallpaper.as_deref() == Some(path.as_path()) {
                    return;
                }
                self.greeter.wallpaper = Some(path);
                self.greeter_dirty = true;
            }
            Action::ClearGreeterWallpaper => {
                if self.greeter.wallpaper.is_none() {
                    return;
                }
                self.greeter.wallpaper = None;
                self.greeter_dirty = true;
            }
            Action::UseDesktopWallpaper => match libluna_render::wallpaper::configured_path() {
                Some(path) if self.greeter.wallpaper.as_deref() != Some(path.as_path()) => {
                    self.greeter.wallpaper = Some(path);
                    self.greeter_dirty = true;
                }
                Some(_) => {}
                None => self.show_status(tr("No desktop wallpaper is set")),
            },
            Action::SetGreeterTheme(preference) => {
                if self.greeter.theme == preference {
                    return;
                }
                self.greeter.theme = preference;
                self.greeter_dirty = true;
            }
            Action::SetGreeterAccent(accent) => {
                if self.greeter.accent == Some(accent) {
                    return;
                }
                self.greeter.accent = Some(accent);
                self.greeter_dirty = true;
            }
            Action::ToggleGreeterClock => {
                self.greeter.show_clock = !self.greeter.show_clock;
                self.greeter_dirty = true;
            }
            Action::ApplyGreeter => self.apply_greeter(),
            Action::TestNotification => self.test_notification(),
        }
    }

    /// Zapisuje ustawienia ekranu blokady (per użytkownik, bez roota).
    fn save_lock(&mut self) {
        match self.lock.save() {
            Ok(()) => self.show_status(tr("Lock screen updated")),
            Err(err) => {
                self.show_status(format!("{}: {err}", tr("Could not save the lock screen")))
            }
        }
    }

    /// Prosi system o zmianę strefy przez polkit (może pokazać okno autoryzacji).
    fn apply_system_timezone(&mut self) {
        let timezone = self.region.effective_timezone();
        match std::process::Command::new("pkexec")
            .args(["timedatectl", "set-timezone", &timezone])
            .spawn()
        {
            Ok(_) => self.show_status(tr("Authorization requested — check the system dialog")),
            Err(err) => self.show_status(format!(
                "{}: {err}",
                tr("Could not start system authorization")
            )),
        }
    }

    /// Otwiera formularz hasła administratora dla zmian ekranu logowania.
    fn apply_greeter(&mut self) {
        if std::env::var_os("LUNA_GREETER_CONFIG").is_some() {
            // Tryb podglądu/testów — zapis bez autoryzacji.
            self.save_greeter();
            return;
        }
        if self.admin.as_ref().is_some_and(AdminPrompt::is_pending) {
            return;
        }
        self.admin = Some(AdminPrompt::new());
    }

    /// Zatwierdza hasło z formularza; zapis idzie w wątku tła (sudo/PAM).
    pub fn submit_admin_password(&mut self) {
        let Some(prompt) = &mut self.admin else {
            return;
        };
        if prompt.is_pending() {
            return;
        }
        let password = prompt.password.text().to_owned();
        if password.is_empty() {
            prompt.error = Some(tr("Enter the administrator password"));
            return;
        }
        let settings = self.greeter.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::greeter_admin::save_with_password(&settings, &password);
            let _ = tx.send(result);
        });
        prompt.password.clear();
        prompt.error = None;
        prompt.pending = Some(rx);
    }

    /// Odbiera wynik zapisu z formularza hasła (bez blokowania UI).
    pub fn poll_admin(&mut self) {
        let result = {
            let Some(prompt) = &mut self.admin else {
                return;
            };
            let Some(rx) = prompt.pending.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(tr("Administrator authorization failed")))
                }
            }
        };
        match result {
            Some(Ok(())) => {
                self.admin = None;
                self.greeter_dirty = false;
                self.reload_greeter();
                self.show_status(tr("Login screen updated"));
            }
            Some(Err(err)) => {
                if let Some(prompt) = &mut self.admin {
                    prompt.pending = None;
                    prompt.error = Some(err);
                }
            }
            None => {}
        }
    }

    /// Wysyła testowe powiadomienie przez D-Bus (`luna-shell` jest daemonem).
    fn test_notification(&mut self) {
        let title = tr("Luna test notification");
        let body = tr("If you can see this, notifications work");
        match crate::notify::send_test_notification(&title, &body) {
            Ok(()) => self.show_status(tr("Test notification sent")),
            Err(err) => self.show_status(format!(
                "{}: {err}",
                tr("Could not send the test notification")
            )),
        }
    }

    /// Wczytuje konfigurację greetera z dysku (po udanym zapisie).
    fn reload_greeter(&mut self) {
        self.greeter = GreeterSettings::load();
        self.wallpapers = pages::wallpapers();
    }

    /// Zapisuje ustawienia ekranu logowania i odświeża stan z dysku.
    ///
    /// Pomocnik przez `pkexec` może skopiować tapetę do `/var/lib/luna/wallpapers`,
    /// więc po zapisie czytamy plik ponownie — UI pokazuje to, co realnie zobaczy greeter.
    fn save_greeter(&mut self) {
        match crate::greeter_admin::save(&self.greeter) {
            Ok(()) => {
                self.greeter_dirty = false;
                self.reload_greeter();
                self.show_status(tr("Login screen updated"));
            }
            Err(err) => {
                // Cofamy zmianę w UI, żeby nie pokazywać stanu, którego nie ma na dysku.
                self.reload_greeter();
                self.show_status(format!(
                    "{}: {err}",
                    tr("Could not update the login screen")
                ));
            }
        }
    }
}

fn nav_entries() -> (NavList, Vec<Option<Page>>) {
    let sections: Vec<(&str, Vec<Page>)> = vec![
        (
            "Environment",
            vec![
                Page::Appearance,
                Page::Icons,
                Page::Login,
                Page::LockScreen,
                Page::Notifications,
                Page::Language,
                Page::Workspaces,
            ],
        ),
        ("Hardware", vec![Page::Displays, Page::Sound, Page::Power]),
        ("Information", vec![Page::System, Page::About]),
    ];
    let mut entries = Vec::new();
    let mut pages = Vec::new();
    let mut icons = Vec::new();
    for (section, items) in sections {
        entries.push(NavEntry::Section(tr(section)));
        pages.push(None);
        icons.push(None);
        for page in items {
            entries.push(NavEntry::Item(tr(page.title_key())));
            pages.push(Some(page));
            icons.push(Some(page.icon()));
        }
    }
    let mut nav = NavList::new(entries);
    nav.set_icons(icons);
    (nav, pages)
}

/// Wysyła polecenie do Selene (`$XDG_RUNTIME_DIR/luna-control`).
fn control_send(line: &str) {
    use std::io::Write;
    let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") else {
        return;
    };
    let path = std::path::Path::new(&runtime).join("luna-control");
    if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(path) {
        let _ = writeln!(stream, "{line}");
    }
}

/// Zamienia wielolinijkowy tekst na jedną linię (protokół `luna-control`).
fn one_line(text: &str) -> String {
    text.replace(['\n', '\r'], " ").trim().to_owned()
}

/// Uruchamia centrum ustawień.
pub fn run() -> Result<(), Box<dyn Error>> {
    // Język z Ustawień (jeśli wybrany) ma pierwszeństwo przed `LANG`.
    set_locale(libluna_i18n::preferred_locale());
    libluna_i18n::load_catalogs_from_dirs();

    let settings = ThemeSettings::load();
    let system_dark = std::env::var("LUNA_SYSTEM_DARK")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false);
    let config = AppConfig::new(tr("Settings"))
        .app_id("eu.luna.Settings")
        .initial_size(1080, 760)
        .settings(settings.clone())
        .system_dark(system_dark);

    let mut app = SettingsApp::new(&settings);
    app.apply_icon_theme_globally();
    // Język może zmienić inna aplikacja (np. luna-files) — nawigacja trzyma
    // gotowe etykiety, więc odświeżamy ją po zmianie katalogu.
    let mut ui_language = libluna_i18n::locale().language().to_owned();
    run_app(config, move |frame: &mut Frame| {
        let language = libluna_i18n::locale().language().to_owned();
        if language != ui_language {
            ui_language = language.clone();
            app.language = language;
            app.rebuild_nav();
            frame.set_title(tr("Settings"));
        }
        app.apply_pending(frame);
        app.sync_system();
        app.poll_admin();

        ui::handle_events(&mut app, frame.events, frame.size, &frame.theme, frame.text);

        // Ctrl+C/X w polach tekstowych → systemowy schowek; Ctrl+V → odczyt.
        if let Some(text) = app.clipboard_text.take() {
            frame.copy_to_clipboard(text);
        }
        if app.paste_requested {
            app.paste_requested = false;
            frame.paste_from_clipboard();
        }

        ui::paint(&app, frame.scene, frame.size, &frame.theme, frame.text);

        if app.status.is_animating() {
            app.status.advance(frame.delta_time(), 2.5);
            frame.request_frame();
        }
        if app.admin.as_ref().is_some_and(|admin| admin.is_pending()) {
            frame.request_frame_after(Duration::from_millis(50));
        }
        if let Some(until) = app.status_until {
            if std::time::Instant::now() >= until {
                app.status.set_open_animated(false);
                app.status_until = None;
            }
            frame.request_frame();
        }
        // Stan systemu (bateria, głośność) zmienia się poza UI.
        frame.request_frame_after(Duration::from_secs(1));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> SettingsApp {
        SettingsApp::new(&ThemeSettings::load())
    }

    #[test]
    fn picker_filters_and_preselects_current_language() {
        let mut app = app();
        app.language = "de".to_owned();
        app.open_picker(PickerKind::Language);
        let picker = app.picker.as_ref().expect("picker");
        let filtered = app.picker_filtered();
        assert!(!filtered.is_empty());
        assert_eq!(filtered[picker.selected].value, "de");

        app.picker.as_mut().unwrap().query.set_text("de");
        let filtered = app.picker_filtered();
        assert!(filtered.iter().all(|item| item.value == "de"));
    }

    #[test]
    fn workspace_editor_starts_with_current_name() {
        let mut app = app();
        app.workspace_layout.set_name(0, Some("Kod"));
        app.open_workspace_editor(0);
        let editor = app.editor.as_ref().expect("editor");
        assert_eq!(editor.entry.text(), "Kod");
        assert!(editor.title.contains('1'));
    }

    #[test]
    fn every_page_has_nav_entry() {
        let app = app();
        assert_eq!(
            app.nav_pages.iter().filter(|p| p.is_some()).count(),
            Page::ALL.len()
        );
        assert!(app
            .nav
            .entries()
            .iter()
            .any(|e| matches!(e, NavEntry::Section(_))));
    }

    #[test]
    fn go_to_selects_page_without_losing_nav_selection() {
        let mut app = app();
        app.go_to(Page::Sound);
        assert_eq!(app.page, Page::Sound);
        let selected = app.nav.selected().expect("selection");
        assert_eq!(app.nav_pages[selected], Some(Page::Sound));
    }

    #[test]
    fn style_and_language_actions_update_state() {
        let mut app = app();
        app.on_action(Action::SetPreference(ThemePreference::Dark));
        assert_eq!(app.theme_pref, ThemePreference::Dark);
        app.on_action(Action::SetAccent(Accent::Purple));
        assert_eq!(app.accent, Accent::Purple);
        app.on_action(Action::SetStyle(0));
        assert_eq!(app.style, None);
    }

    #[test]
    fn status_toast_hides_after_deadline() {
        let mut app = app();
        app.show_status("test");
        // Skracamy termin, żeby test nie czekał 3 s; dalej idziemy tą samą ścieżką co pętla.
        app.status_until = Some(std::time::Instant::now());
        let mut frames = 0;
        while app.status.is_animating() || app.status_until.is_some() {
            if app.status.is_animating() {
                app.status.advance(0.016, 2.5);
            }
            if let Some(until) = app.status_until {
                if std::time::Instant::now() >= until {
                    app.status.set_open_animated(false);
                    app.status_until = None;
                }
            }
            frames += 1;
            assert!(frames < 1000, "toast statusu nie zniknął");
        }
        assert!(!app.status.is_open());
        assert!(app.status.progress() < 0.01);
    }
}
