//! Model poczty niezależny od sieci i od rysowania.
//!
//! Wszystko, co trafia do UI, jest tu „spłaszczone" do własnych typów: nie
//! trzymamy w stanie aplikacji typów z `email-lib`, bo te są pożyczone
//! (self-referencing) i nie da się ich bezpiecznie przetrzymać między klatkami.

use std::time::{SystemTime, UNIX_EPOCH};

/// Rodzaj folderu — decyduje o ikonie i o tym, czy wolno w nim wysyłać/usuwać.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderKind {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
    Custom,
}

impl FolderKind {
    /// Rozpoznaje folder po nazwie (IMAP bywa różny: `Sent`, `Sent Items`…).
    pub fn detect(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("inbox") || lower.contains("przychodz") {
            Self::Inbox
        } else if lower.contains("sent") || lower.contains("wysłane") {
            Self::Sent
        } else if lower.contains("draft") || lower.contains("robocz") {
            Self::Drafts
        } else if lower.contains("trash") || lower.contains("deleted") || lower.contains("kosz") {
            Self::Trash
        } else if lower.contains("archive") || lower.contains("archiw") {
            Self::Archive
        } else {
            Self::Custom
        }
    }
}

/// Folder na liście po lewej.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderInfo {
    pub name: String,
    pub kind: FolderKind,
    /// Liczba nieprzeczytanych (jeśli serwer ją zna — inaczej 0).
    pub unread: usize,
    pub total: usize,
}

/// Skrót wiadomości do listy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageSummary {
    /// Identyfikator backendu (dla IMAP: numer UID jako tekst).
    pub id: String,
    pub from_name: String,
    pub from_addr: String,
    pub to_name: String,
    pub to_addr: String,
    pub subject: String,
    /// Znacznik czasu wiadomości (sekundy od epoki).
    pub date: i64,
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
    pub has_attachment: bool,
    /// Pierwsza linia treści (już oczyszczona).
    pub snippet: String,
}

impl MessageSummary {
    /// Nazwa do pokazania: najpierw nadawca, ale gdy brak — adres.
    pub fn sender(&self) -> &str {
        if !self.from_name.is_empty() {
            &self.from_name
        } else if !self.from_addr.is_empty() {
            &self.from_addr
        } else {
            "?"
        }
    }

    /// Temat z bezpiecznym zastępnikiem (pusty temat to nie „brak wiersza”).
    pub fn subject_or_placeholder(&self) -> &str {
        if self.subject.trim().is_empty() {
            "(bez tematu)"
        } else {
            &self.subject
        }
    }
}

/// Załącznik w podglądzie wiadomości.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentInfo {
    pub name: String,
    pub mime: String,
    pub size: usize,
    pub data: Vec<u8>,
}

impl AttachmentInfo {
    /// Rozmiar w czytelnej formie („1,4 MB”).
    pub fn size_label(&self) -> String {
        format_size(self.size)
    }
}

/// Wczytana treść wiadomości.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageBody {
    pub text: String,
    pub attachments: Vec<AttachmentInfo>,
}

/// Szkic wiadomości — stan edytora, jeszcze nie wysłany.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub to: String,
    pub cc: String,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<AttachmentInfo>,
    /// Id wiadomości, na którą to odpowiedź/przekazanie (dla nagłówków).
    pub in_reply_to: Option<String>,
    /// Adres nadawcy, od którego odpowiadamy.
    pub reply_to: Option<String>,
}

/// Błąd walidacji szkicu, gotowy do pokazania użytkownikowi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    NoRecipients,
    BadRecipient,
}

impl DraftError {
    /// Klucz tłumaczenia (napis angielski, jak w `tr`).
    pub fn message(self) -> &'static str {
        match self {
            Self::NoRecipients => "Add at least one recipient",
            Self::BadRecipient => "One of the addresses looks invalid",
        }
    }
}

/// Czy tekst wygląda na adres e-mail (`local@domena.tld`).
///
/// Celowo prosty: serwer i tak zweryfikuje adres. Chodzi tylko o to, żeby nie
/// wysyłać wiadomości na „wpisz tutaj” albo na urwany adres bez kropki.
pub fn looks_like_address(text: &str) -> bool {
    let text = text.trim();
    let Some((local, domain)) = text.rsplit_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    let Some((host, tld)) = domain.rsplit_once('.') else {
        return false;
    };
    !host.is_empty() && tld.len() >= 2 && !local.starts_with('.') && !local.ends_with('.')
}

