#!/bin/sh
# Instaluje reguły udev potrzebne grom i Flatpakom (Steam Input, kontrolery).
#
#   sudo ./scripts/install-input-rules.sh
#
# Naprawia /dev/uinput (domyślnie 0600 root:root) oraz dostęp do kontrolerów
# Valve. Bez tego Steam z Flatpaka pokazuje „Missing permissions for input
# devices”, a pady nie działają w grach.

set -eu

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/de/udev/60-luna-input.rules"
dst="/etc/udev/rules.d/60-luna-input.rules"

if [ "$(id -u)" -ne 0 ]; then
    echo "Tej instalacji wymaga root. Uruchom:" >&2
    echo "  sudo $0" >&2
    exit 1
fi

if [ ! -f "$src" ]; then
    echo "brak pliku reguł: $src" >&2
    exit 1
fi

install -Dm644 "$src" "$dst"
echo "zainstalowano: $dst"

# Moduł uinput może nie być załadowany — ładujemy, żeby węzeł powstał od razu.
modprobe uinput 2>/dev/null || true

udevadm control --reload-rules
udevadm trigger --subsystem-match=misc --sysname-match=uinput 2>/dev/null || true
udevadm trigger --subsystem-match=input 2>/dev/null || true
udevadm trigger --subsystem-match=usb --attr-match=idVendor=28de 2>/dev/null || true
udevadm settle 2>/dev/null || true

if [ -e /dev/uinput ]; then
    ls -l /dev/uinput
    echo "Gotowe: /dev/uinput dostępny dla grupy 'input' i aktywnej sesji (uaccess)."
else
    echo "Uwaga: /dev/uinput nadal nie istnieje — sprawdź, czy moduł 'uinput' jest dostępny." >&2
fi

echo
echo "Użytkownik musi być w grupie 'input' (na wypadek braku ACL z logind):"
echo "  sudo usermod -aG input \"\${SUDO_USER:-\$USER}\""
echo "Po zmianie grupy wyloguj się i zaloguj ponownie."
