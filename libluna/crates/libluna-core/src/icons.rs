//! Styl ikon Luny (`~/.config/luna/icons.conf`).
//!
//! Ikony wbudowane libLuny są wektorowe (`IconName`), więc „styl” nie podmienia
//! plików — zmienia sposób ich prezentacji. `Luna` rysuje pastelowe, wielokolorowe
//! ilustracje (inny odcień dla plików, terminala, ustawień, multimediów…),
//! `Monochrome` rysuje je kolorem tekstu (spokojniejszy, jednolity wygląd).
//!
//! Dodatkowo można wybrać **zewnętrzny zestaw ikon** w formacie freedesktop
//! (`/usr/share/icons`, `~/.local/share/icons`, `~/.icons`, `XDG_DATA_DIRS/icons`).
//! Taki zestaw jest globalny: launcher, taskbar, pliki i Ustawienia rysują z niego
//! ikony aplikacji/miejsc. Gdy wybrany zestaw nie ma danego wpisu, szukamy
//! oryginalnej ikony PNG/SVG w systemowych motywach (`hicolor`, potem pozostałe),
//! a dopiero na końcu zostaje zestaw wektorowy.

use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use crate::config::{config_file, parse_pairs, save_atomic};

/// Sposób prezentacji wbudowanych ikon wektorowych.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconTheme {
    /// Pastelowe, wielokolorowe ikony (domyślnie).
    #[default]
    Luna,
    /// Ikony monochromatyczne, w kolorze tekstu.
    Monochrome,
}

impl IconTheme {
    pub const ALL: [IconTheme; 2] = [IconTheme::Luna, IconTheme::Monochrome];

    pub const fn name(self) -> &'static str {
        match self {
            IconTheme::Luna => "luna",
            IconTheme::Monochrome => "monochrome",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.name() == name)
    }

    pub const fn is_monochrome(self) -> bool {
        matches!(self, IconTheme::Monochrome)
    }
}

/// Zewnętrzny zestaw ikon freedesktop znaleziony na dysku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSet {
    /// Nazwa katalogu, np. `Papirus`.
    pub name: String,
    /// Katalog zestawu (zawiera `index.theme`).
    pub path: PathBuf,
    /// `Comment` z `index.theme` (jeśli jest).
    pub comment: Option<String>,
}

impl IconSet {
    /// Szuka pliku PNG dla jednej z nazw ikon freedesktop.
    ///
    /// Obsługuje typowe układy: `<motyw>/<rozmiar>/<kategoria>/<nazwa>.png`
    /// oraz `<motyw>/<kategoria>/<rozmiar>/<nazwa>.png`. Ikony dostępne tylko
    /// jako SVG znajdzie [`IconSet::find_svg`], a narysuje `libluna-render`.
    pub fn find_png(&self, candidates: &[&str]) -> Option<PathBuf> {
        self.find_png_sized(candidates, 256.0)
    }

