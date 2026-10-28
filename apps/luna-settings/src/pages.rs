//! Strony centrum ustawień: model wierszy, akcje i odczyt danych systemowych.
//!
//! Rysowanie nie zna typów kontrolek — [`ui`](crate::ui) dostaje gotowe grupy i
//! wiersze, a kliknięcia rozwiązuje przez [`Action`]. Dzięki temu strony można
//! testować bez okna.

use std::path::{Path, PathBuf};

use libluna_core::{Accent, GreeterSettings, IconTheme, LockWallpaper, ThemePreference};
use libluna_i18n::{tr, tr_args};
use libluna_widgets::IconName;
use luna::notifications::Placement;

use crate::app::SettingsApp;

/// Strona centrum ustawień.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Appearance,
    Icons,
    Login,
    LockScreen,
    Notifications,
    Language,
    Workspaces,
    Displays,
    Sound,
    Power,
    System,
    About,
}

impl Page {
    pub const ALL: [Page; 12] = [
        Page::Appearance,
        Page::Icons,
        Page::Login,
        Page::LockScreen,
        Page::Notifications,
        Page::Language,
        Page::Workspaces,
        Page::Displays,
        Page::Sound,
        Page::Power,
        Page::System,
        Page::About,
    ];

    /// Klucz tłumaczenia tytułu.
    pub fn title_key(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Icons => "Icons",
            Page::Login => "Login Screen",
            Page::LockScreen => "Lock Screen",
            Page::Notifications => "Notifications",
            Page::Language => "Language and Region",
            Page::Workspaces => "Workspaces",
            Page::Displays => "Displays",
            Page::Sound => "Sound",
            Page::Power => "Power",
            Page::System => "System",
            Page::About => "About Luna",
        }
    }

    /// Klucz tłumaczenia opisu strony.
    pub fn subtitle_key(self) -> &'static str {
        match self {
            Page::Appearance => "Personalize the look and feel of Luna",
            Page::Icons => "Icon style and preview",
            Page::Login => "Wallpaper and colors of the sign-in screen",
            Page::LockScreen => "Look of your lock screen (no administrator needed)",
            Page::Notifications => "Control notifications and history",
            Page::Language => "Language, formats and time zone",
            Page::Workspaces => "Number and names of workspaces",
            Page::Displays => "Displays, resolution and brightness",
            Page::Sound => "Output volume and devices",
            Page::Power => "Battery and power management",
            Page::System => "System and environment information",
            Page::About => "About the Luna desktop",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Page::Appearance => IconName::Brightness,
            Page::Icons => IconName::Image,
            Page::Login => IconName::User,
            Page::LockScreen => IconName::Star,
            Page::Notifications => IconName::Info,
            Page::Language => IconName::Globe,
            Page::Workspaces => IconName::Monitor,
            Page::Displays => IconName::Monitor,
            Page::Sound => IconName::Volume,
            Page::Power => IconName::Battery,
            Page::System => IconName::Info,
            Page::About => IconName::Moon,
        }
    }
}

/// Akcja wywoływana przez kontrolkę.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    SetPreference(ThemePreference),
    SetAccent(Accent),
    SetStyle(usize),
    SetLanguage(usize),
    /// Otwiera wyszukiwany wybór (język, locale, strefa, ikony).
    OpenPicker(PickerKind),
    SetIconTheme(IconTheme),
    /// Zewnętrzny zestaw ikon freedesktop (`None` = tylko wektorowe).
    SetExternalIconSet(Option<String>),
    SetVolume(u8),
    ToggleMute,
    SetBrightness(u8),
    TogglePowerSaver,
    ToggleDnd,
    /// Tapeta ekranu logowania (indeks w [`SettingsApp::wallpapers`]).
    SetGreeterWallpaper(usize),
    /// Ekran logowania bez tapety — gradient akcentu.
    ClearGreeterWallpaper,
    SetGreeterTheme(ThemePreference),
    SetGreeterAccent(Accent),
    ToggleGreeterClock,
    /// Zapisuje szkic ekranu logowania (formularz hasła administratora).
    ApplyGreeter,
    /// Użyj tapety pulpitu (jeśli jest ustawiona).
    UseDesktopWallpaper,
    // — Ekran blokady (per użytkownik, bez roota) —
    SetLockTheme(ThemePreference),
    SetLockAccent(Accent),
    ToggleLockClock,
    SetLockWallpaper(LockWallpaper),
    // — Powiadomienia / przestrzenie —
    SetNotificationPlacement(Placement),
    /// Wysyła testowe powiadomienie do daemona `luna-shell`.
    TestNotification,
    SetWorkspaceCount(usize),
    EditWorkspaceName(usize),
    /// Ustawia strefę systemową przez `pkexec timedatectl` (wymaga autoryzacji).
    ApplySystemTimezone,
}

