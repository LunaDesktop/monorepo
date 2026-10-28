# Testy Luny

Dokument opisuje, co i jak testujemy — od testów jednostkowych widgetów po pełny
przepływ sesji na zagnieżdżonym kompozytorze.

## 1. Testy jednostkowe i integracyjne (bez okna)

Każdy workspace ma własny `Cargo.toml`/`Cargo.lock`, więc testy uruchamiamy przez
wspólny skrypt:

```sh
./scripts/workspaces.sh test          # wszystkie pakiety, bez doctestów
./scripts/workspaces.sh test -- --nocapture
```

Doctesty wymagają `rustdoc` w `PATH` (w toolchainach bez `rust-docs` ich nie ma):

```sh
./scripts/workspaces.sh test-doc
```

Zakres:

| Warstwa | Co jest sprawdzane |
|---|---|
| `libluna-core` | geometria, kolory, motywy, ustawienia, i18n |
| `libluna-render` | scena, clipping (także tekstu), rasteryzacja, ikony SVG |
| `libluna-widgets` | hit-testing, układ, zaznaczanie i schowek pól tekstowych (`clipboard_command`), galeria ikon |
| `libluna-platform` | mapowanie zdarzeń, schowek `wl_data_device`, akcje okna |
| `selene` | routing wejścia, schowek, xdg-shell, XWM, geometria okien |
| aplikacje | logika UI bez Waylanda: demo (interakcje), Ustawienia, Pliki, Terminal |

### Schowek w polach tekstowych

Przepływ jest trójwarstwowy — warto go znać, pisząc testy:

1. `libluna-widgets::clipboard_command(&mut pole, &KeyEvent)` rozpoznaje
   **Ctrl+C / Ctrl+X / Ctrl+V** i zwraca `TextClipboardCommand`
   (`Copy(Option<String>)`, `Cut(Option<String>)`, `Paste`).
   Sam widget nie zna Waylanda, więc nie dotyka schowka.
2. Aplikacja przekazuje tekst do `Frame::copy_to_clipboard`, a przy `Paste` woła
   `Frame::paste_from_clipboard` i zapamiętuje **pole docelowe**.
3. Platforma odczytuje schowek w tle i wysyła `Event::Paste(String)`;
   aplikacja wpisuje tekst do zapamiętanego pola (`Entry::paste`).

Testy jednostkowe pokrywają krok 1 (`clipboard_command`) oraz routing w aplikacjach
(np. `luna-demo/tests/interaction.rs`, testy `paste_into` w Ustawieniach/Plikach).

## 2. Test dymny całej sesji (`scripts/session-smoke.sh`)

Harness uruchamia **własny, zagnieżdżony Selene** w izolowanym `XDG_RUNTIME_DIR`
i steruje nim przez `SELENE_INPUT_SCRIPT` (te same zdarzenia, co z evdev).
Nie dotyka działającej sesji użytkownika.

```sh
# najpierw binarki (domyślnie profil release, wspólny target/)
./scripts/workspaces.sh build --release

# test (potrzebny działający nadrzędny kompozytor Wayland)
./scripts/session-smoke.sh
# równoważnie, przez wspólny runner:
./scripts/workspaces.sh smoke

# szybszy wariant na buildzie debug
LUNA_SMOKE_PROFILE=debug ./scripts/session-smoke.sh

# wskazanie konkretnego gniazda nadrzędnego kompozytora
LUNA_SMOKE_PARENT=/run/user/1000/wayland-0 ./scripts/session-smoke.sh

# ten sam test z backendem GPU kompozytora (brak EGL = automatyczny fallback CPU)
SELENE_GPU=1 ./scripts/session-smoke.sh
```

Zmienne:

| Zmienna | Znaczenie |
|---|---|
| `LUNA_SMOKE_PROFILE` | `release` (domyślnie) albo `debug` — katalog `target/<profil>` |
| `LUNA_SMOKE_BIN_DIR` | bezpośredni katalog z binarkami |
| `LUNA_SMOKE_PARENT` | ścieżka do gniazda nadrzędnego kompozytora (`wayland-0`) |
| `SELENE_BIN`, `LUNA_SHELL_BIN`, `LUNA_TERMINAL_BIN`, `LUNA_SETTINGS_BIN`, `LUNA_LOCK_BIN` | podmiana pojedynczych binarek |

Wymagania: działający nadrzędny kompozytor Wayland, `python3` oraz `dd`/`stty`
(coreutils). Klienci dostają izolowane `XDG_RUNTIME_DIR` i `XDG_CONFIG_HOME`,
więc test nie zależy od języka ani motywu z `~/.config`.

### Co sprawdza