    /// Jak [`IconSet::find_png`], ale wybiera rozmiar najbliższy `target_px`.
    ///
    /// Najpierw szuka pliku **nie mniejszego** niż cel (dobrej jakości
    /// downscaling), potem większych zapasów, a na końcu mniejszych. Dzięki
    /// temu ikona 18–40 px nie jest skalowana z 256 px.
    pub fn find_png_sized(&self, candidates: &[&str], target_px: f64) -> Option<PathBuf> {
        const SIZES: [u32; 9] = [16, 24, 32, 48, 64, 96, 128, 256, 512];
        const CATEGORIES: [&str; 8] = [
            "apps",
            "places",
            "devices",
            "mimetypes",
            "actions",
            "status",
            "categories",
            "emblems",
        ];
        let target = target_px.max(1.0).ceil() as u32;
        let mut order: Vec<u32> = SIZES
            .iter()
            .copied()
            .filter(|size| *size >= target)
            .chain(SIZES.iter().rev().copied())
            .collect();
        // Starsze zestawy używają też 22 px.
        order.push(22);

        for size in &order {
            let dir = format!("{size}x{size}");
            for category in CATEGORIES {
                for name in candidates {
                    let path = self
                        .path
                        .join(&dir)
                        .join(category)
                        .join(format!("{name}.png"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        for category in CATEGORIES {
            for size in &order {
                for name in candidates {
                    let path = self
                        .path
                        .join(category)
                        .join(size.to_string())
                        .join(format!("{name}.png"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    /// Szuka pliku SVG dla jednej z nazw ikon freedesktop.
    ///
    /// Obsługuje zarówno nowoczesny układ `scalable/<kategoria>/<nazwa>.svg`
    /// (hicolor, Adwaita), jak i zestawy trzymające SVG w katalogach rozmiarów
    /// (Papirus: `<rozmiar>/apps/<nazwa>.svg`).
    pub fn find_svg(&self, candidates: &[&str]) -> Option<PathBuf> {
        const SIZES: [u32; 9] = [16, 22, 24, 32, 48, 64, 128, 256, 512];
        const CATEGORIES: [&str; 8] = [
            "apps",
            "places",
            "devices",
            "mimetypes",
            "actions",
            "status",
            "categories",
            "emblems",
        ];

        for category in CATEGORIES {
            for name in candidates {
                for dir in [
                    self.path.join("scalable").join(category),
                    self.path.join(category).join("scalable"),
                ] {
                    let path = dir.join(format!("{name}.svg"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        for size in SIZES {
            let dir = format!("{size}x{size}");
            for category in CATEGORIES {
                for name in candidates {
                    let path = self
                        .path
                        .join(&dir)
                        .join(category)
                        .join(format!("{name}.svg"));
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
        None
    }
}

/// Domyślny zestaw ikon dołączany do Luny (`assets/icons/Luna-Icons`).
pub const DEFAULT_ICON_SET: &str = "Luna-Icons";

/// Katalogi, w których szukamy zestawów ikon (kolejność = priorytet).
pub fn icon_search_dirs() -> Vec<PathBuf> {
    let mut dirs = bundled_icon_dirs();
    if let Some(data) = std::env::var_os("XDG_DATA_HOME").filter(|value| !value.is_empty()) {
        dirs.push(PathBuf::from(data).join("icons"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/icons"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(".icons"));
    }
    let data_dirs = std::env::var_os("XDG_DATA_DIRS")
        .filter(|value| !value.is_empty())
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });
    for dir in data_dirs {
        dirs.push(dir.join("icons"));
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Katalogi zestawów dołączonych do projektu/instalacji.
///
/// Dzięki temu `Luna-Icons` działa zarówno po `scripts/install.sh`
/// (`<prefix>/share/icons`), jak i bezpośrednio z drzewa źródeł
/// (`<repo>/assets/icons` obok wspólnego `target/`).
fn bundled_icon_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("LUNA_ICONS_DIR").filter(|value| !value.is_empty()) {
        dirs.push(PathBuf::from(dir));
    }
    let Ok(exe) = std::env::current_exe() else {
        return dirs;
    };
    let Some(bin) = exe.parent() else {
        return dirs;
    };
    for up in ["..", "../..", "../../..", "../../../..", "../../../../.."] {
        for leaf in ["share/icons", "assets/icons"] {
            let path = bin.join(up).join(leaf);
            if path.is_dir() {
                dirs.push(path);
            }
        }
    }
    dirs
}

/// Wykrywa zainstalowane zewnętrzne zestawy ikon.
pub fn external_icon_sets() -> Vec<IconSet> {
    let mut found: Vec<IconSet> = Vec::new();
    for root in icon_search_dirs() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // `hicolor` to baza fallbacków, a nie wybieralny motyw.
            if name.eq_ignore_ascii_case("hicolor") {
                continue;
            }
            let index = path.join("index.theme");
            if !index.is_file() || !has_icon_layout(&path) {
                continue;
            }
            if found.iter().any(|set| set.name == name) {
                continue;
            }
            let comment = std::fs::read_to_string(&index)
                .ok()
                .and_then(|text| parse_index_theme(&text, "Comment"));
            found.push(IconSet {
                name: name.to_owned(),
                path,
                comment,
            });
        }
    }
    found.sort_by_cached_key(|set| set.name.to_lowercase());
    found
}

/// Nazwa zestawu z `index.theme` (albo nazwa katalogu).
pub fn icon_set_display_name(set: &IconSet) -> String {
    std::fs::read_to_string(set.path.join("index.theme"))
        .ok()
        .and_then(|text| parse_index_theme(&text, "Name"))
        .unwrap_or_else(|| set.name.clone())
}

/// Czy katalog wygląda na zestaw ikon (a nie np. motyw kursora)?
fn has_icon_layout(path: &Path) -> bool {
    const SIZE_DIRS: [&str; 10] = [
        "16x16", "22x22", "24x24", "32x32", "48x48", "64x64", "96x96", "128x128", "256x256",
        "scalable",
    ];
    if SIZE_DIRS.iter().any(|dir| path.join(dir).is_dir()) {
        return true;
    }
    [
        "apps",
        "places",
        "devices",
        "mimetypes",
        "categories",
        "emblems",
    ]
    .iter()
    .any(|dir| path.join(dir).is_dir())
}

fn parse_index_theme(text: &str, key: &str) -> Option<String> {
    let mut in_theme = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_theme = line.eq_ignore_ascii_case("[Icon Theme]");
            continue;
        }
        if !in_theme {
            continue;
        }
        let Some((k, value)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Znajduje zestaw o podanej nazwie katalogu.
pub fn find_icon_set(name: &str) -> Option<IconSet> {
    external_icon_sets()
        .into_iter()
        .find(|set| set.name == name)
}

static ACTIVE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

fn active_lock() -> &'static RwLock<Option<String>> {
    ACTIVE.get_or_init(|| RwLock::new(None))
}

/// Ustawia globalny zewnętrzny zestaw ikon (`None` = tylko wektorowe).
///
/// Wywołują to Ustawienia po zapisie oraz `libluna-platform` po wykryciu zmiany
/// `icons.conf`, żeby wszystkie aplikacje na libLunie rysowały ten sam zestaw.
pub fn set_active_icon_set(name: Option<String>) {
    if let Ok(mut active) = active_lock().write() {
        *active = name;
    }
}

/// Aktualnie wybrany zewnętrzny zestaw ikon (nazwa katalogu).
pub fn active_icon_set() -> Option<String> {
    active_lock().read().ok().and_then(|active| active.clone())
}

static ACTIVE_THEME: OnceLock<RwLock<IconTheme>> = OnceLock::new();

fn active_theme_lock() -> &'static RwLock<IconTheme> {
    ACTIVE_THEME.get_or_init(|| RwLock::new(IconTheme::default()))
}

/// Ustawia globalny styl wbudowanych ikon wektorowych.
///
/// Wywołują to Ustawienia po zapisie oraz `libluna-platform` po wykryciu zmiany
/// `icons.conf`, żeby pastelowe ikony (albo tryb monochromatyczny) obowiązywały
/// we wszystkich aplikacjach libLuny.
pub fn set_active_icon_theme(theme: IconTheme) {
    if let Ok(mut active) = active_theme_lock().write() {
        *active = theme;
    }
}

/// Aktualny globalny styl wbudowanych ikon wektorowych.
pub fn active_icon_theme() -> IconTheme {
    active_theme_lock()
        .read()
        .map(|theme| *theme)
        .unwrap_or_default()
}

/// Ustawienia ikon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconSettings {
    pub theme: IconTheme,
    /// Nazwa katalogu zewnętrznego zestawu (np. `Papirus`).
    pub external: Option<String>,
    /// Motyw ikon GTK sprzed wybrania zestawu zewnętrznego — przywracany po wyłączeniu.
    pub previous_gtk_theme: Option<String>,
}

impl Default for IconSettings {
    fn default() -> Self {
        Self {
            theme: IconTheme::Luna,
            external: Some(DEFAULT_ICON_SET.to_owned()),
            previous_gtk_theme: None,
        }
    }
}

impl IconSettings {
    pub fn config_path() -> Option<PathBuf> {
        config_file("icons.conf")
    }

    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for (key, value) in parse_pairs(text) {
            match key.as_str() {
                "theme" => {
                    if let Some(theme) = IconTheme::from_name(&value) {
                        out.theme = theme;
                    }
                }
                "icon_set" => {
                    let value = value.trim();
                    out.external = if value.is_empty() || value == "none" || value == "luna" {
                        None
                    } else {
                        Some(value.to_owned())
                    };
                }
                "gtk_previous" => {
                    let value = value.trim();
                    if !value.is_empty() {
                        out.previous_gtk_theme = Some(value.to_owned());
                    }
                }
                _ => {}
            }
        }
        out
    }

    pub fn render(&self) -> String {
        let mut out = format!(
            "# Luna — styl ikon\n# luna | monochrome\ntheme = {}\n",
            self.theme.name()
        );
        // `icon_set` zapisujemy zawsze: `none` oznacza świadomy powrót do ikon
        // wektorowych, a brak klucza w starym pliku → domyślny `Luna-Icons`.
        match &self.external {
            Some(external) => out.push_str(&format!("icon_set = {external}\n")),
            None => out.push_str("icon_set = none\n"),
        }
        if let Some(previous) = &self.previous_gtk_theme {
            out.push_str(&format!("gtk_previous = {previous}\n"));
        }
        out
    }

    /// Ustawia zewnętrzny zestaw i propaguje go do aplikacji GTK/Qt/KDE.
    ///
    /// Przy pierwszym wyborze zapamiętuje poprzedni motyw GTK, a przy powrocie do
    /// ikon Luny go przywraca — tak jak zmiana motywu w GNOME/KDE.
    pub fn set_external(&mut self, name: Option<String>) {
        if self.external == name {
            return;
        }
        match name {
            Some(name) => {
                if self.previous_gtk_theme.is_none() {
                    self.previous_gtk_theme =
                        Some(gtk_icon_theme().unwrap_or_else(|| "Adwaita".to_owned()));
                }
                if find_icon_set(&name).is_some() {
                    apply_icon_theme_globally(&name);
                }
                self.external = Some(name);
            }
            None => {
                self.external = None;
                let previous = self
                    .previous_gtk_theme
                    .take()
                    .unwrap_or_else(|| "Adwaita".to_owned());
                apply_icon_theme_globally(&previous);
            }
        }
    }

    /// Ponownie stosuje bieżący wybór (np. po nieudanym zapisie).
    pub fn apply_current_global(&self) {
        if let Some(name) = &self.external {
            if find_icon_set(name).is_some() {
                apply_icon_theme_globally(name);
            }
        } else if let Some(previous) = &self.previous_gtk_theme {
            apply_icon_theme_globally(previous);
        }
    }

    pub fn save_local(&self, path: &Path) -> std::io::Result<()> {
        save_atomic(path, &self.render())
    }

    pub fn save(&self) -> std::io::Result<()> {
        match Self::config_path() {
            Some(path) => self.save_local(&path),
            None => Ok(()),
        }
    }
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
}

/// Odczytuje wartość z prostego pliku INI (`[sekcja]`, `klucz=wartość`).
fn ini_value(path: &Path, group: &str, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line[1..line.len() - 1].eq_ignore_ascii_case(group);
            continue;
        }
        if !in_group {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(key) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// Ustawia wartość w pliku INI, zachowując pozostałe sekcje i klucze.
fn set_ini_value(path: &Path, group: &str, key: &str, value: &str) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines: Vec<String> = existing.lines().map(str::to_owned).collect();
    let mut in_group = false;
    let mut found_group = false;
    let mut insert_at = lines.len();
    let mut replaced = false;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_group = trimmed[1..trimmed.len() - 1].eq_ignore_ascii_case(group);
            if in_group {
                found_group = true;
                insert_at = index + 1;
            }
            continue;
        }
        if !in_group {
            continue;
        }
        insert_at = index + 1;
        if let Some((name, _)) = trimmed.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                lines[index] = format!("{key}={value}");
                replaced = true;
                break;
            }
        }
    }
    if !replaced {
        if found_group {
            lines.insert(insert_at, format!("{key}={value}"));
        } else {
            lines.push(format!("[{group}]"));
            lines.push(format!("{key}={value}"));
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, lines.join("\n") + "\n")
}

fn run_quiet(program: &str, args: &[&str]) -> bool {
    std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Motyw ikon używany obecnie przez GTK/GNOME.
pub fn gtk_icon_theme() -> Option<String> {
    let config = config_home()?;
    for version in ["gtk-3.0", "gtk-4.0"] {
        if let Some(theme) = ini_value(
            &config.join(version).join("settings.ini"),
            "Settings",
            "gtk-icon-theme-name",
        ) {
            return Some(theme);
        }
    }
    // GNOME trzyma motyw w dconf — to tylko odczyt, nie zmieniamy nic bez wyboru.
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout);
    let value = value.trim().trim_matches('\'').trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Ustawia motyw ikon dla aplikacji GTK, Qt, GNOME i KDE.
///
/// To jest warstwa „globalna” poza libLuną: nowe aplikacje tych toolkitów
/// odczytają wybór przy starcie (działające trzeba uruchomić ponownie).
pub fn apply_icon_theme_globally(name: &str) {
    if let Some(config) = config_home() {
        for version in ["gtk-3.0", "gtk-4.0"] {
            let _ = set_ini_value(
                &config.join(version).join("settings.ini"),
                "Settings",
                "gtk-icon-theme-name",
                name,
            );
        }
        for (toolkit, file) in [("qt5ct", "qt5ct.conf"), ("qt6ct", "qt6ct.conf")] {
            let _ = set_ini_value(
                &config.join(toolkit).join(file),
                "Appearance",
                "icon_theme",
                name,
            );
        }
    }
    // GNOME/GTK: dconf; KDE: kdeglobals. Brak narzędzia nie jest błędem.
    let _ = run_quiet(
        "gsettings",
        &["set", "org.gnome.desktop.interface", "icon-theme", name],
    );
    for tool in ["kwriteconfig6", "kwriteconfig5"] {
        if run_quiet(
            tool,
            &[
                "--file",
                "kdeglobals",
                "--group",
                "Icons",
                "--key",
                "Theme",
                name,
            ],
        ) {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_renders_icon_theme() {
        assert_eq!(
            IconSettings::parse("theme = monochrome\n").theme,
            IconTheme::Monochrome
        );
        assert_eq!(
            IconSettings::parse(&IconSettings::default().render()).theme,
            IconTheme::Luna
        );
        assert_eq!(
            IconSettings::default().external.as_deref(),
            Some(DEFAULT_ICON_SET)
        );
        assert_eq!(
            IconSettings::parse(&IconSettings::default().render())
                .external
                .as_deref(),
            Some(DEFAULT_ICON_SET)
        );
        assert!(IconSettings::parse("icon_set = none\n").external.is_none());
        assert_eq!(
            IconSettings::parse("theme = nieznany\n").theme,
            IconTheme::Luna
        );
    }

    #[test]
    fn active_icon_theme_roundtrip() {
        let previous = active_icon_theme();
        set_active_icon_theme(IconTheme::Monochrome);
        assert!(active_icon_theme().is_monochrome());
        set_active_icon_theme(IconTheme::Luna);
        assert_eq!(active_icon_theme(), IconTheme::Luna);
        set_active_icon_theme(previous);
    }

    #[test]
    fn parses_external_icon_set() {
        let parsed = IconSettings::parse("theme = luna\nicon_set = Papirus\n");
        assert_eq!(parsed.external.as_deref(), Some("Papirus"));
        assert!(parsed.render().contains("icon_set = Papirus"));
        assert!(IconSettings::parse("icon_set = none\n").external.is_none());
        let vector = IconSettings {
            external: None,
            ..IconSettings::default()
        };
        assert!(vector.render().contains("icon_set = none"));
        assert!(IconSettings::parse(&vector.render()).external.is_none());
    }

    #[test]
    fn remembers_previous_gtk_theme() {
        let parsed = IconSettings::parse("icon_set = Papirus\ngtk_previous = Adwaita\n");
        assert_eq!(parsed.previous_gtk_theme.as_deref(), Some("Adwaita"));
        assert!(parsed.render().contains("gtk_previous = Adwaita"));
    }

    #[test]
    fn index_theme_reads_name_and_comment() {
        let text = "[Icon Theme]\nName=Papirus\nComment=Papirus icon theme\n";
        assert_eq!(parse_index_theme(text, "Name").as_deref(), Some("Papirus"));
        assert_eq!(
            parse_index_theme(text, "Comment").as_deref(),
            Some("Papirus icon theme")
        );
    }

    #[test]
    fn ini_roundtrip_preserves_other_keys() {
        let dir = std::env::temp_dir().join(format!(
            "luna-ini-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("settings.ini");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[Settings]\ngtk-font-name=Sans 10\n").unwrap();
        set_ini_value(&path, "Settings", "gtk-icon-theme-name", "Papirus").unwrap();
        assert_eq!(
            ini_value(&path, "Settings", "gtk-icon-theme-name").as_deref(),
            Some("Papirus")
        );
        assert_eq!(
            ini_value(&path, "Settings", "gtk-font-name").as_deref(),
            Some("Sans 10")
        );
        set_ini_value(&path, "Settings", "gtk-icon-theme-name", "Adwaita").unwrap();
        assert_eq!(
            ini_value(&path, "Settings", "gtk-icon-theme-name").as_deref(),
            Some("Adwaita")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn find_png_uses_freedesktop_layout() {
        let root = std::env::temp_dir().join(format!(
            "luna-icon-set-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let apps = root.join("48x48/apps");
        let small = root.join("32x32/apps");
        let categories = root.join("48x48/categories");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::create_dir_all(&small).unwrap();
        std::fs::create_dir_all(&categories).unwrap();
        std::fs::write(apps.join("firefox.png"), b"").unwrap();
        std::fs::write(small.join("firefox.png"), b"").unwrap();
        std::fs::write(categories.join("preferences-desktop.png"), b"").unwrap();
        let set = IconSet {
            name: "Test".to_owned(),
            path: root.clone(),
            comment: None,
        };
        assert_eq!(
            set.find_png(&["firefox"]).as_deref(),
            Some(apps.join("firefox.png").as_path())
        );
        // Dla 26 px wybieramy 32 px, a nie 48/256 px.
        assert_eq!(
            set.find_png_sized(&["firefox"], 26.0).as_deref(),
            Some(small.join("firefox.png").as_path())
        );
        assert_eq!(
            set.find_png(&["preferences-desktop"]).as_deref(),
            Some(categories.join("preferences-desktop.png").as_path())
        );
        assert!(set.find_png(&["missing"]).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn find_svg_supports_scalable_and_papirus_layout() {
        let root = std::env::temp_dir().join(format!(
            "luna-icon-svg-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let scalable = root.join("scalable/apps");
        let papirus = root.join("48x48/apps");
        std::fs::create_dir_all(&scalable).unwrap();
        std::fs::create_dir_all(&papirus).unwrap();
        std::fs::write(scalable.join("org.gnome.Demo.svg"), b"").unwrap();
        std::fs::write(papirus.join("firefox.svg"), b"").unwrap();

        let set = IconSet {
            name: "Test".to_owned(),
            path: root.clone(),
            comment: None,
        };
        assert_eq!(
            set.find_svg(&["org.gnome.Demo"]).as_deref(),
            Some(scalable.join("org.gnome.Demo.svg").as_path())
        );
        assert_eq!(
            set.find_svg(&["firefox"]).as_deref(),
            Some(papirus.join("firefox.svg").as_path())
        );
        assert!(set.find_svg(&["missing"]).is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}
