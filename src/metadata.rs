//! Extract a searchable index document from a parsed DWG.

use std::collections::BTreeMap;

use acadrust::document::CadDocument;
use acadrust::entities::EntityType;
use serde::Serialize;

use crate::text::strip_mtext;

const MAX_TEXT_SAMPLES: usize = 60;
const MAX_ATTRS: usize = 60;

#[derive(Debug, Clone, Serialize)]
pub struct LayoutMeta {
    pub name: String,
    pub paper_w: f64,
    pub paper_h: f64,
    pub entity_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Attr {
    pub tag: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexDoc {
    pub file: String,
    pub version: String,
    pub parse_ms: u128,
    pub entity_count: usize,
    pub layer_count: usize,
    pub layers: Vec<String>,
    pub blocks: Vec<String>,
    pub layouts: Vec<LayoutMeta>,
    pub histogram: BTreeMap<String, usize>,
    pub text_samples: Vec<String>,
    pub attributes: Vec<Attr>,
    pub notifications: Vec<String>,
}

fn kind(e: &EntityType) -> &'static str {
    match e {
        EntityType::Point(_) => "Point",
        EntityType::Line(_) => "Line",
        EntityType::Circle(_) => "Circle",
        EntityType::Arc(_) => "Arc",
        EntityType::Ellipse(_) => "Ellipse",
        EntityType::Polyline(_) => "Polyline",
        EntityType::Polyline2D(_) => "Polyline2D",
        EntityType::Polyline3D(_) => "Polyline3D",
        EntityType::LwPolyline(_) => "LwPolyline",
        EntityType::Text(_) => "Text",
        EntityType::MText(_) => "MText",
        EntityType::Spline(_) => "Spline",
        EntityType::Dimension(_) => "Dimension",
        EntityType::Hatch(_) => "Hatch",
        EntityType::Solid(_) => "Solid",
        EntityType::Insert(_) => "Insert",
        EntityType::Viewport(_) => "Viewport",
        EntityType::AttributeDefinition(_) => "AttributeDefinition",
        EntityType::AttributeEntity(_) => "AttributeEntity",
        EntityType::Leader(_) => "Leader",
        EntityType::MultiLeader(_) => "MultiLeader",
        EntityType::MLine(_) => "MLine",
        EntityType::RasterImage(_) => "RasterImage",
        EntityType::Solid3D(_) => "Solid3D",
        EntityType::Face3D(_) => "Face3D",
        EntityType::Region(_) => "Region",
        EntityType::Wipeout(_) => "Wipeout",
        EntityType::Unknown(_) => "Unknown",
        _ => "Other",
    }
}

fn is_user_block(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('*')
        && !name.starts_with("A$")
        && !name.starts_with('_')
}

pub fn extract(doc: &CadDocument, file: &str, parse_ms: u128) -> IndexDoc {
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut text_samples: Vec<String> = Vec::new();
    let mut seen_text = std::collections::HashSet::new();
    let mut attributes: Vec<Attr> = Vec::new();

    for e in doc.entities() {
        *histogram.entry(kind(e).to_string()).or_default() += 1;
        match e {
            EntityType::Text(t) => push_text(&t.value, &mut text_samples, &mut seen_text),
            EntityType::MText(m) => push_text(&strip_mtext(&m.value), &mut text_samples, &mut seen_text),
            EntityType::AttributeEntity(a) => {
                if attributes.len() < MAX_ATTRS && !a.value.trim().is_empty() {
                    attributes.push(Attr { tag: a.tag.clone(), value: a.value.clone() });
                }
            }
            EntityType::Insert(ins) => {
                for a in &ins.attributes {
                    if attributes.len() < MAX_ATTRS && !a.value.trim().is_empty() {
                        attributes.push(Attr { tag: a.tag.clone(), value: a.value.clone() });
                    }
                }
            }
            _ => {}
        }
    }

    let mut layers: Vec<String> = doc.layers.iter().map(|l| l.name.clone()).collect();
    layers.sort();

    let mut blocks: Vec<String> =
        doc.block_records.iter().map(|b| b.name.clone()).filter(|n| is_user_block(n)).collect();
    blocks.sort();
    blocks.dedup();

    let layouts: Vec<LayoutMeta> = doc
        .objects
        .values()
        .filter_map(|o| match o {
            acadrust::objects::ObjectType::Layout(l) => {
                let ec = doc
                    .block_records
                    .iter()
                    .find(|b| b.handle.value() == l.block_record.value())
                    .map(|b| b.entity_handles.len())
                    .unwrap_or(0);
                Some(LayoutMeta {
                    name: l.name.clone(),
                    paper_w: l.paper_width,
                    paper_h: l.paper_height,
                    entity_count: ec,
                })
            }
            _ => None,
        })
        .collect();

    let notifications: Vec<String> = doc.notifications.iter().map(|n| n.to_string()).collect();

    IndexDoc {
        file: file.to_string(),
        version: format!("{:?}", doc.version),
        parse_ms,
        entity_count: doc.entity_count(),
        layer_count: doc.layers.len(),
        layers,
        blocks,
        layouts,
        histogram,
        text_samples,
        attributes,
        notifications,
    }
}

fn push_text(s: &str, out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>) {
    let t = s.trim();
    if t.is_empty() || out.len() >= MAX_TEXT_SAMPLES {
        return;
    }
    if seen.insert(t.to_string()) {
        out.push(t.to_string());
    }
}
