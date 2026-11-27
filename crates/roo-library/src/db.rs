use rusqlite::{params, Connection};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
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

fn de_string_or_num<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Ok(default_id()),
    }
}

fn de_opt_string_or_num<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { Ok(None) } else { Ok(Some(t.to_string())) }
        }
        serde_json::Value::Number(n) => Ok(Some(n.to_string())),
        _ => Ok(None),
    }
}

fn de_opt_u32_flexible<'de, D>(deserializer: D) -> std::result::Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Number(n) => Ok(n.as_u64().map(|x| x as u32).or_else(|| n.as_f64().map(|x| x as u32))),
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                // Support "1994" or "1994-05-01"
                let prefix: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
                Ok(prefix.parse::<u32>().ok())
            }
        }
        _ => Ok(None),
    }
}

fn de_u32_sample_rate<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(de_opt_u32_flexible(deserializer)?.filter(|&x| x > 0).unwrap_or(44100))
}

fn de_u32_bits<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(de_opt_u32_flexible(deserializer)?.filter(|&x| x > 0).unwrap_or(16))
}

fn de_f64_flexible<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Ok(s.trim().parse::<f64>().unwrap_or(0.0)),
        _ => Ok(0.0),
    }
}

fn de_u64_flexible<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => Ok(n.as_u64().or_else(|| n.as_f64().map(|x| x as u64)).unwrap_or_else(default_now)),
        serde_json::Value::String(s) => Ok(s.trim().parse::<u64>().unwrap_or_else(|_| default_now())),
        _ => Ok(default_now()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackRecord {
    #[serde(default = "default_id", deserialize_with = "de_string_or_num")]
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
    #[serde(default, deserialize_with = "de_opt_u32_flexible")]
    pub year: Option<u32>,
    #[serde(default = "default_unknown")]
    pub genre: String,
    #[serde(rename = "trackNumber", alias = "track_number", default, deserialize_with = "de_opt_u32_flexible")]
    pub track_number: Option<u32>,
    #[serde(rename = "diskNumber", alias = "disk_number", default, deserialize_with = "de_opt_u32_flexible")]
    pub disk_number: Option<u32>,
    #[serde(default, deserialize_with = "de_f64_flexible")]
    pub duration: f64,
    #[serde(rename = "sampleRate", alias = "sample_rate", default = "default_sample_rate", deserialize_with = "de_u32_sample_rate")]
    pub sample_rate: u32,
    #[serde(rename = "bitsPerSample", alias = "bits_per_sample", default = "default_bits_per_sample", deserialize_with = "de_u32_bits")]
    pub bits_per_sample: u32,
    #[serde(rename = "formatName", alias = "format", default = "default_format")]
    pub format: String,
    #[serde(rename = "cover", alias = "coverPath", alias = "cover_path", default)]
    pub cover_path: Option<String>,
    #[serde(rename = "dateAdded", alias = "addedAt", alias = "date_added", default = "default_now", deserialize_with = "de_u64_flexible")]
    pub date_added: u64,
    #[serde(rename = "isTidal", alias = "is_tidal", default)]
    pub is_tidal: bool,
    #[serde(rename = "tidalId", alias = "tidal_id", default, deserialize_with = "de_opt_string_or_num")]
    pub tidal_id: Option<String>,
    #[serde(rename = "tidalAlbumId", alias = "tidal_album_id", alias = "albumId", default, deserialize_with = "de_opt_string_or_num")]
    pub tidal_album_id: Option<String>,
    #[serde(rename = "audioQuality", alias = "audio_quality", default)]
    pub audio_quality: Option<String>,
}

impl TrackRecord {
    /// Parse a TrackRecord from an arbitrary frontend JSON object without failing on
    /// duplicate alias keys (e.g. `filePath` + `path`, `cover` + `coverPath`, `formatName` + `format`)
    /// or JS type variations (string vs number year/id).
    pub fn from_json_value(v: &serde_json::Value) -> Self {
        let str_any = |keys: &[&str]| -> Option<String> {
            for k in keys {
                if let Some(val) = v.get(*k) {
                    match val {
                        serde_json::Value::String(s) => {
                            let t = s.trim();
                            if !t.is_empty() {
                                return Some(t.to_string());
                            }
                        }
                        serde_json::Value::Number(n) => return Some(n.to_string()),
                        _ => {}
                    }
                }
            }
            None
        };

        let opt_u32 = |keys: &[&str]| -> Option<u32> {
            for k in keys {
                if let Some(val) = v.get(*k) {
                    match val {
                        serde_json::Value::Number(n) => {
                            if let Some(x) = n.as_u64() { return Some(x as u32); }
                            if let Some(x) = n.as_f64() { return Some(x as u32); }
                        }
                        serde_json::Value::String(s) => {
                            let prefix: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
                            if let Ok(x) = prefix.parse::<u32>() {
                                return Some(x);
                            }
                        }
                        _ => {}
                    }
                }
            }
            None
        };

        let f64_any = |keys: &[&str]| -> f64 {
            for k in keys {
                if let Some(val) = v.get(*k) {
                    match val {
                        serde_json::Value::Number(n) => if let Some(x) = n.as_f64() { return x; },
                        serde_json::Value::String(s) => if let Ok(x) = s.trim().parse::<f64>() { return x; },
                        _ => {}
                    }
                }
            }
            0.0
        };

        let u64_any = |keys: &[&str]| -> Option<u64> {
            for k in keys {
                if let Some(val) = v.get(*k) {
                    match val {
                        serde_json::Value::Number(n) => {
                            if let Some(x) = n.as_u64() { return Some(x); }
                            if let Some(x) = n.as_f64() { return Some(x as u64); }
                        }
                        serde_json::Value::String(s) => {
                            if let Ok(x) = s.trim().parse::<u64>() { return Some(x); }
                        }
                        _ => {}
                    }
                }
            }
            None
        };

        let path = str_any(&["filePath", "path"]).unwrap_or_default();
        let tidal_id = str_any(&["tidalId", "tidal_id"]);
        let tidal_album_id = str_any(&["tidalAlbumId", "tidal_album_id", "albumId"]);
        let is_tidal_flag = v.get("isTidal")
            .or_else(|| v.get("is_tidal"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let is_tidal = is_tidal_flag || tidal_id.is_some() || path.starts_with("tidal://");
        let id = str_any(&["id"]).unwrap_or_else(|| {
            if let Some(ref tid) = tidal_id {
                format!("tidal_{}", tid)
            } else {
                default_id()
            }
        });

        TrackRecord {
            id,
            path,
            title: str_any(&["title"]).unwrap_or_else(default_unknown),
            artist: str_any(&["artist"]).unwrap_or_else(default_unknown),
            album_artist: str_any(&["albumArtist", "album_artist"]).unwrap_or_default(),
            album: str_any(&["album"]).unwrap_or_else(default_unknown),
            year: opt_u32(&["year"]),
            genre: str_any(&["genre"]).unwrap_or_else(default_unknown),
            track_number: opt_u32(&["trackNumber", "track_number"]),
            disk_number: opt_u32(&["diskNumber", "discNumber", "disk_number"]),
            duration: f64_any(&["duration"]),
            sample_rate: opt_u32(&["sampleRate", "sample_rate"]).filter(|&x| x > 0).unwrap_or(44100),
            bits_per_sample: opt_u32(&["bitsPerSample", "bits_per_sample"]).filter(|&x| x > 0).unwrap_or(16),
            format: str_any(&["formatName", "format"]).unwrap_or_else(default_format),
            cover_path: str_any(&["cover", "coverPath", "cover_path"]),
            date_added: u64_any(&["dateAdded", "addedAt", "date_added"]).unwrap_or_else(default_now),
            is_tidal,
            tidal_id,
            tidal_album_id,
            audio_quality: str_any(&["audioQuality", "audio_quality"]),
        }
    }

    /// Ensure every track (especially TIDAL streaming tracks with empty filesystem paths)
    /// has a stable, unique `path` key for SQLite `UNIQUE(path)` upsert semantics.
    pub fn effective_path(&self) -> String {
        let trimmed = self.path.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
        if self.is_tidal || self.tidal_id.is_some() {
            let tid = self.tidal_id.as_deref().unwrap_or(&self.id);
            return format!("tidal://track/{}", tid);
        }
        format!("track://{}", self.id)
    }
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
        let _ = conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -16000;",
        );
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
                date_added INTEGER,
                is_tidal INTEGER DEFAULT 0,
                tidal_id TEXT,
                tidal_album_id TEXT,
                audio_quality TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_genre ON tracks(genre);
            CREATE TABLE IF NOT EXISTS folders (
                path TEXT PRIMARY KEY,
                added_at INTEGER
            );",
        )?;
        // Safe schema migration for existing SQLite databases created before TIDAL columns existed
        let _ = conn.execute("ALTER TABLE tracks ADD COLUMN is_tidal INTEGER DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE tracks ADD COLUMN tidal_id TEXT", []);
        let _ = conn.execute("ALTER TABLE tracks ADD COLUMN tidal_album_id TEXT", []);
        let _ = conn.execute("ALTER TABLE tracks ADD COLUMN audio_quality TEXT", []);
        Ok(())
    }

    fn upsert_track_with_conn(conn: &Connection, t: &TrackRecord) -> Result<()> {
        let eff_path = t.effective_path();
        let is_tidal_int: i64 = if t.is_tidal || t.tidal_id.is_some() || eff_path.starts_with("tidal://") { 1 } else { 0 };
        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO tracks (id, path, title, artist, album_artist, album, year, genre, track_number, disk_number, duration, sample_rate, bits_per_sample, format, cover_path, date_added, is_tidal, tidal_id, tidal_album_id, audio_quality)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20);",
        )?;
        stmt.execute(params![
            t.id, eff_path, t.title, t.artist, t.album_artist, t.album, t.year, t.genre,
            t.track_number, t.disk_number, t.duration, t.sample_rate, t.bits_per_sample,
            t.format, t.cover_path, t.date_added, is_tidal_int, t.tidal_id, t.tidal_album_id, t.audio_quality
        ])?;
        Ok(())
    }

    pub fn insert_or_update_track(&self, t: &TrackRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        Self::upsert_track_with_conn(&conn, t)
    }

    /// Insert or update a batch of tracks inside a single SQLite transaction
    /// (orders of magnitude faster than per-row autocommit during library import).
    pub fn bulk_upsert_tracks(&self, tracks: &[TrackRecord]) -> Result<usize> {
        if tracks.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut count = 0usize;
        for t in tracks {
            Self::upsert_track_with_conn(&tx, t)?;
            count += 1;
        }
        tx.commit()?;
        Ok(count)
    }

    /// Persist all tracks (upserting local & TIDAL tracks) and prune any previously stored
    /// TIDAL tracks that were removed from the library.
    pub fn sync_library_tracks(&self, tracks: &[TrackRecord]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let mut keep_tidal_paths: HashSet<String> = HashSet::new();
        for t in tracks {
            let eff_path = t.effective_path();
            if t.is_tidal || t.tidal_id.is_some() || eff_path.starts_with("tidal://") {
                keep_tidal_paths.insert(eff_path);
            }
            let _ = Self::upsert_track_with_conn(&tx, t);
        }

        // Remove TIDAL tracks that are no longer in the saved library list
        let existing_tidal_paths: Vec<String> = {
            let mut stmt = tx.prepare("SELECT path FROM tracks WHERE is_tidal = 1 OR path LIKE 'tidal://%'")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).collect()
        };

        for old_path in existing_tidal_paths {
            if !keep_tidal_paths.contains(&old_path) {
                let _ = tx.execute("DELETE FROM tracks WHERE path = ?1", params![old_path]);
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_all_tracks(&self) -> Result<Vec<TrackRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, title, artist, album_artist, album, year, genre, track_number, disk_number, duration, sample_rate, bits_per_sample, format, cover_path, date_added, COALESCE(is_tidal, 0), tidal_id, tidal_album_id, audio_quality
             FROM tracks ORDER BY album, track_number"
        )?;

        let rows = stmt.query_map([], |row| {
            let is_tidal_int: i64 = row.get(16)?;
            let path: String = row.get(1)?;
            Ok(TrackRecord {
                id: row.get(0)?,
                is_tidal: is_tidal_int != 0 || path.starts_with("tidal://"),
                path,
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
                tidal_id: row.get(17)?,
                tidal_album_id: row.get(18)?,
                audio_quality: row.get(19)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tidal_tracks_persist_and_remove() {
        let db = LibraryDatabase::in_memory().unwrap();

        // Simulate JS payloads from frontend (string year, numeric tidalId, empty filePath)
        let js_tracks: Vec<TrackRecord> = serde_json::from_value(serde_json::json!([
            {
                "id": "local_1",
                "filePath": "D:\\Music\\test.flac",
                "title": "Local Song",
                "artist": "Local Artist",
                "album": "Local Album",
                "year": 2020,
                "duration": 210.5
            },
            {
                "id": "tidal_1001",
                "filePath": "",
                "title": "Tidal Song 1",
                "artist": "Rebecca Pidgeon",
                "album": "The Raven",
                "year": "1994",
                "trackNumber": 1,
                "duration": 180,
                "isTidal": true,
                "tidalId": 1001,
                "tidalAlbumId": 5555,
                "audioQuality": "LOSSLESS",
                "cover": "https://resources.tidal.com/images/test.jpg"
            },
            {
                "id": "tidal_1002",
                "filePath": "",
                "title": "Tidal Song 2",
                "artist": "Rebecca Pidgeon",
                "album": "The Raven",
                "year": "1994-05-01",
                "trackNumber": 2,
                "duration": 240,
                "isTidal": true,
                "tidalId": "1002",
                "tidalAlbumId": "5555",
                "audioQuality": "HI_RES_LOSSLESS"
            }
        ])).unwrap();

        db.sync_library_tracks(&js_tracks).unwrap();

        let loaded = db.get_all_tracks().unwrap();
        assert_eq!(loaded.len(), 3, "All 3 tracks (1 local + 2 TIDAL) must persist without path collision");

        let tidal_loaded: Vec<&TrackRecord> = loaded.iter().filter(|t| t.is_tidal).collect();
        assert_eq!(tidal_loaded.len(), 2);
        assert_eq!(tidal_loaded[0].tidal_album_id.as_deref(), Some("5555"));
        assert_eq!(tidal_loaded[0].year, Some(1994));
        assert_eq!(tidal_loaded[0].audio_quality.as_deref(), Some("LOSSLESS"));
        assert_eq!(tidal_loaded[1].audio_quality.as_deref(), Some("HI_RES_LOSSLESS"));

        // Now remove the TIDAL album (sync with only local track)
        db.sync_library_tracks(&js_tracks[0..1]).unwrap();
        let after_remove = db.get_all_tracks().unwrap();
        assert_eq!(after_remove.len(), 1);
        assert!(!after_remove[0].is_tidal);
    }

    #[test]
    fn test_from_json_value_with_duplicate_shim_aliases() {
        let db = LibraryDatabase::in_memory().unwrap();
        // Exact shape produced by frontend/tauri-shim.js normalizeShimTrack()
        let raw = serde_json::json!([
            {
                "id": "tidal_777",
                "filePath": "",
                "path": "",
                "title": "Spanish Harlem",
                "artist": "Rebecca Pidgeon",
                "album": "The Raven",
                "year": "1994",
                "trackNumber": 2,
                "discNumber": 1,
                "duration": 225,
                "cover": "https://resources.tidal.com/images/abc.jpg",
                "coverPath": "https://resources.tidal.com/images/abc.jpg",
                "formatName": "FLAC",
                "format": "FLAC",
                "isTidal": true,
                "tidalId": 777,
                "tidalAlbumId": 9999,
                "albumId": 9999,
                "audioQuality": "HI_RES_LOSSLESS"
            }
        ]);
        let recs: Vec<TrackRecord> = raw.as_array().unwrap().iter().map(TrackRecord::from_json_value).collect();
        db.sync_library_tracks(&recs).unwrap();

        let loaded = db.get_all_tracks().unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].is_tidal);
        assert_eq!(loaded[0].path, "tidal://track/777");
        assert_eq!(loaded[0].tidal_id.as_deref(), Some("777"));
        assert_eq!(loaded[0].tidal_album_id.as_deref(), Some("9999"));
        assert_eq!(loaded[0].disk_number, Some(1));
        assert_eq!(loaded[0].cover_path.as_deref(), Some("https://resources.tidal.com/images/abc.jpg"));
    }

    #[test]
    fn test_bulk_upsert_tracks_transaction_speed_and_accuracy() {
        let db = LibraryDatabase::in_memory().unwrap();
        let mut batch = Vec::with_capacity(1000);
        for i in 0..1000 {
            let raw = serde_json::json!({
                "id": format!("trk_{}", i),
                "filePath": format!("D:\\Music\\Album\\{:04}.flac", i),
                "path": format!("D:\\Music\\Album\\{:04}.flac", i),
                "title": format!("Track {}", i),
                "artist": "Test Artist",
                "album": "Test Album",
                "sampleRate": 192000,
                "bitsPerSample": 24,
                "dateAdded": 1700000000000u64 + i as u64,
                "addedAt": 1700000000000u64 + i as u64
            });
            batch.push(TrackRecord::from_json_value(&raw));
        }
        let t0 = std::time::Instant::now();
        let inserted = db.bulk_upsert_tracks(&batch).unwrap();
        let elapsed = t0.elapsed();
        assert_eq!(inserted, 1000);
        assert_eq!(db.get_all_tracks().unwrap().len(), 1000);
        assert!(elapsed.as_millis() < 2000, "Bulk import took too long: {:?}", elapsed);
    }
}
