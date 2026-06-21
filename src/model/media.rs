use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaSource {
    YoutubeUrl(String),
    SpotifyUrl(String),
    PdfPath(String),
    Mp3Path(String),
}
