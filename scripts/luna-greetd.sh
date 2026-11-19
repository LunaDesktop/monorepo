#!/usr/bin/env bash
# Aktywacja / dezaktywacja ekranu logowania Luny (greetd + luna-greeter-session).
#
# Skrypt **nie instaluje** binarek — zakłada, że środowisko jest już zainstalowane
# (`sudo ./scripts/install.sh`). Zmienia wyłącznie konfigurację greetd i, na jawne
# żądanie, usługę systemową. Każda zmiana /etc/greetd/config.toml jest poprzedzona
# kopią zapasową, więc `--disable` przywraca poprzedni stan.
#
# Użycie:
#   ./scripts/luna-greetd.sh --status          # co jest teraz włączone (bez zmian)
#   sudo ./scripts/luna-greetd.sh --dry-run    # pokaż, co zostanie zapisane
#   sudo ./scripts/luna-greetd.sh              # włącz greeter Luny (bez restartu)
#   sudo ./scripts/luna-greetd.sh --restart    # włącz i przełącz od razu
#   sudo ./scripts/luna-greetd.sh --takeover   # dodatkowo wyłącz inne menedżery logowania
#   sudo ./scripts/luna-greetd.sh --disable    # przywróć poprzednią konfigurację
#
# Bezpieczeństwo: skrypt nigdy nie restartuje greetd bez `--restart`, więc nie
# zrywa działającej sesji graficznej (np. XFCE używanego jako ratunek).
set -euo pipefail

prog=$(basename "$0")
config=/etc/greetd/config.toml
backup_prefix="$config.bak-luna-"
greeter_user=greeter

mode=enable
dry=0
restart=0
takeover=0
create_user=0
vt_override=""

usage() {
    cat <<EOF
Użycie: $prog [opcje]

  --status          pokaż stan (konfiguracja, usługa, binarki, sesje)
  --dry-run         pokaż planowane zmiany, nic nie zapisuj
  --restart         po zmianie zrestartuj greetd (zrywa bieżący greeter)
  --takeover        wyłącz inne menedżery logowania (lightdm/sddm/gdm/…)
  --create-user     utwórz konto systemowe '$greeter_user' (jeśli brak)
  --vt N            numer VT dla greetera (domyślnie zachowany z configu)
  --disable         przywróć konfigurację sprzed aktywacji Luny
  -h, --help        ta pomoc

Plik konfiguracji: $config
Kopie zapasowe:    ${backup_prefix}<czas>
EOF
}

log() { printf '%s\n' "$*"; }
say() { printf '  %s\n' "$*"; }
die() { printf 'błąd: %s\n' "$*" >&2; exit 1; }

# Wykonuje polecenie albo tylko je wypisuje w trybie --dry-run.
run() {
    if [ "$dry" = 1 ]; then
        log "  [dry-run] $*"
    else
        "$@"
    fi
}

need_root() {
    if [ "$dry" = 1 ]; then
        return 0
    fi
    [ "$(id -u)" -eq 0 ] || die "uruchom przez sudo (albo dodaj --dry-run)"
}

have() { command -v "$1" >/dev/null 2>&1; }

# Szuka binarki w PATH i w typowych prefiksach (root bywa bez /usr/local/bin).
find_bin() {
    local name=$1 p
    if p=$(command -v "$name" 2>/dev/null); then
        printf '%s' "$p"
        return 0
    fi
    for p in /usr/local/bin/$name /usr/bin/$name /bin/$name; do
        if [ -x "$p" ]; then
            printf '%s' "$p"
            return 0
        fi
    done
    return 1
}

# Numer VT z istniejącej konfiguracji (ostatni wpis `vt = N`).
detect_vt() {
    [ -r "$config" ] || return 1
    local n
    n=$(grep -E '^[[:space:]]*vt[[:space:]]*=' "$config" 2>/dev/null |
        tail -n 1 | tr -cd '0-9')
    [ -n "$n" ] && printf '%s' "$n"
}

