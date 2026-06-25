use std::collections::BTreeMap;

use acadrust::entities::EntityType;
use acadrust::io::dwg::{DwgReadOptions, DwgReader};
use acadrust::objects::ObjectType;

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
        EntityType::Wipeout(_) => "Wipeout",
        EntityType::Unknown(_) => "Unknown",
        _ => "Other",
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: probe <file.dwg>");
    let t0 = std::time::Instant::now();
    let mut reader = match DwgReader::from_file_with_options(&path, DwgReadOptions::failsafe()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("OPEN ERROR: {e}");
            std::process::exit(2);
        }
    };
    let doc = match reader.read() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("READ ERROR: {e}");
            std::process::exit(3);
        }
    };
    let dt = t0.elapsed();

    let mut hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    for e in doc.entities() {
        *hist.entry(kind(e)).or_default() += 1;
    }

    println!("file        : {path}");
    println!("parsed in   : {:?}", dt);
    println!("version     : {:?}", doc.version);
    println!("entities    : {}", doc.entity_count());
    println!("layers      : {}", doc.layers.len());
    println!("linetypes   : {}", doc.line_types.len());
    println!("text_styles : {}", doc.text_styles.len());
    println!("block_records: {}", doc.block_records.len());
    println!("objects     : {}", doc.objects.len());
    println!("notifications: {}", doc.notifications.len());

    println!("-- entity histogram --");
    for (k, n) in &hist {
        println!("  {k:20} {n}");
    }

    println!("-- block records (layouts) --");
    for br in doc.block_records.iter() {
        let tag = if br.is_model_space() {
            "MODEL"
        } else if br.is_paper_space() {
            "PAPER"
        } else if br.is_layout() {
            "LAYOUT"
        } else {
            "block"
        };
        if br.is_layout() {
            println!(
                "  [{tag:6}] name={:?} entities={} layout_handle={}",
                br.name,
                br.entity_handles.len(),
                br.layout.value()
            );
        }
    }

    println!("-- Viewports --");
    for e in doc.entities() {
        if let EntityType::Viewport(v) = e {
            println!(
                "  id={} ctr=({:.1},{:.1}) wxh={:.1}x{:.1} view_ctr=({:.1},{:.1}) view_h={:.3} twist={:.4}",
                v.id, v.center.x, v.center.y, v.width, v.height, v.view_center.x, v.view_center.y, v.view_height, v.twist_angle
            );
        }
    }

    println!("-- Layout objects --");
    for (h, o) in &doc.objects {
        if let ObjectType::Layout(l) = o {
            println!(
                "  handle={} name={:?} block_record={} paper={}x{}",
                h.value(),
                l.name,
                l.block_record.value(),
                l.paper_width,
                l.paper_height
            );
        }
    }
}
