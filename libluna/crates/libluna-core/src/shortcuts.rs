//! Konfigurowalne skróty klawiszowe pulpitu.
//!
//! Selene czyta ten plik przy każdym naciśnięciu klawisza (z cache po mtime),
//! więc zmiana w Ustawieniach działa bez wylogowania.
//!
//! Plik: `$XDG_CONFIG_HOME/luna/shortcuts.conf` (`akcja = kombinacja`):
//!
//! ```text
//! close-window = Super+Q
//! workspace-1 = Super+1
//! ```
//!
//! Skróty Alt+Tab i Alt+F4 oraz klawisze multimedialne celowo zostają stałe
//! (obsługuje je kompozytor niezależnie od konfiguracji użytkownika).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::config::{config_dir, save_atomic};

/// Akcja pulpitu wywoływana skrótem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    CloseWindow,
    Minimize,
    Maximize,
    /// Przywraca okno z maksymalizacji/pełnego ekranu.
    RestoreWindow,
    Fullscreen,
    SnapLeft,
    SnapRight,
    Overview,
    CycleWindows,
    /// Przełączenie przestrzeni (0–7); z Shift przenosi aktywne okno.
    Workspace(usize),
    Screenshot,
    Logout,
}

impl ShortcutAction {
    /// Wszystkie akcje w kolejności prezentacji w Ustawieniach.
    pub const ALL: [ShortcutAction; 19] = [
        ShortcutAction::CloseWindow,
        ShortcutAction::Minimize,
        ShortcutAction::Maximize,
        ShortcutAction::RestoreWindow,
        ShortcutAction::Fullscreen,
        ShortcutAction::SnapLeft,
        ShortcutAction::SnapRight,
        ShortcutAction::Overview,
        ShortcutAction::CycleWindows,
        ShortcutAction::Workspace(0),
        ShortcutAction::Workspace(1),
        ShortcutAction::Workspace(2),
        ShortcutAction::Workspace(3),
        ShortcutAction::Workspace(4),
        ShortcutAction::Workspace(5),
        ShortcutAction::Workspace(6),
        ShortcutAction::Workspace(7),
        ShortcutAction::Screenshot,
        ShortcutAction::Logout,
    ];

    /// Klucz w pliku konfiguracji (`None` dla akcji spoza edytora).
    pub fn id(self) -> String {
        match self {
            ShortcutAction::CloseWindow => "close-window".to_owned(),
            ShortcutAction::Minimize => "minimize".to_owned(),
            ShortcutAction::Maximize => "maximize".to_owned(),
            ShortcutAction::RestoreWindow => "restore-window".to_owned(),
            ShortcutAction::Fullscreen => "fullscreen".to_owned(),
            ShortcutAction::SnapLeft => "snap-left".to_owned(),
            ShortcutAction::SnapRight => "snap-right".to_owned(),
            ShortcutAction::Overview => "overview".to_owned(),
            ShortcutAction::CycleWindows => "cycle-windows".to_owned(),
            ShortcutAction::Workspace(index) => format!("workspace-{}", index + 1),
            ShortcutAction::Screenshot => "screenshot".to_owned(),
            ShortcutAction::Logout => "logout".to_owned(),
        }
    }

    /// Klucz tłumaczenia nazwy akcji (bez numeru przestrzeni).
    pub fn label_key(self) -> &'static str {
        match self {
            ShortcutAction::CloseWindow => "Close window",
            ShortcutAction::Minimize => "Minimize window",
            ShortcutAction::Maximize => "Maximize or restore",
            ShortcutAction::RestoreWindow => "Restore window",
            ShortcutAction::Fullscreen => "Full screen",
            ShortcutAction::SnapLeft => "Snap to the left",
            ShortcutAction::SnapRight => "Snap to the right",
            ShortcutAction::Overview => "Window overview",
            ShortcutAction::CycleWindows => "Switch windows",
            ShortcutAction::Workspace(_) => "Workspace",
            ShortcutAction::Screenshot => "Take a screenshot",
            ShortcutAction::Logout => "Log out",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id.trim() {
            "close-window" => ShortcutAction::CloseWindow,
            "minimize" => ShortcutAction::Minimize,
            "maximize" => ShortcutAction::Maximize,
            "restore-window" => ShortcutAction::RestoreWindow,
            "fullscreen" => ShortcutAction::Fullscreen,
            "snap-left" => ShortcutAction::SnapLeft,
            "snap-right" => ShortcutAction::SnapRight,
            "overview" => ShortcutAction::Overview,
            "cycle-windows" => ShortcutAction::CycleWindows,
            "screenshot" => ShortcutAction::Screenshot,
            "logout" => ShortcutAction::Logout,
            other => {
                let index = other.strip_prefix("workspace-")?.parse::<usize>().ok()?;
                if index == 0 || index > 8 {
                    return None;
                }
                ShortcutAction::Workspace(index - 1)
            }
        })
    }
}

