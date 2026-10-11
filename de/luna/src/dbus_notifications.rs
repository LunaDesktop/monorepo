//! Usługa powiadomień D‑Bus (`org.freedesktop.Notifications`).
//!
//! Powłoka rejestruje się na sesyjnej magistrali jako **daemon powiadomień**, więc
//! aplikacje (np. `notify-send`, klienci bibliotek) trafiają wprost do toastów Luny.
//! Serwer obsługuje magistralę we własnym wątku (zbus, API blokujące), a UI opróżnia
//! kolejkę w [`NotificationsBus::drain`] i przerysowuje okno po wybudzeniu
//! ([`WakeHandle`]) — powiadomienie pojawia się więc także, gdy powłoka śpi.
//!
//! Gdy nazwę `org.freedesktop.Notifications` trzyma już inny daemon (np. w sesji
//! Xfce obok Luny), [`NotificationsBus::spawn`] zwraca `None` i nic nie przejmujemy.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use libluna_platform::WakeHandle;
use zbus::zvariant::OwnedValue;

/// Żądanie powiadomienia odebrane z magistrali.
#[derive(Debug, Clone, Default)]
pub struct Incoming {
    /// Identyfikator nadany przez nas (do `NotificationClosed`/`ActionInvoked`).
    pub id: u32,
    /// Nazwa aplikacji wysyłającej.
    pub app_name: String,
    /// Podsumowanie (tytuł toasta).
    pub summary: String,
    /// Treść.
    pub body: String,
    /// Czas życia w ms: `-1` = domyślny serwera, `0` = trwałe, `> 0` = jawny.
    pub expire_ms: i32,
    /// Klucze akcji (bez etykiet) do `ActionInvoked`.
    pub actions: Vec<String>,
}

impl Incoming {
    /// Czas życia w sekundach (dla [`crate::notifications::Notification`]).
    pub fn timeout_secs(&self) -> f64 {
        match self.expire_ms {
            0 => 0.0,
            ms if ms > 0 => ms as f64 / 1000.0,
            _ => 5.0,
        }
    }
}

/// Kolejka odebranych powiadomień (współdzielona z wątkiem magistrali).
type Queue = Arc<Mutex<Vec<Incoming>>>;

/// Interfejs `org.freedesktop.Notifications` (serwer).
struct Server {
    queue: Queue,
    waker: WakeHandle,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".to_owned()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        _replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        expire_ms: i32,
    ) -> u32 {
        let keys: Vec<String> = actions.chunks_exact(2).map(|c| c[0].clone()).collect();
        let id = {
            let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
            let id = q.iter().map(|i| i.id).max().unwrap_or(0) + 1;
            q.push(Incoming {
                id,
                app_name,
                summary,
                body,
                expire_ms,
                actions: keys,
            });
            id
        };
        // Obudź pętlę powłoki, żeby nowy toast został narysowany.
        self.waker.wake();
        id
    }

    fn close_notification(&self, _id: u32) {}

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Luna".to_owned(),
            "libLuna".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            "1.2".to_owned(),
        )
    }
}

/// Uchwyt do usługi powiadomień (utrzymuje połączenie i kolejkę).
pub struct NotificationsBus {
    queue: Queue,
    _conn: zbus::blocking::Connection,
}

impl NotificationsBus {
    /// Rejestruje powłokę jako daemon powiadomień na sesyjnej magistrali.
    ///
    /// `None`, gdy brak magistrali albo nazwę `org.freedesktop.Notifications` trzyma
    /// już inny daemon. `waker` budzi pętlę UI po każdym odebranym powiadomieniu.
    pub fn spawn(waker: WakeHandle) -> Option<Self> {
        let queue: Queue = Arc::new(Mutex::new(Vec::new()));
        let server = Server {
            queue: Arc::clone(&queue),
            waker,
        };
        let conn = zbus::blocking::connection::Builder::session()
            .ok()?
            .name("org.freedesktop.Notifications")
            .ok()?
            .serve_at("/org/freedesktop/Notifications", server)
            .ok()?
            .build()
            .ok()?;
        Some(Self { queue, _conn: conn })
    }

    /// Opróżnia kolejkę (UI woła raz na klatkę).
    pub fn drain(&self) -> Vec<Incoming> {
        let mut q = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *q)
    }

    /// Sygnalizuje zamknięcie powiadomienia (`NotificationClosed`).
    ///
    /// `reason`: 1 = wygasło, 2 = zamknięte przez użytkownika, 3 = `CloseNotification`.
    pub fn signal_closed(&self, id: u32, reason: u32) {
        let _ = self._conn.emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "NotificationClosed",
            &(id, reason),
        );
    }

    /// Sygnalizuje wywołanie akcji (`ActionInvoked`).
    pub fn signal_action(&self, id: u32, key: &str) {
        let _ = self._conn.emit_signal(
            None::<&str>,
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "ActionInvoked",
            &(id, key),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_maps_special_values() {
        let with = |ms| {
            Incoming {
                expire_ms: ms,
                ..Default::default()
            }
            .timeout_secs()
        };
        assert_eq!(with(-1), 5.0);
        assert_eq!(with(0), 0.0);
        assert_eq!(with(2500), 2.5);
    }
}
