//! # libluna-i18n
//!
//! Lekki system wielojęzyczny dla libLuny — bez gettext i bez zależności zewnętrznych.
//!
//! Model jest celowo prosty, w duchu gettext:
//!
//! - **kluczem jest angielski napis** (np. `"Appearance"`), więc brak tłumaczenia
//!   degraduje się łagodnie do angielskiego;
//! - każdy język to **osobny plik** `i18n/<kod>.lang` (format `klucz = tłumaczenie`),
//!   wbudowany w binarkę przez `include_str!` — łatwo dodawać i edytować;
//! - dodatkowo języki można **doładować w czasie działania** z katalogów
//!   (`/usr/share/luna/i18n`, `~/.local/share/luna/i18n`, `~/.config/luna/i18n`).
//!
//! ```no_run
//! use libluna_i18n::{tr, Locale};
//! libluna_i18n::set_locale(Locale::new("pl"));
//! assert_eq!(tr("Appearance"), "Wygląd");
//! ```
//!
//! Podstawianie argumentów: `tr_args("Hello, {name}", &[("name", "Ania")])`.
//! Każdy katalog może zdefiniować `__name__` — natywną nazwę języka do UI.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};

pub mod date_format;

/// Klucz z nazwą języka w danym katalogu (np. `Polski`).
const NAME_KEY: &str = "__name__";

/// Znacznik języka, np. `pl`, `pl_PL`, `en`, `de`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Locale(String);

impl Locale {
    /// Tworzy język z tagu (np. `pl_PL.UTF-8` → `pl`).
    pub fn new(tag: impl AsRef<str>) -> Self {
        let raw = tag.as_ref();
        // Utnij kodowanie (`.UTF-8`), modyfikator (`@euro`) i listę priorytetów (`pl:en`).
        let base = raw.split(['.', '@', ':']).next().unwrap_or(raw);
        let language = base.split(['_', '-']).next().unwrap_or(base);
        Self(language.to_ascii_lowercase())
    }

    /// Wykrywa język ze środowiska (`LC_ALL`, `LC_MESSAGES`, `LANGUAGE`, `LANG`).
    pub fn from_env() -> Self {
        for key in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
            if let Ok(value) = std::env::var(key) {
                if !value.is_empty() && value != "C" && value != "POSIX" {
                    return Locale::new(value);
                }
            }
        }
        Locale::new("en")
    }

    /// Kod języka (np. `pl`).
    pub fn language(&self) -> &str {
        &self.0
    }
}

impl Default for Locale {
    fn default() -> Self {
        Locale::new("en")
    }
}

impl From<&str> for Locale {
    fn from(value: &str) -> Self {
        Locale::new(value)
    }
}

impl From<String> for Locale {
    fn from(value: String) -> Self {
        Locale::new(value)
    }
}

/// Język widoczny w wyborze języka (Ustawienia Luny).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    /// Krótki kod języka, np. `pl`.
    pub code: String,
    /// Nazwa do pokazania w UI (natywna, jeśli czcionka ją pokrywa).
    pub name: String,
    /// Czy mamy katalog tłumaczeń; bez katalogu interfejs zostaje po angielsku.
    pub translated: bool,
}

