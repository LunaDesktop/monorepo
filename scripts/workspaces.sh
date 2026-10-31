#!/bin/sh
# Wspólne polecenia dla niezależnych workspace'ów Luna.
#
#   ./scripts/workspaces.sh build          # zbuduj wszystkie pakiety
#   ./scripts/workspaces.sh check          # szybka weryfikacja typów
#   ./scripts/workspaces.sh test           # testy jednostkowe/integracyjne (bez doctestów)
#   ./scripts/workspaces.sh test-doc       # doctesty (wymaga rustdoc w PATH)
#   ./scripts/workspaces.sh smoke          # test dymny sesji (zagnieżdżony Selene)
#   ./scripts/workspaces.sh lock           # odśwież Cargo.lock po zmianie wersji
#   ./scripts/workspaces.sh fmt            # cargo fmt w każdym pakiecie
#
# Dodatkowe argumenty trafiają do cargo, np.:
#   ./scripts/workspaces.sh build --release
#   ./scripts/workspaces.sh test -- --nocapture
#
# Domyślnie wszystkie pakiety budują do wspólnego target/ (nadpisz przez
# CARGO_TARGET_DIR), żeby scripts/install.sh widział ich binarki.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
cmd="${1:-build}"
[ "$#" -gt 0 ] && shift

case "$cmd" in
    build|check|test|test-doc|fmt|lock|smoke) ;;
    *)
        echo "usage: $0 {build|check|test|test-doc|smoke|lock|fmt} [cargo args...]" >&2
        exit 2
        ;;
esac

if [ "$cmd" = "smoke" ]; then
    exec "$root/scripts/session-smoke.sh" "$@"
fi

if [ "$cmd" = "test-doc" ] && ! command -v rustdoc >/dev/null 2>&1; then
    echo "brak rustdoc w PATH — doctesty wymagają dopasowanego rustdoc." >&2
    echo 'Wskazówka: użyj toolchainu rustup, np.' >&2
    echo '  PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/tmp/luna-doc-target ./scripts/workspaces.sh test-doc' >&2
    exit 1
fi

: "${CARGO_TARGET_DIR:=$root/target}"
case "$CARGO_TARGET_DIR" in
    /*) ;;
    *) CARGO_TARGET_DIR="$root/$CARGO_TARGET_DIR" ;;
esac
export CARGO_TARGET_DIR

workspaces="libluna luna selene luna-greeter apps/luna-demo apps/luna-files apps/luna-media apps/luna-clock apps/luna-settings apps/luna-store apps/luna-editor apps/luna-terminal apps/luna-eq"

for w in $workspaces; do
    if [ "$cmd" = "test-doc" ] && ! cargo metadata --no-deps --format-version 1 \
        --manifest-path "$root/$w/Cargo.toml" 2>/dev/null | grep -q '"kind":\["lib"\]'; then
        printf '\n\033[1m==> %s\033[0m (pomijam — brak targetu lib)\n' "$w"
        continue
    fi
    printf '\n\033[1m==> %s\033[0m\n' "$w"
    if [ "$cmd" = "lock" ]; then
        cargo generate-lockfile --offline --manifest-path "$root/$w/Cargo.toml" "$@"
    elif [ "$cmd" = "fmt" ]; then
        cargo fmt --manifest-path "$root/$w/Cargo.toml" "$@"
    elif [ "$cmd" = "test" ]; then
        # Bez `--doc`: doctesty wymagają rustdoc, którego może nie być w toolchainie.
        cargo test --manifest-path "$root/$w/Cargo.toml" --locked --tests "$@"
    elif [ "$cmd" = "test-doc" ]; then
        cargo test --manifest-path "$root/$w/Cargo.toml" --locked --doc "$@"
    else
        cargo "$cmd" --manifest-path "$root/$w/Cargo.toml" --locked "$@"
    fi
done
