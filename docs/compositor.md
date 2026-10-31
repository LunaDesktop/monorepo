# Selene — protokoły i droga do pełnego kompozytora

Selene nie używa Smithery: protokół obsługujemy własną warstwą na `wayland-server`.
Ten plik jest **żywą listą** tego, co już działa, i tego, co jeszcze dzieli nas od
kompozytora, na którym codzienna praca nie wymaga obejść.

## Działa

| Protokół / funkcja | Uwagi |
|---|---|
| `wl_compositor` v4 | `wl_surface`, regiony, damage, frame-callbacki |
| `wl_subcompositor` v1 | powierzchnie potomne: `sync`/`desync`, pozycja, kolejność stosu, składanie z alfą |
| `wl_shm` v1 | ARGB/XRGB/ABGR/XBGR → RGBA, pula z `resize` |
| `wl_output` v4 | geometria, tryb, skala 1×, nazwa, `done` |
| `wl_seat` v5 | pointer + keyboard, `frame`, powtórzenia klawiszy, XKB (pl/en/de) |
| `wl_pointer.set_cursor` + `wp_cursor_shape_v1` v2 | kursory klientów: własny obraz (`set_cursor`) i nazwane kształty (I-beam, łapka, resize); motyw XCursor z `SELENE_CURSOR_THEME`/`XCURSOR_THEME`/GTK, fallback = wbudowana strzałka; za duży obraz (>64 px) rysowany w scenie |
| `wl_data_device_manager` v3 | schowek i drag & drop wewnątrz sesji |
| `xdg_wm_base` v1 | toplevel, popup, pełny `xdg_positioner` (kotwica, grawitacja, flip/slide) |
| `zxdg_decoration_manager_v1` | polityka: zawsze SSD (albo `LUNA_CLIENT_DECORATIONS=1`) |
| `zwlr_layer_shell_v1` v4 | panele, tło, OSD; region wejścia z alfy |
| `zwp_linux_dmabuf_v1` v3 | bufory GPU klientów (`ARGB/XRGB/ABGR/XBGR8888`, modyfikatory EGL); tylko z `SELENE_GPU=1`/`LUNA_GPU=1`, import w `GpuRenderer` |
| `zwlr_foreign_toplevel_manager_v1` v3 | pasek zadań: tytuł, stan, aktywacja, zamknięcie |
| `ext_workspace_manager_v1` | przestrzenie robocze |
| ekran blokady (`luna-lock`) | overlay + `keyboard_interactivity=exclusive`; jedna powierzchnia na wszystkie wyjścia |
| `xwayland_shell_v1` + XWM | okna X11, `_NET_WM_STATE`, ramki, menu X11 |
| diagnostyka X11 | `luna-control x11` — ekran, właściciel WM, liczba okien i treść każdego okna (`content=WxH border=N`) |
| diagnostyka monitorów | `luna-control outputs` — nazwa, `WxH`, pozycja; testy sesji liczą z tego współrzędne |
| diagnostyka dmabuf | `luna-control dmabuf` — czy global jest ogłoszony i ile formatów/modyfikatorów |
| diagnostyka kursora | `luna-control cursor` — bieżący obraz/kształt, powierzchnia klienta, liczba `wl_pointer` i slot fokusu; `SELENE_TRACE_POINTER=1` loguje każdą zmianę fokusu wskaźnika |
| **GPU (nested)** | `SELENE_GPU=1` → `libluna_render::gpu::GpuRenderer` (EGL surfaceless) zamiast CPU; brak EGL = automatyczny fallback, sesja żyje. Szczegóły: [`docs/gpu.md`](gpu.md) |
| test X11 | `scripts/x11-smoke.sh` (`make smoke-x11`): Xwayland + `xclock`, SSD maksymalizacja, kursor klienta X11 (GTK4 przez Xwayland) i zamknięcie |
| X11 CSD (Firefox itd.) | ruch: `_NET_WM_MOVERESIZE` w `_NET_SUPPORTED` **oraz** self-move przez `ConfigureRequest`; maksymalizacja: `_NET_WM_STATE` z akcją 2 i dwoma atomami — cel liczony raz na komunikat |
| `xdg_surface.set_window_geometry` | widoczny prostokąt vs bufor z cieniami CSD |
| multi-monitor | obszary robocze per wyjście, panel rezerwuje miejsce tylko u siebie |

