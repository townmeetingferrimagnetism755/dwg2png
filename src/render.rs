//! Rasterize a `Page` to a PNG (tiny-skia, CPU) and emit a pixel-space text
//! layer (`RenderedLabel`s) referencing every label's position in the image.
//!
//! A single world->screen affine (uniform scale, Y-flip, margin) drives both the
//! raster and the label projection. Primitives are batched by color into one
//! path per color; paper-space viewport overlays are clipped by a rect mask.

use std::collections::BTreeMap;

use serde::Serialize;
use tiny_skia::{
    Color, FillRule, IntRect, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::model::{Bounds, Label, Page, Prim, Rgb};

const MARGIN: f32 = 14.0;
const MIN_SIDE: u32 = 64;

/// Rendering resolution options.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub target_long: f32,
    pub max_side: u32,
    /// When set, also slice the page into ~`tile_px`-sized tiles so a VLM can
    /// read labels from readable crops.
    pub tile_px: Option<u32>,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts { target_long: 3000.0, max_side: 8000, tile_px: None }
    }
}

/// A readable crop of a page with its labels in tile-local pixel space.
#[derive(Debug, Clone)]
pub struct Tile {
    pub col: u32,
    pub row: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
    pub labels: Vec<RenderedLabel>,
}

/// A label projected into image pixel space (baseline-left anchor).
#[derive(Debug, Clone, Serialize)]
pub struct RenderedLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Clockwise screen rotation in degrees.
    pub rot: f32,
    pub color: String,
    pub layer: String,
}

pub struct Raster {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub labels: Vec<RenderedLabel>,
    pub tiles: Vec<Tile>,
}

pub fn render_page(page: &Page, opts: RenderOpts) -> Result<Raster, String> {
    let b = page.bounds;
    if !b.is_valid() {
        return placeholder();
    }
    let bw = b.width();
    let bh = b.height();
    let aspect = bw / bh;

    let (cw, ch) = if aspect >= 1.0 {
        (opts.target_long, opts.target_long / aspect as f32)
    } else {
        (opts.target_long * aspect as f32, opts.target_long)
    };
    let cw = (cw as u32).clamp(MIN_SIDE, opts.max_side);
    let ch = (ch as u32).clamp(MIN_SIDE, opts.max_side);

    let sx = (cw as f64 - 2.0 * MARGIN as f64) / bw;
    let sy = (ch as f64 - 2.0 * MARGIN as f64) / bh;
    let scale = sx.min(sy).max(1e-9);

    let used_w = scale * bw;
    let used_h = scale * bh;
    let tx = (cw as f64 - used_w) / 2.0 - scale * b.minx;
    let ty = (ch as f64 - used_h) / 2.0 + scale * b.maxy; // y' = -scale*y + ty
    let xf = Transform::from_row(scale as f32, 0.0, 0.0, -scale as f32, tx as f32, ty as f32);

    let mut pixmap = Pixmap::new(cw, ch).ok_or("failed to allocate pixmap")?;
    pixmap.fill(Color::WHITE);

    for ov in &page.overlays {
        let mask = rect_mask(cw, ch, ov.clip, xf);
        draw_prims(&mut pixmap, &ov.prims, xf, scale, mask.as_ref());
    }
    draw_prims(&mut pixmap, &page.prims, xf, scale, None);

    let labels = project_labels(&page.labels, scale, tx, ty, cw, ch);
    let tiles = match opts.tile_px {
        Some(tp) if cw > tp || ch > tp => build_tiles(&pixmap, &labels, tp)?,
        _ => Vec::new(),
    };
    let png = pixmap.encode_png().map_err(|e| format!("encode png: {e}"))?;
    Ok(Raster { png, width: cw, height: ch, labels, tiles })
}

/// Slice the full pixmap into a grid of ~`tp`-sized tiles, partitioning labels
/// into each tile (coordinates made tile-local).
fn build_tiles(pixmap: &Pixmap, labels: &[RenderedLabel], tp: u32) -> Result<Vec<Tile>, String> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let cols = w.div_ceil(tp).max(1);
    let rows = h.div_ceil(tp).max(1);
    let tw = w.div_ceil(cols);
    let th = h.div_ceil(rows);
    let mut tiles = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let x = col * tw;
            let y = row * th;
            let cwid = tw.min(w - x);
            let chei = th.min(h - y);
            let rect = IntRect::from_xywh(x as i32, y as i32, cwid, chei).ok_or("bad tile rect")?;
            let sub = pixmap.clone_rect(rect).ok_or("clone_rect failed")?;
            let png = sub.encode_png().map_err(|e| format!("encode tile png: {e}"))?;
            let (fx, fy) = (x as f32, y as f32);
            let tile_labels: Vec<RenderedLabel> = labels
                .iter()
                .filter(|l| l.x >= fx && l.x < fx + cwid as f32 && l.y >= fy && l.y < fy + chei as f32)
                .map(|l| RenderedLabel { x: l.x - fx, y: l.y - fy, ..l.clone() })
                .collect();
            tiles.push(Tile { col, row, x, y, width: cwid, height: chei, png, labels: tile_labels });
        }
    }
    Ok(tiles)
}