/// Kombinacja klawiszy: modyfikatory + keysym (X11, jak w `KeyEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub logo: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub keysym: u32,
}

impl Binding {
    pub const fn new(logo: bool, ctrl: bool, alt: bool, shift: bool, keysym: u32) -> Self {
        Self {
            logo,
            ctrl,
            alt,
            shift,
            keysym,
        }
    }

    /// Czy zdarzenie klawisza odpowiada temu skrótowi (wielkość liter bez znaczenia).
    pub fn matches(&self, logo: bool, ctrl: bool, alt: bool, shift: bool, keysym: u32) -> bool {
        self.logo == logo
            && self.ctrl == ctrl
            && self.alt == alt
            && self.shift == shift
            && normalize_keysym(self.keysym) == normalize_keysym(keysym)
    }

    /// Parsuje zapis typu `Super+Shift+Q`, `Ctrl+Alt+T`, `Super+Left`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut logo = false;
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut key = None;
        for part in text.split('+') {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            match part.to_ascii_lowercase().as_str() {
                "super" | "logo" | "meta" | "win" => logo = true,
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(parse_key(part)?);
                }
            }
        }
        Some(Self {
            logo,
            ctrl,
            alt,
            shift,
            keysym: key?,
        })
    }

    /// Zapis kanoniczny (np. `Super+Shift+Q`).
    pub fn format(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.logo {
            parts.push("Super".to_owned());
        }
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        parts.push(format_key(self.keysym));
        parts.join("+")
    }

    /// Kombinacja z naciśniętego klawisza (modyfikatory już sprawdzone).
    pub fn from_key(logo: bool, ctrl: bool, alt: bool, shift: bool, keysym: u32) -> Self {
        Self {
            logo,
            ctrl,
            alt,
            shift,
            keysym,
        }
    }
}

fn normalize_keysym(keysym: u32) -> u32 {
    if (0x41..=0x5a).contains(&keysym) {
        keysym + 0x20
    } else {
        keysym
    }
}

fn parse_key(name: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let mut chars = lower.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            return Some(ch as u32);
        }
    }
    Some(match lower.as_str() {
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "tab" => 0xff09,
        "return" | "enter" => 0xff0d,
        "space" => 0x20,
        "grave" | "`" => 0x60,
        "escape" | "esc" => 0xff1b,
        "print" | "printscreen" | "print-screen" => 0xff61,
        "backspace" => 0xff08,
        "delete" => 0xffff,
        "home" => 0xff50,
        "end" => 0xff57,
        "page-up" | "pageup" | "prior" => 0xff55,
        "page-down" | "pagedown" | "next" => 0xff56,
        "f1" => 0xffbe,
        "f2" => 0xffbf,
        "f3" => 0xffc0,
        "f4" => 0xffc1,
        "f5" => 0xffc2,
        "f6" => 0xffc3,
        "f7" => 0xffc4,
        "f8" => 0xffc5,
        "f9" => 0xffc6,
        "f10" => 0xffc7,
        "f11" => 0xffc8,
        "f12" => 0xffc9,
        _ => return None,
    })
}

