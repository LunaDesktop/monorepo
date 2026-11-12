# Luna Welcome (`luna-welcome`)

Powitanie w środowisku Luna na libLunie: wersja systemu, najnowsze zmiany,
skróty do Sklepu/Ustawień/dokumentacji oraz — na obrazie live — wyraźne
przekierowanie do **Luna Installer**.

```sh
cargo run
cargo run -- --live        # wymuś tryb live (podgląd)
cargo test
cargo run --bin welcome-render -- --live --dark /tmp/welcome.png
```

## Zachowanie

- **Wykrywanie live ISO** sprawdza standardowe znaczniki live-boot/casper,
  `rd.live.image` oraz linię poleceń jądra (`boot=live`). Wynik można wymusić
  zmienną `LUNA_LIVE=1` (live) albo `LUNA_LIVE=0` (system zainstalowany).
- Na obrazie live pojawia się pasek **„You're running the Luna live image”**
  z przyciskiem **Install Luna**, który uruchamia `luna-installer`
  (albo program wskazany przez `LUNA_INSTALLER`).
- Na zainstalowanym systemie nie ma przycisku instalatora — zostają skróty do
  Sklepu, Ustawień i dokumentacji.
- **Dokumentacja** („Read the documentation”) otwiera adres z pakietu wydania
  („os-info”): `LUNA_DOC_URL` → `DOCUMENTATION_URL`/`DOC_URL`/`DOCS_URL`
  z `/etc/luna-release`, `/etc/pagan-release` lub `/etc/os-release` → lokalny
  `share/os-info/docs/index.html`. Gdy wydanie nic nie poda — domyślne
  `https://luna.paganlinux.eu/docs/`.
- Powitanie **nie zmienia dysku ani ustawień systemowych**. Uruchamia wyłącznie
  inne aplikacje (`luna-installer`, `luna-store`, `luna-settings`) lub
  `xdg-open` dla dokumentacji. Błąd uruchomienia trafia do paska komunikatu.
- Zamknięcie: kontrolki okna, `Esc` lub `Ctrl+W`. Na obrazie live `Enter`
  uruchamia instalator.
- **Autostart:** `luna-session` uruchamia powitanie przy każdym logowaniu.
  Checkbox w stopce **„Nie uruchamiaj ponownie po zalogowaniu”** zapisuje przy
  zamknięciu okna `$XDG_CONFIG_HOME/luna/welcome.conf` (`autostart = false`),
  który blokuje kolejne uruchomienia. Usunięcie pliku (albo `autostart = true`)
  przywraca powitanie.

## „Co nowego”

Treść pochodzi z pierwszego dostępnego pliku:

1. `LUNA_WELCOME_NEWS` (własna ścieżka),
2. `$XDG_DATA_HOME/...` (nadpisanie użytkownika),
3. `/usr/local/share/` i `/usr/share/` (oraz katalogi z `XDG_DATA_DIRS`),
   w każdym z nich kolejno:
   `luna/welcome/news.md`, `os-info/release-notes.md`, `os-info/news.md`,
   `pagan-release/release-notes.md`, `pagan-release/news.md`,
4. wbudowanego `data/news.md`.

Format:

```md
## 0.0.8 — 2026-11-05
- Pierwsza zmiana
- Druga zmiana
```

Nagłówek `##` rozpoczyna wydanie, a linie `- ` / `* ` to punkty. Wersję systemu
czytamy z `LUNA_VERSION`, `/etc/luna-release` (`VERSION=`) albo z pierwszego
wydania w notatkach.

## Podgląd bez Waylanda

```sh
cargo run --bin welcome-render -- --live --dark docs/welcome-dark.png
cargo run --bin welcome-render -- --installed docs/welcome.png
```

## Licencja

GNU GPL v3.0 lub nowsza (`GPL-3.0-or-later`) — pełny tekst w [`LICENSE`](LICENSE).
© 2026 Luna Project.
