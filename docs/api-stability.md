# Stabilność API libLuny

libLuna jest dziś w wersji **0.0.x**. To oznacza, że drobne wersje mogą jeszcze
zmieniać API — ale nie dowolnie: ten dokument mówi, co jest traktowane jako
**kontrakt**, co jest **eksperymentalne**, i jak zamierzamy dojść do 1.0.

## 1. Powierzchnia publiczna

### Stable (traktujemy jak kontrakt)

| Obszar | Wejście | Gwarancja |
|---|---|---|
| Fasada toolkitu | `libluna::prelude::*`, `libluna::t!` | re-eksporty warstw, makro i18n |
| Aplikacja i okno | `libluna_platform::{run, AppConfig, Frame, LayerConfig}` | pętla zdarzeń, akcje okna, warstwy |
| Scena | `libluna_render::{Scene, DrawCommand, TextRun, ImageData}` | niezależna od backendu |
| Backend | `libluna_render::{Renderer, SoftwareRenderer}` | `Box<dyn Renderer>` w platformie |
| Tekst | `libluna_render::TextEngine` | metryki i rysowanie tekstu |
| Motyw | `libluna_core::{Theme, ThemeSettings, Accent, ThemePreference}` | tokeny i rozwiązywanie motywu |
| Zdarzenia | `libluna_core::{Event, KeyEvent, PointerEvent, ScrollEvent, Modifiers, ButtonState}` | model wejścia |
| Widgety bazowe | `Button`, `Entry`, `Switch`, `Slider`, `HeaderBar`, `NavList`, `ViewSwitcher`, `Menu`, `TabBar`, `ScrollView`, `ItemView`, `Toast`, `Card`, `ActionRow`, `StatusPage`, `Spinner`, `Tooltip`, `Banner`… | konstrukcja, `paint`, hit-testing |
| Schowek | `libluna_widgets::clipboard_command`, `TextClipboardCommand`, `TextInput::{copy, cut, paste}` | rozpoznawanie Ctrl+C/X/V i operacje na polu |
| Ikony | `IconName`, `icon::paint_icon`, `paint_icon_themed` | nazwany zestaw + tryb monochromatyczny |
| i18n | `tr`, `set_locale`, `Locale` | klucz = angielski tekst |

Zmiany w tych obszarach wymagają:
1. wpisu w `docs/` (co i dlaczego),
2. testu (jednostkowego lub w `session-smoke.sh`),
3. co najmniej jednej wersji z deprecation, jeśli da się ją sensownie dodać.

### Eksperymentalne (może się zmienić bez deprecation)

- retencyjne drzewo układu `libluna_widgets::layout` (`Node`, `Constraints`,
  `Layout::fit`) — docelowo osobna warstwa `libluna-layout`,
- pojedyncze, „bogate” widgety galerii (`TextView`, `ColumnView`, `Assistant`,
  `EmojiChooser`, `ColorChooser`, `FlowBox`, `MediaControls`…),
- `libluna_render::svg` (`scene_to_svg`, `decode_svg`) i `wallpaper`,
- wewnętrzne API aplikacji `luna-*` (panel, powiadomienia, system),
- szczegóły `libluna_platform` poza `run`/`Frame`/`AppConfig`.

### Wewnętrzne (bez gwarancji)

- pola struktur aplikacji (`Demo`, `SettingsApp`, `App` w Plikach),
- moduły `luna::*` używane przez powłokę,
- protokoły Waylanda obsługiwane przez Selene — patrz `docs/compositor.md`,
- układ plików `~/.config/luna/*` (poza tym, co opisane w `docs/`).

## 2. Zasady zależności

```text
libluna-core ──► libluna-render ──► libluna-widgets ──► libluna (fasada)
       │                │                  ▲
       └────────────────┴──► libluna-platform
```

- **Widgety nie znają platformy** (`libluna-platform` nie jest zależnością
  `libluna-widgets`). Dlatego schowek jest rozpoznawany w widgetach, ale
  wykonywany przez aplikację/`Frame`.
- **Aplikacje korzystają z fasady** (`libluna::prelude`) albo z pełnych nazw
  warstw — nie kopiują kodu platformy.
- **Backend GPU** wchodzi wyłącznie za `libluna_render::Renderer`; widgety nie
  mogą zależeć od EGL/wgpu.

## 3. Wersjonowanie

- `0.x.y`: `x` = zmiany API (mogą być łamiące, ale opisane w `docs/`),
  `y` = poprawki i dodatki wstecznie zgodne.
- `1.0`: zamrażamy tabelę **Stable** powyżej. Od tego momentu obowiązuje semver:
  usunięcie/zmiana sygnatury = major, dodanie = minor.
- Wewnętrzne crate’y (`selene`, `luna`, `luna-greeter`, aplikacje) mają własne
  wersje; nie muszą być synchronizowane z libLuną.
- Każdy workspace ma własny `Cargo.lock`; `scripts/workspaces.sh lock` odświeża
  je po zmianie wersji.

## 4. Jak sprawdzić, co jest publiczne

- `cargo doc --manifest-path libluna/Cargo.toml --no-deps` (wymaga `rustdoc`),
- `docs/` — opis warstw i decyzji,
- `libluna/crates/libluna/src/lib.rs` — faktyczna fasada i `prelude`,
- `libluna/crates/*/src/lib.rs` — re-eksporty (`pub use`).

Przed 1.0 nie utrzymujemy osobnego pliku „API diff”; zamiast tego każdy PR
zmieniający tabelę **Stable** aktualizuje ten dokument.

## 5. Definition of Done dla 1.0 (API)

1. Tabela **Stable** pokrywa wszystko, czego potrzebuje aplikacja trzecia
   (okno, wejście, scena, motyw, widgety bazowe, schowek, i18n, ikony).
2. Przykład `libluna/crates/libluna/examples/minimal.rs` buduje się **tylko** przez fasadę.
3. `docs/api-stability.md`, `docs/architecture.md` i `docs/rendering.md` są spójne
   z kodem (żadnych obietnic bez implementacji).
4. Testy: jednostkowe + `scripts/session-smoke.sh` przechodzą na release.
5. Zamrożony `Renderer` — backend GPU może dojść bez zmian w widgetach.
