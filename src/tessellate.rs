//! Convert acadrust entities into flat render primitives in model coordinates,
//! plus a parallel list of text `Label`s (string + world placement) so every
//! piece of text can be emitted as a position-referenced text layer.
//!
//! Curved geometry (arc, circle, ellipse, polyline bulges) is sampled into
//! polylines *before* the active INSERT affine is applied, so nested blocks
//! remain correct under rotation and non-uniform scale. INSERTs recurse into
//! their block definition with a composed transform and inherited (ByBlock)
//! color.

use std::collections::HashMap;
use std::f64::consts::PI;

use ab_glyph::FontVec;
use acadrust::document::CadDocument;
use acadrust::entities::EntityType;

use crate::color::resolve;
use crate::model::{Aff, Label, Prim, Rgb, P};
use crate::text::{strip_mtext, text_contours};

const ARC_STEP: f64 = 0.12; // ~7 degrees per segment
const DEFAULT_WIDTH: f32 = 1.0;
const MAX_DEPTH: usize = 16;

pub struct Ctx<'a> {
    pub doc: &'a CadDocument,
    pub layer_colors: &'a HashMap<String, Rgb>,
    pub font: &'a FontVec,
    /// Fallback text height (world units) for entities without one (dimensions).
    pub default_text_h: f64,
}

/// Sink for one entity's tessellation output.
pub struct Sink<'b> {
    pub prims: &'b mut Vec<Prim>,
    pub labels: &'b mut Vec<Label>,
}