/// Natywna i angielska nazwa języka spoza katalogów (np. wykrytego z systemu).
fn known_names(code: &str) -> Option<(&'static str, &'static str)> {
    Some(match code {
        "af" => ("Afrikaans", "Afrikaans"),
        "ar" => ("العربية", "Arabic"),
        "az" => ("Azərbaycan", "Azerbaijani"),
        "be" => ("Беларуская", "Belarusian"),
        "bg" => ("Български", "Bulgarian"),
        "bn" => ("বাংলা", "Bengali"),
        "bs" => ("Bosanski", "Bosnian"),
        "ca" => ("Català", "Catalan"),
        "cs" => ("Čeština", "Czech"),
        "cy" => ("Cymraeg", "Welsh"),
        "da" => ("Dansk", "Danish"),
        "de" => ("Deutsch", "German"),
        "el" => ("Ελληνικά", "Greek"),
        "en" => ("English", "English"),
        "eo" => ("Esperanto", "Esperanto"),
        "es" => ("Español", "Spanish"),
        "et" => ("Eesti", "Estonian"),
        "eu" => ("Euskara", "Basque"),
        "fa" => ("فارسی", "Persian"),
        "fi" => ("Suomi", "Finnish"),
        "fil" => ("Filipino", "Filipino"),
        "fr" => ("Français", "French"),
        "ga" => ("Gaeilge", "Irish"),
        "gl" => ("Galego", "Galician"),
        "he" => ("עברית", "Hebrew"),
        "hi" => ("हिन्दी", "Hindi"),
        "hr" => ("Hrvatski", "Croatian"),
        "hu" => ("Magyar", "Hungarian"),
        "hy" => ("Հայերեն", "Armenian"),
        "id" => ("Bahasa Indonesia", "Indonesian"),
        "is" => ("Íslenska", "Icelandic"),
        "it" => ("Italiano", "Italian"),
        "ja" => ("日本語", "Japanese"),
        "ka" => ("ქართული", "Georgian"),
        "kk" => ("Қазақша", "Kazakh"),
        "ko" => ("한국어", "Korean"),
        "lt" => ("Lietuvių", "Lithuanian"),
        "lv" => ("Latviešu", "Latvian"),
        "mk" => ("Македонски", "Macedonian"),
        "mn" => ("Монгол", "Mongolian"),
        "ms" => ("Bahasa Melayu", "Malay"),
        "mt" => ("Malti", "Maltese"),
        "nb" => ("Norsk bokmål", "Norwegian Bokmål"),
        "ne" => ("नेपाली", "Nepali"),
        "nl" => ("Nederlands", "Dutch"),
        "nn" => ("Norsk nynorsk", "Norwegian Nynorsk"),
        "no" => ("Norsk", "Norwegian"),
        "pl" => ("Polski", "Polish"),
        "pt" => ("Português", "Portuguese"),
        "ro" => ("Română", "Romanian"),
        "ru" => ("Русский", "Russian"),
        "sk" => ("Slovenčina", "Slovak"),
        "sl" => ("Slovenščina", "Slovenian"),
        "sq" => ("Shqip", "Albanian"),
        "sr" => ("Српски", "Serbian"),
        "sv" => ("Svenska", "Swedish"),
        "sw" => ("Kiswahili", "Swahili"),
        "ta" => ("தமிழ்", "Tamil"),
        "te" => ("తెలుగు", "Telugu"),
        "th" => ("ไทย", "Thai"),
        "tr" => ("Türkçe", "Turkish"),
        "uk" => ("Українська", "Ukrainian"),
        "ur" => ("اردو", "Urdu"),
        "uz" => ("Oʻzbek", "Uzbek"),
        "vi" => ("Tiếng Việt", "Vietnamese"),
        "zh" => ("中文", "Chinese"),
        "zu" => ("isiZulu", "Zulu"),
        _ => return None,
    })
}

/// Czy czcionka Luna (DejaVu/Adwaita) pokrywa wszystkie znaki nazwy?
///
/// Pismo CJK, arabskie, hebrajskie i indyjskie bez fallbacku renderuje się jako
/// puste prostokąty, więc dla nich pokazujemy nazwę angielską.
fn renderable(name: &str) -> bool {
    name.chars().all(|c| {
        let cp = c as u32;
        (0x20..=0x024f).contains(&cp)
            || (0x0370..=0x03ff).contains(&cp)
            || (0x0400..=0x04ff).contains(&cp)
            || (0x1e00..=0x1eff).contains(&cp)
    })
}

/// Nazwa języka do UI: natywna, jeśli da się ją wyrenderować, inaczej angielska.
fn display_name(code: &str) -> String {
    match known_names(code) {
        Some((native, _)) if renderable(native) => native.to_owned(),
        Some((_, english)) => english.to_owned(),
        None => code.to_ascii_uppercase(),
    }
}