/// Rodzaj wyszukiwanego wyboru.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Language,
    Locale,
    Timezone,
    IconSet,
}

/// Do czego należy wybrany akcent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccentTarget {
    /// Akcent całego środowiska (zapis w `theme.conf` użytkownika).
    Desktop,
    /// Akcent ekranu logowania (`/etc/luna/greeter.conf`).
    Greeter,
    /// Akcent ekranu blokady (`~/.config/luna/lock.conf`).
    Lock,
}

/// Który suwak przeciągamy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderKind {
    Volume,
    Brightness,
}

/// Kontrolka w wierszu ustawień.
#[derive(Debug, Clone)]
pub enum Control {
    /// Tylko tytuł/podtytuł (wiersz informacyjny).
    None,
    /// Wartość po prawej stronie.
    Value(String),
    Switch(bool),
    Slider {
        kind: SliderKind,
        value: f64,
    },
    /// Segmentowy przełącznik; `actions[i]` odpowiada opcji `i`.
    Segmented {
        options: Vec<String>,
        selected: usize,
        actions: Vec<Action>,
    },
    /// Paleta akcentów (indeks w [`Accent::ALL`]).
    Accents {
        selected: usize,
        target: AccentTarget,
    },
    /// Pojedynczy wybór („ptaszek”) — np. wybór tapety ekranu logowania.
    Check(bool),
    /// Pole wyboru otwierające wyszukiwany picker (splitbutton z chevronem).
    Select {
        value: String,
    },
    /// Pasek podglądu ikon (styl ikon).
    IconStrip(Vec<IconName>),
    /// Miniatura tapety z zaznaczeniem.
    Wallpaper {
        path: PathBuf,
        selected: bool,
    },
    Button {
        label: String,
        suggested: bool,
    },
}

/// Wiersz ustawień.
#[derive(Debug, Clone)]
pub struct Row {
    pub title: String,
    pub subtitle: Option<String>,
    pub control: Control,
    pub action: Action,
}

/// Grupa ustawień.
#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    pub rows: Vec<Row>,
}

fn row(title: impl Into<String>, control: Control, action: Action) -> Row {
    Row {
        title: title.into(),
        subtitle: None,
        control,
        action,
    }
}

fn row_sub(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    control: Control,
    action: Action,
) -> Row {
    Row {
        title: title.into(),
        subtitle: Some(subtitle.into()),
        control,
        action,
    }
}

fn value_row(title: impl Into<String>, value: impl Into<String>) -> Row {
    row(title, Control::Value(value.into()), Action::None)
}

fn info_row(title: impl Into<String>, subtitle: impl Into<String>) -> Row {
    row_sub(title, subtitle, Control::None, Action::None)
}

fn group(title: impl Into<String>, rows: Vec<Row>) -> Group {
    Group {
        title: title.into(),
        rows,
    }
}

