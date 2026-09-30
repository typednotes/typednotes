# Shared Typednotes logo

The mark is a T on its side: **⊢**, with a mathematical-symbol shadow shifted
down/right. Its viewBox is centered on the foreground character, with symmetric
room to retain the asymmetric shadow. The application's base logo uses:

- Foreground: black `#000000` on light backgrounds, white `#ffffff` on dark ones.
- Shadow: `#64748b`, with opacity 0.55 near the mark and progressively lower alpha
  toward its soft edge. No background color is baked into the shadow.

`logo` retains the full soft shadow. `mark` is the less-shadow variant. Each
module can override both RGB colors independently while keeping the alpha ramp.

The other projects use **fixed bicolor variants of the light logo**, retaining
their old palettes: ledger green/ochre, liaison teal/orange, lode indigo/magenta,
and lun violet/gold. Web-data and secrets use blue/orange. These variants do not
switch to black/white in dark mode. Their near-shadow band is opaque, and the
outer bands fade through alpha rather than background-blended RGB values.

The mathematical symbols and wordmarks are outlined from Fira Code, so exported
SVGs need no fonts on the viewer's machine. The app scales the complete
`packages/ui/assets/logo/logo.svg` inside a colored pill. The favicon references
the very same asset; no simplified plain-T icon or theme-specific logo is used.

## Generate variants

Requirements: Python 3, `fonttools`, Fira Code (or `LOGO_FONT`/`--font` pointing to a
TTF) and `rsvg-convert` for PNG output. The generator is `scripts/logo.py`.

```sh
# Canonical logo/mark SVGs and the main 1024 px PNG
python3 scripts/logo.py

# Extra PNG sizes, generated outside the canonical asset directory
python3 scripts/logo.py --sizes 16 28 32 56 84 128 256 512 --output-dir out/logos

# Independent foreground/shadow colors and alpha
python3 scripts/logo.py --mark-color '#386ee0' --shadow-color '#f59e0b' \
  --shadow-opacity 0.45 --sizes 64 256 --output-dir out/amber-shadow

# Explicit white foreground raster for a dark surface
python3 scripts/logo.py --theme dark --format png --sizes 32 256 --output-dir out/dark

# SVG only, a title and optional outlined wordmark
python3 scripts/logo.py --format svg --title 'My component' --wordmark 'component' \
  --output-dir out/component

# Refresh component variants with their own bicolor light-logo palettes
python3 scripts/logo.py --components-root ..

# Recolor selected modules only; shadow opacity is retained
python3 scripts/logo.py --components-root .. --components ledger liaison \
  --mark-color '#386ee0' --shadow-color '#f59e0b' --format svg
```

`--variant logo|mark|all` chooses the full-shadow logo or less-shadow mark. Sizes
scale the chosen geometry. `--sizes` is explicit and validates 1–8192 pixels.
`--mark-color`/`--shadow-color` accept independent hex RGB values, and alpha must be
finite in 0–1. Without a foreground override one SVG adapts black/white through
`prefers-color-scheme`; `--theme light|dark` fixes a theme for PNG exports.

Main SVGs and a main PNG are checked in. The generator replaces the old T for
ledger, liaison, lode, lun, web-data and secrets. Extra color/size variants are
generated on demand in the ignored `out/` directory.

Linen retains its woven mark and Infra retains its cube. The generator excludes
Linen, Infra and the infrastructure deployment repository.

Third-party cloud/provider trademarks are separate assets and are not recolored.
