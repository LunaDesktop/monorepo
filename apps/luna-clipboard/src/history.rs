//! Historia schowka: wpisy, przypinanie i zapis na dysk.
//!
//! Format pliku jest celowo prosty i odporny na dowolną treść: każdy wpis to
//! `przypięty <TAB> czas <TAB> hex(UTF-8)`. Hex zajmuje dwa razy więcej, ale
//! nie wymaga ucieczek — w schowku bywają przecież tabulatory i całe pliki.

use std::fs;
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ile wpisów trzyma historia. Przypięte nigdy nie wypadają.
pub const MAX_ENTRIES: usize = 60;

/// Największy pojedynczy wpis, jaki trzymamy w historii (1 MiB).
///
/// Skopiowany log potrafi mieć setki megabajtów; zapis takiego wpisu do
/// `clipboard.conf` (heksadecymalnie, czyli 2× więcej) zamułżył by każdy zapis.
pub const MAX_ENTRY_BYTES: usize = 1024 * 1024;

/// Łączny budżet pamięci na historię (8 MiB) — chroni przed 60 × 1 MiB.
pub const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;

/// Pierwsza linia pliku — pozwala w przyszłości zmienić format bez zgadywania.
const HEADER: &str = "# luna-clipboard 1";

/// Jeden wpis historii.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub text: String,
    pub pinned: bool,
    /// Czas skopiowania (sekundy od epoki; `0` = nieznany, np. wpis testowy).
    pub at: u64,
}

/// Cała historia — najnowsze wpisy na początku.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    pub entries: Vec<Entry>,
}

