//! DWG -> multipage PNG renderer + metadata indexer.
//!
//! Boundaries: `parse` (acadrust adapter, here) -> `pages`/`tessellate`
//! (geometry) -> `render` (raster) and `metadata` (index). `report` renders
//! results to HTML. The acadrust dependency is touched only in this file,
//! `pages`, `tessellate`, and `metadata`.

pub mod color;
pub mod metadata;
pub mod model;
pub mod pages;
pub mod render;
pub mod report;
pub mod tessellate;
pub mod text;

use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::time::Instant;

use ab_glyph::FontVec;
use acadrust::io::dwg::{DwgReadOptions, DwgReader};

use crate::metadata::{extract, IndexDoc};
use crate::pages::build_pages;
use crate::render::{render_page, RenderOpts, RenderedLabel};

/// A rendered page and where its PNG was written (relative to the report).
#[derive(Debug, Clone)]
pub struct PageImage {
    pub name: String,
    pub rel_png: String,
    pub width: u32,
    pub height: u32,
    pub entity_count: usize,
    pub prim_count: usize,
    /// PNG byte size — used as a visual-richness proxy for picking comparisons.
    pub byte_len: usize,
    /// Pixel-space text layer (for the report overlay and downstream VLM use).
    pub labels: Vec<RenderedLabel>,
    /// Readable tile crops (when tiling is enabled).
    pub tiles: Vec<TileRef>,
}

/// A written tile crop, referenced from the report.
#[derive(Debug, Clone)]
pub struct TileRef {
    pub rel_png: String,
    pub col: u32,
    pub row: u32,
    pub width: u32,
    pub height: u32,
    pub label_count: usize,
}

/// Optional side-by-side comparison against a ground-truth raster.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub our_rel_png: String,
    pub our_label: String,
    pub reference_rel_png: String,
    pub reference_label: String,
}

/// Outcome of processing one DWG.
#[derive(Debug, Clone)]
pub struct FileReport {
    pub source: String,
    pub display_name: String,
    pub ok: bool,
    pub error: Option<String>,
    pub index: Option<IndexDoc>,
    pub pages: Vec<PageImage>,
    pub parse_ms: u128,
    pub render_ms: u128,
    pub comparison: Option<Comparison>,
}

/// Parse, render every page, and extract metadata for one DWG file.
///
/// Parsing and rendering are wrapped in `catch_unwind`: a malformed DWG that
/// panics the parser degrades to an error row instead of taking down a batch.
pub struct OutDirs<'a> {
    pub img_dir: &'a Path,
    pub img_rel: &'a str,
    pub labels_dir: &'a Path,
    pub tiles_dir: &'a Path,
    pub tiles_rel: &'a str,
}

