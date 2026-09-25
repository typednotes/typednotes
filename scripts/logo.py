#!/usr/bin/env python3
"""Generate the Typednotes logo: the "T" of www.typednotes.com's title, with
a soft shadow drawn in Lean 4 symbols.

The site draws its title in the figlet font ANSI Shadow on a character grid:
solid `█` cells with a shadow one step down and to the right. The logo keeps
the solid T and replaces the shadow with a soft one: the T shifted down and
right (OFFSET), blurred over a radius of RADIUS cells. Softness is ASCII-art
softness, twice over — the further from the shifted T, the fainter the grey
and the lighter the symbol, from dense (ℕ Σ Π ∃ …) through medium (λ → ⊢ …)
to sparse (· : ¬ …). Distances are measured in pixels on the site's cell
(0.6 em × 1.25 em), so the blur is round, not squashed. Transparent
background:

  logo-dark.svg   for dark backgrounds  (light glyph, the site's own greys)
  logo-light.svg  for light backgrounds (dark glyph)
  logo.svg        both, switched by prefers-color-scheme

and the same three as `mark-*.svg`: the variant for small sizes (the app's
navbar pill, 28 px), with only the hard shadow band, since a
soft edge is noise at that size.

Symbols are outlined from Fira Code (fontTools), so the SVGs need no font.
PNGs (logo 1024 px; mark 28, 56 and 84 px: 1x, 2x, 3x) come from rsvg-convert:

  scripts/logo.py              # LOGO_FONT=… to use another .ttf
"""

import math
import os
import random
import subprocess
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

# The T's solid cells, from the ANSI Shadow glyph on the site.
T = [
    "████████",
    "   ██   ",
    "   ██   ",
    "   ██   ",
    "   ██   ",
]

CW, CH = 12.0, 25.0             # the site's cell at 20 px: 0.6 em wide, 1.25 em tall
FS = 20.0                       # symbol size (Fira Code's advance is 0.6 em: one cell)

# The shadow: the T moved OFFSET = (columns, rows) down and right — two narrow
# columns are about one tall row, so it falls as far right as down — then
# blurred over RADIUS rows' worth of distance.
OFFSET = (2, 1)
RADIUS = 2

# One band per step of distance from the shifted T: 0 is under it (the hard
# shadow), 1 and 2 its soft edge. Each band's symbols, densest first, like an
# ASCII-art brightness ramp, and its grey.
BANDS = [
    "ℕℤΣΠ∃∀≡",
    "λαβ→↔×=∧∨⊢",
    "·:¬,⟨⟩'",
]
PALETTES = {
    # Dark: the site's #f2f2f2 blocks and #7a7a7a shadow, fading out.
    "dark": dict(block="#f2f2f2", band0="#7a7a7a", band1="#545454", band2="#353535"),
    "light": dict(block="#111111", band0="#8c8c8c", band1="#b0b0b0", band2="#d2d2d2"),
}
SEED = 7                        # a fixed arrangement; never the same symbol twice side by side

SOLID = {(r, c) for r, line in enumerate(T) for c, g in enumerate(line) if g == "█"}

# name -> (blur radius, margin in cells, PNG sizes)
VARIANTS = {
    "logo": (RADIUS, 1, [1024]),
    "mark": (0, 1, [28, 56, 84]),
}


def f(v: float) -> str:
    return f"{v:.2f}".rstrip("0").rstrip(".")


class Layout:
    """Where everything goes for one variant: each shadow cell's band, and
    the square viewBox the T and its shadow are centred in."""

    def __init__(self, radius: int, margin: int):
        self.bands = shadow_bands(radius)
        cells = SOLID | set(self.bands)
        r0, c0 = min(r for r, _ in cells), min(c for _, c in cells)
        r1, c1 = max(r for r, _ in cells), max(c for _, c in cells)
        w, h = (c1 - c0 + 1) * CW, (r1 - r0 + 1) * CH
        self.size = max(w, h) + 2 * margin * CW
        self.ox = (self.size - w) / 2 - c0 * CW
        self.oy = (self.size - h) / 2 - r0 * CH


def shadow_bands(radius: int) -> dict[tuple[int, int], int]:
    """Every shadow cell and its band: the pixel distance from the cell to the
    shifted T, in rows, rounded up — 0 under it. Cells the T covers are not
    shadow."""
    dc, dr = OFFSET
    shifted = {(r + dr, c + dc) for r, c in SOLID}
    reach_c = math.ceil(radius * CH / CW)
    rows = range(min(r for r, _ in shifted) - radius, max(r for r, _ in shifted) + radius + 1)
    cols = range(min(c for _, c in shifted) - reach_c, max(c for _, c in shifted) + reach_c + 1)
    bands = {}
    for r in rows:
        for c in cols:
            if (r, c) in SOLID:
                continue
            d = min(math.hypot((c - b) * CW, (r - a) * CH) for a, b in shifted) / CH
            band = math.ceil(d - 1e-9)
            if band <= radius:
                bands[(r, c)] = band
    return bands


