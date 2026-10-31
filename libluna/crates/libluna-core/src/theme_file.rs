//! Style instalowane z plików (`*.theme`) — tokeny motywu bez rekompilacji.
//!
//! Format jest prosty i liniowy (`klucz = wartość`, `#` to komentarz), w duchu plików
//! językowych `libluna-i18n`. Klucze są **nazwane kropkowo**, np. `colors.text`,
//! `spacing.md`, `radius.small`, `typography.body.size`, `metrics.button.height`,
//! a `__name__` to nazwa wyświetlana stylu. Nieznane klucze są ignorowane (zgodność wprzód).
//!
//! Katalogi wyszukiwania (kolejność = priorytet):
//! `~/.config/luna/themes`, `~/.local/share/luna/themes`, `/usr/share/luna/themes`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::appearance::Accent;
use crate::color::Color;
use crate::style::{ColorScheme, Theme};

/// Nadpisania tokenów motywu wczytane z pliku.
#[derive(Debug, Clone, Default)]
pub struct ThemePatch {
    name: Option<String>,
    entries: BTreeMap<String, String>,
}

impl ThemePatch {
    /// Parsuje styl z tekstu (nieznane klucze są zachowane, ale ignorowane przy nakładaniu).
    pub fn parse(src: &str) -> Self {
        let mut patch = ThemePatch::default();
        for line in src.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            if key == "__name__" {
                patch.name = Some(value.to_owned());
            } else {
                patch.entries.insert(key.to_owned(), value.to_owned());
            }
        }
        patch
    }

    /// Wczytuje styl z pliku.
    pub fn from_file(path: &Path) -> io::Result<ThemePatch> {
        let src = std::fs::read_to_string(path)?;
        Ok(ThemePatch::parse(&src))
    }

    /// Nazwa wyświetlana stylu (`__name__`), jeśli podana.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Pary `klucz → wartość` (kolejność posortowana).
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    fn appearance(&self) -> Option<&str> {
        self.entries.get("appearance").map(String::as_str)
    }

    /// Buduje motyw: bazą jest wariant z `appearance` (jeśli podany), w przeciwnym razie
    /// `fallback`. Akcent z `fallback` jest zachowany, o ile styl go nie nadpisuje.
    pub fn build(&self, fallback: Theme) -> Theme {
        let mut theme = match self.appearance() {
            Some("dark") => Theme::dark(),
            Some("light") => Theme::light(),
            _ => {
                let mut theme = fallback;
                self.apply(&mut theme);
                return theme;
            }
        };
        // Zachowaj wybrany przez użytkownika akcent, chyba że styl go nadpisze.
        theme.colors.accent = fallback.colors.accent;
        theme.colors.accent_fg = fallback.colors.accent_fg;
        self.apply(&mut theme);
        theme
    }

    /// Nakłada nadpisania na istniejący motyw (nieznane klucze są ignorowane).
    pub fn apply(&self, theme: &mut Theme) {
        for (key, value) in &self.entries {
            apply_entry(theme, key, value);
        }
    }
}

