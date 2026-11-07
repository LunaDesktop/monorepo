//! `wp_pointer_warp_v1` — przenoszenie kursora w obrębie powierzchni.
//!
//! Używają tego aplikacje X11 przez Xwaylanda, Java/AWT (warp przy otwieraniu menu
//! i przewijaniu list) oraz Wine. Bez globalu te aplikacje „nie trafiają” kursorem
//! w element, który właśnie przesunęły.

use wayland_protocols::wp::pointer_warp::v1::server::wp_pointer_warp_v1;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New};

use crate::state::State;

impl GlobalDispatch<wp_pointer_warp_v1::WpPointerWarpV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<wp_pointer_warp_v1::WpPointerWarpV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<wp_pointer_warp_v1::WpPointerWarpV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &wp_pointer_warp_v1::WpPointerWarpV1,
        request: wp_pointer_warp_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let wp_pointer_warp_v1::Request::WarpPointer { surface, x, y, .. } = request {
            // `pointer` i `serial` przyjmujemy, ale nie walidujemy: kompozytor ma
            // prawo odmówić (i tak robimy, gdy punkt wypada poza okno), a klienty
            // wysyłają tu własny serial `enter`, którego nie indeksujemy.
            crate::seat::warp_pointer(state, &surface, x, y);
        }
    }
}
