#!/usr/bin/env python3
"""Generates the application icon.

Kept in the repository so the logo stays editable rather than being a binary nobody can
change. Run it, then regenerate the platform icon set:

    python tools/icon/generate.py
    npx tauri icon src-tauri/icons/source.png

Design notes
------------
The mark is the five colours of Magic laid out as a pentagon — white at the top, then blue,
black, red and green clockwise, which is the canonical colour wheel — with an upward chevron
in the middle for the optimiser.

Two constraints drove this. It has to survive 32px in a taskbar, which rules out a smooth
gradient: five *discrete* dots stay readable when everything else turns to mush. And it has to
be nothing but geometry and colour, using no Wizards of the Coast asset and imitating none,
which matters for a public repository (see the legal section of the README).

Everything is drawn at 4x and downsampled, because crisp edges at small sizes come from
supersampling rather than from clever drawing.
"""

import math

from PIL import Image, ImageDraw, ImageFilter

SIZE = 512
SS = 4  # supersampling factor
S = SIZE * SS

PANEL_TOP = (38, 43, 60)
PANEL_BOTTOM = (13, 15, 22)

# The five colours, clockwise from the top: white, blue, black, red, green.
# "Black" is rendered as a cool violet — an actually black dot would vanish on a dark panel,
# and this is how black mana is usually shown on dark interfaces anyway.
WUBRG = [
    (243, 234, 206),
    (62, 143, 232),
    (138, 122, 194),
    (228, 87, 61),
    (67, 170, 106),
]

CHEVRON = (236, 240, 249)


def lerp(a, b, t):
    return tuple(round(x + (y - x) * t) for x, y in zip(a, b))


def vertical_gradient(size, top, bottom):
    width, height = size
    image = Image.new("RGB", size)
    draw = ImageDraw.Draw(image)
    for y in range(height):
        draw.line([(0, y), (width, y)], fill=lerp(top, bottom, y / max(height - 1, 1)))
    return image


def rounded_mask(size, radius):
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size[0] - 1, size[1] - 1], radius, fill=255)
    return mask


def clipped(layer, radius):
    """Keeps only the part of `layer` inside the panel's rounded rectangle."""
    empty = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    return Image.composite(layer, empty, rounded_mask((S, S), radius))


def pentagon_points(centre, radius, count=5):
    """Vertices of a regular polygon, first one straight up."""
    cx, cy = centre
    return [
        (
            cx + radius * math.sin(2 * math.pi * i / count),
            cy - radius * math.cos(2 * math.pi * i / count),
        )
        for i in range(count)
    ]


def build():
    panel_radius = round(S * 0.185)
    centre = (S / 2, S / 2)

    panel = vertical_gradient((S, S), PANEL_TOP, PANEL_BOTTOM).convert("RGBA")
    panel.putalpha(rounded_mask((S, S), panel_radius))

    ring_radius = S * 0.285
    dot_radius = S * 0.113
    points = pentagon_points(centre, ring_radius)

    # Each dot gets a bloom in its own colour, which is what stops five flat circles from
    # looking like a chart legend.
    glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow)
    for (x, y), colour in zip(points, WUBRG):
        r = dot_radius * 1.75
        glow_draw.ellipse([x - r, y - r, x + r, y + r], fill=(*colour, 62))
    glow = glow.filter(ImageFilter.GaussianBlur(S * 0.055))
    panel = Image.alpha_composite(panel, clipped(glow, panel_radius))

    # Faint connecting ring, so the five dots read as one mark.
    ring = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    ImageDraw.Draw(ring).ellipse(
        [
            centre[0] - ring_radius,
            centre[1] - ring_radius,
            centre[0] + ring_radius,
            centre[1] + ring_radius,
        ],
        outline=(255, 255, 255, 26),
        width=round(S * 0.011),
    )
    panel = Image.alpha_composite(panel, ring)

    # Flat dots. An earlier version added a lighter ellipse on top for volume; at any real
    # icon size it read as a dent rather than a highlight, so the shape carries it instead.
    dots = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    dots_draw = ImageDraw.Draw(dots)
    for (x, y), colour in zip(points, WUBRG):
        dots_draw.ellipse(
            [x - dot_radius, y - dot_radius, x + dot_radius, y + dot_radius],
            fill=(*colour, 255),
        )

    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shadow.paste((0, 0, 0, 130), (0, 0), dots.split()[3])
    shadow = shadow.filter(ImageFilter.GaussianBlur(S * 0.018))
    offset = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    offset.paste(shadow, (0, round(S * 0.010)), shadow)
    panel = Image.alpha_composite(panel, clipped(offset, panel_radius))
    panel = Image.alpha_composite(panel, dots)

    # The optimiser: one chevron pointing up, in the middle of the wheel. A single stroke
    # rather than a stack of two — at 32px the second one only added mush.
    chevron = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    chevron_draw = ImageDraw.Draw(chevron)
    half_width = S * 0.082
    height = S * 0.070
    stroke = round(S * 0.044)
    apex_y = centre[1] - height * 0.5
    chevron_draw.line(
        [
            (centre[0] - half_width, apex_y + height),
            (centre[0], apex_y),
            (centre[0] + half_width, apex_y + height),
        ],
        fill=(*CHEVRON, 255),
        width=stroke,
        joint="curve",
    )
    # Round the stroke ends by hand: PIL caps lines square and joints do not help at the ends.
    for end_x in (centre[0] - half_width, centre[0] + half_width):
        r = stroke / 2
        chevron_draw.ellipse(
            [end_x - r, apex_y + height - r, end_x + r, apex_y + height + r],
            fill=(*CHEVRON, 255),
        )

    return Image.alpha_composite(panel, chevron).resize((SIZE, SIZE), Image.LANCZOS)


if __name__ == "__main__":
    import pathlib

    out = pathlib.Path(__file__).resolve().parents[2] / "src-tauri" / "icons" / "source.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    build().save(out)
    print(f"wrote {out}")