fn apply_entry(theme: &mut Theme, key: &str, value: &str) {
    if let Some(field) = key.strip_prefix("colors.") {
        if let Some(color) = Color::from_hex(value) {
            set_color(&mut theme.colors, field, color);
        }
        return;
    }
    if let Some(field) = key.strip_prefix("syntax.") {
        if let Some(color) = Color::from_hex(value) {
            set_syntax_color(&mut theme.syntax, field, color);
        }
        return;
    }
    macro_rules! set_f64 {
        ($target:expr) => {
            if let Ok(n) = value.parse::<f64>() {
                $target = n;
            }
        };
    }
    match key {
        "spacing.xs" => set_f64!(theme.spacing.xs),
        "spacing.sm" => set_f64!(theme.spacing.sm),
        "spacing.md" => set_f64!(theme.spacing.md),
        "spacing.lg" => set_f64!(theme.spacing.lg),
        "spacing.xl" => set_f64!(theme.spacing.xl),
        "radius.small" => set_f64!(theme.radius.small),
        "radius.medium" => set_f64!(theme.radius.medium),
        "radius.large" => set_f64!(theme.radius.large),
        "typography.title.size" => set_f64!(theme.typography.title.size),
        "typography.heading.size" => set_f64!(theme.typography.heading.size),
        "typography.body.size" => set_f64!(theme.typography.body.size),
        "typography.caption.size" => set_f64!(theme.typography.caption.size),
        "metrics.headerbar.height" => set_f64!(theme.metrics.headerbar.height),
        "metrics.headerbar.control" => set_f64!(theme.metrics.headerbar.control),
        "metrics.button.height" => set_f64!(theme.metrics.button.height),
        "metrics.switch.width" => set_f64!(theme.metrics.switch.width),
        "metrics.switch.height" => set_f64!(theme.metrics.switch.height),
        "metrics.slider.track_height" => set_f64!(theme.metrics.slider.track_height),
        "metrics.tabs.height" => set_f64!(theme.metrics.tabs.height),
        "metrics.entry.height" => set_f64!(theme.metrics.entry.height),
        "metrics.menu.item_h" => set_f64!(theme.metrics.menu.item_h),
        "metrics.card.header" => set_f64!(theme.metrics.card.header),
        "metrics.itemview.row_h" => set_f64!(theme.metrics.itemview.row_h),
        "accent" => {
            if let Some(a) = Accent::from_name(value) {
                theme.colors.accent = a.color();
                theme.colors.accent_fg = a.foreground();
            }
        }
        _ => {}
    }
}

fn set_color(colors: &mut ColorScheme, field: &str, color: Color) {
    match field {
        "window_bg" => colors.window_bg = color,
        "view_bg" => colors.view_bg = color,
        "card_bg" => colors.card_bg = color,
        "headerbar_bg" => colors.headerbar_bg = color,
        "sidebar_bg" => colors.sidebar_bg = color,
        "text" => colors.text = color,
        "dim_text" => colors.dim_text = color,
        "accent" => colors.accent = color,
        "accent_fg" => colors.accent_fg = color,
        "border" => colors.border = color,
        "shade" => colors.shade = color,
        "error" => colors.error = color,
        "warning" => colors.warning = color,
        "success" => colors.success = color,
        _ => {}
    }
}

fn set_syntax_color(syntax: &mut crate::style::SyntaxColors, field: &str, color: Color) {
    match field {
        "keyword" => syntax.keyword = color,
        "type_name" => syntax.type_name = color,
        "function" => syntax.function = color,
        "string" => syntax.string = color,
        "number" => syntax.number = color,
        "comment" => syntax.comment = color,
        "operator" => syntax.operator = color,
        "constant" => syntax.constant = color,
        "variable" => syntax.variable = color,
        _ => {}
    }
}

/// Wpis dostępnego stylu: identyfikator (nazwa pliku bez rozszerzenia), ścieżka i nazwa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledTheme {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// Katalogi wyszukiwania stylów (kolejność = priorytet).
///
/// Oprócz katalogów standardowych uwzględnia `LUNA_THEMES_DIR` — listę katalogów
/// rozdzielonych `:` (przydatne w dev/testach, by wskazać katalog ze stylami).
pub fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(extra) = std::env::var_os("LUNA_THEMES_DIR") {
        for part in std::env::split_paths(&extra) {
            if !part.as_os_str().is_empty() {
                dirs.push(part);
            }
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".config").join("luna").join("themes"));
        dirs.push(
            home.join(".local")
                .join("share")
                .join("luna")
                .join("themes"),
        );
    }
    dirs.push(PathBuf::from("/usr/share/luna/themes"));
    dirs
}

