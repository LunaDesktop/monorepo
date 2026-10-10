# Roadmap libLuny

Kamienie milowe są ułożone tak, aby **każdy kolejny etap dał się uruchomić i zobaczyć**,
a na końcu odtworzył realne aplikacje środowiska Luna z makiet.

## M0 — Fundament (szkielet + pierwsze okno) — ✅ UKOŃCZONE
- Struktura workspace (cargo), build, `clippy`, `rustfmt`, testy.
- `libluna-core`: geometria (`Point/Size/Rect/Insets`), kolor, skala ekranu, typy zdarzeń,
  tokeny stylu (spacing/radius/typografia/paleta light+dark).
- `libluna-render`: graf sceny + backend CPU (`tiny-skia`), tekst (`fontdue`), clip, linie.
- `libluna-platform`: połączenie Wayland, `xdg-shell` (CSD), bufor `wl_shm`, `calloop`,
  wejście (`seat`, `xkbcommon`), resize/skala, zamknięcie okna.
- **Efekt:** okno z HeaderBar-em (tytuł + `[—] [□] [✕]` + „burger”), rysowane CSD, oraz scena
  makiety Ustawień. Render offline do PNG w obu motywach: `docs/images/settings-*.png`.
- Do weryfikacji na żywo: uruchomienie na kompozytorze Selene (`cargo run -p luna-demo`).

## M0.5 — Wielojęzyczność i wygląd — ✅ UKOŃCZONE
- `libluna-i18n`: system tłumaczeń bez gettext (klucz = angielski napis). **Każdy język w osobnym
  pliku** `i18n/<kod>.lang` (wbudowane pl/en/de przez `include_str!`) oraz doładowywanie
  w runtime z `~/.config/luna/i18n`, `~/.local/share/luna/i18n`, `/usr/share/luna/i18n`;
  wykrywanie z `LANG`/`LC_*`, globalne przełączanie, `languages()`/`language_name()`, makro `t!`.
- `libluna-core`: `ThemePreference` (Auto/Jasny/Ciemny), `Accent` (9 kolorów), `ThemeSettings`
  z rozwiązywaniem motywu i zapisem do `~/.config/luna/theme.conf`.
- `libluna-platform`: zdarzenia wejścia (wskaźnik, klawiatura, scroll) przekazywane do `Frame`;
  sterowanie motywem z aplikacji (`set_preference`/`set_accent`) z natychmiastowym przerysowaniem.
- `luna-demo`: interaktywny panel wyglądu (segmenty trybu, paleta akcentów, wybór języka),
  chowanie **paska dolnego** i **paska bocznego** oraz przeciąganie jego krawędzi (drag, 150–460 px);
  skróty `l`/`a`/`g`/`b`/`s`; zrzuty w kilku językach/motywach w `docs/images/`.

## M1 — Render
- Graf sceny: prostokąty, zaokrąglone prostokąty, obcinanie (clip), cień, obrazy/ikony SVG.
- `libluna-text`: shaping/układanie (Pango albo cosmic-text), fallback fontów, ellipsis.
- `damage`/częściowe odświeżanie.

## M2.5 — Widgety (start) — ⏳ W TOKU
- `libluna-widgets`: widgety wielokrotnego użytku w modelu immediate — `ViewSwitcher`, `NavList`,
  `Menu` (popover `☰`), `Button` (warianty + chevron), `Switch`, `Slider`, `DropDown`
  (rozwijany przez `Menu`), `HeaderBar` (pasek tytułu z kontrolkami okna), `TabBar`/`TabView`
  (zakładki: pasek + model stron), `BottomSheet` (dolny panel modalny), `Pager` (karuzela stron),
  `PathBar` (okruszki), `Entry` (pole tekstowe) i `SearchEntry` (wyszukiwanie na jego bazie),
  `ItemView` (pliki/katalogi: siatka
  lub lista) i `ScrollView` (przewijanie), oraz `Stepper`, `CheckButton` i `RadioGroup`
  (krokowiec, checkbox, wybór jednokrotny); do tego `Card` i `ActionRow` (karty i wiersze
  ustawień), `Toast` (powiadomienie nakładkowe z akcją) i `StatusPage` (ekran stanu: ikona,
  tytuł, opis, akcja — np. „pusty stan” listy); wspólny `PaintCtx`; testy hit-testingu bez okna.
- `libluna-platform`: realne akcje okna z `Frame` (`close`/`minimize`/`toggle_maximize`).
- Demo używa widgetów zamiast ręcznego rysowania (pasek boczny, wybór motywu/widoku, menu `☰`,
  przycisk „Opcje”, przełączniki, suwak, wybór języka, pasek tytułu, zakładki `TabBar`/`TabView`).
- Motywy: wariant (light/dark/auto) + akcent, z trwałością. **Component metrics**: rozmiary
  i paddingi kontrolek wyniesione z widgetów do tokenów (`Theme::metrics`), więc styl steruje
  też geometrią (README, „Własny wygląd”).
