# dwg2png — container size & performance

Measured from the provided `Dockerfile` (Debian bookworm-slim runtime, stripped
release binary, DejaVu Sans font). Reproduce with `docker compose` (see below)
or the commands in each section.

**Environment:** Docker 29.2 (Docker Desktop, linux/amd64), 12 vCPU, 12 GB VM.
Corpus: 11 real DWG 2018 (AC1032) construction/survey drawings, 0.4–46 MB.

## Image & binary size

| Artifact | Size |
|---|---|
| Container image `dwg2png:latest` | **81.1 MB** |
| &nbsp;&nbsp;`debian:bookworm-slim` base | ~74 MB |
| &nbsp;&nbsp;`fonts-dejavu-core` (Latin + Cyrillic) | ~2.8 MB |
| &nbsp;&nbsp;`dwg2png` binary (stripped) | **2.3 MB** |
| Build time (cold, release) | ~2m15s compile |

No runtime dependencies beyond glibc + a TrueType font. No GPU, no C libraries.

## Performance (per file)

Single-threaded per file (`parse` then `render`). Wall time includes process
startup, PNG/tile encoding, and JSON writes.

| Scenario | File | Flags | parse | render | wall | peak RAM |
|---|---|---|---|---|---|---|
| Typical | 0.7 MB, 3 pages | default (3000px) | 77 ms | 0.78 s | **1.0 s** | **201 MB** |
| Typical, hi-res | 0.7 MB, 3 pages | `--size 6000 --tiles` | 57 ms | 1.83 s | **1.9 s** | **423 MB** |
| Worst case | 46 MB, 1 page | `--size 6000 --tiles` | 18.5 s | 7.2 s | **27.2 s** | **2.81 GB** |
| Worst case | 46 MB, 1 page | default (3000px) | 18.8 s | 5.9 s | 25.9 s | 2.77 GB |
| Full batch | 11 files, 23 pages | `--size 6000 --tiles` | — | — | **83 s** | 2.87 GB |

Notes:
- **Memory is driven by DWG size, not render resolution.** The 46 MB drawing
  spends ~18 s in the parser holding a large in-memory model (~2.8 GB); tiling
  and `--size` add little. Sub-MB drawings — the common case — stay at 200–450 MB.
- **Render scales with pixels:** ~0.8 s at 3000px vs ~1.8 s at 6000px for a
  typical 3-page file.
- **Throughput:** ~1–2 s per typical file on one core. Scale horizontally by
  running multiple containers/replicas (one DWG per invocation, fully parallel).

## Output footprint (what a RAG pipeline ingests)

11 files, 23 pages, `--size 6000 --tiles` → **168 MB total**:

| Output | Files | Size | Purpose |
|---|---|---|---|
| `img/` page PNGs | 23 | 55 MB | full-page previews |
| `tiles/` crops | 266 | 84 MB | readable ~1500px tiles for VLM input |
| `labels/` text layer | 23 | 27 MB | **165,588** position-referenced labels (exact text) |
| `json/` index | 11 | 144 KB | per-file metadata (layers, blocks, attributes) |

Text-only ingestion (`labels/` + `json/`, no images) is ~27 MB for the whole
corpus. Drop `--tiles` and lower `--size` to shrink image output.

## Recommended container resources

| Workload | Memory limit | CPU |
|---|---|---|
| Normal DWGs (≤ ~5 MB) | **1 GB** | 1 core/worker |
| Mixed, incl. large site plans (≤ ~50 MB) | **4 GB** | 1 core/worker |

The bundled `docker-compose.yml` sets a 4 GB limit to tolerate worst-case files;
lower it to 1 GB if your inputs are bounded. Parse time and memory grow with DWG
size — gate very large uploads or give those jobs a dedicated high-memory queue.
