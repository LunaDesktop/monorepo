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
| Środowisko | **Luna** | pulpit (panel, launcher, OSD) — `de/luna` |
| Kompozytor | **Selene** | serwer Wayland — `de/selene` |

## Co już działa / What works (M-K5)

- Globalne interfejsy: `wl_compositor` (v4), `wl_shm`, **`wl_output`** (v4, **jeden na monitor**),
  `xdg_wm_base` (v1), **`wl_seat`** (v1) z **wskaźnikiem i klawiaturą**, **`zxdg_decoration_manager_v1`**
  oraz **`zwlr_layer_shell_v1`**.
- Cykl życia powierzchni: `wl_surface` (`attach`/`damage`/`frame`/`commit`), `wl_callback`
  (wywołanie `done` przy commicie), `wl_region`.
- `wl_shm`: `create_pool` → `wl_shm_pool.create_buffer` → `wl_buffer`; piksele bufora są
  odczytywane bez klonowania (cache `mmap` puli, swizzle BGRA→RGBA) i wstawiane do modelu okien.
- xdg-shell: `get_xdg_surface` → `get_toplevel`/`get_popup`, pierwszy `commit` wysyła `configure`
  (oraz `xdg_toplevel.configure`), `ack_configure`, `set_title`/`set_app_id`.
- **Wyjście nested**: `libluna-platform` otwiera okno na nadrzędnym kompozytorze (osobny
  wątek), a `libluna-render` komponuje scenę (tło + okna w kolejności `z`). Renderer ma
  prymityw `Scene::image`/`ImageData` (zero-copy).
- **Wejście**: magistrala (kolejka + para gniazd do wybudzania `poll`). Wątek wyjścia
  przekazuje zdarzenia od rodzica; protokół trasuje je do klientów:
  `wl_pointer` (`enter`/`motion`/`button`/`axis`/`leave`) i `wl_keyboard` (`keymap`
  z wbudowanej mapy XKB, `enter`/`leave`/`key`/`modifiers` z syntezą modyfikatorów).
- **Zarządzanie oknami (pływające)**: trafienie w okno = fokus + podniesienie (`z`);
  **dekoracja po stronie serwera** (SSD): pasek tytułu z tytułem (`set_title`), przyciski
  **minimalizuj / maksymalizuj / zamknij**, **zmiana rozmiaru za wszystkie krawędzie i narożniki**,
  **maksymalizacja** i **pełny ekran** (okno ląduje na swoim monitorze), `xdg-decoration`
  (zawsze `server_side`). Skróty: `Super+M` (minimalizuj), `Super+F` (pełny ekran),
  `Super+↑`/`Super+↓` (maksymalizuj/przywróć), `Super+Tab` (przełącz okna).
- **`xdg_popup`**: menu/kontekstowe z pozycjonera (`anchor_rect` + offset), zamknięcie
  kliknięciem poza (`popup_done`).
- **`wlr-layer-shell`**: panele/docki — kotwice, marginesy, strefa wyłączna, warstwy
  (`background`/`bottom`/`top`/`overlay`) i wybór monitora; fundament pulpitu Luna.
- **Monitory**: model `Vec<OutputInfo>` z pozycją i układem, global `wl_output` per monitor,
  **przenośne wykrywanie DRM/KMS** (`drm.rs`: ioctl `GETRESOURCES`/`GETCONNECTOR`, tryb natywny),
  z fallbackiem na pojedyncze wyjście nested, gdy DRM niedostępne.
- **Schowek** (`wl_data_device_manager`): kopiuj/wklej między klientami — `wl_data_source`,
  `data_offer`/`offer`/`selection` rozsyłane do pozostałych urządzeń, `wl_data_offer.receive`
  przekazywane do źródła (`.send`). **Drag & drop**: `start_drag`, `enter`/`motion`/`leave`/
  `drop` do celu (z ofertą), `dnd_drop_performed`/`dnd_finished`/`cancelled` do źródła.
- **Klawiatura**: układ i wariant z otoczenia (`LUNA_KEYBOARD_LAYOUT`, `LUNA_KEYBOARD_VARIANT`,
  sanityzowane), `wl_keyboard.repeat_info`.
