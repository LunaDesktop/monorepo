//! Konta poczty: konfiguracja serwerów, zapis na dysk i presety dostawców.
//!
//! Plik `~/.config/luna/email.conf` ma prawa `0600` — zawiera hasła. Format
//! jest liniowy i heksadecymalny, żeby tabulatory czy nowe linie w haśle nie
//! rozsypały rekordu. Pusty wpis hasła oznacza „weź z `LUNA_EMAIL_PASSWORD`”.

use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Szyfrowanie połączenia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encryption {
    /// Od razu TLS (port 993/465).
    #[default]
    Tls,
    /// Połączenie jawne, potem STARTTLS (port 143/587).
    StartTls,
    /// Bez szyfrowania — tylko dla lokalnego Bridge'a (Proton, Mailpit).
    None,
}

impl Encryption {
    pub fn name(self) -> &'static str {
        match self {
            Self::Tls => "tls",
            Self::StartTls => "starttls",
            Self::None => "none",
        }
    }

    fn parse(text: &str) -> Self {
        match text.trim().to_lowercase().as_str() {
            "starttls" => Self::StartTls,
            "none" => Self::None,
            _ => Self::Tls,
        }
    }
}

/// Jedno konto poczty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub email: String,
    pub display_name: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_encryption: Encryption,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_encryption: Encryption,
    /// Login IMAP/SMTP (często pełny adres, ale nie zawsze).
    pub login: String,
    /// Hasło (najlepiej „hasło aplikacji”). Puste = z `LUNA_EMAIL_PASSWORD`.
    pub password: String,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            display_name: String::new(),
            imap_host: String::new(),
            imap_port: 993,
            imap_encryption: Encryption::Tls,
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_encryption: Encryption::Tls,
            login: String::new(),
            password: String::new(),
        }
    }
}

impl Account {
    /// Nowe konto z uzupełnionymi serwerami na podstawie domeny adresu.
    pub fn from_email(email: &str) -> Self {
        let email = email.trim();
        let preset = Preset::for_email(email);
        let login = email.to_owned();
        Self {
            name: email.to_owned(),
            email: email.to_owned(),
            display_name: String::new(),
            imap_host: preset.imap_host,
            imap_port: preset.imap_port,
            imap_encryption: preset.imap_encryption,
            smtp_host: preset.smtp_host,
            smtp_port: preset.smtp_port,
            smtp_encryption: preset.smtp_encryption,
            login,
            password: String::new(),
        }
    }

    /// Hasło do faktycznego użycia: z pliku albo ze zmiennej środowiskowej.
    pub fn resolved_password(&self) -> String {
        if !self.password.is_empty() {
            return self.password.clone();
        }
        std::env::var("LUNA_EMAIL_PASSWORD").unwrap_or_default()
    }

    /// Krótki podpis konta do paska bocznego.
    pub fn label(&self) -> String {
        if self.display_name.trim().is_empty() {
            self.email.clone()
        } else {
            format!("{} <{}>", self.display_name.trim(), self.email)
        }
    }

    /// Czy konfiguracja ma komplet pól potrzebnych do połączenia.
    pub fn is_complete(&self) -> bool {
        !self.email.trim().is_empty()
            && !self.imap_host.trim().is_empty()
            && !self.smtp_host.trim().is_empty()
            && !self.login.trim().is_empty()
    }
}

/// Ustawienia serwerów typowego dostawcy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_encryption: Encryption,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_encryption: Encryption,
}

