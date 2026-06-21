use std::fmt::Write as FmtWrite;
use std::path::PathBuf;

use crate::model::sync_map::SyncPoint;
use crate::pdf::tab_detector;

/// Generate an HTML debug file overlaying sync points on each PDF page image.
/// Also runs the tab detector and shows raw detected lines in blue.
/// Returns the path to the written HTML file.
pub async fn generate_sync_debug(
    song_title: String,
    pdf_pages: Vec<PathBuf>,
    page_heights: Vec<f32>,
    points: Vec<SyncPoint>,
    pdf_path: Option<String>, // original PDF path, for re-running the detector
) -> Result<String, String> {
    if pdf_pages.is_empty() {
        return Err("No PDF pages rendered — select a song first".to_string());
    }
    if points.is_empty() {
        return Err("No sync map — run Analyze Sync first".to_string());
    }

    // Re-render at 100 DPI so the tab detector can run in debug mode.
    // Returns (scroll_anchors_per_page, raw_detected_lines_per_page).
    let (anchors_per_page, raw_lines_per_page): (Vec<Vec<f32>>, Vec<Vec<f32>>) = if let Some(ref pdf) = pdf_path {
        let tmp = std::env::temp_dir().join("monotabe").join("debug_render");
        tokio::fs::create_dir_all(&tmp).await.map_err(|e| e.to_string())?;
        let prefix = tmp.join("p");
        let ok = tokio::process::Command::new("pdftoppm")
            .args(["-r", "100", "-png", pdf, &prefix.to_string_lossy()])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            let mut pngs: Vec<PathBuf> = Vec::new();
            let mut rd = tokio::fs::read_dir(&tmp).await.map_err(|e| e.to_string())?;
            while let Some(entry) = rd.next_entry().await.map_err(|e| e.to_string())? {
                let p = entry.path();
                if p.extension().map(|x| x == "png").unwrap_or(false) {
                    pngs.push(p);
                }
            }
            pngs.sort();
            pngs.iter()
                .map(|p| tab_detector::detect_tab_rows_debug(p))
                .unzip()
        } else {
            (vec![], vec![])
        }
    } else {
        (vec![], vec![])
    };

    let out_path = std::env::temp_dir()
        .join("monotabe")
        .join("sync_debug.html");
    tokio::fs::create_dir_all(out_path.parent().unwrap())
        .await
        .map_err(|e| e.to_string())?;

    let html = build_html(&song_title, &pdf_pages, &page_heights, &points, &raw_lines_per_page, &anchors_per_page);
    tokio::fs::write(&out_path, html)
        .await
        .map_err(|e| e.to_string())?;

    Ok(out_path.to_string_lossy().into_owned())
}