# Zachowujemy ewentualny blok [initial_session] (autologowanie użytkownika).
initial_session_block() {
    [ -r "$config" ] || return 0
    awk '
        /^[[:space:]]*\[/ { keep = ($0 ~ /^[[:space:]]*\[initial_session\]/) }
        keep { print }
    ' "$config"
}

greeter_groups_missing() {
    local g missing=""
    for g in video render input; do
        getent group "$g" >/dev/null 2>&1 || continue
        id -nG "$greeter_user" 2>/dev/null | tr ' ' '\n' | grep -qx "$g" || missing="$missing $g"
    done
    printf '%s' "${missing# }"
}

ensure_greeter_user() {
    if ! id "$greeter_user" >/dev/null 2>&1; then
        if [ "$create_user" = 1 ]; then
            log "Tworzę konto systemowe '$greeter_user'…"
            if have useradd; then
                run useradd --system --create-home --home-dir /var/lib/greetd \
                    --shell /usr/sbin/nologin "$greeter_user" || true
            elif have adduser; then
                run adduser --system --home /var/lib/greetd --no-create-home \
                    --group "$greeter_user" || true
            else
                die "brak useradd/adduser — utwórz konto '$greeter_user' ręcznie"
            fi
        else
            die "brak konta '$greeter_user' (dodaj --create-user albo utwórz je ręcznie)"
        fi
    fi

    local missing
    missing=$(greeter_groups_missing)
    if [ -n "$missing" ]; then
        log "Dodaję '$greeter_user' do grup:$missing (potrzebne dla DRM/evdev)"
        local list
        list=$(printf '%s' "$missing" | tr ' ' ',')
        run usermod -aG "$list" "$greeter_user"
    fi
}

check_binaries() {
    local missing=0 name path
    for name in selene luna-greeter luna-greeter-session luna-session; do
        if path=$(find_bin "$name"); then
            say "OK  $name → $path"
        else
            say "BRAK $name"
            missing=1
        fi
    done
    if [ "$missing" = 1 ]; then
        die "najpierw zainstaluj środowisko: sudo ./scripts/install.sh"
    fi
}

list_sessions() {
    local dir found=0 f
    for dir in /usr/local/share/wayland-sessions /usr/share/wayland-sessions \
        /usr/local/share/xsessions /usr/share/xsessions; do
        [ -d "$dir" ] || continue
        for f in "$dir"/*.desktop; do
            [ -e "$f" ] || continue
            say "$f"
            found=1
        done
    done
    [ "$found" = 1 ] || say "(brak plików sesji — greeter użyje wbudowanej Luny)"
}

backup_config() {
    [ -e "$config" ] || return 0
    local stamp dest
    stamp=$(date +%Y%m%d-%H%M%S)
    dest="$backup_prefix$stamp"
    log "Kopia zapasowa: $dest"
    run cp -a "$config" "$dest"
    # Trzymamy maks. 5 kopii, żeby nie zaśmiecać /etc.
    if [ "$dry" = 0 ]; then
        ls -1t "$backup_prefix"* 2>/dev/null | tail -n +6 | while read -r old; do
            rm -f "$old"
        done
    fi
}

write_config() {
    local vt=$1 initial tmp
    initial=$(initial_session_block)
    tmp=$(mktemp)
    {
        cat <<EOF
# Wygenerowane przez $prog (Luna). Pierwotny plik: ${backup_prefix}<czas>.
# Cofnięcie: sudo $0 --disable

[terminal]
vt = $vt

[default_session]
command = "luna-greeter-session"
user = "$greeter_user"
EOF
        if [ -n "$initial" ]; then
            printf '\n%s\n' "$initial"
        fi
    } >"$tmp"

    if [ "$dry" = 1 ]; then
        log "  [dry-run] nowa zawartość $config:"
        sed 's/^/    /' "$tmp"
        rm -f "$tmp"
        return 0
    fi
    install -D -m 644 "$tmp" "$config"
    rm -f "$tmp"
}

other_display_managers() {
    local dm
    for dm in lightdm sddm gdm gdm3 lxdm xdm greetd; do
        have systemctl >/dev/null 2>&1 || return 0
        if systemctl list-unit-files --type=service --no-legend 2>/dev/null |
            awk '{print $1}' | grep -qx "$dm.service"; then
            printf '%s\n' "$dm"
        fi
    done
}

enable_service() {
    if ! have systemctl; then
        say "brak systemd — włącz greetd ręcznie w swoim init"
        return 0
    fi
    run systemctl enable greetd.service

    if [ "$takeover" = 1 ]; then
        local dm
        for dm in $(other_display_managers); do
            [ "$dm" = greetd ] && continue
            log "Wyłączam inny menedżer logowania: $dm"
            run systemctl disable --now "$dm.service" || true
        done
    fi
}

maybe_restart() {
    if [ "$restart" = 1 ]; then
        log "Restartuję greetd…"
        run systemctl restart greetd.service
    else
        log ""
        log "Konfiguracja zapisana. Zadziała po wylogowaniu/restarcie."
        log "Aby przełączyć od razu (zrywa bieżący greeter):"
        say "sudo $0 --restart"
    fi
}

cmd_status() {
    log "== greetd =="
    if have systemctl; then
        systemctl is-enabled greetd.service 2>/dev/null |
            sed 's/^/  włączony przy starcie: /' || true
        systemctl is-active greetd.service 2>/dev/null |
            sed 's/^/  aktywny teraz:       /' || true
    else
        say "brak systemd"
    fi

    log "== konfiguracja ($config) =="
    if [ -r "$config" ]; then
        sed 's/^/  /' "$config"
    else
        say "brak pliku"
    fi

    log "== konto greetera =="
    if id "$greeter_user" >/dev/null 2>&1; then
        say "istnieje; grupy: $(id -nG "$greeter_user" 2>/dev/null)"
        local missing
        missing=$(greeter_groups_missing)
        [ -z "$missing" ] || say "BRAKUJE grup: $missing"
    else
        say "BRAK konta '$greeter_user'"
    fi

    log "== binarki =="
    local name
    for name in selene luna-greeter luna-greeter-session luna-session; do
        if p=$(find_bin "$name"); then
            say "$name → $p"
        else
            say "$name → BRAK"
        fi
    done

    log "== sesje widoczne w greeterze =="
    list_sessions

    log "== kopie zapasowe =="
    ls -1t "$backup_prefix"* 2>/dev/null | sed 's/^/  /' || say "brak"
}

cmd_enable() {
    need_root
    log "== Sprawdzam wymagania =="
    if [ "$dry" = 0 ] && ! have greetd && [ ! -x /usr/bin/greetd ]; then
        die "nie znaleziono greetd — zainstaluj pakiet 'greetd'"
    fi
    check_binaries
    ensure_greeter_user

    local vt=${vt_override:-}
    if [ -z "$vt" ]; then
        vt=$(detect_vt || true)
    fi
    [ -n "$vt" ] || vt=7
    log "VT greetera: $vt"

    # Plik istnieje i już wskazuje na Lunę? Nadal robimy backup, ale informujemy.
    if [ -r "$config" ] && grep -q 'luna-greeter-session' "$config"; then
        log "Konfiguracja już wskazuje na greeter Luny — odświeżam ją."
    fi

    log "== Zapisuję $config =="
    backup_config
    write_config "$vt"

    log "== Usługa =="
    enable_service
    maybe_restart
}

cmd_disable() {
    need_root
    local latest
    latest=$(ls -1t "$backup_prefix"* 2>/dev/null | head -n 1 || true)
    if [ -z "$latest" ]; then
        die "brak kopii zapasowej ($backup_prefix*). Przywróć konfigurację ręcznie:
  - ustaw [default_session].command na swój greeter (np. agreety, tuigreet, regreet),
  - albo wyłącz greetd i włącz poprzedni menedżer logowania (lightdm/sddm/gdm)."
    fi
    log "Przywracam: $latest → $config"
    run cp -a "$latest" "$config"
    maybe_restart
}

main() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --status) mode=status; shift ;;
            --disable) mode=disable; shift ;;
            --dry-run) dry=1; shift ;;
            --restart) restart=1; shift ;;
            --takeover) takeover=1; shift ;;
            --create-user) create_user=1; shift ;;
            --vt) vt_override=${2:-}; shift 2 ;;
            --vt=*) vt_override=${1#*=}; shift ;;
            --user) greeter_user=${2:-}; shift 2 ;;
            --user=*) greeter_user=${1#*=}; shift ;;
            -h | --help) usage; exit 0 ;;
            *) die "nieznana opcja: $1 (--help)" ;;
        esac
    done

    case "$mode" in
        status) cmd_status ;;
        enable) cmd_enable ;;
        disable) cmd_disable ;;
    esac
}

main "$@"
