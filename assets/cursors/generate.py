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
    from PIL import Image, ImageDraw, ImageFilter, ImageFont
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
OUTLINE = "#141820"
FILL = "#ffffff"
SOFT = "#7aa2f7"
DANGER = "#e05555"

BASE = (
    f"fill:{FILL};stroke:{OUTLINE};stroke-width:1.15;"
    "stroke-linejoin:round;stroke-linecap:round"
)
DETAIL = (
    f"fill:none;stroke:{OUTLINE};stroke-width:1.1;"
    "stroke-linecap:round;stroke-linejoin:round"
)
HAIR = f"fill:none;stroke:{OUTLINE};stroke-width:0.95;stroke-linecap:round"
RED = f"fill:none;stroke:{DANGER};stroke-width:1.85;stroke-linecap:round"
RING = f"fill:{FILL};stroke:{OUTLINE};stroke-width:1.45"

# Strzałka: nowoczesny kształt w stylu macOS/Bibata — lekko wcięty ogonek,
# gładkie proporcje i wyraźny czubek w (4.0, 2.2).
ARROW_D = (
    "M4.0 2.2L4.0 17.5C4.0 18.0 4.6 18.2 4.9 17.9L8.2 14.3L10.7 19.8"
    "C10.9 20.2 11.4 20.4 11.8 20.2L13.1 19.6C13.5 19.4 13.7 18.9 13.5 18.5"
    "L11.0 13.1L15.6 12.8C16.1 12.8 16.3 12.1 15.9 11.8Z"
)
ARROW = f'<path style="{BASE}" d="{ARROW_D}"/>'


def capsule(d: str, width: float = 3.2) -> str:
    """Kapsułka rysowana podwójnym obrysem (kciuk, rączka lupy)."""
    return (
        f'<path d="{d}" fill="none" stroke="{OUTLINE}" stroke-width="{width}" '
        f'stroke-linecap="round"/>'
        f'<path d="{d}" fill="none" stroke="{FILL}" stroke-width="{width - 1.4}" '
        f'stroke-linecap="round"/>'
    )


# Otwarta dłoń (grab): anatomiczne, zaokrąglone palce i naturalny kciuk.
OPEN_HAND_D = (
    "M8.4 19.8C7.0 19.5 6.0 18.2 5.4 16.5L3.6 12.3C3.3 11.6 3.7 10.9 4.4 10.7"
    "C5.0 10.5 5.7 10.9 6.0 11.5L7.1 13.6V6.6C7.1 5.9 7.6 5.4 8.3 5.4"
    "C9.0 5.4 9.5 5.9 9.5 6.6V10.0V4.9C9.5 4.2 10.0 3.7 10.7 3.7"
    "C11.4 3.7 11.9 4.2 11.9 4.9V10.0V5.4C11.9 4.7 12.4 4.2 13.1 4.2"
    "C13.8 4.2 14.3 4.7 14.3 5.4V10.3V7.1C14.3 6.5 14.8 6.0 15.4 6.0"
    "C16.0 6.0 16.5 6.5 16.5 7.1V15.4C16.5 18.2 14.5 20.1 11.6 20.1"
    "C10.4 20.1 9.3 20.0 8.4 19.8Z"
)
OPEN_HAND = (
    f'<path style="{BASE}" d="{OPEN_HAND_D}"/>'
    + f'<path style="{HAIR}" d="M9.5 10.2v1.8M11.9 10.2v1.8M14.3 10.4v1.7"/>'
)

# Wskazujący palec (pointer/link): smukły palec wskazujący, zgięte palce po prawej
# i naturalnie ułożony kciuk po lewej.
POINT_HAND_D = (
    "M8.6 11.8V4.2C8.6 3.4 9.2 2.8 10.0 2.8C10.8 2.8 11.4 3.4 11.4 4.2V9.8"
    "C11.7 9.4 12.2 9.2 12.7 9.2C13.4 9.2 14.0 9.6 14.2 10.2"
    "C14.5 9.9 15.0 9.7 15.5 9.7C16.2 9.7 16.8 10.2 17.0 10.8"
    "C17.3 10.6 17.6 10.5 18.0 10.5C18.8 10.5 19.4 11.1 19.4 11.9V15.6"
    "C19.4 18.4 17.4 20.4 14.4 20.4H11.2C9.4 20.4 8.0 19.5 7.0 17.9L4.7 14.2"
    "C4.3 13.5 4.6 12.6 5.3 12.2C6.0 11.8 6.8 12.1 7.2 12.7L8.6 14.8Z"
)
HAND2 = (
    f'<path style="{BASE}" d="{POINT_HAND_D}"/>'
    + f'<path style="{HAIR}" d="M11.4 10.6v2.8M14.2 11.0v2.5M16.9 11.5v2.2"/>'
)

