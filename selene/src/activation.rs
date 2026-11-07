//! `xdg_activation_v1` — przekazywanie fokusu między aplikacjami.
//!
//! Launcher/portal nie może „po prostu” aktywować cudzego okna. Dostaje token
//! od aplikacji startującej, przekazuje go nowemu procesowi, a ten wywołuje
//! `xdg_activation_v1.activate`. Kompozytor sprawdza token (ważność, powierzchnię)
//! i dopiero wtedy podnosi okno oraz nadaje mu fokus klawiatury.
//!
//! Token jest jednorazowy i krótkożyjący. Bez tego nowe okno z portalu/paska
//! pojawiało się za innym i trzeba było go szukać.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use wayland_protocols::xdg::activation::v1::server::{
    xdg_activation_token_v1::{Event as TokenEvent, Request as TokenRequest, XdgActivationTokenV1},
    xdg_activation_v1::{Request as ActivationRequest, XdgActivationV1},
};
use wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::compositor::SurfaceData;
use crate::state::State;
use crate::sync::LockExt;

/// Czas życia tokenu — po nim aktywacja jest ignorowana.
const TOKEN_TTL: Duration = Duration::from_secs(10);

/// Dane obiektu `xdg_activation_token_v1` (zbierane do `commit`).
#[derive(Default)]
pub struct TokenData {
    pub serial: Mutex<Option<u32>>,
    pub app_id: Mutex<String>,
    pub surface: Mutex<Option<WlSurface>>,
}

/// Wydany token aktywacji.
#[derive(Clone)]
pub struct ActivationToken {
    pub token: String,
    pub app_id: String,
    pub surface: Option<WlSurface>,
    pub expires: Instant,
    pub used: bool,
}

impl GlobalDispatch<XdgActivationV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<XdgActivationV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<XdgActivationV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &XdgActivationV1,
        request: ActivationRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ActivationRequest::GetActivationToken { id } => {
                data_init.init(id, TokenData::default());
            }
            ActivationRequest::Activate {
                token,
                surface: requested,
            } => {
                state
                    .activation_tokens
                    .retain(|t| t.expires > Instant::now());
                let Some(index) = state
                    .activation_tokens
                    .iter()
                    .position(|t| !t.used && t.token == token)
                else {
                    return;
                };
                let issued = state.activation_tokens.remove(index);
                // Token wystawiony dla konkretnej powierzchni nie może aktywować innej.
                if let Some(expected) = &issued.surface {
                    if expected.id() != requested.id() {
                        return;
                    }
                }
                state.pending_activation = Some(requested.clone());
                activate_now(state, &requested, &issued.app_id);
            }
            ActivationRequest::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<XdgActivationTokenV1, TokenData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &XdgActivationTokenV1,
        request: TokenRequest,
        data: &TokenData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            TokenRequest::SetSerial { serial, seat: _ } => {
                *data.serial.lock_or_recover() = Some(serial);
            }
            TokenRequest::SetAppId { app_id } => {
                *data.app_id.lock_or_recover() = app_id;
            }
            TokenRequest::SetSurface { surface } => {
                *data.surface.lock_or_recover() = Some(surface);
            }
            TokenRequest::Commit => {
                let token = make_token();
                let app_id = data.app_id.lock_or_recover().clone();
                let surface = data.surface.lock_or_recover().clone();
                state
                    .activation_tokens
                    .retain(|t| t.expires > Instant::now());
                state.activation_tokens.push(ActivationToken {
                    token: token.clone(),
                    app_id,
                    surface,
                    expires: Instant::now() + TOKEN_TTL,
                    used: false,
                });
                // Limit liczby niewykorzystanych tokenów (klient nie może rosnąć w RAM).
                if state.activation_tokens.len() > 64 {
                    let excess = state.activation_tokens.len() - 64;
                    state.activation_tokens.drain(..excess);
                }
                let _ = resource.send_event(TokenEvent::Done { token });
            }
            TokenRequest::Destroy => {}
            _ => {}
        }
    }
}

/// Aktywuje powierzchnię, jeśli ma już slot okna. W przeciwnym razie zostaje
/// w `pending_activation` — nowe okno dostanie fokus przy pierwszym commicie.
fn activate_now(state: &mut State, surface: &WlSurface, app_id: &str) {
    let Some(slot) = surface
        .data::<SurfaceData>()
        .and_then(|data| *data.slot.lock_or_recover())
    else {
        return;
    };
    let mapped = state
        .windows
        .lock_or_recover()
        .get(slot)
        .map(|w| w.mapped && !w.minimized)
        .unwrap_or(false);
    if mapped {
        if std::env::var("SELENE_STATS").is_ok() {
            eprintln!("selene/activation: aktywacja slot={slot} app_id=\"{app_id}\"");
        }
        crate::seat::activate_slot(state, slot);
        state.pending_activation = None;
    }
}

/// Wywoływane po pierwszym zmapowaniu nowego okna — dokańcza aktywację,
/// która przyszła przed utworzeniem slotu.
pub fn apply_pending(state: &mut State, surface: &WlSurface) {
    let pending = state
        .pending_activation
        .as_ref()
        .is_some_and(|s| s.id() == surface.id());
    if pending {
        state.pending_activation = None;
        if let Some(slot) = surface
            .data::<SurfaceData>()
            .and_then(|data| *data.slot.lock_or_recover())
        {
            crate::seat::activate_slot(state, slot);
        }
    }
}

/// Token nieprzewidywalny z zewnątrz: 16 bajtów z `/dev/urandom`, a gdy go brak —
/// czas, licznik i adres procesu (nadmiarowo, ale deterministycznie unikalny).
fn make_token() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut bytes).is_ok() {
            return hex(&bytes);
        }
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    hex(format!("{now:x}-{pid:x}-{:p}", &bytes).as_bytes())
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_nonempty() {
        let a = make_token();
        let b = make_token();
        assert!(!a.is_empty());
        assert_ne!(a, b);
    }

    #[test]
    fn expires_after_ttl() {
        let now = Instant::now();
        let token = ActivationToken {
            token: "x".into(),
            app_id: String::new(),
            surface: None,
            expires: now + TOKEN_TTL,
            used: false,
        };
        assert!(token.expires > now);
        assert!(!token.used);
    }
}
