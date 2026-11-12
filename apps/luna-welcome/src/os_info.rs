//! Metadane systemu z pakietu wydania („os-info”) — wersja i dokumentacja.
//!
//! PaganOS trzyma prawdę o wydaniu w pakiecie `pagan-release` („os-info”):
//! `/etc/os-release`, `/etc/pagan-release` (a Luna dodatkowo `/etc/luna-release`).
//! Pakiet może też dostarczyć notatki wydania i dokumentację offline:
//!
//! - `/usr/local/share/os-info/release-notes.md` (albo `news.md`),
//! - `/usr/share/os-info/...`,
//! - `/usr/local/share/os-info/docs/index.html` (albo `/usr/share/...`).
//!
//! Adres dokumentacji czytamy z `DOCUMENTATION_URL`, `DOC_URL` lub `DOCS_URL`
//! (w tej kolejności), a `LUNA_DOC_URL` pozwala wymusić go z sesji/obrazu.
//! Dzięki temu „Co nowego” i przycisk dokumentacji pochodzą z wydania systemu,
//! a nie z treści wbudowanej w binarkę — bez pobierania czegokolwiek z sieci.

use std::path::PathBuf;

/// Pliki wydania w kolejności pierwszeństwa (Luna → PaganOS → freedesktop).
pub const RELEASE_FILES: &[&str] = &["/etc/luna-release", "/etc/pagan-release", "/etc/os-release"];

/// Klucze z adresem dokumentacji (kolejność = priorytet).
pub const DOC_KEYS: &[&str] = &["DOCUMENTATION_URL", "DOC_URL", "DOCS_URL"];

/// Domyślna dokumentacja Luny, gdy wydanie nie poda własnego adresu.
pub const DEFAULT_DOCS: &str = "https://luna.paganlinux.eu/docs/";

/// Lokalna dokumentacja dostarczona przez wydanie (otwierana przez `xdg-open`).
pub const DOC_INDEX: &[&str] = &[
    "/usr/local/share/os-info/docs/index.html",
    "/usr/share/os-info/docs/index.html",
];

/// Pary `klucz = wartość` z pliku w stylu `os-release` (bez komentarzy).
pub fn parse_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value)
            .trim();
        if !value.is_empty() {
            pairs.push((key.trim().to_owned(), value.to_owned()));
        }
    }
    pairs
}

/// Pierwsza wartość spośród `keys` (kolejność kluczy = priorytet; ostatni wpis
/// o tym samym kluczu wygrywa, jak w `os-release`).
pub fn pick(pairs: &[(String, String)], keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some((_, value)) = pairs
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        {
            return Some(value.clone());
        }
    }
    None
}

/// Wartość z pierwszego pliku wydania, który ją definiuje.
pub fn value(keys: &[&str]) -> Option<String> {
    for path in RELEASE_FILES {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Some(value) = pick(&parse_pairs(&text), keys) {
            return Some(value);
        }
    }
    None
}

/// Wersja systemu z pakietu wydania (`VERSION_ID` → `VERSION` → `BUILD_ID`).
pub fn version() -> Option<String> {
    value(&["VERSION_ID", "VERSION", "BUILD_ID"])
}

/// Źródło dokumentacji z konkretnych par `klucz = wartość` (testowalne).
///
/// Kolejność: `DOCUMENTATION_URL` → `DOC_URL` → `DOCS_URL`. Akceptujemy URL
/// (`http`/`https`/`file`) i ścieżkę lokalną.
pub fn documentation_from(pairs: &[(String, String)]) -> Option<String> {
    // Niepoprawna wartość pod kluczem o wyższym priorytecie nie blokuje
    // kolejnych (np. literówka w `DOC_URL` przy poprawnym `DOCS_URL`).
    for key in DOC_KEYS {
        if let Some((_, value)) = pairs
            .iter()
            .rev()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        {
            if looks_like_target(value) {
                return Some(value.clone());
            }
        }
    }
    None
}

