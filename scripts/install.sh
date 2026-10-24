#!/usr/bin/env bash
# Instalator środowiska Luna (libLuna + Selene + powłoka Luna) na zwykłym PC.
#
# Buduje binarki w trybie release i instaluje je do prefiksu (domyślnie /usr/local):
#   bin/selene, bin/luna-shell, bin/luna-demo, bin/luna-session
#   share/wayland-sessions/selene.desktop   (sesja „Luna” w menedżerze logowania)
#   share/icons/hicolor/scalable/apps/luna.svg   (ikona sesji)
#   share/icons/hicolor/<N>x<N>/apps/luna.png     (rastrowe, dla greeterów bez SVG)
#   share/pixmaps/luna.png
#   share/luna/i18n/*.lang, share/luna/themes/*.theme
#   share/licenses/luna/LICENSE
#
# Przykłady:
#   ./scripts/install.sh                      # instalacja do /usr/local (wymaga uprawnień)
#   sudo ./scripts/install.sh --prefix /usr
#   ./scripts/install.sh --destdir /tmp/test --prefix /usr   # instalacja „do paczki”/testowa
#   ./scripts/install.sh --dry-run            # tylko pokaż, co zostanie zrobione
#   sudo ./scripts/install.sh --uninstall     # usuń zainstalowane pliki
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
        -h|--help) usage; exit 0 ;;
        *) echo "nieznana opcja: $1" >&2; usage; exit 2 ;;
    esac
done

dest="${destdir}${prefix}"
lib="$root/target/release"

say() { printf '%s\n' "$*"; }
step() { printf '\n\033[1m%s\033[0m\n' "$*"; }
do_cmd() { if [ "$dry_run" = 1 ]; then say "  [dry-run] $*"; else "$@"; fi; }

# ── Pliki: źródło | cel | prawa ───────────────────────────────────────────────
files() {
    printf '%s|%s|%s\n' "$lib/selene"          "$dest/bin/selene"          "755"
    printf '%s|%s|%s\n' "$lib/luna-shell"      "$dest/bin/luna-shell"      "755"
    printf '%s|%s|%s\n' "$lib/luna-keyring"    "$dest/bin/luna-keyring"    "755"
    printf '%s|%s|%s\n' "$lib/luna-polkit"     "$dest/bin/luna-polkit"     "755"
    printf '%s|%s|%s\n' "$lib/luna-demo"       "$dest/bin/luna-demo"       "755"
    if [ -f "$lib/gallery" ]; then
        printf '%s|%s|%s\n' "$lib/gallery"     "$dest/bin/luna-gallery"    "755"
    fi
    # Demo libLuny: prawdziwe okno + widgety (binarka `showcase` z luna-demo).
    if [ -f "$lib/showcase" ]; then
        printf '%s|%s|%s\n' "$lib/showcase"    "$dest/bin/luna-showcase"    "755"
        printf '%s|%s|%s\n' "$root/de/session/luna-showcase.desktop" "$dest/share/applications/luna-showcase.desktop" "644"
    fi
    printf '%s|%s|%s\n' "$root/de/session/luna-session"  "$dest/bin/luna-session" "755"
    # Backend portali XDG dla Luny — bez tego `xdg-desktop-portal` nie wybiera backendu
    # (aplikacje flatpak/żądania portalu kończą się timeoutem).
    printf '%s|%s|%s\n' "$root/de/session/luna-portals.conf" "$dest/share/xdg-desktop-portal/luna-portals.conf" "644"
    printf '%s|%s|%s\n' "$root/de/session/selene.desktop" "$dest/share/wayland-sessions/selene.desktop" "644"
    printf '%s|%s|%s\n' "$root/assets/branding/luna.svg" "$dest/share/icons/hicolor/scalable/apps/luna.svg" "644"
    printf '%s|%s|%s\n' "$root/LICENSE"        "$dest/share/licenses/luna/LICENSE" "644"
    local f
    for f in "$root"/crates/libluna-i18n/i18n/*.lang; do
        printf '%s|%s|%s\n' "$f" "$dest/share/luna/i18n/$(basename "$f")" "644"
    done
    for f in "$root"/examples/luna/themes/*.theme; do
        printf '%s|%s|%s\n' "$f" "$dest/share/luna/themes/$(basename "$f")" "644"
    done
}

# Rozmiary rastrowej ikony sesji (greeter bez wsparcia SVG użyje którejś z nich).
icon_sizes="16 24 32 48 64 128 256"

if [ "$uninstall" = 1 ]; then
    step "Odinstalowywanie Luna z $dest"
    while IFS='|' read -r _ dst _; do
        do_cmd rm -f "$dst"
    done < <(files)
    # Rastrowa ikona sesji (generowana w trakcie instalacji).
    for s in $icon_sizes; do
        do_cmd rm -f "$dest/share/icons/hicolor/${s}x${s}/apps/luna.png"
    done
    do_cmd rm -f "$dest/share/pixmaps/luna.png"
    # Puste katalogi po naszych plikach.
    for d in "$dest/share/luna/i18n" "$dest/share/luna/themes" "$dest/share/luna" \
             "$dest/share/wayland-sessions" "$dest/share/licenses/luna" \
             "$dest/share/icons/hicolor/scalable/apps"; do
        do_cmd rmdir "$d" 2>/dev/null || true
    done
    for s in $icon_sizes; do
        do_cmd rmdir "$dest/share/icons/hicolor/${s}x${s}/apps" 2>/dev/null || true
    done
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
    ( cd "$root" && do_cmd "$cargo" build --release --locked -p selene -p luna -p luna-demo --bins )
fi

# Brak binarek przy --no-build to błąd.
if [ "$dry_run" = 0 ] && [ "$do_build" = 0 ]; then
    [ -x "$lib/selene" ] || { echo "brak $lib/selene — zbuduj lub usuń --no-build" >&2; exit 1; }
fi

step "Instalacja do $dest"
while IFS='|' read -r src dst mode; do
    if [ "$dry_run" = 0 ] && [ ! -f "$src" ]; then
        echo "brak pliku źródłowego: $src" >&2
        exit 1
    fi
    do_cmd install -D -m "$mode" "$src" "$dst"
done < <(files)

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
for name in selene luna-shell luna-keyring luna-polkit luna-demo luna-session; do
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
