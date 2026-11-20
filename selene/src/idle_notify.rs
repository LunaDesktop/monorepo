//! `ext_idle_notifier_v1` — klienci dowiadują się, kiedy użytkownik jest bezczynny.
//!
//! Bez tego protokołu odtwarzacze i przeglądarki nie mają jak się zorientować,
//! że nikt nie rusza myszą: mogą co najwyżej blokować wygaszanie
//! (`zwp_idle_inhibit_v1`), ale nie wiedzą, kiedy schować kontrolki albo
//! wstrzymać odtwarzanie.
//!
//! Powiadomienia korzystają z tego samego zegara co wygaszanie ekranu
//! (`State::idle_last_input`), więc `resumed` przychodzi dokładnie wtedy, gdy
//! użytkownik wrócił. Inhibitor (wideo, prezentacja) wstrzymuje także je —
//! zgodnie z `zwp_idle_inhibit_v1`.

use std::time::Duration;

use wayland_protocols::ext::idle_notify::v1::server::{
    ext_idle_notification_v1::{Event as NotificationEvent, ExtIdleNotificationV1},
    ext_idle_notifier_v1::{ExtIdleNotifierV1, Request},
};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::state::State;

/// Jedno zarejestrowane powiadomienie o bezczynności.
///
/// Trzymamy próg i stan „czy wysłaliśmy już `idled`” obok zasobu, bo dane
/// użytkownika w `wayland-server` są tylko do odczytu — a potrzebujemy zmiany.
pub struct IdleNotification {
    pub resource: ExtIdleNotificationV1,
    /// Próg w milisekundach (`0` = natychmiast, zgodnie z protokołem).
    pub timeout_ms: u32,
    /// Czy klient dostał `idled` i czeka na `resumed`.
    pub idled: bool,
}

impl GlobalDispatch<ExtIdleNotifierV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ExtIdleNotifierV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ExtIdleNotifierV1,
        request: Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            Request::GetIdleNotification {
                id,
                seat: _,
                timeout,
            } => {
                register(state, data_init, id, timeout);
            }
            Request::GetInputIdleNotification {
                id,
                seat: _,
                timeout,
            } => {
                // Selene ma jeden wspólny zegar wejścia (klawiatura + wskaźnik),
                // więc powiadomienie „per urządzenie” ma ten sam próg.
                register(state, data_init, id, timeout);
            }
            Request::Destroy => {}
            _ => {}
        }
    }
}

fn register(
    state: &mut State,
    data_init: &mut DataInit<'_, State>,
    id: New<ExtIdleNotificationV1>,
    timeout: u32,
) {
    let resource = data_init.init(id, ());
    state.idle_notifications.retain(|n| n.resource.is_alive());
    state.idle_notifications.push(IdleNotification {
        resource,
        timeout_ms: timeout,
        idled: false,
    });
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ExtIdleNotificationV1,
        request: <ExtIdleNotificationV1 as Resource>::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let wayland_protocols::ext::idle_notify::v1::server::ext_idle_notification_v1::Request::Destroy =
            request
        {
            state
                .idle_notifications
                .retain(|n| n.resource.id() != resource.id());
        }
    }
}

/// Ile milisekund do najbliższego progu powiadomienia (`None` = brak klientów).
pub fn wait_ms(state: &State) -> Option<i64> {
    let idle = state.idle_last_input.elapsed();
    let mut best: Option<i64> = None;
    for notification in &state.idle_notifications {
        let timeout = Duration::from_millis(u64::from(notification.timeout_ms));
        let left = timeout
            .saturating_sub(idle)
            .as_millis()
            .min(i64::MAX as u128) as i64;
        best = Some(best.map_or(left, |current: i64| current.min(left)));
    }
    // `idled` musi wyjść natychmiast po przekroczeniu progu, więc budzimy się od razu.
    best.map(|ms| ms.max(0))
}

/// Czy przy danej bezczynności próg został przekroczony.
pub fn is_due(idle: Duration, timeout_ms: u32) -> bool {
    idle >= Duration::from_millis(u64::from(timeout_ms))
}

/// Wysyła `idled`/`resumed` według bieżącej bezczynności.
///
/// Wywoływane w pętli głównej (razem z `tick_idle`) — gdy nie ma klientów,
/// funkcja kończy się od razu.
pub fn tick(state: &mut State) {
    if state.idle_notifications.is_empty() {
        return;
    }
    // Inhibitor wstrzymuje powiadomienia: klient, który sam zablokował
    // wygaszanie (np. odtwarza film), nie powinien dostawać `idled`.
    let idle = if crate::idle_inhibit::inhibited(state) {
        Duration::ZERO
    } else {
        state.idle_last_input.elapsed()
    };
    for notification in &mut state.idle_notifications {
        let should_idle = is_due(idle, notification.timeout_ms);
        if should_idle && !notification.idled {
            notification.idled = true;
            let _ = notification.resource.send_event(NotificationEvent::Idled);
        } else if !should_idle && notification.idled {
            notification.idled = false;
            let _ = notification.resource.send_event(NotificationEvent::Resumed);
        }
    }
    state
        .idle_notifications
        .retain(|notification| notification.resource.is_alive());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_clients_means_no_wakeup() {
        assert_eq!(wait_ms(&State::default()), None);
    }

    #[test]
    fn due_threshold_matches_the_protocol() {
        // Próg 0 ms = natychmiast (tak działa `ext_idle_notifier_v1`).
        assert!(is_due(Duration::from_millis(0), 0));
        assert!(!is_due(Duration::from_millis(999), 1000));
        assert!(is_due(Duration::from_millis(1000), 1000));
        assert!(is_due(Duration::from_secs(5), 1000));
    }
}