## Braki — kolejność wg wpływu na codzienną pracę

1. **`xdg_activation_v1`** — przenoszenie fokusu przy uruchamianiu z portali/paska.
   Bez tego nowe okno bywa za innym i trzeba go szukać.
2. **`zwp_primary_selection_v1`** — wklejanie środkowym przyciskiem (terminale).
3. **`zwp_pointer_constraints_v1` + `zwp_relative_pointer_v1`** — gry, mouselook,
   blokada kursora.
4. **`zwp_text_input_v3` + input-method** — IME (CJK, kompozycja znaków).
5. **`wp_viewporter` + `wp_fractional_scale_v1`** — skalowanie 1,25/1,5× bez rozmycia.
6. **`zwp_idle_inhibit_v1`** — odtwarzacze wideo i prezentacje nie pozwalają wygasić ekranu.
7. **`wp_tearing_control_v1`** — brak vsync dla gier (opcjonalnie).
8. **`xdg_toplevel` v2+** — `wm_capabilities`, `configure_bounds`, stan `tiled`;
   dziś ogłaszamy v1, więc klienty nie dostają pełnej informacji o kafelkowaniu.
9. **`buffer_scale` / `buffer_transform`** — całkowite HiDPI i obrót ekranu.
10. **`ext_session_lock` / screencopy** — blokada sesji i zrzuty ekranu aplikacji.
    Dziś `luna-lock` robi blokadę warstwą `overlay` (działa w Lunie, ale nie jest
    standardowym protokołem blokady dla zewnętrznych lockerów); `ext_session_lock`
    i screencopy pozostają do dodania.

## Dekoracje okien (SSD/CSD)

Domyślnie rysujemy własny pasek (SSD) i odpowiadamy `server_side` na negocjację
`zxdg_decoration_manager_v1`. Część klientów (Chromium: Chrome, Ecosia, Edge,
Brave; Electron) potwierdza `server_side`, ale **nadal rysuje własne dekoracje**.
Wykrywamy to po buforze większym od `xdg_surface.set_window_geometry` o cienie CSD
(`image_offset > 0`) oraz po `app_id`/tytule przeglądarki i wtedy **nie dokładamy
swojego paska**. Bez tego okno miało podwójny tytuł, a górna strefa naszego paska
przechwytywała kliknięcia w menu ⋮ i przyciski klienta.

Popupy `xdg_popup` dostają pierwszą konfigurację już przy commicie **bez bufora**
(z rozmiarem z `xdg_positioner`), są utrzymywane nad rodzicem po ponownym pokazaniu
i oddają fokus klawiatury do menu nadrzędnego lub okna po zamknięciu. Wcześniej
menu Chrome’a nie mogło się poprawnie zmapować, a po zamknięciu klawiatura
zostawała na ukrytej powierzchni.

## Znane problemy do zbadania

- **Zed przez XWayland potrafi „zjeść” cały rdzeń** (97% CPU, kilka GB RAM) mimo
  poprawnego UI. Podejrzenie: pętla `ConfigureNotify`/expose w XWM albo brak
  `_NET_WM_SYNC_REQUEST`. Do reprodukcji na izolowanej sesji z `DISPLAY` Xwaylanda.
- `xdg_popup` nie obsługuje jeszcze `reposition` (przestawianie już otwartego menu
  przy zmianie rozmiaru rodzica); `grab` jest realizowany tylko przez zwykłe
  trasowanie wejścia i zamykanie po kliknięciu poza menu.