pub fn emit(ctx: &Ctx, e: &EntityType, aff: &Aff, inherited: Rgb, depth: usize, sink: &mut Sink) {
    if depth > MAX_DEPTH {
        return;
    }
    match e {
        EntityType::Line(l) => {
            let col = ecolor(ctx, &l.common.color, &l.common.layer, inherited);
            push_line(sink.prims, &[P::new(l.start.x, l.start.y), P::new(l.end.x, l.end.y)], aff, col, false);
        }
        EntityType::Circle(c) => {
            let col = ecolor(ctx, &c.common.color, &c.common.layer, inherited);
            let pts = sample_arc(c.center.x, c.center.y, c.radius, 0.0, 2.0 * PI);
            push_line(sink.prims, &pts, aff, col, true);
        }
        EntityType::Arc(a) => {
            let col = ecolor(ctx, &a.common.color, &a.common.layer, inherited);
            let mut end = a.end_angle;
            if end < a.start_angle {
                end += 2.0 * PI;
            }
            let pts = sample_arc(a.center.x, a.center.y, a.radius, a.start_angle, end);
            push_line(sink.prims, &pts, aff, col, false);
        }
        EntityType::Ellipse(el) => {
            let col = ecolor(ctx, &el.common.color, &el.common.layer, inherited);
            let pts = sample_ellipse(el);
            let closed = (el.end_parameter - el.start_parameter).abs() >= 2.0 * PI - 1e-6;
            push_line(sink.prims, &pts, aff, col, closed);
        }
        EntityType::LwPolyline(p) => {
            let col = ecolor(ctx, &p.common.color, &p.common.layer, inherited);
            let pts = lwpoly_points(p);
            push_line(sink.prims, &pts, aff, col, p.is_closed);
        }
        EntityType::Polyline2D(p) => {
            let col = ecolor(ctx, &p.common.color, &p.common.layer, inherited);
            let raw: Vec<(P, f64)> =
                p.vertices.iter().map(|v| (P::new(v.location.x, v.location.y), v.bulge)).collect();
            let pts = bulge_path(&raw, p.flags.is_closed());
            push_line(sink.prims, &pts, aff, col, p.flags.is_closed());
        }
        EntityType::Polyline(p) => {
            let col = ecolor(ctx, &p.common.color, &p.common.layer, inherited);
            let pts: Vec<P> = p.vertices.iter().map(|v| P::new(v.location.x, v.location.y)).collect();
            push_line(sink.prims, &pts, aff, col, p.flags.is_closed());
        }
        EntityType::Polyline3D(p) => {
            let col = ecolor(ctx, &p.common.color, &p.common.layer, inherited);
            let pts: Vec<P> = p.vertices.iter().map(|v| P::new(v.position.x, v.position.y)).collect();
            push_line(sink.prims, &pts, aff, col, false);
        }
        EntityType::Spline(s) => {
            let col = ecolor(ctx, &s.common.color, &s.common.layer, inherited);
            let pts: Vec<P> = if !s.fit_points.is_empty() {
                s.fit_points.iter().map(|v| P::new(v.x, v.y)).collect()
            } else {
                s.control_points.iter().map(|v| P::new(v.x, v.y)).collect()
            };
            push_line(sink.prims, &pts, aff, col, s.flags.closed);
        }
        EntityType::Solid(s) => {
            let col = ecolor(ctx, &s.common.color, &s.common.layer, inherited);
            let quad = [
                P::new(s.first_corner.x, s.first_corner.y),
                P::new(s.second_corner.x, s.second_corner.y),
                P::new(s.fourth_corner.x, s.fourth_corner.y),
                P::new(s.third_corner.x, s.third_corner.y),
            ];
            sink.prims.push(Prim::Fill { contours: vec![map_all(aff, &quad)], color: col });
        }
        EntityType::Point(pt) => {
            let col = ecolor(ctx, &pt.common.color, &pt.common.layer, inherited);
            sink.prims.push(Prim::Dot { p: aff.apply(P::new(pt.location.x, pt.location.y)), color: col });
        }
        EntityType::Text(t) => {
            let col = ecolor(ctx, &t.common.color, &t.common.layer, inherited);
            let h = pos_h(t.height, ctx.default_text_h);
            emit_text(ctx, &t.value, h, P::new(t.insertion_point.x, t.insertion_point.y), t.rotation, aff, col, &t.common.layer, sink);
        }
        EntityType::MText(m) => {
            let col = ecolor(ctx, &m.common.color, &m.common.layer, inherited);
            let h = pos_h(m.height, ctx.default_text_h);
            emit_mtext(ctx, &m.value, h, P::new(m.insertion_point.x, m.insertion_point.y), m.rotation, aff, col, &m.common.layer, sink);
        }
        EntityType::AttributeEntity(a) => {
            let col = ecolor(ctx, &a.common.color, &a.common.layer, inherited);
            let h = pos_h(a.height, ctx.default_text_h);
            emit_text(ctx, &a.value, h, P::new(a.insertion_point.x, a.insertion_point.y), a.rotation, aff, col, &a.common.layer, sink);
        }
        EntityType::Dimension(d) => {
            let base = d.base();
            let col = ecolor(ctx, &base.common.color, &base.common.layer, inherited);
            let txt = dimension_text(&base.text, d.measurement());
            if !txt.is_empty() {
                let h = ctx.default_text_h;
                let mp = if base.text_middle_point.x == 0.0 && base.text_middle_point.y == 0.0 {
                    base.insertion_point
                } else {
                    base.text_middle_point
                };
                emit_text(ctx, &txt, h, P::new(mp.x, mp.y), base.text_rotation, aff, col, &base.common.layer, sink);
            }
        }
        EntityType::MultiLeader(ml) => {
            if let Some(t) = ml.text() {
                if !t.trim().is_empty() {
                    let col = ecolor(ctx, &ml.common.color, &ml.common.layer, inherited);
                    let h = pos_h(ml.text_height, ctx.default_text_h);
                    let loc = ml.context.text_location;
                    emit_text(ctx, &strip_mtext(t), h, P::new(loc.x, loc.y), ml.context.text_rotation, aff, col, &ml.common.layer, sink);
                }
            }
        }
        EntityType::Insert(ins) => {
            let ink = ecolor(ctx, &ins.common.color, &ins.common.layer, inherited);
            let local = Aff::insert(ins.insert_point.x, ins.insert_point.y, ins.rotation, ins.x_scale(), ins.y_scale());
            let child = aff.then(&local);
            if let Some(block) = ctx.doc.block_records.get(&ins.block_name) {
                for h in &block.entity_handles {
                    if let Some(ce) = ctx.doc.get_entity(*h) {
                        emit(ctx, ce, &child, ink, depth + 1, sink);
                    }
                }
            }
            for attr in &ins.attributes {
                let col = ecolor(ctx, &attr.common.color, &attr.common.layer, ink);
                let h = pos_h(attr.height, ctx.default_text_h);
                emit_text(ctx, &attr.value, h, P::new(attr.insertion_point.x, attr.insertion_point.y), attr.rotation, aff, col, &attr.common.layer, sink);
            }
        }
        // Not rendered in v1: AttributeDefinition (template), Hatch, Leader,
        // MLine, Viewport, 3D solids, raster images.
        _ => {}
    }
}

