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
| Toolkit GUI | **libLuna** | widgety, render, i18n (`libluna/crates/libluna-*`) |
| Środowisko | **Luna** | pulpit (panel, launcher, OSD) — `luna` |
| Kompozytor | **Selene** | serwer Wayland — `selene` |
| Logowanie | **Luna Greeter** | ekran logowania dla greetd — `luna-greeter` |

Pakiety źródłowe są osobnymi workspace'ami w katalogach najwyższego poziomu:
`libluna/`, `luna/`, `selene/`, `luna-greeter/` oraz `apps/*`. Katalog `de/` zawiera już
tylko **integrację sesji** (launcher, wpisy `.desktop`, portale XDG, przykładowy greetd),
wspólną dla tych pakietów.

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
  `Super+↑`/`Super+↓` (maksymalizuj/przywróć), `Super+Tab` (przełącz okna),
  `Super+L` (wyloguj — kończy sesję).
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
- **Realne wyjście DRM/KMS** (`SELENE_BACKEND=drm`, opcjonalne): przejęcie mastera, wybór
  trybu natywnego monitora (maks. rozdzielczość, potem najwyższe odświeżanie), bufor
  „dumb”, framebuffer (`ADDFB2`), `SETCRTC` i **page-flip** na wykrytych monitorach; render
  sceny wprost do buforów (podwójne buforowanie). Gdy master trzyma inna sesja — bezpieczny
  fallback do nested (nie odbiera ekranu).
- **Wejście evdev** (`SELENE_INPUT=evdev`, opcjonalne): własne czytanie `/dev/input/event*`
  (wskaźnik względny z wirtualnym kursorem, przyciski, kółko, klawiatura) — rodzime wejście
  dla trybu DRM, bez nadrzędnego kompozytora. Wymaga dostępu do grupy `input`/root.
- **Izolacja od sygnałów konsoli**: w trybie DRM Selene zakłada własną grupę procesów i
  przejmuje pierwszy plan VT (`vt.rs`). Bez tego konsola jądra (zostaje w trybie tekstowym)
  wysyłała `SIGINT` z Ctrl+C do **całej** grupy procesów sesji — ginął proces `greetd`
  nadzorujący sesję, a logind kończył sesję (wyglądało to jak wylogowanie). `SIGINT`,
  `SIGQUIT` i `SIGTSTP` są ignorowane, a `SIGTERM`/`SIGHUP` logują pid nadawcy i kończą
  sesję z kodem `128+n`.
- Wielojęzyczność (`libluna-i18n`) i optymalizacja (`[profile.release]` LTO, cache `shm`).

## Powłoka Luna / Luna shell (`luna`)

Środowisko graficzne Luna (odpowiednik GNOME Shell) na libLunie, działające na kompozytorze
Selene.

- `luna/src/panel.rs` — **panel górny** (sam przycisk burger, tytuł okna, zegar/data,
  przełączniki z ikonami, przestrzenie robocze, użytkownik) i **launcher** w trzech widokach:
  **Lista**, **Siatka**, **Karuzela** (segmentowy przełącznik w nagłówku).
  Ikony z pakietu `libluna-widgets::icon` (`IconName`/`paint_icon`) — wektorowe, spójne ze stylem.
- `luna-panel` (bin) — render headless: `docs/images/luna-panel-idle.png`,
  `luna-launcher-{list,grid,carousel}.png`, `luna-popup-*.png` (w tym
  `luna-popup-notifications.png`) i `luna-osd.png`.
- `luna-shell` (bin) — powierzchnia „chrome” na żywo: burger otwiera/zamyka launcher,
  przełącznik widoku, **przewijanie listy**, klik w aplikację **uruchamia** ją;
  **nawigacja klawiaturą** (strzałki poruszają zaznaczeniem, `Enter` uruchamia, `Esc` zamyka),
  do tego **OSD** i **powiadomienia** (`osd.rs`, `notifications.rs`) z ikonami pakietu.
  Klik w zegar otwiera kalendarz **i centrum powiadomień** (historia, „Wyczyść”);
  kropka przy zegarze pokazuje, że w historii są powiadomienia.
