use std::path::Path;

/// Per-system detection result: where to scroll the viewport, and where the
/// string lines actually sit (for LLM image identification).
pub struct TabSystem {
    /// y_frac to scroll to (above chord names, so chord names appear at viewport top).
    pub scroll_anchor: f32,
    /// y_frac at the vertical center of the 6 (or 4) string lines.
    /// Use this when telling the LLM where to find the row in the image.
    pub string_center: f32,
    /// Number of bars (measures) detected in this row.
    pub bar_count: usize,
}

/// Primary entry point — returns scroll anchors only (used by the auto-scroll path).
pub fn detect_tab_rows(png_path: &Path) -> Vec<f32> {
    detect_inner(png_path, false)
        .map(|(sys, _)| sys.into_iter().map(|s| s.scroll_anchor).collect())
        .unwrap_or_default()
}

/// Returns full per-system data for the LLM path.
pub fn detect_tab_systems(png_path: &Path) -> Vec<TabSystem> {
    detect_inner(png_path, false)
        .map(|(sys, _)| sys)
        .unwrap_or_default()
}

/// Debug variant — returns (systems, raw_line_y_fracs).
pub fn detect_tab_rows_debug(png_path: &Path) -> (Vec<f32>, Vec<f32>) {
    detect_inner(png_path, true)
        .map(|(sys, raw)| {
            let anchors = sys.into_iter().map(|s| s.scroll_anchor).collect();
            (anchors, raw)
        })
        .unwrap_or_default()
}

struct Stave {
    line_count: usize,
    top_y: f32,
    bottom_y: f32,
    spacing: f32,
}

fn detect_inner(png_path: &Path, debug: bool) -> Option<(Vec<TabSystem>, Vec<f32>)> {
    let img = image::open(png_path).ok()?.to_luma8();
    let (width, height) = img.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let x0 = (width as f32 * 0.10) as u32;
    let x1 = (width as f32 * 0.90) as u32;
    let inner_width = (x1 - x0) as usize;
    let dark_threshold = 110u8;

    // ── Step 1: find qualifying pixel rows ──────────────────────────────────
    // Coverage ≥30% AND max gap between dark segments ≤25 px.
    let is_line_row: Vec<bool> = (0..height)
        .map(|y| row_is_string_line(&img, y, x0, x1, dark_threshold, inner_width))
        .collect();

    // ── Step 2: collapse consecutive qualifying rows into single lines ────────
    let mut lines: Vec<f32> = Vec::new();
    let mut in_line = false;
    let mut line_start = 0u32;
    for y in 0..height {
        match (is_line_row[y as usize], in_line) {
            (true, false) => { in_line = true; line_start = y; }
            (false, true) => {
                in_line = false;
                lines.push((line_start + y) as f32 * 0.5);
            }
            _ => {}
        }
    }
    if in_line {
        lines.push((line_start + height) as f32 * 0.5);
    }

    let line_y_fracs: Vec<f32> = if debug {
        lines.iter().map(|&y| y / height as f32).collect()
    } else {
        Vec::new()
    };

    if lines.len() < 4 {
        return Some((Vec::new(), line_y_fracs));
    }

    // ── Step 3: split detected lines into staves ─────────────────────────────
    let gaps: Vec<f32> = lines.windows(2).map(|w| w[1] - w[0]).collect();
    let mut sorted_gaps = gaps.clone();
    sorted_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_gap = sorted_gaps[sorted_gaps.len() / 2];
    let boundary = median_gap * 2.5;

    let mut staves: Vec<Stave> = Vec::new();
    let mut group: Vec<f32> = vec![lines[0]];

    let commit = |group: &[f32], staves: &mut Vec<Stave>| {
        let n = group.len();
        if n < 4 || n > 6 { return; }
        let spacings: Vec<f32> = group.windows(2).map(|w| w[1] - w[0]).collect();
        let mean = spacings.iter().sum::<f32>() / spacings.len() as f32;
        if mean < 3.0 || mean > 35.0 { return; }
        if spacings.iter().any(|&s| (s - mean).abs() / mean > 0.35) { return; }
        staves.push(Stave { line_count: n, top_y: group[0], bottom_y: group[n - 1], spacing: mean });
    };

    for (i, &gap) in gaps.iter().enumerate() {
        if gap > boundary {
            commit(&group, &mut staves);
            group.clear();
        }
        group.push(lines[i + 1]);
    }
    commit(&group, &mut staves);

    // ── Step 4: compute scroll anchors and string centers ────────────────────
    // scroll_anchor: positioned above the chord names so that content is visible
    //   at the top of the viewport.  Uses 55% of the median inter-system gap.
    // string_center: vertical midpoint of the string band — what the LLM should
    //   use to visually identify each row in the page image.
    let tab_staves: Vec<&Stave> = staves
        .iter()
        .filter(|s| s.line_count == 4 || s.line_count == 6)
        .collect();

    let inter_gaps: Vec<f32> = tab_staves
        .windows(2)
        .map(|w| w[1].top_y - w[0].top_y)
        .collect();
    let inter_median = if inter_gaps.is_empty() {
        tab_staves.first().map(|s| s.spacing * 12.0).unwrap_or(60.0)
    } else {
        let mut sorted = inter_gaps.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    };
    let above_offset = inter_median * 0.55;

    let mut systems: Vec<TabSystem> = Vec::new();
    for stave in &tab_staves {
        let notation_above = staves.iter().rev().find(|s| {
            s.line_count == 5
                && s.bottom_y < stave.top_y
                && (stave.top_y - s.bottom_y) < 3.0 * stave.spacing
        });

        let anchor_px = if let Some(notation) = notation_above {
            (notation.top_y - 2.0 * stave.spacing).max(0.0)
        } else {
            (stave.top_y - above_offset).max(0.0)
        };

        let center_px = (stave.top_y + stave.bottom_y) * 0.5;

        systems.push(TabSystem {
            scroll_anchor: anchor_px / height as f32,
            string_center: center_px / height as f32,
            bar_count: count_bars_in_stave(&img, stave, width),
        });
    }

    Some((systems, line_y_fracs))
}

