# Luna Calculator (`luna-calculator`)

Kalkulator standardowy i naukowy środowiska Luna na libLunie. Cała arytmetyka
wykonuje się lokalnie w aplikacji — bez GTK, bez bazy danych i bez sieci.

![Tryb standardowy](../../docs/images/calculator-standard.png)

![Historia obliczeń](../../docs/images/calculator-history.png)

![Tryb naukowy](../../docs/images/calculator-scientific.png)

![Menu aplikacji](../../docs/images/calculator-menu.png)

![Ustawienia](../../docs/images/calculator-settings.png)

![O kalkulatorze](../../docs/images/calculator-about.png)

## Funkcje

- **Standard** — cztery działania, procenty, zmiana znaku, nawiasy i pamięć
  ostatniego wyniku (`Ans`).
- **Naukowy** — trygonometria (`sin`, `cos`, `tan` oraz funkcje odwrotne),
  logarytmy (`ln`, `log`), pierwiastki, potęgi, silnia, odwrotność, `π`, `e`,
  notacja naukowa (`EXP`, `10ˣ`) i przełącznik `DEG`/`RAD`.
- **Podgląd na żywo** — wynik pojawia się pod wyrażeniem jeszcze przed `=`.
- **Klawiatura** — cyfry, operatory, `Enter` (=), `Backspace`, `Esc` (czyść),
  `Ctrl+C` (kopiuj wynik), `Ctrl+V` (wklej wyrażenie), `Ctrl+H` (historia),
  `Ctrl+,` (ustawienia), `F1` (o aplikacji).
- **Menu** — hamburger w pasku tytułu z **Historią**, **Ustawieniami**
  i **O kalkulatorze**.
- **Historia** — ostatnie 100 obliczeń, kliknięcie wstawia wynik, przycisk
  **Wyczyść historię**; zapis w `~/.local/share/luna/calculator/history.tsv`.
- **Ustawienia** — jednostka kąta (`DEG`/`RAD`) i tryb startowy
  (`Standard`/`Naukowy`), zapisywane w `~/.config/luna/calculator.conf`.
  Tryb startowy działa przy następnym uruchomieniu; bieżący tryb zmienia
  przełącznik w pasku tytułu.
- **Responsywność** — standardowy tryb startuje w węższym oknie, naukowy w szerszym;
  przy wąskim oknie panel naukowy układa się nad standardowym. Długie wyrażenie
  jest dopasowywane rozmiarem, a potem skracane wielokropkiem od lewej.
- **Niejawne mnożenie** — `2π`, `3(4+5)` i `2sin(30)` działają bez dodatkowego `×`.
- **Procent i modulo** — `%` jest postfiksowe (`50%` = `0.5`), a `mod` liczy resztę
  z dzielenia (`7 mod 3` = `1`).

## Uruchamianie

```sh
cargo run --manifest-path apps/luna-calculator/Cargo.toml
cargo run --manifest-path apps/luna-calculator/Cargo.toml -- --scientific
cargo test --manifest-path apps/luna-calculator/Cargo.toml --lib
cargo run --manifest-path apps/luna-calculator/Cargo.toml --bin preview
```

## Skróty

| Klawisz | Działanie |
|---|---|
| `0`–`9`, `.` | wpisywanie liczby |
| `+`, `-`, `*`, `/`, `^`, `%`, `!` | operatory |
| `(` `)` | nawiasy |
| `Enter` | oblicz (`=`) |
| `Backspace` | usuń ostatni znak lub całą funkcję (`sin(`) |
| `Esc`, `Delete` | wyczyść wyrażenie |
| `Ctrl+C` | skopiuj wynik |
| `Ctrl+V` | wklej wyrażenie |
| `Ctrl+H` | otwórz historię |
| `Ctrl+,` | otwórz ustawienia |
| `F1` | o aplikacji |

## Prywatność

Aplikacja nie zna konta użytkownika i nie wykonuje żadnych zapytań sieciowych.
Lokalnie zapisuje historię obliczeń (`history.tsv`, prawa `0600`) oraz jednostkę
kąta i tryb startowy w `~/.config/luna/calculator.conf`. Historię można
wyczyścić z panelu; reszta stanu żyje w pamięci procesu.

## Licencja

GNU GPL v3.0 lub nowsza (`GPL-3.0-or-later`) — pełny tekst w [`LICENSE`](LICENSE).
© 2026 Luna Project.
