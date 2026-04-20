use rusqlite::{Connection, params};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

use crate::types::{AppError, AppState, Track};

/// Initialize the SQLite database in the app data directory.
/// Creates the DB file and runs schema migrations on first run.
pub fn init_db(app_handle: &tauri::AppHandle) -> Result<Connection, AppError> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Io(e.to_string()))?;

    std::fs::create_dir_all(&app_data_dir)?;

    let db_path = app_data_dir.join("library.db");
    let conn = Connection::open(&db_path)
        .map_err(|e| AppError::Database(format!("Failed to open database: {}", e)))?;

    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tracks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            path            TEXT NOT NULL UNIQUE,
            dir_path        TEXT NOT NULL,
            filename        TEXT NOT NULL,
            title           TEXT,
            artist          TEXT,
            album           TEXT,
            album_artist    TEXT,
            year            INTEGER,
            genre           TEXT,
            track_number    INTEGER,
            disc_number     INTEGER,
            duration_secs   REAL,
            cover_art_path  TEXT,
            missing         INTEGER NOT NULL DEFAULT 0,
            indexed_at      INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_tracks_dir    ON tracks(dir_path);
        CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
        CREATE INDEX IF NOT EXISTS idx_tracks_album  ON tracks(album);

        CREATE TABLE IF NOT EXISTS app_state (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tags (
            id    INTEGER PRIMARY KEY AUTOINCREMENT,
            name  TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT '#6366f1'
        );

        CREATE TABLE IF NOT EXISTS track_tags (
            track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
            tag_id   INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
            PRIMARY KEY (track_id, tag_id)
        );
        ",
    )
    .map_err(|e| AppError::Database(format!("Schema creation failed: {}", e)))?;

    Ok(())
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Insert a track. If the path already exists, the insert is ignored.
/// Returns the rowid of the inserted (or existing) row.
pub fn insert_track(conn: &Connection, track: &Track) -> Result<i64, AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO tracks
            (path, dir_path, filename, title, artist, album, album_artist,
             year, genre, track_number, disc_number, duration_secs, cover_art_path,
             missing, indexed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            track.path,
            track.dir_path,
            track.filename,
            track.title,
            track.artist,
            track.album,
            track.album_artist,
            track.year,
            track.genre,
            track.track_number,
            track.disc_number,
            track.duration_secs,
            track.cover_art_path,
            track.missing as i64,
            now_unix(),
        ],
    )?;

    // Return the id of the row (existing or newly inserted)
    let id: i64 = conn.query_row(
        "SELECT id FROM tracks WHERE path = ?1",
        params![track.path],
        |row| row.get(0),
    )?;

    Ok(id)
}

/// Update an existing track record identified by its path.
pub fn update_track(conn: &Connection, track: &Track) -> Result<(), AppError> {
    conn.execute(
        "UPDATE tracks SET
            dir_path       = ?1,
            filename       = ?2,
            title          = ?3,
            artist         = ?4,
            album          = ?5,
            album_artist   = ?6,
            year           = ?7,
            genre          = ?8,
            track_number   = ?9,
            disc_number    = ?10,
            duration_secs  = ?11,
            cover_art_path = ?12,
            missing        = ?13,
            indexed_at     = ?14
         WHERE path = ?15",
        params![
            track.dir_path,
            track.filename,
            track.title,
            track.artist,
            track.album,
            track.album_artist,
            track.year,
            track.genre,
            track.track_number,
            track.disc_number,
            track.duration_secs,
            track.cover_art_path,
            track.missing as i64,
            now_unix(),
            track.path,
        ],
    )?;

    Ok(())
}

/// Return all tracks in the library.
pub fn get_all_tracks(conn: &Connection) -> Result<Vec<Track>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, path, dir_path, filename, title, artist, album, album_artist,
                year, genre, track_number, disc_number, duration_secs, cover_art_path, missing
         FROM tracks
         ORDER BY dir_path, filename",
    )?;

    let tracks = stmt
        .query_map([], |row| {
            Ok(Track {
                id: row.get(0)?,
                path: row.get(1)?,
                dir_path: row.get(2)?,
                filename: row.get(3)?,
                title: row.get(4)?,
                artist: row.get(5)?,
                album: row.get(6)?,
                album_artist: row.get(7)?,
                year: row.get(8)?,
                genre: row.get(9)?,
                track_number: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
                disc_number: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
                duration_secs: row.get(12)?,
                cover_art_path: row.get(13)?,
                missing: row.get::<_, i64>(14)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tracks)
}

/// Fetch a single track by its id.
pub fn get_track_by_id(conn: &Connection, id: i64) -> Result<Option<Track>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, path, dir_path, filename, title, artist, album, album_artist,
                year, genre, track_number, disc_number, duration_secs, cover_art_path, missing
         FROM tracks WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        Ok(Track {
            id: row.get(0)?,
            path: row.get(1)?,
            dir_path: row.get(2)?,
            filename: row.get(3)?,
            title: row.get(4)?,
            artist: row.get(5)?,
            album: row.get(6)?,
            album_artist: row.get(7)?,
            year: row.get(8)?,
            genre: row.get(9)?,
            track_number: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
            disc_number: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
            duration_secs: row.get(12)?,
            cover_art_path: row.get(13)?,
            missing: row.get::<_, i64>(14)? != 0,
        })
    })?;

    Ok(rows.next().transpose()?)
}