1. **Start kompozytora** — Selene tworzy `wayland-0` i gniazdo `luna-control`.
2. **Panel** — `luna-shell` mapuje powierzchnię warstwową (`layer=true`).
3. **Launcher + przypinanie** — klik w burger otwiera launcher, a PPM na pierwszym
   kaflu zapisuje `pinned=` do `$XDG_CONFIG_HOME/luna/launcher.conf`; drugi PPM
   (ta sama pozycja, bo przypięta aplikacja przeskakuje na początek) usuwa wpis.
   To sprawdza całą drogę: hit-test kafla → `Launcher::toggle_pin` → zapis atomowy.
4. **Okna** — `luna-settings` i terminale mapują się jako zwykłe okna.
5. **Klawiatura** — znak wysłany skryptem trafia do procesu w PTY (`dd` zapisuje bajt).
6. **Schowek między klientami** — terminal A kopiuje linię (`Ctrl+Shift+C`),
   terminal B wkleja ją (`Ctrl+Shift+V`), a wynik trafia do pliku.
   To testuje pełną drogę: `wl_data_source` → Selene → `wl_data_offer` → drugi klient.
7. **Ekran blokady** — `luna-lock` mapuje overlay i **przejmuje klawiaturę**:
   znak wysłany po zablokowaniu nie dociera do terminala pod spodem.
8. **Zakończenie** — wszystkie procesy kończą się po `SIGTERM` (brak zawieszonych klientów).

### Test X11 (`scripts/x11-smoke.sh`)

Osobny test dla ścieżki `Xwayland + XWM` (wymaga `xclock` z pakietu `x11-apps`):

```sh
./scripts/x11-smoke.sh
# lub
make smoke-x11
```

Sprawdza: start Xwaylanda i marker `luna-display`, `wm=true` z
`luna-control x11`, okno `xclock` jako zwykłe okno (nie warstwa),
**maksymalizację** i **zamknięcie** przyciskami naszego SSD oraz to, że szerokość
po maksymalizacji to dokładnie szerokość monitora (regresja obramowania X11).

### Czego harness nie robi

- **Nie loguje się przez greeter** (`greetd`/PAM) i **nie odblokowuje** ekranu hasłem —
  to wymaga prawdziwych poświadczeń i pełnej sesji logind. Blokada jest weryfikowana
  jako overlay + przejęcie klawiatury; odblokowanie zostaje testem manualnym.
- **Nie testuje DRM/KMS** — celowo używa backendu nested, żeby nie przejmować ekranu.
  Ścieżkę DRM sprawdzaj na maszynie testowej: `SELENE_BACKEND=drm SELENE_INPUT=evdev selene`.
- **Nie testuje hotplug monitorów** ani XWayland (wyłączony przez `SELENE_NO_XWAYLAND=1`).

### Debugowanie nieudanego przebiegu

Skrypt przy błędzie **zostawia katalog tymczasowy** i wypisuje jego ścieżkę. Zawiera:

- `selene.log` — komunikaty kompozytora i odpowiedzi `luna-control`,
- `<nazwa>.log` — stdout/stderr każdej aplikacji (panel, terminale, Ustawienia, blokada),
- `input` — dokładny skrypt wejścia, który został wysłany.

Na powodzenie katalog jest usuwany.

## 3. Testy manualne (pełna sesja)

Przed wydaniem warto przejść checklistę na realnej sesji DRM:

1. Greeter → wybór sesji Luna → logowanie.
2. Panel: launcher, quick panel, powiadomienia, przełączanie przestrzeni.
3. Terminal: zaznaczanie, `Ctrl+Shift+C/V`, nowa karta, Ustawienia terminala.
4. Ustawienia: język, motyw, tapeta ekranu logowania (wymaga hasła administratora).
5. Pliki: kopiowanie/przenoszenie, konflikty nazw, kosz, podgląd.
6. `luna-lock` (Super+L): blokada na wszystkich monitorach, odblokowanie hasłem.
7. Wylogowanie z menu sesji → powrót do greetera bez zawieszonych procesów.

## 4. Zasady pisania nowych testów

- Logika UI ma być testowalna **bez Waylanda** — zdarzenia wchodzą przez
  `handle_events`, stan wychodzi jako pola struktury aplikacji.
- Akcje platformy (`close`, `copy_to_clipboard`, `paste_from_clipboard`) są
  odkładane do stanu aplikacji i aplikowane w pętli klatki — dzięki temu test
  nie potrzebuje `Frame`.
- Nowy skrót klawiaturowy w polu tekstowym dodawaj przez
  `libluna-widgets::clipboard_command`, nie przez kopiowanie logiki do aplikacji.
- Nowy protokół w Selene dodawaj razem z asercją w `session-smoke.sh` albo
  testem jednostkowym routingu.

## 5. Powiązane dokumenty

- [`docs/rendering.md`](rendering.md) — backend CPU, benchmark i kryteria GPU,
- [`docs/api-stability.md`](api-stability.md) — co jest kontraktem, co eksperymentalne,
- [`docs/architecture.md`](architecture.md) — warstwy i granice odpowiedzialności.
