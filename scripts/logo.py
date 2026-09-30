#!/usr/bin/env python3
"""Generate the shared Typednotes ⊢ logo, with an outlined mathematical shadow.

The base foreground is black on light backgrounds and white on dark backgrounds.
The shadow fades by alpha, never by mixing in an assumed background. One adaptive
SVG covers both themes. The app uses the full logo SVG at icon scale.

  python3 scripts/logo.py                         # canonical SVGs + one 1024 PNG
  python3 scripts/logo.py --format svg            # no rasterizer required
  python3 scripts/logo.py --sizes 28 56 84 256 --output-dir out/logos
  python3 scripts/logo.py --mark-color '#386ee0' --shadow-color '#64748b'
  python3 scripts/logo.py --components-root .. --format svg

Dependencies: fonttools, Fira Code (or --font/LOGO_FONT), and rsvg-convert for
PNG output. Extra sizes/colors belong in a generated output directory.
"""

import math
import argparse
import html
import os
import random
import re
import tempfile
import subprocess
from pathlib import Path

from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

# A T rotated onto its side: the logical turnstile, ⊢.
T = [
    "██      ",
    "██      ",
    "████████",
    "██      ",
    "██      ",
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
MARK_COLOR = None               # adaptive black/white; --mark-color overrides it
SHADOW_COLOR = "#64748b"
SHADOW_ALPHA = 0.55
ALPHA_RAMP = (1.0, 0.5, 0.2)
SEED = 7                        # a fixed arrangement; never the same symbol twice side by side

SOLID = {(r, c) for r, line in enumerate(T) for c, g in enumerate(line) if g == "█"}

# name -> (blur radius, margin in cells, PNG sizes)
VARIANTS = {
    "logo": (RADIUS, 1, [1024]),
    "mark": (0, 1, [1024]),
}

COMPONENT_TARGETS = {
    "ledger": ("logo.svg", ""), "liaison": ("logo.svg", ""),
    "lode": ("logo.svg", ""), "lun": ("logo.svg", ""),
    "web-data": ("logo.svg", "web-data"),
    "secrets": ("logo.svg", "secrets"),
}

# Fixed bicolor light-logo palettes retained from each project's previous T.
# These are module variants; only the application's base mark adapts to a theme.
COMPONENT_COLORS = {
    "ledger": ("#1f5c3f", "#b8892b"),
    "liaison": ("#0e6b6f", "#e0763a"),
    "lode": ("#3730a3", "#c2417a"),
    "lun": ("#3b2f7a", "#e0a526"),
    "web-data": ("#1d4ed8", "#ea580c"),
    "secrets": ("#1d4ed8", "#ea580c"),
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
        # Center on the foreground character, not its asymmetric shadow.
        sr0, sc0 = min(r for r, _ in SOLID), min(c for _, c in SOLID)
        sr1, sc1 = max(r for r, _ in SOLID), max(c for _, c in SOLID)
        center_x, center_y = (sc0 + sc1 + 1) * CW / 2, (sr0 + sr1 + 1) * CH / 2
        extent = max(center_x - c0 * CW, (c1 + 1) * CW - center_x,
                     center_y - r0 * CH, (r1 + 1) * CH - center_y)
        self.size = 2 * (extent + margin * CW)
        self.ox = self.size / 2 - center_x
        self.oy = self.size / 2 - center_y


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
        f'width="{f((c1 - c0 + 1) * CW)}" height="{f((r1 - r0 + 1) * CH + (0.5 if r1 < max(r for r, _ in SOLID) else 0))}"/>'
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
        # Decompose first: some symbols are other glyphs transformed (Fira
        # Code's ∀ is its A flipped), and a component's transform must apply
        # before the flip into SVG's downward y, not be composed with it.
        outline = DecomposingRecordingPen(glyphs)
        glyph.draw(outline)
        pen = SVGPathPen(glyphs, ntos=f)
        outline.replay(TransformPen(pen, (scale, 0, 0, -scale, x, baseline)))
        paths.setdefault(band, []).append(pen.getCommands())
    # Faintest first, so a denser band is never painted over.
    return {band: "".join(paths[band]) for band in sorted(paths, reverse=True)}


def svg(lay: Layout, body: str, title: str = "Typednotes", wordmark: str = "", color: str | None = MARK_COLOR, theme: str = "auto") -> str:
    width = lay.size
    if wordmark:
        font = TTFont(font_path())
        glyphs, cmap = font.getGlyphSet(), font.getBestCmap() or {}
        units = getattr(font["head"], "unitsPerEm")
        scale = 42 / units
        x = lay.size + 12
        paths = []
        cap = getattr(font["OS/2"], "sCapHeight", 0) or units * .7
        baseline = lay.size / 2 + cap * scale / 2
        for character in wordmark:
            if ord(character) not in cmap:
                raise ValueError(f"Font has no glyph for {character!r}")
            glyph = glyphs[cmap[ord(character)]]
            outline = DecomposingRecordingPen(glyphs)
            glyph.draw(outline)
            pen = SVGPathPen(glyphs, ntos=f)
            outline.replay(TransformPen(pen, (scale, 0, 0, -scale, x, baseline)))
            paths.append(pen.getCommands())
            x += glyph.width * scale
        width = x + 12
        body += f'<path data-layer="wordmark" class="foreground" fill="{color or "#000000"}" d="{"".join(paths)}"/>\n'
    if color:
        style = ""
    elif theme == "auto":
        style = '<style>.foreground{fill:#000000}@media(prefers-color-scheme:dark){.foreground{fill:#ffffff}}</style>\n'
    else:
        style = f'<style>.foreground{{fill:{"#ffffff" if theme == "dark" else "#000000"}}}</style>\n'
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {f(width)} {f(lay.size)}" '
        f'width="{f(width)}" height="{f(lay.size)}" role="img" aria-label="{html.escape(title, quote=True)}">\n'
        f"<title>{html.escape(title)}</title>\n{style}{body}</svg>\n"
    )


def body(lay: Layout, color: str | None, shadow_color: str, alpha: float) -> str:
    parts = [f'<path data-layer="shadow" fill="{shadow_color}" fill-opacity="{f(alpha * ALPHA_RAMP[band])}" d="{d}"/>'
             for band, d in shadow(lay).items()]
    parts.append(f'<g data-layer="mark" class="foreground" fill="{color or "#000000"}">\n{blocks(lay)}\n</g>')
    return "\n".join(parts) + "\n"


def color_arg(value: str) -> str:
    if not re.fullmatch(r"#[0-9a-fA-F]{3}(?:[0-9a-fA-F]{3})?", value):
        raise argparse.ArgumentTypeError("use a #RGB or #RRGGBB color")
    return value.lower()


def size_arg(value: str) -> int:
    try:
        size = int(value)
    except ValueError as error:
        raise argparse.ArgumentTypeError("sizes must be integers") from error
    if not 1 <= size <= 8192:
        raise argparse.ArgumentTypeError("sizes must be between 1 and 8192 pixels")
    return size


def render_png(source: Path, target: Path, size: int) -> None:
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), str(source), "-o", str(target)], check=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--mark-color", type=color_arg, default=MARK_COLOR)
    parser.add_argument("--theme", choices=["auto", "light", "dark"], default="auto", help="SVG foreground theme; PNG defaults to light unless dark is explicit")
    parser.add_argument("--shadow-color", type=color_arg, help="override the base or component shadow RGB")
    parser.add_argument("--shadow-opacity", type=float, help="override alpha (base .55; component light logos 1.0)")
    parser.add_argument("--font", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path(__file__).resolve().parent.parent / "packages/ui/assets/logo")
    parser.add_argument("--format", choices=["svg", "png", "both"], default="both")
    parser.add_argument("--sizes", nargs="+", type=size_arg, default=[1024])
    parser.add_argument("--variant", choices=["logo", "mark", "all"], default="all")
    parser.add_argument("--title", default="Typednotes")
    parser.add_argument("--wordmark", default="")
    parser.add_argument("--components-root", type=Path, help="regenerate canonical first-party component SVGs in sibling repositories")
    parser.add_argument("--components", nargs="+", choices=list(COMPONENT_TARGETS), help="limit component regeneration to these repository names")
    args = parser.parse_args()
    if args.components and not args.components_root:
        parser.error("--components requires --components-root")
    if args.components_root:
        for component in (args.components or COMPONENT_TARGETS):
            if not (args.components_root / component).is_dir():
                parser.error(f"missing component repository: {args.components_root / component}")
    if args.shadow_opacity is not None and (not math.isfinite(args.shadow_opacity) or not 0 <= args.shadow_opacity <= 1):
        parser.error("shadow opacity must be between 0 and 1")
    if args.font:
        os.environ["LOGO_FONT"] = str(args.font)
    if len(BANDS) != RADIUS + 1:
        raise SystemExit(f"BANDS needs {RADIUS + 1} entries (bands 0…RADIUS)")
    out = args.output_dir
    shadow_color = args.shadow_color or SHADOW_COLOR
    shadow_opacity = SHADOW_ALPHA if args.shadow_opacity is None else args.shadow_opacity
    out.mkdir(parents=True, exist_ok=True)
    for name, (radius, margin, sizes) in VARIANTS.items():
        if args.variant != "all" and args.variant != name:
            continue
        lay = Layout(radius, margin)
        path = out / f"{name}.svg"
        artwork = svg(lay, body(lay, args.mark_color, shadow_color, shadow_opacity), args.title, args.wordmark, args.mark_color, args.theme)
        if args.format != "png":
            path.write_text(artwork)
        # Canonical defaults retain just one main raster; custom sizes can
        # request both names explicitly without adding theme-specific assets.
        if args.format != "svg" and (name == "logo" or args.variant == "mark" or args.sizes != [1024] or args.format == "png"):
            with tempfile.NamedTemporaryFile(suffix=".svg") as source:
                source.write(artwork.encode()); source.flush()
                for size in args.sizes:
                    target = out / (f"{name}.png" if args.sizes == [1024] else f"{name}-{size}.png")
                    render_png(Path(source.name), target, size)
                    print(target)
        if args.format != "png":
            print(path)
    if args.components_root:
        root = args.components_root.resolve()
        lay = Layout(RADIUS, 1)
        targets = COMPONENT_TARGETS
        if args.components:
            targets = {name: targets[name] for name in args.components}
        for component, (filename, label) in targets.items():
            directory = root / component
            if not directory.is_dir():
                raise SystemExit(f"Missing component repository: {directory}")
            target = directory / filename
            target.parent.mkdir(parents=True, exist_ok=True)
            foreground, shadow = COMPONENT_COLORS[component]
            foreground = args.mark_color or foreground
            shadow = args.shadow_color or shadow
            alpha = 1.0 if args.shadow_opacity is None else args.shadow_opacity
            artwork = svg(lay, body(lay, foreground, shadow, alpha), component, label, foreground, "light")
            target.write_text(artwork)
            print(target)


if __name__ == "__main__":
    main()
