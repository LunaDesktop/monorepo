# Osobne repozytoria i aktualizowanie pakietów

To repozytorium jest **meta-repozytorium**: każdy pakiet ma własny `Cargo.toml`
z tabelą `[workspace]` i własny `Cargo.lock`. Dzięki temu można go otworzyć,
zbudować, przetestować i wydać niezależnie od reszty środowiska.

| Katalog | Workspace | Zależy od | Rola |
| --- | --- | --- | --- |
| `libluna/` | `libluna` | — | toolkit GUI (fasada + `libluna-core/-render/-platform/-widgets/-i18n`) |
| `luna/` | `luna` | libluna | powłoka: panel, launcher, OSD, keyring/polkit |
| `selene/` | `selene` | libluna | kompozytor Wayland |
| `luna-greeter/` | `luna-greeter` | libluna | ekran logowania dla greetd |
| `apps/luna-demo/` | `luna-demo` | libluna | demo / galeria widgetów |
| `apps/luna-files/` | `luna-files` | libluna | menedżer plików |
| `apps/luna-media/` | `luna-media` | libluna | odtwarzacz wideo i przeglądarka multimediów |
| `apps/luna-editor/` | `luna-editor` | libluna | edytor kodu z wtyczkami instalowanymi z zewnątrz |
| `apps/luna-eq/` | `luna-eq` | — (wtyczka LADSPA) | korektor graficzny dla PipeWire |
| `apps/luna-settings/` | `luna-settings` | libluna, luna | centrum ustawień |
| `apps/luna-clock/` | `luna-clock` | libluna | zegar, minutnik, stoper i kalendarz |
| `apps/luna-store/` | `luna-store` | libluna | sklep i aktualizacje (pacman/pag/apt/dnf/emerge/Flatpak) |
| `apps/luna-terminal/` | `luna-terminal` | libluna | terminal |

`de/` nie jest pakietem Cargo — to wspólna **integracja sesji** (launcher,
wpisy `.desktop`, portale XDG, przykładowy greetd).

## Codzienna praca w meta-repo

```bash
./scripts/workspaces.sh build      # wszystkie pakiety
./scripts/workspaces.sh check      # szybka weryfikacja typów
./scripts/workspaces.sh test       # testy jednostkowe/integracyjne (bez doctestów)
./scripts/workspaces.sh test-doc   # doctesty (wymaga rustdoc w PATH)
./scripts/workspaces.sh lock       # odśwież Cargo.lock po zmianie wersji

./scripts/install.sh                  # całe środowisko
./scripts/install.sh --only terminal  # tylko terminal
```

Odpowiedniki przez `make`: `make build`, `make check`, `make test`,
`make test-doc`, `make lock`, `make install`, `make install-terminal`.

`test` pomija doctesty, bo nie każdy toolchain ma `rustdoc`. Jeśli systemowy `make`
nie startuje z powodu braku `libguile-3.0.so.1`, użyj `./scripts/workspaces.sh`
lub doinstaluj `guile` (`sudo pag install guile`).

`make` ustawia wspólny `CARGO_TARGET_DIR=target/`, więc `scripts/install.sh`
znajduje binarki ze wszystkich pakietów. Bezpośrednie `cargo build` w katalogu
pakietu tworzy jego własny `target/` — to jest w porządku.

## Wydzielenie pakietu do własnego repozytorium

Najpierw **zacommituj wszystko** — `git subtree` i `git archive` widzą tylko
zacommitowane pliki (dotyczy też wygenerowanych `Cargo.lock`).

### Wariant A: `git subtree` (mirror z meta-repo)

```bash
# np. libLuna
git subtree split -P libluna -b split/libluna
git remote add libluna git@github.com:TWOJ_USER/libLuna.git
git push libluna split/libluna:main

# np. terminal
git subtree split -P apps/luna-terminal -b split/luna-terminal
git remote add terminal git@github.com:TWOJ_USER/luna-terminal.git
git push terminal split/luna-terminal:main
```

