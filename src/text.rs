//! Vector text rendering via TrueType glyph outlines (ab_glyph).
//!
//! Produces filled contours in model coordinates for a text string placed at a
//! given insertion point, height (world units) and rotation, folded through the
//! active INSERT affine. Glyph curves are flattened to polygons; holes are
//! preserved by emitting all contours into a single non-zero-winding fill.

use ab_glyph::{Font, FontVec, OutlineCurve, Point as GPoint};

use crate::model::{Aff, P};

const GAP_EPS: f32 = 0.5; // font-unit distance that starts a new contour
const QUAD_STEPS: usize = 6;
const CUBIC_STEPS: usize = 10;

/// Load a font from a TTF/OTF file.
pub fn load_font(path: &str) -> Result<FontVec, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read font {path}: {e}"))?;
    FontVec::try_from_vec(bytes).map_err(|e| format!("parse font {path}: {e}"))
}

/// Reconstruct filled contours (model coords) for `text`.
pub fn text_contours(
    font: &FontVec,
    text: &str,
    height: f64,
    origin: P,
    rotation: f64,
    aff: &Aff,
) -> Vec<Vec<P>> {
    let upm = font.units_per_em().unwrap_or(1000.0) as f64;
    if upm <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    let s = height / upm;
    let (sin, cos) = rotation.sin_cos();
    let mut pen: f64 = 0.0;
    let mut contours: Vec<Vec<P>> = Vec::new();

    for ch in text.chars() {
        if ch == '\n' || (ch.is_control() && ch != '\t') {
            continue;
        }
        let id = font.glyph_id(ch);
        let pen_now = pen;
        // font-space (fx,fy) -> model coords
        let map = |fx: f32, fy: f32| -> P {
            let ex = s * (pen_now + fx as f64);
            let ey = s * fy as f64;
            let lx = origin.x + cos * ex - sin * ey;
            let ly = origin.y + sin * ex + cos * ey;
            aff.apply(P::new(lx, ly))
        };

        if let Some(outline) = font.outline(id) {
            let mut cur: Vec<P> = Vec::new();
            let mut last: Option<GPoint> = None;
            for curve in &outline.curves {
                let start = curve_start(curve);
                if last.map_or(true, |l| dist(l, start) > GAP_EPS) {
                    if cur.len() >= 2 {
                        contours.push(std::mem::take(&mut cur));
                    } else {
                        cur.clear();
                    }
                    cur.push(map(start.x, start.y));
                }
                flatten(curve, &map, &mut cur);
                last = Some(curve_end(curve));
            }
            if cur.len() >= 2 {
                contours.push(cur);
            }
        }
        pen += font.h_advance_unscaled(id) as f64;
    }
    contours
}

fn dist(a: GPoint, b: GPoint) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

fn curve_start(c: &OutlineCurve) -> GPoint {
    match c {
        OutlineCurve::Line(a, _) => *a,
        OutlineCurve::Quad(a, _, _) => *a,
        OutlineCurve::Cubic(a, _, _, _) => *a,
    }
}

fn curve_end(c: &OutlineCurve) -> GPoint {
    match c {
        OutlineCurve::Line(_, b) => *b,
        OutlineCurve::Quad(_, _, b) => *b,
        OutlineCurve::Cubic(_, _, _, b) => *b,
    }
}

fn flatten(c: &OutlineCurve, map: &impl Fn(f32, f32) -> P, out: &mut Vec<P>) {
    match c {
        OutlineCurve::Line(_, b) => out.push(map(b.x, b.y)),
        OutlineCurve::Quad(a, ctrl, b) => {
            for i in 1..=QUAD_STEPS {
                let t = i as f32 / QUAD_STEPS as f32;
                let mt = 1.0 - t;
                let x = mt * mt * a.x + 2.0 * mt * t * ctrl.x + t * t * b.x;
                let y = mt * mt * a.y + 2.0 * mt * t * ctrl.y + t * t * b.y;
                out.push(map(x, y));
            }
        }
        OutlineCurve::Cubic(a, c1, c2, b) => {
            for i in 1..=CUBIC_STEPS {
                let t = i as f32 / CUBIC_STEPS as f32;
                let mt = 1.0 - t;
                let x = mt * mt * mt * a.x
                    + 3.0 * mt * mt * t * c1.x
                    + 3.0 * mt * t * t * c2.x
                    + t * t * t * b.x;
                let y = mt * mt * mt * a.y
                    + 3.0 * mt * mt * t * c1.y
                    + 3.0 * mt * t * t * c2.y
                    + t * t * t * b.y;
                out.push(map(x, y));
            }
        }
    }
}

/// Strip MTEXT inline formatting codes to readable plain text.
/// Handles \P (newline -> space), \~, font/height/color groups, and braces.
pub fn strip_mtext(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                match chars.peek().copied() {
                    Some('P') | Some('p') => { out.push(' '); chars.next(); }
                    Some('~') => { out.push(' '); chars.next(); }
                    Some('\\') => { out.push('\\'); chars.next(); }
                    Some('{') => { out.push('{'); chars.next(); }
                    Some('}') => { out.push('}'); chars.next(); }
                    // formatting group like \fArial|b0|...; or \H2.5x; or \C1; -> skip to ';'
                    Some(f) if "fFhHcCtTqQwWaApАkKlLoO".contains(f) => {
                        chars.next();
                        for d in chars.by_ref() {
                            if d == ';' {
                                break;
                            }
                        }
                    }
                    _ => {}
                }
            }
            '{' | '}' => {}
            _ => out.push(c),
        }
    }
    out
}
