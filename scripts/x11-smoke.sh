#!/bin/sh
# Test dymny **XWayland + XWM** — prawdziwa aplikacja X11 w zagnieżdżonym Selene.
#
# Sprawdza to, czego nie da się sprawdzić unit testami:
#   1. Xwayland startuje i łączy się z kompozytorem (`luna-display`),
#   2. XWM zostaje menedżerem okien (`wm=true` z `luna-control x11`),
#   3. okno X11 (`xclock`) pojawia się w modelu jako zwykłe okno (nie warstwa),
#   4. klik w nasz przycisk **maksymalizuj** działa na oknie X11,
#   5. kursor ustawiony przez klienta X11 dociera przez `wl_pointer.set_cursor`,
#   6. klik w **zamknij** zamyka okno (SSD + routing wskaźnika).
#
# Wymagania: działający nadrzędny kompozytor Wayland, `python3`, `xclock`,
# opcjonalnie `python3-gi` + GTK4 (test kursora X11), i zbudowane binarki
# (`./scripts/workspaces.sh build --release`).
#
# Uruchomienie:
#   ./scripts/x11-smoke.sh
#   LUNA_SMOKE_PROFILE=debug ./scripts/x11-smoke.sh
#
# Kod wyjścia: 0 = OK, 1 = błąd asercji, 2 = brak warunków (binarki/kompozytor/xclock).
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${LUNA_SMOKE_PROFILE:-release}"
bin_dir="${LUNA_SMOKE_BIN_DIR:-$root/target/$profile}"
selene_bin="${SELENE_BIN:-$bin_dir/selene}"

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
keep_rt=0
fail() {
    printf '\033[31mBŁĄD:\033[0m %s\n' "$*" >&2
    keep_rt=1
    exit 1
}

[ -x "$selene_bin" ] || fail "brak binarki: $selene_bin (zbuduj: ./scripts/workspaces.sh build --release)"
command -v python3 >/dev/null 2>&1 || fail "wymagany python3"
command -v xclock >/dev/null 2>&1 || fail "wymagany xclock (pakiet x11-apps)"

parent="${LUNA_SMOKE_PARENT:-}"
if [ -z "$parent" ] && [ -n "${WAYLAND_DISPLAY:-}" ]; then
    case "$WAYLAND_DISPLAY" in
        /*) parent="$WAYLAND_DISPLAY" ;;
        *) parent="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/$WAYLAND_DISPLAY" ;;
    esac
fi
if [ -z "$parent" ]; then
    for sock in /run/user/"$(id -u)"/wayland-*; do
        [ -S "$sock" ] || continue
        case "$sock" in *.lock) continue ;; esac
        parent="$sock"
        break
    done
fi
[ -n "$parent" ] && [ -S "$parent" ] || fail \
    "brak nadrzędnego kompozytora Wayland (ustaw LUNA_SMOKE_PARENT=/ścieżka/do/wayland-0)"

rt="$(mktemp -d "${TMPDIR:-/tmp}/luna-x11-smoke.XXXXXX")"
chmod 700 "$rt"
log="$rt/selene.log"
: >"$rt/input"
pids=""

cleanup() {
    set +e
    for pid in $pids; do kill "$pid" 2>/dev/null; done
    sleep 0.3
    for pid in $pids; do kill -9 "$pid" 2>/dev/null; done
    if [ "$keep_rt" = 1 ]; then
        printf 'Logi testu X11: %s\n' "$rt" >&2
    else
        rm -rf "$rt"
    fi
}
trap cleanup EXIT INT TERM

control() {
    python3 - "$rt/luna-control" "$1" <<'PY'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
s.sendall((sys.argv[2] + "\n").encode())
s.close()
PY
}

windows_snapshot() {
    control windows >/dev/null 2>&1 || return 0
    sleep 0.25
    awk '
        /selene\/control: okna=/ { buf = "" }
        /selene\/control:/ { buf = buf $0 "\n" }
        END { printf "%s", buf }
    ' "$log"
}

wait_window() { # wait_window <tytuł> [próby]
    title="$1"
    tries="${2:-100}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        if windows_snapshot | grep 'mapped=true' | grep -F "\"$title\"" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

wait_window_gone() { # wait_window_gone <tytuł> [próby]
    title="$1"
    tries="${2:-100}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        if ! windows_snapshot | grep 'mapped=true' | grep -F "\"$title\"" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

# `x y w h` ostatniego zmapowanego okna o tytule.
win_geom() { # win_geom <tytuł>
    title="$1"
    windows_snapshot | grep 'mapped=true' | grep -F "\"$title\"" | tail -n 1 |
        sed -n 's/.*"'"$title"'" \([0-9]*\)x\([0-9]*\) @ (\([-0-9.]*\),\([-0-9.]*\)).*/\3 \4 \1 \2/p'
}