/// Źródło dokumentacji: `LUNA_DOC_URL`, pliki wydania (`/etc/os-release` itd.
/// — `DOCUMENTATION_URL`/`DOC_URL`/`DOCS_URL`), lokalny `index.html` z
/// `share/os-info/docs/`. `None` = użyj [`DEFAULT_DOCS`].
pub fn documentation() -> Option<String> {
    // Wymuszenie z sesji/obrazu niestandardowego ma najwyższy priorytet.
    if let Ok(url) = std::env::var("LUNA_DOC_URL") {
        let url = url.trim();
        if looks_like_target(url) {
            return Some(url.to_owned());
        }
    }
    for path in RELEASE_FILES {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Some(url) = documentation_from(&parse_pairs(&text)) {
            return Some(url);
        }
    }
    DOC_INDEX
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

/// Docelowy adres „Read the documentation”: wydanie/os-info, a gdy brak —
/// [`DEFAULT_DOCS`]. Zawsze zwraca niepusty napis.
pub fn documentation_target() -> String {
    documentation().unwrap_or_else(|| DEFAULT_DOCS.to_owned())
}

/// Czy wartość da się otworzyć przez `xdg-open` (URL albo ścieżka lokalna).
fn looks_like_target(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && (value.starts_with("https://")
            || value.starts_with("http://")
            || value.starts_with("file://")
            || value.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_values_and_ignores_comments() {
        let pairs =
            parse_pairs("# komentarz\nNAME=\"Luna\"\nVERSION_ID=1.2.3\nVERSION='1.2.3 (Noc)'\n");
        assert_eq!(pick(&pairs, &["NAME"]).as_deref(), Some("Luna"));
        assert_eq!(pick(&pairs, &["VERSION_ID"]).as_deref(), Some("1.2.3"));
        assert_eq!(pick(&pairs, &["VERSION"]).as_deref(), Some("1.2.3 (Noc)"));
    }

    #[test]
    fn key_order_is_priority() {
        let pairs = parse_pairs("VERSION=\"1.2.3 (Noc)\"\nVERSION_ID=1.2.3\n");
        assert_eq!(
            pick(&pairs, &["VERSION_ID", "VERSION"]).as_deref(),
            Some("1.2.3")
        );
        // Ostatni wpis o tym samym kluczu wygrywa.
        let pairs = parse_pairs("NAME=Stary\nNAME=Nowy\n");
        assert_eq!(pick(&pairs, &["NAME"]).as_deref(), Some("Nowy"));
    }

    #[test]
    fn documentation_accepts_urls_and_local_paths() {
        assert!(looks_like_target("https://luna.paganlinux.eu/docs/"));
        assert!(looks_like_target("http://example.org"));
        assert!(looks_like_target(
            "file:///usr/share/os-info/docs/index.html"
        ));
        assert!(looks_like_target("/usr/share/os-info/docs/index.html"));
        assert!(!looks_like_target(""));
        assert!(!looks_like_target("ftp://example.org"));
    }

    #[test]
    fn doc_url_key_is_supported() {
        // Wystarczy `DOC_URL` w `/etc/os-release`.
        let pairs = parse_pairs("NAME=Luna\nDOC_URL=https://docs.example.org/luna\n");
        assert_eq!(
            documentation_from(&pairs).as_deref(),
            Some("https://docs.example.org/luna")
        );
        // Priorytet: `DOCUMENTATION_URL` przed `DOC_URL` i `DOCS_URL`.
        let pairs = parse_pairs(
            "DOCS_URL=https://c.example\nDOC_URL=https://b.example\nDOCUMENTATION_URL=https://a.example\n",
        );
        assert_eq!(
            documentation_from(&pairs).as_deref(),
            Some("https://a.example")
        );
        // Niepoprawny schemat jest ignorowany.
        let pairs =
            parse_pairs("DOC_URL=ftp://example.org\nDOCS_URL=/usr/share/os-info/docs/index.html\n");
        assert_eq!(
            documentation_from(&pairs).as_deref(),
            Some("/usr/share/os-info/docs/index.html")
        );
    }

    #[test]
    fn documentation_target_never_empty() {
        assert!(!documentation_target().trim().is_empty());
    }

    #[test]
    fn release_files_are_ordered_luna_first() {
        assert_eq!(RELEASE_FILES[0], "/etc/luna-release");
        assert!(DOC_INDEX
            .iter()
            .all(|path| path.ends_with("docs/index.html")));
    }
}