/// Style dostępne w podanych katalogach (pierwszy wygrywa przy powtórzonym `id`).
pub fn available_in(dirs: &[PathBuf]) -> Vec<StyledTheme> {
    let mut out: Vec<StyledTheme> = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("theme") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            if out.iter().any(|t| t.id == id) {
                continue;
            }
            let name = ThemePatch::from_file(&path)
                .ok()
                .and_then(|p| p.name().map(str::to_owned))
                .unwrap_or_else(|| id.clone());
            out.push(StyledTheme { id, name, path });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Style dostępne w standardowych katalogach.
pub fn available() -> Vec<StyledTheme> {
    available_in(&search_dirs())
}

/// Wczytuje styl o danym `id` ze standardowych katalogów.
pub fn load(id: &str) -> Option<ThemePatch> {
    available()
        .into_iter()
        .find(|t| t.id == id)
        .and_then(|t| ThemePatch::from_file(&t.path).ok())
}

/// Wczytuje styl z podanej nazwy (`id`) albo bezpośredniej ścieżki do pliku.
pub fn load_spec(spec: &str) -> Option<ThemePatch> {
    let path = Path::new(spec);
    if path.extension().and_then(|e| e.to_str()) == Some("theme") {
        return ThemePatch::from_file(path).ok();
    }
    load(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NORD: &str = "\
# przykładowy styl
__name__ = Nord
appearance = dark
colors.card_bg = #3b4252
colors.text = #eceff4
accent = teal
radius.small = 4
metrics.button.height = 38
nieznany.klucz = cokolwiek
";

    #[test]
    fn parse_name_and_entries() {
        let p = ThemePatch::parse(NORD);
        assert_eq!(p.name(), Some("Nord"));
        assert_eq!(p.entries().count(), 7);
    }

    #[test]
    fn build_overrides_and_keeps_fallback_accent() {
        // Styl bez `accent` → zachowuje akcent z fallbacku.
        let patch = ThemePatch::parse("appearance = dark\ncolors.text = #123456\n");
        let base = Theme::light().accented(Accent::Purple.color(), Accent::Purple.foreground());
        let theme = patch.build(base);
        assert_eq!(theme.colors.text, Color::from_hex("#123456").unwrap());
        assert_eq!(theme.colors.accent, Accent::Purple.color());
        // `appearance = dark` przełącza paletę na ciemną.
        assert_ne!(theme.colors.window_bg, Theme::light().colors.window_bg);
    }

    #[test]
    fn parse_applies_geometry_and_accent_and_ignores_unknown() {
        let patch = ThemePatch::parse(NORD);
        let theme = patch.build(Theme::light());
        assert_eq!(theme.radius.small, 4.0);
        assert_eq!(theme.metrics.button.height, 38.0);
        assert_eq!(theme.colors.accent, Accent::Teal.color());
        assert_eq!(theme.colors.card_bg, Color::from_hex("#3b4252").unwrap());
    }

    #[test]
    fn syntax_colors_can_be_overridden_in_style_files() {
        let patch = ThemePatch::parse("syntax.keyword = #112233\nsyntax.comment = #445566\n");
        let theme = patch.build(Theme::light());
        assert_eq!(theme.syntax.keyword, Color::from_hex("#112233").unwrap());
        assert_eq!(theme.syntax.comment, Color::from_hex("#445566").unwrap());
    }

    #[test]
    fn available_and_load_from_dir() {
        let dir = std::env::temp_dir().join("libluna-theme-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("testish.theme");
        std::fs::write(&path, "__name__ = Testish\ncolors.text = #010203\n").unwrap();

        let list = available_in(std::slice::from_ref(&dir));
        assert!(list
            .iter()
            .any(|t| t.id == "testish" && t.name == "Testish"));

        let patch = ThemePatch::from_file(&path).unwrap();
        let theme = patch.build(Theme::light());
        assert_eq!(theme.colors.text, Color::from_hex("#010203").unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