/// Buduje grupy i wiersze dla aktywnej strony.
pub fn groups(app: &SettingsApp) -> Vec<Group> {
    match app.page {
        Page::Appearance => appearance(app),
        Page::Icons => icons(app),
        Page::Notifications => notifications(app),
        Page::Language => language(app),
        Page::Login => login(app),
        Page::LockScreen => lock_screen(app),
        Page::Workspaces => workspaces(app),
        Page::Displays => displays(app),
        Page::Sound => sound(app),
        Page::Power => power(app),
        Page::System => system(),
        Page::About => about(),
    }
}

fn appearance(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetPreference(ThemePreference::Light),
        Action::SetPreference(ThemePreference::Dark),
        Action::SetPreference(ThemePreference::System),
    ];
    let mode_selected = match app.theme_pref {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };

    let mut rows = vec![
        row_sub(
            tr("Theme mode"),
            tr("System colors are used when Auto is selected"),
            Control::Segmented {
                options: mode,
                selected: mode_selected,
                actions: mode_actions,
            },
            Action::None,
        ),
        row(
            tr("Accent color"),
            Control::Accents {
                selected: Accent::ALL
                    .iter()
                    .position(|a| *a == app.accent)
                    .unwrap_or(0),
                target: AccentTarget::Desktop,
            },
            Action::None,
        ),
    ];

    if !app.styles.is_empty() {
        let mut options = vec![tr("Default")];
        let mut actions = vec![Action::SetStyle(0)];
        let mut selected = 0;
        for (i, style) in app.styles.iter().enumerate() {
            options.push(style.name.clone());
            actions.push(Action::SetStyle(i + 1));
            if app.style.as_deref() == Some(style.id.as_str()) {
                selected = i + 1;
            }
        }
        rows.push(row(
            tr("Style"),
            Control::Segmented {
                options,
                selected,
                actions,
            },
            Action::None,
        ));
    }

    vec![group(tr("Appearance"), rows)]
}

fn notifications(app: &SettingsApp) -> Vec<Group> {
    let placements = vec![
        "↖".to_owned(),
        "↗".to_owned(),
        "↙".to_owned(),
        "↘".to_owned(),
    ];
    let placement_actions = Placement::ALL
        .iter()
        .map(|p| Action::SetNotificationPlacement(*p))
        .collect();
    let placement_selected = Placement::ALL
        .iter()
        .position(|p| *p == app.notification_placement)
        .unwrap_or(0);

    vec![
        group(
            tr("Notifications"),
            vec![
                row_sub(
                    tr("Notification position"),
                    format!(
                        "{}  ↖ ↗ ↙ ↘",
                        tr("Corner of the screen where toasts appear")
                    ),
                    Control::Segmented {
                        options: placements,
                        selected: placement_selected,
                        actions: placement_actions,
                    },
                    Action::None,
                ),
                row_sub(
                    tr("Do Not Disturb"),
                    tr("Hide toasts, keep them in the notification center"),
                    Control::Switch(app.dnd),
                    Action::ToggleDnd,
                ),
                info_row(
                    tr("History"),
                    tr("Click the clock on the panel to open the notification center"),
                ),
                row_sub(
                    tr("Test notification"),
                    tr("Sends a sample notification through the running daemon"),
                    Control::Button {
                        label: tr("Send test"),
                        suggested: false,
                    },
                    Action::TestNotification,
                ),
                value_row(tr("Notification daemon"), "luna-shell"),
                info_row(
                    tr("Supported features"),
                    tr("Actions, replaces_id, CloseNotification, persistent notifications"),
                ),
            ],
        ),
        group(
            tr("Information"),
            vec![info_row(
                tr("Provided by luna-shell over D-Bus"),
                "org.freedesktop.Notifications",
            )],
        ),
    ]
}

