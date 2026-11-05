# Gry i Steam pod Luną

Luna uruchamia gry natywnie oraz przez **Steam Flatpak** i **Lutris**. Ich poprawne
działanie zależy od dwóch rzeczy poza samym kompozytorem: uprawnień do urządzeń
wejściowych oraz działającego portalu XDG (Flatpak używa go do uruchamiania gier).

## 0. Wdrożenie poprawek systemowych

Sesja Luny uruchamia `/usr/local/bin/selene` (systemowy), a nie wersję z
`~/.local/bin`, więc sama instalacja użytkownika nie wystarczy. Po aktualizacji
repozytorium wykonaj:

```sh
sudo ./scripts/install.sh --only selene --no-build   # nowy kompozytor
sudo ./scripts/install.sh --only base   --no-build   # luna-session + i18n
sudo ./scripts/install-input-rules.sh                # /dev/uinput + pady
sudo pag install gnome-desktop-44.5                  # backend portali
```

Następnie **wyloguj się i zaloguj ponownie**. Dopiero nowa sesja uruchomi nowy
kompozytor i portal.

## 1. „Missing permissions for input devices”

Steam Flatpak sprawdza, czy ma prawo odczytu i zapisu do `/dev/uinput`. Na świeżym
systemie węzeł wygląda tak:

```text
crw------- 1 root root 10, 223 /dev/uinput
```

Sandbox Flatpaka (`--device=all`, który Steam ma domyślnie) widzi urządzenie, ale
**hostowe uprawnienia nadal obowiązują**, więc zapis się nie udaje. Steam Input
(wirtualny pad) nie działa, a przy starcie pojawia się ostrzeżenie.

Naprawa — reguła w `de/udev/60-luna-input.rules`:

```sh
sudo ./scripts/install-input-rules.sh
```

Skrypt:
- instaluje `/etc/udev/rules.d/60-luna-input.rules`,
- ładuje `uinput` i przeładowuje reguły,
- nadaje `/dev/uinput` grupę `input`, tryb `0660` oraz ACL aktywnej sesji (`uaccess`),
- dodaje dostęp do kontrolerów Valve (USB `28de`, `hidraw`).

Po instalacji sprawdź:

```sh
ls -l /dev/uinput
# crw-rw----+ 1 root input ... /dev/uinput
```

Użytkownik powinien też należeć do grupy `input` (fallback, gdy ACL z logind nie
działa):

```sh
sudo usermod -aG input "$USER"
```

Po zmianie grupy **wyloguj się i zaloguj ponownie**.

## 2. Flatpak nie uruchamia się / „portal timed out”

W logu sesji (`~/.local/state/luna/session.log`) może pojawić się:

```text
luna-session: xdg-desktop-portal: backend GTK niekompletny
  (libgnome-desktop-3.so.20 => not found) — pomijam portal
```

Wtedy `xdg-desktop-portal` nie działa, a Flatpak (w tym Steam) czeka na
`org.freedesktop.portal.Flatpak` i kończy się przekroczeniem czasu. To nie jest
błąd Luny — brakuje biblioteki wymaganej przez `xdg-desktop-portal-gtk`.

Naprawa (Pagan Linux):

```sh
sudo pag install gnome-desktop-44.5
ldd /usr/lib/xdg-desktop-portal-gtk | grep "not found"   # pusto = OK
```

Następnie wyloguj się i zaloguj ponownie — `luna-session` sam uruchomi portal,
gdy backend jest kompletny. Weryfikacja:

```sh
pgrep -af xdg-desktop-portal
```

Na innych dystrybucjach zainstaluj pakiet dostarczający `libgnome-desktop-3.so.20`
(np. `gnome-desktop3` / `libgnome-desktop-3-20`).

## 3. Uprawnienia Flatpaka

Steam Flatpak ma już `devices=all`:

```sh
flatpak info --show-permissions com.valvesoftware.Steam | grep devices
# devices=all;
```

Jeśli korzystasz z innej instalacji bez tej pozycji:

```sh
flatpak override --user --device=input com.valvesoftware.Steam
# albo pełny dostęp do urządzeń:
flatpak override --user --device=all com.valvesoftware.Steam
```

## 4. Uszkodzona aktualizacja Steam

W `~/.var/app/com.valvesoftware.Steam/.local/share/Steam/logs/console-linux.txt`
może pojawić się:

```text
Error: Failed to unzip package (.../bins_codecs_ubuntu12.zip)
Fatal error: Failed to load steamui.so
```

To przerwane pobranie aktualizacji. Pomaga:

```sh
flatpak update com.valvesoftware.Steam
# awaryjnie: usuń niedokończone archiwum i uruchom Steam ponownie
rm -f ~/.var/app/com.valvesoftware.Steam/.local/share/Steam/package/*.zip
```

## 5. Mysz i pady w grach

Selene ogłasza `zwp_relative_pointer_v1` (surowy ruch myszy dla mouselooku) i
poprawnie realizuje „implicit grab” przycisków. Blokada kursora
(`zwp_pointer_constraints_v1`) jest nadal w planach — patrz
[`docs/compositor.md`](compositor.md).

## Szybka diagnostyka

```sh
# urządzenia
ls -l /dev/uinput
flatpak info --show-permissions com.valvesoftware.Steam | grep -E "devices|filesystems"

# portal
pgrep -af xdg-desktop-portal
ldd /usr/lib/xdg-desktop-portal-gtk | grep "not found"

# Steam
flatpak run com.valvesoftware.Steam
tail -n 30 ~/.var/app/com.valvesoftware.Steam/.local/share/Steam/logs/console-linux.txt
```
