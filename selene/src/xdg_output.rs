//! `zxdg_output_manager_v1` — opis monitorów dla aplikacji.
//!
//! `wl_output` niesie tylko geometrię fizyczną; pojęcia „pozycja w przestrzeni
//! logicznej” i „nazwa złącza” (`DP-1`) żyją w tym protokole. Bez niego Qt zgłasza
//! ostrzeżenia o nieznanych ekranach, a Java/AWT, OBS i część gier widzą tylko
//! pierwszy monitor.

use wayland_protocols::xdg::xdg_output::zv1::server::{zxdg_output_manager_v1, zxdg_output_v1};
use wayland_server::protocol::wl_output::{self, WlOutput};
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

use crate::monitors::OutputInfo;
use crate::state::State;
use crate::sync::LockExt;

/// Indeks monitora i zasób `wl_output`, z którym klient powiązał `zxdg_output_v1`.
#[derive(Debug, Clone)]
pub struct XdgOutputData {
    index: usize,
    /// Potrzebny do domknięcia partii `wl_output.done` (patrz [`send_info`]).
    output: WlOutput,
}

impl GlobalDispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<zxdg_output_manager_v1::ZxdgOutputManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<zxdg_output_manager_v1::ZxdgOutputManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &zxdg_output_manager_v1::ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zxdg_output_manager_v1::Request::GetXdgOutput { id, output } = request {
            // Indeks monitora jest niesiony przez zasób `wl_output` (patrz
            // `monitors::GlobalDispatch::bind`). Brak indeksu = obcy zasób.
            let Some(index) = output.data::<usize>().copied() else {
                return;
            };
            let info = state.outputs.lock_or_recover().get(index).cloned();
            let resource = data_init.init(
                id,
                XdgOutputData {
                    index,
                    output: output.clone(),
                },
            );
            if let Some(info) = info {
                send_info(&resource, &output, &info);
            }
            state.xdg_outputs.retain(|r| r.is_alive());
            state.xdg_outputs.push(resource);
        }
    }
}

impl Dispatch<zxdg_output_v1::ZxdgOutputV1, XdgOutputData> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _resource: &zxdg_output_v1::ZxdgOutputV1,
        _request: zxdg_output_v1::Request,
        _data: &XdgOutputData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

/// Wysyła opis monitora. Zdarzenia spoza wersji obiektu nie mogą wyjść —
/// `name`/`description` są od v2, a `done` jest przestarzałe od v3 (od tej wersji
/// koniec sekwencji ogłasza `wl_output.done`).
///
/// **Kolejność jest istotna**: `wl_output.done` musi przyjść *po* naszych zdarzeniach.
/// Klient v3 (GTK4) liczy monitor przy `wl_output.done` — gdy to zdarzenie dotarło
/// wcześniej (przy samym `bind`), widział zerowy rozmiar logiczny i zerową skalę,
/// co kończyło się asercjami `scale >= 1` i segfaultem.
fn send_info(xdg: &zxdg_output_v1::ZxdgOutputV1, output: &WlOutput, info: &OutputInfo) {
    let version = xdg.version();
    let _ = xdg.send_event(zxdg_output_v1::Event::LogicalPosition {
        x: info.x,
        y: info.y,
    });
    let scale = info.scale.max(1);
    let _ = xdg.send_event(zxdg_output_v1::Event::LogicalSize {
        width: info.width / scale,
        height: info.height / scale,
    });
    if version >= 2 {
        let _ = xdg.send_event(zxdg_output_v1::Event::Name {
            name: info.name.clone(),
        });
        let _ = xdg.send_event(zxdg_output_v1::Event::Description {
            description: format!("Luna {}", info.name),
        });
    }
    if version < 3 {
        let _ = xdg.send_event(zxdg_output_v1::Event::Done);
    }
    // Domknięcie partii dla klientów v3 (i dla `wl_output` v2+, gdzie `done` istnieje).
    if output.is_alive() && output.version() >= 2 {
        let _ = output.send_event(wl_output::Event::Done);
    }
}

/// Ponawia opisy po zmianie modelu monitorów (tryb, układ, skala).
pub fn refresh(state: &mut State) {
    state.xdg_outputs.retain(|r| r.is_alive());
    let outs = state.outputs.lock_or_recover().clone();
    for resource in &state.xdg_outputs {
        let Some(data) = resource.data::<XdgOutputData>() else {
            continue;
        };
        if let Some(info) = outs.get(data.index) {
            send_info(resource, &data.output, info);
        }
    }
}