/// Wyciąga kod języka z nazwy locale (`pl_PL.UTF-8` → `pl`), odrzucając `C`/`POSIX`.
fn language_code(locale: &str) -> Option<String> {
    let base = locale.trim().split(['.', '@']).next().unwrap_or_default();
    let code = base
        .split(['_', '-'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let alphabetic = !code.is_empty() && code.chars().all(|c| c.is_ascii_alphabetic());
    if !alphabetic || code == "c" || code == "posix" {
        return None;
    }
    (2..=3).contains(&code.len()).then_some(code)
}

/// Kody języków zainstalowanych w systemie (`locale -a`), bez powtórzeń.
pub fn system_language_codes() -> Vec<String> {
    let Ok(output) = Command::new("locale").arg("-a").output() else {
        return Vec::new();
    };
    let mut codes = BTreeSet::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(code) = language_code(line) {
            codes.insert(code);
        }
    }
    codes.into_iter().collect()
}

/// Katalog tłumaczeń jednego języka.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: HashMap<String, String>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Buduje katalog z tekstu (format `klucz = tłumaczenie`).
    pub fn parse(src: &str) -> Self {
        let mut catalog = Self::new();
        catalog.parse_into(src);
        catalog
    }

    /// Dodaje/aktualizuje wpisy z tekstu (pozwala nakładać pliki na siebie).
    pub fn parse_into(&mut self, src: &str) {
        for line in src.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                self.entries
                    .insert(key.trim().to_owned(), value.trim().to_owned());
            }
        }
    }

    /// Tłumaczenie dla klucza, jeśli istnieje.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// Natywna nazwa języka z `__name__` (jeśli zdefiniowana).
    pub fn name(&self) -> Option<&str> {
        self.get(NAME_KEY)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Zbiór katalogów + bieżący język + język zapasowy.
#[derive(Debug, Clone)]
pub struct Translations {
    locale: Locale,
    catalogs: BTreeMap<String, Catalog>,
    fallback: String,
}

impl Translations {
    /// Tworzy pusty zbiór z językiem zapasowym (domyślnie `en`).
    pub fn new(locale: Locale) -> Self {
        Self {
            locale,
            catalogs: BTreeMap::new(),
            fallback: "en".to_owned(),
        }
    }

    /// Dodaje/nałada katalog języka z tekstu.
    pub fn add_source(&mut self, language: &str, src: &str) -> &mut Self {
        self.catalogs
            .entry(language.to_ascii_lowercase())
            .or_default()
            .parse_into(src);
        self
    }

    /// Zestaw wbudowanych języków libLuny (pliki `i18n/*.lang` wbudowane w binarkę).
    pub fn builtin() -> Self {
        let mut t = Translations::new(Locale::from_env());
        t.add_source("en", include_str!("../i18n/en.lang"));
        t.add_source("pl", include_str!("../i18n/pl.lang"));
        t.add_source("de", include_str!("../i18n/de.lang"));
        // Języki „startowe”: katalog obejmuje najczęściej widoczne napisy,
        // a nieprzetłumaczone klucze wracają do angielskiego (kontrakt `tr`).
        // CMK, arabski i pismo złożone wymagają fallbacku czcionek/shaping — poza tym etapem.
        t.add_source("fr", include_str!("../i18n/fr.lang"));
        t.add_source("es", include_str!("../i18n/es.lang"));
        t.add_source("pt", include_str!("../i18n/pt.lang"));
        t.add_source("it", include_str!("../i18n/it.lang"));
        t.add_source("nl", include_str!("../i18n/nl.lang"));
        t.add_source("ru", include_str!("../i18n/ru.lang"));
        t.add_source("uk", include_str!("../i18n/uk.lang"));
        t.add_source("tr", include_str!("../i18n/tr.lang"));
        t.add_source("sv", include_str!("../i18n/sv.lang"));
        t.add_source("cs", include_str!("../i18n/cs.lang"));
        t.add_source("sw", include_str!("../i18n/sw.lang"));
        t
    }

    pub fn locale(&self) -> &Locale {
        &self.locale
    }

    /// Ustawia bieżący język.
    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    /// Krótki kod bieżącego języka.
    pub fn language(&self) -> &str {
        self.locale.language()
    }

    /// Dostępne kody języków (posortowane).
    pub fn languages(&self) -> Vec<String> {
        self.catalogs.keys().cloned().collect()
    }

    /// Natywna nazwa języka z jego katalogu (np. `Polski`).
    pub fn language_name(&self, language: &str) -> Option<&str> {
        self.catalogs.get(language).and_then(Catalog::name)
    }

    /// Czy mamy (niepusty) katalog tłumaczeń dla języka.
    pub fn has_catalog(&self, language: &str) -> bool {
        self.catalogs
            .get(&language.to_ascii_lowercase())
            .is_some_and(|catalog| !catalog.is_empty())
    }

    /// Języki do wyboru: katalogi Luny **oraz** locale zainstalowane w systemie.
    ///
    /// Pozycje bez katalogu mają `translated: false` — UI zostaje po angielsku.
    pub fn language_list(&self) -> Vec<Language> {
        let mut codes: BTreeSet<String> = self.catalogs.keys().cloned().collect();
        codes.extend(system_language_codes());
        let mut list: Vec<Language> = codes
            .into_iter()
            .map(|code| Language {
                translated: self.has_catalog(&code),
                name: self
                    .language_name(&code)
                    .map(str::to_owned)
                    .unwrap_or_else(|| display_name(&code)),
                code,
            })
            .collect();
        list.sort_by_cached_key(|language| (language.name.to_lowercase(), language.code.clone()));
        list
    }

    /// Tłumaczenie klucza; przy braku wpisu zwraca sam klucz (angielski).
    pub fn translate<'a>(&'a self, key: &'a str) -> &'a str {
        if let Some(catalog) = self.catalogs.get(self.locale.language()) {
            if let Some(value) = catalog.get(key) {
                return value;
            }
        }
        if let Some(catalog) = self.catalogs.get(&self.fallback) {
            if let Some(value) = catalog.get(key) {
                return value;
            }
        }
        key
    }

    /// Tłumaczenie z podstawieniem `{argumentów}`.
    pub fn translate_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut out = self.translate(key).to_owned();
        for (name, value) in args {
            out = out.replace(&format!("{{{name}}}"), value);
        }
        out
    }

    /// Doładowuje katalog z pojedynczego pliku `*.lang` (kod języka = nazwa pliku).
    pub fn load_file(&mut self, path: &Path) -> io::Result<()> {
        let Some(code) = path.file_stem().and_then(|s| s.to_str()) else {
            return Ok(());
        };
        let src = std::fs::read_to_string(path)?;
        self.add_source(code, &src);
        Ok(())
    }

    /// Doładowuje wszystkie pliki `*.lang` z katalogu; zwraca liczbę wczytanych.
    pub fn load_dir(&mut self, dir: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        let mut count = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lang")
                && self.load_file(&path).is_ok()
            {
                count += 1;
            }
        }
        count
    }
}

