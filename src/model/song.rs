use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instrument {
    Guitar,
    Bass,
}

impl std::fmt::Display for Instrument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instrument::Guitar => write!(f, "guitar"),
            Instrument::Bass => write!(f, "bass"),
        }
    }
}

impl Instrument {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "guitar" => Some(Instrument::Guitar),
            "bass" => Some(Instrument::Bass),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub instrument: Instrument,
    pub youtube_url: Option<String>,
    pub spotify_url: Option<String>,
    pub pdf_path: Option<String>,
    pub mp3_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Song {
    pub fn new(title: String, artist: String, instrument: Instrument) -> Self {
        let now = unix_now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            artist,
            instrument,
            youtube_url: None,
            spotify_url: None,
            pdf_path: None,
            mp3_path: None,
            created_at: now,
            updated_at: now,
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum InstrumentFilter {
    #[default]
    All,
    Guitar,
    Bass,
}