/// Set the `missing` flag for a track identified by its file path.
pub fn mark_missing(conn: &Connection, path: &str, missing: bool) -> Result<(), AppError> {
    conn.execute(
        "UPDATE tracks SET missing = ?1 WHERE path = ?2",
        params![missing as i64, path],
    )?;
    Ok(())
}

/// Read all app_state keys and construct an AppState.
/// Missing keys fall back to sensible defaults.
pub fn get_app_state(conn: &Connection) -> Result<AppState, AppError> {
    let mut stmt = conn.prepare("SELECT key, value FROM app_state")?;
    let pairs: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    let mut last_track_id: Option<i64> = None;
    let mut last_position_secs: f64 = 0.0;
    let mut volume: f32 = 1.0;
    let mut root_directories: Vec<String> = Vec::new();

    for (key, value) in pairs {
        match key.as_str() {
            "last_track_id" => {
                last_track_id = value.parse::<i64>().ok();
            }
            "last_position_secs" => {
                last_position_secs = value.parse::<f64>().unwrap_or(0.0);
            }
            "volume" => {
                volume = value.parse::<f32>().unwrap_or(1.0);
            }
            "root_directories" => {
                root_directories =
                    serde_json::from_str(&value).unwrap_or_default();
            }
            _ => {}
        }
    }

    Ok(AppState {
        last_track_id,
        last_position_secs,
        volume,
        root_directories,
    })
}

/// Upsert all app_state keys from the given AppState.
pub fn save_app_state(conn: &Connection, state: &AppState) -> Result<(), AppError> {
    let root_dirs_json = serde_json::to_string(&state.root_directories)
        .map_err(|e| AppError::Database(format!("Failed to serialize root_directories: {}", e)))?;

    let pairs: &[(&str, String)] = &[
        (
            "last_track_id",
            state
                .last_track_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        ),
        (
            "last_position_secs",
            state.last_position_secs.to_string(),
        ),
        ("volume", state.volume.to_string()),
        ("root_directories", root_dirs_json),
    ];

    for (key, value) in pairs {
        // Skip storing last_track_id as empty string — use NULL semantics via absence
        if key == &"last_track_id" && value.is_empty() {
            conn.execute("DELETE FROM app_state WHERE key = ?1", params![key])?;
        } else {
            conn.execute(
                "INSERT INTO app_state (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
    }

    Ok(())
}

/// Delete all tracks and reset app_state to defaults.
pub fn reset_library(conn: &Connection) -> Result<(), AppError> {
    conn.execute("DELETE FROM tracks", [])?;
    conn.execute("DELETE FROM app_state", [])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tag functions
// ---------------------------------------------------------------------------

use crate::types::Tag;

pub fn create_tag(conn: &Connection, name: &str, color: &str) -> Result<Tag, AppError> {
    conn.execute(
        "INSERT INTO tags (name, color) VALUES (?1, ?2)",
        params![name, color],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Tag { id, name: name.to_string(), color: color.to_string() })
}

pub fn delete_tag(conn: &Connection, tag_id: i64) -> Result<(), AppError> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![tag_id])?;
    Ok(())
}

pub fn get_all_tags(conn: &Connection) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare("SELECT id, name, color FROM tags ORDER BY name")?;
    let tags = stmt
        .query_map([], |row| Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
        }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tags)
}

pub fn assign_tag(conn: &Connection, track_id: i64, tag_id: i64) -> Result<(), AppError> {
    conn.execute(
        "INSERT OR IGNORE INTO track_tags (track_id, tag_id) VALUES (?1, ?2)",
        params![track_id, tag_id],
    )?;
    Ok(())
}

pub fn remove_tag_from_track(conn: &Connection, track_id: i64, tag_id: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM track_tags WHERE track_id = ?1 AND tag_id = ?2",
        params![track_id, tag_id],
    )?;
    Ok(())
}

/// Returns all tag IDs assigned to a track.
pub fn get_tags_for_track(conn: &Connection, track_id: i64) -> Result<Vec<Tag>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.color FROM tags t
         JOIN track_tags tt ON tt.tag_id = t.id
         WHERE tt.track_id = ?1
         ORDER BY t.name",
    )?;
    let tags = stmt
        .query_map(params![track_id], |row| Ok(Tag {
            id: row.get(0)?,
            name: row.get(1)?,
            color: row.get(2)?,
        }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tags)
}

