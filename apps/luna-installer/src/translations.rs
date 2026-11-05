//! Języki systemowe i pokrycie tłumaczeń Luny.
//!
//! Lista pochodzi z **systemu** (`locale -a` / `/usr/share/i18n/SUPPORTED`),
//! a nie tylko z wbudowanych języków libLuny. Przy każdym języku pokazujemy,
//! ile procent kluczy Luna ma przetłumaczonych — dzięki temu widać, czy wybór
//! daje pełny interfejs, czy tylko język systemowy.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use libluna_core::region;

// Migawka katalogów libLuny (kopie z `libluna/crates/libluna-i18n/i18n/`),
// używana tylko jako zapas na minimalnym systemie. Na zainstalowanym systemie
// i na obrazie live świeże katalogi przychodzą z `libluna_i18n::catalog_dirs()`
// (`/usr/share/luna/i18n`) i nadpisują te wbudowane.
const EN: &str = include_str!("../data/i18n/en.lang");
const PL: &str = include_str!("../data/i18n/pl.lang");
const DE: &str = include_str!("../data/i18n/de.lang");

/// Język do wyboru w instalatorze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageOption {
    pub code: String,
    pub name: String,
    /// Najlepszy locale systemowy dla tego języka (np. `pl_PL.UTF-8`).
    pub locale: String,
    /// Pokrycie tłumaczeń Luny w procentach (0–100).
    pub coverage: u8,
}

impl LanguageOption {
    /// Etykieta w liście: `Polski (pl) — 98%`.
    pub fn label(&self) -> String {
        format!("{} ({}) — {}%", self.name, self.code, self.coverage)
    }
}

/// Wszystkie języki widoczne w systemie, posortowane po nazwie.
pub fn options() -> Vec<LanguageOption> {
    let locales = region::system_locales();
    let catalogs = catalogs();
    let base = catalogs.get("en").cloned().unwrap_or_default();

    let mut by_code: BTreeMap<String, LanguageOption> = BTreeMap::new();
    for locale in &locales {
        let code = language_code(locale);
        if code.is_empty() || code == "c" || code == "posix" {
            continue;
        }
        let entry = by_code.entry(code.clone()).or_insert_with(|| {
            let coverage = coverage_percent(&base, catalogs.get(&code));
            LanguageOption {
                code: code.clone(),
                name: language_name(&code),
                locale: locale.clone(),
                coverage,
            }
        });
        // Preferuj UTF-8 i locale z krajem (np. `pl_PL.UTF-8`).
        if better_locale(locale, &entry.locale) {
            entry.locale = locale.clone();
        }
    }

    // Języki wbudowane Luny muszą być dostępne nawet w minimalnym systemie.
    for (code, fallback) in [
        ("en", "en_US.UTF-8"),
        ("pl", "pl_PL.UTF-8"),
        ("de", "de_DE.UTF-8"),
    ] {
        by_code
            .entry(code.to_owned())
            .or_insert_with(|| LanguageOption {
                code: code.to_owned(),
                name: language_name(code),
                locale: fallback.to_owned(),
                coverage: coverage_percent(&base, catalogs.get(code)),
            });
    }

    let mut list: Vec<_> = by_code.into_values().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

/// Kod języka z locale (`pl_PL.UTF-8` → `pl`).
pub fn language_code(locale: &str) -> String {
    let base = locale.split(['.', '@', ':']).next().unwrap_or(locale);
    base.split(['_', '-'])
        .next()
        .unwrap_or(base)
        .to_ascii_lowercase()
}

/// Nazwa języka w tym języku (dla popularnych języków; inaczej kod).
pub fn language_name(code: &str) -> String {
    let code = code.to_ascii_lowercase();
    let name = match code.as_str() {
        "pl" => "Polski",
        "en" => "English",
        "de" => "Deutsch",
        "fr" => "Français",
        "es" => "Español",
        "it" => "Italiano",
        "pt" => "Português",
        "nl" => "Nederlands",
        "cs" => "Čeština",
        "sk" => "Slovenčina",
        "uk" => "Українська",
        "ru" => "Русский",
        "sv" => "Svenska",
        "no" | "nb" => "Norsk",
        "da" => "Dansk",
        "fi" => "Suomi",
        "hu" => "Magyar",
        "ro" => "Română",
        "bg" => "Български",
        "el" => "Ελληνικά",
        "tr" => "Türkçe",
        "ja" => "日本語",
        "zh" => "中文",
        "ko" => "한국어",
        "ar" => "العربية",
        "he" => "עברית",
        "hi" => "हिन्दी",
        "vi" => "Tiếng Việt",
        "th" => "ไทย",
        "id" => "Bahasa Indonesia",
        "ca" => "Català",
        "eu" => "Euskara",
        "gl" => "Galego",
        "et" => "Eesti",
        "lv" => "Latviešu",
        "lt" => "Lietuvių",
        "sl" => "Slovenščina",
        "hr" => "Hrvatski",
        "sr" => "Српски",
        "bs" => "Bosanski",
        "mk" => "Македонски",
        "sq" => "Shqip",
        "is" => "Íslenska",
        "ga" => "Gaeilge",
        "cy" => "Cymraeg",
        other => return other.to_ascii_uppercase(),
    };
    name.to_owned()
}

fn better_locale(candidate: &str, current: &str) -> bool {
    let score = |value: &str| {
        let mut score = 0;
        if value.to_ascii_lowercase().contains("utf-8")
            || value.to_ascii_lowercase().contains("utf8")
        {
            score += 4;
        }
        if value.contains('_') {
            score += 2;
        }
        if value == "C" || value == "POSIX" {
            score -= 10;
        }
        score
    };
    score(candidate) > score(current)
}

/// Pokrycie: ile kluczy bazowych (en) ma tłumaczenie.
pub fn coverage_percent(base: &HashSet<String>, language: Option<&HashSet<String>>) -> u8 {
    if base.is_empty() {
        return 0;
    }
    let Some(language) = language else {
        return 0;
    };
    let translated = base.iter().filter(|key| language.contains(*key)).count();
    ((translated * 100) / base.len()).min(100) as u8
}

/// Klucze katalogów wbudowanych + katalogi instalowane w systemie.
fn catalogs() -> BTreeMap<String, HashSet<String>> {
    let mut sources: Vec<(String, String)> = vec![
        ("en".to_owned(), EN.to_owned()),
        ("pl".to_owned(), PL.to_owned()),
        ("de".to_owned(), DE.to_owned()),
    ];
    for dir in libluna_i18n::catalog_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("lang") {
                continue;
            }
            let Some(code) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(&path) {
                sources.push((code.to_ascii_lowercase(), text));
            }
        }
    }

    let mut result: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for (code, text) in sources {
        let keys = result.entry(code).or_default();
        keys.extend(parse_keys(&text));
    }
    result
}