fn language(app: &SettingsApp) -> Vec<Group> {
    let language_name = app
        .languages
        .iter()
        .find(|(code, _)| *code == app.language)
        .map(|(code, name)| format!("{name} ({code})"))
        .unwrap_or_else(|| app.language.clone());

    let locale = app.region.effective_locale();
    let locale_value = if app.region.locale.is_some() {
        locale
    } else {
        format!("{}: {locale}", tr("System"))
    };
    let timezone = app.region.effective_timezone();
    let timezone_value = if app.region.timezone.is_some() {
        timezone.clone()
    } else {
        format!("{}: {timezone}", tr("System"))
    };

    vec![
        group(
            tr("Language and Region"),
            vec![
                row_sub(
                    tr("Interface language"),
                    tr("Applies immediately to all Luna apps"),
                    Control::Select {
                        value: language_name,
                    },
                    Action::OpenPicker(PickerKind::Language),
                ),
                row_sub(
                    tr("Region and formats"),
                    tr("Date, time and number formats"),
                    Control::Select { value: locale_value },
                    Action::OpenPicker(PickerKind::Locale),
                ),
                row_sub(
                    tr("Time zone"),
                    tr("Used by the Luna clock (per session)"),
                    Control::Select {
                        value: timezone_value,
                    },
                    Action::OpenPicker(PickerKind::Timezone),
                ),
                row_sub(
                    tr("Apply time zone to the system"),
                    tr("Requires administrator approval (polkit)"),
                    Control::Button {
                        label: tr("Apply"),
                        suggested: false,
                    },
                    Action::ApplySystemTimezone,
                ),
            ],
        ),
        group(
            tr("Configuration"),
            vec![
                value_row(
                    tr("Saved to ~/.config/luna/locale.conf"),
                    libluna_i18n::locale_config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                value_row(
                    tr("Region configuration"),
                    libluna_core::RegionSettings::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                info_row(
                    tr("Lists are detected from the system"),
                    tr("Interface language uses Luna catalogs; locale and time zone come from the OS"),
                ),
                info_row(
                    tr("Other applications"),
                    tr("Luna apps switch immediately; other programs pick up the language after restart"),
                ),
            ],
        ),
    ]
}

fn icons(app: &SettingsApp) -> Vec<Group> {
    let selected = IconTheme::ALL
        .iter()
        .position(|theme| *theme == app.icons.theme)
        .unwrap_or(0);
    let options = vec![tr("Luna (pastel)"), tr("Monochrome")];
    let actions = IconTheme::ALL
        .iter()
        .map(|theme| Action::SetIconTheme(*theme))
        .collect();
    let preview = vec![
        IconName::Files,
        IconName::Terminal,
        IconName::Settings,
        IconName::Globe,
        IconName::Music,
        IconName::Image,
        IconName::Wifi,
        IconName::Moon,
    ];
    let external = match &app.icons.external {
        Some(name) => libluna_core::icons::find_icon_set(name)
            .map(|set| libluna_core::icons::icon_set_display_name(&set))
            .unwrap_or_else(|| name.clone()),
        None => tr("Vector icons (Luna)"),
    };
    let dirs = libluna_core::icons::icon_search_dirs()
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(" · ");

    vec![
        group(
            tr("Icons"),
            vec![
                row_sub(
                    tr("Icon style"),
                    tr("Pastel illustrations or monochrome icons"),
                    Control::Segmented {
                        options,
                        selected,
                        actions,
                    },
                    Action::None,
                ),
                row_sub(
                    tr("External icon set"),
                    tr("Global for the whole environment; installed in the icon directories below"),
                    Control::Select { value: external },
                    Action::OpenPicker(PickerKind::IconSet),
                ),
                row(tr("Preview"), Control::IconStrip(preview), Action::None),
            ],
        ),
        group(
            tr("Information"),
            vec![
                info_row(
                    tr("Icons are vector-based"),
                    tr("The style changes colors; an external set replaces application and place icons"),
                ),
                value_row(tr("Installed icon sets"), app.icon_sets.len().to_string()),
                info_row(tr("Icon directories"), dirs),
            ],
        ),
    ]
}

fn lock_screen(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetLockTheme(ThemePreference::Light),
        Action::SetLockTheme(ThemePreference::Dark),
        Action::SetLockTheme(ThemePreference::System),
    ];
    let mode_selected = match app.lock.theme {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };
    let accent = app.lock.accent.unwrap_or(Accent::Blue);
    let accent_selected = Accent::ALL
        .iter()
        .position(|candidate| *candidate == accent)
        .unwrap_or(0);

    vec![
        group(
            tr("Appearance"),
            vec![
                row_sub(
                    tr("Theme mode"),
                    tr("Colors of the lock screen card and background"),
                    Control::Segmented {
                        options: mode,
                        selected: mode_selected,
                        actions: mode_actions,
                    },
                    Action::None,
                ),
                row(
                    tr("Accent color"),
                    Control::Accents {
                        selected: accent_selected,
                        target: AccentTarget::Lock,
                    },
                    Action::None,
                ),
                row(
                    tr("Show clock"),
                    Control::Switch(app.lock.show_clock),
                    Action::ToggleLockClock,
                ),
            ],
        ),
        group(
            tr("Background"),
            vec![
                row_sub(
                    tr("Desktop wallpaper"),
                    tr("The same wallpaper as the desktop, dimmed"),
                    Control::Check(app.lock.wallpaper == LockWallpaper::Desktop),
                    Action::SetLockWallpaper(LockWallpaper::Desktop),
                ),
                row_sub(
                    tr("Solid color"),
                    tr("No image — a quiet, dark background"),
                    Control::Check(app.lock.wallpaper == LockWallpaper::None),
                    Action::SetLockWallpaper(LockWallpaper::None),
                ),
            ],
        ),
        group(
            tr("Configuration"),
            vec![
                value_row(
                    tr("Lock screen configuration"),
                    libluna_core::LockSettings::config_path()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "—".to_owned()),
                ),
                info_row(
                    tr("No administrator approval required"),
                    tr("The lock screen runs in your session and uses your user settings"),
                ),
            ],
        ),
    ]
}

