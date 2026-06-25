//! Enumerate renderable pages from a document's Layout objects (Model space +
//! paper-space sheets), tessellate each into primitives + text labels, and
//! project model geometry/labels through paper-space viewports.

use std::collections::HashMap;

use ab_glyph::FontVec;
use acadrust::document::CadDocument;
use acadrust::entities::EntityType;
use acadrust::objects::ObjectType;

use crate::color::layer_rgb;
use crate::model::{Aff, Bounds, Label, Overlay, Page, Prim, Rgb, P};
use crate::tessellate::{emit, Ctx, Sink};

const FRAME_GEOM_MIN: usize = 10;
const DEFAULT_TEXT_H: f64 = 2.5;

pub fn layer_colors(doc: &CadDocument) -> HashMap<String, Rgb> {
    doc.layers.iter().map(|l| (l.name.clone(), layer_rgb(&l.color))).collect()
}

/// Median height of placed `Text` entities, as a fallback for entities (e.g.
/// dimensions) that don't carry an explicit height.
fn median_text_height(doc: &CadDocument) -> f64 {
    let mut hs: Vec<f64> = doc
        .entities()
        .filter_map(|e| match e {
            EntityType::Text(t) if t.height > 0.0 => Some(t.height),
            _ => None,
        })
        .collect();
    if hs.is_empty() {
        return DEFAULT_TEXT_H;
    }
    hs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    hs[hs.len() / 2]
}

struct LayoutRef {
    name: String,
    block_record: u64,
    tab_order: i16,
    paper: Option<(f64, f64)>,
}

pub fn build_pages(doc: &CadDocument, font: &FontVec) -> Vec<Page> {
    let colors = layer_colors(doc);
    let ctx = Ctx { doc, layer_colors: &colors, font, default_text_h: median_text_height(doc) };

    let mut layouts: Vec<LayoutRef> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            ObjectType::Layout(l) => Some(LayoutRef {
                name: l.name.clone(),
                block_record: l.block_record.value(),
                tab_order: l.tab_order,
                paper: Some((l.paper_width, l.paper_height)),
            }),
            _ => None,
        })
        .collect();
    layouts.sort_by(|a, b| {
        let am = a.name == "Model";
        let bm = b.name == "Model";
        bm.cmp(&am).then(a.tab_order.cmp(&b.tab_order))
    });

    // Tessellate model space once; reuse for every viewport projection.
    let model_handle = layouts.iter().find(|l| l.name == "Model").map(|l| l.block_record);
    let (model_prims, model_labels) = match model_handle {
        Some(h) => tessellate_block(&ctx, doc, h),
        None => (Vec::new(), Vec::new()),
    };

    let mut pages = Vec::new();
    for lay in layouts {
        let Some(br) = doc.block_records.iter().find(|b| b.handle.value() == lay.block_record) else {
            continue;
        };
        let entity_count = br.entity_handles.len();
        let is_model = lay.name == "Model";
        if !is_model && entity_count == 0 {
            continue;
        }

        if is_model {
            let bounds = framed_bounds(&model_prims);
            pages.push(Page {
                name: lay.name,
                prims: model_prims.clone(),
                overlays: Vec::new(),
                labels: model_labels.clone(),
                bounds,
                entity_count,
                paper: lay.paper,
            });
            continue;
        }

        let mut prims: Vec<Prim> = Vec::new();
        let mut labels: Vec<Label> = Vec::new();
        let mut overlays: Vec<Overlay> = Vec::new();
        {
            let mut sink = Sink { prims: &mut prims, labels: &mut labels };
            for h in &br.entity_handles {
                let Some(e) = doc.get_entity(*h) else { continue };
                if let EntityType::Viewport(vp) = e {
                    if let Some((ov, vp_labels)) = viewport_projection(vp, &model_prims, &model_labels) {
                        overlays.push(ov);
                        sink.labels.extend(vp_labels);
                    }
                } else {
                    emit(&ctx, e, &Aff::IDENTITY, Rgb::BLACK, 0, &mut sink);
                }
            }
        }

        let mut bounds = Bounds::empty();
        extend_from_prims(&mut bounds, &prims);
        for ov in &overlays {
            bounds.extend(P::new(ov.clip.minx, ov.clip.miny));
            bounds.extend(P::new(ov.clip.maxx, ov.clip.maxy));
        }
        if !bounds.is_valid() {
            if let Some((w, h)) = lay.paper {
                if w > 0.0 && h > 0.0 {
                    bounds = Bounds { minx: 0.0, miny: 0.0, maxx: w, maxy: h };
                }
            }
        }

        pages.push(Page { name: lay.name, prims, overlays, labels, bounds, entity_count, paper: lay.paper });
    }
    pages
}

