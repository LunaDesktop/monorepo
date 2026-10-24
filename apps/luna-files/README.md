# Luna Files (`luna-files`)

Pierwsza „duża” aplikacja uniwersum Luna: menedżer plików zbudowany **wyłącznie na
libLunie** (`libluna-platform` + `libluna-widgets`), bez GTK, uruchamiany na
kompozytorze Selene.

Dostępne binarki:

| Binarka | Opis |
|---|---|
| `luna-files` | podstawowa nazwa aplikacji (okno „Pliki”) |
| `gaminedes` | alias — ta sama aplikacja, inna nazwa okna i App ID |

## Co już działa

**Nawigacja i widok**
- breadcrumb z klikalnymi segmentami, `Ctrl+L` — edycja ścieżki, `Backspace` — w górę,
  `Alt+←/→` — historia, przyciski myszy Back/Forward;
- **lista z kolumnami** (Nazwa / Rozmiar / Typ / Modyfikacja) i klikalnymi nagłówkami
  sortowania (strzałka pokazuje kierunek), **siatka** kafelków z ikonami typów plików;
- sortowanie (nazwa/rozmiar/data/typ, rosnąco/malejąco), ukrywanie plików (`Ctrl+H`),
  filtr na żywo (`Ctrl+F`), panel boczny chowany `F9`;
- zakładki (`Ctrl+T`, `Ctrl+W`, `Tab`), widok dzielony (`F3`), podgląd pliku (`Space`:
  tekst/kod oraz PNG, dla reszty metadane);
- wielokrotne zaznaczanie: `Ctrl+klik`, `Shift+klik` (zakres), `Shift+↑/↓`, `Ctrl+A`;
  belka statusu pokazuje liczbę **i łączny rozmiar** zaznaczonych;
- menu kontekstowe (prawy przycisk) i menu główne (☰) — wszystkie napisy przez i18n.

**Pliki**
- kopiowanie/wycinanie/wklejanie (`Ctrl+C/X/V`) z **dialogiem konfliktów**
  (Zastąp / Pomiń / Zmień nazwę / Zastąp wszystkie);
- przenoszenie do kosza zgodnego z **XDG Trash Spec** (`Delete`) i trwałe usuwanie
  (`Shift+Delete`), **przywracanie z kosza** na pierwotną ścieżkę oraz opróżnianie
  kosza z potwierdzeniem;
- zmiana nazwy (`F2`), nowy folder (`Ctrl+N`), odświeżanie (`F5`/`Ctrl+R`),
  otwieranie plików przez `xdg-open`, terminal w katalogu (`F4`).

**Urządzenia**
- panel boczny: miejsca XDG (z `~/.config/user-dirs.dirs`), kosz, zamontowane dyski
  z wolnym miejscem (rozmiar z `statvfs`);
- montowanie/odmontowanie przez UDisks2 (`udisksctl` → D-Bus, hasło pyta `luna-polkit`);
- lista odświeża się co 2 s, więc podłączenie pendrive'a pojawia się „samo”.

**Operacje w tle**
- kolejka zadań w osobnym wątku (`ops.rs`): postęp w belce statusu z **prędkością
  i szacowanym czasem**, anulowanie (`✕` lub `Escape`), wynik zbiorczy w powiadomieniu;
  UI nigdy nie blokuje się na dużym kopiowaniu.

## Architektura

| Plik | Rola |
|---|---|
| `model.rs` | wpisy katalogu, sortowanie, filtr, rozmiary i daty (testy bez UI) |
| `list.rs` | lista z kolumnami, nagłówki sortowania, wirtualizacja wierszy |
| `trash.rs` | kosz XDG: `files/` + `info/*.trashinfo`, kolizje nazw, `EXDEV` |
| `places.rs` | miejsca XDG, montowania (`/proc/self/mountinfo`), `statvfs`, UDisks2 |
| `ops.rs` | wielowątkowa kolejka kopiuj/przenieś/usuń/zmień nazwę |
| `app.rs` | stan aplikacji, historia, zakładki, powiadomienia, pętla klatki |
| `ui.rs` | layout, rysowanie, obsługa myszy/klawiatury, menu |

## Skróty

| Skrót | Akcja |
|---|---|
| `Ctrl+L` | wpisz ścieżkę |
| `Ctrl+F` | szukaj w katalogu |
| `Ctrl+T` / `Ctrl+W` | nowa karta / zamknij kartę |
| `Ctrl+C` / `Ctrl+X` / `Ctrl+V` | kopiuj / wytnij / wklej |
| `Ctrl+A` | zaznacz wszystko |
| `Ctrl+H` | ukryj/pokaż pliki ukryte |
| `F9` | pokaż/ukryj panel boczny |
| `Ctrl+N` | nowy folder |
| `F2` / `F3` / `F4` / `F5` | zmiana nazwy / dwa panele / terminal / odśwież |
| `Space` | podgląd |
| `Delete` / `Shift+Delete` | do kosza / trwale usuń |
| `Escape` | zamknij podgląd/menu, wyczyść zaznaczenie |

## Następne kroki

- MTP/PTP (telefony) przez GVFS/libmtp, LUKS i montowanie partycji systemowych;
- miniatury obrazów i wideo, podgląd PDF, kolumny (Miller), masowa zmiana nazw;
- archiwa (zip/tar/7z) bez zewnętrznych aplikacji;
- bezpośrednie sygnały UDisks2 (`InterfacesAdded/Removed`) zamiast odpytywania;
- systemowy schowek dla „Kopiuj ścieżkę” (wymaga API schowka w `libluna-platform`);
- „Otwórz za pomocą…” z wyborem aplikacji i zapamiętywaniem domyślnych.