fn workspaces(app: &SettingsApp) -> Vec<Group> {
    let options: Vec<String> = (1..=libluna_core::MAX_WORKSPACES)
        .map(|n| n.to_string())
        .collect();
    let actions: Vec<Action> = (1..=libluna_core::MAX_WORKSPACES)
        .map(Action::SetWorkspaceCount)
        .collect();
    let selected = app
        .workspace_layout
        .count()
        .saturating_sub(1)
        .min(options.len() - 1);

    let mut name_rows = Vec::new();
    for index in 0..app.workspace_layout.count() {
        name_rows.push(row(
            tr_args("Workspace {n}", &[("n", &(index + 1).to_string())]),
            Control::Value(app.workspace_layout.name(index)),
            Action::EditWorkspaceName(index),
        ));
    }

    vec![
        group(
            tr("Workspaces"),
            vec![row_sub(
                tr("Number of workspaces"),
                tr("The panel and Super+Tab update immediately"),
                Control::Segmented {
                    options,
                    selected,
                    actions,
                },
                Action::None,
            )],
        ),
        group(tr("Names"), name_rows),
        group(
            tr("Information"),
            vec![info_row(
                tr("Names and count update immediately"),
                tr("Click a workspace to rename it; the panel refreshes through luna-control"),
            )],
        ),
    ]
}

