#!/usr/bin/env python3
"""Generator motywu kursorów Luna-Cursors (format XCursor).

Motyw jest w pełni generowany z wektorów: każdy kształt to SVG w siatce 24×24,
rasteryzowany do PNG (cairosvg), a potem pakowany do binarnego XCursor razem
z hotspotem. Dzięki temu jeden plik źródłowy opisuje kształt, hotspot i aliasy.

Wymagania: python3, cairosvg, Pillow.

Uruchom: python3 assets/cursors/generate.py
→ assets/cursors/Luna-Cursors/{index.theme,cursor.theme,cursors/*}
→ docs/images/cursors.png (arkusz podglądu do dokumentacji)

Format XCursor (little-endian):
  nagłówek: magic "Xcur", header=16, version=0x00010000, ntoc
  TOC:      type=0xFFFD0002 (obraz), subtype=rozmiar nominalny, offset
  obraz:    header=36, type, subtype, version=1, width, height, xhot, yhot,
            delay, width*height pikseli ARGB (bajty B, G, R, A)
"""

from __future__ import annotations

import io
import struct
import sys
from pathlib import Path

try:
    import cairosvg
    from PIL import Image, ImageDraw, ImageFont
except ImportError as error:  # pragma: no cover - zależność środowiska
    print(
        f"brak zależności: {error}\nZainstaluj: python3 -m pip install cairosvg pillow",
        file=sys.stderr,
    )
    raise SystemExit(1)

ROOT = Path(__file__).resolve().parent
THEME = ROOT / "Luna-Cursors"
DOCS = ROOT.parents[1] / "docs" / "images"
SIZES = (24, 32, 48)

# Wspólne style. Jasne wypełnienie i ciemny (ale nie czarny) obrys czytają się
# na jasnym i ciemnym tle, a przy tym nie wyglądają jak z Windows 95. Cienki
# obrys + miękkie łączenia to cały sekret „nowoczesnego” kursora.
OUTLINE = "#1b202b"
FILL = "#ffffff"
SOFT = "#7aa2f7"
DANGER = "#e05555"

BASE = (
    f"fill:{FILL};stroke:{OUTLINE};stroke-width:1.3;"
    "stroke-linejoin:round;stroke-linecap:round"
)
DETAIL = (
    f"fill:none;stroke:{OUTLINE};stroke-width:1.25;"
    "stroke-linecap:round;stroke-linejoin:round"
)
HAIR = f"fill:none;stroke:{OUTLINE};stroke-width:1.0;stroke-linecap:round"
RED = f"fill:none;stroke:{DANGER};stroke-width:1.9;stroke-linecap:round"
RING = f"fill:{FILL};stroke:{OUTLINE};stroke-width:1.65"

# Strzałka: wysoka i smukła (szeroka wyglądała topornie), z zaokrąglonymi łączeniami.
ARROW_D = "M3.9 2.1V17.6L7.8 13.9L10.2 19.7L12.8 18.6L10.4 12.9H16.0Z"
ARROW = f'<path style="{BASE}" d="{ARROW_D}"/>'


def capsule(d: str, width: float = 3.4) -> str:
    """Kapsułka rysowana podwójnym obrysem (kciuk, rączka lupy)."""
    return (
        f'<path d="{d}" fill="none" stroke="{OUTLINE}" stroke-width="{width}" '
        f'stroke-linecap="round"/>'
        f'<path d="{d}" fill="none" stroke="{FILL}" stroke-width="{width - 1.5}" '
        f'stroke-linecap="round"/>'
    )


# Sylwetka dłoni jednym konturem (fale u góry to palce). Osobne prostokąty na
# palce przy 24 px zlewały się w plamę — dlatego jeden kształt + cienkie linie.
OPEN_HAND_D = (
    "M10.6 20.8a3.8 3.8 0 0 1-3.8-3.8V6.9a1.25 1.25 0 0 1 2.5 0v3.4"
    "V5.2a1.25 1.25 0 0 1 2.5 0v4.0V6.1a1.25 1.25 0 0 1 2.5 0v3.7"
    "V7.4a1.2 1.2 0 0 1 2.4 0v9.6a3.6 3.6 0 0 1-3.6 3.6Z"
)
OPEN_HAND = (
    capsule("M7.6 14.6 4.6 10.2", 3.6)
    + f'<path style="{BASE}" d="{OPEN_HAND_D}"/>'
)

