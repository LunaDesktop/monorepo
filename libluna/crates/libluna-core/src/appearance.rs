//! Preferencje wyglądu: tryb (auto/jasny/ciemny), kolor akcentu oraz ich trwałość.
//!
//! Warstwa [`ThemeSettings`] jest czysta (bez I/O poza zapisem/odczytem pliku) i może
//! być używana zarówno przez platformę, jak i testy. Rozwiązywanie motywu
//! (`resolve`) jest deterministyczne i tanie.

use std::io;
use std::path::PathBuf;

use crate::color::Color;
use crate::style::Theme;

/// Kolor akcentu (paleta zgodna z duchem GNOME 47+).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Accent {
    #[default]
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
    Slate,
}

impl Accent {
    /// Wszystkie kolory akcentu w kolejności prezentacji.
    pub const ALL: [Accent; 9] = [
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Yellow,
        Accent::Orange,
        Accent::Red,
        Accent::Pink,
        Accent::Purple,
        Accent::Slate,
    ];

    /// Stabilna nazwa (do zapisu w konfiguracji).
    pub const fn name(self) -> &'static str {
        match self {
            Accent::Blue => "blue",
            Accent::Teal => "teal",
            Accent::Green => "green",
            Accent::Yellow => "yellow",
            Accent::Orange => "orange",
            Accent::Red => "red",
            Accent::Pink => "pink",
            Accent::Purple => "purple",
            Accent::Slate => "slate",
        }
    }

    pub fn from_name(name: &str) -> Option<Accent> {
        Accent::ALL.into_iter().find(|a| a.name() == name)
    }

    /// Kolor akcentu (jasny wariant — wspólny dla obu motywów).
    pub const fn color(self) -> Color {
        match self {
            Accent::Blue => Color::from_rgba8(0x35, 0x84, 0xe4, 0xff),
            Accent::Teal => Color::from_rgba8(0x21, 0x90, 0xa4, 0xff),
            Accent::Green => Color::from_rgba8(0x3a, 0x94, 0x4a, 0xff),
            Accent::Yellow => Color::from_rgba8(0xc8, 0x88, 0x00, 0xff),
            Accent::Orange => Color::from_rgba8(0xed, 0x5b, 0x00, 0xff),
            Accent::Red => Color::from_rgba8(0xe6, 0x2d, 0x42, 0xff),
            Accent::Pink => Color::from_rgba8(0xd5, 0x61, 0x99, 0xff),
            Accent::Purple => Color::from_rgba8(0x91, 0x41, 0xac, 0xff),
            Accent::Slate => Color::from_rgba8(0x6f, 0x83, 0x96, 0xff),
        }
    }

    /// Kolor tekstu/ikon na akcencie (kontrast).
    pub const fn foreground(self) -> Color {
        match self {
            // Żółty jest zbyt jasny na biały tekst.
            Accent::Yellow => Color::from_rgba8(0x2e, 0x2e, 0x2e, 0xff),
            _ => Color::from_rgba8(0xff, 0xff, 0xff, 0xff),
        }
    }
}

/// Preferencja trybu wyglądu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreference {
    /// Podążaj za ustawieniem systemu.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePreference {
    pub const ALL: [ThemePreference; 3] = [
        ThemePreference::Light,
        ThemePreference::Dark,
        ThemePreference::System,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ThemePreference::System => "system",
            ThemePreference::Light => "light",
            ThemePreference::Dark => "dark",
        }
    }

    pub fn from_name(name: &str) -> Option<ThemePreference> {
        ThemePreference::ALL.into_iter().find(|p| p.name() == name)
    }

    /// Czy przy zadanym stanie systemu wybrać wariant ciemny.
    pub const fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemePreference::System => system_dark,
            ThemePreference::Light => false,
            ThemePreference::Dark => true,
        }
    }

    /// Rozwiązuje preferencję do konkretnego motywu z zadanym akcentem.
    pub fn resolve(self, system_dark: bool, accent: Accent) -> Theme {
        let base = if self.is_dark(system_dark) {
            Theme::dark()
        } else {
            Theme::light()
        };
        base.accented(accent.color(), accent.foreground())
    }
}