impl Preset {
    /// Rozpoznaje dostawcę po domenie. Nieznana domena dostaje zgadywane
    /// `imap.<domena>`/`smtp.<domena>` — użytkownik i tak może poprawić.
    pub fn for_email(email: &str) -> Self {
        let domain = email
            .rsplit_once('@')
            .map(|(_, domain)| domain.trim().to_lowercase())
            .unwrap_or_default();

        let tls = |imap_host: &str, imap_port: u16, smtp_host: &str, smtp_port: u16| Self {
            imap_host: imap_host.to_owned(),
            imap_port,
            imap_encryption: Encryption::Tls,
            smtp_host: smtp_host.to_owned(),
            smtp_port,
            smtp_encryption: Encryption::Tls,
        };

        match domain.as_str() {
            "gmail.com" | "googlemail.com" => tls("imap.gmail.com", 993, "smtp.gmail.com", 465),
            "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => Self {
                imap_host: "outlook.office365.com".to_owned(),
                imap_port: 993,
                imap_encryption: Encryption::Tls,
                smtp_host: "smtp.office365.com".to_owned(),
                smtp_port: 587,
                smtp_encryption: Encryption::StartTls,
            },
            "yahoo.com" | "ymail.com" => {
                tls("imap.mail.yahoo.com", 993, "smtp.mail.yahoo.com", 465)
            }
            "icloud.com" | "me.com" | "mac.com" => Self {
                imap_host: "imap.mail.me.com".to_owned(),
                imap_port: 993,
                imap_encryption: Encryption::Tls,
                smtp_host: "smtp.mail.me.com".to_owned(),
                smtp_port: 587,
                smtp_encryption: Encryption::StartTls,
            },
            "proton.me" | "protonmail.com" | "pm.me" => Self {
                // Proton wymaga lokalnego Bridge'a — bez szyfrowania na loopbacku.
                imap_host: "127.0.0.1".to_owned(),
                imap_port: 1143,
                imap_encryption: Encryption::None,
                smtp_host: "127.0.0.1".to_owned(),
                smtp_port: 1025,
                smtp_encryption: Encryption::None,
            },
            "wp.pl" => tls("imap.wp.pl", 993, "smtp.wp.pl", 465),
            "o2.pl" => tls("imap.o2.pl", 993, "smtp.o2.pl", 465),
            "interia.pl" | "interia.eu" => tls("imap.interia.pl", 993, "smtp.interia.pl", 465),
            "onet.pl" | "onet.eu" => tls("imap.poczta.onet.pl", 993, "smtp.poczta.onet.pl", 465),
            "gazeta.pl" => tls("imap.gazeta.pl", 993, "smtp.gazeta.pl", 465),
            _ => {
                let host = if domain.is_empty() {
                    "localhost"
                } else {
                    &domain
                };
                tls(&format!("imap.{host}"), 993, &format!("smtp.{host}"), 465)
            }
        }
    }
}

/// Wszystkie konta i aktywne z nich.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountStore {
    pub accounts: Vec<Account>,
    /// Indeks aktywnego konta (pilnowany przy wczytywaniu).
    pub active: usize,
}

impl AccountStore {
    pub fn active_account(&self) -> Option<&Account> {
        self.accounts.get(self.active)
    }

    pub fn active_account_mut(&mut self) -> Option<&mut Account> {
        self.accounts.get_mut(self.active)
    }

    /// Ustawia aktywne konto bez wyjścia poza zakres.
    pub fn set_active(&mut self, index: usize) {
        if index < self.accounts.len() {
            self.active = index;
        }
    }

    /// Dodaje konto albo podmienia istniejące o tym samym adresie.
    pub fn upsert(&mut self, account: Account) -> usize {
        if let Some(index) = self
            .accounts
            .iter()
            .position(|existing| existing.email.eq_ignore_ascii_case(&account.email))
        {
            self.accounts[index] = account;
            self.active = index;
            index
        } else {
            self.accounts.push(account);
            self.active = self.accounts.len() - 1;
            self.active
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.accounts.len() {
            self.accounts.remove(index);
            if self.active >= self.accounts.len() {
                self.active = self.accounts.len().saturating_sub(1);
            }
        }
    }

    /// `~/.config/luna/email.conf`.
    pub fn path() -> PathBuf {
        config_dir().join("email.conf")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path()).unwrap_or_default()
    }

    /// Wczytuje konta; uszkodzone linie pomijamy, żeby jeden błąd nie
    /// blokował startu całej aplikacji.
    pub fn load_from(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let mut store = Self::default();
        let mut active: Option<usize> = None;
        for line in content.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            if fields.first() == Some(&"active") {
                active = fields.get(1).and_then(|value| value.parse().ok());
                continue;
            }
            if fields.first() != Some(&"account") || fields.len() < 12 {
                continue;
            }
            let decoded = |index: usize| -> String {
                hex_decode(fields[index])
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .unwrap_or_default()
            };
            let account = Account {
                name: decoded(1),
                email: decoded(2),
                display_name: decoded(3),
                imap_host: decoded(4),
                imap_port: fields[5].parse().unwrap_or(993),
                imap_encryption: Encryption::parse(fields[6]),
                smtp_host: decoded(7),
                smtp_port: fields[8].parse().unwrap_or(465),
                smtp_encryption: Encryption::parse(fields[9]),
                login: decoded(10),
                password: decoded(11),
            };
            if !account.email.is_empty() {
                store.accounts.push(account);
            }
        }
        if let Some(index) = active {
            store.set_active(index);
        }
        Some(store)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::path())
    }

    /// Zapis atomowy z prawami `0600` — plik zawiera hasła.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::from("# luna-email 1\n");
        out.push_str(&format!("active\t{}\n", self.active));
        for account in &self.accounts {
            out.push_str("account");
            for value in [
                &account.name,
                &account.email,
                &account.display_name,
                &account.imap_host,
            ] {
                out.push('\t');
                out.push_str(&hex_encode(value));
            }
            out.push('\t');
            out.push_str(&account.imap_port.to_string());
            out.push('\t');
            out.push_str(account.imap_encryption.name());
            out.push('\t');
            out.push_str(&hex_encode(&account.smtp_host));
            out.push('\t');
            out.push_str(&account.smtp_port.to_string());
            out.push('\t');
            out.push_str(account.smtp_encryption.name());
            out.push('\t');
            out.push_str(&hex_encode(&account.login));
            out.push('\t');
            out.push_str(&hex_encode(&account.password));
            out.push('\n');
        }
        let tmp = path.with_extension("conf.tmp");
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)?;
            io::Write::write_all(&mut file, out.as_bytes())?;
        }
        fs::rename(&tmp, path)
    }
}

fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("luna");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/".into());
    PathBuf::from(home).join(".config/luna")
}

fn hex_encode(text: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(text.len() * 2);
    for byte in text.as_bytes() {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, ()> {
    if text.len() % 2 != 0 {
        return Err(());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = (bytes[index] as char).to_digit(16).ok_or(())?;
        let low = (bytes[index + 1] as char).to_digit(16).ok_or(())?;
        out.push((high * 16 + low) as u8);
        index += 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_cover_popular_providers() {
        let gmail = Preset::for_email("ktos@gmail.com");
        assert_eq!(gmail.imap_host, "imap.gmail.com");
        assert_eq!(gmail.smtp_port, 465);
        assert_eq!(gmail.smtp_encryption, Encryption::Tls);

        let outlook = Preset::for_email("ktos@outlook.com");
        assert_eq!(outlook.imap_host, "outlook.office365.com");
        assert_eq!(outlook.smtp_encryption, Encryption::StartTls);

        let wp = Preset::for_email("ktos@wp.pl");
        assert_eq!(wp.imap_host, "imap.wp.pl");

        let lokalny = Preset::for_email("ktos@proton.me");
        assert_eq!(lokalny.imap_port, 1143);
        assert_eq!(lokalny.imap_encryption, Encryption::None);
    }

    #[test]
    fn unknown_domain_gets_a_sensible_guess() {
        let preset = Preset::for_email("ktos@example.org");
        assert_eq!(preset.imap_host, "imap.example.org");
        assert_eq!(preset.smtp_host, "smtp.example.org");
    }

    #[test]
    fn store_roundtrip_keeps_password_and_active_account() {
        let dir = std::env::temp_dir().join(format!("luna-email-{}", std::process::id()));
        let path = dir.join("email.conf");
        let mut store = AccountStore::default();
        let mut first = Account::from_email("ala@example.org");
        first.password = "tajne\thasło\nz ramką".to_owned();
        first.display_name = "Ala Nowak".to_owned();
        store.upsert(first);
        let second = Account::from_email("bob@example.org");
        store.upsert(second);
        store.set_active(0);
        store.save_to(&path).expect("zapis");

        let loaded = AccountStore::load_from(&path).expect("odczyt");
        assert_eq!(loaded.accounts.len(), 2);
        assert_eq!(loaded.active, 0);
        assert_eq!(loaded.accounts[0].password, "tajne\thasło\nz ramką");
        assert_eq!(loaded.accounts[0].display_name, "Ala Nowak");
        assert_eq!(loaded.accounts[1].email, "bob@example.org");

        // Plik z hasłami nie może być czytelny dla grupy/świata.
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn upsert_replaces_account_with_the_same_address() {
        let mut store = AccountStore::default();
        store.upsert(Account::from_email("ala@example.org"));
        let mut updated = Account::from_email("ala@example.org");
        updated.display_name = "Ala".to_owned();
        store.upsert(updated);
        assert_eq!(store.accounts.len(), 1);
        assert_eq!(store.accounts[0].display_name, "Ala");
    }

    #[test]
    fn removing_active_account_keeps_index_in_range() {
        let mut store = AccountStore::default();
        store.upsert(Account::from_email("a@example.org"));
        store.upsert(Account::from_email("b@example.org"));
        assert_eq!(store.active, 1);
        store.remove(1);
        assert_eq!(store.active, 0);
        assert!(store.active_account().is_some());
    }

    #[test]
    fn broken_lines_do_not_break_loading() {
        let dir = std::env::temp_dir().join(format!("luna-email-broken-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("email.conf");
        fs::write(&path, "# c\nzła linia\naccount\t01\nactive\t0\n").unwrap();
        let loaded = AccountStore::load_from(&path).expect("odczyt");
        assert!(loaded.accounts.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