impl Default for Translations {
    fn default() -> Self {
        Translations::builtin()
    }
}

/// Standardowe katalogi wyszukiwania języków doładowywanych w runtime.
pub fn catalog_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(PathBuf::from("/usr/share/luna/i18n"));
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    {
        dirs.push(data.join("luna/i18n"));
    }
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    {
        dirs.push(config.join("luna/i18n"));
    }
    dirs
}

/// Globalna instancja tłumaczeń.
fn global() -> &'static RwLock<Translations> {
    static GLOBAL: OnceLock<RwLock<Translations>> = OnceLock::new();
    GLOBAL.get_or_init(|| RwLock::new(Translations::builtin()))
}

/// Ustawia globalny język.
pub fn set_locale(locale: impl Into<Locale>) {
    if let Ok(mut guard) = global().write() {
        guard.set_locale(locale.into());
    }
}

/// Bieżący globalny język.
pub fn locale() -> Locale {
    global()
        .read()
        .map(|g| g.locale().clone())
        .unwrap_or_default()
}

/// Doładowuje języki ze standardowych katalogów (nadpisują wbudowane).
/// Zwraca liczbę wczytanych plików.
pub fn load_catalogs_from_dirs() -> usize {
    let dirs = catalog_dirs();
    match global().write() {
        Ok(mut guard) => dirs.iter().map(|d| guard.load_dir(d)).sum(),
        Err(_) => 0,
    }
}

/// Plik wyboru języka z Ustawień Luny: `$XDG_CONFIG_HOME/luna/locale.conf`.
pub fn locale_config_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join("luna/locale.conf"));
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".config/luna/locale.conf"))
}