/// Trwałe ustawienia wyglądu (tryb + akcent + styl) z flagą „zmienione”.
#[derive(Debug, Clone)]
pub struct ThemeSettings {
    preference: ThemePreference,
    accent: Accent,
    style: Option<String>,
    dirty: bool,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            preference: ThemePreference::System,
            accent: Accent::Blue,
            style: None,
            dirty: false,
        }
    }
}

impl PartialEq for ThemeSettings {
    fn eq(&self, other: &Self) -> bool {
        self.preference == other.preference
            && self.accent == other.accent
            && self.style == other.style
    }
}

impl ThemeSettings {
    pub fn new(preference: ThemePreference, accent: Accent) -> Self {
        Self {
            preference,
            accent,
            style: None,
            dirty: false,
        }
    }

    pub const fn preference(&self) -> ThemePreference {
        self.preference
    }

    pub const fn accent(&self) -> Accent {
        self.accent
    }

    /// Nazwa wybranego stylu (instalowanego z pliku), jeśli ustawiona.
    pub fn style(&self) -> Option<&str> {
        self.style.as_deref()
    }

    pub fn set_preference(&mut self, preference: ThemePreference) {
        if self.preference != preference {
            self.preference = preference;
            self.dirty = true;
        }
    }

    pub fn set_accent(&mut self, accent: Accent) {
        if self.accent != accent {
            self.accent = accent;
            self.dirty = true;
        }
    }

    /// Ustawia styl (instalowany z pliku); `None` = wbudowany wygląd.
    pub fn set_style(&mut self, style: Option<String>) {
        if self.style != style {
            self.style = style;
            self.dirty = true;
        }
    }

    /// Czy coś zmieniono od ostatniego [`clear_dirty`](Self::clear_dirty).
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Oznacza motyw do ponownego rozwiązania (np. po zmianie ustawień dostępności,
    /// które nie są częścią tego pliku).
    pub fn touch(&mut self) {
        self.dirty = true;
    }

    /// Zdejmuje flagę „zmienione” (opcjonalnie zapisuje stan na dysk).
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Rozwiązuje bieżące ustawienia do konkretnego motywu (tryb + akcent).
    pub fn resolve(&self, system_dark: bool) -> Theme {
        let base = if self.preference.is_dark(system_dark) {
            Theme::dark()
        } else {
            Theme::light()
        };
        base.accented(self.accent.color(), self.accent.foreground())
    }

    /// Jak [`resolve`](Self::resolve), ale nakłada też wybrany **styl** z pliku
    /// (`~/.local/share/luna/themes/<nazwa>.theme` itd.). **Bez** ustawień
    /// dostępności — te dokłada [`resolve_themed`](Self::resolve_themed).
    ///
    /// Przydatne aplikacjom, które chcą same nałożyć aktualne ustawienia
    /// dostępności (np. Ustawienia zmieniają je na żywo).
    pub fn resolve_base(&self, system_dark: bool) -> Theme {
        let base = self.resolve(system_dark);
        match self.style.as_deref().and_then(crate::theme_file::load) {
            Some(patch) => patch.build(base),
            None => base,
        }
    }

    /// Jak [`resolve_base`](Self::resolve_base), ale nakłada też ustawienia
    /// dostępności (skala tekstu, wysoki kontrast) z `accessibility.conf`.
    pub fn resolve_themed(&self, system_dark: bool) -> Theme {
        self.resolve_base(system_dark)
            .with_accessibility(&crate::accessibility::AccessibilitySettings::load())
    }

    // --- Trwałość (prosty format `klucz = wartość`) ---

    /// Domyślna ścieżka konfiguracji: `$XDG_CONFIG_HOME/luna/theme.conf`.
    pub fn config_path() -> Option<PathBuf> {
        Some(config_dir()?.join("luna").join("theme.conf"))
    }

    /// Wczytuje ustawienia z domyślnej ścieżki (po cichu zwraca domyślne przy błędzie).
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::from_config_str(&s))
            .unwrap_or_default()
    }

    /// Parsuje konfigurację z tekstu (nieznane klucze są ignorowane).
    pub fn from_config_str(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "preference" => {
                    if let Some(p) = ThemePreference::from_name(value) {
                        settings.preference = p;
                    }
                }
                "accent" => {
                    if let Some(a) = Accent::from_name(value) {
                        settings.accent = a;
                    }
                }
                "style" => {
                    settings.style = if value.is_empty() {
                        None
                    } else {
                        Some(value.to_owned())
                    };
                }
                _ => {}
            }
        }
        settings.dirty = false;
        settings
    }

    /// Serializuje ustawienia do tekstu konfiguracji.
    pub fn to_config_string(&self) -> String {
        let mut out = format!(
            "preference = {}\naccent = {}\n",
            self.preference.name(),
            self.accent.name()
        );
        if let Some(style) = &self.style {
            out.push_str(&format!("style = {style}\n"));
        }
        out
    }

    /// Zapisuje ustawienia do domyślnej ścieżki (tworzy katalogi).
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_config_string())
    }
}