/// Zamienia „Ania <ania@example.org>, bob@example.org” na listę adresów.
///
/// Obsługuje przecinki i średniki, bo oba bywają wpisywane. Puste fragmenty
/// (np. po ostatnim przecinku) są pomijane.
pub fn parse_addresses(input: &str) -> Vec<String> {
    input
        .split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            // „Nazwa <adres>” → sam adres; nazwę i tak odtworzymy z nagłówka.
            match (part.rfind('<'), part.rfind('>')) {
                (Some(start), Some(end)) if end > start => part[start + 1..end].trim().to_owned(),
                _ => part.to_owned(),
            }
        })
        .collect()
}

/// Sprawdza adresy z pola „Do”. Puste pole = błąd (nie ma do kogo wysłać).
pub fn validate_draft(draft: &Draft) -> Result<Vec<String>, DraftError> {
    let recipients = parse_addresses(&draft.to);
    if recipients.is_empty() {
        return Err(DraftError::NoRecipients);
    }
    if recipients.iter().any(|addr| !looks_like_address(addr)) {
        return Err(DraftError::BadRecipient);
    }
    Ok(recipients)
}

/// Pierwsze litery do awatara (maks. 2 znaki, wielkie litery).
pub fn initials(name: &str) -> String {
    let mut out = String::new();
    for word in name.split_whitespace() {
        if let Some(ch) = word.chars().next() {
            if ch.is_alphanumeric() {
                out.extend(ch.to_uppercase());
                if out.chars().count() >= 2 {
                    break;
                }
            }
        }
    }
    if out.is_empty() {
        "@".to_owned()
    } else {
        out
    }
}

/// Polska odmiana rzeczownika po liczbie (`1 wiadomość`, `2 wiadomości`).
pub fn plural(n: usize, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let last_two = n % 100;
    let last = n % 10;
    if n == 1 {
        one
    } else if (2..=4).contains(&last) && !(12..=14).contains(&last_two) {
        few
    } else {
        many
    }
}

/// Czytelny rozmiar pliku.
pub fn format_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else {
        format!("{:.1} GB", b / (KB * KB * KB))
    }
}

/// Bieżący czas w sekundach (bez paniki, gdy zegar stoi przed epoką).
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Etykieta czasu na liście: godzina (dziś), data (ten rok), pełna data.
///
/// Liczymy w UTC tylko jako *przybliżenie* — do listy wystarczy. Dzięki temu
/// brak zależności od strefy i od `chrono` w warstwie UI.
pub fn format_when(secs: i64, now: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let (day, month, year, hour, minute) = civil_from_unix(secs);
    let (today_day, today_month, today_year, _, _) = civil_from_unix(now);
    if (day, month, year) == (today_day, today_month, today_year) {
        format!("{hour:02}:{minute:02}")
    } else if year == today_year {
        format!("{day:02}.{month:02}")
    } else {
        format!("{day:02}.{month:02}.{year}")
    }
}

/// Pełna data do nagłówka podglądu: `12.03.2025, 14:07`.
pub fn format_full_date(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    let (day, month, year, hour, minute) = civil_from_unix(secs);
    format!("{day:02}.{month:02}.{year}, {hour:02}:{minute:02}")
}

/// Konwersja unix → (dzień, miesiąc, rok, godzina, minuta) bez zewnętrznych crate'ów.
///
/// Algorytm Howarda Hinnanta dla dni cywilnych; poprawny dla lat 1970+.
fn civil_from_unix(secs: i64) -> (i64, i64, i64, i64, i64) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    // 719468 = dni od 0000-03-01 do 1970-01-01.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (day, month, year, hour, minute)
}

/// Skrót treści do listy: jedna linia, bez cudzysłowów z odpowiedzi.
pub fn snippet(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('>') || line.starts_with("-- ") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(line);
        if out.chars().count() > max_chars {
            break;
        }
    }
    let mut chars = out.chars();
    let trimmed: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}…", trimmed.trim_end())
    } else {
        trimmed
    }
}

/// Temat odpowiedzi — dokładamy `Re:` tylko raz.
pub fn reply_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    if trimmed.to_lowercase().starts_with("re:") {
        trimmed.to_owned()
    } else if trimmed.is_empty() {
        "Re:".to_owned()
    } else {
        format!("Re: {trimmed}")
    }
}

/// Temat przekazania — `Fwd:` bez podwajania.
pub fn forward_subject(subject: &str) -> String {
    let trimmed = subject.trim();
    let lower = trimmed.to_lowercase();
    if lower.starts_with("fwd:") || lower.starts_with("fw:") {
        trimmed.to_owned()
    } else if trimmed.is_empty() {
        "Fwd:".to_owned()
    } else {
        format!("Fwd: {trimmed}")
    }
}

