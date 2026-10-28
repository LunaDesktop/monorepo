#!/bin/sh
# Test dymny całej sesji Luny na **zagnieżdżonym** Selene.
#
# Uruchamia własny kompozytor w izolowanym XDG_RUNTIME_DIR (nie dotyka działającej
# sesji), a następnie sprawdza realny przepływ: panel → okna → klawiatura →
# schowek między klientami → ekran blokady → porządne zakończenie procesów.
#
# Wymagania:
#   - działający nadrzędny kompozytor Wayland (sesja Luny/GNOME/KDE/Sway…),
#   - python3 (sterowanie Selene przez gniazdo `luna-control`),
#   - coreutils (`dd`, `stty` — sprawdzenie wejścia w PTY),
#   - zbudowane binarki: ./scripts/workspaces.sh build
#
# Uruchomienie:
#   ./scripts/session-smoke.sh
#   LUNA_SMOKE_PROFILE=debug ./scripts/session-smoke.sh
#   LUNA_SMOKE_PARENT=/run/user/1000/wayland-0 ./scripts/session-smoke.sh
#
# Kod wyjścia: 0 = wszystko przeszło, 1 = błąd asercji, 2 = brak warunków
# (brak binarek albo nadrzędnego kompozytora).
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${LUNA_SMOKE_PROFILE:-release}"
bin_dir="${LUNA_SMOKE_BIN_DIR:-$root/target/$profile}"

selene_bin="${SELENE_BIN:-$bin_dir/selene}"
shell_bin="${LUNA_SHELL_BIN:-$bin_dir/luna-shell}"
term_bin="${LUNA_TERMINAL_BIN:-$bin_dir/luna-terminal}"
settings_bin="${LUNA_SETTINGS_BIN:-$bin_dir/luna-settings}"
lock_bin="${LUNA_LOCK_BIN:-$bin_dir/luna-lock}"

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
note() { printf '    %s\n' "$*"; }
keep_rt=0
fail() {
    printf '\033[31mBŁĄD:\033[0m %s\n' "$*" >&2
    keep_rt=1
    exit 1
}

for bin in "$selene_bin" "$shell_bin" "$term_bin" "$settings_bin" "$lock_bin"; do
    [ -x "$bin" ] || fail "brak binarki: $bin (zbuduj: ./scripts/workspaces.sh build --release)"
done
command -v python3 >/dev/null 2>&1 || fail "wymagany python3 (sterowanie luna-control)"

# ── Nadrzędny kompozytor ─────────────────────────────────────────────────────
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

# ── Izolowane środowisko ─────────────────────────────────────────────────────
rt="$(mktemp -d "${TMPDIR:-/tmp}/luna-smoke.XXXXXX")"
chmod 700 "$rt"
log="$rt/selene.log"
: >"$rt/input"

pids=""
start() { # start <nazwa> <polecenie...>
    name="$1"
    shift
    "$@" >"$rt/$name.log" 2>&1 &
    echo "$!" >"$rt/$name.pid"
    pids="$pids $!"
}
stop() { # stop <nazwa>
    name="$1"
    [ -f "$rt/$name.pid" ] || return 0
    pid="$(cat "$rt/$name.pid")"
    [ -n "$pid" ] || return 0
    kill "$pid" 2>/dev/null || true
    : >"$rt/$name.pid"
}
cleanup() {
    set +e
    for pid in $pids; do kill "$pid" 2>/dev/null; done
    sleep 0.3
    for pid in $pids; do kill -9 "$pid" 2>/dev/null; done
    if [ "$keep_rt" = 1 ]; then
        printf 'Logi sesji testowej: %s\n' "$rt" >&2
    else
        rm -rf "$rt"
    fi
}
trap cleanup EXIT INT TERM

# Klienci łączą się z zagnieżdżonym Selene; nie dziedziczą trybu DRM/evdev.
# Izolujemy też konfigurację i język, żeby tytuły okien i motyw były przewidywalne
# (inaczej wynik zależy od `~/.config/luna/locale.conf` i `theme.conf`).
# `-u LUNA_STATS` jest ważne: platforma włącza statystyki samym **istnieniem** zmiennej.
client_env="-u LUNA_STATS XDG_RUNTIME_DIR=$rt XDG_CONFIG_HOME=$rt/config WAYLAND_DISPLAY=wayland-0 LUNA_SYSTEM_DARK=0 LANGUAGE=en LANG=C.UTF-8"