/// Returns a map of track_id -> Vec<tag_id> for all assignments.
pub fn get_all_track_tag_ids(conn: &Connection) -> Result<Vec<(i64, i64)>, AppError> {
    let mut stmt = conn.prepare("SELECT track_id, tag_id FROM track_tags")?;
    let pairs = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_in_memory() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        create_schema(&conn).expect("schema");
        conn
    }

    fn sample_track(path: &str) -> Track {
        Track {
            id: 0,
            path: path.to_string(),
            dir_path: "/music".to_string(),
            filename: "song.flac".to_string(),
            title: Some("Test Song".to_string()),
            artist: Some("Artist".to_string()),
            album: Some("Album".to_string()),
            album_artist: None,
            year: Some(2024),
            genre: Some("Electronic".to_string()),
            track_number: Some(1),
            disc_number: None,
            duration_secs: Some(180.5),
            cover_art_path: None,
            missing: false,
        }
    }

    #[test]
    fn test_insert_and_get_all_tracks() {
        let conn = open_in_memory();
        let track = sample_track("/music/song.flac");
        let id = insert_track(&conn, &track).unwrap();
        assert!(id > 0);

        let tracks = get_all_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].path, "/music/song.flac");
        assert_eq!(tracks[0].title, Some("Test Song".to_string()));
    }

    #[test]
    fn test_insert_ignore_duplicate_path() {
        let conn = open_in_memory();
        let track = sample_track("/music/song.flac");
        let id1 = insert_track(&conn, &track).unwrap();
        let id2 = insert_track(&conn, &track).unwrap();
        assert_eq!(id1, id2);

        let tracks = get_all_tracks(&conn).unwrap();
        assert_eq!(tracks.len(), 1);
    }

    #[test]
    fn test_update_track() {
        let conn = open_in_memory();
        let mut track = sample_track("/music/song.flac");
        insert_track(&conn, &track).unwrap();

        track.title = Some("Updated Title".to_string());
        update_track(&conn, &track).unwrap();

        let tracks = get_all_tracks(&conn).unwrap();
        assert_eq!(tracks[0].title, Some("Updated Title".to_string()));
    }

    #[test]
    fn test_mark_missing() {
        let conn = open_in_memory();
        let track = sample_track("/music/song.flac");
        insert_track(&conn, &track).unwrap();

        mark_missing(&conn, "/music/song.flac", true).unwrap();
        let tracks = get_all_tracks(&conn).unwrap();
        assert!(tracks[0].missing);

        mark_missing(&conn, "/music/song.flac", false).unwrap();
        let tracks = get_all_tracks(&conn).unwrap();
        assert!(!tracks[0].missing);
    }

    #[test]
    fn test_app_state_round_trip() {
        let conn = open_in_memory();

        let state = AppState {
            last_track_id: Some(42),
            last_position_secs: 12.5,
            volume: 0.75,
            root_directories: vec!["/music".to_string(), "/albums".to_string()],
        };

        save_app_state(&conn, &state).unwrap();
        let loaded = get_app_state(&conn).unwrap();

        assert_eq!(loaded.last_track_id, Some(42));
        assert!((loaded.last_position_secs - 12.5).abs() < f64::EPSILON);
        assert!((loaded.volume - 0.75).abs() < f32::EPSILON);
        assert_eq!(loaded.root_directories, vec!["/music", "/albums"]);
    }

    #[test]
    fn test_app_state_defaults_when_empty() {
        let conn = open_in_memory();
        let state = get_app_state(&conn).unwrap();
        assert_eq!(state.last_track_id, None);
        assert_eq!(state.last_position_secs, 0.0);
        assert_eq!(state.volume, 1.0);
        assert!(state.root_directories.is_empty());
    }

    #[test]
    fn test_reset_library() {
        let conn = open_in_memory();
        insert_track(&conn, &sample_track("/music/a.flac")).unwrap();
        insert_track(&conn, &sample_track("/music/b.flac")).unwrap();

        let app_state = AppState {
            last_track_id: Some(1),
            last_position_secs: 5.0,
            volume: 0.5,
            root_directories: vec!["/music".to_string()],
        };
        save_app_state(&conn, &app_state).unwrap();

        reset_library(&conn).unwrap();

        let tracks = get_all_tracks(&conn).unwrap();
        assert!(tracks.is_empty());

        let state = get_app_state(&conn).unwrap();
        assert_eq!(state.last_track_id, None);
        assert_eq!(state.root_directories.len(), 0);
    }

    #[test]
    fn test_app_state_no_last_track_id() {
        let conn = open_in_memory();
        let state = AppState {
            last_track_id: None,
            last_position_secs: 0.0,
            volume: 1.0,
            root_directories: vec![],
        };
        save_app_state(&conn, &state).unwrap();
        let loaded = get_app_state(&conn).unwrap();
        assert_eq!(loaded.last_track_id, None);
    }
}
