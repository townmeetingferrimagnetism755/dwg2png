# Using dwg2png output in RAG / VLM chains

dwg2png converts opaque DWGs into grounded, citable artifacts. Two retrieval
channels, both lossless and offline.

## Pipeline

```
DWG ──[dwg2png]──▶ labels/*.json  (exact text + pixel bbox + page + layer)
                   json/*.json     (index: layers, blocks, attributes)
                   tiles/*.png      (readable crops for VLM input)
                   img/*.png        (full-page previews)
        │
        ├─ text channel ─▶ chunk labels ─▶ embed ─▶ vector store
        └─ image channel ▶ embed tiles  ─▶ multimodal store
                                   │
                              retriever ─▶ LLM/VLM ─▶ answer + citation(file,page,bbox)
```

## Text channel (no OCR)

Each `labels/*.json` entry is an exact string with `page`, `layer`, pixel
`x,y,w,h`, and `color`. Build chunks and keep geometry as metadata:

- **Chunk** by label cluster — group labels on the same `layer` whose boxes are
  spatially close (they usually form one annotation). One chunk = the joined
  text of a cluster. `scripts/labels_to_chunks.py` does this.
- **Metadata** per chunk: `{file, page, layer, bbox:[x,y,w,h], tile?}`. Use it
  for filtering (by layer/sheet) and for grounded citations ("sheet *Sheet1*,
  near (x,y)").
- **Structured fields**: also index `json/*.json` `attributes` (title-block
  tag/value pairs, e.g. areas, IDs), `layers`, and `blocks` as document-level
  metadata or as their own retrievable records.

## Image channel (multimodal)

- Embed `tiles/*.png` (~1500 px crops) with an image embedding model; tiles are
  small enough that a VLM can read the text in them.
- Each tile's `tiles.json` already lists the labels inside it — attach that text
  to the tile's vector as caption/metadata, or pass both image and text to the
  VLM at answer time. This pairs visual context with exact strings.

## Why this fits RAG

- **Lossless**: text is read from the DWG, not OCR'd — no recognition errors.
- **Grounded**: every string has a page + pixel box for precise citations.
- **Multimodal**: text and readable image tiles for the same region.
- **Deterministic & offline**: same input → same output; no network/CAD engine.

## Chunking helper

```bash
python3 scripts/labels_to_chunks.py out/labels --out chunks.jsonl \
  [--max-gap PX] [--min-chars N]
```

Reads every `labels/*.json` (a dir or a single file), clusters labels per page
and layer by spatial proximity, and writes JSONL chunks:

```json
{"text": "valve DN100 ...", "metadata": {"file": "f00-p01.json", "page": "Sheet1",
 "layer": "pipes", "bbox": [120.0, 880.0, 540.0, 22.0], "n_labels": 3}}
```

Embed `text`; store `metadata` for filtering and citation. Tune `--max-gap`
(cluster distance) and `--min-chars` (drop trivial chunks) to your drawings.