fn format_key(keysym: u32) -> String {
    let keysym = normalize_keysym(keysym);
    if (0x20..=0x7e).contains(&keysym) {
        let Some(ch) = char::from_u32(keysym) else {
            return format!("0x{keysym:x}");
        };
        return match ch {
            '`' => "grave".to_owned(),
            ' ' => "Space".to_owned(),
            other => other.to_ascii_uppercase().to_string(),
        };
    }
    match keysym {
        0xff51 => "Left".to_owned(),
        0xff52 => "Up".to_owned(),
        0xff53 => "Right".to_owned(),
        0xff54 => "Down".to_owned(),
        0xff09 => "Tab".to_owned(),
        0xff0d => "Return".to_owned(),
        0xff1b => "Escape".to_owned(),
        0xff61 => "Print".to_owned(),
        0xff08 => "Backspace".to_owned(),
        0xffff => "Delete".to_owned(),
        0xff50 => "Home".to_owned(),
        0xff57 => "End".to_owned(),
        0xff55 => "Page-Up".to_owned(),
        0xff56 => "Page-Down".to_owned(),
        0xffbe..=0xffc9 => format!("F{}", keysym - 0xffbe + 1),
        other => format!("0x{other:x}"),
    }
}

/// Zestaw skrótów (każda akcja ma dokładnie jeden binding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSettings {
    bindings: Vec<(ShortcutAction, Binding)>,
    profile: ShortcutProfile,
}

/// Nazwany zestaw skrótów.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutProfile {
    /// Wbudowany układ Luny (Super + …).
    Luna,
    /// Ręcznie zmienione kombinacje.
    Custom,
}

impl ShortcutProfile {
    /// Profile dostępne w Ustawieniach.
    pub const SELECTABLE: [ShortcutProfile; 1] = [ShortcutProfile::Luna];

    pub fn id(self) -> &'static str {
        match self {
            ShortcutProfile::Luna => "luna",
            ShortcutProfile::Custom => "custom",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "luna" => Some(ShortcutProfile::Luna),
            "custom" => Some(ShortcutProfile::Custom),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            ShortcutProfile::Luna => "Luna",
            ShortcutProfile::Custom => "Custom",
        }
    }

    /// Gotowy zestaw kombinacji profilu (`None` dla „Custom”).
    pub fn preset(self) -> Option<Vec<(ShortcutAction, Binding)>> {
        let table: &[(&str, &str)] = match self {
            ShortcutProfile::Luna => &[
                ("close-window", "Super+Q"),
                ("minimize", "Super+M"),
                ("maximize", "Super+Up"),
                ("restore-window", "Super+Down"),
                ("fullscreen", "Super+F"),
                ("snap-left", "Super+Left"),
                ("snap-right", "Super+Right"),
                ("overview", "Super+Tab"),
                ("cycle-windows", "Super+grave"),
                ("workspace-1", "Super+1"),
                ("workspace-2", "Super+2"),
                ("workspace-3", "Super+3"),
                ("workspace-4", "Super+4"),
                ("workspace-5", "Super+5"),
                ("workspace-6", "Super+6"),
                ("workspace-7", "Super+7"),
                ("workspace-8", "Super+8"),
                ("screenshot", "Super+F12"),
                ("logout", "Super+L"),
            ],
            ShortcutProfile::Custom => return None,
        };
        let mut bindings = Vec::with_capacity(table.len());
        for (action_id, text) in table {
            let action = ShortcutAction::from_id(action_id)?;
            let binding = Binding::parse(text)?;
            bindings.push((action, binding));
        }
        Some(bindings)
    }
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        let mut settings = Self {
            bindings: ShortcutProfile::Luna.preset().unwrap_or_default(),
            profile: ShortcutProfile::Luna,
        };
        settings.sort();
        settings
    }
}

