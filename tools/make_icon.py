"""Draws the AluKeys icon: an aluminium keycap with the ⌘ symbol on a blue tile.

Writes crates/alukeys/assets/icon-<size>.png (embedded by build.rs), alukeys.ico and a
512 px preview. The ⌘ symbol is constructed from lines and arcs (no fonts involved).

    python3 tools/make_icon.py
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

OUT = Path(__file__).resolve().parent.parent / "crates" / "alukeys" / "assets"
SIZES = [16, 20, 24, 32, 40, 48, 64, 256]
S = 1024  # drawing resolution


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def vertical_gradient(w, h, top, bottom):
    g = Image.new("RGBA", (w, h))
    px = g.load()
    for y in range(h):
        c = lerp(top, bottom, y / max(1, h - 1)) + (255,)
        for x in range(w):
            px[x, y] = c
    return g


def rounded_mask(w, h, r, box=None):
    m = Image.new("L", (w, h), 0)
    ImageDraw.Draw(m).rounded_rectangle(box or (0, 0, w - 1, h - 1), r, fill=255)
    return m


def command_symbol(draw, cx, cy, d, r, width, fill):
    """⌘: a square of half-side d whose sides run on into four 3/4 loops of radius r."""
    a, b = cx - d, cx + d
    t, u = cy - d, cy + d
    # Sides, extended to the loops' tangent points.
    draw.line([(a, t - r), (a, u + r)], fill=fill, width=width)
    draw.line([(b, t - r), (b, u + r)], fill=fill, width=width)
    draw.line([(a - r, t), (b + r, t)], fill=fill, width=width)
    draw.line([(a - r, u), (b + r, u)], fill=fill, width=width)
    # Loops: each skips the quadrant that faces the square.
    for (ox, oy, start, end) in [
        (a - r, t - r, 90, 360),
        (b + r, t - r, 180, 450),
        (a - r, u + r, 0, 270),
        (b + r, u + r, 270, 540),
    ]:
        # PIL strokes arcs inward from the box, so grow it to centre the stroke on r.
        e = r + width / 2
        draw.arc([ox - e, oy - e, ox + e, oy + e], start, end, fill=fill, width=width)


def render(detail):
    """Full icon at S×S. `detail` False gives a bolder version for 16–24 px."""
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

    # Blue tile.
    pad = 40 if detail else 0
    tile = vertical_gradient(S, S, (0x2F, 0x8C, 0xFF), (0x0B, 0x3F, 0xB5))
    img.paste(tile, (0, 0), rounded_mask(S, S, 230, (pad, pad, S - 1 - pad, S - 1 - pad)))

    # Keycap shadow.
    kp = 170 if detail else 110
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle((kp, kp + 40, S - kp, S - kp + 40), 120, fill=(0, 20, 60, 140))
    img.alpha_composite(shadow.filter(ImageFilter.GaussianBlur(30)))

    # Aluminium keycap: body, then a slightly lighter dished top.
    body = vertical_gradient(S, S, (0xF4, 0xF6, 0xF8), (0xB9, 0xC0, 0xC8))
    img.paste(body, (0, 0), rounded_mask(S, S, 120, (kp, kp, S - kp, S - kp)))
    inset = 34 if detail else 24
    top = vertical_gradient(S, S, (0xFF, 0xFF, 0xFF), (0xDD, 0xE2, 0xE8))
    img.paste(top, (0, 0), rounded_mask(S, S, 96, (kp + inset, kp + inset, S - kp - inset, S - kp - inset * 2)))

    # ⌘ in graphite.
    draw = ImageDraw.Draw(img)
    cy = S // 2 - inset // 2
    if detail:
        command_symbol(draw, S // 2, cy, d=78, r=62, width=42, fill=(0x2A, 0x2F, 0x36, 255))
    else:
        command_symbol(draw, S // 2, cy, d=96, r=80, width=70, fill=(0x1F, 0x24, 0x2B, 255))
    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    big, small = render(True), render(False)
    images = []
    for size in SIZES:
        src = small if size <= 24 else big
        im = src.resize((size, size), Image.LANCZOS)
        im.save(OUT / f"icon-{size}.png")
        images.append(im)
    big.resize((512, 512), Image.LANCZOS).save(OUT / "icon-preview.png")
    images[-1].save(OUT / "alukeys.ico", sizes=[(s, s) for s in SIZES])
    print("written to", OUT)


if __name__ == "__main__":
    main()
