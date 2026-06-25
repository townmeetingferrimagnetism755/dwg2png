---
name: dwg2png
description: Convert AutoCAD DWG drawings into multipage PNG previews, readable tiles, a lossless position-referenced text layer, and a searchable metadata index — for RAG/VLM ingestion and CAD content extraction. Use when working with .dwg files, rendering DWG to images, extracting text/labels/title-block attributes from CAD drawings or blueprints, building RAG or multimodal pipelines over engineering/construction/survey drawings, or running the dwg2png CLI/Docker tool. Triggers on DWG, AutoCAD, CAD drawing, blueprint, dwg2png, "DWG to PNG", CAD text extraction, or indexing engineering drawings.
---

# dwg2png

`dwg2png` is a pure-Rust CLI that turns an opaque binary DWG (R13–R2018) into
retrievable artifacts: one PNG per layout (Model space + paper sheets, with
viewport projection), optional readable tiles, a **position-referenced text
layer** (every label as exact text + pixel box — no OCR), and a JSON metadata
index. It runs offline with no GPU, no C libraries, and no external CAD engine.

Use this skill to render DWGs, extract their text/metadata, or feed CAD drawings
into a RAG/VLM pipeline.

## Get the tool

Prefer the Docker image (self-contained, includes a font with broad script
coverage); fall back to a release binary or building from source. Source repo:
`diskd-ai/dwg2png`.

```bash
# A) Docker (build once from the repo)
git clone https://github.com/diskd-ai/dwg2png && cd dwg2png
docker build -t dwg2png:latest .

# B) Prebuilt binary (macOS arm64/x64, Linux x64, Windows x64)
gh release download --repo diskd-ai/dwg2png --pattern '*linux*'
tar -xzf dwg2png-*-linux-gnu.tar.gz   # -> ./dwg2png

# C) From source
cargo build --release --bin dwg2png   # -> target/release/dwg2png
```

Native runs need a TrueType font; default is macOS Arial. On Linux/Docker set
`DWG2PNG_FONT` (the image sets it to DejaVu Sans) or pass `--font /path.ttf`.
Use a font whose glyph coverage matches the drawings' labels.

## Run it

```bash
# whole folder -> high-res pages + tiles + text layer + index
dwg2png input/ --out out --size 6000 --tiles --no-compare

# Docker equivalent
docker run --rm -v "$PWD/input:/work/input:ro" -v "$PWD/out:/work/out" \
  dwg2png:latest /work/input --out /work/out --size 6000 --tiles --no-compare
```

Inputs may be files or directories (recursed for `*.dwg`). Key flags:

- `--out DIR` — output root (default `out`)
- `--size PX` — long-edge pixels (default 3000, up to 12000)
- `--tiles` — also emit ~1500 px readable crops (`--tile-size N` to change)
- `--font TTF` — typeface (or `DWG2PNG_FONT` env)
- `--no-compare` — skip the ground-truth PDF comparison (no `pdftoppm` needed)

Full flag reference: see [references/cli.md](references/cli.md).

## What it produces (under `--out`)

```
out/
  img/fNN-pMM.png        page previews (p00 = Model space, then paper sheets)
  tiles/fNN-pMM/rRcC.png readable crops + tiles.json (per-tile labels)
  labels/fNN-pMM.json    text layer: exact strings + pixel x,y,w,h + layer + color
  json/fNN.json          per-file index: layers, blocks, attributes, histogram
  report.html            self-contained report (toggleable SVG text overlay)
```

`fNN` = file index, `pMM` = page index. Output schemas:
[references/outputs.md](references/outputs.md).

## Reading every label (the key idea)

CAD labels are sub-pixel against a large drawing extent, and VLMs downscale
images on input — so a single raster never makes every label legible. Two
channels solve this; use both:

1. **Text layer** (`labels/*.json`) — strings come straight from the DWG, so they
   are 100% accurate (no OCR), each anchored to a pixel box. This is the reliable
   channel for "read every label" and for grounded citations.
2. **Tiles** (`--tiles`) — ~1500 px crops where text is large enough to read
   visually; each tile's `tiles.json` carries the labels inside it.

## Using it in a RAG / VLM pipeline

The text layer + index give OCR-free, positionally-grounded text; the tiles give
readable image context. Build chunks from the labels and attach geometry as
metadata for citations. A ready-made helper:

```bash
# cluster labels into embed-ready chunks (JSONL) with {file,page,layer,bbox}
python3 scripts/labels_to_chunks.py out/labels --out chunks.jsonl
```

Patterns (chunking, embedding, citation, multimodal): see
[references/rag.md](references/rag.md).

## Requirements & sizing

- Image ~81 MB; binary ~2.3 MB. No GPU/C deps.
- Typical file (≤1 MB): ~1–2 s, 200–450 MB RAM. Large 40–50 MB drawings:
  ~25–30 s, **~2.8 GB RAM** (parse-bound) — give those a high-memory queue.
- Memory tracks DWG **file size**, not render resolution. Process one DWG per
  invocation; scale out with parallel workers/replicas.

## Notes

- Not rendered (but counted/indexed): hatch fills, dimension geometry (its text
  *is* captured), leaders, lineweights. Drawings stay recognizable.
- A corrupt/unreadable DWG degrades to a placeholder via failsafe parsing instead
  of crashing — safe for batch ingestion.
