//! Menu zasilania powłoki: wylogowanie, restart, wyłączenie.
//!
//! Akcje idą przez `loginctl` (logind): sesja jest kończona po identyfikatorze
//! sesji, a restart/wyłączenie to `loginctl reboot` / `loginctl poweroff`. Dzięki
//! temu całość działa w zwykłej sesji użytkownika (polkit zezwala sesji aktywnej).

use std::process::Command;

/// Akcja pozycji menu zasilania.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    /// Zablokuj ekran.
    Lock,
    /// Zakończ bieżącą sesję.
    Logout,
    /// Uruchom ponownie komputer.
    Reboot,
    /// Wyłącz komputer.
    PowerOff,
}

/// Pojedyncza pozycja menu.
#[derive(Debug, Clone)]
pub struct PowerItem {
    pub label: String,
    pub action: PowerAction,
}

/// Pozycje menu (z lokalizacją).
pub fn items() -> Vec<PowerItem> {
    vec![
        PowerItem {
            label: libluna_i18n::tr("Zablokuj ekran").into(),
            action: PowerAction::Lock,
        },
        PowerItem {
            label: libluna_i18n::tr("Wyloguj").into(),
            action: PowerAction::Logout,
        },
        PowerItem {
            label: libluna_i18n::tr("Uruchom ponownie").into(),
            action: PowerAction::Reboot,
        },
        PowerItem {
            label: libluna_i18n::tr("Wyłącz komputer").into(),
            action: PowerAction::PowerOff,
        },
    ]
}

/// Wykonuje akcję (`loginctl` w tle; powłoka zaraz zniknie).
pub fn perform(action: PowerAction) {
    match action {
        PowerAction::Lock => spawn("loginctl", &["lock-session".to_owned()]),
        PowerAction::Logout => {
            if let Some(args) = logout_args(
                std::env::var("XDG_SESSION_ID").ok().as_deref(),
                std::env::var("USER").ok().as_deref(),
            ) {
                spawn("loginctl", &args);
            }
        }
        PowerAction::Reboot => spawn("loginctl", &["reboot".to_owned()]),
        PowerAction::PowerOff => spawn("loginctl", &["poweroff".to_owned()]),
    }
}

/// Argumenty `loginctl` do wylogowania: najpierw sesja, w ostateczności cały user.
///
/// Rozdzielone od [`perform`], żeby dało się to przetestować bez uruchamiania czegokolwiek.
fn logout_args(session_id: Option<&str>, user: Option<&str>) -> Option<Vec<String>> {
    match (
        session_id.filter(|s| !s.is_empty()),
        user.filter(|u| !u.is_empty()),
    ) {
        (Some(id), _) => Some(vec!["terminate-session".to_owned(), id.to_owned()]),
        (None, Some(u)) => Some(vec!["terminate-user".to_owned(), u.to_owned()]),
        _ => None,
    }
}

fn spawn(program: &str, args: &[String]) {
    let _ = Command::new(program).args(args).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logout_prefers_the_session() {
        assert_eq!(
            logout_args(Some("c3"), Some("patryks")),
            Some(vec!["terminate-session".into(), "c3".into()])
        );
    }

    #[test]
    fn logout_falls_back_to_user() {
        assert_eq!(
            logout_args(None, Some("patryks")),
            Some(vec!["terminate-user".into(), "patryks".into()])
        );
        assert_eq!(
            logout_args(Some(""), Some("patryks")),
            Some(vec!["terminate-user".into(), "patryks".into()])
        );
    }

    #[test]
    fn logout_without_any_target_is_a_noop() {
        assert_eq!(logout_args(None, None), None);
        assert_eq!(logout_args(Some(""), None), None);
    }

    #[test]
    fn menu_has_logout_reboot_poweroff() {
        let actions: Vec<_> = items().iter().map(|i| i.action).collect();
        assert_eq!(
            actions,
            vec![
                PowerAction::Lock,
                PowerAction::Logout,
                PowerAction::Reboot,
                PowerAction::PowerOff
            ]
        );
    }
}