# Wskazujący palec i pięść jako jeden kontur — dwa osobne kształty zostawiały
# sztuczną kreskę między palcem a dłonią (wyglądało jak „butelka”).
POINT_HAND_D = (
    "M9.2 11.2V4.15a1.35 1.35 0 0 1 2.7 0V11.2H15.2a2.2 2.2 0 0 1 2.2 2.2"
    "V16.9a3.9 3.9 0 0 1-3.9 3.9H10.7a3.8 3.8 0 0 1-3.8-3.8"
    "C5.4 15.2 5.6 12.8 7.6 11.8Z"
)
HAND2 = (
    f'<path style="{BASE}" d="{POINT_HAND_D}"/>'
    # Kciuk jest częścią konturu (wybrzuszenie po lewej), a ta kreska tylko
    # oddziela go od dłoni — inaczej kciuk wyglądał jak doklejony patyczek.
    + f'<path style="{HAIR}" d="M9.0 11.6c-.8 1.1-1.2 2.6-1.2 4.1"/>'
    + f'<path style="{HAIR}" d="M13.2 11.8v2.6M15.4 12.0v2.4"/>'
)

# Zaciśnięta pięść: górna krawędź to cztery kłykcie (fale), a kciuk leży
# w poprzek dłoni. Same pionowe kreski w klocku czytały się jak koszyk.
FIST = (
    f'<path style="{BASE}" d="M10.2 20.8a3.7 3.7 0 0 1-3.7-3.7'
    'C4.9 15.3 5.1 12.7 7.4 12.3'
    'a1.2 0.9 0 0 1 2.4 0a1.2 0.9 0 0 1 2.4 0'
    'a1.2 0.9 0 0 1 2.4 0a1.2 0.9 0 0 1 2.4 0'
    'V17.1a3.7 3.7 0 0 1-3.7 3.7Z"/>'
    + f'<path style="{HAIR}" d="M8.7 12.7c-.8 1.0-1.1 2.3-1.1 3.6"/>'
)

# Lupa: grubszy pierścień czyta się lepiej niż cienki okrąg.
ZOOM_LENS = f'<circle cx="10.1" cy="10.1" r="5.9" style="{RING}"/>'
ZOOM_HANDLE = capsule("M14.4 14.4 20.6 20.6", 3.6)