# Czeka, aż szerokość okna będzie równa `want` (maksymalizacja jest asynchroniczna:
# X11 dostaje ConfigureWindow, klient przerysowuje, XWM aktualizuje model).
wait_geom_width() { # wait_geom_width <tytuł> <szerokość> [próby]
    title="$1"
    want="$2"
    tries="${3:-50}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        set -- $(win_geom "$title")
        [ "${3:-0}" -eq "$want" ] && return 0
        sleep 0.1
        i=$((i + 1))
    done
    return 1
}

send_input() { printf '%s\n' "$@" >>"$rt/input"; }

# ── Start zagnieżdżonego kompozytora (XWayland WŁĄCZONY) ────────────────────
say "Selene (nested) + Xwayland na $parent"
env -u LUNA_STATS -u SELENE_NO_XWAYLAND XDG_RUNTIME_DIR="$rt" \
    XDG_CONFIG_HOME="$rt/config" \
    WAYLAND_DISPLAY="$parent" \
    SELENE_BACKEND=nested \
    SELENE_INPUT=none \
    SELENE_INPUT_SCRIPT="$rt/input" \
    "$selene_bin" >"$log" 2>&1 &
selene_pid=$!
pids="$pids $selene_pid"

i=0
while [ "$i" -lt 100 ]; do
    [ -S "$rt/luna-control" ] && break
    kill -0 "$selene_pid" 2>/dev/null || fail "Selene zakończył się przy starcie (zobacz $log)"
    sleep 0.2
    i=$((i + 1))
done
[ -S "$rt/luna-control" ] || fail "Selene nie utworzył gniazda luna-control"

# `luna-display` powstaje dopiero po **udanym** połączeniu XWM z X11.
i=0
while [ "$i" -lt 100 ]; do
    [ -f "$rt/luna-display" ] && break
    sleep 0.2
    i=$((i + 1))
done
[ -f "$rt/luna-display" ] || fail "Xwayland nie wystartował (brak $rt/luna-display)"
display="$(cat "$rt/luna-display")"
case "$display" in
    ''|*[!0-9]*) fail "zły numer ekranu w luna-display: $display" ;;
esac
note "X11 podłączony jako DISPLAY=:$display"

# Szerokość monitora — nie zakładamy 1280 px (nested może dostać inny rozmiar).
control outputs >/dev/null 2>&1 || true
sleep 0.3
out_w="$(grep 'selene/control:   output 0 ' "$log" | tail -n 1 |
    sed -n 's/.*output 0 [^ ]* \([0-9]*\)x[0-9]* @.*/\1/p')"
[ -n "$out_w" ] || out_w=1280

say "XWM: właściciel menedżera okien"
control x11 >/dev/null 2>&1 || true
sleep 0.3
if ! grep -q 'selene/control: x11 .*wm=true' "$log"; then
    fail "XWM nie został menedżerem okien (oczekiwano wm=true)"
fi
note "wm=true"

# ── Aplikacja X11 ───────────────────────────────────────────────────────────
say "xclock: okno X11 w modelu kompozytora"
DISPLAY=":$display" xclock -geometry 320x240+120+120 >"$rt/xclock.log" 2>&1 &
xclock_pid=$!
pids="$pids $xclock_pid"
wait_window "xclock" || fail "okno xclock się nie zmapowało (zobacz $rt/xclock.log)"
line="$(windows_snapshot | grep 'mapped=true' | grep -F '"xclock"' | tail -n 1)"
case "$line" in
    *layer=true*) fail "okno X11 potraktowane jako warstwa: $line" ;;
