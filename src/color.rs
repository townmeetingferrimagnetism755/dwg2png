//! Color resolution: AutoCAD color model -> concrete RGB for white-paper output.
//!
//! Rules (CAD convention):
//! - `ByLayer`  -> the entity's layer color
//! - `ByBlock`  -> color inherited from the enclosing INSERT
//! - `Index`/true color -> direct RGB
//! - color 7 / pure white renders as black on white paper

use std::collections::HashMap;

use acadrust::types::Color;

use crate::model::Rgb;

/// Map a resolved RGB onto white paper: near-white becomes black so that
/// model-space "white" geometry is visible.
fn for_white_paper(r: u8, g: u8, b: u8) -> Rgb {
    if r >= 250 && g >= 250 && b >= 250 {
        Rgb::BLACK
    } else {
        Rgb(r, g, b)
    }
}

/// Resolve a layer's own display color (used to build the layer color table).
pub fn layer_rgb(color: &Color) -> Rgb {
    match color.rgb() {
        Some((r, g, b)) => for_white_paper(r, g, b),
        None => Rgb::BLACK,
    }
}

/// Resolve an entity color given its layer and the inherited (ByBlock) color.
pub fn resolve(
    color: &Color,
    layer: &str,
    layer_colors: &HashMap<String, Rgb>,
    inherited: Rgb,
) -> Rgb {
    match color {
        Color::ByLayer => layer_colors.get(layer).copied().unwrap_or(Rgb::BLACK),
        Color::ByBlock => inherited,
        other => match other.rgb() {
            Some((r, g, b)) => for_white_paper(r, g, b),
            None => Rgb::BLACK,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /* REQ-color-1: ByLayer resolves through the layer table; ByBlock inherits. */
    #[test]
    fn bylayer_and_byblock_resolution() {
        let mut layers = HashMap::new();
        layers.insert("WALLS".to_string(), Rgb(10, 20, 30));
        let inherited = Rgb(7, 7, 7);
        assert_eq!(resolve(&Color::ByLayer, "WALLS", &layers, inherited), Rgb(10, 20, 30));
        assert_eq!(resolve(&Color::ByLayer, "MISSING", &layers, inherited), Rgb::BLACK);
        assert_eq!(resolve(&Color::ByBlock, "WALLS", &layers, inherited), inherited);
    }

    /* REQ-color-2: white (ACI 7) renders black on white paper. */
    #[test]
    fn white_becomes_black_on_paper() {
        let layers = HashMap::new();
        assert_eq!(resolve(&Color::Index(7), "x", &layers, Rgb::BLACK), Rgb::BLACK);
        // a non-white index stays itself
        assert_eq!(resolve(&Color::Index(1), "x", &layers, Rgb::BLACK), Rgb(255, 0, 0));
    }
}