- Układ: **retencyjne drzewo `measure`/`allocate`** (`libluna-widgets::layout`) — `Node`
  (`VBox`/`HBox`, `Padding`, `Expanded`, `Center`, `Spacer`, `Label`, `Fixed`), `Constraints`,
  `Layout::fit`; przykład karty złożonej z drzewa (`bin/layout.rs` → `docs/images/layout.png`).
- Dalej: **style instalowane jako pliki** (jak w GNOME/KDE) z **oknem podglądu** przy wyborze,
  a potem scroll/overflow w `TabBar`, migracja `luna-demo` na drzewo układu.

## M2 — Układ + widgety bazowe
- Dwuprzebiegowy silnik measure/allocate — ✅ **częściowo** (silnik + kontenery `VBox`/`HBox`,
  `Padding`, `Expanded`, `Center`, `Spacer`, `Label`; do zrobienia: migracja demo i zaokrąglenia API).
- `Box` (h/v), `Label`, `Icon`, `Button`, `Separator`, `Spacer`.
- Stany: hover/pressed/disabled/focus + pierścień fokusu klawiatury.

## M3 — Sterowanie i listy
- `Switch`, `Slider`, `CheckButton`, `RadioButton`, `Entry`, `DropDown`, `Stepper`.
- `ScrollView`, `ListView`, `GridView` (siatka kafelków) z zaznaczaniem i aktywacją.
- `Sidebar` (sekcje + wiersze z ikonami, grupy rozwijane).

## M4 — „Chrome” okna
- `Window`, `HeaderBar` (slot start/center/end), `WindowControls` (`— □ ✕`).
- `MenuButton` + `Popover` (menu „burger”: pozycje, separatory, skróty, podmenu).
- `TabBar` / `ViewSwitcher` (segmentowy: Lista | Siatka).
- `PathBar` (okruszki z `>` i `▾`), `SearchBar`/`SearchEntry`.
- Bottom bar / pasek statusu.

## M5 — Zestaw „adwaita”
- `Card` (nagłówek sekcji + wiersze), `ActionRow` (tytuł/podtytuł + suffix).
- `PreferencesGroup`, `Banner`, `Toast`, `StatusPage`.
- `Spinner`, `Tooltip`, ikony symboliczne motywowane.

## M6 — Odtworzenie makiet (cel!)
- **Luna Settings** — panel boczny (Przegląd/Zaawansowane), karty z `Switch`/`DropDown`/
  `Slider`, pasek akcji, pasek statusu.
- **Luna Files** — HeaderBar z `PathBar` + wyszukiwarką + przełącznikiem widoku + kontrolkami
  okna, panel boczny (Miejsca/Urządzenia/Sieć), widok siatki, popover opcji/sortowania,
  bottom bar z wolnym miejscem.

## M7 — Migracja z GTK4
- Najpierw `karton-settings` (mniejszy, `window.c` ~590 linii + strony) — strona po stronie.
- Potem `karton-files` (`window.c` ~5100 linii) — etapami.
- Opcjonalnie ujednolicenie `karton-shell` na libLunę (dziś cairo/pango bezpośrednio).

## Później (poza MVP)
- Dostępność AT-SPI, IME, schowek, drag & drop, animacje, backend GPU, mini-CSS dla motywów.

## Inwentarz widgetów (z makiet → moduł docelowy)

| Widget | Moduł | Makieta |
|---|---|---|
| Window + CSD HeaderBar | `libluna-adwaita` | obie |
| WindowControls `— □ ✕` | `libluna-adwaita` | obie |
| MenuButton + Popover (burger) | `libluna-widgets` | obie |
| TabBar / ViewSwitcher | `libluna-widgets` | Settings, Files |
| BottomSheet | `libluna-widgets` | Settings, Files |
| Pager (karuzela stron) | `libluna-widgets` | (przeglądarka, onboarding) |
| Sidebar (sekcje, wiersze) | `libluna-adwaita` | obie |
| PathBar (okruszki) | `libluna-widgets` | Files |
| Entry (pole tekstowe) | `libluna-widgets` | Settings, Files |
| SearchEntry / SearchBar | `libluna-widgets` | Files |
| GridView / ListView | `libluna-widgets` (`ItemView`) | Files |
| ScrollView | `libluna-widgets` | obie |
| Card + ActionRow | `libluna-widgets` | Settings |
| Switch | `libluna-widgets` | Settings |
| Slider (+ etykieta %) | `libluna-widgets` | Settings |
| DropDown | `libluna-widgets` | Settings |
| Button (pill/suggested/flat) | `libluna-widgets` | Settings |
| Stepper / Check / Radio | `libluna-widgets` | Files (popover) |
| Toast | `libluna-widgets` | Settings, Files |
| StatusBar / bottom info | `libluna-widgets` | obie |
| StatusPage (pusty stan) | `libluna-widgets` | Files |

## Kryterium „gotowca” (Definition of Done dla MVP)
Odtworzenie **obu makiet** (Settings i Files) wyłącznie na libLunie, bez GTK4, z jasnym i
ciemnym motywem, uruchamialne na kompozytorze Selene, w rozsądnym rozmiarze binarki
i zużyciu pamięci względem odpowiedników GTK4.