/// Katalog konfiguracji użytkownika (`$XDG_CONFIG_HOME` lub `~/.config`).
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

/// Plik ze stanem systemowego motywu (`1` = ciemny, `0` = jasny).
///
/// Zapisuje go powłoka na podstawie portalu `org.freedesktop.appearance.color-scheme`.
pub fn system_dark_path() -> Option<PathBuf> {
    Some(config_dir()?.join("luna").join("system-dark"))
}

/// Czy system preferuje ciemny motyw.
///
/// Kolejność: jawny override `LUNA_SYSTEM_DARK` → plik zapisany przez powłokę → jasny.
pub fn system_dark() -> bool {
    if let Ok(v) = std::env::var("LUNA_SYSTEM_DARK") {
        return !v.is_empty() && v != "0";
    }
    system_dark_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}

/// Zapisuje stan systemowego motywu (wywołuje watcher portalu w powłoce).
pub fn set_system_dark(dark: bool) -> io::Result<()> {
    let Some(path) = system_dark_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, if dark { "1" } else { "0" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_is_dark_semantics() {
        assert!(ThemePreference::Dark.is_dark(false));
        assert!(!ThemePreference::Light.is_dark(true));
        assert!(ThemePreference::System.is_dark(true));
        assert!(!ThemePreference::System.is_dark(false));
    }

    #[test]
    fn resolve_applies_accent() {
        let t = ThemePreference::Light.resolve(false, Accent::Purple);
        assert_eq!(t.colors.accent, Accent::Purple.color());
        assert_ne!(t.colors.accent, Theme::light().colors.accent);
    }

    #[test]
    fn accent_roundtrip_names() {
        for a in Accent::ALL {
            assert_eq!(Accent::from_name(a.name()), Some(a));
        }
        for p in ThemePreference::ALL {
            assert_eq!(ThemePreference::from_name(p.name()), Some(p));
        }
    }

    #[test]
    fn settings_config_roundtrip() {
        let mut s = ThemeSettings::default();
        s.set_preference(ThemePreference::Dark);
        s.set_accent(Accent::Teal);
        s.set_style(Some("nord".to_owned()));
        assert!(s.is_dirty());
        let parsed = ThemeSettings::from_config_str(&s.to_config_string());
        assert_eq!(parsed, s);
        assert_eq!(parsed.style(), Some("nord"));
        assert!(!parsed.is_dirty());
    }

    #[test]
    fn dirty_only_on_change() {
        let mut s = ThemeSettings::default();
        s.clear_dirty();
        s.set_preference(s.preference());
        assert!(!s.is_dirty());
        s.set_preference(ThemePreference::Dark);
        assert!(s.is_dirty());
    }
}
