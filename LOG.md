# LOG

## 2026-06-25 — Initial DWG→multipage PNG renderer + indexer

**Goal:** Build a pure-Rust SaaS backend library that indexes DWG files —
render every layout to PNG and extract searchable metadata — verified on real
sample files with proof in an HTML report.

**Motivation:** Indexing DWGs needs both per-sheet previews and searchable
fields (text, title-block attributes, layers, layouts). Chose `acadrust`
(MPL-2.0, pure Rust) over LibreDWG to avoid GPL + C FFI + the crashy C parser.

**Changes:**
- `acadrust 0.4.0` adapter (`lib.rs`): failsafe DWG read, `catch_unwind` around
  parse and render for crash isolation.
- Pure geometry layer: entity → render primitives with arc/circle/ellipse/bulge
  sampling, INSERT block recursion (composed affine + ByBlock color), TrueType
  vector text (ab_glyph).
- Page model: Layout objects → Model space + paper sheets. Paper-space
  **viewport projection** of model geometry (position/scale/twist + clip mask).
- `tiny-skia` CPU renderer: world→screen transform, color-batched stroke/fill,
  hairline widths, Y-flip, viewport clip masks.
- `metadata.rs` → `IndexDoc` JSON (histogram, layers, blocks, attributes, text,
  layouts). `report.rs` → self-contained HTML with thumbnails + PDF comparison.

**Verification status:** PASS. 11/11 real DWG 2018 files (4–46 MB) parsed,
23 pages rendered, 0 crashes. Corrupt/recovered file degraded via failsafe.
Side-by-side vs plotted PDF matches network topology and building footprints.
Metadata extracts real cadastral attributes and Cyrillic text correctly.

**Open / next:** Hatch fills, dimension geometry, leaders, lineweights,
per-viewport UCS/layer-freeze, De Boor spline evaluation, text alignment.
Productionize acadrust risk: vendor/fork, corpus regression, sandboxed worker.

## 2026-06-25 — Label readability for VLM consumption

**Goal:** Make every label readable by a VLM / understand the blueprint 100%.

**Motivation:** Tiny CAD labels are sub-pixel against a large extent, and VLMs
downscale on input, so raster resolution alone can't make all labels legible.

**Changes:**
- **Text layer:** capture every text run (Text, MText, Attribute, Dimension,
  MultiLeader) as a `Label` with world placement, project it through viewports,
  and emit `labels/*.json` with exact strings + pixel `x,y,w,h`, rotation, color,
  layer. Lossless (no OCR).
- **Resolution:** `--size` (default 3000, up to 12000).
- **Tiling:** `--tiles` slices each page into ~1500 px crops (tiny-skia
  `clone_rect`) with per-tile labels — readable VLM input.
- **Report:** toggleable inline-SVG text overlay (crisp at any zoom) + tile grids.

**Verification:** PASS. Full batch at `--size 5000 --tiles`: 11/11 files,
23 pages, 216 tiles, **128,410 position-referenced labels** (e.g. pipe specs
`кжсТ2 45х3,0(125)-1-ппу-ПЭ` legible in tiles). 8 unit tests pass, 0 warnings.
