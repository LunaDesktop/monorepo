//! Trwały stan greetera: ostatnio wybrane konto i sesja.
//!
//! Plik należy do użytkownika `greeter` (to jego katalog stanu), więc traktujemy
//! go jako **niezaufaną podpowiedź**: zapisujemy tylko nazwę konta i identyfikator
//! sesji, a przy odczycie walidujemy je i nigdy nie wykonujemy z niego poleceń.
//! Polecenie sesji zawsze pochodzi z systemowych plików `.desktop`.

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Ostatnio wybrane konto i sesja.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prefs {
    pub user: Option<String>,
    pub session: Option<String>,
}

impl Prefs {
    /// Parsuje prosty plik `klucz=wartość` (nieznane klucze są ignorowane).
    pub fn parse(text: &str) -> Self {
        let mut prefs = Prefs::default();
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim() {
                "last_user" if valid_username(&value) => prefs.user = Some(value),
                "last_session" if valid_session_id(&value) => prefs.session = Some(value),
                _ => {}
            }
        }
        prefs
    }

    /// Serializuje do formatu akceptowanego przez [`Prefs::parse`].
    pub fn render(&self) -> String {
        let mut out = String::from("# Luna Greeter — ostatnio wybrane konto i sesja\n");
        if let Some(user) = &self.user {
            out.push_str(&format!("last_user={user}\n"));
        }
        if let Some(session) = &self.session {
            out.push_str(&format!("last_session={session}\n"));
        }
        out
    }
}

/// Ścieżka pliku stanu. Preferujemy katalog stanu greetera, a gdy go brak —
/// standardowy katalog greetd (jeśli da się do niego pisać).
pub fn path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("XDG_STATE_HOME") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir).join("luna/greeter.conf"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home).join(".local/state/luna/greeter.conf"));
        }
    }
    Some(PathBuf::from("/var/lib/greetd/luna/greeter.conf"))
}

/// Wczytuje stan (błędy i brak pliku → puste preferencje).
pub fn load() -> Prefs {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| Prefs::parse(&text))
        .unwrap_or_default()
}

/// Zapisuje stan atomowo; błędy są celowo ignorowane (greeter ma działać także
/// na systemie tylko do odczytu).
pub fn save(prefs: &Prefs) {
    if let Some(path) = path() {
        let _ = save_to(&path, prefs);
    }
}

fn save_to(path: &Path, prefs: &Prefs) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".greeter.conf.{}.tmp", std::process::id()));
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(prefs.render().as_bytes())?;
        file.sync_all().ok();
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Czy nazwa wygląda jak poprawny login (bez białych znaków, `:` i `/`).
pub fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@' | '$'))
}

/// Identyfikator sesji pochodzi z nazwy pliku `.desktop`.
pub fn valid_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_valid_entries() {
        let prefs = Prefs::parse(
            "# komentarz\nlast_user=patryk\nlast_session=selene\nlast_user=zły user\n",
        );
        assert_eq!(prefs.user.as_deref(), Some("patryk"));
        assert_eq!(prefs.session.as_deref(), Some("selene"));
        assert!(valid_username("patryk"));
        assert!(valid_username("jan.doe@example.com"));
        assert!(!valid_username("jan doe"));
        assert!(!valid_session_id("../evil"));
    }

    #[test]
    fn roundtrip_and_atomic_save() {
        let dir = std::env::temp_dir().join(format!(
            "luna-greeter-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path = dir.join("greeter.conf");
        let prefs = Prefs {
            user: Some("patryk".to_owned()),
            session: Some("selene".to_owned()),
        };
        save_to(&path, &prefs).expect("zapis");
        let text = std::fs::read_to_string(&path).expect("odczyt");
        assert_eq!(Prefs::parse(&text), prefs);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }
}