Po wydzieleniu katalog pakietu jest korzeniem nowego repo i ma już `[workspace]`,
więc `cargo build` działa. **Zależności `path` do innych pakietów trzeba jednak
przepisać na `git`** (albo wersje z rejestru), bo poza meta-repo te katalogi
nie istnieją.

Przykład dla `selene/Cargo.toml` po wydzieleniu:

```toml
[dependencies]
libluna-core = { git = "https://github.com/TWOJ_USER/libLuna.git", tag = "v0.0.1" }
libluna-render = { git = "https://github.com/TWOJ_USER/libLuna.git", tag = "v0.0.1" }
libluna-i18n = { git = "https://github.com/TWOJ_USER/libLuna.git", tag = "v0.0.1" }
libluna-platform = { git = "https://github.com/TWOJ_USER/libLuna.git", tag = "v0.0.1" }
libluna-widgets = { git = "https://github.com/TWOJ_USER/libLuna.git", tag = "v0.0.1" }
```

Dla aplikacji analogicznie; `apps/luna-settings` dodatkowo:

```toml
luna = { git = "https://github.com/TWOJ_USER/luna.git", tag = "v0.0.1" }
```

### Wariant B: `git filter-repo` (jednorazowe wydzielenie z historią)

```bash
git clone <meta-repo> libLuna
cd libLuna
git filter-repo --subdirectory-filter libluna
```

Po obu wariantach usuń z manifestu niepotrzebne już komentarze o monorepo
i ustaw `repository = "https://github.com/TWOJ_USER/<repo>"`.

## Aktualizacja tylko jednego pakietu

Przykład: nowa wersja terminala.

1. Zmień kod wyłącznie w `apps/luna-terminal/`.
2. Podnieś `version` w `apps/luna-terminal/Cargo.toml` (np. `0.0.2`).
3. `cd apps/luna-terminal && cargo test --locked`
4. Zacommituj i oznacz: `git tag luna-terminal-v0.0.2`
5. Wypchnij mirror: `git subtree push --prefix=apps/luna-terminal <remote> main`
   (albo zbuduj pakiet OS z tego katalogu).

Reszta środowiska nie wymaga zmian ani przebudowy. Analogicznie:

```bash
make install-terminal
sudo ./scripts/install.sh --prefix /usr --only terminal
```

## Kolejność wydań

Ponieważ pakiety zależą od siebie, przy zmianie API libLuny zachowaj kolejność:

1. `libluna/` (tag `vX.Y.Z`),
2. `luna/`, `selene/`, `luna-greeter/` (mogą aktualizować tag libLuny),
3. `apps/*` (w tym `luna-settings`, które zależy też od `luna`).

Aktualizacja samej aplikacji (np. terminala) nie wymaga nowej wersji libLuny,
jeśli nie zmieniasz API toolkitu.

## Wersjonowanie i `Cargo.lock`

- Wersje w `Cargo.toml` są jawne (nie dziedziczone z meta-repo), więc każdy
  pakiet można wydać niezależnie.
- Zależności `path` mają też `version = "0.0.1"` — Cargo pilnuje zgodności.
- **Commituj `Cargo.lock`** w każdym workspace'ie (są binarkami/aplikacjami;
  powtarzalne buildy są ważniejsze niż „biblioteczna” konwencja).
- Po podniesieniu `version` w dowolnym pakiecie uruchom `./scripts/workspaces.sh lock`.
  Cargo wymaga też, aby zależności `path` miały **zgodny wymóg wersji** (np. po
  `libluna` 0.0.5 wszystkie `libluna-*` powinny mieć `version = "0.0.5"`).
- Jeśli wersje pakietów zaczną się rozjeżdżać, zaktualizuj `version` w
  zależnościach `path` (np. `libluna-core = { path = "...", version = "0.1.0" }`).

## Pakiety OS (PaganOS)

`packaging/paganos/` nadal buduje z jednego tarballa całego meta-repo, ale każda
receptura wchodzi do właściwego workspace'u (`apps/luna-demo`, `selene/`, ...).
Dla pełnej niezależności pakietów OS można zrobić osobny tarball z wydzielonego
repo (Wariant A/B) i wskazać go w `source:` receptury.
