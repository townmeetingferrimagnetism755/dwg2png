# dwg2png output schemas

All under `--out`. `fNN` = zero-padded file index (input order), `pMM` = page
index (`p00` = Model space).

## Contents

- [Directory layout](#directory-layout)
- [labels/fNN-pMM.json — text layer](#text-layer)
- [json/fNN.json — metadata index](#metadata-index)
- [tiles/fNN-pMM/tiles.json — tile manifest](#tile-manifest)

## Directory layout

```
out/
  img/fNN-pMM.png            full page PNG
  tiles/fNN-pMM/rRcC.png     tile crops (row R, col C), if --tiles
  tiles/fNN-pMM/tiles.json   tile manifest with per-tile labels
  labels/fNN-pMM.json        text layer (page pixel space)
  json/fNN.json              per-file metadata index
  report.html                self-contained HTML report
```

## Text layer

`labels/fNN-pMM.json` — every text run on the page, in **page pixel space**.

```json
{
  "image": "f00-p00.png",
  "page": "Model",
  "width": 6000,
  "height": 3992,
  "label_count": 3778,
  "labels": [
    {
      "text": "node-217",
      "x": 1032.0, "y": 1732.0,   // baseline-left anchor, pixels
      "w": 41.0,  "h": 18.0,       // approx box size, pixels
      "rot": -0.0,                  // clockwise screen rotation, degrees
      "color": "#000000",
      "layer": "labels"
    }
  ]
}
```

- Strings come directly from the DWG (Text, MText, block Attributes, Dimension
  text, MultiLeader text) — exact, no OCR.
- `x,y` is the text baseline-left in the page raster; `w,h` is an approximate
  bounding box (advance-width estimate); `rot` is clockwise degrees in screen
  space (Y-down).
- Use as retrieval text + grounding metadata (cite `page` + `x,y,w,h`).

## Metadata index

`json/fNN.json` — per-file searchable summary.

```json
{
  "file": "drawing.dwg",
  "version": "AC1032",
  "parse_ms": 69,
  "entity_count": 8838,
  "layer_count": 31,
  "layers": ["0", "walls", "labels", "..."],
  "blocks": ["TITLEBLOCK", "VALVE", "..."],
  "layouts": [
    { "name": "Model", "paper_w": 210, "paper_h": 297, "entity_count": 5015 },
    { "name": "Sheet1", "paper_w": 594, "paper_h": 841, "entity_count": 2 }
  ],
  "histogram": { "Line": 1527, "LwPolyline": 1286, "Text": 3077, "Insert": 861 },
  "text_samples": ["...", "..."],
  "attributes": [ { "tag": "AREA", "value": "440.1" } ],
  "notifications": ["[Warning] ..."]
}
```

`attributes` are block attribute tag/value pairs (title-block fields — often the
most useful structured metadata). `histogram` counts entities by type.
`notifications` are non-fatal parser diagnostics.

## Tile manifest

`tiles/fNN-pMM/tiles.json` — present when `--tiles` is used. Tile pixel
coordinates are **tile-local** (origin at the tile's top-left).

```json
{
  "image_width": 6000,
  "image_height": 4210,
  "tiles": [
    {
      "file": "r1c1.png",
      "col": 1, "row": 1,
      "x": 1500, "y": 1404,          // tile origin in the full page
      "width": 1500, "height": 1404,
      "labels": [ { "text": "...", "x": 12.0, "y": 880.0, "w": 60.0, "h": 20.0,
                    "rot": 0.0, "color": "#ff0000", "layer": "pipes" } ]
    }
  ]
}
```

To map a tile-local label back to the full page: add the tile's `x`,`y`.