/// Parsuje prosty format `klucz = wartość` z `locale.conf`.
fn parse_language(text: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "language" {
            continue;
        }
        let value = value.trim();
        if !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Some(value.to_owned());
        }
    }
    None
}

fn read_language(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| parse_language(&text))
}

fn write_language(path: &Path, language: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = format!("# Luna — język interfejsu\nlanguage = {language}\n");
    // Zapis przez plik tymczasowy: przerwany zapis nie zostawia pustej konfiguracji.
    let tmp = path.with_extension("conf.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Język wybrany w Ustawieniach (albo wykryty z środowiska, gdy brak pliku).
pub fn preferred_locale() -> Locale {
    locale_config_path()
        .and_then(|path| read_language(&path))
        .map(Locale::new)
        .unwrap_or_else(Locale::from_env)
}

/// Zapisuje wybór języka do `$XDG_CONFIG_HOME/luna/locale.conf`.
///
/// Dodatkowo propaguje `LANGUAGE` do **całej sesji użytkownika**:
/// - `~/.config/environment.d/00-luna-locale.conf` (nowe usługi/procesy),
/// - środowiska aktywacji D-Busa (`dbus-update-activation-environment`),
/// - menedżera użytkownika systemd (`systemctl --user import-environment`).
///
/// Dzięki temu nowe aplikacje (także spoza Luny) startują od razu w nowym języku;
/// działające już programy muszą zostać uruchomione ponownie — tak samo jak w GNOME/KDE.
pub fn save_preferred_locale(locale: &Locale) -> io::Result<()> {
    let Some(path) = locale_config_path() else {
        return Ok(());
    };
    write_language(&path, locale.language())?;
    propagate_locale(locale.language());
    Ok(())
}

/// Ścieżka pliku `environment.d` dla języka Luny.
fn environment_locale_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    Some(config.join("environment.d").join("00-luna-locale.conf"))
}

/// Pełny tag locale dla języka (jeśli znamy mapowanie).
fn full_locale(language: &str) -> Option<&'static str> {
    match language {
        "pl" => Some("pl_PL.UTF-8"),
        "en" => Some("en_US.UTF-8"),
        "de" => Some("de_DE.UTF-8"),
        "fr" => Some("fr_FR.UTF-8"),
        "es" => Some("es_ES.UTF-8"),
        "pt" => Some("pt_PT.UTF-8"),
        "it" => Some("it_IT.UTF-8"),
        "nl" => Some("nl_NL.UTF-8"),
        "ru" => Some("ru_RU.UTF-8"),
        "uk" => Some("uk_UA.UTF-8"),
        "tr" => Some("tr_TR.UTF-8"),
        "sv" => Some("sv_SE.UTF-8"),
        "cs" => Some("cs_CZ.UTF-8"),
        "sw" => Some("sw_KE.UTF-8"),
        _ => None,
    }
}

/// Katalog locale generowanych przez użytkownika (bez roota).
fn user_locale_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".local/share/locale"))
}

fn user_locale_exists(full: &str) -> bool {
    user_locale_root()
        .map(|root| root.join(full).is_dir())
        .unwrap_or(false)
}

/// Generuje locale do katalogu użytkownika, gdy brak go w systemie.
///
/// Działa bez roota (`localedef` pisze do `~/.local/share/locale`), dzięki czemu
/// np. niemiecki może działać w Qt/GTK nawet na systemie, gdzie wygenerowano tylko
/// `pl_PL`/`en_US`. Zwraca `true`, jeśli locale jest dostępne.
fn ensure_user_locale(full: &str) -> bool {
    if locale_available(full) || user_locale_exists(full) {
        return true;
    }
    let Some(root) = user_locale_root() else {
        return false;
    };
    let Some(source) = full.split('.').next() else {
        return false;
    };
    if let Some(parent) = root.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::create_dir_all(&root);
    let target = root.join(full);
    for program in ["localedef", "/usr/sbin/localedef", "/usr/bin/localedef"] {
        if Command::new(program)
            .args(["-i", source, "-f", "UTF-8"])
            .arg(&target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return target.is_dir();
        }
    }
    false
}

/// Przygotowuje locale użytkownika dla znanych języków (wywoływane przy zmianie).
fn prepare_user_locales() {
    for full in ["pl_PL.UTF-8", "en_US.UTF-8", "de_DE.UTF-8", "C.UTF-8"] {
        let _ = ensure_user_locale(full);
    }
}