- **Wyszukiwanie w launcherze**: pole ma przycisk **„×”** czyszczący frazę, a zamknięcie
  launchera resetuje wyszukiwanie i przewinięcie — kolejne otwarcie pokazuje pełną listę.
- **Menu PPM w launcherze**: prawy klik na aplikacji otwiera menu z akcjami
  *Otwórz*, *Dodaj do ulubionych / Usuń z ulubionych*, *Otwórz jako administrator*
  (polkit/pkexec — okno hasła), *Kopiuj polecenie uruchamiające* i *Odinstaluj*
  (otwiera Sklep na pakiecie). Menu obsługuje też strzałki i `Enter`; `Esc` zamyka
  samo menu, nie launcher.
- **Świeża lista aplikacji**: po instalacji pakietu (nowy plik `.desktop`) launcher
  sam wykrywa zmianę (tani znacznik katalogów `applications/`, sprawdzany co sekundę)
  i doda nową aplikację bez restartu powłoki.
- **Panel i launcher**: każdy klikalny element (burger, przyciski okien, kropki przestrzeni,
  przełączniki, zegar, użytkownik, zasilanie, kafelki/wiersze launchera, przełącznik widoku,
  „×” wyszukiwania, kafelki i przyciski Quick Panelu, menu sesji, listy Wi-Fi i głośności)
  podświetla się pod kursorem. Mechanizm jest wspólny (`libluna_widgets::Hover`):
  powłoka podaje mu zdarzenia, a rysowanie pyta `is_hovered`/`index`.
- **Ekran blokady** (`luna-lock`): pełnoekranowy overlay `wlr-layer-shell` z klawiaturą
  wyłączną, hasło weryfikowane w tle przez PAM, jedna powierzchnia
  rysowana na wszystkich monitorach. Wejście: menu sesji → **Zablokuj ekran**.
  Tło to **ta sama tapeta co na pulpicie** (przyciemniona pod kartę):
  `LUNA_WALLPAPER`/`SELENE_WALLPAPER`, a potem `~/.config/luna/wallpaper.conf`
  (`path=/ścieżka/do.png`); brak pliku → wbudowana grafika Luny.
  `SELENE_NO_WALLPAPER=1` wyłącza tapetę także na ekranie blokady.
- **Menu sesji**: Ustawienia, Zablokuj ekran, Wyloguj, Uruchom ponownie, Wyłącz komputer.
  Restart i wyłączenie idą przez `systemctl` (a nie `loginctl`, które takich poleceń nie ma).
- **Daemon powiadomień** (w `luna-shell`): standardowe `org.freedesktop.Notifications`
  z akcjami, `replaces_id`, `CloseNotification` i wpisami trwałymi (`expire_timeout = 0`).
  Historia, DND i centrum powiadomień są w tej samej powłoce; aplikacje wołają zwykły D-Bus.
- **Luna Settings** (`luna-settings`, `apps/luna-settings`) — centrum ustawień na
  libLunie: sidebar z sekcjami **Środowisko / Sprzęt / Informacje** i stronami
  **Wygląd** (motyw, akcent, styl), **Ikony** (styl wektorowy + **zewnętrzne zestawy
  freedesktop**, np. Papirus, z podglądem i wyborem globalnym), **Ekran logowania**
  (tapety z miniaturami, motyw i zegar greetera; szkic zmian, przycisk **Zastosuj
  zmiany** i **formularz hasła administratora** przez `sudo -S`), **Ekran blokady**
  (per użytkownik, bez roota), **Powiadomienia** (DND + róg toastów + **test**),
  **Język i region** (wyszukiwany wybór języka/locale/strefy; `locale.conf` +
  `region.conf`; zmiana języka działa **natychmiast** we wszystkich aplikacjach Luny),
  **Przestrzenie robocze** (liczba + nazwy; panel i `Super+Tab` aktualizują się
  na żywo przez `luna-control`), **Ekrany** (odczyt DRM
  + jasność), **Dźwięk**, **Zasilanie**, **System** i **O Luna**. Wejście z menu sesji
  („Ustawienia”) i wpisu `.desktop`. Podglądy:
  `cargo run --manifest-path apps/luna-settings/Cargo.toml --bin preview` → `docs/images/luna-settings-*.png`.
