# dwg2png

Pure-Rust **DWG &rarr; multipage PNG** renderer and **metadata indexer** for a
SaaS backend that indexes DWG files. Parses native DWG (R13&ndash;R2018) with
[`acadrust`](https://github.com/hakanaktt/acadrust) (MPL-2.0, no C deps), renders
each layout (Model space + paper-space sheets) to PNG with
[`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) on the CPU, and extracts a
searchable index document (layers, blocks, title-block attributes, text,
layouts) as JSON.

No GPL (LibreDWG avoided), no FFI, no GPU, no external CAD engine.

## Download

Prebuilt binaries for macOS (Intel + Apple Silicon), Linux, and Windows are
attached to each [release](https://github.com/diskd-ai/dwg2png/releases).
Download the archive for your platform, extract `dwg2png`, and run it.

```sh
# macOS (Apple Silicon) example
tar -xzf dwg2png-aarch64-apple-darwin.tar.gz
./dwg2png --help
```

Or build from source: `cargo build --release` (Rust stable).

Releases are produced automatically by `.github/workflows/release.yml` when a
`v*` tag is pushed.

## Usage

```sh
# Render + index a file or a whole directory tree of .dwg files
dwg2png <file-or-dir> [more...] --out out [--size PX] [--tiles] [--font TTF] [--no-compare]

# Recommended for VLM ingestion: high-res + readable tiles + text layer
dwg2png drawings/ --out out --size 6000 --tiles
```

Flags: `--size` = long-edge pixels (default 3000, up to 12000). `--tiles` =
also emit ~1500 px readable crops (`--tile-size N` to change). `--font` defaults
to `/System/Library/Fonts/Supplemental/Arial.ttf` (Cyrillic); override via
`DWG2PNG_FONT`. When a sibling PDF matches a DWG name and `pdftoppm` is on
`PATH`, the report shows a side-by-side against the plotted PDF.

Outputs under `--out`:

- `img/fNN-pMM.png` &mdash; one PNG per page (`p00` = Model space, then each paper sheet)
- `labels/fNN-pMM.json` &mdash; **position-referenced text layer**: every label with exact text, pixel `x,y,w,h`, screen rotation, color, and layer
- `tiles/fNN-pMM/rRcC.png` + `tiles.json` &mdash; readable crops with per-tile labels (when `--tiles`)
- `json/fNN.json` &mdash; the searchable `IndexDoc` per file
- `report.html` &mdash; self-contained report: thumbnails, a **toggleable SVG text overlay**, ground-truth comparisons, and the tile crops

### Reading all labels (VLM / search)

Tiny CAD labels are sub-pixel against a large drawing extent, and VLMs downscale
images on input &mdash; so no single raster makes every label legible. Two
complementary channels solve this:

1. **Text layer** (`labels/*.json`) &mdash; the strings come straight from the DWG,
   so they are 100% accurate (no OCR), each anchored to a pixel position. This is
   the reliable channel for "read every label."
2. **Tiles** (`--tiles`) &mdash; ~1500 px crops where text is large enough to read
   visually; feed tiles (plus the matching per-tile labels) to a VLM.

The report's "Toggle text layer" button overlays the text as crisp vector SVG
that stays sharp at any zoom.

## Architecture (boundaries)

```
parse (acadrust adapter) ─→ tessellate/pages (pure geometry) ─→ render (tiny-skia) ─→ PNG
                                       └────────────────────────→ metadata ─→ IndexDoc (JSON)
report renders results to HTML.
```

| Module | Responsibility |
|--------|----------------|
| `lib.rs` | composition root: parse (only acadrust I/O), orchestrate, crash isolation |
| `model.rs` | pure domain types: `P`, `Aff`, `Rgb`, `Prim`, `Bounds`, `Page`, `Overlay` |
| `color.rs` | ACI / ByLayer / ByBlock &rarr; RGB (color 7 &rarr; black on white paper) |
| `tessellate.rs` | entity &rarr; primitives + text `Label`s; arc/bulge sampling; INSERT recursion; Text/MText/Attribute/Dimension/MultiLeader |
| `text.rs` | TrueType glyph outlines (ab_glyph) &rarr; filled contours; MTEXT code stripping |
| `pages.rs` | Layout enumeration; model framing; **paper-space viewport projection** (geometry + labels) |
| `render.rs` | world&rarr;screen transform, color-batched stroke/fill, viewport clip masks, **pixel-space text layer**, **tiling** |
| `metadata.rs` | `IndexDoc`: histogram, layers, blocks, attributes, text, layouts |
| `report.rs` | HTML report |

acadrust is imported only in `lib.rs`, `pages.rs`, `tessellate.rs`, `metadata.rs`
&mdash; the rest depends on the pure `model` types, so the parser can be swapped
without touching geometry or rendering.

## What it renders

Line, LwPolyline, Polyline/2D/3D (with bulge arcs), Circle, Arc, Ellipse, Spline
(fit/control points), Solid, Point, Text, MText, Attribute values, and INSERT
blocks (recursive, with composed transform and ByBlock color inheritance).
Paper-space **viewports project model geometry** into the sheet at the plotted
position, scale, and twist, clipped to the viewport rectangle.

## Verified on real data

Run against 11 real DWG 2018 (AC1032) construction/survey files
(`Шумерля` heat-network as-builts), 4&ndash;46 MB:

- **11/11 parsed, 23 pages rendered, 0 crashes.**
- Parse 33&ndash;85 ms for typical files; the 46 MB file in ~20 s.
- The corrupt/recovered file (bad header) **degraded to a placeholder via
  failsafe instead of crashing** &mdash; the SaaS crash-isolation requirement.
- Cyrillic text, layer colors, title-block attributes (e.g. cadastral numbers,
  areas) all extracted and rendered correctly.
- Side-by-side vs a plotted reference PDF shows matching network topology,
  building footprints, and utility runs.

## Known v1 limitations

- **Not rendered (counted in metadata):** Hatch fills, Dimension geometry,
  Leader/MultiLeader, MLine, raster images, 3D solids. The drawing remains
  recognizable from lines/polylines/text/blocks.
- **Splines** use fit points when present, else the control polygon
  (no De&nbsp;Boor) &mdash; chosen for crash-safety on malformed knots.
- **Text** ignores horizontal/vertical alignment offsets; SHX fonts are
  substituted with the configured TrueType font.
- **Lineweights** render as hairlines (constant ~1 px).
- **Per-viewport UCS / layer freeze** not applied; a viewport aimed at an empty
  model region renders empty (correct, but sparse).
