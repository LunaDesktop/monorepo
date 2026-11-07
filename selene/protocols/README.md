# Vendored protokoły Waylanda

Te pliki XML nie są częścią crate'a `wayland-protocols` w wersji używanej przez
Selene, a są potrzebne do działania IME:

| Plik | Interfejsy | Źródło |
|---|---|---|
| `input-method-unstable-v2.xml` | `zwp_input_method_v2`, `zwp_input_popup_surface_v2`, `zwp_input_method_keyboard_grab_v2`, `zwp_input_method_manager_v2` | fcitx5, `src/lib/fcitx-wayland/input-method-v2/` |
| `virtual-keyboard-unstable-v1.xml` | `zwp_virtual_keyboard_v1`, `zwp_virtual_keyboard_manager_v1` | fcitx5, `src/lib/fcitx-wayland/input-method-v2/` |

Oba pliki są objęte licencją MIT (nagłówki `copyright` w plikach). Kod
serwerowy generuje `wayland-scanner` w `src/protocols.rs` — bez `build.rs`.

`zwp_text_input_v3` pochodzi z `wayland-protocols` (feature `unstable`) i nie
wymaga kopiowania.