def blocks(lay: Layout) -> str:
    """The T as few rects as possible: horizontal runs of █, merged down
    across rows that repeat them (the T is its bar and its stem). Each rect
    overlaps the one below by half a unit, as the site's fillRect does, so no
    seam shows where they meet."""
    rects: list[list[int]] = []                    # [first row, last row, first col, last col]
    for r, line in enumerate(T):
        c = 0
        while c < len(line):
            if line[c] != "█":
                c += 1
                continue
            start = c
            while c < len(line) and line[c] == "█":
                c += 1
            above = next((x for x in rects if x[1] == r - 1 and x[2:] == [start, c - 1]), None)
            if above:
                above[1] = r
            else:
                rects.append([r, r, start, c - 1])
    return "\n".join(
        f'<rect x="{f(lay.ox + c0 * CW)}" y="{f(lay.oy + r0 * CH)}" '
        f'width="{f((c1 - c0 + 1) * CW)}" height="{f((r1 - r0 + 1) * CH + 0.5)}"/>'
        for r0, r1, c0, c1 in rects
    )


def font_path() -> str:
    if os.environ.get("LOGO_FONT"):
        return os.environ["LOGO_FONT"]
    return subprocess.run(
        ["fc-match", "-f", "%{file}", "Fira Code"], capture_output=True, text=True, check=True
    ).stdout


def shadow(lay: Layout) -> dict[int, str]:
    """The shadow's symbols as one outlined path per band, each centred in its
    cell (on its cap height, where the site's `textBaseline = 'middle'` puts it).
    Symbols are drawn in the same order for every variant, so the mark's are
    the logo's."""
    font = TTFont(font_path())
    glyphs, cmap = font.getGlyphSet(), font.getBestCmap() or {}
    missing = [s for s in "".join(BANDS) if ord(s) not in cmap]
    if missing:
        raise SystemExit(f"{font_path()} has no glyph for {''.join(missing)}; set LOGO_FONT")
    units = getattr(font["head"], "unitsPerEm")
    scale = FS / units
    cap = getattr(font["OS/2"], "sCapHeight", 0) or units * 0.7

    rng = random.Random(SEED)
    placed: dict[tuple[int, int], str] = {}
    paths: dict[int, list[str]] = {}
    for (r, c), band in sorted(shadow_bands(RADIUS).items()):
        neighbours = {placed.get((r, c - 1)), placed.get((r - 1, c))}
        ch = rng.choice([s for s in BANDS[band] if s not in neighbours] or BANDS[band])
        placed[(r, c)] = ch
        if (r, c) not in lay.bands:
            continue
        glyph = glyphs[cmap[ord(ch)]]
        x = lay.ox + c * CW + CW / 2 - glyph.width * scale / 2
        baseline = lay.oy + r * CH + CH / 2 + cap * scale / 2
        pen = SVGPathPen(glyphs, ntos=f)
        glyph.draw(TransformPen(pen, (scale, 0, 0, -scale, x, baseline)))
        paths.setdefault(band, []).append(pen.getCommands())
    # Faintest first, so a denser band is never painted over.
    return {band: "".join(paths[band]) for band in sorted(paths, reverse=True)}


def svg(lay: Layout, body: str, style: str = "") -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {f(lay.size)} {f(lay.size)}" '
        f'width="{f(lay.size)}" height="{f(lay.size)}" role="img" aria-label="Typednotes">\n'
        f"<title>Typednotes</title>\n{style}{body}</svg>\n"
    )


def body(lay: Layout, paint) -> str:
    """`paint(tone)` is the attribute that colours `block` or `band0`…`band2`."""
    parts = [f'<path {paint(f"band{band}")} d="{d}"/>' for band, d in shadow(lay).items()]
    parts.append(f'<g {paint("block")}>\n{blocks(lay)}\n</g>')
    return "\n".join(parts) + "\n"


def main() -> None:
    if len(BANDS) != RADIUS + 1:
        raise SystemExit(f"BANDS needs {RADIUS + 1} entries (bands 0…RADIUS)")
    out = Path(__file__).resolve().parent.parent / "packages/ui/assets/logo"
    out.mkdir(parents=True, exist_ok=True)

    def rules(p):
        return "".join(f".{t}{{fill:{v}}}" for t, v in p.items())

    style = (
        "<style>\n"
        f"{rules(PALETTES['light'])}\n"
        f"@media (prefers-color-scheme: dark){{{rules(PALETTES['dark'])}}}\n"
        "</style>\n"
    )
    for name, (radius, margin, sizes) in VARIANTS.items():
        lay = Layout(radius, margin)
        for mode, p in PALETTES.items():
            path = out / f"{name}-{mode}.svg"
            path.write_text(svg(lay, body(lay, lambda t: f'fill="{p[t]}"')))
            for px in sizes:
                png = out / (f"{name}-{mode}.png" if len(sizes) == 1 else f"{name}-{mode}-{px}.png")
                subprocess.run(["rsvg-convert", "-w", str(px), "-h", str(px), str(path), "-o", str(png)],
                               check=True)
        (out / f"{name}.svg").write_text(svg(lay, body(lay, lambda t: f'class="{t}"'), style))
        print(f"{name}: {name}.svg, {name}-light.svg, {name}-dark.svg, PNGs at {sizes} px")


if __name__ == "__main__":
    main()