- **Luna Terminal** (`luna-terminal`, `apps/luna-terminal`) — emulator VT100/xterm bez
  GTK/VTE: `posix_openpt` + `fork`/`exec`, scrollback 2000 linii, kolory 16/256/truecolor,
  tryb alternatywny (`vim`, `less`, `htop`), kursory aplikacji i mapa klawiatury
  xterm-256color. Font monospace: `TextEngine::mono()` + `AppConfig::monospace(true)`.
  Wejście z wpisu `eu.luna.Terminal.desktop`; podgląd headless:
  `cargo run --manifest-path apps/luna-terminal/Cargo.toml --bin preview` → `docs/images/luna-terminal.png`.
  Szczegóły: [`docs/terminal.md`](../docs/terminal.md).
- **Luna Editor** (`luna-editor`, `apps/luna-editor`) — edytor tekstu i kodu:
  podświetlanie składni (Rust, TOML, JSON, Markdown, Shell, C/C++, Python) przez
  nowy `CodeView` w libLunie, numery linii, bieżąca linia, przewijanie w obu osiach,
  zaznaczanie, schowek, undo/redo, wyszukiwanie (`Ctrl+F`), „Idź do linii”
  (`Ctrl+G`), zachowanie LF/CRLF i BOM oraz atomowy zapis pliku.
  Wejście z `eu.luna.Editor.desktop` (MIME tekst/kod — także z Plików przez
  „Otwórz za pomocą”); podgląd headless:
  `cargo run --manifest-path apps/luna-editor/Cargo.toml --bin editor-render --`
  → `docs/images/luna-editor.png` i `luna-editor-dark.png`.
  Szczegóły: [`docs/editor.md`](../docs/editor.md).
- **Luna Media** (`luna-media`, `apps/luna-media`) — przeglądarka obrazów, RAW-ów,
  filmów i PDF-ów: galeria z miniaturami, podgląd z zoomem i panoramowaniem,
  pokaz slajdów, odtwarzacz wideo (pauza, przewijanie, wyciszenie) oraz czytnik
  PDF ze zmianą stron i zaznaczaniem tekstu. Obrazy PNG/JPEG/SVG dekodujemy
  natywnie, RAW-y przez osadzony podgląd JPEG lub pełną demozajkę `dcraw_emu`,
  resztę i filmy przez `ffmpeg`, strony PDF przez poppler; dźwięk przez
  `pw-play`/`paplay`.
  Wejście z wpisu `eu.luna.Media.desktop`; podgląd headless:
  `cargo run --manifest-path apps/luna-media/Cargo.toml --bin preview` →
  `docs/images/luna-media.png`, `luna-media-viewer.png`, `luna-media-video.png`,
  `luna-media-pdf.png`.
  Szczegóły: [`docs/media.md`](../docs/media.md).
- **Luna Clock** (`luna-clock`, `apps/luna-clock`) — zegar lokalny i światowy,
  minutnik, stoper oraz kalendarz z własnymi wydarzeniami, świętami liczącymi się
  offline i automatycznym motywem o wybranych godzinach. Panel czyta ten sam plik
  wydarzeń (kropki i plan dnia) i otwiera aplikację na wybranym dniu; po zamknięciu
  aplikacji działający minutnik/stoper pokazuje się jako widget pod paskiem.
  Wejście z `eu.luna.Clock.desktop`; podglądy:
  `cargo run --manifest-path apps/luna-clock/Cargo.toml --bin preview` →
  `docs/images/clock-*.png`.
  Szczegóły i prywatność: [`docs/clock.md`](../docs/clock.md).
