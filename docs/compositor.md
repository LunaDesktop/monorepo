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
| `zxdg_output_manager_v1` v3 (`xdg-output`) | pozycja i rozmiar **logiczny**, nazwa i opis złącza (`DP-1`). Zdarzenia idą **przed** `wl_output.done` — klient v3 kończy partię na `wl_output.done` i przy odwrotnej kolejności liczył monitor o zerowym rozmiarze (GTK4: asercja `scale >= 1` i segfault). `zxdg_output_v1.done` wysyłamy tylko do v1/v2 (od v3 jest przestarzałe) |
| `wp_single_pixel_buffer_v1` | bufory 1×1 (`memfd`, ARGB8888) — tła i maski GTK4/Chromium bez pełnego bufora shm |
| `wp_content_type_v1` | rodzaj treści (`none`/`photo`/`video`) zapisywany w `SurfaceData.content_type`; wejście dla przyszłych decyzji o skanoucie |
| `wp_tearing_control_v1` | preferencja `vsync`/`async` przyjęta i zapisana (`SurfaceData.tearing`); prezentacja nadal synchronizowana do vblanku — protokół pozwala ją zignorować |
| `wp_pointer_warp_v1` | przenoszenie kursora w obrębie okna (aplikacje X11 przez Xwaylanda, Java/AWT, Wine); żądanie poza okno jest odrzucane, zgodnie z protokołem |
| `zwp_keyboard_shortcuts_inhibit_v1` | inhibitor skrótów pulpitu dla gry/emulatora/zdalnego pulpitu; przy aktywnym inhibitorze `keyboard_key` nie zjada Super/Alt+Tab/Alt+F4/Print ani skrótów z `shortcuts.conf`, a stan `active`/`inactive` idzie do klienta przy zmianie fokusu |
| `wl_seat` v5 | pointer + keyboard, `frame`, powtórzenia klawiszy, XKB (pl/en/de) |
| `wl_pointer.set_cursor` + `wp_cursor_shape_v1` v2 | kursory klientów: własny obraz (`set_cursor`), **także bufor `dmabuf`** (Zed/GPUI), i nazwane kształty (I-beam, łapka, resize); motyw XCursor z `SELENE_CURSOR_THEME`/`XCURSOR_THEME`/GTK z rozwinięciem listy `Inherits` (`Pagan-Cursors` → `Adwaita`), fallback = wbudowana strzałka; za duży obraz (>64 px) rysowany w scenie; kursor jest widoczny od pierwszego kadru (start na środku monitora, bez czekania na ruch myszy) |
| `wl_data_device_manager` v3 | schowek i drag & drop wewnątrz sesji; `wl_data_offer.accept` jest przekazywane źródłu jako `wl_data_source.target` (bez tego XWayland/CLI nie wysyłają danych), oferty mają wersję klienta, nie na sztywno v3; **negocjacja akcji** (`set_actions`/`action`): kompozytor liczy część wspólną ofert źródła i celu, wysyła wybór obu stronom, a `Shift` w trakcie przeciągania przełącza kopiowanie ↔ przenoszenie |
| most schowka **Wayland ↔ X11 (tekst)** | Xwayland **nie implementuje** `wl_data_device`, więc most robi kompozytor (jak KWin/Mutter). Wayland → X11: przy `set_selection` pobieramy tekst od źródła (rura + `poll` w pętli głównej), a XWM wystawia go jako `CLIPBOARD` (`TARGETS`, `UTF8_STRING`, `STRING`, `TEXT`, `text/plain*`). X11 → Wayland: XWM wykrywa właściciela `CLIPBOARD`, pobiera tekst (`TARGETS` + dane), a pętla główna ustawia go jako selekcję Waylanda; odświeżanie co 0,5 s łapie zmianę treści przy tym samym oknie (Zed). Dzięki temu „Chrome → Zed” i „Zed → Chrome” działają |
| `zwp_primary_selection_v1` | selekcja pierwotna — wklejanie środkowym przyciskiem (terminale, edytory, XWayland) |
| `zwp_pointer_constraints_v1` | blokada kursora w miejscu (mouselook) i zamknięcie w regionie; `oneshot` kończy się po pierwszym przycisku; `luna-control cursor` pokazuje aktywną blokadę |
| `zwp_idle_inhibit_v1` | wideo/prezentacje nie wygaszają ekranu ani nie uruchamiają blokady/usypiania |
| `ext_idle_notify_v1` | klienci (odtwarzacze, przeglądarki) dostają `idled`/`resumed` po własnym progu `timeout`; ten sam zegar co wygaszanie, inhibitor wstrzymuje powiadomienia, a pętla budzi się na najbliższy próg |
| `zwp_text_input_v3` + `zwp_input_method_v2` | IME: fcitx5/ibus, kompozycja CJK, preedit/commit, `delete_surrounding_text`, grab klawiatury i `zwp_virtual_keyboard_v1` dla przepuszczonych klawiszy; okno kandydatów (`zwp_input_popup_surface_v2`) rysowane przy polu tekstowym |
| `xdg_activation_v1` | tokeny aktywacji (portale, launchery): nowe okno dostaje fokus i jest podnoszone; token jednorazowy, 10 s |
| `ext_data_control_v1` | menedżer schowka: historia i przetrwanie zamknięcia źródła; źródła menedżera są tłumaczone na wspólny model i widoczne dla zwykłych klientów (także selekcja pierwotna) |
| `wp_viewporter` + `wp_fractional_scale_v1` | destination/source dla powierzchni; `preferred_scale=120` (1×) na razie, gotowe pod HiDPI |
| `xdg_wm_base` v3 | toplevel, popup, pełny `xdg_positioner` (kotwica, grawitacja, flip/slide) + **przycinanie do obszaru roboczego**; `xdg_popup.reposition`; monitor wybierany po kotwicy popupu (dwa wyjścia). `wl_surface.enter`/`leave` wysyłane dla okien, popupów i powierzchni potomnych |
| `zxdg_decoration_manager_v1` | polityka: zawsze SSD (albo `LUNA_CLIENT_DECORATIONS=1`) |
| `zwlr_layer_shell_v1` v4 | panele, tło, OSD; region wejścia z alfy |
| `zwp_linux_dmabuf_v1` v3 | bufory GPU klientów (`ARGB/XRGB/ABGR/XBGR8888`, modyfikatory EGL, **NV12/NV21/YUV420/YV12**) — tylko z `SELENE_GPU=1`/`LUNA_GPU=1`, import w `GpuRenderer` |
| `zwlr_foreign_toplevel_manager_v1` v3 | pasek zadań: tytuł, stan, aktywacja, zamknięcie |
| `ext_workspace_manager_v1` | przestrzenie robocze |
| ekran blokady (`luna-lock`) | overlay + `keyboard_interactivity=exclusive`; jedna powierzchnia na wszystkie wyjścia |
| `xwayland_shell_v1` + XWM | okna X11, `_NET_WM_STATE`, ramki, menu X11 |
| diagnostyka X11 | `luna-control x11` — ekran, właściciel WM, liczba okien i treść każdego okna (`content=WxH border=N`) |
| diagnostyka monitorów | `luna-control outputs` — nazwa, `WxH`, pozycja; testy sesji liczą z tego współrzędne |
| diagnostyka dmabuf | `luna-control dmabuf` — czy global jest ogłoszony i ile formatów/modyfikatorów |
| diagnostyka kursora | `luna-control cursor` — bieżący obraz/kształt, powierzchnia klienta, liczba `wl_pointer` i slot fokusu; `SELENE_TRACE_POINTER=1` loguje każdą zmianę fokusu wskaźnika |
| diagnostyka schowka | `luna-control clipboard` — rodzaj źródła (`wayland`/`control`), MIME, liczba urządzeń danych (w tym Xwayland) i selekcja pierwotna; przy `SELENE_STATS=1` logowane są też `get_data_device`/`set_selection` |
| **GPU (nested)** | `SELENE_GPU=1` → `libluna_render::gpu::GpuRenderer` (EGL surfaceless) zamiast CPU; brak EGL = automatyczny fallback, sesja żyje. Szczegóły: [`docs/gpu.md`](gpu.md) |
| test X11 | `scripts/x11-smoke.sh` (`make smoke-x11`): Xwayland + `xclock`, SSD maksymalizacja, kursor klienta X11 (GTK4 przez Xwayland) i zamknięcie |
| X11 CSD (Firefox itd.) | ruch: `_NET_WM_MOVERESIZE` w `_NET_SUPPORTED` **oraz** self-move przez `ConfigureRequest`; maksymalizacja: `_NET_WM_STATE` z akcją 2 i dwoma atomami — cel liczony raz na komunikat |
| `xdg_surface.set_window_geometry` | widoczny prostokąt vs bufor z cieniami CSD |
| wejście w klientach | „implicit grab” przycisków: ruch i zwolnienie wracają do klienta, który dostał wciśnięcie; `zwp_relative_pointer_v1` dla gier/mouselooku |
| multi-monitor | obszary robocze per wyjście, panel rezerwuje miejsce tylko u siebie |

