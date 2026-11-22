use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use anyhow::Result;

fn default_id() -> String {
    format!("{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}
fn default_unknown() -> String { "Unknown".to_string() }
fn default_sample_rate() -> u32 { 44100 }
fn default_bits_per_sample() -> u32 { 16 }
fn default_format() -> String { "UNKNOWN".to_string() }
fn default_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRecord {
    #[serde(default = "default_id")]
    pub id: String,
    #[serde(rename = "filePath", alias = "path", default)]
    pub path: String,
    #[serde(default = "default_unknown")]
    pub title: String,
    #[serde(default = "default_unknown")]
    pub artist: String,
    #[serde(rename = "albumArtist", alias = "album_artist", default)]
    pub album_artist: String,
    #[serde(default = "default_unknown")]
    pub album: String,
    #[serde(default)]
    pub year: Option<u32>,
    #[serde(default = "default_unknown")]
    pub genre: String,
    #[serde(rename = "trackNumber", alias = "track_number", default)]
    pub track_number: Option<u32>,
    #[serde(rename = "diskNumber", alias = "disk_number", default)]
    pub disk_number: Option<u32>,
    #[serde(default)]
    pub duration: f64,
    #[serde(rename = "sampleRate", alias = "sample_rate", default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(rename = "bitsPerSample", alias = "bits_per_sample", default = "default_bits_per_sample")]
    pub bits_per_sample: u32,
    #[serde(rename = "formatName", alias = "format", default = "default_format")]
    pub format: String,
    #[serde(rename = "cover", alias = "coverPath", alias = "cover_path", default)]
    pub cover_path: Option<String>,
    #[serde(rename = "dateAdded", alias = "addedAt", alias = "date_added", default = "default_now")]
    pub date_added: u64,
}

pub struct LibraryDatabase {
    conn: std::sync::Mutex<Connection>,
}

impl LibraryDatabase {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: std::sync::Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tracks (
                id TEXT PRIMARY KEY,
                path TEXT UNIQUE,
                title TEXT,
                artist TEXT,
                album_artist TEXT,
                album TEXT,
                year INTEGER,
                genre TEXT,
                track_number INTEGER,
                disk_number INTEGER,
                duration REAL,
                sample_rate INTEGER,
                bits_per_sample INTEGER,
                format TEXT,
                cover_path TEXT,
                date_added INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_genre ON tracks(genre);
            CREATE TABLE IF NOT EXISTS folders (
                path TEXT PRIMARY KEY,
                added_at INTEGER
            );",
        )?;
        Ok(())
    }

    pub fn insert_or_update_track(&self, t: &TrackRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album_artist, album, year, genre, track_number, disk_number, duration, sample_rate, bits_per_sample, format, cover_path, date_added)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(path) DO UPDATE SET
                title = excluded.title,
                artist = excluded.artist,
                album_artist = excluded.album_artist,
                album = excluded.album,
                year = excluded.year,
                genre = excluded.genre,
                track_number = excluded.track_number,
                disk_number = excluded.disk_number,
                duration = excluded.duration,
                sample_rate = excluded.sample_rate,
                bits_per_sample = excluded.bits_per_sample,
                format = excluded.format,
                cover_path = excluded.cover_path;",
            params![
                t.id, t.path, t.title, t.artist, t.album_artist, t.album, t.year, t.genre,
                t.track_number, t.disk_number, t.duration, t.sample_rate, t.bits_per_sample,
                t.format, t.cover_path, t.date_added
            ],
        )?;
        Ok(())
    }

    pub fn get_all_tracks(&self) -> Result<Vec<TrackRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, path, title, artist, album_artist, album, year, genre, track_number, disk_number, duration, sample_rate, bits_per_sample, format, cover_path, date_added FROM tracks ORDER BY album, track_number")?;

        let rows = stmt.query_map([], |row| {
            Ok(TrackRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album_artist: row.get(4)?,
                album: row.get(5)?,
                year: row.get(6)?,
                genre: row.get(7)?,
                track_number: row.get(8)?,
                disk_number: row.get(9)?,
                duration: row.get(10)?,
                sample_rate: row.get(11)?,
                bits_per_sample: row.get(12)?,
                format: row.get(13)?,
                cover_path: row.get(14)?,
                date_added: row.get(15)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn add_folder(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        conn.execute(
            "INSERT OR IGNORE INTO folders (path, added_at) VALUES (?1, ?2)",
            params![path, now],
        )?;
        Ok(())
    }

    pub fn remove_folder(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM folders WHERE path = ?1", params![path])?;
        conn.execute("DELETE FROM tracks WHERE path LIKE ?1", params![format!("{}%", path)])?;
        Ok(())
    }

    pub fn get_folders(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path FROM folders")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }
}
