# Przykładowe motywy Luny

Pliki `.theme` można skopiować do:

```
~/.config/luna/themes/          # najwyższy priorytet (użytkownik)
~/.local/share/luna/themes/     # użytkownik
/usr/share/luna/themes/         # system
```

a następnie wybrać w **Ustawienia → Wygląd → Styl**. Katalog można też
wskazać zmienną `LUNA_THEMES_DIR` (lista katalogów rozdzielona `:`).

## Pliki

| Plik | Co pokazuje |
| --- | --- |
| `nord.theme` | klasyczny zapis `klucz = wartość`, ciemna paleta |
| `paper.theme` | jasna paleta + zmiana geometrii (promienie, wysokość kontrolek) |
| `redluna.theme` | czytelny zapis `::Name`, sekcja `panel:` |
| `full-example.theme` | **wszystkie** obsługiwane klucze (paleta, składnia, odstępy, promienie, typografia, metryki, panel) |

## Najprostszy motyw

```ini
::Name - Mój motyw
appearance = dark
accent = teal

panel:
-color-bar: red
-round: false
-float: true
-transparent: 40%
```

- `color-bar` — kolor paska (i nagłówków okien, jeśli nie ustawiono `colors.headerbar_bg`),
- `round` — zaokrąglone rogi paska,
- `float` — pływający pasek (nie zajmuje całej krawędzi),
- `transparent` — procent przezroczystości, `opacity` — procent krycia,
- kolory: `#rrggbb`, `#rrggbbaa` lub nazwy (`red`, `teal`, `black`…).

Położenie panelu (góra/dół/lewo/prawo) i tryb tray’a (ikony / ikona+nazwa)
ustawia się w **Ustawieniach → Wygląd → Panel**, nie w motywie.

## Motywy 2.0 (grafiki dla przycisków itd.)

Plan rozszerzenia silnika o assety (PNG/SVG, 9-slice, stany kontrolek) jest
opisany w [`THEMING-TODO.md`](../../../THEMING-TODO.md).