## Braki — kolejność wg wpływu na codzienną pracę

1. **`xdg_toplevel` v2+** — `wm_capabilities`, `configure_bounds`, stan `tiled`;
   dziś ogłaszamy v1, więc klienty nie dostają pełnej informacji o kafelkowaniu.
2. **`buffer_scale` / `buffer_transform`** — pełne HiDPI (także ułamkowe) i obrót
   ekranu; `wp_fractional_scale_v1` już ogłasza skalę 1×.
3. **`ext_session_lock` / screencopy** — standardowa blokada sesji i zrzuty ekranu
   aplikacji; dziś `luna-lock` używa warstwy `overlay`.
4. **`wp_presentation_time`** — znaczniki prezentacji klatki (gry, pomiary opóźnienia).
5. **`zwp_xwayland_keyboard_grab_v1`** — grab klawiatury dla aplikacji X11
   (pełnoekranowe gry, maszyny wirtualne, zdalny pulpit).
6. **`zwp_pointer_gestures_v1` / `zwp_tablet_manager_v2`** — gesty touchpada i tablety
   graficzne (wymaga wejścia wielodotykowego z backendu).
7. **`zwlr_output_management_v1`, `zwlr_screencopy_v1`** — zmiana trybów i zrzuty
   ekranu narzędziami trzecich stron (dziś: `luna-control`).
