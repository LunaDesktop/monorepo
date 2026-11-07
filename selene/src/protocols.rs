//! Protokoły spoza `wayland-protocols`, generowane lokalnie z XML.
//!
//! `zwp_text_input_v3` jest w `wayland-protocols`, ale jego drugiej połowy —
//! `zwp_input_method_v2` — tam nie ma (projekt protokołu nie został jeszcze
//! włączony do repozytorium wayland-protocols). Fcitx5 i inne implementacje
//! wożą ten sam plik XML, więc robimy to samo. `zwp_virtual_keyboard_v1` jest
//! potrzebny, aby input method mogła odesłać klawisze, których nie skonsumowała.
//!
//! Pliki XML leżą w `selene/protocols/` (licencja MIT, nagłówki w plikach).

pub mod input_method_v2 {
    //! `zwp_input_method_v2`, `zwp_input_popup_surface_v2`,
    //! `zwp_input_method_keyboard_grab_v2`, `zwp_input_method_manager_v2`.

    // `input-method-v2.xml` odwołuje się do enumów `zwp_text_input_v3`.
    pub use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3;

    pub mod __interfaces {
        use wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("./protocols/input-method-unstable-v2.xml");
    }
    use self::__interfaces::*;
    use wayland_server;
    use wayland_server::protocol::*;

    wayland_scanner::generate_server_code!("./protocols/input-method-unstable-v2.xml");
}

pub mod virtual_keyboard_v1 {
    //! `zwp_virtual_keyboard_v1`, `zwp_virtual_keyboard_manager_v1`.

    pub mod __interfaces {
        use wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("./protocols/virtual-keyboard-unstable-v1.xml");
    }
    use self::__interfaces::*;
    use wayland_server;
    use wayland_server::protocol::*;

    wayland_scanner::generate_server_code!("./protocols/virtual-keyboard-unstable-v1.xml");
}