/// Klucze katalogu `*.lang` (bez `__name__`).
pub fn parse_keys(text: &str) -> HashSet<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, _) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() || key == "__name__" {
                return None;
            }
            Some(key.to_owned())
        })
        .collect()
}

/// Proponowany locale dla kodu języka na podstawie listy systemowej.
pub fn locale_for(code: &str, system_locales: &[String]) -> String {
    let code = code.to_ascii_lowercase();
    let mut candidates: Vec<&String> = system_locales
        .iter()
        .filter(|locale| language_code(locale) == code)
        .collect();
    candidates.sort_by_key(|locale| {
        let lower = locale.to_ascii_lowercase();
        let mut score = 0;
        if lower.contains("utf-8") || lower.contains("utf8") {
            score -= 4;
        }
        if !locale.contains('_') {
            score += 1;
        }
        score
    });
    if let Some(locale) = candidates.first() {
        return (*locale).clone();
    }
    match code.as_str() {
        "pl" => "pl_PL.UTF-8".to_owned(),
        "de" => "de_DE.UTF-8".to_owned(),
        "en" => "en_US.UTF-8".to_owned(),
        other => format!("{other}.UTF-8"),
    }
}

/// Sensowna strefa czasowa dla języka (tylko podpowiedź; użytkownik może zmienić).
pub fn suggest_timezone(code: &str) -> &'static str {
    match code {
        "pl" => "Europe/Warsaw",
        "de" => "Europe/Berlin",
        "fr" => "Europe/Paris",
        "es" => "Europe/Madrid",
        "it" => "Europe/Rome",
        "pt" => "Europe/Lisbon",
        "nl" => "Europe/Amsterdam",
        "cs" => "Europe/Prague",
        "sk" => "Europe/Bratislava",
        "uk" => "Europe/Kyiv",
        "ru" => "Europe/Moscow",
        "sv" => "Europe/Stockholm",
        "no" | "nb" => "Europe/Oslo",
        "da" => "Europe/Copenhagen",
        "fi" => "Europe/Helsinki",
        "hu" => "Europe/Budapest",
        "ro" => "Europe/Bucharest",
        "bg" => "Europe/Sofia",
        "el" => "Europe/Athens",
        "tr" => "Europe/Istanbul",
        "ja" => "Asia/Tokyo",
        "zh" => "Asia/Shanghai",
        "ko" => "Asia/Seoul",
        "ar" => "Asia/Riyadh",
        "he" => "Asia/Jerusalem",
        "hi" => "Asia/Kolkata",
        "vi" => "Asia/Ho_Chi_Minh",
        "th" => "Asia/Bangkok",
        "id" => "Asia/Jakarta",
        "en" => "Europe/London",
        _ => "UTC",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_language_code() {
        assert_eq!(language_code("pl_PL.UTF-8"), "pl");
        assert_eq!(language_code("de-DE"), "de");
        assert_eq!(language_code("en"), "en");
    }

    #[test]
    fn parses_catalog_keys() {
        let keys = parse_keys("# c\n__name__ = Polski\nWelcome = Witaj\nA = B\n");
        assert!(keys.contains("Welcome"));
        assert!(keys.contains("A"));
        assert!(!keys.contains("__name__"));
    }

    #[test]
    fn coverage_is_relative_to_english_base() {
        let base: HashSet<String> = ["a", "b", "c", "d"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let pl: HashSet<String> = ["a", "b", "c"].into_iter().map(str::to_owned).collect();
        assert_eq!(coverage_percent(&base, Some(&pl)), 75);
        assert_eq!(coverage_percent(&base, None), 0);
    }

    #[test]
    fn builtin_languages_exist_and_have_coverage() {
        let options = options();
        assert!(options.iter().any(|option| option.code == "pl"));
        let pl = options.iter().find(|option| option.code == "pl").unwrap();
        assert!(pl.coverage > 50, "polskie pokrycie: {}", pl.coverage);
        assert!(pl.locale.to_lowercase().contains("pl"));
    }
}