# ── Sterowanie Selene ────────────────────────────────────────────────────────
control() {
    python3 - "$rt/luna-control" "$1" <<'PY'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
s.sendall((sys.argv[2] + "\n").encode())
s.close()
PY
}

# Ostatni zrzut listy okien z logu Selene (kolejność = kolejność stosu).
windows_snapshot() {
    control windows >/dev/null 2>&1 || return 0
    sleep 0.25
    awk '
        /selene\/control: okna=/ { buf = "" }
        /selene\/control:/ { buf = buf $0 "\n" }
        END { printf "%s", buf }
    ' "$log"
}

wait_title() { # wait_title <fragment tytułu> [próby]
    title="$1"
    tries="${2:-60}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        # Fragment, nie cały tytuł: Ustawienia mają tytuł zależny od języka
        # („Pagan Linux Settings” / „Ustawienia — Pagan Linux”).
        if windows_snapshot | grep 'mapped=true' | grep -F "$title" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

# Powierzchnie warstwowe nie mają tytułu (`xdg_toplevel`), więc rozpoznajemy je
# po warstwie (`z`): panel = top (1000000), blokada = overlay (2000000).
wait_layer() { # wait_layer <fragment-linii> [próby]
    frag="$1"
    tries="${2:-60}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        if windows_snapshot | grep 'mapped=true' | grep 'layer=true' | grep -F "$frag" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

win_xy() { # win_xy <tytuł> -> "x y" najwyższego zmapowanego okna
    title="$1"
    windows_snapshot | grep 'mapped=true' | grep -F "\"$title\"" | tail -n 1 |
        sed -n 's/.*@ (\([-0-9.]*\),\([-0-9.]*\)).*/\1 \2/p'
}

# Wszystkie terminale mają tytuł „Terminal”, więc czekamy na **wzrost liczby**
# zmapowanych okien, a nie na sam tytuł (inaczej czekanie kończy się na starym oknie).
terminal_count() {
    windows_snapshot | grep 'mapped=true' | grep -F '"Terminal"' | wc -l | tr -d ' '
}
wait_terminals() { # wait_terminals <oczekiwana liczba> [próby]
    want="$1"
    tries="${2:-60}"
    i=0
    while [ "$i" -lt "$tries" ]; do
        have="$(terminal_count)"
        [ "$have" -ge "$want" ] && return 0
        sleep 0.2
        i=$((i + 1))
    done
    return 1
}

start_terminal() { # start_terminal <nazwa> <polecenie...>
    name="$1"
    shift
    want=$(( $(terminal_count) + 1 ))
    start "$name" "$@"
    wait_terminals "$want" || fail "terminal $name się nie zmapował"
    sleep 0.3
}

send_input() { printf '%s\n' "$@" >>"$rt/input"; }
triple_click() { send_input "click $1 $2" "click $1 $2" "click $1 $2"; }
copy_keys() {
    send_input "keydown 29" "keydown 42" "keydown 46" \
        "keyup 46" "keyup 42" "keyup 29"
}
paste_keys() {
    send_input "keydown 29" "keydown 42" "keydown 47" \
        "keyup 47" "keyup 42" "keyup 29"
}
enter_keys() { send_input "keydown 28" "keyup 28"; }
press_key() { send_input "keydown $1" "keyup $1"; }

# ── Start zagnieżdżonego kompozytora ────────────────────────────────────────
say "Selene (nested) na $parent"
env -u SELENE_STATS -u LUNA_STATS XDG_RUNTIME_DIR="$rt" \
    XDG_CONFIG_HOME="$rt/config" \
    LANGUAGE=en LANG=C.UTF-8 \
    WAYLAND_DISPLAY="$parent" \
    SELENE_BACKEND=nested \
    SELENE_INPUT=none \
    SELENE_NO_XWAYLAND=1 \
    SELENE_INPUT_SCRIPT="$rt/input" \
    "$selene_bin" >"$log" 2>&1 &
selene_pid=$!
echo "$selene_pid" >"$rt/selene.pid"
pids="$pids $selene_pid"

i=0
while [ "$i" -lt 50 ]; do
    [ -S "$rt/luna-control" ] && break
    kill -0 "$selene_pid" 2>/dev/null || fail "Selene zakończył się przy starcie (zobacz $log)"
    sleep 0.2
    i=$((i + 1))
done
[ -S "$rt/luna-control" ] || fail "Selene nie utworzył gniazda luna-control"
note "gniazdo: $rt/wayland-0"

# ── Panel powłoki ───────────────────────────────────────────────────────────
say "luna-shell: panel warstwowy"
start shell env $client_env "$shell_bin"
wait_layer "z=1000000" || fail "panel Luna Shell się nie zmapował"
note "panel widoczny"

# ── dmabuf: ogłaszany tylko z GPU ───────────────────────────────────────────
say "dmabuf: zgodny z backendem"
control dmabuf >/dev/null 2>&1 || true
sleep 0.3
if [ -n "${SELENE_GPU:-}" ] || [ -n "${LUNA_GPU:-}" ]; then
    grep -q 'selene/control: dmabuf enabled=true' "$log" ||
        fail "brak globala zwp_linux_dmabuf_v1 przy włączonym GPU"
    note "dmabuf włączony (GPU)"
else
    grep -q 'selene/control: dmabuf enabled=false' "$log" ||
        fail "dmabuf ogłoszony bez backendu GPU"
    note "dmabuf wyłączony (CPU)"
fi

# ── Launcher: PPM przypina/odpina aplikację ─────────────────────────────────
say "launcher: PPM przypina aplikację (stan w launcher.conf)"
launcher_conf="$rt/config/luna/launcher.conf"
# Szerokość wyjścia bierzemy z kompozytora — nie zakładamy 1280 px.
# Szerokość powierzchni panelu bierzemy z jej bufora (`data=WxH`) — w trybie
# zagnieżdżonym okno Selene może zostać rozciągnięte przez nadrzędny kompozytor
# inaczej niż wynika to z modelu `wl_output`.
screen_w="$(windows_snapshot | grep 'layer=true' | grep 'z=1000000' | tail -n 1 |
    sed -n 's/.*data=\([0-9]*\)x[0-9]*$/\1/p')"
if [ -z "$screen_w" ]; then
    control outputs >/dev/null 2>&1 || true
    sleep 0.3
    screen_w="$(grep 'selene/control:   output 0 ' "$log" | tail -n 1 |
        sed -n 's/.*output 0 [^ ]* \([0-9]*\)x[0-9]* @.*/\1/p')"
