# Luna Installer (`luna-installer`)

Kreator instalacji systemu Luna z obrazu live, zbudowany wyłącznie na libLunie.
Prowadzi przez kroki w stylu Calamares i zleca pracę z dyskiem backendowi
uruchamianemu przez `pkexec`.

```sh
cargo run
cargo test
cargo run --bin installer-render -- --step disk --dark /tmp/installer.png
```

## Kroki

1. **Welcome** — opis instalacji i ostrzeżenie o trybie live.
2. **Language & keyboard** — języki **z systemu** (nie tylko z Luny) z pokryciem
   tłumaczeń Luny w procentach, wyszukiwany popup układów XKB, warianty i model.
3. **Region & timezone** — strefa wykrywana automatycznie, wyszukiwany popup
   `zone.tab`; język podpowiada strefę.
4. **Network** — status połączenia, lista Wi-Fi i łączenie przez `nmcli` (sprawdzanie w tle)
   (hasło przez `stdin`, nie `argv`), ponowne skanowanie.
5. **Requirements** — architektura, live image, UEFI/BIOS, Secure Boot, RAM,
   zasilanie, sieć, backend, dyski i wymagane narzędzia. Błędy blokują dalej.
6. **Disk & partitioning** — dysk + trzy tryby:
   - **automatyczny** (nowa tablica partycji, cały dysk),
   - **zainstaluj obok** (największy obszar nieprzydzielony, min. 20 GiB),
   - **ręczny** (wybór root/EFI/swap, bez zmiany rozmiarów).
   Do tego **ext4/Btrfs/XFS**, swap (0/2/4/8 GiB lub partycja), **LUKS2**,
   GRUB albo brak bootloadera i **aktualizacja systemu w trakcie instalacji**.
7. **Account** — dane użytkownika, hasło, hostname, autologin i tryb roota
   (zablokowany / hasło użytkownika / osobne hasło roota).
8. **Summary** — pełny plan, ostrzeżenia i obowiązkowe potwierdzenie
   „Wyczyść dysk i zainstaluj Lunę”.
9. **Progress** — procent, dziennik backendu, ukrywanie szczegółów, zapis logu.
10. **Done** — restart, wyłączenie, ponowna próba po błędzie i zapis logu.

## Bezpieczeństwo

- UI nie formatuje dysków sam: tworzy plan i uruchamia
  `luna-install-backend` (albo ścieżkę z `LUNA_INSTALL_BACKEND`).
- Hasła (użytkownik, root, LUKS) idą do backendu osobnymi liniami **stdin**;
  nigdy nie trafiają do `argv`, `ps` ani dziennika.
- Dyski z działającym systemem/nośnikiem live i mniejsze niż 8 GiB są
  zablokowane. Tryb „obok” wymaga 20 GiB nieprzydzielonego miejsca.
- Bez potwierdzenia w podsumowaniu backend nie jest uruchamiany.
- Backend dodatkowo sprawdza, czy dysk nie jest partycją i nie jest zamontowany.

## Backend

Kontrakt:

```sh
luna-install-backend --mode erase|free|manual --disk /dev/nvme0n1 \
    --filesystem btrfs --swap 4 --encrypt yes --bootloader grub \
    --update yes --firmware uefi --hostname luna-pc --username luna \
    --full-name "Luna Tester" --locale pl_PL.UTF-8 --timezone Europe/Warsaw \
    --keyboard-layout pl --keyboard-variant "" --keyboard-model pc105 \
    --autologin no --root-mode locked \
    [--root-partition /dev/... --efi-partition /dev/... --swap-partition /dev/...]
# stdin: hasło użytkownika, hasło roota (custom), hasło LUKS
```

Referencyjny backend (`data/luna-install-backend`, instalowany do
`/usr/libexec/luna/`) obsługuje:

- GPT + ESP/bios_grub albo użycie istniejącej ESP,
- `ext4`, `Btrfs` (subwoluminy `@`/`@home`, kompresja zstd) i `XFS`,
- LUKS2 + `/etc/crypttab`,
- swapfile albo wybraną partycję swap,
- `rsync` systemu live, fstab z UUID, locale, strefę, klawiaturę,
- konto w grupach `sudo/wheel/video/render/input/audio/...`,
- autologin w `greetd` (jeśli konfiguracja istnieje),
- aktualizację w chroot — te same menedżery co Sklep:
  `pacman -Syu`, `pag upgrade -y`, `apt-get update && apt-get -y upgrade`,
  `dnf -y upgrade`, `emerge -uDN @world`,
- GRUB UEFI/BIOS oraz odświeżenie initramfs.

Wymaga m.in. `rsync`, `blkid`, `sgdisk`/`parted`, `mkfs.*`, `cryptsetup`
(dla LUKS), `grub-install`. Dystrybucja może dostarczyć własny backend o tym
samym kontrakcie.

## Podgląd bez Waylanda

```sh
for step in welcome language region network requirements disk account summary progress done; do
  cargo run --bin installer-render -- --step "$step" "/tmp/installer-$step.png"
done
cargo run --bin installer-render -- --step done --error --dark /tmp/installer-error.png
```

## Licencja

GNU GPL v3.0 lub nowsza (`GPL-3.0-or-later`) — pełny tekst w [`LICENSE`](LICENSE).
© 2026 Luna Project.
