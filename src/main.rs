//! dwg2png CLI: render DWG files to multipage PNG + JSON index + HTML report.
//!
//! Usage:
//!   dwg2png <file-or-dir> [more...] [--out DIR] [--font TTF] [--no-compare]

use std::path::{Path, PathBuf};
use std::process::Command;

use dwg2png::render::RenderOpts;
use dwg2png::report::write_report;
use dwg2png::{process_file, Comparison, FileReport, OutDirs};

const DEFAULT_FONT: &str = "/System/Library/Fonts/Supplemental/Arial.ttf";

struct Args {
    inputs: Vec<PathBuf>,
    out: PathBuf,
    font: String,
    compare: bool,
    size: f32,
    tiles: Option<u32>,
}

fn parse_args() -> Args {
    let mut inputs = Vec::new();
    let mut out = PathBuf::from("out");
    let mut font = std::env::var("DWG2PNG_FONT").unwrap_or_else(|_| DEFAULT_FONT.to_string());
    let mut compare = true;
    let mut size = 3000.0_f32;
    let mut tiles = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = PathBuf::from(it.next().expect("--out needs a value")),
            "--font" => font = it.next().expect("--font needs a value"),
            "--no-compare" => compare = false,
            "--size" => {
                size = it.next().expect("--size needs a value").parse().expect("--size must be a number")
            }
            "--tiles" => tiles = Some(1500),
            "--tile-size" => {
                tiles = Some(it.next().expect("--tile-size needs a value").parse().expect("--tile-size must be a number"))
            }
            _ => inputs.push(PathBuf::from(a)),
        }
    }
    Args { inputs, out, font, compare, size, tiles }
}

fn collect_dwgs(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
            entries.sort();
            for e in entries {
                collect_dwgs(&e, out);
            }
        }
    } else if p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("dwg")).unwrap_or(false) {
        out.push(p.to_path_buf());
    }
}

/// Normalize a filename stem by trimming a leading numeric/punct prefix.
fn core_name(stem: &str) -> String {
    stem.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ' ' || c == '-')
        .to_lowercase()
        .trim()
        .to_string()
}

/// Find a sibling PDF whose core name matches the DWG (ground-truth plot).
fn ref_pdf_for(dwg: &Path) -> Option<PathBuf> {
    let dir = dwg.parent()?;
    let core = core_name(dwg.file_stem()?.to_str()?);
    if core.len() < 5 {
        return None;
    }
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(usize, PathBuf)> = None;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("pdf")) != Some(true) {
            continue;
        }
        let Some(pc) = p.file_stem().and_then(|s| s.to_str()).map(core_name) else { continue };
        if pc.len() < 5 {
            continue;
        }
        if pc == core || pc.starts_with(&core) || core.starts_with(&pc) {
            let score = pc.len().min(core.len());
            if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
                best = Some((score, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Rasterize page 1 of a PDF to PNG via pdftoppm. Returns the written file name.
fn rasterize_pdf(pdf: &Path, img_dir: &Path, stem: &str) -> Option<String> {
    let out_base = img_dir.join(stem);
    let status = Command::new("pdftoppm")
        .args(["-png", "-f", "1", "-l", "1", "-r", "110", "-singlefile"])
        .arg(pdf)
        .arg(&out_base)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let produced = format!("{stem}.png");
    if img_dir.join(&produced).exists() {
        Some(produced)
    } else {
        None
    }
}

fn main() {
    let args = parse_args();
    if args.inputs.is_empty() {
        eprintln!("usage: dwg2png <file-or-dir>... [--out DIR] [--font TTF] [--size PX] [--no-compare]");
        std::process::exit(1);
    }
    let opts = RenderOpts {
        target_long: args.size.clamp(512.0, 12000.0),
        max_side: 12000,
        tile_px: args.tiles.map(|t| t.clamp(256, 4000)),
    };

    let font = match dwg2png::text::load_font(&args.font) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("font error: {e}");
            std::process::exit(1);
        }
    };

    let img_dir = args.out.join("img");
    let json_dir = args.out.join("json");
    let labels_dir = args.out.join("labels");
    let tiles_dir = args.out.join("tiles");
    std::fs::create_dir_all(&img_dir).expect("create img dir");
    std::fs::create_dir_all(&json_dir).expect("create json dir");
    std::fs::create_dir_all(&labels_dir).expect("create labels dir");
    std::fs::create_dir_all(&tiles_dir).expect("create tiles dir");
    let dirs = OutDirs {
        img_dir: &img_dir,
        img_rel: "img",
        labels_dir: &labels_dir,
        tiles_dir: &tiles_dir,
        tiles_rel: "tiles",
    };

    let mut dwgs = Vec::new();
    for inp in &args.inputs {
        collect_dwgs(inp, &mut dwgs);
    }
    eprintln!("found {} DWG file(s)", dwgs.len());

    let mut reports: Vec<FileReport> = Vec::new();
    for (i, dwg) in dwgs.iter().enumerate() {
        eprint!("[{:>2}/{}] {} ... ", i + 1, dwgs.len(), dwg.file_name().and_then(|s| s.to_str()).unwrap_or("?"));
        let mut rep = process_file(dwg, &dirs, &font, i, opts);

        // write JSON index
        if let Some(ix) = &rep.index {
            if let Ok(js) = serde_json::to_string_pretty(ix) {
                let _ = std::fs::write(json_dir.join(format!("f{i:02}.json")), js);
            }
        }

        // ground-truth comparison
        if args.compare && !rep.pages.is_empty() {
            if let Some(pdf) = ref_pdf_for(dwg) {
                if let Some(ref_png) = rasterize_pdf(&pdf, &img_dir, &format!("ref{i:02}")) {
                    // Compare against the page with the most rendered ink
                    // (PNG size proxy) — usually the plotted sheet; else Model.
                    let model = &rep.pages[0];
                    let our = rep
                        .pages
                        .iter()
                        .skip(1)
                        .max_by_key(|p| p.byte_len)
                        .filter(|p| p.byte_len * 3 > model.byte_len)
                        .unwrap_or(model);
                    rep.comparison = Some(Comparison {
                        our_rel_png: our.rel_png.clone(),
                        our_label: format!("dwg2png render ({})", our.name),
                        reference_rel_png: format!("img/{ref_png}"),
                        reference_label: format!(
                            "Ground truth: {} (page 1, plotted PDF)",
                            pdf.file_name().and_then(|s| s.to_str()).unwrap_or("ref.pdf")
                        ),
                    });
                }
            }
        }

        eprintln!(
            "{} ({} pages, parse {} ms, render {} ms){}",
            if rep.ok { "ok" } else { "FAIL" },
            rep.pages.len(),
            rep.parse_ms,
            rep.render_ms,
            rep.error.as_ref().map(|e| format!(" [{e}]")).unwrap_or_default()
        );
        reports.push(rep);
    }

    let html = args.out.join("report.html");
    write_report(&reports, &html).expect("write report");
    eprintln!("\nreport: {}", html.display());
    let ok = reports.iter().filter(|r| r.ok).count();
    let pages: usize = reports.iter().map(|r| r.pages.len()).sum();
    eprintln!("done: {}/{} files ok, {} pages", ok, reports.len(), pages);
}