# nazwa → (kształt SVG, hotspot w siatce 24×24, aliasy)
CURSORS: dict[str, tuple[str, tuple[float, float], tuple[str, ...]]] = {
    "left_ptr": (
        ARROW,
        (3.9, 2.1),
        ("default", "arrow", "top_left_arrow", "dnd-none"),
    ),
    "left_ptr_watch": (
        f'<g transform="scale(0.72)">{ARROW}</g>'
        f'<circle cx="17.4" cy="17.4" r="5.2" style="{RING}"/>'
        f'<path style="{HAIR}" d="M17.4 14.8v2.7l1.9 1.2"/>',
        (2.8, 1.5),
        ("progress",),
    ),
    "watch": (
        f'<circle cx="12" cy="12" r="8.0" style="{RING}"/>'
        # Krótki akcent na pierścieniu — zegar „pracuje”, a nie tylko stoi.
        f'<path d="M12 5.7a6.3 6.3 0 0 1 6.0 4.4" fill="none" stroke="{SOFT}" '
        'stroke-width="1.7" stroke-linecap="round"/>'
        f'<path style="{DETAIL}" d="M12 7.6v4.6l3.2 1.9"/>'
        f'<circle cx="12" cy="12" r="0.95" fill="{OUTLINE}"/>',
        (12.0, 12.0),
        ("wait",),
    ),
    "hand1": (
        OPEN_HAND,
        (12.0, 12.0),
        ("openhand", "grab", "grab_cursor"),
    ),
    "grabbing": (
        FIST,
        (12.0, 12.0),
        ("closedhand", "grabbing_cursor"),
    ),
    "hand2": (
        HAND2,
        (9.2, 2.8),
        ("hand", "pointer", "pointing_hand"),
    ),
    "xterm": (
        # Cienka belka gubi się przy 24 px — tutaj grubiej niż reszta detali.
        f'<path d="M7.4 4.6h9.2M12 4.6v14.8M7.4 19.4h9.2" fill="none" '
        f'stroke="{OUTLINE}" stroke-width="1.6" stroke-linecap="round"/>',
        (12.0, 12.0),
        ("text", "vertical-text"),
    ),
    "crosshair": (
        f'<path d="M12 2.2v7.6M12 14.2v7.6M2.2 12h7.6M14.2 12h7.6" fill="none" '
        f'stroke="{OUTLINE}" stroke-width="1.5" stroke-linecap="round"/>'
        f'<circle cx="12" cy="12" r="1.15" fill="{OUTLINE}"/>',
        (12.0, 12.0),
        ("cell", "plus"),
    ),
    "fleur": (
        f'<path style="{BASE}" d="M12 1.6 15.2 6h-2.2v4.6h4.6V8.4L21.9 12l-4.3 3.6v-2.2'
        'h-4.6v4.6h2.2L12 22.4 8.8 18h2.2v-4.6H6.4v2.2L2.1 12l4.3-3.6v2.2H11V6H8.8Z"/>',
        (12.0, 12.0),
        ("move", "all-scroll", "all-resize"),
    ),
    "sb_h_double_arrow": (
        f'<path style="{BASE}" d="M1.8 12 6.2 7.6v3.0h11.6v-3.0L22.2 12l-4.4 4.4'
        'v-3.0H6.2v3.0Z"/>',
        (12.0, 12.0),
        ("ew-resize", "col-resize", "size_hor"),
    ),
    "sb_v_double_arrow": (
        f'<path style="{BASE}" d="M12 1.8 16.4 6.2h-3.0v11.6h3.0L12 22.2l-4.4-4.4'
        'h3.0V6.2h-3.0Z"/>',
        (12.0, 12.0),
        ("ns-resize", "row-resize", "size_ver"),
    ),
    "fd_double_arrow": (
        f'<g transform="rotate(45 12 12)"><path style="{BASE}" '
        'd="M1.8 12 6.2 7.6v3.0h11.6v-3.0L22.2 12l-4.4 4.4v-3.0H6.2v3.0Z"/></g>',
        (12.0, 12.0),
        ("nwse-resize", "top_left_corner", "bottom_right_corner"),
    ),
    "bd_double_arrow": (
        f'<g transform="rotate(-45 12 12)"><path style="{BASE}" '
        'd="M1.8 12 6.2 7.6v3.0h11.6v-3.0L22.2 12l-4.4 4.4v-3.0H6.2v3.0Z"/></g>',
        (12.0, 12.0),
        ("nesw-resize", "top_right_corner", "bottom_left_corner"),
    ),
    "sb_left_arrow": (
        f'<path style="{BASE}" d="M2.0 12 7.2 6.6v3.0h14.6v4.8H7.2v3.0Z"/>',
        (2.0, 12.0),
        ("w-resize",),
    ),
    "sb_right_arrow": (
        f'<path style="{BASE}" d="M22.0 12 16.8 6.6v3.0H2.2v4.8h14.6v3.0Z"/>',
        (22.0, 12.0),
        ("e-resize",),
    ),
    "sb_up_arrow": (
        f'<path style="{BASE}" d="M12 2.0 17.6 8.2h-3.2v5.4H9.6V8.2H6.4Z"/>',
        (12.0, 2.0),
        ("n-resize",),
    ),
    "sb_down_arrow": (
        f'<path style="{BASE}" d="M12 22.0 6.4 15.8h3.2v-5.4h4.8v5.4h3.2Z"/>',
        (12.0, 22.0),
        ("s-resize",),
    ),
    "crossed_circle": (
        f'<circle cx="12" cy="12" r="8.1" style="{RING}"/>'
        f'<path style="{RED}" d="M6.3 17.7 17.7 6.3"/>',
        (12.0, 12.0),
        ("not-allowed", "no-drop", "forbidden", "dnd-no-drop"),
    ),
    "question_arrow": (
        f'<g transform="scale(0.8)">{ARROW}</g>'
        f'<path style="{DETAIL}" d="M14.6 15.0a2.2 2.2 0 1 1 3.0 2.1'
        'c-.8.3-1.1.8-1.1 1.5"/>'
        f'<circle cx="16.5" cy="20.2" r="0.95" fill="{OUTLINE}"/>',
        (3.1, 1.7),
        ("help", "whats_this"),
    ),
    "up_arrow": (
        f'<path style="{BASE}" d="M12 2.0 17.6 8.2h-3.2v5.4H9.6V8.2H6.4Z"/>',
        (12.0, 2.0),
        (),
    ),
    "zoom-in": (
        ZOOM_LENS
        + ZOOM_HANDLE
        + f'<path style="{DETAIL}" d="M7.3 10.1h5.6M10.1 7.3v5.6"/>',
        (10.1, 10.1),
        (),
    ),
    "zoom-out": (
        ZOOM_LENS + ZOOM_HANDLE + f'<path style="{DETAIL}" d="M7.3 10.1h5.6"/>',
        (10.1, 10.1),
        (),
    ),
    "copy": (
        f'<g transform="scale(0.78)">{ARROW}</g>'
        '<rect x="12.6" y="12.4" width="8.6" height="8.8" rx="1.8" style="'
        + BASE
        + '"/>'
        + f'<path style="{HAIR}" d="M15.0 16.4h3.8M15.0 18.6h3.8"/>',
        (3.0, 1.6),
        ("dnd-copy",),
    ),
    "context-menu": (
        f'<g transform="scale(0.78)">{ARROW}</g>'
        '<rect x="12.2" y="12.6" width="9.2" height="8.2" rx="1.8" style="'
        + BASE
        + '"/>'
        + f'<path style="{HAIR}" d="M14.6 15.2h4.4M14.6 17.0h4.4M14.6 18.8h4.4"/>',
        (3.0, 1.6),
        (),
    ),
    "link": (
        f'<g transform="scale(0.78)">{ARROW}</g>'
        + capsule("M16.4 20.6 18.8 18.2", 3.0)
        + capsule("M17.6 17.0 20.0 14.6", 3.0),
        (3.0, 1.6),
        ("alias", "dnd-link"),
    ),
    "move": (
        f'<path style="{BASE}" d="M12 1.6 15.2 6h-2.2v4.6h4.6V8.4L21.9 12l-4.3 3.6v-2.2'
        'h-4.6v4.6h2.2L12 22.4 8.8 18h2.2v-4.6H6.4v2.2L2.1 12l4.3-3.6v2.2H11V6H8.8Z"/>',
        (12.0, 12.0),
        ("dnd-move",),
    ),
}

