use rusqlite::{params, Connection, Result};
use std::path::Path;

use crate::model::song::{Instrument, Song};
use crate::model::sync_map::{SyncPoint, TabSyncMap};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(library_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(library_dir).ok();
        let conn = Connection::open(library_dir.join("library.db"))?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS songs (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL,
                artist      TEXT NOT NULL,
                instrument  TEXT NOT NULL,
                youtube_url TEXT,
                spotify_url TEXT,
                pdf_path    TEXT,
                mp3_path    TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_maps (
                song_id    TEXT PRIMARY KEY,
                map_json   TEXT NOT NULL,
                model_used TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE
            );
            ",
        )
    }

    pub fn insert_song(&self, song: &Song) -> Result<()> {
        self.conn.execute(
            "INSERT INTO songs
             (id, title, artist, instrument, youtube_url, spotify_url, pdf_path, mp3_path, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                song.id,
                song.title,
                song.artist,
                song.instrument.to_string(),
                song.youtube_url,
                song.spotify_url,
                song.pdf_path,
                song.mp3_path,
                song.created_at,
                song.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_song(&self, song: &Song) -> Result<()> {
        self.conn.execute(
            "UPDATE songs SET title=?1, artist=?2, instrument=?3,
             youtube_url=?4, spotify_url=?5, pdf_path=?6, mp3_path=?7,
             updated_at=?8 WHERE id=?9",
            params![
                song.title,
                song.artist,
                song.instrument.to_string(),
                song.youtube_url,
                song.spotify_url,
                song.pdf_path,
                song.mp3_path,
                song.updated_at,
                song.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_song(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM songs WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn all_songs(&self) -> Result<Vec<Song>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, artist, instrument, youtube_url, spotify_url,
                    pdf_path, mp3_path, created_at, updated_at
             FROM songs ORDER BY artist, title",
        )?;
        let songs = stmt
            .query_map([], |row| {
                let instr: String = row.get(3)?;
                Ok(Song {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    instrument: Instrument::from_str(&instr).unwrap_or(Instrument::Guitar),
                    youtube_url: row.get(4)?,
                    spotify_url: row.get(5)?,
                    pdf_path: row.get(6)?,
                    mp3_path: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(songs)
    }

    pub fn save_sync_map(&self, map: &TabSyncMap) -> Result<()> {
        let json = serde_json::to_string(&map.points).unwrap_or_default();
        let now = unix_now();
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_maps (song_id, map_json, model_used, created_at)
             VALUES (?1,?2,?3,?4)",
            params![map.song_id, json, map.model_used, now],
        )?;
        Ok(())
    }

    pub fn load_sync_map(&self, song_id: &str) -> Result<Option<TabSyncMap>> {
        let mut stmt = self.conn.prepare(
            "SELECT song_id, map_json, model_used FROM sync_maps WHERE song_id=?1",
        )?;
        let result = stmt.query_row(params![song_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });
        match result {
            Ok((sid, map_json, model_used)) => {
                let points: Vec<SyncPoint> = serde_json::from_str(&map_json).unwrap_or_default();
                Ok(Some(TabSyncMap { song_id: sid, points, model_used }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