fi
[ -n "$screen_w" ] || screen_w=1280
lw=$((screen_w - 80))
if [ "$lw" -gt 720 ]; then lw=720; fi
content_x=$(((screen_w - lw) / 2 + 18))
cell=$(((lw - 36 - 36) / 4))
tile_x=$((content_x + cell / 2))
send_input "click 20 20" # burger w lewym górnym rogu paska
sleep 0.6
# Pierwszy kafel siatki launchera (te same stałe co `launcher_rects`).
send_input "rclick $tile_x 206"
sleep 0.8
if ! grep -q '^pinned=' "$launcher_conf" 2>/dev/null; then
    fail "PPM nie przypiął aplikacji (brak wpisu w $launcher_conf)"
fi
note "aplikacja przypięta"
send_input "rclick $tile_x 206" # ta sama pozycja = ta sama (już przypięta) aplikacja
sleep 0.8
if grep -q '^pinned=' "$launcher_conf" 2>/dev/null; then
    fail "drugi PPM nie odpiął aplikacji"
fi
note "aplikacja odpięta"
press_key 1 # Esc zamyka launcher
sleep 0.4

# ── Ustawienia ──────────────────────────────────────────────────────────────
say "luna-settings: okno ustawień"
start settings env $client_env "$settings_bin"
wait_title "Settings" || fail "okno Ustawień się nie zmapowało"
note "okno Ustawień widoczne"

# ── Terminal A: kopiowanie ──────────────────────────────────────────────────
say "terminal A: zaznaczenie i Ctrl+Shift+C"
start_terminal term-a env $client_env "$term_bin" -e sh -c \
    'i=0; while [ $i -lt 40 ]; do echo SMOKE-CLIP; i=$((i+1)); done; sleep 3600'
set -- $(win_xy "Terminal")
wx="${1:-60}"
wy="${2:-20}"
cx=$((wx + 40))
cy=$((wy + 70))
triple_click "$cx" "$cy" # potrójny klik = cała linia (każda zawiera SMOKE-CLIP)
sleep 0.4
copy_keys
sleep 0.6
note "skopiowano zawartość linii"

