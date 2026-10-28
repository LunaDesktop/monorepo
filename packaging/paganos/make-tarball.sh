#!/usr/bin/env bash
# Generuje lokalny tarball workspace'u libLuna (dla receptur PaganOS) i składa gotowy
# do wysłania katalog `out/de/{libluna,selene}/` z uzupełnionymi sumami sha256.
#
# Szablony w de/*/PAGBUILD.yaml zostają nietknięte (mają tokeny __*_SHA256__).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
ver="$(grep -m1 '^version =' "$root/libluna/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
tarball="luna-${ver}.tar.gz"

out="$here/out/de"
rm -rf "$here/out"
mkdir -p "$out/libluna" "$out/selene"

# `git archive HEAD` widzi tylko zacommitowane pliki — bez Cargo.lock build --locked padnie.
missing=""
for lock in libluna/Cargo.lock luna/Cargo.lock selene/Cargo.lock luna-greeter/Cargo.lock \
            apps/luna-demo/Cargo.lock apps/luna-files/Cargo.lock \
            apps/luna-settings/Cargo.lock apps/luna-terminal/Cargo.lock; do
    git -C "$root" ls-files --error-unmatch "$lock" >/dev/null 2>&1 || missing="$missing $lock"
done
if [ -n "$missing" ]; then
    echo "UWAGA: nieśledzone Cargo.lock (tarball ich nie zawiera):$missing" >&2
    echo "        zacommituj je przed wydaniem — receptury budują z --locked." >&2
fi

# Tarball z bieżącego HEAD (tylko pliki zacommitowane), katalog nadrzędny: luna-${ver}/
git -C "$root" archive --format=tar.gz --prefix="luna-${ver}/" \
    -o "$out/libluna/$tarball" HEAD
cp -f "$out/libluna/$tarball" "$out/selene/$tarball"

sha_tar="$(sha256sum "$out/libluna/$tarball" | cut -d' ' -f1)"
sha_ses="$(sha256sum "$here/de/selene/luna-session" | cut -d' ' -f1)"
sha_desk="$(sha256sum "$here/de/selene/selene.desktop" | cut -d' ' -f1)"

sed "s/__TARBALL_SHA256__/$sha_tar/" \
    "$here/de/libluna/PAGBUILD.yaml" > "$out/libluna/PAGBUILD.yaml"

sed -e "s/__TARBALL_SHA256__/$sha_tar/" \
    -e "s/__SESSION_SHA256__/$sha_ses/" \
    -e "s/__DESKTOP_SHA256__/$sha_desk/" \
    "$here/de/selene/PAGBUILD.yaml" > "$out/selene/PAGBUILD.yaml"

cp -f "$here/de/selene/luna-session" "$here/de/selene/selene.desktop" "$out/selene/"

cat <<EOF
libLuna/Selene ${ver} — gotowe do wysłania w: packaging/paganos/out/

  sha256 tarball : $sha_tar
  sha256 session : $sha_ses
  sha256 desktop : $sha_desk

Skopiuj zawartość do repo receptur:
  packaging/paganos/out/de/libluna/  ->  de/libluna/
  packaging/paganos/out/de/selene/   ->  de/selene/

Test lokalny (gdy masz pagbuild):
  pagbuild packaging/paganos/out/de/libluna/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/selene/PAGBUILD.yaml
EOF
