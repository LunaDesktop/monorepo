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

## Co już działa / What works (M-K2)

- Globalne interfejsy: `wl_compositor` (v4), `wl_shm`, **`wl_output`** (v4), `xdg_wm_base` (v1),
  **`wl_seat`** (v1) z **wskaźnikiem i klawiaturą**.
- Cykl życia powierzchni: `wl_surface` (`attach`/`damage`/`frame`/`commit`), `wl_callback`
  (wywołanie `done` przy commicie), `wl_region`.
- `wl_shm`: `create_pool` → `wl_shm_pool.create_buffer` → `wl_buffer`; piksele bufora są
  odczytywane (`mmap` puli, swizzle BGRA→RGBA) i wstawiane do modelu okien.
- xdg-shell: `get_xdg_surface` → `get_toplevel`, pierwszy `commit` wysyła `configure`
  (oraz `xdg_toplevel.configure`), `ack_configure`, `set_title`/`set_app_id`.
- **Wyjście nested**: `libluna-platform` otwiera okno na nadrzędnym kompozytorze (osobny
  wątek), a `libluna-render` komponuje scenę (tło + okna w kolejności `z`). Renderer ma
  prymityw `Scene::image`/`ImageData`.
- **Wejście**: magistrala (kolejka + para gniazd do wybudzania `poll`). Wątek wyjścia
  przekazuje zdarzenia od rodzica; protokół trasuje je do klientów:
  `wl_pointer` (`enter`/`motion`/`button`/`axis`/`leave`) i `wl_keyboard` (`keymap`
  z wbudowanej mapy XKB, `enter`/`leave`/`key`/`modifiers` z syntezą modyfikatorów).
- **Zarządzanie oknami (pływające)**: trafienie w okno = fokus + podniesienie (`z`),
  przeciągnięcie lewym przyciskiem przenosi okno; kaskada przy starcie.
- **Monitory**: model `Vec<OutputInfo>` (gotowy na wiele); okno nested = jeden monitor
  (rozmiar śledzony z konfiguracji okna).
- Wielojęzyczność (`libluna-i18n`) i optymalizacja (`[profile.release]` LTO).

## Jeszcze nie / Not yet

- **Wiele monitorów**: realne wyjścia DRM/KMS i przypisywanie okien do monitorów (dziś jeden).
- **Dekoracje/zachowanie okien**: brak `xdg-decoration`, resize krawędziami, minimalizacji,
  pełnego ekranu, `xdg_popup`, `layer-shell`.
- **Klawiatura**: mapa `us` przez systemowe dane XKB (`/usr/share/X11/xkb`); inne układy,
  IME i `repeat_info` — do zrobienia.
- Schowek (`wl_data_device`), `wl_output` dynamiczne (zmiana trybu), fokus klawiatury na klik
  poza oknem, SnapEdge’y/układ.
- Optymalizacje: cache mapy `shm`, brak klonowania pikseli na klatkę (cache `ImageData`),
  render tylko przy zmianie modelu. Backend sprzętowy (DRM/KMS + GLES).

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
  main.rs        # display, globale, gniazdo, pętla poll (nasłuch + backend + magistrala)
  state.rs       # stan: model okien, z-order, fokus, drag, monitory
  compositor.rs  # wl_compositor / wl_surface / wl_region / wl_callback + kompozycja
  shm.rs         # wl_shm / wl_shm_pool / wl_buffer (+ odczyt pikseli)
  shell.rs       # xdg_wm_base / xdg_surface / xdg_toplevel / xdg_positioner
  monitors.rs    # wl_output (model wielu monitorów)
  seat.rs        # wl_seat: wskaźnik + klawiatura + trasowanie wejścia
  input.rs       # magistrala wejścia (kolejka + wybudzanie)
  output.rs      # wyjście nested (libluna-platform + libluna-render)
```