fn build_html(
    title: &str,
    pages: &[PathBuf],
    page_heights: &[f32],
    points: &[SyncPoint],
    raw_lines: &[Vec<f32>],
    anchors: &[Vec<f32>],
) -> String {
    let total_dur = points.last().map(|p| p.time_secs).unwrap_or(0.0);

    let mut html = String::new();
    let _ = write!(
        html,
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sync Debug — {title}</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #1a1a1a; color: #eee; font-family: sans-serif; padding: 20px; }}
  h1 {{ font-size: 18px; margin-bottom: 6px; }}
  .meta {{ font-size: 12px; color: #999; margin-bottom: 20px; }}
  .pages {{ display: flex; flex-direction: column; gap: 24px; }}
  .page-wrap {{ display: inline-block; position: relative; }}
  .page-wrap img {{ display: block; border: 1px solid #444; }}
  .page-label {{ font-size: 12px; color: #aaa; margin-bottom: 4px; }}
  .sync-line {{
    position: absolute; left: 0; right: 0; height: 2px; pointer-events: none;
  }}
  .sync-label {{
    position: absolute; left: 4px; transform: translateY(-100%);
    background: rgba(0,0,0,0.75); color: #fff;
    font-size: 10px; padding: 1px 5px; border-radius: 2px;
    white-space: nowrap; pointer-events: none;
  }}
  .legend {{ margin-top: 20px; font-size: 12px; color: #aaa; }}
</style>
</head>
<body>
<h1>Sync Debug — {title}</h1>
<p class="meta">{n_pts} sync points · last timestamp {total_dur:.1}s</p>
<div class="pages">
"#,
        title = title,
        n_pts = points.len(),
        total_dur = total_dur,
    );

    for (page_idx, page_path) in pages.iter().enumerate() {
        // Points for this page, sorted by time
        let page_points: Vec<&SyncPoint> = points
            .iter()
            .filter(|p| p.page as usize == page_idx)
            .collect();

        let page_height_px = page_heights.get(page_idx).copied().unwrap_or(1650.0);
        let file_url = format!("file://{}", page_path.to_string_lossy());

        let _ = write!(
            html,
            r#"<div>
  <div class="page-label">Page {page_num} — {n} sync points</div>
  <div class="page-wrap" style="height:{h}px">
    <img src="{url}" style="height:{h}px; width:auto;">
"#,
            page_num = page_idx + 1,
            n = page_points.len(),
            h = page_height_px as u32,
            url = file_url,
        );

        // Thin blue lines = raw detected string rows (before grouping)
        if let Some(page_raw) = raw_lines.get(page_idx) {
            for &y in page_raw {
                let top_pct = y * 100.0;
                let _ = write!(
                    html,
                    "    <div style=\"position:absolute;left:0;right:0;top:{top:.3}%;height:1px;background:rgba(80,160,255,0.55);pointer-events:none;\"></div>\n",
                    top = top_pct
                );
            }
        }

        // Orange dashed lines = detector scroll anchors (where app will scroll to)
        if let Some(page_anchors) = anchors.get(page_idx) {
            for (i, &y) in page_anchors.iter().enumerate() {
                let top_pct = y * 100.0;
                let _ = write!(
                    html,
                    "    <div style=\"position:absolute;left:0;right:0;top:{top:.3}%;height:2px;\
                     background:rgba(255,160,0,0.85);border-top:2px dashed rgba(255,160,0,0.85);\
                     pointer-events:none;\"></div>\
                     <div style=\"position:absolute;right:4px;top:{top:.3}%;transform:translateY(-100%);\
                     background:rgba(0,0,0,0.75);color:#ffa030;font-size:10px;padding:1px 5px;\
                     border-radius:2px;pointer-events:none;\">↑ row {n} anchor</div>\n",
                    top = top_pct,
                    n = i + 1,
                );
            }
        }

        // Deduplicate by y_frac so we don't draw 50 overlapping lines at the same row.
        // Show only the FIRST and LAST timestamp for each unique y position.
        let mut row_groups: Vec<(f32, f32, f32)> = Vec::new(); // (y_frac, first_t, last_t)
        for pt in &page_points {
            let y = pt.y_offset_px;
            if let Some(last) = row_groups.last_mut() {
                if (last.0 - y).abs() < 0.001 {
                    last.2 = pt.time_secs;
                    continue;
                }
            }
            row_groups.push((y, pt.time_secs, pt.time_secs));
        }

        for (row_i, &(y_frac, first_t, last_t)) in row_groups.iter().enumerate() {
            let color = hue_to_rgb(row_i as f32 / row_groups.len().max(1) as f32);
            let top_pct = y_frac * 100.0;

            let label = if (last_t - first_t).abs() < 0.1 {
                format!("{:.1}s", first_t)
            } else {
                format!("{:.1}s – {:.1}s", first_t, last_t)
            };

            let _ = write!(
                html,
                r#"    <div class="sync-line" style="top:{top:.3}%; background:{color};"></div>
    <div class="sync-label" style="top:{top:.3}%; color:{color};">{label}</div>
"#,
                top = top_pct,
                color = color,
                label = label,
            );
        }

        let _ = write!(html, "  </div>\n</div>\n");
    }

    let _ = write!(
        html,
        r#"</div>
<p class="legend">
  <span style="color:#50a0ff">■</span> Thin blue lines = individual string rows the detector found before grouping.<br>
  <span style="color:#ffa030">■</span> Dashed orange lines = detector scroll anchors (where the viewport top will be for each system).<br>
  <span style="color:#80ff80">■</span> Solid coloured lines = saved sync points (from last LLM analysis). Colour runs green → red.
</p>
</body>
</html>
"#
    );

    html
}

/// Map a 0..1 fraction to a CSS `hsl(…)` colour string (green → yellow → red).
fn hue_to_rgb(t: f32) -> String {
    // Hue: 120° (green) → 0° (red) as t goes 0→1
    let hue = (1.0 - t) * 120.0;
    format!("hsl({hue:.0},90%,65%)")
}