esac
note "okno widoczne: $(printf '%s' "$line" | sed -n 's/.*"xclock" //p')"
control x11 >/dev/null 2>&1 || true
sleep 0.3
grep 'selene/control:   x11 ' "$log" | tail -n 3 || true

set -- $(win_geom "xclock")
x="${1:-120}"
y="${2:-120}"
w="${3:-320}"
h="${4:-240}"
note "geometria: ${w}x${h} @ (${x},${y})"

# ── Maksymalizacja naszym SSD ───────────────────────────────────────────────
say "SSD: maksymalizacja okna X11"
set -- $(win_geom "xclock")
x="${1:-120}"
y="${2:-120}"
w="${3:-320}"
mx=$((x + w - 53))
my=$((y + 19))
send_input "click $mx $my"
wait_geom_width "xclock" "$out_w" 50 || {
    set -- $(win_geom "xclock")
    fail "maksymalizacja nie zadziałała (szerokość=${3:-?}, oczekiwano $out_w)"
}
note "zmaksymalizowane: szerokość=$out_w"

# ── Kursor od klienta X11 ───────────────────────────────────────────────────
# `xclock` używa domyślnego kursora X11, więc Xwayland nie ma czego ustawiać.
# Uruchamiamy GTK4 z backendem X11: pole tekstowe wymusza kursor I-beam, który
# Xwayland przekazuje przez `wl_pointer.set_cursor` (albo `wp_cursor_shape_v1`).
say "kursor: ustawiony przez klienta X11"
if python3 -c 'import gi; gi.require_version("Gtk", "4.0")' >/dev/null 2>&1; then
    env -u WAYLAND_DISPLAY -u GDK_BACKEND DISPLAY=":$display" GDK_BACKEND=x11 \
        LANGUAGE=en LANG=C.UTF-8 python3 - >"$rt/gtk.log" 2>&1 <<'PY' &
import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk

win = Gtk.Window()
win.set_title("luna-cursor")
entry = Gtk.Entry()
entry.set_placeholder_text("pole tekstowe")
win.set_child(entry)
win.set_default_size(420, 200)
win.present()
GLib.MainLoop().run()
PY
    gtk_pid=$!
    pids="$pids $gtk_pid"
    if wait_window "luna-cursor" 100; then
        # Ruch w **treść** okna (nie w pasek SSD) — dopiero wtedy Xwayland wysyła kursor.
        set -- $(win_geom "luna-cursor")
        send_input "move $(( ${1:-60} + ${3:-420} / 2 )) $(( ${2:-20} + ${4:-200} / 2 ))"
        sleep 0.5
        control cursor >/dev/null 2>&1 || true
        sleep 0.3
        if grep -q "selene/control: cursor image=custom:" "$log"; then
            note "obraz od klienta (wl_pointer.set_cursor)"
        elif grep -q "selene/control: cursor image=shape:" "$log"; then
            note "kształt przez wp_cursor_shape_v1"
        else
            fail "kursor nad oknem X11 nie został ustawiony przez klienta"
        fi
    else
        fail "okno GTK4 przez Xwayland się nie zmapowało (zobacz $rt/gtk.log)"
    fi
    kill "$gtk_pid" 2>/dev/null || true
else
    note "pominięto: brak python3-gi/GTK4 (wymagane do testu kursora X11)"
fi

# ── Zamknięcie naszym SSD ───────────────────────────────────────────────────
say "SSD: zamknięcie okna X11"
i=0
while [ "$i" -lt 3 ]; do
    set -- $(win_geom "xclock")
    [ "$#" -ge 4 ] || break
    x="$1"
    y="$2"
    w="$3"
    cx=$((x + w - 21))
    cy=$((y + 19))
    send_input "click $cx $cy"
    if wait_window_gone "xclock" 30; then
        break
    fi
    i=$((i + 1))
done
wait_window_gone "xclock" || fail "okno X11 nie zamknęło się po kliknięciu „×”"
note "okno zamknięte"

say "PASS — XWayland, XWM, mapowanie, maksymalizacja i zamykanie okien X11 działają"
