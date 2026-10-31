#!/usr/bin/env bash
# Generuje lokalny tarball workspace'u libLuna (dla receptur PaganOS) i składa gotowy
# do wysłania katalog `out/de/{libluna,selene,luna-settings,luna-desktop}/`
# z uzupełnionymi sumami sha256.
#
# Szablony w de/*/PAGBUILD.yaml zostają nietknięte (mają tokeny __*_SHA256__).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(git -C "$here" rev-parse --show-toplevel)"
ver="$(grep -m1 '^version =' "$root/libluna/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
tarball="luna-${ver}.tar.gz"

out="$here/out/de"
rm -rf "$here/out"
mkdir -p "$out/libluna" "$out/selene" "$out/luna-settings" "$out/luna-shell" \
    "$out/luna-greeter" "$out/luna-desktop"

# `git archive HEAD` widzi tylko zacommitowane pliki — bez Cargo.lock build --locked padnie.
missing=""
for lock in libluna/Cargo.lock luna/Cargo.lock selene/Cargo.lock luna-greeter/Cargo.lock \
            apps/luna-demo/Cargo.lock apps/luna-files/Cargo.lock apps/luna-media/Cargo.lock \
            apps/luna-settings/Cargo.lock apps/luna-terminal/Cargo.lock \
            apps/luna-eq/Cargo.lock; do
    git -C "$root" ls-files --error-unmatch "$lock" >/dev/null 2>&1 || missing="$missing $lock"
done
if [ -n "$missing" ]; then
    echo "UWAGA: nieśledzone Cargo.lock (tarball ich nie zawiera):$missing" >&2
    echo "        zacommituj je przed wydaniem — receptury budują z --locked." >&2
fi

# Tarball z bieżącego HEAD (tylko pliki zacommitowane), katalog nadrzędny: luna-${ver}/
git -C "$root" archive --format=tar.gz --prefix="luna-${ver}/" \
    -o "$out/libluna/$tarball" HEAD
for dir in selene luna-settings luna-shell luna-greeter; do
    cp -f "$out/libluna/$tarball" "$out/$dir/$tarball"
done

sha_tar="$(sha256sum "$out/libluna/$tarball" | cut -d' ' -f1)"
sha_ses="$(sha256sum "$here/de/selene/luna-session" | cut -d' ' -f1)"
sha_desk="$(sha256sum "$here/de/selene/selene.desktop" | cut -d' ' -f1)"
sha_settings_desk="$(sha256sum "$root/de/session/eu.luna.Settings.desktop" | cut -d' ' -f1)"
sha_portals="$(sha256sum "$here/de/luna-shell/luna-portals.conf" | cut -d' ' -f1)"
sha_shell_ses="$(sha256sum "$here/de/luna-shell/luna-session" | cut -d' ' -f1)"

sed "s/__TARBALL_SHA256__/$sha_tar/" \
    "$here/de/libluna/PAGBUILD.yaml" > "$out/libluna/PAGBUILD.yaml"

sed -e "s/__TARBALL_SHA256__/$sha_tar/" \
    -e "s/__SESSION_SHA256__/$sha_ses/" \
    -e "s/__DESKTOP_SHA256__/$sha_desk/" \
    "$here/de/selene/PAGBUILD.yaml" > "$out/selene/PAGBUILD.yaml"
cp -f "$here/de/selene/luna-session" "$here/de/selene/selene.desktop" "$out/selene/"

sed -e "s/__TARBALL_SHA256__/$sha_tar/" \
    -e "s/__DESKTOP_SHA256__/$sha_settings_desk/" \
    "$here/de/luna-settings/PAGBUILD.yaml" > "$out/luna-settings/PAGBUILD.yaml"
cp -f "$root/de/session/eu.luna.Settings.desktop" "$out/luna-settings/"

sed -e "s/__TARBALL_SHA256__/$sha_tar/" \
    -e "s/__SESSION_SHA256__/$sha_shell_ses/" \
    -e "s/__PORTALS_SHA256__/$sha_portals/" \
    "$here/de/luna-shell/PAGBUILD.yaml" > "$out/luna-shell/PAGBUILD.yaml"
cp -f "$here/de/luna-shell/luna-session" "$here/de/luna-shell/luna-portals.conf" \
    "$out/luna-shell/"

sed -e "s/__TARBALL_SHA256__/$sha_tar/" \
    "$here/de/luna-greeter/PAGBUILD.yaml" > "$out/luna-greeter/PAGBUILD.yaml"

# Metapakiet nie ma źródeł ani sum — kopiujemy bez podmian.
cp -f "$here/de/luna-desktop/PAGBUILD.yaml" "$out/luna-desktop/PAGBUILD.yaml"

cat <<MSG
libLuna/Selene/Luna Settings ${ver} — gotowe do wysłania w: packaging/paganos/out/

  sha256 tarball        : $sha_tar
  sha256 session        : $sha_ses
  sha256 selene.desktop : $sha_desk
  sha256 settings.desktop: $sha_settings_desk
  sha256 portals        : $sha_portals

Skopiuj zawartość do repo receptur:
  packaging/paganos/out/de/libluna/       ->  de/libluna/
  packaging/paganos/out/de/selene/        ->  de/selene/
  packaging/paganos/out/de/luna-settings/ ->  de/luna-settings/
  packaging/paganos/out/de/luna-shell/    ->  de/luna-shell/
  packaging/paganos/out/de/luna-greeter/  ->  de/luna-greeter/
  packaging/paganos/out/de/luna-desktop/  ->  de/luna-desktop/

Test lokalny (gdy masz pagbuild):
  pagbuild packaging/paganos/out/de/libluna/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/selene/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/luna-settings/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/luna-shell/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/luna-greeter/PAGBUILD.yaml
  pagbuild packaging/paganos/out/de/luna-desktop/PAGBUILD.yaml
MSG