/// Count the number of bars (measures) in a stave by detecting barlines.
///
/// A barline is a vertical line that crosses the full stave height. It shows
/// up as a column where all inter-string gaps are dark. String lines themselves
/// are horizontal so they don't create false positives here — inter-string pixels
/// are only dark at actual barlines (or at note numbers, which at most darken one
/// or two gaps rather than all of them).
fn count_bars_in_stave(img: &image::GrayImage, stave: &Stave, width: u32) -> usize {
    let height = img.height();
    let threshold = 110u8;

    if stave.line_count < 2 {
        return 1;
    }

    // Sample midpoints between consecutive string lines
    let inter_ys: Vec<u32> = (0..stave.line_count - 1)
        .map(|i| (stave.top_y + (i as f32 + 0.5) * stave.spacing).round() as u32)
        .filter(|&y| y < height)
        .collect();

    if inter_ys.is_empty() {
        return 1;
    }

    // Require all but at most one inter-gap to be dark (tolerates minor rendering gaps)
    let min_hits = inter_ys.len().saturating_sub(1).max(1);

    let x0 = (width as f32 * 0.02) as u32;
    let x1 = (width as f32 * 0.98) as u32;

    let mut barline_count = 0usize;
    let mut in_barline = false;

    for x in x0..x1 {
        let hits = inter_ys
            .iter()
            .filter(|&&y| img.get_pixel(x, y)[0] < threshold)
            .count();
        let is_bl = hits >= min_hits;
        match (is_bl, in_barline) {
            (true, false) => {
                barline_count += 1;
                in_barline = true;
            }
            (false, true) => {
                in_barline = false;
            }
            _ => {}
        }
    }

    // barlines include the left and right stave edges, so bars = barlines - 1
    barline_count.saturating_sub(1).max(1)
}

fn row_is_string_line(
    img: &image::GrayImage,
    y: u32,
    x0: u32,
    x1: u32,
    threshold: u8,
    inner_width: usize,
) -> bool {
    let mut dark_count = 0usize;
    let mut max_gap = 0usize;
    let mut current_gap = 0usize;
    let mut first_dark = false;
    let mut in_dark = false;

    for x in x0..x1 {
        let dark = img.get_pixel(x, y)[0] < threshold;
        if dark {
            dark_count += 1;
            if first_dark && !in_dark {
                if current_gap > max_gap { max_gap = current_gap; }
                current_gap = 0;
            }
            first_dark = true;
            in_dark = true;
        } else {
            in_dark = false;
            if first_dark { current_gap += 1; }
        }
    }
    let coverage = dark_count as f32 / inner_width as f32;
    coverage >= 0.30 && max_gap <= 25
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: render martian.pdf page 0 at 100 DPI and verify rows are detected.
    /// Run with: cargo test test_tab_detector -- --nocapture
    #[test]
    fn test_tab_detector_martian() {
        let root = env!("CARGO_MANIFEST_DIR");
        let pdf_path = format!("{root}/test_files/martian.pdf");
        if !std::path::Path::new(&pdf_path).exists() {
            println!("SKIP: test_files/martian.pdf not found");
            return;
        }

        let tmp = std::env::temp_dir().join("monotabe_test_detector");
        std::fs::create_dir_all(&tmp).unwrap();
        let prefix = tmp.join("p");
        let status = std::process::Command::new("pdftoppm")
            .args([
                "-r", "100", "-png", "-f", "1", "-l", "1",
                &pdf_path,
                &prefix.to_string_lossy(),
            ])
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => { println!("SKIP: pdftoppm not available"); return; }
        }

        let mut pngs: Vec<_> = std::fs::read_dir(&tmp)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "png").unwrap_or(false))
            .collect();
        pngs.sort();
        assert!(!pngs.is_empty(), "no PNG rendered");

        let (anchors, raw_lines) = detect_tab_rows_debug(&pngs[0]);
        let systems = detect_tab_systems(&pngs[0]);

        println!("=== Raw detected lines ({} total) ===", raw_lines.len());
        for (i, &y) in raw_lines.iter().enumerate() {
            println!("  line {:3}: y_frac={y:.4}", i + 1);
        }
        println!("\n=== Tab systems ({} total) ===", systems.len());
        for (i, s) in systems.iter().enumerate() {
            println!(
                "  Row {}: scroll_anchor={:.4}  string_center={:.4}",
                i + 1, s.scroll_anchor, s.string_center
            );
        }

        assert!(!anchors.is_empty(), "expected at least one tab row on page 0");
        for &y in &anchors {
            assert!(y >= 0.0 && y < 1.0, "anchor y_frac {y:.4} out of range");
        }
        for w in anchors.windows(2) {
            assert!(w[1] > w[0], "anchors not in ascending order");
        }
        // string_center must be > scroll_anchor for each system
        for s in &systems {
            assert!(s.string_center > s.scroll_anchor,
                "string_center {:.4} should be below scroll_anchor {:.4}",
                s.string_center, s.scroll_anchor);
        }
    }
}