fn tessellate_block(ctx: &Ctx, doc: &CadDocument, block_handle: u64) -> (Vec<Prim>, Vec<Label>) {
    let mut prims = Vec::new();
    let mut labels = Vec::new();
    if let Some(br) = doc.block_records.iter().find(|b| b.handle.value() == block_handle) {
        let mut sink = Sink { prims: &mut prims, labels: &mut labels };
        for h in &br.entity_handles {
            if let Some(e) = doc.get_entity(*h) {
                emit(ctx, e, &Aff::IDENTITY, Rgb::BLACK, 0, &mut sink);
            }
        }
    }
    (prims, labels)
}

/// Project model prims + labels through one floating viewport.
fn viewport_projection(
    vp: &acadrust::entities::Viewport,
    model_prims: &[Prim],
    model_labels: &[Label],
) -> Option<(Overlay, Vec<Label>)> {
    if vp.view_height <= 0.0 || vp.width <= 0.0 || vp.height <= 0.0 {
        return None;
    }
    let self_ref = (vp.view_center.x - vp.center.x).hypot(vp.view_center.y - vp.center.y) < 1e-3;
    if self_ref {
        return None;
    }
    let s = vp.height / vp.view_height;
    let rot = -vp.twist_angle;
    let (sin, cos) = rot.sin_cos();
    let vc = (vp.view_center.x, vp.view_center.y);
    let ctr = (vp.center.x, vp.center.y);
    let a = s * cos;
    let b = s * sin;
    let c = -s * sin;
    let d = s * cos;
    let aff = Aff { a, b, c, d, e: ctr.0 - (a * vc.0 + c * vc.1), f: ctr.1 - (b * vc.0 + d * vc.1) };

    let clip = Bounds {
        minx: ctr.0 - vp.width / 2.0,
        miny: ctr.1 - vp.height / 2.0,
        maxx: ctr.0 + vp.width / 2.0,
        maxy: ctr.1 + vp.height / 2.0,
    };
    let prims = transform_prims(model_prims, &aff);
    let labels = transform_labels(model_labels, &aff)
        .into_iter()
        .filter(|l| {
            l.origin.x >= clip.minx && l.origin.x <= clip.maxx && l.origin.y >= clip.miny && l.origin.y <= clip.maxy
        })
        .collect();
    Some((Overlay { clip, prims }, labels))
}

fn transform_prims(prims: &[Prim], aff: &Aff) -> Vec<Prim> {
    prims
        .iter()
        .map(|p| match p {
            Prim::Line { pts, color, width_px, closed } => Prim::Line {
                pts: pts.iter().map(|q| aff.apply(*q)).collect(),
                color: *color,
                width_px: *width_px,
                closed: *closed,
            },
            Prim::Fill { contours, color } => Prim::Fill {
                contours: contours.iter().map(|c| c.iter().map(|q| aff.apply(*q)).collect()).collect(),
                color: *color,
            },
            Prim::Dot { p, color } => Prim::Dot { p: aff.apply(*p), color: *color },
        })
        .collect()
}

fn transform_labels(labels: &[Label], aff: &Aff) -> Vec<Label> {
    let s = (aff.a * aff.d - aff.b * aff.c).abs().sqrt();
    let drot = aff.b.atan2(aff.a);
    labels
        .iter()
        .map(|l| Label {
            text: l.text.clone(),
            origin: aff.apply(l.origin),
            height: l.height * s,
            rotation: l.rotation + drot,
            color: l.color,
            layer: l.layer.clone(),
        })
        .collect()
}

fn framed_bounds(prims: &[Prim]) -> Bounds {
    let mut geom = Bounds::empty();
    let mut full = Bounds::empty();
    let mut geom_count = 0usize;
    for p in prims {
        match p {
            Prim::Line { pts, .. } => {
                geom_count += 1;
                for q in pts {
                    geom.extend(*q);
                    full.extend(*q);
                }
            }
            Prim::Fill { contours, .. } => {
                geom_count += 1;
                for c in contours {
                    for q in c {
                        geom.extend(*q);
                        full.extend(*q);
                    }
                }
            }
            Prim::Dot { p, .. } => full.extend(*p),
        }
    }
    if geom_count >= FRAME_GEOM_MIN && geom.is_valid() {
        geom
    } else {
        full
    }
}

fn extend_from_prims(b: &mut Bounds, prims: &[Prim]) {
    for prim in prims {
        match prim {
            Prim::Line { pts, .. } => {
                for p in pts {
                    b.extend(*p);
                }
            }
            Prim::Fill { contours, .. } => {
                for c in contours {
                    for p in c {
                        b.extend(*p);
                    }
                }
            }
            Prim::Dot { p, .. } => b.extend(*p),
        }
    }
}
