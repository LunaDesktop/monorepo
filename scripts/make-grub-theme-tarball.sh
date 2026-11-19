#!/bin/sh
# Buduje tarball motywu GRUB dla receptury PaganOS (`luna-grub-theme`).
#
# Użycie:
#   scripts/make-grub-theme-tarball.sh [katalog_wyjściowy]
#
# Domyślnie zapisuje obok receptury w packaging/paganos/github/de/luna-grub-theme.
# Po zmianie motywu skopiuj nowy plik do repo receptur, zaktualizuj `pkgver`
# (albo `pkgrel`) i wpisz nowy sha256sum w PAGBUILD.yaml.
set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
ver="0.1.0"
out="${1:-$root/packaging/paganos/github/de/luna-grub-theme}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

python3 "$root/assets/grub/luna/generate.py"

pkg="$stage/luna-grub-theme-$ver"
mkdir -p "$pkg"
find "$root/assets/grub/luna" -maxdepth 1 -type f \
    ! -name '*.py' ! -name '*.pyc' ! -name 'README.md' -exec cp {} "$pkg/" \;
cp "$root/assets/grub/luna/README.md" "$pkg/"
cp "$root/de/grub/05_luna_theme" "$pkg/"

mkdir -p "$out"
tarball="$out/luna-grub-theme-$ver.tar.gz"
tar -C "$stage" --mtime='2026-01-01 00:00:00' --owner=0 --group=0 \
    --numeric-owner --sort=name -cf - "luna-grub-theme-$ver" | gzip -n -9 >"$tarball"

echo "zapisano: $tarball"
sha256sum "$tarball"