# Zaciśnięta pięść (grabbing): zwarte zgięte palce i kciuk obejmujący dłoń.
FIST = (
    f'<path style="{BASE}" d="M9.6 20.1C7.6 20.1 6.2 18.8 5.4 17.0L4.2 14.2'
    'C3.9 13.5 4.3 12.7 5.0 12.5C5.6 12.3 6.2 12.6 6.5 13.2L7.2 14.5V10.6'
    'C7.2 9.8 7.8 9.2 8.6 9.2C9.1 9.2 9.5 9.5 9.8 9.9'
    'C10.0 9.3 10.6 8.9 11.3 8.9C11.9 8.9 12.4 9.2 12.7 9.7'
    'C13.0 9.2 13.5 8.9 14.1 8.9C14.8 8.9 15.4 9.3 15.6 10.0'
    'C15.9 9.7 16.3 9.5 16.8 9.5C17.6 9.5 18.2 10.1 18.2 10.9V15.4'
    'C18.2 18.2 16.2 20.1 13.3 20.1Z"/>'
    + f'<path style="{HAIR}" d="M9.8 10.5v2.4M12.7 10.3v2.5M15.5 10.5v2.3"/>'
)

# Lupa: grubszy pierścień czyta się lepiej niż cienki okrąg.
ZOOM_LENS = f'<circle cx="10.1" cy="10.1" r="5.7" style="{RING}"/>'
ZOOM_HANDLE = capsule("M14.3 14.3 20.3 20.3", 3.4)

