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
- **Wejście evdev** (`SELENE_INPUT=evdev`, opcjonalne): własne czytanie `/dev/input/event*`
  (wskaźnik względny z wirtualnym kursorem, przyciski, kółko, klawiatura) — rodzime wejście
  dla trybu DRM, bez nadrzędnego kompozytora. Wymaga dostępu do grupy `input`/root.
- Wielojęzyczność (`libluna-i18n`) i optymalizacja (`[profile.release]` LTO, cache `shm`).

## Powłoka Luna / Luna shell (`de/luna`)

Środowisko graficzne Luna (odpowiednik GNOME Shell) na libLunie, działające na kompozytorze
Selene.

- `de/luna/src/panel.rs` — **panel górny** (sam przycisk burger, tytuł okna, zegar/data,
  przełączniki z ikonami, przestrzenie robocze, użytkownik) i **launcher** w trzech widokach:
  **Lista**, **Siatka**, **Karuzela** (segmentowy przełącznik w nagłówku).
  Ikony z pakietu `libluna-widgets::icon` (`IconName`/`paint_icon`) — wektorowe, spójne ze stylem.
- `luna-panel` (bin) — render headless: `docs/images/luna-shell-{light,dark}.png`
  oraz `docs/images/luna-launcher-{list,grid,carousel}.png`.
- `luna-shell` (bin) — powierzchnia „chrome” na żywo: burger otwiera/zamyka launcher,
  przełącznik widoku, klik w aplikację zamyka, `Esc` zamyka; do tego **OSD** i **powiadomienia**
  (`osd.rs`, `notifications.rs`).
- `libluna-platform` obsługuje teraz oba tryby: okno `xdg-shell` (CSD) i powierzchnię
  warstwową (`AppConfig::layer(LayerConfig::…)`) — `LayerConfig::top_panel`/`overlay`/`top_chrome`
  oraz przezroczyste tło (`AppConfig::transparent`).
- Dane (`PanelState`, `App`) są oddzielone od rysowania (`PaintCtx`), więc ten sam kod
  obsługuje podglądy i sesję na żywo.
- Dalej: usługa D-Bus `org.freedesktop.Notifications`, centrum powiadomień, menu kontekstowe.

## Jeszcze nie / Not yet

- **Wejście evdev** (`evdev.rs`): zaimplementowane i sterowane `SELENE_INPUT=evdev`, ale
  wymaga dostępu do `/dev/input` (grupa `input`/root); bez tego Selene zgłasza
  `evdev input unavailable`.
- GLES/akceleracja, hotplug monitorów (`zwlr_output_manager`), `libinput` (gesty touchpada,
  akceleracja) zamiast surowych zdarzeń.
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

`XDG_RUNTIME_DIR` zwykle ustawia menedżer sesji. Gdy go brak (np. `pam_systemd` nie
założył sesji), Selene wybiera katalog sam: najpierw `/run/user/<uid>`, a gdy ten nie
istnieje — tworzy prywatny katalog `0700` pod `$TMPDIR`. Skrypt `luna-session` robi ten
sam wybór wcześniej, żeby zmienną dziedziczyli także klienci.

Tryb nested (domyślny) uruchamia okno na nadrzędnym kompozytorze. **Realne wyjście DRM**
(gdy masz wolnego mastera, np. z osobnej konsoli):

```sh
SELENE_BACKEND=drm XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run -p selene
# diagnostyka bez zmiany trybu:  SELENE_DRM_PROBE=1 cargo run -p selene
# rodzime wejście (gdy brak nadrzędnego kompozytora):  SELENE_INPUT=evdev cargo run -p selene
```

Uwaga: tryb DRM przejmuje ekran (page-flip); gdy master trzyma Twoja sesja, Selene
zgłasza to i **nie** rusza wyświetlaczy.

Instalator środowiska (binarki + sesja „Luna” + tłumaczenia + motywy) — `scripts/install.sh`:

```sh
./scripts/install.sh --destdir /tmp/luna-test --prefix /usr   # test bez zmian w systemie
sudo ./scripts/install.sh --prefix /usr                        # instalacja systemowa
```

Pliki sesji: `de/session/luna-session` (launcher) i `de/session/selene.desktop`
(`share/wayland-sessions`).

Klienta podłączysz ustawiając `WAYLAND_DISPLAY=wayland-0` i `XDG_RUNTIME_DIR` na ten sam
katalog (renderowanie pojawi się, gdy wejdzie backend wyświetlania).

## Bezpieczeństwo / Security

- Klienty Wayland działają z prawami tego samego użytkownika i są **zaufane** — kompozytor
  nie sandboxuje aplikacji (jak w każdym Waylandzie). Granica bezpieczeństwa to użytkownik/system.
- Gniazdo Wayland powstaje w `XDG_RUNTIME_DIR` o prawach `0700`; Selene waliduje ten katalog,
  a gdy go brak — wybiera bezpieczny fallback (bez setuid, bez gniazd sieciowych).
- Wejście (`/dev/input`, grupa `input`) i master DRM (`/dev/dri`, grupa `video`/root) to
  uprawnienia grupowe — świadoma decyzja trybu DRM, nie wymóg trybu nested.
- Odporność: obsługa klienta jest izolowana (`catch_unwind`), blokady odzyskują stan po
  „zatruciu” mutexa (`sync::LockExt`), bufor `wl_shm` walidowany (rozmiary, `stride`, bez
  SIGBUS/OOM), a kolejki (frame-callbacks, MIME) mają limity.

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
  evdev.rs       # wejście z /dev/input/event* (tryb DRM, wirtualny kursor)
  input.rs       # magistrala wejścia (kolejka + wybudzanie)
  output.rs      # wyjście nested (libluna-platform + libluna-render)
```
