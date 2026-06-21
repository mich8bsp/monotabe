use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPoint {
    pub page: u32,
    pub y_offset_px: f32,
    pub time_secs: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSyncMap {
    pub song_id: String,
    pub points: Vec<SyncPoint>,
    pub model_used: String,
}

impl TabSyncMap {
    /// Returns the interpolated (page, y_offset_px) for the given playback time.
    pub fn scroll_position_at(&self, time_secs: f32) -> Option<(u32, f32)> {
        if self.points.is_empty() {
            return None;
        }
        let idx = self.points.partition_point(|p| p.time_secs <= time_secs);
        if idx == 0 {
            let p = &self.points[0];
            return Some((p.page, p.y_offset_px));
        }
        if idx >= self.points.len() {
            let p = self.points.last().unwrap();
            return Some((p.page, p.y_offset_px));
        }
        let before = &self.points[idx - 1];
        let after = &self.points[idx];
        // Don't interpolate across page boundaries — the y_frac would go backwards.
        // Hold at `before` position; the hard jump to the next page happens naturally
        // when time crosses `after.time_secs` and idx advances.
        if before.page != after.page {
            return Some((before.page, before.y_offset_px));
        }
        let t = (time_secs - before.time_secs) / (after.time_secs - before.time_secs);
        Some((before.page, before.y_offset_px + t * (after.y_offset_px - before.y_offset_px)))
    }
}
