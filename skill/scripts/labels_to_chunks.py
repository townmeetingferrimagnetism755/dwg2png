#!/usr/bin/env python3
"""Cluster a dwg2png text layer into embed-ready RAG chunks.

Reads dwg2png `labels/*.json` (a directory or a single file), groups labels per
page and layer by spatial proximity, and writes JSONL chunks:

    {"text": "...", "metadata": {"file","page","layer","bbox","n_labels"}}

Embed `text`; keep `metadata` for filtering and grounded citations.

Usage:
    labels_to_chunks.py <labels-dir-or-file> [--out chunks.jsonl]
                        [--max-gap PX] [--min-chars N]

`--max-gap` defaults to ~2x the median label height (adapts to --size).
Stdlib only; no dependencies.
"""

import argparse
import json
import os
import statistics
import sys


def _load(path):
    """Yield (filename, doc) for each labels JSON under path."""
    if os.path.isdir(path):
        files = sorted(
            os.path.join(path, f) for f in os.listdir(path) if f.endswith(".json")
        )
    else:
        files = [path]
    for f in files:
        with open(f, encoding="utf-8") as fh:
            yield os.path.basename(f), json.load(fh)


class _UF:
    def __init__(self, n):
        self.p = list(range(n))

    def find(self, x):
        while self.p[x] != x:
            self.p[x] = self.p[self.p[x]]
            x = self.p[x]
        return x

    def union(self, a, b):
        self.p[self.find(a)] = self.find(b)


def _gap(a, b):
    """Edge gap between two boxes on each axis (0 if overlapping)."""
    dx = max(a["x"] - (b["x"] + b["w"]), b["x"] - (a["x"] + a["w"]), 0.0)
    dy = max(a["y"] - a["h"] - b["y"], b["y"] - b["h"] - a["y"], 0.0)
    return dx, dy


def _cluster(labels, max_gap):
    """Connected components by box proximity, using a grid for neighbor lookup."""
    n = len(labels)
    uf = _UF(n)
    cell = max(max_gap, 1.0)
    grid = {}
    for i, l in enumerate(labels):
        gx, gy = int(l["x"] // cell), int(l["y"] // cell)
        grid.setdefault((gx, gy), []).append(i)
    for i, l in enumerate(labels):
        gx, gy = int(l["x"] // cell), int(l["y"] // cell)
        for ox in (-1, 0, 1):
            for oy in (-1, 0, 1):
                for j in grid.get((gx + ox, gy + oy), ()):
                    if j <= i:
                        continue
                    dx, dy = _gap(l, labels[j])
                    if dx <= max_gap and dy <= max_gap:
                        uf.union(i, j)
    groups = {}
    for i in range(n):
        groups.setdefault(uf.find(i), []).append(labels[i])
    return list(groups.values())


def _chunk_text(members):
    """Reading order: top-to-bottom then left-to-right; join with spaces."""
    ordered = sorted(members, key=lambda m: (round(m["y"]), m["x"]))
    return " ".join(m["text"].strip() for m in ordered if m["text"].strip())


def _bbox(members):
    x0 = min(m["x"] for m in members)
    y0 = min(m["y"] - m["h"] for m in members)
    x1 = max(m["x"] + m["w"] for m in members)
    y1 = max(m["y"] for m in members)
    return [round(x0, 1), round(y0, 1), round(x1 - x0, 1), round(y1 - y0, 1)]


def chunks_for_doc(fname, doc, max_gap, min_chars):
    labels = doc.get("labels", [])
    if not labels:
        return
    if max_gap is None:
        heights = [l["h"] for l in labels if l.get("h", 0) > 0]
        gap = 2.0 * statistics.median(heights) if heights else 40.0
    else:
        gap = max_gap
    by_layer = {}
    for l in labels:
        by_layer.setdefault(l.get("layer", ""), []).append(l)
    for layer, group in by_layer.items():
        for cluster in _cluster(group, gap):
            text = _chunk_text(cluster)
            if len(text) < min_chars:
                continue
            yield {
                "text": text,
                "metadata": {
                    "file": fname,
                    "page": doc.get("page", ""),
                    "layer": layer,
                    "bbox": _bbox(cluster),
                    "n_labels": len(cluster),
                },
            }


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("path", help="labels/ directory or a single labels JSON file")
    ap.add_argument("--out", default="-", help="output JSONL (default: stdout)")
    ap.add_argument("--max-gap", type=float, default=None,
                    help="cluster distance in px (default: ~2x median label height)")
    ap.add_argument("--min-chars", type=int, default=2,
                    help="drop chunks shorter than this many characters")
    args = ap.parse_args()

    out = sys.stdout if args.out == "-" else open(args.out, "w", encoding="utf-8")
    n_chunks = 0
    try:
        for fname, doc in _load(args.path):
            for chunk in chunks_for_doc(fname, doc, args.max_gap, args.min_chars):
                out.write(json.dumps(chunk, ensure_ascii=False) + "\n")
                n_chunks += 1
    finally:
        if out is not sys.stdout:
            out.close()
    print(f"wrote {n_chunks} chunks", file=sys.stderr)


if __name__ == "__main__":
    main()
