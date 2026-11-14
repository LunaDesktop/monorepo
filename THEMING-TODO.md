# Motywy 2.0 — TODO / roadmapa

Status: **plan na kolejną wersję** (niezaimplementowane).
Dokument opisuje kierunek rozwoju silnika motywów tak, aby motywy mogły
dostarczać także grafiki (przyciski, pola, przełączniki itd.) i pozostały
spójne z resztą libLuny.

Zasady nadrzędne:

- **Wsteczna zgodność** — każdy istniejący plik `.theme` działa dalej.
- **Zawsze jest fallback** — brak grafiki = dzisiejszy rysunek wektorowy.
- **Jeden słownik nazw** — te same nazwy tokenów i assetów w całym środowisku.
- **Motyw nie psuje układu** — grafiki nie zmieniają metryk bez jawnej deklaracji.
- **Zero I/O na klatkę** — assety ładowane raz, cache po ścieżce i mtime.
- **Bezpieczeństwo** — tylko pliki z katalogu motywu, bez `..` i symlinków na zewnątrz.

---

## 1. Grafiki dla kontrolek (najważniejsze)

### 1.1 Model assetów

- [ ] Sekcja `assets:` w pliku `.theme`:
  ```ini
  assets:
  -button: assets/button.png
  -button-hover: assets/button-hover.png
  -button-active: assets/button-active.png
  -button-disabled: assets/button-disabled.png
  ```
- [ ] Katalog motywu może być folderem (nie tylko pojedynczym plikiem):
  ```
  ~/.local/share/luna/themes/aurora/
    theme.theme
    assets/button.png
    assets/entry.svg
  ```
- [ ] Obsługa **PNG i SVG** (SVG przez `resvg`, już jest w zależnościach).
- [ ] Warianty gęstości: `button@2x.png`, wybór wg skali wyjścia.
- [ ] Weryfikacja: maks. rozmiar pliku, maks. wymiary, dozwolone rozszerzenia.

### 1.2 Kontrolki do pokrycia (z stanami)

- [ ] `button` — normal / hover / active / disabled / focus
- [ ] `entry` — normal / focus / disabled / placeholder
- [ ] `switch` — track + thumb (on/off)
- [ ] `check` i `radio` — off / on / indeterminate
- [ ] `slider` — track / fill / thumb (normal, active)
- [ ] `card` — tło, nagłówek, obramowanie
- [ ] `headerbar` — tło, przyciski okna (min/max/close)
- [ ] `menu` — tło, separator, podświetlenie pozycji
- [ ] `tabs` — aktywna/zawieszona karta
- [ ] `scrollbar` — tor i uchwyt
- [ ] `toast`, `tooltip`, `banner` — tło i obramowanie
- [ ] `panel` — pasek, wskaźniki, tray, kropki przestrzeni
- [ ] `launcher` — tło karty, kafel aplikacji, sidebar

### 1.3 Skalowanie: 9-slice

- [ ] Dla assetów rozciągalnych deklaracja marginesów:
  ```ini
  assets:
  -button: assets/button.png
  -button-slice: 12 12 12 12
  ```
- [ ] Nowa operacja w rendererze: `image_9slice(rect, image, insets)`.
- [ ] Fallback: brak `*-slice` = zwykłe rozciągnięcie (z ostrzeżeniem w walidatorze dla przycisków).

### 1.4 Ikony i zestawy ikon

- [ ] Nadpisania pojedynczych ikon w motywie (`icons: -files: assets/files.svg`).
- [ ] Motyw może wskazywać zewnętrzny zestaw freedesktop (`external-icon-set`).
- [ ] Spójne traktowanie ikon panelu (monochromatyczne) i aplikacji (kolorowe).

### 1.5 Kompozycja z obecnym rysowaniem

- [ ] Assety rysowane **pod** treścią (tekst/ikony zawsze nad obrazem).
- [ ] Gdy grafika nie istnieje → obecny kod wektorowy bez zmian.
- [ ] Motyw może mieszać: np. własne tło przycisku + systemowa ikona.

---

## 2. Rozszerzenie schematu motywu

- [ ] Nagłówek wersji: `schema: 2` (domyślnie 1, gdy brak).
- [ ] Warianty jasny/ciemny w jednym pliku:
  ```ini
  [light]
  colors:
  -window_bg: #fafafa

  [dark]
  colors:
  -window_bg: #131820
  ```
