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
| `wl_data_device_manager` v3 | schowek i drag & drop wewnątrz sesji |
| `xdg_wm_base` v1 | toplevel, popup, pełny `xdg_positioner` (kotwica, grawitacja, flip/slide) |
| `zxdg_decoration_manager_v1` | polityka: zawsze SSD (albo `LUNA_CLIENT_DECORATIONS=1`) |
| `zwlr_layer_shell_v1` v4 | panele, tło, OSD; region wejścia z alfy |
| `zwlr_foreign_toplevel_manager_v1` v3 | pasek zadań: tytuł, stan, aktywacja, zamknięcie |
| `ext_workspace_manager_v1` | przestrzenie robocze |
| ekran blokady (`luna-lock`) | overlay + `keyboard_interactivity=exclusive`; jedna powierzchnia na wszystkie wyjścia |
| `xwayland_shell_v1` + XWM | okna X11, `_NET_WM_STATE`, ramki, menu X11 |
| diagnostyka X11 | `luna-control x11` — ekran, właściciel WM, liczba okien i treść każdego okna (`content=WxH border=N`) |
| diagnostyka monitorów | `luna-control outputs` — nazwa, `WxH`, pozycja; testy sesji liczą z tego współrzędne |
| **GPU (nested)** | `SELENE_GPU=1` → `libluna_render::gpu::GpuRenderer` (EGL surfaceless) zamiast CPU; brak EGL = automatyczny fallback, sesja żyje. Szczegóły: [`docs/gpu.md`](gpu.md) |
| test X11 | `scripts/x11-smoke.sh` (`make smoke-x11`): Xwayland + `xclock`, SSD maksymalizacja i zamknięcie |
| X11 CSD (Firefox itd.) | ruch: `_NET_WM_MOVERESIZE` w `_NET_SUPPORTED` **oraz** self-move przez `ConfigureRequest`; maksymalizacja: `_NET_WM_STATE` z akcją 2 i dwoma atomami — cel liczony raz na komunikat |
| `xdg_surface.set_window_geometry` | widoczny prostokąt vs bufor z cieniami CSD |
| multi-monitor | obszary robocze per wyjście, panel rezerwuje miejsce tylko u siebie |

## Braki — kolejność wg wpływu na codzienną pracę

1. **`xdg_activation_v1`** — przenoszenie fokusu przy uruchamianiu z portali/paska.
   Bez tego nowe okno bywa za innym i trzeba go szukać.
2. **`wl_pointer.set_cursor` + `wp_cursor_shape_v1`** — własne kursory klienta
   (I-beam, łapka, strzałki resize). Dziś zawsze rysujemy kursor systemowy.
3. **`zwp_primary_selection_v1`** — wklejanie środkowym przyciskiem (terminale).
4. **`zwp_pointer_constraints_v1` + `zwp_relative_pointer_v1`** — gry, mouselook,
   blokada kursora.
5. **`zwp_text_input_v3` + input-method** — IME (CJK, kompozycja znaków).
6. **`wp_viewporter` + `wp_fractional_scale_v1`** — skalowanie 1,25/1,5× bez rozmycia.
7. **`zwp_linux_dmabuf_v1`** — zero-copy z GPU. **Największa luka:** klienty Vulkan/EGL
   (Zed natywnie na Waylandzie, hardware video) potrzebują importu dmabuf (GBM/EGL),
   a nasz renderer jest CPU. Do zaplanowania razem z back-endem GPU.
8. **`zwp_idle_inhibit_v1`** — odtwarzacze wideo i prezentacje nie pozwalają wygasić ekranu.
9. **`wp_tearing_control_v1`** — brak vsync dla gier (opcjonalnie).
10. **`xdg_toplevel` v2+** — `wm_capabilities`, `configure_bounds`, stan `tiled`;
    dziś ogłaszamy v1, więc klienty nie dostają pełnej informacji o kafelkowaniu.
11. **`buffer_scale` / `buffer_transform`** — całkowite HiDPI i obrót ekranu.
12. **`ext_session_lock` / screencopy** — blokada sesji i zrzuty ekranu aplikacji.
    Dziś `luna-lock` robi blokadę warstwą `overlay` (działa w Lunie, ale nie jest
    standardowym protokołem blokady dla zewnętrznych lockerów); `ext_session_lock`
    i screencopy pozostają do dodania.

## Znane problemy do zbadania

- **Zed przez XWayland potrafi „zjeść” cały rdzeń** (97% CPU, kilka GB RAM) mimo
  poprawnego UI. Podejrzenie: pętla `ConfigureNotify`/expose w XWM albo brak
  `_NET_WM_SYNC_REQUEST`. Do reprodukcji na izolowanej sesji z `DISPLAY` Xwaylanda.
- `xdg_popup` nie obsługuje jeszcze `reposition`/`reactive` (przestawianie już
  otwartego menu przy zmianie rozmiaru rodzica).