impl ShortcutSettings {
    /// Bieżąca kombinacja akcji.
    pub fn binding(&self, action: ShortcutAction) -> Option<Binding> {
        self.bindings
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, binding)| *binding)
    }

    /// Aktywny profil (albo „Custom” po ręcznej zmianie).
    pub fn profile(&self) -> ShortcutProfile {
        self.profile
    }

    /// Zastępuje wszystkie kombinacje presetem profilu.
    pub fn apply_profile(&mut self, profile: ShortcutProfile) {
        let Some(bindings) = profile.preset() else {
            return;
        };
        self.bindings = bindings;
        self.profile = profile;
        self.sort();
    }

    /// Ustawia kombinację; każda pojedyncza zmiana oznacza profil „Custom”.
    pub fn set(&mut self, action: ShortcutAction, binding: Binding) {
        self.put(action, binding);
        self.profile = ShortcutProfile::Custom;
    }

    fn put(&mut self, action: ShortcutAction, binding: Binding) {
        self.bindings.retain(|(candidate, _)| *candidate != action);
        self.bindings.push((action, binding));
        self.sort();
    }

    /// Przypisuje kombinację, **zamieniając** ją z akcją, która jej używała.
    ///
    /// Dzięki temu żadna akcja nie traci skrótu (i nie ma cichego konfliktu).
    pub fn assign(&mut self, action: ShortcutAction, binding: Binding) {
        let previous = self.binding(action);
        if let Some(other) = self.conflicts_with(binding, action) {
            if let Some(previous) = previous {
                self.put(other, previous);
            }
        }
        self.put(action, binding);
        self.profile = ShortcutProfile::Custom;
    }

    /// Akcja, która już używa tej kombinacji (do ostrzeżenia w UI).
    pub fn conflicts_with(
        &self,
        binding: Binding,
        except: ShortcutAction,
    ) -> Option<ShortcutAction> {
        self.bindings
            .iter()
            .find(|(action, candidate)| *action != except && *candidate == binding)
            .map(|(action, _)| *action)
    }

    /// Wczytuje konfigurację; brak pliku = wartości domyślne.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// Parsuje plik `profil` + `akcja = kombinacja` (nieznane wpisy pomija).
    pub fn parse(text: &str) -> Self {
        let mut profile = None;
        let mut has_bindings = false;
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "profile" {
                profile = ShortcutProfile::from_id(value);
                continue;
            }
            let Some(action) = ShortcutAction::from_id(key) else {
                continue;
            };
            let Some(binding) = Binding::parse(value) else {
                continue;
            };
            entries.push((action, binding));
            has_bindings = true;
        }

        let mut settings = Self::default();
        match profile {
            Some(ShortcutProfile::Custom) => {
                settings.profile = ShortcutProfile::Custom;
            }
            Some(profile) => settings.apply_profile(profile),
            // Stary plik bez nagłówka profilu: jeśli ma własne wpisy, to „Custom”.
            None if has_bindings => settings.profile = ShortcutProfile::Custom,
            None => {}
        }
        for (action, binding) in entries {
            settings.put(action, binding);
        }
        // Profil z nagłówka zostaje zachowany (wpisy to jego konkretyzacja).
        if let Some(profile) = profile {
            settings.profile = profile;
        }
        settings
    }

    pub fn to_config(&self) -> String {
        let mut out = format!(
            "# Skróty klawiszowe Luny (profil + akcja = kombinacja)\nprofile = {}\n",
            self.profile.id()
        );
        for (action, binding) in &self.bindings {
            out.push_str(&format!("{} = {}\n", action.id(), binding.format()));
        }
        out
    }

    pub fn config_path() -> Option<PathBuf> {
        Some(config_dir()?.join("shortcuts.conf"))
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "brak katalogu konfiguracji",
            ));
        };
        save_atomic(&path, &self.to_config())
    }

    fn sort(&mut self) {
        self.bindings.sort_by_key(|(action, _)| {
            ShortcutAction::ALL
                .iter()
                .position(|candidate| candidate == action)
                .unwrap_or(usize::MAX)
        });
    }
}

struct Cached {
    mtime: Option<SystemTime>,
    settings: ShortcutSettings,
}