- [ ] Cienie/elevation jako tokeny: `shadow.card`, `shadow.panel`.
- [ ] Animacje jako tokeny: `motion.fast`, `motion.normal`, `motion.easing`.
- [ ] Typografia: rodzina i grubość (`font.family`, `font.weight`), pliki fontów w motywie.
- [ ] Dekoracje okien: styl paska tytułu, układ przycisków (lewo/prawo), wysokość.
- [ ] Panel: osobne warianty dla góra/dół/lewo/prawo + tray (ikony/nazwy).
- [ ] Scoping tokenów per widget (`button.radius`, `entry.padding`) zamiast tylko globalnych.
- [ ] Aliasy czytelne po ludzku dla nowych sekcji (`button:`, `window:`, `panel:`).

---

## 3. Architektura silnika

- [ ] `Theme` pozostaje `Copy` (tokeny). Assety w osobnym rejestrze:
  `ThemeAssets` / `AssetRegistry` w `libluna-core`.
- [ ] Leniwne ładowanie + cache `(ścieżka, mtime, skala)`; inwalidacja przy zmianie pliku.
- [ ] Katalog assetów zależny od identyfikatora motywu (nie ze ścieżki podanej w pliku).
- [ ] Wspólny resolver dla aplikacji i powłoki (te same assety w całym środowisku).
- [ ] Hot reload motywu bez restartu aplikacji (mtime katalogu motywu).
- [ ] Wydajność: atlas/tekstury współdzielone, brak dekodowania PNG na klatkę.
- [ ] Dostępność: tryb wysokiego kontrastu i skala tekstu mogą nadpisywać assety.
- [ ] Obrazy dekoracyjne nie mogą przechwytywać kliknięć (region wejścia bez zmian).

---

## 4. Narzędzia i walidacja

- [ ] `luna-theme-check <plik|katalog>` — walidacja kluczy, assetów, ścieżek i 9-slice.
- [ ] `luna-theme-preview <motyw>` — render podglądów kontrolek do PNG (jak obecny `preview`).
- [ ] Schemat JSON (`theme.schema.json`) do autouzupełniania w edytorach.
- [ ] Ostrzeżenia w Ustawieniach: „ten motyw używa nieobsługiwanego klucza”.
- [ ] Testy golden: kilka motywów × kilka kontrolek × jasny/ciemny.
- [ ] CI: walidacja przykładów z `examples/luna/themes/`.

---

## 5. Ustawienia (UX)

- [ ] Galeria motywów z miniaturami (render na żywo, nie statyczne PNG).
- [ ] „Edytuj motyw” i „Otwórz katalog motywu”.
- [ ] Eksport obecnych ustawień do pliku `.theme`.
- [ ] Import motywu (folder/zip) z walidacją i podglądem.
- [ ] Podgląd panelu (pozycja, float, round, tray) przed zastosowaniem.
- [ ] Informacja, które elementy pochodzą z motywu, a które są systemowe.

---

## 6. Dokumentacja

- [ ] Rozszerzyć stronę `docs/theming.html` (PL/EN) o sekcję `assets:`.
- [ ] Tabela wszystkich tokenów i assetów z wartościami domyślnymi.
- [ ] Przykładowy motyw z grafikami w `examples/luna/themes/` (folder + assety).
- [ ] `examples/luna/themes/README.md` — szybki start i link do tej roadmapy.
- [ ] Opisać zasady 9-slice i przygotowania grafik (przezroczystość, DPI).

---

## 7. Kolejność wdrażania (proponowane milestone'y)

- [ ] **M1 — fundament**: rejestr assetów, ładowanie PNG/SVG, `image_9slice`,
      `button` + `entry` + `switch`, fallback i cache.
- [ ] **M2 — reszta kontrolek**: stany hover/active/disabled/focus, panel, menu,
      karty, scrollbar, toast/tooltip.
- [ ] **M3 — schemat 2.0**: warianty light/dark, sekcje per widget, cienie,
      animacje, fonty, dekoracje okien.
- [ ] **M4 — UX**: galeria z miniaturami, import/eksport, podgląd panelu.
- [ ] **M5 — jakość**: `luna-theme-check`, schemat JSON, testy golden, dokumentacja.

---

## Poza zakresem (na razie)

- Shadery i efekty GPU w motywach.
- Animowane assety (APNG/WebP/Lottie).
- Motywy pobierane z sieci bez walidacji.
- Zmiana układu widgetów przez motyw (tylko wygląd, nie struktura).
