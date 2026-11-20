//! Powiadomienia systemowe o nowej poczcie.
//!
//! Wysyłamy je bezpośrednio przez `org.freedesktop.Notifications` (serwerem
//! w Lunie jest `luna-shell`), a nie przez `notify-send`. Hint
//! `desktop-entry` sprawia, że kliknięcie toasta aktywuje Luna Mail.
//!
//! Wysyłka idzie w osobnym wątku: magistrala może chwilowo nie odpowiadać,
//! a pętla rysowania nie może się przez to zatrzymać.

use std::collections::HashMap;

use libluna_i18n::{tr, tr_args};
use zbus::zvariant::Value;

const APP_NAME: &str = "Luna Mail";
const DESKTOP_ENTRY: &str = "eu.luna.Email.desktop";
const ICON: &str = "internet-mail";
/// Toasty nie są miejscem na cały temat wiadomości.
const BODY_LIMIT: usize = 160;

/// Powiadomienie o nowej wiadomości (albo kilku).
///
/// `latest` to `(nadawca, temat)` najnowszej wiadomości — pokazujemy je, żeby
/// użytkownik wiedział, czy warto przełączyć się do klienta.
pub fn new_mail(count: usize, latest: Option<(&str, &str)>) {
    let summary = if count == 1 {
        tr("New message")
    } else {
        tr_args("{n} new messages", &[("n", &count.to_string())])
    };
    let body = match latest {
        Some((sender, subject)) => {
            let sender = sender.trim();
            let subject = shorten(subject);
            if sender.is_empty() {
                subject
            } else if subject.is_empty() {
                shorten(sender)
            } else {
                format!("{sender}: {subject}")
            }
        }
        None => tr("Open Luna Mail to read it"),
    };
    send(summary, body, 8000, false);
}

/// Wysyła powiadomienie bez blokowania UI.
fn send(summary: String, body: String, timeout_ms: i32, critical: bool) {
    std::thread::spawn(move || {
        if let Err(error) = send_blocking(&summary, &body, timeout_ms, critical) {
            eprintln!("luna-email: nie udało się wysłać powiadomienia: {error}");
        }
    });
}

fn send_blocking(summary: &str, body: &str, timeout_ms: i32, critical: bool) -> Result<(), String> {
    let connection =
        zbus::blocking::Connection::session().map_err(|error| format!("D-Bus: {error}"))?;
    let open_label = tr("Open Luna Mail");
    let actions: Vec<&str> = vec!["default", open_label.as_str()];
    let urgency: u8 = if critical { 2 } else { 1 };
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    hints.insert("desktop-entry", Value::from(DESKTOP_ENTRY));
    hints.insert("urgency", Value::from(urgency));
    hints.insert("image-path", Value::from(ICON));
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                APP_NAME, 0u32, ICON, summary, body, actions, hints, timeout_ms,
            ),
        )
        .map_err(|error| format!("D-Bus: {error}"))?;
    Ok(())
}

/// Skraca tekst do jednej linii o rozsądnej długości.
fn shorten(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for ch in text.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if ch == ' ' {
            if last_was_space || out.is_empty() {
                continue;
            }
            last_was_space = true;
        } else {
            last_was_space = false;
        }
        out.push(ch);
        if out.chars().count() >= BODY_LIMIT {
            out.push('…');
            break;
        }
    }
    out.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_collapses_whitespace() {
        assert_eq!(shorten("  ala\n\tma   kota "), "ala ma kota");
    }

    #[test]
    fn shorten_limits_long_text() {
        let text = "x".repeat(BODY_LIMIT * 2);
        let out = shorten(&text);
        assert_eq!(out.chars().count(), BODY_LIMIT + 1);
        assert!(out.ends_with('…'));
    }
}
