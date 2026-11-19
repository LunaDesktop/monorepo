# Motyw GRUB „Luna” dla PaganOS

Nocne niebo, półksiężyc z brandingu Luny i menu systemów na półprzezroczystym
panelu — motyw jest instalowany razem ze środowiskiem i włączany automatycznie
przez `/etc/grub.d/05_luna_theme`.

## Pliki

| Plik | Rola |
| --- | --- |
| `theme.txt` | układ menu, pasek odliczania i podpowiedź klawiszy |
| `background.png` | tło 1920×1080 (skalowane przez GRUB-a do rozdzielczości ekranu) |
| `select_*.png` | 9-slice podświetlenia wybranej pozycji |
| `terminal_*.png` | 9-slice tła konsoli GRUB-a (`C`) |
| `dejavu_*.pf2` | fonty menu i konsoli (DejaVu Sans / Mono) |
| `generate.py` | generator tła i pixmap (Pillow + opcjonalnie `grub-mkfont`) |

Motyw nie zależy od języka: GRUB sam tłumaczy nazwy systemów, a jedyny stały
napis to symbole klawiszy (`↑ ↓ · Enter · E · C`).

## Włączanie i wyłączanie

Motyw trafia do `<prefix>/share/grub/themes/luna` oraz — jeśli istnieje
`/boot/grub` — do `/boot/grub/themes/luna`. Skrypt `05_luna_theme` preferuje
kopię na `/boot`, bo GRUB zawsze czyta tę partycję (także przy osobnym `/boot`
i starszym GRUB-ie, który nie widzi `/usr`).

`05_luna_theme` działa tylko wtedy, gdy w `/etc/default/grub` **nie ma**
`GRUB_THEME`. Jawny wybór administratora zawsze wygrywa — np.:

```sh
GRUB_THEME="/usr/share/grub/themes/starfield/theme.txt"
```

Po zmianie motywu odśwież menu:

```sh
sudo grub-mkconfig -o /boot/grub/grub.cfg
```

Motyw jest pakowany jako `luna-grub-theme` i wchodzi w zależności metapakietu
`luna-desktop`, więc aktualizacja środowiska instaluje go i odświeża menu.

## Odtwarzanie grafiki

```sh
python3 assets/grub/luna/generate.py
```

Skrypt zapisuje `background.png`, dziewięć kawałków `select_*`/`terminal_*`
oraz fonty `*.pf2` (jeśli w systemie jest `grub-mkfont`). Zrzut w
`docs/images/grub-theme.png` pochodzi z rozruchu testowego GRUB-a w QEMU.

## Licencja

Grafika i konfiguracja: MPL-2.0 (spójnie z brandingiem Luny).
Fonty DejaVu: licencja Bitstream Vera / public domain — patrz
`/usr/share/doc/dejavu-ttf/` w systemie.