- **Realne wyjście DRM/KMS** (`SELENE_BACKEND=drm`, opcjonalne): przejęcie mastera, bufor
  „dumb”, framebuffer (`ADDFB2`), `SETCRTC` i **page-flip** na wykrytych monitorach; render
  sceny wprost do buforów (podwójne buforowanie). Gdy master trzyma inna sesja — bezpieczny
  fallback do nested (nie odbiera ekranu).
- Wielojęzyczność (`libluna-i18n`) i optymalizacja (`[profile.release]` LTO, cache `shm`).

## Powłoka Luna / Luna shell (`de/luna`)

Środowisko graficzne Luna (odpowiednik GNOME Shell) na libLunie, działające na kompozytorze
Selene.

- `de/luna/src/panel.rs` — **panel górny** (menu, tytuł okna, zegar/data, przełączniki,
  przestrzenie robocze, użytkownik) i **launcher** (siatka aplikacji + wyszukiwanie).
- `luna-panel` (bin) — render headless: `docs/images/luna-shell-{light,dark}.png`.
- `luna-shell` (bin) — panel na żywo jako **powierzchnia warstwowa** (`wlr-layer-shell`).
- `libluna-platform` obsługuje teraz oba tryby: okno `xdg-shell` (CSD) i powierzchnię
  warstwową (`AppConfig::layer(LayerConfig::…)`) — `LayerConfig::top_panel`/`overlay`.
- Dane (`PanelState`, `App`) są oddzielone od rysowania (`PaintCtx`), więc ten sam kod
  obsługuje podglądy i sesję na żywo.
- Dalej: OSD, powiadomienia, menu kontekstowe.

## Jeszcze nie / Not yet

- **Wejście w trybie DRM** (evdev/`libinput`) — skanowanie działa, ale bez rodzimego wejścia
  okna są tylko wyświetlane; GLES/akceleracja i hotplug (`zwlr_output_manager`) — do zrobienia.
- **Selekcja pierwotna** (`primary selection`); IME / `text-input`.
- `wl_output` dynamiczne (zmiana trybu), fokus klawiatury po kliknięciu poza oknem.
- Render tylko przy zmianie modelu (częściowe odświeżanie/damage).

## Uruchomienie / Running

```sh
# katalog runtime z prawem 0700
mkdir -p .selene-runtime && chmod 700 .selene-runtime
XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run -p selene
# → "Selene compositor started (wayland-0)" + gniazdo wayland-0 w katalogu runtime
```

Tryb nested (domyślny) uruchamia okno na nadrzędnym kompozytorze. **Realne wyjście DRM**
(gdy masz wolnego mastera, np. z osobnej konsoli):

```sh
SELENE_BACKEND=drm XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run -p selene
# diagnostyka bez zmiany trybu:  SELENE_DRM_PROBE=1 cargo run -p selene
```

Uwaga: tryb DRM przejmuje ekran (page-flip); gdy master trzyma Twoja sesja, Selene
zgłasza to i **nie** rusza wyświetlaczy.

Klienta podłączysz ustawiając `WAYLAND_DISPLAY=wayland-0` i `XDG_RUNTIME_DIR` na ten sam
katalog (renderowanie pojawi się, gdy wejdzie backend wyświetlania).

## Architektura / Architecture

```
de/selene/src/
  main.rs        # display, globale (wl_output per monitor), gniazdo, pętla poll
  state.rs       # stan: model okien (min/max/fullscreen), z-order, fokus, drag/resize, monitory
  compositor.rs  # wl_compositor / wl_surface / wl_region / wl_callback + kompozycja okien
  shm.rs         # wl_shm / wl_shm_pool / wl_buffer (+ cache mmap, odczyt pikseli)
  shell.rs       # xdg_wm_base / xdg_surface / xdg_toplevel / xdg_popup / xdg_positioner
  decoration.rs  # zxdg_decoration_manager_v1 (SSD: odpowiedź server_side)
  layer.rs       # zwlr_layer_shell_v1 (panele/docki: kotwice, warstwy, wybór monitora)
  data.rs        # wl_data_device_manager (schowek + drag & drop)
  monitors.rs    # wl_output (model wielu monitorów, układ, trafienia)
  drm.rs         # wykrywanie + scanout DRM/KMS (ioctl, dumb buffer, page-flip), fallback nested
  seat.rs        # wl_seat: wskaźnik + klawiatura + trasowanie wejścia i operacje okien
  input.rs       # magistrala wejścia (kolejka + wybudzanie)
  output.rs      # wyjście nested (libluna-platform + libluna-render)
```
