//! Pure domain types for 2D rendering: points, colors, affine transforms,
//! render primitives, bounds, and pages. No I/O, no acadrust types.

/// RGB color, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
}

/// A 2D point in model/world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

impl P {
    pub fn new(x: f64, y: f64) -> Self {
        P { x, y }
    }
}

/// A 2D affine transform mapping (x,y) -> (a*x + c*y + e, b*x + d*y + f).
///
/// Used to fold INSERT/block nesting (translate, rotate, scale) into a single
/// matrix applied to every emitted point, so nested geometry stays correct
/// under arbitrary rotation and non-uniform scale.
#[derive(Debug, Clone, Copy)]
pub struct Aff {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Aff {
    pub const IDENTITY: Aff = Aff { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn apply(&self, p: P) -> P {
        P {
            x: self.a * p.x + self.c * p.y + self.e,
            y: self.b * p.x + self.d * p.y + self.f,
        }
    }

    /// self ∘ other (apply `other` first, then `self`).
    pub fn then(&self, inner: &Aff) -> Aff {
        // result = self * inner (matrix composition for affine 2x3)
        Aff {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }

    /// Insert-style transform: scale, then rotate (radians), then translate.
    pub fn insert(tx: f64, ty: f64, rot: f64, sx: f64, sy: f64) -> Aff {
        let (s, co) = rot.sin_cos();
        // R * S
        Aff {
            a: co * sx,
            b: s * sx,
            c: -s * sy,
            d: co * sy,
            e: tx,
            f: ty,
        }
    }
}

/// A render primitive in world coordinates.
#[derive(Debug, Clone)]
pub enum Prim {
    /// Open or closed stroked polyline.
    Line { pts: Vec<P>, color: Rgb, width_px: f32, closed: bool },
    /// Filled region (one or more contours, non-zero winding — holes supported).
    Fill { contours: Vec<Vec<P>>, color: Rgb },
    /// A point marker (survey points, node dots).
    Dot { p: P, color: Rgb },
}

/// Axis-aligned bounds accumulator.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub minx: f64,
    pub miny: f64,
    pub maxx: f64,
    pub maxy: f64,
}

impl Bounds {
    pub fn empty() -> Self {
        Bounds { minx: f64::INFINITY, miny: f64::INFINITY, maxx: f64::NEG_INFINITY, maxy: f64::NEG_INFINITY }
    }

    pub fn extend(&mut self, p: P) {
        if p.x.is_finite() && p.y.is_finite() {
            if p.x < self.minx { self.minx = p.x; }
            if p.y < self.miny { self.miny = p.y; }
            if p.x > self.maxx { self.maxx = p.x; }
            if p.y > self.maxy { self.maxy = p.y; }
        }
    }

    pub fn is_valid(&self) -> bool {
        self.minx.is_finite()
            && self.miny.is_finite()
            && self.maxx > self.minx
            && self.maxy > self.miny
    }

    pub fn width(&self) -> f64 {
        self.maxx - self.minx
    }

    pub fn height(&self) -> f64 {
        self.maxy - self.miny
    }
}

/// A text label in world coordinates, kept alongside the rasterized glyphs so it
/// can be emitted as a position-referenced text layer (for VLM/search use).
#[derive(Debug, Clone)]
pub struct Label {
    pub text: String,
    pub origin: P,
    pub height: f64,
    pub rotation: f64,
    pub color: Rgb,
    pub layer: String,
}

/// A clipped layer drawn on top of a page: model geometry projected through a
/// paper-space viewport, clipped to the viewport rectangle.
#[derive(Debug, Clone)]
pub struct Overlay {
    pub clip: Bounds,
    pub prims: Vec<Prim>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /* REQ-aff-1: an INSERT transform translates, then rotates+scales relative
       to the insertion point (a point at the origin lands on the insert point). */
    #[test]
    fn insert_origin_maps_to_insert_point() {
        let a = Aff::insert(10.0, 20.0, std::f64::consts::FRAC_PI_2, 2.0, 2.0);
        let p = a.apply(P::new(0.0, 0.0));
        assert!((p.x - 10.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9);
        // unit x, rotated 90deg, scaled 2 -> (0,2) offset from insert point
        let q = a.apply(P::new(1.0, 0.0));
        assert!((q.x - 10.0).abs() < 1e-9 && (q.y - 22.0).abs() < 1e-9, "got {q:?}");
    }

    /* REQ-aff-2: composition `then` equals applying inner then outer. */
    #[test]
    fn then_is_function_composition() {
        let inner = Aff::insert(1.0, 0.0, 0.0, 3.0, 3.0);
        let outer = Aff::insert(0.0, 5.0, std::f64::consts::PI, 1.0, 1.0);
        let composed = outer.then(&inner);
        let p = P::new(2.0, 1.0);
        let via_composed = composed.apply(p);
        let via_chain = outer.apply(inner.apply(p));
        assert!((via_composed.x - via_chain.x).abs() < 1e-9);
        assert!((via_composed.y - via_chain.y).abs() < 1e-9);
    }
}

/// One renderable page (a layout: Model or a paper-space sheet).
#[derive(Debug, Clone)]
pub struct Page {
    pub name: String,
    pub prims: Vec<Prim>,
    pub overlays: Vec<Overlay>,
    pub labels: Vec<Label>,
    pub bounds: Bounds,
    pub entity_count: usize,
    pub paper: Option<(f64, f64)>,
}