# Kolejność na arkuszu podglądu (tylko kształty fizyczne, bez aliasów).
SHEET = [
    "left_ptr",
    "hand2",
    "hand1",
    "grabbing",
    "xterm",
    "crosshair",
    "fleur",
    "watch",
    "left_ptr_watch",
    "crossed_circle",
    "question_arrow",
    "up_arrow",
    "sb_h_double_arrow",
    "sb_v_double_arrow",
    "fd_double_arrow",
    "bd_double_arrow",
    "sb_left_arrow",
    "sb_right_arrow",
    "sb_up_arrow",
    "sb_down_arrow",
    "zoom-in",
    "zoom-out",
    "copy",
    "context-menu",
]

IMAGE_TYPE = 0xFFFD0002
MAGIC = 0x72756358  # "Xcur"


def render_svg(body: str, size: int) -> bytes:
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
        f'viewBox="0 0 24 24">{body}</svg>'
    )
    return cairosvg.svg2png(
        bytestring=svg.encode("utf-8"), output_width=size, output_height=size
    )


def image_chunk(body: str, nominal: int, hotspot: tuple[float, float]) -> bytes:
    png = render_svg(body, nominal)
    with Image.open(io.BytesIO(png)) as image:
        rgba = image.convert("RGBA")
        width, height = rgba.size
        pixels = rgba.tobytes()
    # Xcursor trzyma piksele jako ARGB w Little Endian, czyli w pliku B, G, R, A.
    argb = bytearray(len(pixels))
    for i in range(0, len(pixels), 4):
        argb[i] = pixels[i + 2]
        argb[i + 1] = pixels[i + 1]
        argb[i + 2] = pixels[i]
        argb[i + 3] = pixels[i + 3]
    xhot = min(max(round(hotspot[0] * nominal / 24.0), 0), width - 1)
    yhot = min(max(round(hotspot[1] * nominal / 24.0), 0), height - 1)
    header = struct.pack(
        "<IIIIIIIII", 36, IMAGE_TYPE, nominal, 1, width, height, xhot, yhot, 0
    )
    return header + bytes(argb)