/// Ustawia `LANGUAGE`/`LANG` w **bieżącym procesie** (bez zapisu plików).
///
/// Wywołuje to `libluna-platform` po wykryciu zmiany `locale.conf`, żeby procesy
/// uruchamiane z powłoki (launcher, terminal) dziedziczyły język także wtedy,
/// gdy nie używają gettext (`LANGUAGE`).
pub fn sync_environment(locale: &Locale) {
    let language = locale.language();
    std::env::set_var("LANGUAGE", language);
    let Some(full) = full_locale(language) else {
        return;
    };
    if locale_available(full) {
        std::env::set_var("LANG", full);
    } else if user_locale_exists(full) {
        std::env::set_var("LANG", full);
        if let Some(root) = user_locale_root() {
            std::env::set_var("LOCPATH", root);
        }
    }
}

fn propagate_locale(language: &str) {
    // Locale spoza pl/en/de generujemy dopiero przy wyborze języka; bez tego
    // `LANGUAGE` byłoby nowe, a `LANG` (używany przez Qt/GTK) zostałby stary.
    if let Some(locale) = full_locale(language) {
        let _ = ensure_user_locale(locale);
    }
    // Bieżący proces i wszystkie uruchamiane z niego dzieci.
    prepare_user_locales();
    sync_environment(&Locale::new(language));

    // Gdy locale języka jest wygenerowane w systemie lub w katalogu użytkownika,
    // ustawiamy też `LANG` — z tego korzystają m.in. aplikacje Qt.
    let full = full_locale(language)
        .filter(|locale| locale_available(locale) || user_locale_exists(locale));
    if let Some(locale) = full {
        std::env::set_var("LANG", locale);
    }
    // `LOCPATH` jest potrzebne tylko wtedy, gdy locale pochodzi z katalogu użytkownika.
    let user_locale = full.is_some_and(|locale| !locale_available(locale));
    if user_locale {
        if let Some(root) = user_locale_root() {
            std::env::set_var("LOCPATH", &root);
        }
    }

    if let Some(path) = environment_locale_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut body = format!(
            "# Wygenerowane przez Luna Settings — język interfejsu.\nLANGUAGE={language}\n"
        );
        if let Some(locale) = full {
            body.push_str(&format!("LANG={locale}\n"));
        }
        if user_locale {
            if let Some(root) = user_locale_root() {
                body.push_str(&format!("LOCPATH={}\n", root.display()));
            }
        }
        let _ = std::fs::write(path, body);
    }

    // Aplikacje aktywowane przez D-Busa i usługi użytkownika mają widzieć nowy język.
    let mut vars: Vec<&str> = vec!["LANGUAGE"];
    if full.is_some() {
        vars.push("LANG");
    }
    if user_locale {
        vars.push("LOCPATH");
    }
    const DBUS_UPDATE: [&str; 3] = [
        "dbus-update-activation-environment",
        "/usr/bin/dbus-update-activation-environment",
        "/usr/sbin/dbus-update-activation-environment",
    ];
    for program in DBUS_UPDATE {
        let mut command = Command::new(program);
        command.arg("--systemd").args(&vars);
        if command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            break;
        }
    }
    const SYSTEMCTL: [&str; 3] = ["systemctl", "/usr/bin/systemctl", "/usr/sbin/systemctl"];
    for program in SYSTEMCTL {
        let mut command = Command::new(program);
        command.args(["--user", "import-environment"]).args(&vars);
        if command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            break;
        }
    }
}

/// Czy locale jest wygenerowane w systemie (`locale -a`)?
fn locale_available(name: &str) -> bool {
    fn normalize(value: &str) -> String {
        value
            .chars()
            .filter(|c| *c != '-' && *c != '.')
            .flat_map(char::to_lowercase)
            .collect()
    }
    let wanted = normalize(name);
    Command::new("locale")
        .arg("-a")
        .output()
        .ok()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| normalize(line.trim()) == wanted)
        })
        .unwrap_or(false)
}

/// Dostępne kody języków (globalnie).
pub fn languages() -> Vec<String> {
    global().read().map(|g| g.languages()).unwrap_or_default()
}

