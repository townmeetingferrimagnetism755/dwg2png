# dwg2png CLI reference

## Synopsis

```
dwg2png <file-or-dir>... [--out DIR] [--size PX] [--tiles] [--tile-size N]
                         [--font TTF] [--no-compare]
```

Positional arguments are files or directories; directories are recursed for
`*.dwg` (case-insensitive). Multiple inputs may be given.

## Flags

| Flag | Default | Effect |
|------|---------|--------|
| `--out DIR` | `out` | Output root. Creates `img/`, `tiles/`, `labels/`, `json/`, `report.html`. |
| `--size PX` | `3000` | Long-edge pixels of each page PNG. Clamped to 512–12000. |
| `--tiles` | off | Also emit ~1500 px tile crops with per-tile labels. |
| `--tile-size N` | 1500 | Tile edge in pixels (256–4000). Implies tiling. |
| `--font TTF` | macOS Arial | TrueType font for text rendering. |
| `--no-compare` | off | Skip ground-truth PDF comparison (avoids needing `pdftoppm`). |

Environment: `DWG2PNG_FONT` sets the default font path (overridden by `--font`).

## Recommended invocations

```bash
# Fast previews + text layer + index (default resolution, no tiles)
dwg2png input/ --out out --no-compare

# RAG/VLM ingestion: high-res pages + readable tiles + text layer
dwg2png input/ --out out --size 6000 --tiles --no-compare

# Single file
dwg2png drawing.dwg --out out --size 6000 --tiles --no-compare

# Linux/Docker with an explicit font
DWG2PNG_FONT=/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
  dwg2png input/ --out out --tiles --no-compare
```

## Behavior notes

- **Pages:** `p00` is Model space; subsequent pages are paper-space layouts.
  Paper sheets project their model geometry through viewports (position, scale,
  rotation), so a sheet renders like its plotted output.
- **Ground-truth compare:** without `--no-compare`, if a sibling PDF whose name
  matches a DWG is present and `pdftoppm` is on `PATH`, the report shows a
  side-by-side. Omit it for headless ingestion.
- **Exit/robustness:** a corrupt DWG yields a placeholder page rather than
  failing the batch; per-file parse/render timings print to stderr.
- **Stderr** carries progress + timing; **stdout** is unused (safe to ignore).

## Performance & memory (see also the project MEASUREMENTS.md)

| Input | size flag | wall | peak RAM |
|-------|-----------|------|----------|
| ~0.7 MB, 3 pages | default 3000 | ~1.0 s | ~200 MB |
| ~0.7 MB, 3 pages | `--size 6000 --tiles` | ~1.9 s | ~420 MB |
| ~46 MB, 1 page | `--size 6000 --tiles` | ~27 s | ~2.8 GB |

Memory scales with DWG size (parser-bound), not with `--size`/`--tiles`.
