# Selene — kompozytor Wayland środowiska Luna

**English** — Selene is the Wayland compositor at the heart of the **Luna** desktop. It is
written in **pure Rust** and deliberately **does not depend on smithay**: the protocol is
handled by our own thin layer on top of `wayland-server` (core + xdg-shell), and the event
loop is a direct `poll` over the listening socket and the backend descriptor. `libLuna` is a
normal crate dependency (`libluna-core`, `libluna-render`, `libluna-i18n`).

**Polski** — Selene to kompozytor Wayland będący sercem środowiska **Luna**. Napisany w
**czystym Rust** i świadomie **bez zależności od smithay**: protokół obsługuje własna cienka
warstwa na `wayland-server` (rdzeń + xdg-shell), a pętla zdarzeń to bezpośredni `poll` po
gnieździe nasłuchu i deskryptorze backendu. `libLuna` jest zwykłą zależnością crate'ową.

## Nazewnictwo / Naming

| Warstwa / Layer | Nazwa / Name | Opis |
| --- | --- | --- |
| Toolkit GUI | **libLuna** | widgety, render, i18n (`crates/libluna-*`) |
| Środowisko | **Luna** | pulpit (panel, launcher, ustawienia) — `de/luna` (wkrótce) |
| Kompozytor | **Selene** | serwer Wayland — `de/selene` |

## Co już działa / What works (M-K0)

- Globalne interfejsy: `wl_compositor` (v4), `wl_shm`, `xdg_wm_base` (v1), `wl_seat` (v1).
- Cykl życia powierzchni: `wl_surface` (`attach`/`damage`/`frame`/`commit`), `wl_callback`
  (wywołanie `done` przy commicie), `wl_region`.
- `wl_shm`: `create_pool` → `wl_shm_pool.create_buffer` → `wl_buffer` (metadane bufora).
- xdg-shell: `get_xdg_surface` → `get_toplevel`, pierwszy `commit` wysyła `configure`
  (oraz `xdg_toplevel.configure`), `ack_configure`, `set_title`/`set_app_id`.
- Minimalny `wl_seat` (obiekty wejścia tworzone, bez zdarzeń).
- Wielojęzyczność: lokalizacja z otoczenia + `i18n/` ładowane w runtime (`libluna-i18n`).
- Optymalizacja: `[profile.release]` z pełnym LTO i 1 jednostką kodegenu.

## Jeszcze nie / Not yet

- **Wyjście/renderowanie**: integracja `libluna-render` i backend wyświetlania
  (nested Wayland lub DRM/KMS). Dziś kompozytor przyjmuje klientów, ale nie pokazuje okien.
- **Wejście**: zdarzenia klawiatury/wskaźnika (`libinput`/evdev) i `wl_pointer`/`wl_keyboard`.
- Protokoły dodatkowe: `xdg_popup`, `wl_output`, `layer-shell`, `xdg-decoration`, clipboard.
- Zarządzanie oknami (układ, fokus, warstwy) — to warstwa **Luna** (`de/luna`).

## Uruchomienie / Running

```sh
# katalog runtime z prawem 0700
mkdir -p .selene-runtime && chmod 700 .selene-runtime
XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run -p selene
# → "Selene compositor started (wayland-0)" + gniazdo wayland-0 w katalogu runtime
```

Klienta podłączysz ustawiając `WAYLAND_DISPLAY=wayland-0` i `XDG_RUNTIME_DIR` na ten sam
katalog (renderowanie pojawi się, gdy wejdzie backend wyświetlania).

## Architektura / Architecture

```
de/selene/src/
  main.rs        # display, globale, gniazdo, pętla poll
  state.rs       # stan współdzielony (czasy, seriale, liczniki)
  compositor.rs  # wl_compositor / wl_surface / wl_region / wl_callback
  shm.rs         # wl_shm / wl_shm_pool / wl_buffer
  shell.rs       # xdg_wm_base / xdg_surface / xdg_toplevel / xdg_positioner
  seat.rs        # wl_seat (minimalnie)
```
