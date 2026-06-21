use std::path::PathBuf;

/// Render all pages of a PDF to PNG files in a temp directory using pdftoppm.
/// Returns sorted list of rendered page paths.
/// Requires `pdftoppm` from the poppler-utils system package.
pub async fn render_pages(pdf_path: String, song_id: String) -> Result<Vec<PathBuf>, String> {
    let out_dir = std::env::temp_dir().join("monotabe").join(&song_id);
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| format!("Could not create temp dir: {e}"))?;

    // Clear stale pages from a previous render of this song
    let mut entries = tokio::fs::read_dir(&out_dir)
        .await
        .map_err(|e| e.to_string())?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let _ = tokio::fs::remove_file(entry.path()).await;
    }

    let out_prefix = out_dir.join("page");
    let status = tokio::process::Command::new("pdftoppm")
        .args([
            "-r", "150",
            "-png",
            &pdf_path,
            &out_prefix.to_string_lossy(),
        ])
        .status()
        .await
        .map_err(|e| format!("pdftoppm not found (install poppler-utils): {e}"))?;

    if !status.success() {
        return Err(format!("pdftoppm failed with status: {status}"));
    }

    let mut read_dir = tokio::fs::read_dir(&out_dir)
        .await
        .map_err(|e| e.to_string())?;

    let mut pages = Vec::new();
    while let Some(entry) = read_dir.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.extension().map(|x| x == "png").unwrap_or(false) {
            pages.push(path);
        }
    }
    pages.sort();
    Ok(pages)
}
