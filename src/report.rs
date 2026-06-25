//! Render a batch of `FileReport`s into a single self-contained HTML report.

use std::fmt::Write as _;
use std::path::Path;

use crate::FileReport;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const CSS: &str = r#"
:root{--bg:#0f1115;--card:#171a21;--mut:#8b93a7;--fg:#e6e9ef;--acc:#5aa9ff;--ok:#3fb950;--bad:#f85149;--line:#262b36}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:14px/1.5 -apple-system,Segoe UI,Roboto,Helvetica,Arial,sans-serif}
.wrap{max-width:1200px;margin:0 auto;padding:28px 20px 80px}
h1{font-size:24px;margin:0 0 4px} h2{font-size:18px;margin:26px 0 10px}
.sub{color:var(--mut);margin:0 0 22px}
table{border-collapse:collapse;width:100%;margin:8px 0 16px;font-size:13px}
th,td{border:1px solid var(--line);padding:6px 9px;text-align:left;vertical-align:top}
th{background:#11141a;color:var(--mut);font-weight:600}
.card{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:18px 20px;margin:18px 0}
.badge{display:inline-block;padding:2px 9px;border-radius:20px;font-size:12px;font-weight:600}
.b-ok{background:rgba(63,185,80,.15);color:var(--ok)} .b-bad{background:rgba(248,81,73,.15);color:var(--bad)}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));gap:16px;margin-top:12px}
.page{background:#0c0e12;border:1px solid var(--line);border-radius:8px;padding:10px}
.page img{width:100%;height:auto;background:#fff;border-radius:4px;display:block}
.page .cap{color:var(--mut);font-size:12px;margin-top:7px;display:flex;justify-content:space-between;gap:8px}
.kv{display:grid;grid-template-columns:max-content 1fr;gap:4px 16px;font-size:13px;margin:6px 0 10px}
.kv .k{color:var(--mut)}
.chips span{display:inline-block;background:#11141a;border:1px solid var(--line);border-radius:6px;padding:2px 8px;margin:2px;font-size:12px;color:#cfd6e4}
.cmp{display:grid;grid-template-columns:1fr 1fr;gap:16px;margin-top:10px}
.cmp figcaption{color:var(--mut);font-size:12px;margin-top:6px}
.cmp img{width:100%;height:auto;background:#fff;border-radius:6px;border:1px solid var(--line)}
code{background:#11141a;border:1px solid var(--line);border-radius:4px;padding:1px 5px;font-size:12px}
.note{color:var(--mut);font-size:12px}
.mono{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}
.viewer{position:relative;line-height:0}
.viewer img{width:100%;height:auto;display:block;border-radius:4px}
.tlayer{display:none;position:absolute;inset:0;width:100%;height:100%;pointer-events:none}
.tlayer text{paint-order:stroke;stroke:#fff;stroke-width:2px;stroke-linejoin:round;dominant-baseline:alphabetic}
.show-text .viewer img{opacity:.28}
.show-text .tlayer{display:block}
.toolbar{position:sticky;top:0;z-index:9;background:rgba(15,17,21,.92);backdrop-filter:blur(6px);
  padding:10px 0;margin:0 0 8px;display:flex;gap:10px;align-items:center}
.btn{background:#1d2330;color:var(--fg);border:1px solid var(--line);border-radius:7px;
  padding:7px 14px;font-size:13px;cursor:pointer}
.btn:hover{border-color:var(--acc)}
"#;

pub fn write_report(reports: &[FileReport], out_html: &Path) -> std::io::Result<()> {
    let mut h = String::new();
    let _ = write!(
        h,
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <title>dwg2png &mdash; render &amp; index report</title><style>{CSS}</style></head><body><div class=\"wrap\">"
    );

    let ok = reports.iter().filter(|r| r.ok).count();
    let pages: usize = reports.iter().map(|r| r.pages.len()).sum();
    let _ = write!(
        h,
        "<h1>dwg2png &mdash; render &amp; index report</h1>\
         <p class=\"sub\">Pure-Rust DWG&rarr;PNG via <code>acadrust 0.4.0</code> + <code>tiny-skia</code>. \
         {} files &middot; {ok} parsed OK &middot; {pages} pages rendered.</p>",
        reports.len()
    );

    let total_labels: usize = reports.iter().flat_map(|r| &r.pages).map(|p| p.labels.len()).sum();
    let _ = write!(
        h,
        "<div class=\"toolbar\"><button class=\"btn\" onclick=\"document.body.classList.toggle('show-text')\">\
         Toggle text layer</button><span class=\"note\">{total_labels} position-referenced labels across all pages \
         &mdash; exact text (no OCR), overlaid crisp at any zoom.</span></div>"
    );

    // summary table
    h.push_str("<h2>Summary</h2><table><tr><th>#</th><th>File</th><th>Status</th><th>Version</th>\
                <th>Entities</th><th>Layers</th><th>Pages</th><th>Parse</th><th>Render</th></tr>");
    for (i, r) in reports.iter().enumerate() {
        let (badge, label) = if r.ok {
            ("b-ok", "OK")
        } else {
            ("b-bad", "FAIL")
        };
        let ver = r.index.as_ref().map(|x| x.version.clone()).unwrap_or_default();
        let ents = r.index.as_ref().map(|x| x.entity_count).unwrap_or(0);
        let lays = r.index.as_ref().map(|x| x.layer_count).unwrap_or(0);
        let _ = write!(
            h,
            "<tr><td>{}</td><td><a href=\"#f{i}\">{}</a></td>\
             <td><span class=\"badge {badge}\">{label}</span></td><td>{}</td>\
             <td>{ents}</td><td>{lays}</td><td>{}</td><td>{} ms</td><td>{} ms</td></tr>",
            i + 1,
            esc(&r.display_name),
            esc(&ver),
            r.pages.len(),
            r.parse_ms,
            r.render_ms
        );
    }
    h.push_str("</table>");

    for (i, r) in reports.iter().enumerate() {
        let _ = write!(h, "<div class=\"card\" id=\"f{i}\">");
        let (badge, label) = if r.ok { ("b-ok", "OK") } else { ("b-bad", "FAIL") };
        let _ = write!(
            h,
            "<h2 style=\"margin-top:4px\">{}. {} <span class=\"badge {badge}\">{label}</span></h2>\
             <p class=\"note mono\">{}</p>",
            i + 1,
            esc(&r.display_name),
            esc(&r.source)
        );
        if let Some(err) = &r.error {
            let _ = write!(h, "<p><span class=\"badge b-bad\">error</span> <code>{}</code></p>", esc(err));
        }

        if let Some(ix) = &r.index {
            let _ = write!(
                h,
                "<div class=\"kv\">\
                 <div class=\"k\">Version</div><div>{}</div>\
                 <div class=\"k\">Entities</div><div>{}</div>\
                 <div class=\"k\">Layers</div><div>{}</div>\
                 <div class=\"k\">Parse / Render</div><div>{} ms / {} ms</div></div>",
                esc(&ix.version),
                ix.entity_count,
                ix.layer_count,
                r.parse_ms,
                r.render_ms
            );

            // entity histogram
            h.push_str("<b>Entity histogram</b><div class=\"chips\">");
            for (k, n) in &ix.histogram {
                let _ = write!(h, "<span>{} &middot; {n}</span>", esc(k));
            }
            h.push_str("</div>");

            // layouts
            if !ix.layouts.is_empty() {
                h.push_str("<b>Layouts (pages)</b><table><tr><th>Name</th><th>Paper (mm)</th><th>Entities</th></tr>");
                for l in &ix.layouts {
                    let _ = write!(
                        h,
                        "<tr><td>{}</td><td>{:.0} &times; {:.0}</td><td>{}</td></tr>",
                        esc(&l.name),
                        l.paper_w,
                        l.paper_h,
                        l.entity_count
                    );
                }
                h.push_str("</table>");
            }

            // attributes (title-block fields)
            if !ix.attributes.is_empty() {
                h.push_str("<b>Block attributes (sample)</b><div class=\"chips\">");
                for a in ix.attributes.iter().take(30) {
                    let _ = write!(h, "<span>{}=<b>{}</b></span>", esc(&a.tag), esc(&a.value));
                }
                h.push_str("</div>");
            }

            // text samples
            if !ix.text_samples.is_empty() {
                h.push_str("<b>Indexed text (sample)</b><div class=\"chips\">");
                for t in ix.text_samples.iter().take(40) {
                    let short = if t.chars().count() > 40 {
                        format!("{}…", t.chars().take(40).collect::<String>())
                    } else {
                        t.clone()
                    };
                    let _ = write!(h, "<span>{}</span>", esc(&short));
                }
                h.push_str("</div>");
            }

            if !ix.notifications.is_empty() {
                h.push_str("<b>Parser diagnostics</b><div class=\"chips\">");
                for n in &ix.notifications {
                    let _ = write!(h, "<span>{}</span>", esc(n));
                }
                h.push_str("</div>");
            }
        }

        // comparison
        if let Some(c) = &r.comparison {
            let _ = write!(
                h,
                "<h3>Ground-truth comparison</h3><div class=\"cmp\">\
                 <figure><img src=\"{}\"><figcaption>{}</figcaption></figure>\
                 <figure><img src=\"{}\"><figcaption>{}</figcaption></figure></div>",
                esc(&c.our_rel_png),
                esc(&c.our_label),
                esc(&c.reference_rel_png),
                esc(&c.reference_label)
            );
        }

        // page thumbnails with toggleable SVG text overlay
        if !r.pages.is_empty() {
            h.push_str("<h3>Rendered pages</h3><div class=\"grid\">");
            for p in &r.pages {
                let _ = write!(h, "<div class=\"page\"><div class=\"viewer\">");
                let _ = write!(
                    h,
                    "<a href=\"{}\" target=\"_blank\"><img src=\"{}\" loading=\"lazy\"></a>",
                    esc(&p.rel_png),
                    esc(&p.rel_png)
                );
                svg_text_layer(&mut h, p);
                let _ = write!(
                    h,
                    "</div><div class=\"cap\"><span>{}</span><span>{}&times;{} &middot; {} labels</span></div></div>",
                    esc(&p.name),
                    p.width,
                    p.height,
                    p.labels.len()
                );
            }
            h.push_str("</div>");

            // readable tiles (when tiling enabled)
            for p in &r.pages {
                if p.tiles.is_empty() {
                    continue;
                }
                let _ = write!(
                    h,
                    "<details><summary>Readable tiles &mdash; {} ({} crops)</summary><div class=\"grid\">",
                    esc(&p.name),
                    p.tiles.len()
                );
                for t in &p.tiles {
                    let _ = write!(
                        h,
                        "<div class=\"page\"><a href=\"{}\" target=\"_blank\"><img src=\"{}\" loading=\"lazy\"></a>\
                         <div class=\"cap\"><span>r{}c{}</span><span>{}&times;{} &middot; {} labels</span></div></div>",
                        esc(&t.rel_png),
                        esc(&t.rel_png),
                        t.row,
                        t.col,
                        t.width,
                        t.height,
                        t.label_count
                    );
                }
                h.push_str("</div></details>");
            }
        }

        h.push_str("</div>"); // card
    }

    h.push_str("</div></body></html>");
    std::fs::write(out_html, h)
}

const SVG_LABEL_CAP: usize = 4000;

/// Inline SVG overlay: one crisp `<text>` per label in image pixel space.
fn svg_text_layer(h: &mut String, p: &crate::PageImage) {
    if p.labels.is_empty() {
        return;
    }
    let _ = write!(
        h,
        "<svg class=\"tlayer\" viewBox=\"0 0 {} {}\" preserveAspectRatio=\"none\">",
        p.width, p.height
    );
    for l in p.labels.iter().take(SVG_LABEL_CAP) {
        let fs = l.h.max(1.0);
        let _ = write!(
            h,
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{:.1}\" fill=\"{}\" transform=\"rotate({:.2} {:.1} {:.1})\">{}</text>",
            l.x, l.y, fs, esc(&l.color), l.rot, l.x, l.y, esc(&l.text)
        );
    }
    h.push_str("</svg>");
}
