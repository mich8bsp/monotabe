use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

use crate::model::sync_map::SyncPoint;
use crate::pdf::tab_detector;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Returns ANTHROPIC_API_KEY from environment, or an error string.
pub fn api_key() -> Result<String, String> {
    std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set".to_string())
}

/// Render PDF pages at 100 DPI, detect tab row y-positions from the images, then
/// ask the LLM only for timing (measures per row, tempo, repeats).
/// Returns SyncPoints with y_frac stored in y_offset_px (scroll code scales by page height).
pub async fn analyze_tab_sync(
    api_key: String,
    model: String,
    pdf_path: String,
    song_id: String,
    audio_duration_secs: f32,
) -> Result<Vec<SyncPoint>, String> {
    // Render pages at 100 DPI
    let llm_dir = std::env::temp_dir().join("monotabe").join(format!("{song_id}_llm"));
    tokio::fs::create_dir_all(&llm_dir)
        .await
        .map_err(|e| format!("temp dir error: {e}"))?;

    let prefix = llm_dir.join("p");
    let status = tokio::process::Command::new("pdftoppm")
        .args(["-r", "100", "-png", &pdf_path, &prefix.to_string_lossy()])
        .status()
        .await
        .map_err(|e| format!("pdftoppm not found: {e}"))?;

    if !status.success() {
        return Err("pdftoppm failed during LLM render".to_string());
    }

    // Collect + sort page files
    let pages: Vec<PathBuf> = {
        let mut rd = tokio::fs::read_dir(&llm_dir)
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

    let n_pages = pages.len();
    if n_pages == 0 {
        return Err("No pages rendered for LLM".to_string());
    }

    // Detect tab row y positions from images (blocking, but images are small).
    // page_systems[page][row] = TabSystem { scroll_anchor, string_center }
    // We give the LLM string_center (so it can visually locate the row in the image)
    // and use scroll_anchor when building SyncPoints (so the viewport scrolls to show
    // chord names at the top, not just the middle of the string band).
    let page_systems: Vec<Vec<tab_detector::TabSystem>> = pages
        .iter()
        .map(|p| tab_detector::detect_tab_systems(p))
        .collect();

    let total_detected: usize = page_systems.iter().map(|r| r.len()).sum();
    let has_detection = total_detected > 0;

    // Build the prompt
    let intro = if has_detection {
        // Primary path: we know the y positions, ask LLM only for timing.
        // Use string_center so the LLM can find the row visually in the image.
        let mut layout = String::new();
        for (i, systems) in page_systems.iter().enumerate() {
            if systems.is_empty() {
                layout.push_str(&format!("Page {i}: no tab rows detected\n"));
            } else {
                layout.push_str(&format!("Page {i} — {} tab row(s):\n", systems.len()));
                for (j, s) in systems.iter().enumerate() {
                    layout.push_str(&format!("  Row {}: y_frac={:.4}\n", j + 1, s.string_center));
                }
            }
        }
        format!(
            "You are analyzing {n_pages} page(s) of a guitar/bass tablature PDF.\n\
             Audio duration: {audio_duration_secs:.1} seconds.\n\
             \n\
             The tab row positions have already been measured from the images:\n\
             {layout}\n\
             YOUR ONLY JOB is to determine TIMING for each measure:\n\
             1. Count measures in each row (vertical bar lines crossing all strings).\n\
             2. Find any tempo marking (e.g. '♩= 120', 'q = 90').\n\
                If found: secs_per_measure = (60 / BPM) * beats_per_bar (usually 4).\n\
                If not found: secs_per_measure = {audio_duration_secs:.1} / total_measure_count.\n\
             3. Assign time_secs to each measure sequentially from 0.0.\n\
                All measures in the same row get the row's y_frac from the list above.\n\
             4. If repeat signs (||: :||) or D.C./D.S. are present, include repeated passes.\n\
             \n\
             Return ONLY a raw JSON array — one object per measure:\n\
             [{{\"page\":0,\"row\":1,\"time_secs\":0.0}}, ...]\n\
             \n\
             Rules:\n\
             - page is 0-indexed; row is 1-indexed per page (matching the list above)\n\
             - First entry MUST have time_secs=0.0\n\
             - time_secs must be strictly increasing\n\
             - Last entry time_secs must be close to {audio_duration_secs:.1}"
        )
    } else {
        // Fallback: detection failed, ask LLM to estimate y positions
        format!(
            "You are analyzing {n_pages} page(s) of a guitar/bass tablature PDF.\n\
             Audio duration: {audio_duration_secs:.1} seconds.\n\
             \n\
             A tab row is a FULL-WIDTH band of exactly 6 parallel horizontal lines\n\
             (4 for bass) with fret numbers on them. Chord diagrams (small grid boxes\n\
             near the top of the page) are NOT tab rows. Title/header text is NOT a tab row.\n\
             \n\
             For each measure emit page (0-indexed), y_frac (0.0=top, 1.0=bottom of that\n\
             page image; use the vertical center of the 6 string lines), and time_secs.\n\
             The first tab row y_frac is typically 0.3–0.6 because headers come first.\n\
             \n\
             Return ONLY raw JSON: [{{\"page\":0,\"y_frac\":0.42,\"time_secs\":0.0}}, ...]\n\
             First time_secs=0.0, strictly increasing, last ≈ {audio_duration_secs:.1}."
        )
    };

    let mut content: Vec<Value> = vec![json!({ "type": "text", "text": intro })];

    for (i, path) in pages.iter().enumerate() {
        let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
        let encoded = STANDARD.encode(&bytes);
        content.push(json!({ "type": "text", "text": format!("Page {i}:") }));
        content.push(json!({
            "type": "image",
            "source": { "type": "base64", "media_type": "image/png", "data": encoded }
        }));
    }

    let body = json!({
        "model": model,
        "max_tokens": 8192,
        "system": "You are a music tab analyst. Always respond with ONLY raw JSON — no markdown, no code blocks, no explanation.",
        "messages": [{"role": "user", "content": content}]
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(API_URL)
        .header("x-api-key", &api_key)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("API error {status}: {text}"));
    }

    let json_resp: Value =
        resp.json().await.map_err(|e| format!("Response parse error: {e}"))?;
    let raw_text = json_resp["content"][0]["text"]
        .as_str()
        .ok_or_else(|| "No text in API response".to_string())?;

    let clean = strip_code_fences(raw_text);
    let fixed = clean.replace("\">", "\":");
    let clean = fixed.as_str();

    let points: Vec<SyncPoint> = if has_detection {
        let timing: Vec<TimingPoint> = serde_json::from_str(clean)
            .map_err(|e| format!("JSON parse error: {e}\nRaw: {clean}"))?;
        timing
            .into_iter()
            .filter_map(|tp| {
                let page_idx = tp.page as usize;
                let row_idx = (tp.row as usize).saturating_sub(1); // 1-indexed → 0-indexed
                // Use scroll_anchor (not string_center) so the viewport scrolls to
                // show chord names at the top, not the middle of the string band.
                let y_frac = page_systems.get(page_idx)?.get(row_idx)?.scroll_anchor;
                Some(SyncPoint { page: tp.page, y_offset_px: y_frac, time_secs: tp.time_secs })
            })
            .collect()
    } else {
        let raw: Vec<RawSyncPoint> = serde_json::from_str(clean)
            .map_err(|e| format!("JSON parse error: {e}\nRaw: {clean}"))?;
        raw.into_iter()
            .map(|r| SyncPoint { page: r.page, y_offset_px: r.y_frac, time_secs: r.time_secs })
            .collect()
    };

    let _ = tokio::fs::remove_dir_all(&llm_dir).await;

    Ok(points)
}

// Response structs

#[derive(serde::Deserialize)]
struct TimingPoint {
    page: u32,
    row: u32,
    time_secs: f32,
}

#[derive(serde::Deserialize)]
struct RawSyncPoint {
    page: u32,
    y_frac: f32,
    time_secs: f32,
}

fn strip_code_fences(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix("```json").unwrap_or(s);
    let s = s.strip_prefix("```").unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::Source;

    /// End-to-end sync analysis test using test_files/martian.pdf + martian.flac.
    /// Requires ANTHROPIC_API_KEY env var and pdftoppm to be installed.
    /// Run with: cargo test test_sync_analysis_martian -- --nocapture
    #[tokio::test]
    async fn test_sync_analysis_martian() {
        let api_key = match std::env::var("ANTHROPIC_API_KEY") {
            Ok(k) => k,
            Err(_) => {
                println!("SKIP: ANTHROPIC_API_KEY not set");
                return;
            }
        };

        let root = env!("CARGO_MANIFEST_DIR");
        let pdf_path = format!("{root}/test_files/martian.pdf");
        let flac_path = format!("{root}/test_files/martian.flac");

        assert!(
            std::path::Path::new(&pdf_path).exists(),
            "test_files/martian.pdf not found"
        );
        assert!(
            std::path::Path::new(&flac_path).exists(),
            "test_files/martian.flac not found — add the file to test_files/"
        );

        let duration_secs = {
            let file = std::fs::File::open(&flac_path).expect("failed to open martian.flac");
            let decoder = rodio::Decoder::new(std::io::BufReader::new(file))
                .expect("failed to decode martian.flac");
            decoder
                .total_duration()
                .map(|d| d.as_secs_f32())
                .unwrap_or_else(|| {
                    println!("Warning: could not read duration from flac, defaulting to 300s");
                    300.0
                })
        };
        println!("Audio duration: {duration_secs:.1}s");

        let result = analyze_tab_sync(
            api_key,
            "claude-sonnet-4-6".to_string(),
            pdf_path,
            "test-martian".to_string(),
            duration_secs,
        )
        .await;

        match &result {
            Ok(pts) => println!("Received {} sync points", pts.len()),
            Err(e) => println!("FAILED: {e}"),
        }

        let points = result.expect("sync analysis should succeed");

        assert!(!points.is_empty(), "expected at least one sync point");

        for (i, pt) in points.iter().enumerate() {
            assert!(pt.page < 4, "point {i}: page {} out of range", pt.page);
            assert!(
                (0.0..=1.0).contains(&pt.y_offset_px),
                "point {i}: y_frac {} not in [0, 1]",
                pt.y_offset_px
            );
            assert!(pt.time_secs >= 0.0, "point {i}: negative time_secs {}", pt.time_secs);
            assert!(
                pt.time_secs <= duration_secs + 5.0,
                "point {i}: time_secs {:.1} exceeds audio duration {duration_secs:.1}",
                pt.time_secs
            );
        }

        println!("Sync points:");
        for pt in &points {
            println!(
                "  page={} y_frac={:.3} time={:.1}s",
                pt.page, pt.y_offset_px, pt.time_secs
            );
        }
    }
}
