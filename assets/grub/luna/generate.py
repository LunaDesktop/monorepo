#!/usr/bin/env python3
"""Generuje motyw GRUB „Luna” dla PaganOS.

Wszystkie pliki (tło, 9-slice podświetlenia i terminala oraz fonty PF2) powstają
tutaj, więc motyw da się odtworzyć bez ręcznego retuszowania. Wymaga Pillow oraz
(opcjonalnie) `grub-mkfont`; bez niego zostają już wygenerowane pliki .pf2.

Uruchomienie:
    python3 assets/grub/luna/generate.py
"""

from __future__ import annotations

import random
import shutil
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
W, H = 1920, 1080

# Paleta z brandingu Luny (półksiężyc + złota gwiazda).
MOON_LIGHT = (216, 204, 240)
MOON_DARK = (181, 213, 241)
MOON_LINE = (123, 103, 171)
GOLD = (243, 210, 124)
SKY_TOP = (6, 9, 20)
SKY_BOTTOM = (16, 20, 42)
PANEL_FILL = (10, 14, 28, 196)
PANEL_LINE = (108, 123, 184, 126)
TEXT = (236, 239, 250)
DIM = (150, 160, 192)

FONT_DIR = Path("/usr/share/fonts/dejavu")


def mix(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def font(size: int, *, bold: bool = False) -> ImageFont.FreeTypeFont:
    name = "DejaVuSans-Bold.ttf" if bold else "DejaVuSans.ttf"
    return ImageFont.truetype(str(FONT_DIR / name), size)


def sky() -> Image.Image:
    img = Image.new("RGB", (W, H))
    draw = ImageDraw.Draw(img)
    for y in range(H):
        draw.line([(0, y), (W, y)], fill=mix(SKY_TOP, SKY_BOTTOM, y / (H - 1)))

    # Miękkie mgławice — dwie plamy w kolorach akcentów Luny.
    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    g.ellipse((1040, -120, 1900, 620), fill=(126, 104, 196, 92))
    g.ellipse((-280, 620, 780, 1300), fill=(42, 79, 138, 84))
    g.ellipse((1120, 700, 1760, 1180), fill=(90, 120, 200, 46))
    glow = glow.filter(ImageFilter.GaussianBlur(120))
    img = Image.alpha_composite(img.convert("RGBA"), glow).convert("RGB")
    return img


def stars(img: Image.Image) -> None:
    draw = ImageDraw.Draw(img, "RGBA")
    rng = random.Random(0x4C554E41)  # „LUNA” — deterministyczne gwiazdy.
    for _ in range(240):
        x = rng.uniform(0, W)
        y = rng.uniform(0, 900)
        r = rng.choice([0.6, 0.8, 1.0, 1.2, 1.8])
        a = rng.randint(50, 190)
        draw.ellipse((x - r, y - r, x + r, y + r), fill=(232, 236, 255, a))

    # Kilka złotych iskier (czteroramienne) wokół półksiężyca.
    for x, y, size, alpha in [
        (1668, 214, 16, 235),
        (1772, 420, 9, 170),
        (1288, 176, 7, 140),
        (1590, 610, 6, 120),
    ]:
        draw.line((x - size, y, x + size, y), fill=(*GOLD, alpha), width=2)
        draw.line((x, y - size, x, y + size), fill=(*GOLD, alpha), width=2)
        draw.ellipse((x - 2, y - 2, x + 2, y + 2), fill=(*GOLD, 255))


def crescent(img: Image.Image) -> None:
    # Półksiężyc: duże koło minus mniejsze, przesunięte w prawo-górę.
    cx, cy, r = 1510, 360, 200
    mask = Image.new("L", (W, H), 0)
    m = ImageDraw.Draw(mask)
    m.ellipse((cx - r, cy - r, cx + r, cy + r), fill=255)
    m.ellipse((cx - r + 86, cy - r - 30, cx + r + 86, cy + r - 30), fill=0)

    glow = mask.filter(ImageFilter.GaussianBlur(46))
    glow_rgba = Image.new("RGBA", (W, H), (*MOON_DARK, 0))
    glow_rgba.putalpha(glow.point(lambda v: int(v * 0.55)))
    img.alpha_composite(glow_rgba)

    body = Image.new("RGBA", (W, H), (*MOON_LIGHT, 255))
    body.putalpha(mask)
    img.alpha_composite(body)

    line = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    l = ImageDraw.Draw(line)
    # Delikatna orbita wokół półksiężyca (kolor obrysu z brandingu).
    l.ellipse((cx - r - 34, cy - r - 34, cx + r + 34, cy + r + 34), outline=(*MOON_LINE, 80), width=2)
    l.ellipse((cx + r + 22, cy - 78, cx + r + 30, cy - 70), fill=(*GOLD, 210))
    img.alpha_composite(line)


def hills(img: Image.Image) -> None:
    draw = ImageDraw.Draw(img, "RGBA")
    back = [
        (0, 840), (180, 780), (360, 830), (560, 760), (760, 820),
        (980, 748), (1200, 810), (1430, 740), (1680, 800), (1920, 750),
        (1920, 1080), (0, 1080),
    ]
    front = [
        (0, 940), (250, 880), (470, 930), (700, 870), (930, 925),
        (1180, 862), (1420, 918), (1660, 858), (1920, 905),
        (1920, 1080), (0, 1080),
    ]
    draw.polygon(back, fill=(24, 32, 62, 255))
    draw.polygon(front, fill=(7, 10, 22, 255))

    # Delikatna linia horyzontu.
    draw.line([(0, 900), (1920, 900)], fill=(94, 111, 176, 70), width=1)


def panel(img: Image.Image) -> None:
    layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    d.rounded_rectangle((110, 300, 930, 900), radius=28, fill=PANEL_FILL, outline=PANEL_LINE, width=2)

    # Pionowy akcent przy panelu: złoto → fiolet → błękit.
    stops = [GOLD, (142, 123, 208), MOON_DARK]
    for y in range(600):
        t = y / 599
        c = mix(stops[0], stops[1], t * 2) if t < 0.5 else mix(stops[1], stops[2], (t - 0.5) * 2)
        d.line([(110, 300 + y), (115, 300 + y)], fill=(*c, 255))
    img.alpha_composite(layer)


def wordmark(img: Image.Image) -> None:
    draw = ImageDraw.Draw(img, "RGBA")
    draw.text((150, 160), "PaganOS", font=font(66, bold=True), fill=(*TEXT, 255))
    # Rozstrzelony podtytuł — rysowany znak po znaku (GRUB nie ma letter-spacingu).
    sub = "L U N A   ·   L I N U X   ·   W A Y L A N D"
    draw.text((154, 250), sub, font=font(19), fill=(*MOON_DARK, 220))
    # Cienka linia oddzielająca znak firmowy od menu.
    draw.line([(154, 286), (886, 286)], fill=(*MOON_LINE, 120), width=1)


def background() -> Image.Image:
    base = sky()
    rgba = base.convert("RGBA")
    stars(rgba)
    crescent(rgba)
    hills(rgba)
    panel(rgba)
    wordmark(rgba)

    # Lekka winieta, żeby menu i napisy były czytelne na każdym monitorze.
    vignette = Image.new("L", (W, H), 0)
    v = ImageDraw.Draw(vignette)
    for i in range(120):
        alpha = int(70 * (i / 119) ** 2)
        v.ellipse((-i * 6, -i * 6, W + i * 6, H + i * 6), outline=alpha, width=8)
    shade = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    shade.putalpha(vignette)
    rgba = Image.alpha_composite(rgba, shade)
    return rgba.convert("RGB")


def nine_slice(src: Image.Image, prefix: str, corner: int = 18) -> None:
    w, h = src.size
    boxes = {
        "nw": (0, 0, corner, corner),
        "n": (corner, 0, w - corner, corner),
        "ne": (w - corner, 0, w, corner),
        "w": (0, corner, corner, h - corner),
        "c": (corner, corner, w - corner, h - corner),
        "e": (w - corner, corner, w, h - corner),
        "sw": (0, h - corner, corner, h),
        "s": (corner, h - corner, w - corner, h),
        "se": (w - corner, h - corner, w, h),
    }
    for name, box in boxes.items():
        src.crop(box).save(HERE / f"{prefix}_{name}.png")


def select_pixmaps() -> None:
    w, h, r = 200, 44, 14
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for x in range(w):
        t = x / (w - 1)
        c = mix((142, 123, 208), (90, 169, 255), t)
        d.line([(x, 0), (x, h - 1)], fill=(*c, 70))
    mask = Image.new("L", (w, h), 0)
    ImageDraw.Draw(mask).rounded_rectangle((1, 1, w - 2, h - 2), radius=r, fill=255)
    img.putalpha(mask.point(lambda v: int(v * 0.62)))
    d.rounded_rectangle((1, 1, w - 2, h - 2), radius=r, outline=(*MOON_LIGHT, 190), width=2)
    d.rounded_rectangle((6, 8, 8, h - 9), radius=1, fill=(*GOLD, 235))
    nine_slice(img, "select")


def terminal_pixmaps() -> None:
    size, r = 240, 14
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((1, 1, size - 2, size - 2), radius=r, fill=(10, 14, 28, 244))
    d.rounded_rectangle((1, 1, size - 2, size - 2), radius=r, outline=(93, 107, 166, 190), width=2)
    nine_slice(img, "terminal")


def fonts() -> None:
    grub_mkfont = shutil.which("grub-mkfont")
    if not grub_mkfont:
        print("grub-mkfont: pominięto (użyję istniejących .pf2)")
        return
    faces = [
        ("dejavu_16.pf2", "DejaVuSans.ttf", 16),
        ("dejavu_18.pf2", "DejaVuSans.ttf", 18),
        ("dejavu_bold_18.pf2", "DejaVuSans-Bold.ttf", 18),
        ("dejavu_mono_14.pf2", "DejaVuSansMono.ttf", 14),
    ]
    for out, face, size in faces:
        subprocess.run(
            [grub_mkfont, "-s", str(size), "-o", str(HERE / out), str(FONT_DIR / face)],
            check=True,
        )


def main() -> None:
    background().save(HERE / "background.png", optimize=True)
    select_pixmaps()
    terminal_pixmaps()
    fonts()
    print(f"motyw GRUB: {HERE}")


if __name__ == "__main__":
    main()