8. **X11: XDND i skalowanie `_XWAYLAND_GLOBAL_OUTPUT_SCALE`** — przeciąganie między
   X11 a Waylandem i HiDPI dla okien X11.
9. **Kadrowanie `wp_viewport.set_source`** — destination już skaluje bufor; wycinek
   źródłowy wymaga jeszcze shadera/CPU cropu.
10. **Wieloplanowe YUV** — formaty są już negocjowane i przekazywane do EGL;
    ostateczna konwersja zależy od sterownika (na Mesa działa, na NVIDIA bywa
    wymagany shader YUV).

Akcje DnD (`set_actions`/`action`) są już negocjowane wewnątrz sesji: cel dostaje
`wl_data_offer.action`, źródło `wl_data_source.action`, a `Shift` przełącza
kopiowanie ↔ przenoszenie. `ext_data_control_v1` nadal nie ma drag & drop
(ograniczenie protokołu).

Most schowka obsługuje **tekst** w obie strony, a aplikacje libLuny potrafią
dodatkowo kopiować i wklejać **dowolne typy MIME** (`image/png`, `text/html`,
`text/uri-list`) oraz korzystać z **selekcji pierwotnej** (middle-click,
`Frame::copy_to_primary`/`paste_from_primary`). Do zrobienia zostaje most
**X11 → obraz** (XWM cache'uje jeden typ tekstowy). Historia schowka jest już
dostępna w [`luna-clipboard`](clipboard.md).

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
oraz — zgodnie z `xdg-shell` — **nigdy nie dostają fokusu klawiatury**. Klik w menu
podnosi je tylko w kolejności `z`; fokus zostaje na oknie nadrzędnym. Nadawanie
fokusu popupowi (dawniej robione przez `set_keyboard_focus`) powodowało, że klienty
X11/GTK/Chromium traktowały to jako zamknięcie menu: menu znikało już przy `pressed`,
więc `released` i wybór pozycji ginęły. Dotyczyło to zarówno menu X11 (Chrome,
Thunar, terminale), jak i `xdg_popup`.

## Kursor X11 („X” zamiast strzałki)

Objaw: w aplikacji X11 (np. Zed przez Xwayland) widać literę **X** zamiast
strzałki, a w natywnych klientach Wayland kursory są poprawne.

Przyczyna: Xwayland używa wbudowanego `X_cursor` dla okien, które nie ustawiły
własnego kursora, gdy w systemie **nie ma motywu `default`** (a `Pagan-Cursors`
bywa pustym opakowaniem z samym `Inherits`). Naprawa jest trójwarstwowa:

- `selene/src/xwm.rs` ustawia kursor okna głównego X11 na `XC_left_ptr` z fontu
  `cursor` — okna potomne dziedziczą go, więc działa nawet bez motywu XCursor;
- instalator tworzy `<prefix>/share/icons/default/index.theme` (`Inherits=Adwaita`),
  o ile użytkownik/dystrybucja nie mają własnego wyboru;
- `luna-session` wybiera pierwszy motyw z **niepustym** `cursors/` i eksportuje
  `XCURSOR_THEME`, `XCURSOR_SIZE` i `XCURSOR_PATH` (libXcursor nie zagląda
  domyślnie do `/usr/local/share/icons`).

## Znane problemy do zbadania

- **Zed przez XWayland potrafi „zjeść” cały rdzeń** (97% CPU, kilka GB RAM) mimo
  poprawnego UI. Podejrzenie: pętla `ConfigureNotify`/expose w XWM albo brak
  `_NET_WM_SYNC_REQUEST`. Do reprodukcji na izolowanej sesji z `DISPLAY` Xwaylanda.
- `xdg_popup.reposition` jest obsługiwany (przelicza pozycję i wysyła świeżą
  konfigurację). `grab` jest realizowany przez zwykłe trasowanie wejścia i
  zamykanie po kliknięciu poza menu.
- **Bąble Chrome/Ecosia rysowane jako `wl_subsurface`** (menu rozszerzeń, menu
  profilu): wskaźnik trafia teraz w **powierzchnię potomną** (`wl_pointer.enter`
  z jej `wl_surface` i współrzędnymi liczonymi od jej początku), bo Chromium
  hit-testuje je po tej powierzchni. Wcześniej `enter`/`motion` szły tylko do okna,
  więc bąbel nie dostawał ani podświetlenia, ani kliknięć (klik zamykał menu jak
  „poza nim”). Menu `xdg_popup` (⋮, PPM, podmenu) działały i działają.
  Ryzyko: powierzchnie potomne innych klientów (wideo/GL) też dostają `enter` —
  klient, który ich nie zna, ignoruje zdarzenie, ale warto to obserwować.
- **Nowe menu pod nieruchomym kursorem** wymaga wysłania `wl_pointer.enter` bez
  ruchu myszy. Pętla główna robi to przez `seat::refresh_pointer_focus` po każdej
  zmianie stosu okien; bez tego przeglądarki nie podświetlały pozycji menu, dopóki
  nie ruszyłeś myszą (a pierwszy klik bywał traktowany jak klik poza menu).