/// Ustawienia z cache po mtime — Selene woła to przy każdym klawiszu.
pub fn active() -> ShortcutSettings {
    static CACHE: OnceLock<Mutex<Cached>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| {
        Mutex::new(Cached {
            mtime: None,
            settings: ShortcutSettings::default(),
        })
    });
    let mtime = ShortcutSettings::config_path()
        .and_then(|path| std::fs::metadata(path).ok())
        .and_then(|meta| meta.modified().ok());
    let mut guard = match cache.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if guard.mtime != mtime {
        guard.settings = ShortcutSettings::load();
        guard.mtime = mtime;
    }
    guard.settings.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_every_editable_action() {
        let settings = ShortcutSettings::default();
        for action in ShortcutAction::ALL {
            assert!(settings.binding(action).is_some(), "{action:?}");
        }
        assert_eq!(
            settings.binding(ShortcutAction::CloseWindow),
            Binding::parse("Super+Q")
        );
    }

    #[test]
    fn parse_and_format_roundtrip() {
        for text in [
            "Super+Q",
            "Ctrl+Alt+T",
            "Super+Shift+Left",
            "Super+grave",
            "Super+F12",
            "Super+Tab",
        ] {
            let binding = Binding::parse(text).expect(text);
            assert_eq!(Binding::parse(&binding.format()), Some(binding), "{text}");
        }
    }

    #[test]
    fn matches_ignores_letter_case() {
        let binding = Binding::parse("Super+q").unwrap();
        assert!(binding.matches(true, false, false, false, 0x0071));
        assert!(binding.matches(true, false, false, false, 0x0051));
        assert!(!binding.matches(false, false, false, false, 0x0071));
    }

    #[test]
    fn config_roundtrip_and_conflict_detection() {
        let mut settings = ShortcutSettings::default();
        let new = Binding::parse("Super+Shift+K").unwrap();
        settings.set(ShortcutAction::CloseWindow, new);
        let parsed = ShortcutSettings::parse(&settings.to_config());
        assert_eq!(parsed.binding(ShortcutAction::CloseWindow), Some(new));
        let other = Binding::parse("Super+M").unwrap();
        assert_eq!(
            settings.conflicts_with(other, ShortcutAction::CloseWindow),
            Some(ShortcutAction::Minimize)
        );
    }

    #[test]
    fn assign_swaps_with_the_conflicting_action() {
        let mut settings = ShortcutSettings::default();
        let minimize = settings.binding(ShortcutAction::Minimize).unwrap();
        let close = settings.binding(ShortcutAction::CloseWindow).unwrap();
        settings.assign(ShortcutAction::CloseWindow, minimize);
        assert_eq!(
            settings.binding(ShortcutAction::CloseWindow),
            Some(minimize)
        );
        assert_eq!(settings.binding(ShortcutAction::Minimize), Some(close));
    }

    #[test]
    fn presets_cover_every_action_without_duplicates() {
        for profile in ShortcutProfile::SELECTABLE {
            let preset = profile.preset().expect("preset");
            assert_eq!(preset.len(), ShortcutAction::ALL.len(), "{profile:?}");
            for action in ShortcutAction::ALL {
                assert!(
                    preset.iter().any(|(candidate, _)| *candidate == action),
                    "brak {action:?} w {profile:?}"
                );
            }
            for (index, (action, binding)) in preset.iter().enumerate() {
                for (other, candidate) in preset.iter().skip(index + 1) {
                    assert_ne!(
                        binding, candidate,
                        "konflikt {action:?}/{other:?} w profilu {profile:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn profile_roundtrips_through_config() {
        let mut settings = ShortcutSettings::default();
        settings.apply_profile(ShortcutProfile::Luna);
        let parsed = ShortcutSettings::parse(&settings.to_config());
        assert_eq!(parsed.profile(), ShortcutProfile::Luna);
        assert_eq!(parsed, settings);
    }

    #[test]
    fn manual_change_switches_to_custom() {
        let mut settings = ShortcutSettings::default();
        let binding = Binding::parse("Super+Shift+W").unwrap();
        settings.assign(ShortcutAction::CloseWindow, binding);
        assert_eq!(settings.profile(), ShortcutProfile::Custom);
    }

    #[test]
    fn unknown_actions_are_ignored() {
        let settings = ShortcutSettings::parse("nonsense = Super+X\nclose-window = Super+W\n");
        assert_eq!(
            settings.binding(ShortcutAction::CloseWindow),
            Binding::parse("Super+W")
        );
        assert_eq!(
            settings.bindings.len(),
            ShortcutSettings::default().bindings.len()
        );
    }
}
