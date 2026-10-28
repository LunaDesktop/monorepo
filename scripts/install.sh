#!/usr/bin/env bash
# Instalator środowiska Luna (libLuna + Selene + powłoka Luna) na zwykłym PC.
#
# Buduje binarki w trybie release i instaluje je do prefiksu (domyślnie /usr/local):
#   bin/selene, bin/luna-shell, bin/luna-demo, bin/luna-session
#   bin/luna-greeter, bin/luna-greeter-config, bin/luna-greeter-session (ekran logowania dla greetd)
#   bin/luna-settings (centrum ustawień; zapis ekranu logowania przez pkexec)
#   bin/luna-terminal (emulator terminala: PTY + VT100/xterm na libLunie)
#   share/wayland-sessions/selene.desktop   (sesja „Luna” w menedżerze logowania)
#   share/icons/hicolor/scalable/apps/luna.svg   (ikona sesji)
#   share/icons/hicolor/<N>x<N>/apps/luna.png     (rastrowe, dla greeterów bez SVG)
#   share/icons/Luna-Icons/                       (pastelowy pakiet PNG 16–512 px + SVG, freedesktop)
#   share/pixmaps/luna.png
#   share/luna/i18n/*.lang, share/luna/themes/*.theme
#   share/luna/greetd/config.toml.example   (konfiguracja greetd dla Luny)
#   share/licenses/luna/LICENSE
#
# Przykłady:
#   ./scripts/install.sh                      # instalacja do /usr/local (wymaga uprawnień)
#   sudo ./scripts/install.sh --prefix /usr
#   ./scripts/install.sh --destdir /tmp/test --prefix /usr   # instalacja „do paczki”/testowa
#   ./scripts/install.sh --dry-run            # tylko pokaż, co zostanie zrobione
#   sudo ./scripts/install.sh --uninstall     # usuń zainstalowane pliki
#   sudo ./scripts/install.sh --only terminal # aktualizuj TYLKO terminal (nie całe środowisko)
#
# Test bez psucia systemu:  ./scripts/install.sh --destdir /tmp/luna-test --prefix /usr --no-build

set -euo pipefail

prog="$(basename "$0")"
root="$(cd "$(dirname "$0")/.." && pwd)"

prefix="/usr/local"
destdir=""
do_build=1
dry_run=0
uninstall=0
only=""

# Znajduje cargo: $CARGO → PATH → typowe lokalizacje rustup/systemowe.
find_cargo() {
    if [ -n "${CARGO:-}" ]; then
        if command -v "$CARGO" >/dev/null 2>&1 || [ -x "$CARGO" ]; then
            printf '%s' "$CARGO"; return 0
        fi
    fi
    if command -v cargo >/dev/null 2>&1; then
        printf 'cargo'; return 0
    fi
    local c
    for c in "$HOME/.cargo/bin/cargo" /usr/local/bin/cargo /usr/bin/cargo; do
        if [ -x "$c" ]; then printf '%s' "$c"; return 0; fi
    done
    return 1
}

usage() {
    cat <<EOF
$prog — instalator środowiska Luna

Użycie: $prog [opcje]

  --prefix DIR     prefiks instalacji (domyślnie /usr/local)
  --destdir DIR    katalog pośredni (DESTDIR) — instalacja „do paczki”/testowa
  --no-build       nie buduj binarek (użyj istniejących z target/release)
  --dry-run        pokaż planowane operacje, nic nie zmieniaj
  --uninstall      usuń zainstalowane pliki
  --only NAME      tylko jeden komponent: base, selene, luna, greeter,
                   demo, settings, files, terminal, icons
  -h, --help       ta pomoc

Zmienne: CARGO (domyślnie: cargo), PREFIX, DESTDIR
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --prefix) prefix="$2"; shift 2 ;;
        --prefix=*) prefix="${1#*=}"; shift ;;
        --destdir) destdir="$2"; shift 2 ;;
        --destdir=*) destdir="${1#*=}"; shift ;;
        --no-build) do_build=0; shift ;;
        --dry-run) dry_run=1; shift ;;
        --uninstall) uninstall=1; shift ;;
        --only) only="$2"; shift 2 ;;
        --only=*) only="${1#*=}"; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "nieznana opcja: $1" >&2; usage; exit 2 ;;
    esac
done

case "$only" in
    ""|base|selene|luna|greeter|demo|settings|files|terminal|icons) ;;
    *) echo "nieznany komponent: $only (dozwolone: base, selene, luna, greeter, demo, settings, files, terminal, icons)" >&2; exit 2 ;;
esac

dest="${destdir}${prefix}"