fn login(app: &SettingsApp) -> Vec<Group> {
    let mode = vec![tr("Light"), tr("Dark"), tr("Auto")];
    let mode_actions = vec![
        Action::SetGreeterTheme(ThemePreference::Light),
        Action::SetGreeterTheme(ThemePreference::Dark),
        Action::SetGreeterTheme(ThemePreference::System),
    ];
    let mode_selected = match app.greeter.theme {
        ThemePreference::Light => 0,
        ThemePreference::Dark => 1,
        ThemePreference::System => 2,
    };
    let accent = app.greeter.accent.unwrap_or(Accent::Blue);
    let accent_selected = Accent::ALL
        .iter()
        .position(|candidate| *candidate == accent)
        .unwrap_or(0);

    let appearance = vec![
        row_sub(
            tr("Theme mode"),
            tr("Colors of the sign-in screen"),
            Control::Segmented {
                options: mode,
                selected: mode_selected,
                actions: mode_actions,
            },
            Action::None,
        ),
        row(
            tr("Accent color"),
            Control::Accents {
                selected: accent_selected,
                target: AccentTarget::Greeter,
            },
            Action::None,
        ),
        row(
            tr("Show clock"),
            Control::Switch(app.greeter.show_clock),
            Action::ToggleGreeterClock,
        ),
    ];

    // Tapety: gradient + wykryte pliki PNG. Greeter dekoduje wyłącznie PNG.
    let mut wallpaper_rows = vec![row_sub(
        tr("Gradient"),
        tr("No image — a subtle accent glow"),
        Control::Check(app.greeter.wallpaper.is_none()),
        Action::ClearGreeterWallpaper,
    )];

    if let Some(desktop) = libluna_render::wallpaper::configured_path() {
        wallpaper_rows.push(row_sub(
            tr("Desktop wallpaper"),
            desktop.display().to_string(),
            Control::Button {
                label: tr("Use"),
                suggested: false,
            },
            Action::UseDesktopWallpaper,
        ));
    }

    if app.wallpapers.is_empty() {
        wallpaper_rows.push(info_row(
            tr("No wallpapers found"),
            "/usr/share/wallpapers · ~/.local/share/backgrounds",
        ));
    } else {
        for (index, path) in app.wallpapers.iter().enumerate() {
            let selected = app.greeter.wallpaper.as_deref() == Some(path.as_path());
            wallpaper_rows.push(row_sub(
                wallpaper_label(path),
                path.parent()
                    .map(|parent| parent.display().to_string())
                    .unwrap_or_default(),
                Control::Wallpaper {
                    path: path.clone(),
                    selected,
                },
                Action::SetGreeterWallpaper(index),
            ));
        }
    }

    if let Some(current) = &app.greeter.wallpaper {
        if !app.wallpapers.iter().any(|path| path == current) {
            wallpaper_rows.push(value_row(
                tr("Current wallpaper"),
                current.display().to_string(),
            ));
        }
    }

    let apply = vec![
        row_sub(
            tr("Apply login screen changes"),
            if app.greeter_dirty {
                tr("You have unsaved changes — administrator approval is required")
            } else {
                tr("Saves the current selection with administrator approval")
            },
            Control::Button {
                label: tr("Apply changes"),
                suggested: true,
            },
            Action::ApplyGreeter,
        ),
        info_row(
            tr("Changes require administrator approval"),
            tr("The sign-in screen runs as a separate user"),
        ),
    ];

    vec![
        group(tr("Appearance"), appearance),
        group(tr("Apply changes"), apply),
        group(tr("Wallpaper"), wallpaper_rows),
        group(
            tr("Configuration"),
            vec![value_row(
                tr("Configuration file"),
                GreeterSettings::config_path().display().to_string(),
            )],
        ),
    ]
}

/// Czytelna nazwa tapety: `katalog/plik.png`.
fn wallpaper_label(path: &Path) -> String {
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("wallpaper.png");
    match path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
    {
        Some(dir) => format!("{dir}/{file}"),
        None => file.to_owned(),
    }
}

/// Maksymalna liczba proponowanych tapet (żeby strona pozostała użyteczna).
const MAX_WALLPAPERS: usize = 40;

/// Tapety systemowe i użytkownika w formacie PNG (tylko taki greeter dekoduje).
pub fn wallpapers() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/wallpapers"),
        PathBuf::from("/usr/share/backgrounds"),
        PathBuf::from("/usr/local/share/wallpapers"),
        PathBuf::from("/var/lib/luna/wallpapers"),
    ];
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    if let Some(data) = data_home {
        roots.push(data.join("backgrounds"));
        roots.push(data.join("wallpapers"));
    }

    let mut found = Vec::new();
    for root in roots {
        scan_png(&root, 0, &mut found);
    }
    found.sort();
    found.dedup();
    found.truncate(MAX_WALLPAPERS);
    found
}