/// Natywna nazwa języka (globalnie).
pub fn language_name(language: &str) -> Option<String> {
    global()
        .read()
        .ok()
        .and_then(|g| g.language_name(language).map(str::to_owned))
}

/// Czy mamy katalog tłumaczeń dla języka (globalnie).
pub fn has_catalog(language: &str) -> bool {
    global()
        .read()
        .map(|g| g.has_catalog(language))
        .unwrap_or(false)
}

/// Języki do wyboru w Ustawieniach (katalogi Luny + locale systemowe).
pub fn language_list() -> Vec<Language> {
    global()
        .read()
        .map(|g| g.language_list())
        .unwrap_or_default()
}

/// Przetłumaczony napis (globalnie).
pub fn tr(key: &str) -> String {
    global()
        .read()
        .map(|g| g.translate(key).to_owned())
        .unwrap_or_else(|_| key.to_owned())
}

/// Przetłumaczony napis z podstawieniem argumentów (globalnie).
pub fn tr_args(key: &str, args: &[(&str, &str)]) -> String {
    global()
        .read()
        .map(|g| g.translate_args(key, args))
        .unwrap_or_else(|_| key.to_owned())
}

/// Skrót do [`tr`] / [`tr_args`].
///
/// ```ignore
/// let s = t!("Appearance");
/// let s = t!("Witaj, {name}", name = "Ania");
/// ```
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::tr($key)
    };
    ($key:expr, $($k:ident = $v:expr),+ $(,)?) => {
        $crate::tr_args($key, &[$( (stringify!($k), $v) ),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn locale_normalization() {
        assert_eq!(Locale::new("pl_PL.UTF-8").language(), "pl");
        assert_eq!(Locale::new("de-DE").language(), "de");
        assert_eq!(Locale::new("en_US").language(), "en");
        assert_eq!(Locale::new("C").language(), "c");
    }

    #[test]
    fn catalog_parses_and_ignores_comments() {
        let c = Catalog::parse("# komentarz\nA = B\n\nC = D = E\n");
        assert_eq!(c.get("A"), Some("B"));
        // Dzielimy tylko po pierwszym `=`.
        assert_eq!(c.get("C"), Some("D = E"));
    }

    #[test]
    fn builtin_translates_and_falls_back() {
        let mut t = Translations::new(Locale::new("pl"));
        t.add_source("pl", include_str!("../i18n/pl.lang"));
        assert_eq!(t.translate("Appearance"), "Wygląd");
        assert_eq!(t.translate("Nieznany klucz"), "Nieznany klucz");
    }

    #[test]
    fn switching_locale_changes_output() {
        let mut t = Translations::new(Locale::new("en"));
        t.add_source("pl", include_str!("../i18n/pl.lang"));
        t.add_source("de", include_str!("../i18n/de.lang"));
        t.set_locale(Locale::new("de"));
        assert_eq!(t.translate("Dark"), "Dunkel");
        t.set_locale(Locale::new("pl"));
        assert_eq!(t.translate("Dark"), "Ciemny");
    }

    #[test]
    fn language_names_and_listing() {
        let t = Translations::builtin();
        assert_eq!(t.language_name("pl"), Some("Polski"));
        assert_eq!(t.language_name("de"), Some("Deutsch"));
        assert_eq!(t.language_name("en"), Some("English"));
        assert!(t.languages().contains(&"pl".to_owned()));
    }

    #[test]
    fn locale_tags_accept_encoding_and_priority_list() {
        assert_eq!(Locale::new("pl_PL.UTF-8").language(), "pl");
        assert_eq!(Locale::new("de_DE@euro").language(), "de");
        assert_eq!(Locale::new("pl:en").language(), "pl");
    }

    #[test]
    fn locale_names_are_reduced_to_language_codes() {
        assert_eq!(language_code("pl_PL.UTF-8").as_deref(), Some("pl"));
        assert_eq!(language_code("de_CH.utf8@euro").as_deref(), Some("de"));
        assert_eq!(language_code("sr_RS.UTF-8@latin").as_deref(), Some("sr"));
        assert_eq!(language_code("C"), None);
        assert_eq!(language_code("C.UTF-8"), None);
        assert_eq!(language_code("POSIX"), None);
        assert_eq!(language_code(""), None);
    }

    #[test]
    fn names_use_native_spelling_only_when_glyphs_exist() {
        assert_eq!(display_name("fi"), "Suomi");
        assert_eq!(display_name("ru"), "Русский");
        assert_eq!(display_name("el"), "Ελληνικά");
        // Bez fallbacku czcionek pismo CJK/arabskie renderuje się jako kratki.
        assert_eq!(display_name("ja"), "Japanese");
        assert_eq!(display_name("ar"), "Arabic");
        assert_eq!(display_name("zz"), "ZZ");
    }

    #[test]
    fn language_list_merges_catalogs_with_system_locales() {
        let t = Translations::builtin();
        let list = t.language_list();
        for code in t.languages() {
            let entry = list
                .iter()
                .find(|language| language.code == code)
                .unwrap_or_else(|| panic!("{code} missing from language list"));
            assert!(entry.translated, "{code} should have a catalog");
            assert!(!entry.name.is_empty(), "{code} needs a display name");
        }
        let names: Vec<String> = list
            .iter()
            .map(|language| language.name.to_lowercase())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "language list is sorted by name");
    }

    #[test]
    fn args_substitution() {
        let t = Translations::new(Locale::new("en"));
        assert_eq!(
            t.translate_args("Hi, {name}!", &[("name", "Ada")]),
            "Hi, Ada!"
        );
    }

    #[test]
    fn load_file_overrides_and_adds() {
        let dir = std::env::temp_dir().join("libluna-i18n-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("xx.lang");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "__name__ = Testish").unwrap();
        writeln!(f, "Appearance = Xyglad").unwrap();
        drop(f);

        let mut t = Translations::builtin();
        t.load_file(&path).unwrap();
        t.set_locale(Locale::new("xx"));
        assert_eq!(t.translate("Appearance"), "Xyglad");
        assert_eq!(t.language_name("xx"), Some("Testish"));
        // Klucz nieprzetłumaczony w `xx` spada do bazy `en`.
        assert_eq!(t.translate("System"), "System");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn locale_config_parses_and_roundtrips() {
        assert_eq!(parse_language("language = pl\n").as_deref(), Some("pl"));
        assert_eq!(
            parse_language("# komentarz\n language=de_DE \n").as_deref(),
            Some("de_DE")
        );
        assert_eq!(parse_language("language = \n"), None);
        assert_eq!(parse_language("language = ../zle\n"), None);
        assert_eq!(parse_language("appearance = dark\n"), None);

        let dir = std::env::temp_dir().join(format!(
            "libluna-i18n-locale-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("luna/locale.conf");
        write_language(&path, "de").unwrap();
        assert_eq!(read_language(&path).as_deref(), Some("de"));
        std::fs::write(&path, "# ręcznie\nlanguage=pl\n").unwrap();
        assert_eq!(read_language(&path).as_deref(), Some("pl"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod starter_language_tests {
    use super::{Locale, Translations};

    #[test]
    fn starter_catalogs_are_registered_and_translate() {
        let mut t = Translations::builtin();
        for (code, name) in [
            ("fr", "Français"),
            ("es", "Español"),
            ("pt", "Português"),
            ("it", "Italiano"),
            ("nl", "Nederlands"),
            ("ru", "Русский"),
            ("uk", "Українська"),
            ("tr", "Türkçe"),
            ("sv", "Svenska"),
            ("cs", "Čeština"),
            ("sw", "Kiswahili"),
        ] {
            assert_eq!(t.language_name(code), Some(name), "nazwa języka {code}");
        }
        t.set_locale(Locale::new("fr"));
        assert_eq!(t.translate("Settings"), "Paramètres");
        t.set_locale(Locale::new("ru"));
        assert_eq!(t.translate("Close"), "Закрыть");
        t.set_locale(Locale::new("sw"));
        assert_eq!(t.translate("Close"), "Funga");
        t.set_locale(Locale::new("fr"));
        // Brak wpisu w katalogu = klucz angielski (kontrakt `tr`).
        assert_eq!(t.translate("Nieznany klucz"), "Nieznany klucz");
    }
}