- **Wielojęzyczność na żywo**: `libluna-platform` obserwuje `~/.config/luna/locale.conf`,
  przeładowuje katalogi `libluna-i18n`, ustawia `LANGUAGE`/`LANG` i odrysowuje okno;
  aplikacje z cache'owanymi napisami (sidebar, nawigacja, tytuł okna) odświeżają je po
  zmianie języka. Ustawienia zapisują też `~/.config/environment.d/00-luna-locale.conf`
  i aktualizują środowisko D-Busa/systemd user, więc **nowe aplikacje spoza Luny**
  startują w nowym języku. Szczegóły: `docs/settings.md`.
- **Galeria ikon**: `cargo run --manifest-path apps/luna-demo/Cargo.toml --bin icons` → `docs/images/icons.png`
  (cały `IconName::ALL` z podpisami, motyw jasny i ciemny).
- **Prawdziwe aplikacje**: `apps.rs` skanuje pliki `.desktop` (`XDG_DATA_HOME`/`XDG_DATA_DIRS`),
  parsuje `[Desktop Entry]` (nazwy lokalizowane po `LANG`), filtruje `NoDisplay`/`Hidden`;
  podgląd: `cargo run --manifest-path luna/Cargo.toml --bin luna-apps`. Gdy brak `.desktop` — zestaw demo.
- `libluna-platform` obsługuje teraz oba tryby: okno `xdg-shell` (CSD) i powierzchnię
  warstwową (`AppConfig::layer(LayerConfig::…)`) — `LayerConfig::top_panel`/`overlay`/`top_chrome`
  oraz przezroczyste tło (`AppConfig::transparent`).
- Dane (`PanelState`, `App`) są oddzielone od rysowania (`PaintCtx`), więc ten sam kod
  obsługuje podglądy i sesję na żywo. Podgląd centrum powiadomień:
  `docs/images/luna-popup-notifications.png` (`cargo run --manifest-path luna/Cargo.toml --bin luna-panel`).

## Ekran logowania / Login manager (`luna-greeter`)

- `luna-greeter` jest frontendem **greetd** (JSON po `GREETD_SOCK` / `/run/greetd-ipc`);
  PAM, logind i uruchomienie sesji zostają w daemonie, a UI działa jako użytkownik `greeter`.
- Ekran pokazuje lokalne konta z `/etc/passwd`, pozwala wpisać dowolną nazwę (LDAP/SSSD),
  a sesje wykrywa z systemowych `wayland-sessions/` i `xsessions/` (menu Wayland/X11).
  Po udanym logowaniu zapamiętuje ostatnie konto/sesję, a awatary bierze z AccountsService.
  Obsługuje `auth_message` (`secret`/`visible`/`info`/`error`), ponowną próbę po `auth_error`
  i uruchamia wybraną sesję przez `start_session`.
- `de/session/luna-greeter-session` startuje Selene jako kompozytor greetera i po zakończeniu
  greetera zwalnia DRM. Konfiguracja greetd: `de/greetd/config.toml.example`.
- Szczegóły instalacji, wymagania (`video`/`render`/`input`, `wlr-layer-shell`) i tryb podglądu:
  [`docs/greeter.md`](../docs/greeter.md).

## Jeszcze nie / Not yet

- **Wejście evdev** (`evdev.rs`): zaimplementowane i sterowane `SELENE_INPUT=evdev`, ale
  wymaga dostępu do `/dev/input` (grupa `input`/root); bez tego Selene zgłasza
  `evdev input unavailable`.
- GLES/akceleracja: ✅ kompozycja **nested** przez EGL (`SELENE_GPU=1`), bufory GPU
  klientów (`zwp_linux_dmabuf_v1`); w trybie DRM wyjście nadal CPU. Brakuje
  hotplug monitorów (`zwlr_output_manager`), `libinput` (gesty touchpada,
  akceleracja) zamiast surowych zdarzeń.