fn scan_png(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 3 || out.len() >= MAX_WALLPAPERS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if out.len() >= MAX_WALLPAPERS {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            scan_png(&path, depth + 1, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            out.push(path);
        }
    }
}

fn displays(app: &SettingsApp) -> Vec<Group> {
    let mons = monitors();
    let mut rows = Vec::new();
    if mons.is_empty() {
        rows.push(info_row(tr("No displays detected"), tr("Not detected")));
    } else {
        for m in &mons {
            let (subtitle, value) = if m.connected {
                (
                    m.mode.clone().unwrap_or_else(|| tr("Not detected")),
                    if m.enabled {
                        tr("Enabled")
                    } else {
                        tr("Disabled")
                    },
                )
            } else {
                (tr("Not connected"), tr("Disabled"))
            };
            rows.push(row_sub(
                m.name.clone(),
                subtitle,
                Control::Value(value),
                Action::None,
            ));
        }
    }

    let brightness = match app.brightness {
        Some(v) => row(
            tr("Screen brightness"),
            Control::Slider {
                kind: SliderKind::Brightness,
                value: v as f64 / 100.0,
            },
            Action::SetBrightness(0),
        ),
        None => value_row(tr("Brightness"), tr("Unavailable")),
    };

    vec![
        group(tr("Monitor"), rows),
        group(tr("Brightness"), vec![brightness]),
        group(
            tr("Resolution"),
            vec![info_row(
                tr("Adjusting resolution and layout requires Selene output management"),
                "zwlr_output_manager_v1",
            )],
        ),
    ]
}

fn sound(app: &SettingsApp) -> Vec<Group> {
    let (volume, muted) = app.volume.unwrap_or((0, false));
    let sink = app.audio_sink.clone().unwrap_or_else(|| tr("Unavailable"));
    let volume_row = if app.volume.is_some() {
        row_sub(
            tr("Volume"),
            sink,
            Control::Slider {
                kind: SliderKind::Volume,
                value: volume as f64 / 100.0,
            },
            Action::SetVolume(0),
        )
    } else {
        row_sub(
            tr("Volume"),
            sink,
            Control::Value(tr("Unavailable")),
            Action::None,
        )
    };
    vec![group(
        tr("Sound"),
        vec![
            volume_row,
            row(tr("Mute"), Control::Switch(muted), Action::ToggleMute),
        ],
    )]
}

fn power(app: &SettingsApp) -> Vec<Group> {
    let battery_rows = match app.battery {
        Some(b) => vec![
            value_row(tr("Level"), format!("{}%", b.percent)),
            value_row(
                tr("Status"),
                if b.charging {
                    tr("Charging")
                } else {
                    tr("Discharging")
                },
            ),
            value_row(
                tr("Remaining time"),
                match b.minutes {
                    Some(m) if m > 0 => format!("{} h {} min", m / 60, m % 60),
                    _ => "—".to_owned(),
                },
            ),
        ],
        None => vec![value_row(tr("Battery"), tr("No battery (desktop)"))],
    };

    let subtitle = if luna::system::ppd_available() {
        let profile = app
            .power_profile
            .clone()
            .unwrap_or_else(|| "balanced".to_owned());
        format!("{}: {profile}", tr("Power saving"))
    } else {
        tr("Power profiles daemon is unavailable; the compositor frame rate is limited instead.")
    };

    vec![
        group(tr("Battery"), battery_rows),
        group(
            tr("Power saving"),
            vec![row(
                tr("Use the power-saver profile"),
                Control::Switch(app.power_saver),
                Action::TogglePowerSaver,
            )
            .with_subtitle(subtitle)],
        ),
    ]
}

