# dwg2png Skill

> **Install:** `npx skills add https://github.com/diskd-ai/dwg2png --skill dwg2png` | [skills.sh](https://skills.sh)
>
> Ships in the [`diskd-ai/dwg2png`](https://github.com/diskd-ai/dwg2png) tool repo under `skill/`.

Render AutoCAD **DWG** drawings to multipage PNGs and extract a **lossless,
position-referenced text layer** plus a metadata index — built for RAG/VLM
ingestion and CAD content extraction. Pure-Rust, offline, no GPU or CAD engine.

---

## Scope & Purpose

This skill helps an agent run the `dwg2png` tool and consume its output:

* Render every DWG layout (Model space + paper sheets, via viewport projection) to PNG
* Emit a text layer: every label as **exact text + pixel box** (no OCR)
* Emit readable tiles so a VLM can read labels visually
* Extract a per-file index: layers, blocks, title-block attributes, entity histogram
* Wire the output into RAG / multimodal pipelines

---

## When to Use This Skill

**Triggers:**
* `.dwg` files, AutoCAD drawings, blueprints, engineering/construction/survey drawings
* "DWG to PNG", rendering or previewing CAD drawings
* Extracting text / labels / title-block attributes from CAD
* Building a RAG or multimodal pipeline over CAD drawings
* Running the `dwg2png` CLI or Docker image

**Use cases:**
* Convert a folder of DWGs into page previews + tiles for review
* Pull exact label text + positions out of a drawing (no OCR)
* Index title-block attributes (IDs, areas, etc.) and layer/block inventory
* Feed drawings into a retrieval chain with grounded citations

---

## Quick Reference

### Get the tool

```bash
# Docker (self-contained; bundles a font)
git clone https://github.com/diskd-ai/dwg2png && cd dwg2png
docker build -t dwg2png:latest .

# or a prebuilt binary (macOS arm64/x64, Linux x64, Windows x64)
gh release download --repo diskd-ai/dwg2png --pattern '*linux*'
```

### Run

```bash
dwg2png input/ --out out --size 6000 --tiles --no-compare
```

### Output (`out/`)

```
img/fNN-pMM.png        page previews (p00 = Model space)
tiles/fNN-pMM/...      readable crops + tiles.json (per-tile labels)
labels/fNN-pMM.json    text layer: exact strings + pixel x,y,w,h + layer + color
json/fNN.json          index: layers, blocks, attributes, histogram
report.html            self-contained report (toggleable text overlay)
```

---

## Skill Structure

```
dwg2png/
  SKILL.md                     # workflow: get / run / consume output
  README.md                    # this file (overview)
  references/
    cli.md                     # full CLI flags, invocations, perf
    outputs.md                 # labels / index / tile-manifest schemas
    rag.md                     # RAG + multimodal integration patterns
  scripts/
    labels_to_chunks.py        # cluster the text layer into embed-ready chunks
```

---

## RAG in one step

```bash
python3 scripts/labels_to_chunks.py out/labels --out chunks.jsonl
# -> {"text": "...", "metadata": {"file","page","layer","bbox","n_labels"}}
```

Embed `text`; keep `metadata` for filtering and grounded citations. Pair with the
`tiles/*.png` crops for multimodal answers. See
[references/rag.md](references/rag.md).

---

## Requirements

* Image ~81 MB; binary ~2.3 MB; no GPU / C libraries.
* Typical DWG (≤1 MB): ~1–2 s, 200–450 MB RAM. Large 40–50 MB drawings:
  ~25–30 s, ~2.8 GB RAM (parser-bound) — route to a high-memory queue.
* One DWG per invocation; scale out with parallel workers.

---

## Resources

* **Full workflow**: [SKILL.md](SKILL.md)
* **CLI reference**: [references/cli.md](references/cli.md)
* **Output schemas**: [references/outputs.md](references/outputs.md)
* **RAG integration**: [references/rag.md](references/rag.md)
* **Source**: https://github.com/diskd-ai/dwg2png

---

## License

MPL-2.0