fn project_labels(labels: &[Label], scale: f64, tx: f64, ty: f64, cw: u32, ch: u32) -> Vec<RenderedLabel> {
    let (wf, hf) = (cw as f64, ch as f64);
    let mut out = Vec::with_capacity(labels.len());
    for l in labels {
        let px = scale * l.origin.x + tx;
        let py = -scale * l.origin.y + ty;
        if !px.is_finite() || !py.is_finite() {
            continue;
        }
        // keep labels whose anchor is on (or just off) the canvas
        if px < -200.0 || px > wf + 200.0 || py < -200.0 || py > hf + 200.0 {
            continue;
        }
        let h_px = (l.height * scale) as f32;
        let w_px = l.text.chars().count() as f32 * h_px * 0.55;
        out.push(RenderedLabel {
            text: l.text.clone(),
            x: px as f32,
            y: py as f32,
            w: w_px,
            h: h_px,
            rot: -(l.rotation.to_degrees() as f32),
            color: format!("#{:02x}{:02x}{:02x}", l.color.0, l.color.1, l.color.2),
            layer: l.layer.clone(),
        });
    }
    out
}

fn draw_prims(pixmap: &mut Pixmap, prims: &[Prim], xf: Transform, scale: f64, mask: Option<&Mask>) {
    let mut line_groups: BTreeMap<(u8, u8, u8), PathBuilder> = BTreeMap::new();
    let mut fill_groups: BTreeMap<(u8, u8, u8), PathBuilder> = BTreeMap::new();
    let mut dot_groups: BTreeMap<(u8, u8, u8), PathBuilder> = BTreeMap::new();
    let dot_r = (1.4 / scale) as f32;

    for prim in prims {
        match prim {
            Prim::Line { pts, color, closed, .. } => {
                if pts.len() < 2 {
                    continue;
                }
                let pb = line_groups.entry(key(*color)).or_default();
                pb.move_to(pts[0].x as f32, pts[0].y as f32);
                for p in &pts[1..] {
                    pb.line_to(p.x as f32, p.y as f32);
                }
                if *closed {
                    pb.close();
                }
            }
            Prim::Fill { contours, color } => {
                let pb = fill_groups.entry(key(*color)).or_default();
                for c in contours {
                    if c.len() < 3 {
                        continue;
                    }
                    pb.move_to(c[0].x as f32, c[0].y as f32);
                    for p in &c[1..] {
                        pb.line_to(p.x as f32, p.y as f32);
                    }
                    pb.close();
                }
            }
            Prim::Dot { p, color } => {
                if dot_r <= 0.0 {
                    continue;
                }
                let pb = dot_groups.entry(key(*color)).or_default();
                pb.push_circle(p.x as f32, p.y as f32, dot_r.max(0.01));
            }
        }
    }

    let stroke = Stroke {
        width: (1.0 / scale) as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    for (c, pb) in line_groups {
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint(c), &stroke, xf, mask);
        }
    }
    for (c, pb) in fill_groups {
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint(c), FillRule::Winding, xf, mask);
        }
    }
    for (c, pb) in dot_groups {
        if let Some(path) = pb.finish() {
            pixmap.fill_path(&path, &paint(c), FillRule::Winding, xf, mask);
        }
    }
}

fn rect_mask(w: u32, h: u32, clip: Bounds, xf: Transform) -> Option<Mask> {
    if !clip.is_valid() {
        return None;
    }
    let mut mask = Mask::new(w, h)?;
    let mut pb = PathBuilder::new();
    pb.move_to(clip.minx as f32, clip.miny as f32);
    pb.line_to(clip.maxx as f32, clip.miny as f32);
    pb.line_to(clip.maxx as f32, clip.maxy as f32);
    pb.line_to(clip.minx as f32, clip.maxy as f32);
    pb.close();
    let path = pb.finish()?;
    mask.fill_path(&path, FillRule::Winding, true, xf);
    Some(mask)
}

fn key(c: Rgb) -> (u8, u8, u8) {
    (c.0, c.1, c.2)
}

fn paint<'a>(c: (u8, u8, u8)) -> Paint<'a> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.0, c.1, c.2, 255);
    p.anti_alias = true;
    p
}

fn placeholder() -> Result<Raster, String> {
    let (cw, ch) = (640u32, 200u32);
    let mut pixmap = Pixmap::new(cw, ch).ok_or("failed to allocate pixmap")?;
    pixmap.fill(Color::from_rgba8(245, 245, 245, 255));
    let png = pixmap.encode_png().map_err(|e| format!("encode png: {e}"))?;
    Ok(Raster { png, width: cw, height: ch, labels: Vec::new(), tiles: Vec::new() })
}