impl History {
    /// Bieżący czas w sekundach (bez paniki, gdy zegar stoi przed epoką).
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    }

    /// Dopisuje wpis z bieżącym czasem.
    pub fn push(&mut self, text: &str) -> bool {
        let now = Self::now();
        self.push_at(text, now)
    }

    /// Dopisuje wpis o podanym czasie; `false` = pusto, duplikat ostatniego
    /// albo wpis przekracza [`MAX_ENTRY_BYTES`].
    pub fn push_at(&mut self, text: &str, at: u64) -> bool {
        let text = text.trim_end_matches('\0');
        if text.trim().is_empty() {
            return false;
        }
        if text.len() > MAX_ENTRY_BYTES {
            return false;
        }
        if self.entries.first().is_some_and(|entry| entry.text == text) {
            return false;
        }
        // Ten sam tekst wraca na górę, ale zachowuje przypięcie — inaczej
        // `Ctrl+P` znikałoby po ponownym skopiowaniu tej samej treści.
        let pinned = match self.entries.iter().position(|entry| entry.text == text) {
            Some(index) => self.entries.remove(index).pinned,
            None => false,
        };
        self.entries.insert(
            0,
            Entry {
                text: text.to_owned(),
                pinned,
                at,
            },
        );
        self.trim();
        true
    }

    /// Usuwa najstarsze wpisy ponad limit, pomijając przypięte.
    pub fn trim(&mut self) {
        while self.entries.len() > MAX_ENTRIES {
            match self.entries.iter().rposition(|entry| !entry.pinned) {
                Some(index) => {
                    self.entries.remove(index);
                }
                // Wszystko przypięte — nie wyrzucamy nic.
                None => break,
            }
        }
        // Drugi limit: łączny rozmiar. Bez tego 60 wpisów po 1 MiB dawało
        // 120 MB w pliku konfiguracji (heksadecymalnie) przepisywane przy
        // każdym skopiowaniu tekstu.
        let mut total: usize = self.entries.iter().map(|entry| entry.text.len()).sum();
        while total > MAX_TOTAL_BYTES {
            let Some(index) = self.entries.iter().rposition(|entry| !entry.pinned) else {
                break;
            };
            total = total.saturating_sub(self.entries[index].text.len());
            self.entries.remove(index);
        }
    }

    pub fn toggle_pin(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.pinned = !entry.pinned;
        }
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    /// Indeksy wpisów pasujących do zapytania (bez wielkości liter).
    ///
    /// Puste zapytanie zwraca wszystko; szukamy w całej treści, nie tylko
    /// w widocznym początku — użytkownik pamięta środek wklejonego tekstu.
    pub fn matches(&self, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| needle.is_empty() || contains_ignore_case(&entry.text, &needle))
            .map(|(index, _)| index)
            .collect()
    }

    /// Ścieżka historii (`~/.config/luna/clipboard.conf`).
    pub fn path() -> PathBuf {
        config_dir().join("clipboard.conf")
    }

    pub fn load() -> Self {
        Self::load_from(&Self::path()).unwrap_or_default()
    }

    /// Wczytuje historię; uszkodzone linie są pomijane, nie przerywają startu.
    pub fn load_from(path: &Path) -> Option<Self> {
        let content = fs::read_to_string(path).ok()?;
        let mut history = Self::default();
        for line in content.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split('\t');
            let (Some(flag), Some(at), Some(hex)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let Ok(decoded) = hex_decode(hex) else {
                continue;
            };
            let Ok(text) = String::from_utf8(decoded) else {
                continue;
            };
            history.entries.push(Entry {
                text,
                pinned: flag == "1",
                at: at.parse().unwrap_or(0),
            });
        }
        history.trim();
        Some(history)
    }

    pub fn save(&self) -> io::Result<()> {
        self.save_to(&Self::path())
    }

    /// Zapis atomowy z prawami `0600` — schowek bywa prywatny.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = String::from(HEADER);
        out.push('\n');
        for entry in &self.entries {
            out.push(if entry.pinned { '1' } else { '0' });
            out.push('\t');
            out.push_str(&entry.at.to_string());
            out.push('\t');
            out.push_str(&hex_encode(entry.text.as_bytes()));
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

/// `contains` bez alokacji całego wpisu przy zapytaniu ASCII.
///
/// Wpis może mieć do 1 MiB, a filtr liczy się po każdym klawiszu —
/// `to_lowercase()` na całym tekście to zbędne megabajty na klawisz.
fn contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    if needle_lower.is_ascii() {
        let needle = needle_lower.as_bytes();
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("luna");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| "/".into());
    PathBuf::from(home).join(".config/luna")
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
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
    fn newest_entry_goes_first_and_duplicates_are_skipped() {
        let mut history = History::default();
        assert!(history.push_at("pierwszy", 1));
        assert!(history.push_at("drugi", 2));
        assert_eq!(history.entries[0].text, "drugi");
        // Ten sam tekst pod rząd nie tworzy kopii.
        assert!(!history.push_at("drugi", 3));
        assert_eq!(history.entries.len(), 2);
    }

    #[test]
    fn repeated_text_moves_to_top_and_keeps_pin() {
        let mut history = History::default();
        history.push_at("a", 1);
        history.push_at("b", 2);
        history.toggle_pin(1); // "a" przypięty
        assert!(history.push_at("a", 3));
        assert_eq!(history.entries[0].text, "a");
        assert!(history.entries[0].pinned);
        assert_eq!(history.entries.len(), 2);
    }

    #[test]
    fn empty_and_whitespace_entries_are_ignored() {
        let mut history = History::default();
        assert!(!history.push_at("", 1));
        assert!(!history.push_at("  \n\t ", 2));
        assert!(history.entries.is_empty());
    }

    #[test]
    fn trim_drops_oldest_unpinned_only() {
        let mut history = History::default();
        history.push_at("przypięty", 0);
        history.entries[0].pinned = true;
        for index in 0..MAX_ENTRIES + 20 {
            history.push_at(&format!("wpis {index}"), index as u64);
        }
        assert_eq!(history.entries.len(), MAX_ENTRIES);
        assert!(history.entries.iter().any(|entry| entry.pinned));
    }

    #[test]
    fn search_is_case_insensitive_and_covers_the_whole_text() {
        let mut history = History::default();
        history.push_at("początek\nŚRODEK\nkoniec", 1);
        history.push_at("inny", 2);
        // Najnowszy wpis jest pierwszy, więc `ŚRODEK` siedzi na indeksie 1.
        assert_eq!(history.matches("środek"), vec![1]);
        assert_eq!(history.matches("").len(), 2);
        assert!(history.matches("nie ma").is_empty());
    }

    #[test]
    fn hex_roundtrip_handles_unicode_and_control_characters() {
        let text = "zażółć\tgęślą\njaźń \u{1}\0";
        let encoded = hex_encode(text.as_bytes());
        let decoded = hex_decode(&encoded).expect("poprawny hex");
        assert_eq!(String::from_utf8(decoded).unwrap(), text);
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zz").is_err());
    }

    #[test]
    fn save_and_load_roundtrip_keeps_pins_and_time() {
        let dir = std::env::temp_dir().join(format!("luna-clip-{}", std::process::id()));
        let path = dir.join("clipboard.conf");
        let mut history = History::default();
        history.push_at("pierwszy\tz tabulatorem", 42);
        history.push_at("drugi\nz nową linią", 43);
        history.toggle_pin(0);
        history.save_to(&path).expect("zapis");

        let loaded = History::load_from(&path).expect("odczyt");
        assert_eq!(loaded, history);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn broken_lines_do_not_break_loading() {
        let dir = std::env::temp_dir().join(format!("luna-clip-broken-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clipboard.conf");
        fs::write(&path, "# header\nzła linia\n0\t1\tnie-hex\n0\t2\t6869\n").unwrap();
        let loaded = History::load_from(&path).expect("odczyt");
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].text, "hi");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod budget_tests {
    use super::{History, MAX_ENTRY_BYTES, MAX_TOTAL_BYTES};

    #[test]
    fn oversized_entries_are_rejected() {
        let mut history = History::default();
        let big = "x".repeat(MAX_ENTRY_BYTES + 1);
        assert!(!history.push_at(&big, 1));
        assert!(
            history.entries.is_empty(),
            "zbyt duży wpis nie trafia do historii"
        );
        let fits = "x".repeat(MAX_ENTRY_BYTES);
        assert!(history.push_at(&fits, 2));
    }

    #[test]
    fn total_budget_drops_oldest_unpinned() {
        let mut history = History::default();
        let chunk = "y".repeat(MAX_ENTRY_BYTES);
        // 8 wpisów po 1 MiB = dokładnie budżet; dziewiąty musi wypchnąć najstarszy.
        for index in 0..9u64 {
            history.push_at(&format!("{index}{chunk}"), index + 1);
        }
        let total: usize = history.entries.iter().map(|entry| entry.text.len()).sum();
        assert!(
            total <= MAX_TOTAL_BYTES,
            "łącznie {total} B przekracza budżet"
        );
        assert!(history.entries.len() < 9);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::History;

    #[test]
    fn filter_is_case_insensitive_for_ascii_and_polish() {
        let mut history = History::default();
        history.push_at("Zażółć gęślą jaźń", 1);
        history.push_at("Cargo Test", 2);
        assert_eq!(history.matches("zażółć").len(), 1);
        assert_eq!(history.matches("cargo").len(), 1);
        assert_eq!(history.matches("GĘŚLĄ").len(), 1);
        assert_eq!(history.matches("nie ma").len(), 0);
        assert_eq!(history.matches("").len(), 2);
    }
}