/// Treść odpowiedzi z cytatem oryginału (plain text, bez HTML).
pub fn reply_body(sender: &str, date: i64, body: &str) -> String {
    let when = format_full_date(date);
    let quoted: String = body
        .lines()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    // Nagłówek cytatu jest neutralny językowo — trafia do treści wiadomości,
    // więc nie może zależeć od języka interfejsu nadawcy.
    format!("\n\n--- {sender}, {when} ---\n{quoted}")
}

/// Treść przekazania z nagłówkiem.
pub fn forward_body(sender: &str, date: i64, subject: &str, body: &str) -> String {
    let when = format_full_date(date);
    format!(
        "\n\n---------- Przekazana wiadomość ----------\nOd: {sender}\nData: {when}\nTemat: {subject}\n\n{body}"
    )
}

/// Wybiera pierwszą niepustą linię tematu do nagłówka okna.
pub fn window_title(folder: &str, unread: usize) -> String {
    if unread > 0 {
        format!("{folder} ({unread})")
    } else {
        folder.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(to: &str) -> Draft {
        Draft {
            to: to.to_owned(),
            ..Draft::default()
        }
    }

    #[test]
    fn address_check_accepts_normal_and_display_forms() {
        assert!(looks_like_address("ala@example.org"));
        assert!(looks_like_address("ala.nowak+tag@mail.example.co.uk"));
        assert!(!looks_like_address("ala"));
        assert!(!looks_like_address("ala@example"));
        assert!(!looks_like_address("ala @example.org"));
        assert!(!looks_like_address(".ala@example.org"));
    }

    #[test]
    fn addresses_are_split_on_commas_and_semicolons() {
        let parsed = parse_addresses("Ania <ania@example.org>, bob@example.org; Cytryn");
        assert_eq!(
            parsed,
            vec!["ania@example.org", "bob@example.org", "Cytryn"]
        );
    }

    #[test]
    fn draft_requires_at_least_one_valid_recipient() {
        assert_eq!(validate_draft(&draft("")), Err(DraftError::NoRecipients));
        assert_eq!(
            validate_draft(&draft("wpisz tutaj")),
            Err(DraftError::BadRecipient)
        );
        assert_eq!(
            validate_draft(&draft("ala@example.org")),
            Ok(vec!["ala@example.org".to_owned()])
        );
    }

    #[test]
    fn reply_subject_does_not_double_the_prefix() {
        assert_eq!(reply_subject("Raport"), "Re: Raport");
        assert_eq!(reply_subject("re: Raport"), "re: Raport");
        assert_eq!(reply_subject(""), "Re:");
    }

    #[test]
    fn initials_take_two_words_and_fall_back_to_at() {
        assert_eq!(initials("Patryk Szkudlarek"), "PS");
        assert_eq!(initials("Ala Nowak"), "AN");
        assert_eq!(initials("  ania  "), "A");
        assert_eq!(initials(""), "@");
        // Pojedyncze słowo daje jedną literę — dwa znaki tylko z dwóch słów.
        assert_eq!(initials("123"), "1");
    }

    #[test]
    fn plural_matches_polish_rules() {
        assert_eq!(plural(1, "a", "b", "c"), "a");
        assert_eq!(plural(3, "a", "b", "c"), "b");
        assert_eq!(plural(5, "a", "b", "c"), "c");
        assert_eq!(plural(12, "a", "b", "c"), "c");
        assert_eq!(plural(22, "a", "b", "c"), "b");
    }

    #[test]
    fn snippet_skips_quotes_and_shortens() {
        let body = "> stary cytat\n\nNowa treść\nciąg dalszy";
        assert_eq!(snippet(body, 30), "Nowa treść ciąg dalszy");
        assert_eq!(snippet(body, 4), "Nowa…");
    }

    #[test]
    fn when_is_time_today_and_date_otherwise() {
        let now = 1_700_000_000; // 2023-11-14 22:13 UTC
        assert_eq!(format_when(now, now), "22:13");
        assert_eq!(format_when(now - 86_400, now), "13.11");
        assert_eq!(format_when(now - 400 * 86_400, now), "10.10.2022");
        assert_eq!(format_when(0, now), "");
    }

    #[test]
    fn sizes_are_human_readable() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn folder_kind_survives_server_naming() {
        assert_eq!(FolderKind::detect("INBOX"), FolderKind::Inbox);
        assert_eq!(FolderKind::detect("[Gmail]/Wysłane"), FolderKind::Sent);
        assert_eq!(FolderKind::detect("Sent Items"), FolderKind::Sent);
        assert_eq!(FolderKind::detect("Kosz"), FolderKind::Trash);
        assert_eq!(FolderKind::detect("Projekty"), FolderKind::Custom);
    }
}