# nazwa → (kształt SVG, hotspot w siatce 24×24, aliasy)
CURSORS: dict[str, tuple[str, tuple[float, float], tuple[str, ...]]] = {
    "left_ptr": (
        ARROW,
        (4.0, 2.2),
        ("default", "arrow", "top_left_arrow", "dnd-none"),
    ),
    "left_ptr_watch": (
        f'<g transform="scale(0.72)">{ARROW}</g>'
        f'<circle cx="17.2" cy="17.2" r="5.1" style="{RING}"/>'
        f'<path d="M17.2 13.4a3.8 3.8 0 0 1 3.6 2.7" fill="none" stroke="{SOFT}" '
        'stroke-width="1.4" stroke-linecap="round"/>'
        f'<path style="{HAIR}" d="M17.2 14.9v2.4l1.7 1.0"/>',
        (2.9, 1.6),
        ("progress",),
    ),
    "watch": (
        f'<circle cx="12" cy="12" r="7.8" style="{RING}"/>'
        f'<path d="M12 6.0a6.0 6.0 0 0 1 5.7 4.2" fill="none" stroke="{SOFT}" '
        'stroke-width="1.8" stroke-linecap="round"/>'
        f'<path style="{DETAIL}" d="M12 7.8v4.3l3.0 1.8"/>'
        f'<circle cx="12" cy="12" r="0.95" fill="{OUTLINE}"/>',
        (12.0, 12.0),
        ("wait",),
    ),
    "hand1": (
        OPEN_HAND,
        (11.0, 11.0),
        ("openhand", "grab", "grab_cursor"),
    ),
    "grabbing": (
        FIST,
        (11.5, 13.0),
        ("closedhand", "grabbing_cursor"),
    ),
    "hand2": (
        HAND2,
        (10.0, 2.8),
        ("hand", "pointer", "pointing_hand"),
    ),
    "xterm": (
        # Biała obwódka pod ciemnym rdzeniem — czytelna w czarnym terminalu i na białej kartce.
        f'<path d="M8.0 4.6h8.0M12 4.6v14.8M8.0 19.4h8.0" fill="none" '
        f'stroke="{FILL}" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round"/>'
        f'<path d="M8.0 4.6h8.0M12 4.6v14.8M8.0 19.4h8.0" fill="none" '
        f'stroke="{OUTLINE}" stroke-width="1.45" stroke-linecap="round" stroke-linejoin="round"/>',
        (12.0, 12.0),
        ("text", "vertical-text"),
    ),
    "crosshair": (
        f'<path d="M12 2.6v7.0M12 14.4v7.0M2.6 12h7.0M14.4 12h7.0" fill="none" '
        f'stroke="{FILL}" stroke-width="3.0" stroke-linecap="round"/>'
        f'<path d="M12 2.6v7.0M12 14.4v7.0M2.6 12h7.0M14.4 12h7.0" fill="none" '
        f'stroke="{OUTLINE}" stroke-width="1.4" stroke-linecap="round"/>'
        f'<circle cx="12" cy="12" r="1.1" fill="{OUTLINE}" stroke="{FILL}" stroke-width="0.6"/>',
        (12.0, 12.0),
        ("cell", "plus"),
    ),
    "fleur": (
        f'<path style="{BASE}" d="M12 2.0 15.0 5.8h-1.9v5.1h5.1V9.0L22.0 12l-3.8 3.0v-1.9'
        'h-5.1v5.1h1.9L12 22.0 9.0 18.2h1.9v-5.1H5.8v1.9L2.0 12l3.8-3.0v1.9h5.1V5.8H9.0Z"/>',
        (12.0, 12.0),
        ("move", "all-scroll", "all-resize"),
    ),
    "sb_h_double_arrow": (
        f'<path style="{BASE}" d="M2.2 12 6.4 8.0v2.7h11.2V8.0L21.8 12l-4.2 4.0'
        'v-2.7H6.4V16.0Z"/>',
        (12.0, 12.0),
        ("ew-resize", "col-resize", "size_hor"),
    ),
    "sb_v_double_arrow": (
        f'<path style="{BASE}" d="M12 2.2 16.0 6.4h-2.7v11.2h2.7L12 21.8l-4.0-4.2'
        'h2.7V6.4H8.0Z"/>',
        (12.0, 12.0),
        ("ns-resize", "row-resize", "size_ver"),
    ),
    "fd_double_arrow": (
        f'<g transform="rotate(45 12 12)"><path style="{BASE}" '
        'd="M2.2 12 6.4 8.0v2.7h11.2V8.0L21.8 12l-4.2 4.0v-2.7H6.4V16.0Z"/></g>',
        (12.0, 12.0),
        ("nwse-resize", "top_left_corner", "bottom_right_corner"),
    ),
    "bd_double_arrow": (
        f'<g transform="rotate(-45 12 12)"><path style="{BASE}" '
        'd="M2.2 12 6.4 8.0v2.7h11.2V8.0L21.8 12l-4.2 4.0v-2.7H6.4V16.0Z"/></g>',
        (12.0, 12.0),
        ("nesw-resize", "top_right_corner", "bottom_left_corner"),
    ),
    "sb_left_arrow": (
        f'<path style="{BASE}" d="M2.4 12 7.2 7.2v3.2h13.8v3.2H7.2v3.2Z"/>',
        (2.4, 12.0),
        ("w-resize",),
    ),
    "sb_right_arrow": (
        f'<path style="{BASE}" d="M21.6 12 16.8 7.2v3.2H3.0v3.2h13.8v3.2Z"/>',
        (21.6, 12.0),
        ("e-resize",),
    ),
    "sb_up_arrow": (
        f'<path style="{BASE}" d="M12 2.4 16.8 7.2h-3.2v13.8h-3.2V7.2H7.2Z"/>',
        (12.0, 2.4),
        ("n-resize",),
    ),
    "sb_down_arrow": (
        f'<path style="{BASE}" d="M12 21.6 7.2 16.8h3.2V3.0h3.2v13.8h3.2Z"/>',
        (12.0, 21.6),
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
        f'<path d="M14.6 15.0a2.2 2.2 0 1 1 3.0 2.1c-.8.3-1.1.8-1.1 1.5" '
        f'fill="none" stroke="{FILL}" stroke-width="2.8" stroke-linecap="round"/>'
        f'<circle cx="16.5" cy="20.2" r="1.75" fill="{FILL}"/>'
        f'<path style="{DETAIL}" d="M14.6 15.0a2.2 2.2 0 1 1 3.0 2.1'
        'c-.8.3-1.1.8-1.1 1.5"/>'
        f'<circle cx="16.5" cy="20.2" r="0.95" fill="{OUTLINE}"/>',
        (3.2, 1.8),
        ("help", "whats_this"),
    ),
    "up_arrow": (
        f'<path style="{BASE}" d="M12 2.4 16.8 7.2h-3.2v13.8h-3.2V7.2H7.2Z"/>',
        (12.0, 2.4),
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


def render_sprite(body: str, size: int) -> Image.Image:
    """Rasteryzuje SVG z delikatnym miękkim cieniem pod spodem (styl nowoczesnych kursorów)."""
    svg = (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" '
        f'viewBox="0 0 24 24">{body}</svg>'
    )
    png = cairosvg.svg2png(
        bytestring=svg.encode("utf-8"), output_width=size, output_height=size
    )
    with Image.open(io.BytesIO(png)) as image:
        fg = image.convert("RGBA")

    # Miękki cień wyprowadzony z kanału alfa kształtu, bez wychodzenia poza kadr.
    alpha = fg.getchannel("A")
    blur_r = max(0.7, size / 28.0)
    dx = max(0, round(size * 0.02))
    dy = max(1, round(size * 0.04))
    shadow_alpha = alpha.point(lambda a: int(a * 0.34)).filter(
        ImageFilter.GaussianBlur(radius=blur_r)
    )
    shadow = Image.new("RGBA", fg.size, (10, 14, 22, 0))
    shadow.putalpha(shadow_alpha)

    canvas = Image.new("RGBA", fg.size, (0, 0, 0, 0))
    canvas.alpha_composite(shadow, dest=(dx, dy))
    canvas.alpha_composite(fg)
    return canvas


def image_chunk(body: str, nominal: int, hotspot: tuple[float, float]) -> bytes:
    rgba = render_sprite(body, nominal)
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
        sprite = render_sprite(CURSORS[name][0], icon)
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