fn pos_h(h: f64, fallback: f64) -> f64 {
    if h > 0.0 {
        h
    } else {
        fallback
    }
}

fn dimension_text(text: &str, measurement: f64) -> String {
    let t = text.trim();
    if t.is_empty() || t == "<>" {
        // round to 2 decimals, drop trailing zeros
        let v = (measurement * 100.0).round() / 100.0;
        let s = format!("{v}");
        s
    } else {
        t.replace("<>", &format!("{:.2}", measurement))
    }
}

fn ecolor(ctx: &Ctx, color: &acadrust::types::Color, layer: &str, inherited: Rgb) -> Rgb {
    resolve(color, layer, ctx.layer_colors, inherited)
}

fn map_all(aff: &Aff, pts: &[P]) -> Vec<P> {
    pts.iter().map(|p| aff.apply(*p)).collect()
}

fn push_line(out: &mut Vec<Prim>, local: &[P], aff: &Aff, color: Rgb, closed: bool) {
    if local.len() < 2 {
        return;
    }
    out.push(Prim::Line { pts: map_all(aff, local), color, width_px: DEFAULT_WIDTH, closed });
}

/// Rasterize a single text run and record its `Label` (world placement).
#[allow(clippy::too_many_arguments)]
fn emit_text(ctx: &Ctx, text: &str, h: f64, origin: P, rotation: f64, aff: &Aff, color: Rgb, layer: &str, sink: &mut Sink) {
    if text.trim().is_empty() {
        return;
    }
    let contours = text_contours(ctx.font, text, h, origin, rotation, aff);
    if !contours.is_empty() {
        sink.prims.push(Prim::Fill { contours, color });
    }
    // world-space placement of the label
    let det = (aff.a * aff.d - aff.b * aff.c).abs();
    let s = det.sqrt();
    sink.labels.push(Label {
        text: text.trim().to_string(),
        origin: aff.apply(origin),
        height: h * s,
        rotation: rotation + aff.b.atan2(aff.a),
        color,
        layer: layer.to_string(),
    });
}

#[allow(clippy::too_many_arguments)]
fn emit_mtext(ctx: &Ctx, raw: &str, h: f64, origin: P, rot: f64, aff: &Aff, color: Rgb, layer: &str, sink: &mut Sink) {
    let normalized = raw.replace("\\P", "\n").replace("\\p", "\n");
    let (sin, cos) = rot.sin_cos();
    for (k, line) in normalized.split('\n').enumerate() {
        let plain = strip_mtext(line);
        if plain.trim().is_empty() {
            continue;
        }
        let dy = -(k as f64) * h * 1.6;
        let ox = origin.x - sin * dy;
        let oy = origin.y + cos * dy;
        emit_text(ctx, &plain, h, P::new(ox, oy), rot, aff, color, layer, sink);
    }
}

fn sample_arc(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> Vec<P> {
    if !(r.is_finite() && r > 0.0) {
        return Vec::new();
    }
    let sweep = a1 - a0;
    let n = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = a0 + sweep * (i as f64 / n as f64);
        pts.push(P::new(cx + r * t.cos(), cy + r * t.sin()));
    }
    pts
}

fn sample_ellipse(el: &acadrust::entities::Ellipse) -> Vec<P> {
    let cx = el.center.x;
    let cy = el.center.y;
    let mjx = el.major_axis.x;
    let mjy = el.major_axis.y;
    let mnx = -mjy * el.minor_axis_ratio;
    let mny = mjx * el.minor_axis_ratio;
    let a0 = el.start_parameter;
    let a1 = el.end_parameter;
    let sweep = a1 - a0;
    let n = ((sweep.abs() / ARC_STEP).ceil() as usize).max(2);
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = a0 + sweep * (i as f64 / n as f64);
        let (s, c) = t.sin_cos();
        pts.push(P::new(cx + c * mjx + s * mnx, cy + c * mjy + s * mny));
    }
    pts
}