# ── Terminal B: wklejanie z systemowego schowka ─────────────────────────────
say "terminal B: Ctrl+Shift+V z innego klienta"
paste_out="$rt/pasted.txt"
start_terminal term-b env $client_env "$term_bin" -e sh -c \
    'IFS= read -r line; printf "%s" "$line" > "$0"; sleep 3600' "$paste_out"
set -- $(win_xy "Terminal") # najwyższe okno = B
bx=$(( ${1:-60} + 40 ))
by=$(( ${2:-20} + 70 ))
send_input "click $bx $by"
sleep 0.3
paste_keys
sleep 1.5
enter_keys
sleep 0.5
[ -f "$paste_out" ] || fail "wklejony tekst nie dotarł do terminala B"
grep -q "SMOKE-CLIP" "$paste_out" || fail "schowek nie zawierał SMOKE-CLIP: $(cat "$paste_out")"
note "schowek przeszedł między dwoma klientami"

# ── Terminal C: zwykłe wpisywanie ───────────────────────────────────────────
say "terminal C: klawiatura trafia do okna"
typed="$rt/typed.bin"
start_terminal term-c env $client_env "$term_bin" -e sh -c \
    'stty raw -echo; dd bs=1 count=1 of="$0" 2>/dev/null; stty sane; sleep 3600' "$typed"
set -- $(win_xy "Terminal")
cx=$(( ${1:-60} + 40 ))
cy=$(( ${2:-20} + 70 ))
send_input "click $cx $cy"
sleep 0.8
press_key 31 # 's'
sleep 0.6
[ -s "$typed" ] || fail "klawisz nie dotarł do terminala"
[ "$(cat "$typed")" = "s" ] || fail "terminal dostał inny znak: $(cat "$typed")"
note "klawiatura działa"

# ── Ekran blokady: przejmuje klawiaturę ─────────────────────────────────────
say "luna-lock: overlay + klawiatura wyłączna"
pre="$rt/prelock.bin"
start_terminal term-d env $client_env "$term_bin" -e sh -c \
    'stty raw -echo; dd bs=1 count=1 of="$0" 2>/dev/null; stty sane; sleep 3600' "$pre"
set -- $(win_xy "Terminal")
dx=$(( ${1:-60} + 40 ))
dy=$(( ${2:-20} + 70 ))
send_input "click $dx $dy"
sleep 0.6
press_key 30 # 'a' — kontrola, że fokus naprawdę jest w terminalu
sleep 0.6
[ -s "$pre" ] || fail "terminal D nie miał fokusu przed blokadą"
note "fokus w terminalu potwierdzony"

locked="$rt/locked.bin"
start_terminal term-e env $client_env "$term_bin" -e sh -c \
    'stty raw -echo; dd bs=1 count=1 of="$0" 2>/dev/null; stty sane; sleep 3600' "$locked"
set -- $(win_xy "Terminal")
ex=$(( ${1:-60} + 40 ))
ey=$(( ${2:-20} + 70 ))
send_input "click $ex $ey"
sleep 0.6

start lock env $client_env "$lock_bin"
wait_layer "z=2000000" || fail "ekran blokady się nie zmapował"
press_key 48 # 'b' — powinno trafić do ekranu blokady, nie do terminala
sleep 0.8
# `dd` tworzy plik wyjściowy od razu, więc sprawdzamy **rozmiar**, nie samo istnienie.
[ ! -s "$locked" ] || fail "ekran blokady nie przejął klawiatury (terminal dostał znak)"
note "ekran blokady przejął klawiaturę (odblokowanie hasłem pozostaje ręczne)"

# ── Zakończenie ─────────────────────────────────────────────────────────────
say "zakończenie procesów"
stop lock
stop term-a
stop term-b
stop term-c
stop term-d
stop term-e
stop settings
stop shell
stop selene

for name in lock term-a term-b term-c term-d term-e settings shell selene; do
    [ -f "$rt/$name.pid" ] || continue
    pid="$(cat "$rt/$name.pid")"
    [ -n "$pid" ] || continue
    i=0
    while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 25 ]; do
        sleep 0.2
        i=$((i + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        fail "proces $name ($pid) nie zakończył się po SIGTERM"
    fi
done
note "wszystkie procesy zakończone"

say "PASS — panel, okna, klawiatura, schowek i blokada działają w zagnieżdżonej sesji"
