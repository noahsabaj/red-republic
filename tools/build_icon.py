"""Generate the application icon.

Run:

    python tools/build_icon.py

Writes `godot/icon.png` (the Godot application icon) and `godot/icon.ico` (the
Windows executable's icon resource, which `tools/package.ps1` points the export
preset at). Both are committed, because they are art rather than build output —
this script exists so the design is reproducible and adjustable, not so that
every build regenerates it. Nothing in CI runs it, and it deliberately has no
guard test asserting the committed files match: a permanent harness around a
one-off art artifact is more apparatus than content.

# The design

A red star on the dark survey plate, in the palette the game already uses —
`godot/ui/theme.gd`'s PAPER_RAISED, ACCENT and RULE. Chosen from four rendered
candidates because it is the only one that survives 16 px, which is the size that
actually decides an icon: the alternatives (surveyed contours, a works
silhouette, an RR monogram) all read beautifully at 256 and turn to mush in a
taskbar.

# Why each size is drawn rather than downsampled

Downsampling one 256 px master to 16 px keeps the star legible but muddies the
plate outline into a grey halo. So every size is drawn at 4x *its own*
dimensions and reduced from there, and two details are size-dependent: the
outline is dropped below 32 px, where a one-pixel border is noise rather than
structure, and the star grows slightly to hold its weight as the plate loses it.
"""

import math
import os

from PIL import Image, ImageDraw

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "godot")

# Straight off godot/ui/theme.gd. Kept as 0-255 triples rather than the 0-1
# floats the theme uses, because that is what PIL wants; the values are the same
# colours and the theme is the source.
PAPER_RAISED = (33, 41, 48)
ACCENT = (184, 61, 51)
RULE = (61, 71, 82)

# Every size Windows asks for, plus the 256 the Godot app icon uses.
SIZES = (16, 24, 32, 48, 64, 128, 256)

SS = 4  # supersample factor


def star_points(cx, cy, outer, inner, points=5, rot=-math.pi / 2):
    """A `points`-pointed star, first point straight up."""
    pts = []
    for i in range(points * 2):
        r = outer if i % 2 == 0 else inner
        a = rot + i * math.pi / points
        pts.append((cx + r * math.cos(a), cy + r * math.sin(a)))
    return pts


def render(size):
    """One icon at one size, drawn at 4x and reduced."""
    n = size * SS
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    radius = n * 0.133
    d.rounded_rectangle([0, 0, n - 1, n - 1], radius=radius, fill=PAPER_RAISED + (255,))

    # Below 32 px a border is a grey halo rather than an edge, so the star takes
    # over the job of holding the shape.
    outlined = size >= 32
    if outlined:
        d.rounded_rectangle(
            [0, 0, n - 1, n - 1], radius=radius, outline=RULE + (255,), width=max(1, int(n * 0.012))
        )

    outer = n * (0.33 if outlined else 0.36)
    d.polygon(star_points(n / 2, n / 2 + n * 0.015, outer, outer * 0.404), fill=ACCENT + (255,))

    return img.resize((size, size), Image.LANCZOS)


def main():
    frames = [render(size) for size in SIZES]
    by_size = dict(zip(SIZES, frames))

    by_size[256].save(os.path.join(OUT, "icon.png"))

    # An .ico is a container; PIL writes one entry per size from the frames it is
    # given. Handing it the 256 and asking for `sizes` would downsample, which is
    # the thing this script draws each size to avoid, so the frames are appended
    # explicitly.
    by_size[256].save(
        os.path.join(OUT, "icon.ico"),
        format="ICO",
        sizes=[(s, s) for s in SIZES],
        append_images=[by_size[s] for s in SIZES if s != 256],
    )

    print("wrote godot/icon.png and godot/icon.ico (%s)" % ", ".join(str(s) for s in SIZES))


if __name__ == "__main__":
    main()
