//! Klawiatura: listy układów, wariantów i modeli XKB.
//!
//! Listy czytamy z `evdev.lst` (czytelny plik reguł XKB), a gdy go brak —
//! z wbudowanego, niewielkiego zestawu. Instalator zapisuje wybór do
//! `/etc/default/keyboard`, `/etc/vconsole.conf`, Xorg i środowiska XKB.

use std::path::Path;

/// Pozycja listy XKB (`code` + czytelna nazwa).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbOption {
    pub code: String,
    pub name: String,
}

/// Wariant układu (`pl` + `legacy`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbVariant {
    pub layout: String,
    pub code: String,
    pub name: String,
}

/// Wszystkie listy potrzebne w kroku „Klawiatura”.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XkbLists {
    pub layouts: Vec<XkbOption>,
    pub variants: Vec<XkbVariant>,
    pub models: Vec<XkbOption>,
}

impl XkbOption {
    pub fn label(&self) -> String {
        format!("{} — {}", self.code, self.name)
    }
}

/// Wybór użytkownika.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    pub layout: String,
    pub variant: String,
    pub model: String,
}

impl Default for Keyboard {
    fn default() -> Self {
        Self {
            layout: "us".to_owned(),
            variant: String::new(),
            model: "pc105".to_owned(),
        }
    }
}

impl Keyboard {
    pub fn variants_for<'a>(&self, lists: &'a XkbLists) -> Vec<&'a XkbVariant> {
        lists
            .variants
            .iter()
            .filter(|variant| variant.layout == self.layout)
            .collect()
    }
}

/// Wczytuje listy z systemu (z fallbackiem wbudowanym).
pub fn load() -> XkbLists {
    for path in [
        "/usr/share/X11/xkb/rules/evdev.lst",
        "/usr/local/share/X11/xkb/rules/evdev.lst",
        "/usr/share/X11/xkb/rules/base.lst",
    ] {
        if let Ok(text) = std::fs::read_to_string(Path::new(path)) {
            let lists = parse_lst(&text);
            if !lists.layouts.is_empty() {
                return lists;
            }
        }
    }
    fallback()
}

/// Parser `*.lst` (sekcje `! model`, `! layout`, `! variant`).
pub fn parse_lst(text: &str) -> XkbLists {
    let mut lists = XkbLists::default();
    let mut section = "";
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix("! ") {
            section = name.trim();
            continue;
        }
        let trimmed = line.trim_start();
        let Some((code, rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let name = rest.trim().to_owned();
        if code.is_empty() || name.is_empty() {
            continue;
        }
        match section {
            "layout" => lists.layouts.push(XkbOption {
                code: code.to_owned(),
                name,
            }),
            "model" => lists.models.push(XkbOption {
                code: code.to_owned(),
                name,
            }),
            "variant" => {
                let Some((layout, variant_name)) = name.split_once(':') else {
                    continue;
                };
                lists.variants.push(XkbVariant {
                    layout: layout.trim().to_owned(),
                    code: code.to_owned(),
                    name: variant_name.trim().to_owned(),
                });
            }
            _ => {}
        }
    }
    lists
}

/// Domyślny układ dla języka interfejsu.
pub fn default_layout(language: &str) -> &'static str {
    match language {
        "pl" => "pl",
        "de" => "de",
        "fr" => "fr",
        "es" => "es",
        "it" => "it",
        "pt" => "pt",
        "nl" => "us",
        "cs" => "cz",
        "sk" => "sk",
        "uk" => "ua",
        "ru" => "ru",
        "sv" => "se",
        "no" | "nb" => "no",
        "da" => "dk",
        "fi" => "fi",
        "hu" => "hu",
        "ro" => "ro",
        "bg" => "bg",
        "el" => "gr",
        "tr" => "tr",
        "ja" => "jp",
        "ko" => "kr",
        "zh" => "cn",
        "he" => "il",
        "hi" => "in",
        "th" => "th",
        "vi" => "vn",
        "id" => "id",
        _ => "us",
    }
}

fn fallback() -> XkbLists {
    let opt = |code: &str, name: &str| XkbOption {
        code: code.to_owned(),
        name: name.to_owned(),
    };
    XkbLists {
        layouts: vec![
            opt("us", "English (US)"),
            opt("gb", "English (UK)"),
            opt("pl", "Polish"),
            opt("de", "German"),
            opt("fr", "French"),
            opt("es", "Spanish"),
            opt("it", "Italian"),
            opt("pt", "Portuguese"),
            opt("nl", "Dutch"),
            opt("cz", "Czech"),
            opt("sk", "Slovak"),
            opt("ua", "Ukrainian"),
            opt("ru", "Russian"),
            opt("se", "Swedish"),
            opt("no", "Norwegian"),
            opt("dk", "Danish"),
            opt("fi", "Finnish"),
            opt("hu", "Hungarian"),
            opt("ro", "Romanian"),
            opt("bg", "Bulgarian"),
            opt("gr", "Greek"),
            opt("tr", "Turkish"),
            opt("jp", "Japanese"),
            opt("kr", "Korean"),
            opt("cn", "Chinese"),
            opt("il", "Hebrew"),
            opt("in", "Indian"),
            opt("th", "Thai"),
            opt("vn", "Vietnamese"),
            opt("id", "Indonesian"),
        ],
        variants: vec![],
        models: vec![
            opt("pc105", "Generic 105-key PC"),
            opt("pc104", "Generic 104-key PC"),
            opt("pc101", "Generic 101-key PC"),
            opt("apple", "Apple"),
            opt("thinkpad", "ThinkPad"),
        ],
    }
}

/// Walidacja kodu układu/modelu/wariantu (bezpieczne dla plików konfiguracyjnych).
pub fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '(' | ')' | ':'))
}

/// Znajduje indeks kodu albo 0.
pub fn index_of(list: &[XkbOption], code: &str) -> usize {
    list.iter()
        .position(|option| option.code == code)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "! model\n  pc105  Generic 105-key PC\n  pc104  Generic 104-key PC\n! layout\n  us     English (US)\n  pl     Polish\n! variant\n  legacy  pl: Polish (legacy)\n  phonetic  us: English (phonetic)\n";

    #[test]
    fn parses_sections() {
        let lists = parse_lst(SAMPLE);
        assert_eq!(lists.layouts.len(), 2);
        assert_eq!(lists.models.len(), 2);
        assert_eq!(lists.variants.len(), 2);
        assert_eq!(lists.layouts[1].code, "pl");
        assert_eq!(lists.variants[0].layout, "pl");
        assert_eq!(lists.variants[0].code, "legacy");
    }

    #[test]
    fn plural_layout_defaults_to_polish() {
        assert_eq!(default_layout("pl"), "pl");
        assert_eq!(default_layout("de"), "de");
        assert_eq!(default_layout("xx"), "us");
    }

    #[test]
    fn validates_codes() {
        assert!(valid_code("pl"));
        assert!(valid_code("pc105"));
        assert!(!valid_code(""));
        assert!(!valid_code("pl;rm -rf"));
    }

    #[test]
    fn system_lists_or_fallback_are_available() {
        let lists = load();
        assert!(!lists.layouts.is_empty());
        assert!(!lists.models.is_empty());
    }
}
