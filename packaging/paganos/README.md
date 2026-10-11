# Receptury PaganOS dla libLuna i Selene

**English** — `PAGBUILD.yaml` recipes for the **PaganOS** build system (`pagbuild`/`pag`),
packaging **libLuna** (the GUI toolkit) and **Selene** (the Wayland compositor). They follow
the conventions of the central `recipes` repo (`https://git.paganlinux.eu/recipes.git`):
a local source tarball, `cargo build --release --locked`, files installed into `${PKGDIR}`.

**Polski** — receptury `PAGBUILD.yaml` dla systemu budowania **PaganOS** (`pagbuild`/`pag`),
pakietujące **libLunę** (toolkit GUI) oraz **Selene** (kompozytor Wayland). Trzymają się
konwencji centralnego repo `recipes`: lokalny tarball źródła, `cargo build --release --locked`,
pliki instalowane do `${PKGDIR}`.

## Zawartość

| Plik | Rola |
| --- | --- |
| `de/libluna/PAGBUILD.yaml` | pakiet `libluna` — demo (`luna-demo`) + galeria (`luna-gallery`), tłumaczenia, przykładowe motywy |
| `de/selene/PAGBUILD.yaml` | pakiet `selene` — binarka kompozytora, skrypt sesji i wpis sesji Wayland |
| `de/selene/luna-session` | launcher sesji (ustawia `XDG_*` i `SELENE_BACKEND`) |
| `de/selene/selene.desktop` | wpis do `/usr/share/wayland-sessions` |
| `make-tarball.sh` | generuje tarball i składa gotowy katalog `out/` z uzupełnionymi sha256 |

Oba pakiety budują się z **tego samego** tarballa (wspólny workspace). Katalog w tarballu to
`luna-${pkgver}/`, stąd `cd luna-${pkgver}` w fazach `build:`.

## Jak zrobić wydanie receptur

```sh
# 1) Zapisz stan, który chcesz spakować (receptury biorą `git archive HEAD`).
git add -A && git commit -m "release: 0.0.1"

# 2) Wygeneruj tarball + uzupełnij sha256 (powstaje packaging/paganos/out/).
packaging/paganos/make-tarball.sh

# 3) Skopiuj do centralnego repo receptur i zbuduj.
cp -r packaging/paganos/out/de/libluna  <recipes>/de/libluna
cp -r packaging/paganos/out/de/selene   <recipes>/de/selene
pagbuild <recipes>/de/libluna/PAGBUILD.yaml
pagbuild <recipes>/de/selene/PAGBUILD.yaml
```

Szablony w `de/*/PAGBUILD.yaml` mają tokeny (`__TARBALL_SHA256__` itd.) i **nie** zawierają
sum — te wpisuje dopiero skrypt, bo zależą od konkretnego commita/taga.

## Zależności

- **runtime:** `libxkbcommon` (linkowane dynamicznie — potwierdzone `ldd`) oraz `dejavu-ttf`
  (toolkit ładuje `DejaVuSans.ttf` z `/usr/share/fonts`; jeśli jest `adwaita-fonts`, użyje go
  jako pierwszego).
- **build:** `rustc` (dostarcza `cargo`). Crates pobiera `cargo`; brak `-sys` wymagających
  `pkg-config` (wayland-client buduje się z czysto-rdzeniowym backendem — brak `libwayland.so`).

## Uwagi

- `Selene` to kompozytor Wayland; wpis sesji startuje na DRM/KMS (`SELENE_BACKEND=drm`).
  Gdy master DRM trzyma inna sesja, Selene **bezpiecznie** spada do wyjścia nested.
- `XDG_RUNTIME_DIR` pochodzi od menedżera sesji; gdy go brak (np. `pam_systemd` nie założył
  sesji), `luna-session` i sam Selene wybierają katalog awaryjnie (`/run/user/<uid>`, dalej
  prywatny `0700` pod `$TMPDIR`).
- Wejście w trybie DRM (evdev) to kolejny krok — póki go nie ma, sesja jest „podglądowa”.
- `selene` nie deklaruje zależności od pakietu `libluna`, bo linkuje toolkit statycznie
  (Rust) — zależność nie jest potrzebna w runtime.