# Wszystkie niezależne workspace'y budują do wspólnego target/, dzięki czemu
# instalator i `make build` widzą te same binarki. Można nadpisać CARGO_TARGET_DIR.
: "${CARGO_TARGET_DIR:=$root/target}"
case "$CARGO_TARGET_DIR" in
    /*) ;;
    *) CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR" ;;
esac
export CARGO_TARGET_DIR
lib="$CARGO_TARGET_DIR/release"

# Typowa pułapka: target/ po wcześniejszym `sudo cargo build` należy do roota.
# Dajemy czytelny komunikat zamiast błędu cargo o pliku blokady.
if [ "$dry_run" = 0 ] && [ "$uninstall" = 0 ] && [ "$do_build" = 1 ] && [ -d "$lib" ] && { [ ! -w "$lib" ] || { [ -d "$lib/deps" ] && [ ! -w "$lib/deps" ]; } || { [ -e "$lib/.cargo-lock" ] && [ ! -w "$lib/.cargo-lock" ]; }; }; then
    echo "brak prawa zapisu do $lib (często po buildzie przez sudo)" >&2
    echo "napraw:  sudo chown -R \$(id -u):\$(id -g) '$CARGO_TARGET_DIR'" >&2
    echo "albo:    CARGO_TARGET_DIR=/tmp/luna-target $prog ..." >&2
    exit 1
fi

say() { printf '%s\n' "$*"; }
step() { printf '\n\033[1m%s\033[0m\n' "$*"; }
do_cmd() { if [ "$dry_run" = 1 ]; then say "  [dry-run] $*"; else "$@"; fi; }

# ── Pliki: komponent | źródło | cel | prawa ───────────────────────────────────
files() {
    printf '%s|%s|%s|%s\n' "selene"   "$lib/selene"          "$dest/bin/selene"          "755"
    printf '%s|%s|%s|%s\n' "selene"   "$root/de/session/selene.desktop" "$dest/share/wayland-sessions/selene.desktop" "644"
    printf '%s|%s|%s|%s\n' "luna"     "$lib/luna-shell"      "$dest/bin/luna-shell"      "755"
    printf '%s|%s|%s|%s\n' "luna"     "$lib/luna-keyring"    "$dest/bin/luna-keyring"    "755"
    printf '%s|%s|%s|%s\n' "luna"     "$lib/luna-polkit"     "$dest/bin/luna-polkit"     "755"
    printf '%s|%s|%s|%s\n' "luna"     "$lib/luna-lock"       "$dest/bin/luna-lock"       "755"
    printf '%s|%s|%s|%s\n' "greeter"  "$lib/luna-greeter"    "$dest/bin/luna-greeter"    "755"
    printf '%s|%s|%s|%s\n' "greeter"  "$lib/luna-greeter-config" "$dest/bin/luna-greeter-config" "755"
    printf '%s|%s|%s|%s\n' "greeter"  "$root/de/session/luna-greeter-session" "$dest/bin/luna-greeter-session" "755"
    printf '%s|%s|%s|%s\n' "greeter"  "$root/de/greetd/config.toml.example" "$dest/share/luna/greetd/config.toml.example" "644"
    printf '%s|%s|%s|%s\n' "demo"     "$lib/luna-demo"       "$dest/bin/luna-demo"       "755"
    if [ -f "$lib/luna-settings" ]; then
        printf '%s|%s|%s|%s\n' "settings" "$lib/luna-settings" "$dest/bin/luna-settings" "755"
        printf '%s|%s|%s|%s\n' "settings" "$root/de/session/eu.luna.Settings.desktop" "$dest/share/applications/eu.luna.Settings.desktop" "644"
    fi
    if [ -f "$lib/luna-terminal" ]; then
        printf '%s|%s|%s|%s\n' "terminal" "$lib/luna-terminal" "$dest/bin/luna-terminal" "755"
        printf '%s|%s|%s|%s\n' "terminal" "$root/de/session/eu.luna.Terminal.desktop" "$dest/share/applications/eu.luna.Terminal.desktop" "644"
    fi
    # Menedżer plików Luna Files (+ alias `gaminedes`).
    if [ -f "$lib/luna-files" ]; then
        printf '%s|%s|%s|%s\n' "files" "$lib/luna-files" "$dest/bin/luna-files" "755"
        printf '%s|%s|%s|%s\n' "files" "$root/de/session/eu.luna.Files.desktop" "$dest/share/applications/eu.luna.Files.desktop" "644"
    fi
    if [ -f "$lib/gaminedes" ]; then
        printf '%s|%s|%s|%s\n' "files" "$lib/gaminedes" "$dest/bin/gaminedes" "755"
    fi
    if [ -f "$lib/gallery" ]; then
        printf '%s|%s|%s|%s\n' "demo" "$lib/gallery" "$dest/bin/luna-gallery" "755"
    fi
    # Demo libLuny: prawdziwe okno + widgety (binarka `showcase` z luna-demo).
    if [ -f "$lib/showcase" ]; then
        printf '%s|%s|%s|%s\n' "demo" "$lib/showcase" "$dest/bin/luna-showcase" "755"
        printf '%s|%s|%s|%s\n' "demo" "$root/de/session/luna-showcase.desktop" "$dest/share/applications/luna-showcase.desktop" "644"
    fi
    printf '%s|%s|%s|%s\n' "base"     "$root/de/session/luna-session" "$dest/bin/luna-session" "755"
    # Backend portali XDG dla Luny — bez tego `xdg-desktop-portal` nie wybiera backendu
    # (aplikacje flatpak/żądania portalu kończą się timeoutem).
    printf '%s|%s|%s|%s\n' "base"     "$root/de/session/luna-portals.conf" "$dest/share/xdg-desktop-portal/luna-portals.conf" "644"
    printf '%s|%s|%s|%s\n' "base"     "$root/assets/branding/luna.svg" "$dest/share/icons/hicolor/scalable/apps/luna.svg" "644"
    printf '%s|%s|%s|%s\n' "base"     "$root/LICENSE" "$dest/share/licenses/luna/LICENSE" "644"
    local f
    for f in "$root"/libluna/crates/libluna-i18n/i18n/*.lang; do
        printf '%s|%s|%s|%s\n' "base" "$f" "$dest/share/luna/i18n/$(basename "$f")" "644"
    done
    for f in "$root"/examples/luna/themes/*.theme; do
        printf '%s|%s|%s|%s\n' "base" "$f" "$dest/share/luna/themes/$(basename "$f")" "644"
    done
}

# Rozmiary rastrowej ikony sesji (greeter bez wsparcia SVG użyje którejś z nich).
icon_sizes="16 24 32 48 64 128 256 512"

if [ "$uninstall" = 1 ]; then
    step "Odinstalowywanie Luna z $dest"
    while IFS='|' read -r comp _ dst _; do
        [ -z "$only" ] || [ "$comp" = "$only" ] || continue
        do_cmd rm -f "$dst"
    done < <(files)
    if [ -z "$only" ] || [ "$only" = "base" ]; then
        # Rastrowa ikona sesji (generowana w trakcie instalacji).
        for s in $icon_sizes; do
            do_cmd rm -f "$dest/share/icons/hicolor/${s}x${s}/apps/luna.png"
        done
        do_cmd rm -f "$dest/share/pixmaps/luna.png"
        do_cmd rm -f "$dest/share/dbus-1/services/org.freedesktop.secrets.service"
        # Puste katalogi po naszych plikach.
        for d in "$dest/share/luna/i18n" "$dest/share/luna/themes" "$dest/share/luna/greetd" "$dest/share/luna" \
                 "$dest/share/wayland-sessions" "$dest/share/licenses/luna" \
                 "$dest/share/dbus-1/services" \
                 "$dest/share/icons/hicolor/scalable/apps"; do
            do_cmd rmdir "$d" 2>/dev/null || true
        done
        for s in $icon_sizes; do
            do_cmd rmdir "$dest/share/icons/hicolor/${s}x${s}/apps" 2>/dev/null || true
        done
    fi
    # Pastelowy pakiet ikon (freedesktop) instalowany jako całe drzewo.
    if [ -z "$only" ] || [ "$only" = "base" ] || [ "$only" = "icons" ]; then
        do_cmd rm -rf "$dest/share/icons/Luna-Icons"
    fi
    say "Gotowe."
    exit 0
fi

if [ "$do_build" = 1 ]; then
    step "Budowanie (release, --locked)"
    if ! cargo="$(find_cargo)"; then
        cat >&2 <<'EOF'
Nie znaleziono cargo. Zainstaluj toolchain Rust i spróbuj ponownie:
  • PaganOS:          sudo pag install rustc      # pakiet 'rustc' dostarcza cargo
  • inne dystrybucje: sudo pacman/apt/dnf install rustc   (lub 'rust')
  • rustup:           curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  • własna ścieżka:   CARGO=/ścieżka/do/cargo ./scripts/install.sh ...
EOF
        exit 1
    fi
    say "używam: $cargo"

    # Każdy pakiet to osobny workspace Cargo — budujemy go z jego katalogu.
    build_dir() {
        if [ "$dry_run" = 1 ]; then
            say "  [dry-run] (cd $1 && $cargo build --release --locked)"
        else
            ( cd "$1" && "$cargo" build --release --locked )
        fi
    }

    build_component() {
        case "$1" in
            selene)   build_dir "$root/selene" ;;
            luna)     build_dir "$root/luna" ;;
            greeter)  build_dir "$root/luna-greeter" ;;
            demo)     build_dir "$root/apps/luna-demo" ;;
            settings) build_dir "$root/apps/luna-settings" ;;
            files)    build_dir "$root/apps/luna-files" ;;
            terminal) build_dir "$root/apps/luna-terminal" ;;
            base)     : ;;
            icons)    : ;;
            *) echo "nieznany komponent: $1" >&2; exit 2 ;;
        esac
    }

    if [ -n "$only" ]; then
        build_component "$only"
    else
        for c in selene luna greeter demo settings files terminal; do
            build_component "$c"
        done
    fi
fi

# Brak binarek przy --no-build to błąd.
if [ "$dry_run" = 0 ] && [ "$do_build" = 0 ]; then
    [ -x "$lib/selene" ] || { echo "brak $lib/selene — zbuduj lub usuń --no-build" >&2; exit 1; }
fi

step "Instalacja do $dest"
if [ "$do_build" = 0 ]; then
    say "tryb --no-build: używam istniejących binarek z $lib (bez kompilacji)"
    say "jeśli źródła są nowsze, uruchom instalator bez --no-build albo najpierw:"
    say "  ./scripts/workspaces.sh build --release"
fi

# Pułapka: `--no-build` instaluje to, co leży w target/ — po edycji źródeł i nieudanym
# buildzie (np. przez pliki roota w target/) kopiowana jest **stara** binarka i poprawka
# „nie działa”. Ostrzegamy, gdy źródła są nowsze niż binarka.
stale_bins=""
check_stale() {
    _bin="$1"; shift
    [ -f "$_bin" ] || return 0
    if [ -n "$(find "$@" -name '*.rs' -newer "$_bin" -print -quit 2>/dev/null)" ]; then
        stale_bins="$stale_bins $(basename "$_bin")"
    fi
}
if [ -f "$lib/selene" ]; then
    check_stale "$lib/selene" "$root/selene/src"
    check_stale "$lib/luna-shell" "$root/luna/src"
    check_stale "$lib/luna-keyring" "$root/luna/src"
    check_stale "$lib/luna-lock" "$root/luna/src"
    check_stale "$lib/luna-polkit" "$root/luna/src"
    check_stale "$lib/luna-greeter" "$root/luna-greeter/src"
    for app in demo files settings terminal; do
        check_stale "$lib/luna-$app" "$root/apps/luna-$app/src"
    done
fi
if [ -n "$stale_bins" ]; then
    say ""
    say "UWAGA: te binarki są STARSZE niż źródła:$stale_bins"
    say "  Instalacja wstawi stary kod. Najpierw:  make build"
    say "  (gdy build nie może pisać do target/:  sudo chown -R \$(id -u):\$(id -g) target)"
    say ""
fi
while IFS='|' read -r comp src dst mode; do
    [ -z "$only" ] || [ "$comp" = "$only" ] || continue
    if [ "$dry_run" = 0 ] && [ ! -f "$src" ]; then
        echo "brak pliku źródłowego: $src" >&2
        exit 1
    fi
    do_cmd install -D -m "$mode" "$src" "$dst"
done < <(files)

# Aktywacja D-Bus usługi sekretów. Bez tego pliku D-Bus nie potrafi „wskrzesić”
# `luna-keyring`, gdy ten padnie albo zostanie uruchomiony później niż aplikacja,
# która pyta o hasła — objawia się to jako „keyring nie działa”.
if [ -z "$only" ] || [ "$only" = "luna" ] || [ "$only" = "base" ]; then
    dbus_dir="$dest/share/dbus-1/services"
    do_cmd install -d "$dbus_dir"
    if [ "$dry_run" = 1 ]; then
        say "  [dry-run] $dbus_dir/org.freedesktop.secrets.service"
    else
        printf '%s\n' \
            '[D-BUS Service]' \
            'Name=org.freedesktop.secrets' \
            "Exec=$prefix/bin/luna-keyring" \
            >"$dbus_dir/org.freedesktop.secrets.service"
    fi
fi

# Pastelowy pakiet ikon freedesktop — wspólny dla Luny i aplikacji GTK/Qt.
# Instalujemy go przy **każdym** przebiegu (także `--only terminal/files/settings`),
# bo Pliki i Ustawienia szukają go w `<prefix>/share/icons`.
if [ -d "$root/assets/icons/Luna-Icons" ]; then
    step "Ikony Luna (PNG, freedesktop)"
    do_cmd install -d "$dest/share/icons"
    do_cmd cp -a "$root/assets/icons/Luna-Icons" "$dest/share/icons/"
    # Pastelowa ikona sesji Luny (także dla greeterów korzystających z hicolor).
    do_cmd install -D -m644 "$root/assets/branding/luna.svg" \
        "$dest/share/icons/hicolor/scalable/apps/luna.svg"
    if [ "$destdir" = "" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
        do_cmd gtk-update-icon-cache -q -t -f "$dest/share/icons/Luna-Icons" || true
    fi
fi

# Pomocnik PAM polkit musi być setuid root. W niektórych instalacjach PaganOS bit
# jest zgubiony — wtedy **żaden** agent (także luna-polkit) nie zweryfikuje hasła,
# a okno autoryzacji kończy się błędem. Przywracamy wyłącznie brakujący bit.
if [ "$destdir" = "" ] && [ "$(id -u)" -eq 0 ]; then
    for helper in /usr/lib/polkit-1/polkit-agent-helper-1 \
                  /usr/libexec/polkit-agent-helper-1 \
                  /usr/lib64/polkit-1/polkit-agent-helper-1; do
        if [ -f "$helper" ] && [ ! -u "$helper" ]; then
            say "polkit: przywracam setuid na $helper (wymagane do logowania hasłem)"
            do_cmd chmod 4755 "$helper"
        fi
    done
fi

# Podsumowanie: co **naprawdę** leży teraz w prefiksie i skąd. Bez tego łatwo
# pomyśleć, że „instalacja nic nie zrobiła”, a problemem jest stary build albo
# nierestrygowana sesja (podmiana pliku nie restartuje działającego `selene`).
step "Zainstalowane binarki"
for name in selene luna-shell luna-lock luna-keyring luna-polkit luna-greeter luna-greeter-config luna-demo luna-settings luna-terminal luna-session; do
    s="$lib/$name"
    [ -f "$s" ] || s="$root/de/session/$name"
    d="$dest/bin/$name"
    [ -f "$d" ] || continue
    say "  $name"
    say "    źródło:     $(stat -c %y "$s" 2>/dev/null | cut -d. -f1)  ($s)"
    say "    zainstal.:  $(stat -c %y "$d" 2>/dev/null | cut -d. -f1)  ($d)"
done
say ""
say "Uwaga: działająca sesja ma w pamięci STARE procesy — wyloguj się i zaloguj ponownie,"
say "żeby nowy selene/luna-shell faktycznie wystartował."

# Ikona sesji w rozmiarach rastrowych — dla greeterów bez rsvg.
if [ -z "$only" ] || [ "$only" = "base" ]; then
step "Ikona sesji (rastrowa)"
if command -v rsvg-convert >/dev/null 2>&1; then
    for s in $icon_sizes; do
        dir="$dest/share/icons/hicolor/${s}x${s}/apps"
        do_cmd install -d "$dir"
        do_cmd rsvg-convert -w "$s" -h "$s" "$root/assets/branding/luna.svg" -o "$dir/luna.png"
    done
    do_cmd install -d "$dest/share/pixmaps"
    do_cmd rsvg-convert -w 256 -h 256 "$root/assets/branding/luna.svg" -o "$dest/share/pixmaps/luna.png"
    # Odświeżenie cache ikon tylko w instalacji systemowej (bez DESTDIR).
    if [ "$destdir" = "" ] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
        do_cmd gtk-update-icon-cache -q -t -f "$dest/share/icons/hicolor" || true
    fi
else
    say "  (pomijam PNG — brak rsvg-convert; greeter użyje SVG)"
fi
fi

step "Gotowe"
say "Zainstalowano w: $dest"
say ""
say "Uruchomienie sesji Luna na PC:"
say "  1) wyloguj się z bieżącego pulpitu (Selene musi przejąć master DRM),"
say "  2) wybierz sesję „Luna” w menedżerze logowania, albo z konsoli (TTY):"
say "       SELENE_BACKEND=drm luna-session"
say "  3) testowo, obok działającej sesji (bez przejmowania ekranu):"
say "       SELENE_BACKEND=nested selene &   # okno Selene na obecnym pulpicie"
say "       WAYLAND_DISPLAY=wayland-0 luna-shell"
say ""
say "Uwaga: na DRM/KMS potrzebny jest wolny master i dostęp do /dev/input (grupa 'input')."