pub fn process_file(
    path: &Path,
    dirs: &OutDirs,
    font: &FontVec,
    file_idx: usize,
    opts: RenderOpts,
) -> FileReport {
    let display_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_string();
    let source = path.display().to_string();

    let mut report = FileReport {
        source: source.clone(),
        display_name: display_name.clone(),
        ok: false,
        error: None,
        index: None,
        pages: Vec::new(),
        parse_ms: 0,
        render_ms: 0,
        comparison: None,
    };

    let t0 = Instant::now();
    let mut reader = match DwgReader::from_file_with_options(path, DwgReadOptions::failsafe()) {
        Ok(r) => r,
        Err(e) => {
            report.error = Some(format!("open: {e}"));
            return report;
        }
    };

    let read = std::panic::catch_unwind(AssertUnwindSafe(|| reader.read()));
    let doc = match read {
        Ok(Ok(doc)) => doc,
        Ok(Err(e)) => {
            report.error = Some(format!("read: {e}"));
            return report;
        }
        Err(_) => {
            report.error = Some("read: parser panicked (caught)".to_string());
            return report;
        }
    };
    report.parse_ms = t0.elapsed().as_millis();

    report.index = Some(extract(&doc, &display_name, report.parse_ms));

    let tr = Instant::now();
    let pages = build_pages(&doc, font);
    for (pi, page) in pages.iter().enumerate() {
        let stem = format!("f{file_idx:02}-p{pi:02}");
        let file_name = format!("{stem}.png");
        let abs = dirs.img_dir.join(&file_name);
        let render = std::panic::catch_unwind(AssertUnwindSafe(|| render_page(page, opts)));
        match render {
            Ok(Ok(raster)) => {
                if let Err(e) = std::fs::write(&abs, &raster.png) {
                    report.error = Some(format!("write {}: {e}", abs.display()));
                    continue;
                }
                // Write the position-referenced text layer as JSON.
                let labels_json = serde_json::json!({
                    "image": file_name,
                    "page": page.name,
                    "width": raster.width,
                    "height": raster.height,
                    "label_count": raster.labels.len(),
                    "labels": raster.labels,
                });
                if let Ok(js) = serde_json::to_string(&labels_json) {
                    let _ = std::fs::write(dirs.labels_dir.join(format!("{stem}.json")), js);
                }

                let tiles = write_tiles(dirs, &stem, &raster);

                let prim_count =
                    page.prims.len() + page.overlays.iter().map(|o| o.prims.len()).sum::<usize>();
                report.pages.push(PageImage {
                    name: page.name.clone(),
                    rel_png: format!("{}/{file_name}", dirs.img_rel),
                    width: raster.width,
                    height: raster.height,
                    entity_count: page.entity_count,
                    prim_count,
                    byte_len: raster.png.len(),
                    labels: raster.labels,
                    tiles,
                });
            }
            Ok(Err(e)) => report.error = Some(format!("render {}: {e}", page.name)),
            Err(_) => report.error = Some(format!("render {}: panicked (caught)", page.name)),
        }
    }
    report.render_ms = tr.elapsed().as_millis();
    report.ok = !report.pages.is_empty();
    report
}

/// Write tile PNGs + a per-page tile manifest; return references for the report.
fn write_tiles(dirs: &OutDirs, stem: &str, raster: &crate::render::Raster) -> Vec<TileRef> {
    if raster.tiles.is_empty() {
        return Vec::new();
    }
    let page_dir = dirs.tiles_dir.join(stem);
    if std::fs::create_dir_all(&page_dir).is_err() {
        return Vec::new();
    }
    let mut refs = Vec::new();
    let mut manifest = Vec::new();
    for t in &raster.tiles {
        let tname = format!("r{}c{}.png", t.row, t.col);
        if std::fs::write(page_dir.join(&tname), &t.png).is_err() {
            continue;
        }
        manifest.push(serde_json::json!({
            "file": tname, "col": t.col, "row": t.row,
            "x": t.x, "y": t.y, "width": t.width, "height": t.height,
            "labels": t.labels,
        }));
        refs.push(TileRef {
            rel_png: format!("{}/{stem}/{tname}", dirs.tiles_rel),
            col: t.col,
            row: t.row,
            width: t.width,
            height: t.height,
            label_count: t.labels.len(),
        });
    }
    let manifest_json = serde_json::json!({
        "image_width": raster.width, "image_height": raster.height, "tiles": manifest,
    });
    if let Ok(js) = serde_json::to_string(&manifest_json) {
        let _ = std::fs::write(page_dir.join("tiles.json"), js);
    }
    refs
}

/// Convenience: index a DWG to JSON (metadata only, no rendering).
pub fn index_to_json(path: &Path) -> Result<IndexDoc, String> {
    let mut reader = DwgReader::from_file_with_options(path, DwgReadOptions::failsafe())
        .map_err(|e| format!("open: {e}"))?;
    let t0 = Instant::now();
    let doc = reader.read().map_err(|e| format!("read: {e}"))?;
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("?");
    Ok(extract(&doc, name, t0.elapsed().as_millis()))
}
