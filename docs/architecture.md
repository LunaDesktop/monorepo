# Architektura libLuny

Nazewnictwo projektu (analogia do GTK / GNOME / Mutter):

| Warstwa | Odpowiednik GNOME | Odpowiednik KDE | Nasza nazwa |
|---|---|---|---|
| Toolkit (biblioteka) | GTK | Qt | **libLuna** |
| Środowisko / powłoka | GNOME | KDE Plasma | **Luna** |
| Kompozytor Wayland | Mutter | KWin | **Selene** |

## 1. Warstwy (mapowanie pojęć GTK/GDK/Gsk/libadwaita → libLuna)

| Rola | Odpowiednik w GTK/Adw | Moduł libLuny | Stan |
|---|---|---|---|
| Prymitywy: geometria, kolor, skala, zdarzenia, tokeny | grunt/pango types | `libluna-core` | ✅ M0 |
| Rysowanie 2D, buforowanie, clip | Gsk (+Cairo) | `libluna-render` | ✅ M0 |
| Tekst: shaping, metryki, rasteryzacja | Pango | `libluna-render::text` (fontdue) | ✅ M0 (docelowo cosmic-text) |
| Warstwa platformy / okna / wejście | GDK | `libluna-platform` | ✅ M0 |
| Silnik układu (measure/allocate) | GtkLayoutManager | `libluna-layout` (docelowo; dziś `libluna-widgets::layout`) | ⏳ M2 |
| Widgety bazowe | Gtk* | `libluna-widgets` | ⏳ M2–M3 |
| Zestaw „libadwaita” (Card, HeaderBar, Sidebar…) | libadwaita | `libluna-adwaita` (docelowo) | ⏳ M4–M5 |
| Pętla zdarzeń i aplikacja | GtkApplication / GMainLoop | `libluna-app` (docelowo; dziś `libluna-platform::run`) | ⏳ M4 |

Fasada `libluna` (crate `libluna/crates/libluna`) zbiera warstwy w jedno wejście dla aplikacji.

## 2. Decyzja (podjęta): czysty Rust

Wybraliśmy **czysty Rust**, bez GObject/glib i bez GTK/Qt. Uzasadnienie:

- **Lekkość**: brak runtime glib/gobject; statycznie linkowany, mały zestaw zależności.
- **Bezpieczeństwo**: zarządzanie pamięcią przez kompilator zamiast ręcznego refcountingu.
- **Nowoczesny toolchain**: `cargo` + `clippy` + `rustfmt`.
- **Integracja**: kompozytor Selene jest zwykłym serwerem Wayland, więc wystarczy standardowy
  protokół (`xdg-shell`, a dla paneli `wlr-layer-shell`).

Rozważane, odrzucone alternatywy:

- **C + GObject + Cairo/Pango** — natywne dla obecnego KartONDE i najtańsze w migracji, ale
  zostaje w świecie glib i wciąga ciężki GObject.
- **Rust + C ABI / mostek GObject** — daje nowoczesny rdzeń, lecz dodaje dwie granice FFI.

## 3. Rysowanie i tekst

- **Graf sceny** (`libluna-render::scene`) jest niezależny od backendu; budują go widgety.
- **Kontrakt backendu**: `libluna_render::Renderer` (`renderer.rs`) — `resize`, `draw`,
  `data`, `encode_png`, `text_engine`; platforma trzyma `Box<dyn Renderer>`.
  Dzięki temu backend GPU może dojść bez zmian w widgetach (szczegóły i pomiary:
  [`docs/rendering.md`](rendering.md)).
- **Backend M0**: CPU (`tiny-skia`) z antyaliasingiem — kształty, zaokrąglenia, obramowania,
  linie, clip. Bufor RGBA8888 trafia do `wl_shm` (konwersja do BGRA/`Argb8888`).
- **Tekst M0**: `fontdue` (font Adwaita Sans z systemu). To świadome uproszczenie — bez
  kerningu i pism złożonych. Docelowo `cosmic-text` (shaping, fallback fontów), bez zmiany
  kontraktu `measure`/`draw`.
- Później: opcjonalny backend GPU i śledzenie uszkodzeń (damage) do minimalizacji odświeżeń.

## 4. Model widgetu (wspólny dla wszystkich warstw)

```
Widget:
  measure(constraints) -> Size            // faza 1: ile chce
  allocate(rect)                          // faza 2: gdzie trafia
  draw(scene, theme)                      // rysowanie sceny
  handle(event) -> Handled | Propagate    // wejście
  focus/grab/hover state
```

- Layout: dwuprzebiegowy **measure/allocate** (jak GTK/Flutter) — przewidywalny i prosty.
- Wejście: jedno drzewo zdarzeń, propagacja od liści wzwyż, fokus klawiatury, grab wskaźnika.

## 5. Komunikacja z kompozytorem (Selene)

- Okna aplikacji: `xdg-shell` z dekoracjami po stronie klienta (CSD) — sami rysujemy HeaderBar.
- Panele/powłoka: `wlr-layer-shell` (jak `karton-shell`).
- Do rozważenia później: `xdg-decoration`, `fractional-scale-v1`, `viewporter`,
  `foreign-toplevel-management` (integracja z panelem zadań), `idle-inhibit`.
- Buforowanie: start od `wl_shm` (CPU), później opcjonalny backend EGL/`wl_drm`.

## 6. Motywowanie

- Tokeny: `spacing` (3/6/12/18/24), `radius` (6/9/12/pill), `typography`
  (title/heading/body/caption), **metryki kontrolek** (`Metrics`) oraz palety `light`/`dark` + `accent`.
- Warstwa „adwaita-look”: karty, listy, nagłówki, przyciski, przełączniki — zgodne
  wizualnie z makietami.
- Geometria kontrolek nie jest już stałą w widgetach — wynika z `Theme::metrics`, więc styl
  steruje też rozmiarami (HeaderBar, Switch, Button…).
- Opcjonalnie później: mini-podzbiór CSS po klasach stylu; tokeny mają pierwszeństwo.

## 7. Świadomie odłożone (poza MVP)

- Dostępność: AT-SPI (przez D-Bus) — niezbędna dla realnego pulpitu, ale faza późniejsza.
- IME / `text-input-v3`, klawiatura wirtualna.
- Schowek: **kopiowanie i wklejanie** działają (`Frame::copy_to_clipboard`,
  `Frame::paste_from_clipboard` → `Event::Paste`, `wl_data_device`). Pola tekstowe
  korzystają z tego przez `libluna-widgets::clipboard_command` (Ctrl+C/X/V),
  a aplikacja wysyła tekst do `Frame` i kieruje `Event::Paste` do aktywnego pola
  (terminal, demo, Ustawienia, Pliki). Drag & drop — później.
- Animacje i easing, backend GPU.
- Stabilność API i zasady wersjonowania: [`docs/api-stability.md`](api-stability.md).
- Testy (jednostkowe, integracyjne, dymna sesja): [`docs/testing.md`](testing.md).
