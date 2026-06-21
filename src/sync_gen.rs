use std::path::PathBuf;

use crate::model::sync_map::SyncPoint;
use crate::pdf::tab_detector;

/// Generate a sync map without LLM: divide the audio duration equally among
/// all detected tab rows across all pages.
///
/// Scroll behaviour per row:
///   • 0 % – 75 %  of row duration → viewport is fixed at the row's scroll anchor
///   • 75 % – 100 % of row duration → viewport smoothly slides to the next row's anchor
///   (cross-page transitions are an instant snap; the slide only happens within a page)
pub async fn generate_simple_sync(
    pdf_path: String,
    song_id: String,
    audio_duration_secs: f32,
) -> Result<Vec<SyncPoint>, String> {
    // Render all pages at 100 DPI so the tab detector can run
    let tmp = std::env::temp_dir()
        .join("monotabe")
        .join(format!("{song_id}_sync"));
    tokio::fs::create_dir_all(&tmp)
        .await
        .map_err(|e| format!("temp dir error: {e}"))?;

    let prefix = tmp.join("p");
    let status = tokio::process::Command::new("pdftoppm")
        .args(["-r", "100", "-png", &pdf_path, &prefix.to_string_lossy()])
        .status()
        .await
        .map_err(|e| format!("pdftoppm not found: {e}"))?;

    if !status.success() {
        return Err("pdftoppm failed while rendering PDF for sync".to_string());
    }

    // Collect page PNGs in order
    let pages: Vec<PathBuf> = {
        let mut rd = tokio::fs::read_dir(&tmp)
            .await
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        while let Some(entry) = rd.next_entry().await.map_err(|e| e.to_string())? {
            let p = entry.path();
            if p.extension().map(|x| x == "png").unwrap_or(false) {
                v.push(p);
            }
        }
        v.sort();
        v
    };

    if pages.is_empty() {
        return Err("No pages rendered from PDF".to_string());
    }

    // Detect tab systems per page: (page_index, scroll_anchor_y_frac)
    let all_rows: Vec<(u32, f32)> = pages
        .iter()
        .enumerate()
        .flat_map(|(page_idx, png)| {
            tab_detector::detect_tab_systems(png)
                .into_iter()
                .map(move |s| (page_idx as u32, s.scroll_anchor))
        })
        .collect();

    let _ = tokio::fs::remove_dir_all(&tmp).await;

    if all_rows.is_empty() {
        return Err("No tab rows detected in PDF — cannot generate sync".to_string());
    }

    let n = all_rows.len();
    let row_duration = audio_duration_secs / n as f32;

    // Build sync points:
    //   Row i snaps at  i * row_duration
    //   Hold point at   i * row_duration + 0.75 * row_duration  (same y)
    //   Next row snaps at  (i+1) * row_duration  ← starts the slide
    //
    // The linear interpolation in scroll_position_at() between the hold point
    // and the next row's snap point produces the smooth slide.
    let mut points: Vec<SyncPoint> = Vec::with_capacity(n * 2);

    for (i, &(page, anchor)) in all_rows.iter().enumerate() {
        let row_start = i as f32 * row_duration;

        // Snap to this row
        points.push(SyncPoint { page, y_offset_px: anchor, time_secs: row_start });

        // Hold point: keep the viewport fixed for the first 75% of the row,
        // then let the interpolation in the scroll path create a smooth slide
        // into the next row (even across page boundaries).
        let is_last = i + 1 == n;
        if !is_last {
            points.push(SyncPoint {
                page,
                y_offset_px: anchor,
                time_secs: row_start + 0.75 * row_duration,
            });
        }
    }

    Ok(points)
}