- **Selekcja pierwotna** (`primary selection`); IME / `text-input`.
- `wl_output` dynamiczne (zmiana trybu), fokus klawiatury po kliknięciu poza oknem.
- Render tylko przy zmianie modelu (częściowe odświeżanie/damage).

## Uruchomienie / Running

```sh
# katalog runtime z prawem 0700
mkdir -p .selene-runtime && chmod 700 .selene-runtime
XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run --manifest-path selene/Cargo.toml
# → "Selene compositor started (wayland-0)" + gniazdo wayland-0 w katalogu runtime
```

`XDG_RUNTIME_DIR` zwykle ustawia menedżer sesji. Gdy go brak (np. `pam_systemd` nie
założył sesji), Selene wybiera katalog sam: najpierw `/run/user/<uid>`, a gdy ten nie
istnieje — tworzy prywatny katalog `0700` pod `$TMPDIR`. Skrypt `luna-session` robi ten
sam wybór wcześniej, żeby zmienną dziedziczyli także klienci. `luna-session` uruchamia
także powłokę `luna-shell` (panel/OSD), o ile jest zainstalowana; w trybie DRM Selene
rysuje własny kursor (strzałkę), bo nie ma nadrzędnego kompozytora.

Log uruchomienia sesji trafia do stałego pliku `~/.local/state/luna/session.log`
(LightDM trzyma tylko dwa pliki `.xsession-errors` i obraca je przy każdej sesji, więc
bez tego diagnostyka przepada).
- **Zrzuty bez myszy**: `touch "$XDG_RUNTIME_DIR/luna-shell-launcher"` otwiera/zamyka
  launcher w działającej powłoce (przydatne do testów języka i wyglądu).

Tryb nested (domyślny) uruchamia okno na nadrzędnym kompozytorze. **Realne wyjście DRM**
(gdy masz wolnego mastera, np. z osobnej konsoli):

```sh
SELENE_BACKEND=drm XDG_RUNTIME_DIR="$PWD/.selene-runtime" cargo run --manifest-path selene/Cargo.toml
# diagnostyka bez zmiany trybu:  SELENE_DRM_PROBE=1 cargo run --manifest-path selene/Cargo.toml
# rodzime wejście (gdy brak nadrzędnego kompozytora):  SELENE_INPUT=evdev cargo run --manifest-path selene/Cargo.toml
```

Uwaga: tryb DRM przejmuje ekran (page-flip); gdy master trzyma Twoja sesja, Selene
zgłasza to i **nie** rusza wyświetlaczy. Gdy sesję startuje menedżer logowania, greeter
potrafi jeszcze chwilę trzymać master — Selene czeka na niego do ~5 s, po czym się poddaje.

Instalator środowiska (binarki + sesja „Luna” + tłumaczenia + motywy) — `scripts/install.sh`:

```sh
./scripts/install.sh --destdir /tmp/luna-test --prefix /usr   # test bez zmian w systemie
sudo ./scripts/install.sh --prefix /usr                        # instalacja systemowa
```

Pliki sesji: `de/session/luna-session` (launcher) i `de/session/selene.desktop`
(`share/wayland-sessions`). Ikona sesji (`Icon=luna`) to `assets/branding/luna.svg` — instalator kładzie
go w `share/icons/hicolor/scalable/apps` oraz rasteryzuje (rsvg-convert) do PNG w rozmiarach
16–256 i `share/pixmaps`, więc greeter pokaże go nawet bez obsługi SVG.

Klienta podłączysz ustawiając `WAYLAND_DISPLAY=wayland-0` (`XDG_RUNTIME_DIR` klient wybierze
awaryjnie sam, tak jak Selene — patrz `libluna_core::runtime`). Renderowanie pojawi się, gdy
wejdzie backend wyświetlania.

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
selene/src/
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
