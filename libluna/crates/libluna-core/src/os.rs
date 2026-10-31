//! Nazwa systemu operacyjnego (z `/etc/os-release`).
//!
//! Aplikacje Luny nie powinny zakładać, że dystrybucja nazywa się „Luna” —
//! nazwę bierzemy z systemu, żeby tytuły okien i etykiety pasowały do PaganOS,
//! Pagix albo dowolnej innej dystrybucji zbudowanej na Lunie.

/// Nazwa systemu bez wersji (np. `Pagan Linux`).
///
/// Kolejność: `NAME`, potem `PRETTY_NAME`, na końcu `ID`. Wartości w cudzysłowach
/// są odcyfrowywane; brak pliku nie jest błędem (zwracamy `Linux`).
pub fn name() -> String {
    // Pozwala nadpisać nazwę w sesji/paczce (spójnie z instalatorem).
    if let Ok(value) = std::env::var("LUNA_SYSTEM_NAME") {
        let value = value.trim();
        if !value.is_empty() {
            return value.to_owned();
        }
    }
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| parse_name(&text))
        .unwrap_or_else(|| "Linux".to_owned())
}

/// Wyciąga czytelną nazwę z zawartości pliku `os-release`.
pub fn parse_name(text: &str) -> Option<String> {
    let mut name = None;
    let mut pretty = None;
    let mut id = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_owned();
        if value.is_empty() || value.contains(['\n', '\r', '\0']) {
            continue;
        }
        match key.trim() {
            "NAME" => name.get_or_insert(value),
            "PRETTY_NAME" => pretty.get_or_insert(value),
            "ID" => id.get_or_insert(value),
            _ => continue,
        };
    }
    name.or(pretty).or_else(|| {
        id.map(|id| {
            let mut chars = id.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => id,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_name_over_versioned_pretty_name() {
        let text = r#"
NAME="Pagan Linux"
PRETTY_NAME="Pagan Linux 0.5.3 Mokosza"
ID=pagan
"#;
        assert_eq!(parse_name(text).as_deref(), Some("Pagan Linux"));
    }

    #[test]
    fn falls_back_to_pretty_name_then_id() {
        assert_eq!(
            parse_name("PRETTY_NAME=\"Pagix 1.0\"\n").as_deref(),
            Some("Pagix 1.0")
        );
        assert_eq!(parse_name("ID=pagos\n").as_deref(), Some("Pagos"));
        assert_eq!(parse_name("").as_deref(), None);
    }
}
