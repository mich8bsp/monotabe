use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

use crate::model::sync_map::SyncPoint;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Returns ANTHROPIC_API_KEY from environment, or an error string.
pub fn api_key() -> Result<String, String> {
    std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| "ANTHROPIC_API_KEY environment variable not set".to_string())
}

/// Render PDF pages at 72 DPI (for LLM), build the Claude API request,
/// and return parsed SyncPoints (y_frac in [0..1] scaled to display 150 DPI).
pub async fn analyze_tab_sync(
    api_key: String,
    model: String,
    pdf_path: String,
    song_id: String,
    audio_duration_secs: f32,
) -> Result<Vec<SyncPoint>, String> {
    // Render low-res pages for LLM (72 DPI to stay within API limits)
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

    // Build message content: text intro + one image block per page
    let mut content: Vec<Value> = vec![json!({
        "type": "text",
        "text": format!(
            "You are analyzing {n_pages} page(s) of a guitar/bass tablature PDF.\n\
             The audio recording is exactly {audio_duration_secs:.1} seconds long.\n\
             \n\
             Your job is to produce a precise timing map so the tab can auto-scroll in sync with the audio.\n\
             \n\
             STEP 1 — Count measures:\n\
             Look at each page carefully. In guitar/bass tab, measures are separated by VERTICAL BAR LINES\n\
             that cross all 6 (guitar) or 4 (bass) strings. Count every measure on every page.\n\
             \n\
             STEP 2 — Find tempo:\n\
             Look for a BPM or tempo marking (e.g. '♩= 120', 'q=90', 'Tempo: 100 BPM') near the top of page 0.\n\
             If found: seconds_per_measure = (60.0 / BPM) * beats_per_bar (beats_per_bar is usually 4).\n\
             If NOT found: seconds_per_measure = {audio_duration_secs:.1} / total_measure_count.\n\
             \n\
             STEP 3 — Compute y positions:\n\
             For each measure, compute y_frac = the vertical center of that measure's staff row,\n\
             as a fraction of the full page height (0.0 = very top, 1.0 = very bottom).\n\
             Measures within the same staff row share a y_frac. Rows are stacked vertically down the page.\n\
             DO NOT assign y_frac values uniformly — base them on actual visual row positions in the image.\n\
             \n\
             STEP 4 — Handle repeats:\n\
             If the tab has repeat signs (||: and :||) or a D.C./D.S., include repeated sections with\n\
             the correct time_secs for each pass.\n\
             \n\
             Return ONLY a raw JSON array, one object per measure, no explanation, no markdown:\n\
             [{{\"page\":0,\"y_frac\":0.05,\"time_secs\":0.0}}, ...]\n\
             \n\
             Rules:\n\
             - page is 0-indexed\n\
             - First entry MUST be {{\"page\":0,\"y_frac\":0.0,\"time_secs\":0.0}}\n\
             - time_secs must be strictly increasing\n\
             - Last entry time_secs must be close to {audio_duration_secs:.1}\n\
             - y_frac values MUST reflect the actual visual row positions, not be evenly spaced"
        )
    })];

    for (i, path) in pages.iter().enumerate() {
        let bytes = tokio::fs::read(path).await.map_err(|e| e.to_string())?;
        let encoded = STANDARD.encode(&bytes);
        content.push(json!({
            "type": "text",
            "text": format!("Page {i}:")
        }));
        content.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": "image/png",
                "data": encoded
            }
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

    let json_resp: Value = resp.json().await.map_err(|e| format!("Response parse error: {e}"))?;
    let raw_text = json_resp["content"][0]["text"]
        .as_str()
        .ok_or_else(|| "No text in API response".to_string())?;

    // Claude might wrap JSON in a code block; strip if needed
    let clean = strip_code_fences(raw_text);

    // Claude occasionally emits `"key">value` instead of `"key":value` when
    // expressing an approximate number. Fix it before parsing.
    let fixed = clean.replace("\">", "\":");
    let clean = fixed.as_str();

    let raw_points: Vec<RawSyncPoint> =
        serde_json::from_str(clean).map_err(|e| format!("JSON parse error: {e}\nRaw: {clean}"))?;

    // Convert y_frac → y_offset_px at 150 DPI (standard letter: 1650px high)
    // We scale from 72 DPI reference: page_height_72dpi ≈ 792px → at 150 DPI ≈ 1650px
    // Since y_frac is dimensionless, no DPI scaling needed — we store y_frac directly.
    // The auto-scroll code will multiply by actual rendered page height.
    let points = raw_points
        .into_iter()
        .map(|r| SyncPoint {
            page: r.page,
            y_offset_px: r.y_frac, // stored as fraction; scroll code scales by page height
            time_secs: r.time_secs,
        })
        .collect();

    // Clean up LLM temp dir
    let _ = tokio::fs::remove_dir_all(&llm_dir).await;

    Ok(points)
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

        // Read duration from the flac file using rodio/symphonia
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

        // martian.pdf has 4 pages
        for (i, pt) in points.iter().enumerate() {
            assert!(
                pt.page < 4,
                "point {i}: page {} out of range (pdf has 4 pages)",
                pt.page
            );
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
