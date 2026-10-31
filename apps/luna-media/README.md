# Luna Media (`luna-media`)

Przeglądarka obrazów, RAW-ów, filmów i PDF-ów środowiska Luna na libLunie — galeria
z miniaturami, podgląd z zoomem i pokazem slajdów, odtwarzacz wideo z dźwiękiem
oraz czytnik PDF ze zmianą stron i zaznaczaniem tekstu.

```sh
cargo run                       # galeria katalogu Obrazy (albo ostatniego)
cargo run -- ~/Wideo/film.mp4   # otwiera film w podglądzie
cargo run -- ~/Obrazy           # otwiera galerię katalogu
```

| Obsługa | Sposób |
|---|---|
| PNG, JPEG, SVG | natywnie |
| GIF, WebP, BMP, TIFF, AVIF… | `ffmpeg` |
| RAW (CR2, CR3, NEF, ARW, DNG, ORF, RW2, RAF, PEF…) | osadzony podgląd JPEG + pełna demozajka `dcraw_emu` |
| PDF | `pdfinfo` + `pdftocairo` / `pdftotext` (poppler) |
| filmy (MP4, MKV, WebM…) | `ffmpeg` + `ffprobe` |
| dźwięk | `pw-play` / `paplay` |

Pełny opis funkcji, skrótów, architektury i ograniczeń:
[`docs/media.md`](../../docs/media.md).

Podgląd bez okna:

```sh
cargo run --bin preview
# → ../../docs/images/luna-media.png, luna-media-viewer.png,
#   luna-media-video.png, luna-media-pdf.png
```

## Licencja

GNU GPL v3.0 lub nowsza (`GPL-3.0-or-later`) — pełny tekst w [`LICENSE`](LICENSE).
© 2026 Luna Project.