def write_xcursor(path: Path, chunks: list[tuple[int, bytes]]) -> None:
    offset = 16 + len(chunks) * 12
    toc = b""
    body = b""
    for nominal, chunk in chunks:
        toc += struct.pack("<III", IMAGE_TYPE, nominal, offset)
        offset += len(chunk)
        body += chunk
    path.write_bytes(
        struct.pack("<IIII", MAGIC, 16, 0x00010000, len(chunks)) + toc + body
    )


def main() -> None:
    cursors_dir = THEME / "cursors"
    if cursors_dir.exists():
        for entry in cursors_dir.iterdir():
            entry.unlink()
    cursors_dir.mkdir(parents=True, exist_ok=True)

    for name, (body, hotspot, aliases) in CURSORS.items():
        chunks = [(size, image_chunk(body, size, hotspot)) for size in SIZES]
        write_xcursor(cursors_dir / name, chunks)
        for alias in aliases:
            link = cursors_dir / alias
            if link.exists() or link.is_symlink():
                link.unlink()
            link.symlink_to(name)

    THEME.mkdir(parents=True, exist_ok=True)
    theme_text = (
        "[Icon Theme]\n"
        "Name=Luna-Cursors\n"
        "Comment=Kursory Luny — białe kształty z cienkim obrysem, czytelne na jasnym i ciemnym tle\n"
        "Inherits=Adwaita\n"
    )
    (THEME / "index.theme").write_text(theme_text, encoding="utf-8")
    (THEME / "cursor.theme").write_text(theme_text, encoding="utf-8")

    write_sheet(cursors_dir)
    print(
        f"Luna-Cursors: {len(CURSORS)} kształtów w {len(SIZES)} rozmiarach → {THEME}"
    )


def write_sheet(cursors_dir: Path) -> None:
    """Arkusz podglądu wszystkich fizycznych kształtów do dokumentacji."""
    cell_w, cell_h, icon = 118, 92, 48
    columns = 6
    rows = (len(SHEET) + columns - 1) // columns
    sheet = Image.new("RGB", (columns * cell_w, rows * cell_h), (36, 36, 36))
    draw = ImageDraw.Draw(sheet)
    try:
        font = ImageFont.truetype("/usr/share/fonts/TTF/DejaVuSans.ttf", 11)
    except OSError:
        font = ImageFont.load_default()
    for index, name in enumerate(SHEET):
        png = render_svg(CURSORS[name][0], icon)
        with Image.open(io.BytesIO(png)) as image:
            sprite = image.convert("RGBA")
        x = (index % columns) * cell_w + (cell_w - icon) // 2
        y = (index // columns) * cell_h + 10
        sheet.paste(sprite, (x, y), sprite)
        label = name
        box = draw.textbbox((0, 0), label, font=font)
        draw.text(
            (
                (index % columns) * cell_w + (cell_w - (box[2] - box[0])) // 2,
                y + icon + 12,
            ),
            label,
            fill=(232, 232, 232),
            font=font,
        )
    DOCS.mkdir(parents=True, exist_ok=True)
    sheet.save(DOCS / "cursors.png", optimize=True)
    print(f"arkusz: {DOCS / 'cursors.png'}")


if __name__ == "__main__":
    main()