fn system() -> Vec<Group> {
    vec![
        group(
            tr("Device"),
            vec![
                value_row(tr("Hostname"), hostname()),
                value_row(tr("Operating system"), os_pretty_name()),
                value_row(tr("Kernel"), kernel_release()),
                value_row(tr("Architecture"), std::env::consts::ARCH),
            ],
        ),
        group(
            tr("Environment"),
            vec![
                value_row(tr("User"), user_name()),
                value_row(tr("Compositor"), "Selene"),
                value_row(tr("Shell"), "luna-shell"),
                value_row(tr("Toolkit"), "libLuna"),
                value_row(tr("Luna version"), env!("CARGO_PKG_VERSION")),
            ],
        ),
    ]
}

fn about() -> Vec<Group> {
    vec![
        group(
            tr("About Luna"),
            vec![
                value_row(tr("Version"), env!("CARGO_PKG_VERSION")),
                value_row(tr("Desktop environment"), "Luna"),
                value_row(tr("Compositor"), "Selene"),
                value_row(tr("Toolkit"), "libLuna"),
                value_row(tr("License"), "MPL-2.0"),
            ],
        ),
        group(
            tr("Information"),
            vec![
                info_row(
                    tr("Luna is a lightweight desktop environment written in Rust."),
                    "Wayland · libLuna",
                ),
                info_row(
                    tr("Built with libLuna — no GTK, no Qt."),
                    "github.com/mijsys/libLuna",
                ),
            ],
        ),
    ]
}

impl Row {
    fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
}

/// Monitor wykryty w `/sys/class/drm` (tylko odczyt — sterowanie wymaga Selene).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub name: String,
    pub connected: bool,
    pub enabled: bool,
    pub mode: Option<String>,
}

/// Lista podłączonych złączy DRM (best-effort, bez uprawnień).
pub fn monitors() -> Vec<Monitor> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        // Złącza mają postać `card0-HDMI-A-1`; pomijamy `card0`, `renderD128` itd.
        let Some((_, connector)) = dir_name.split_once('-') else {
            continue;
        };
        if connector.is_empty() {
            continue;
        }
        let path = entry.path();
        let status = read_trim(path.join("status")).unwrap_or_default();
        let connected = status == "connected";
        let enabled = read_trim(path.join("enabled")).as_deref() == Some("enabled");
        let mode = std::fs::read_to_string(path.join("modes"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .next()
                    .map(|line| line.trim().to_owned())
                    .filter(|line| !line.is_empty())
            });
        out.push(Monitor {
            name: connector.to_owned(),
            connected,
            enabled,
            mode,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn read_trim(path: std::path::PathBuf) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let (key, value) = line.split_once('=')?;
                (key.trim() == "PRETTY_NAME")
                    .then(|| value.trim().trim_matches('"').trim_matches('\'').to_owned())
            })
        })
        .unwrap_or_else(|| "Linux".to_owned())
}

fn kernel_release() -> String {
    read_trim("/proc/sys/kernel/osrelease".into()).unwrap_or_else(|| tr("Not detected"))
}

fn hostname() -> String {
    read_trim("/etc/hostname".into())
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| tr("Not detected"))
}

fn user_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "?".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_pages_have_titles_and_icons() {
        for page in Page::ALL {
            assert!(!page.title_key().is_empty());
            assert!(!page.subtitle_key().is_empty());
        }
    }

    #[test]
    fn wallpaper_labels_keep_directory_context() {
        assert_eq!(
            wallpaper_label(Path::new("/usr/share/wallpapers/paganlinux/tlo.png")),
            "paganlinux/tlo.png"
        );
        assert_eq!(wallpaper_label(Path::new("tlo.png")), "tlo.png");
    }

    #[test]
    fn monitors_parse_card_prefix() {
        // Funkcja czyta prawdziwe /sys; testujemy tylko, że nie panikuje i zwraca
        // unikalne, posortowane nazwy złączy (na maszynie bez DRM lista jest pusta).
        let list = monitors();
        let mut names: Vec<_> = list.iter().map(|m| m.name.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            list.iter().map(|m| m.name.clone()).collect::<Vec<_>>()
        );
    }
}