fn lwpoly_points(p: &acadrust::entities::LwPolyline) -> Vec<P> {
    let raw: Vec<(P, f64)> =
        p.vertices.iter().map(|v| (P::new(v.location.x, v.location.y), v.bulge)).collect();
    bulge_path(&raw, p.is_closed)
}

fn bulge_path(verts: &[(P, f64)], closed: bool) -> Vec<P> {
    if verts.is_empty() {
        return Vec::new();
    }
    let mut out = vec![verts[0].0];
    let n = verts.len();
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        let (p0, b) = verts[i];
        let p1 = verts[(i + 1) % n].0;
        if b.abs() < 1e-9 {
            out.push(p1);
        } else {
            out.extend(arc_from_bulge(p0, p1, b));
        }
    }
    out
}

fn arc_from_bulge(p0: P, p1: P, bulge: f64) -> Vec<P> {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let chord = (dx * dx + dy * dy).sqrt();
    if chord < 1e-12 {
        return vec![p1];
    }
    let theta = 4.0 * bulge.atan();
    let half = theta / 2.0;
    let sin_half = half.sin();
    if sin_half.abs() < 1e-12 {
        return vec![p1];
    }
    let mx = (p0.x + p1.x) / 2.0;
    let my = (p0.y + p1.y) / 2.0;
    let nx = -dy / chord;
    let ny = dx / chord;
    let apo = (chord / 2.0) / half.tan();
    let cx = mx + nx * apo;
    let cy = my + ny * apo;
    let r = (chord / 2.0) / sin_half;
    let start = (p0.y - cy).atan2(p0.x - cx);
    let n = ((theta.abs() / ARC_STEP).ceil() as usize).max(1);
    let mut pts = Vec::with_capacity(n);
    for i in 1..=n {
        let t = start + theta * (i as f64 / n as f64);
        pts.push(P::new(cx + r.abs() * t.cos(), cy + r.abs() * t.sin()));
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    /* REQ-bulge-1: a bulge of 1 between two points produces a semicircle that
       ends at the far endpoint and bows to one side. */
    #[test]
    fn bulge_semicircle_endpoints_and_bow() {
        let pts = arc_from_bulge(P::new(0.0, 0.0), P::new(2.0, 0.0), 1.0);
        let last = pts.last().unwrap();
        assert!((last.x - 2.0).abs() < 1e-6 && last.y.abs() < 1e-6, "ends at far point");
        let max_y = pts.iter().map(|p| p.y.abs()).fold(0.0_f64, f64::max);
        assert!((0.99..=1.0 + 1e-6).contains(&max_y), "semicircle radius ~1 bow, got {max_y}");
    }

    /* REQ-bulge-2: zero bulge degenerates to a straight segment (just the end point). */
    #[test]
    fn zero_bulge_is_straight() {
        let pts = arc_from_bulge(P::new(0.0, 0.0), P::new(5.0, 3.0), 0.0);
        assert_eq!(pts, vec![P::new(5.0, 3.0)]);
    }

    /* REQ-arc-1: a full-circle sample is closed (first ~ last) and on-radius. */
    #[test]
    fn circle_sample_on_radius() {
        let pts = sample_arc(1.0, 2.0, 3.0, 0.0, 2.0 * PI);
        for p in &pts {
            let r = ((p.x - 1.0).powi(2) + (p.y - 2.0).powi(2)).sqrt();
            assert!((r - 3.0).abs() < 1e-9);
        }
        let first = pts.first().unwrap();
        let last = pts.last().unwrap();
        assert!((first.x - last.x).abs() < 1e-9 && (first.y - last.y).abs() < 1e-9);
    }

    /* REQ-dim-1: empty/<> dimension text falls back to the measurement. */
    #[test]
    fn dimension_text_fallback() {
        assert_eq!(dimension_text("", 12.5), "12.5");
        assert_eq!(dimension_text("<>", 12.5), "12.5");
        assert_eq!(dimension_text("DN100", 0.0), "DN100");
    }
}
