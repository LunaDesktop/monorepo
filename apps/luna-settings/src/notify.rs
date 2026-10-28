//! Testowe powiadomienie z Ustawień.
//!
//! Korzystamy bezpośrednio z D-Busa (`org.freedesktop.Notifications`), a nie
//! z `notify-send` — dzięki temu przycisk działa na każdej instalacji Luny,
//! o ile działa daemon powiadomień `luna-shell`.

use std::collections::HashMap;

use zbus::zvariant::Value;

/// Wysyła powiadomienie do sesyjnego daemona powiadomień.
pub fn send_test_notification(title: &str, body: &str) -> Result<(), String> {
    let connection =
        zbus::blocking::Connection::session().map_err(|err| format!("D-Bus: {err}"))?;
    let actions: Vec<&str> = Vec::new();
    let hints: HashMap<&str, Value<'_>> = HashMap::new();
    connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Luna Settings",
                0u32,
                "dialog-information",
                title,
                body,
                actions,
                hints,
                5000i32,
            ),
        )
        .map_err(|err| format!("D-Bus: {err}"))?;
    Ok(())
}
