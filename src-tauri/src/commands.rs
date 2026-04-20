use std::path::Path;

use crate::{
    db,
    player::Player,
    scanner,
    spectrogram,
    types::{AppError, AppState, DirNode, ScanResult, SpectrogramData, Tag, Track, WaveformData},
    waveform,
};
use std::sync::Mutex;
use tauri::{Emitter, State};

/// Scan a directory for audio files, index them, and emit `scan_progress` events.
/// Also persists the path to root_directories in app_state.
#[tauri::command]
pub async fn scan_directory(
    path: String,
    app_handle: tauri::AppHandle,
) -> Result<ScanResult, AppError> {
    // Persist this path as a root directory
    let conn = db::init_db(&app_handle)?;
    let mut state = db::get_app_state(&conn)?;
    if !state.root_directories.contains(&path) {
        state.root_directories.push(path.clone());
        db::save_app_state(&conn, &state)?;
    }
    scanner::scan_directory(path, app_handle).await
}

/// Remove a root directory from the persisted list (does not delete tracks).
#[tauri::command]
pub async fn remove_root_directory(
    path: String,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    let mut state = db::get_app_state(&conn)?;
    state.root_directories.retain(|d| d != &path);
    db::save_app_state(&conn, &state)
}

/// Return all tracks in the library.
#[tauri::command]
pub async fn get_library(app_handle: tauri::AppHandle) -> Result<Vec<Track>, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_all_tracks(&conn)
}

/// Build a directory tree from the indexed tracks, filtered to persisted root directories.
///
/// If `path` is `Some`, only tracks under that prefix are shown.
/// If `path` is `None`, only tracks under the persisted root_directories are shown.
/// If no root directories have been added yet, returns an empty tree.
#[tauri::command]
pub async fn get_directory_tree(
    path: Option<String>,
    app_handle: tauri::AppHandle,
) -> Result<Vec<DirNode>, AppError> {
    let conn = db::init_db(&app_handle)?;
    let all_tracks = db::get_all_tracks(&conn)?;

    let tracks: Vec<Track> = match &path {
        Some(root) => all_tracks
            .into_iter()
            .filter(|t| t.dir_path.starts_with(root.as_str()))
            .collect(),
        None => {
            // Filter to only tracks under persisted root directories
            let state = db::get_app_state(&conn)?;
            if state.root_directories.is_empty() {
                return Ok(Vec::new());
            }
            all_tracks
                .into_iter()
                .filter(|t| {
                    state.root_directories.iter().any(|root| {
                        t.dir_path.starts_with(root.as_str())
                    })
                })
                .collect()
        }
    };

    Ok(build_tree(tracks))
}

/// Return the full metadata for a single track by id.
#[tauri::command]
pub async fn get_track_metadata(
    track_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<Track, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_track_by_id(&conn, track_id)?
        .ok_or(AppError::TrackNotFound(track_id))
}

/// Return the raw bytes of the cover art for a track, or `None` if absent.
#[tauri::command]
pub async fn get_cover_art(
    track_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<Option<Vec<u8>>, AppError> {
    let conn = db::init_db(&app_handle)?;
    let track = db::get_track_by_id(&conn, track_id)?
        .ok_or(AppError::TrackNotFound(track_id))?;

    match track.cover_art_path {
        None => Ok(None),
        Some(art_path) => {
            let bytes = std::fs::read(Path::new(&art_path))
                .map_err(|e| AppError::Io(e.to_string()))?;
            Ok(Some(bytes))
        }
    }
}

/// Delete all tracks from the library and reset app state.
#[tauri::command]
pub async fn reset_library(app_handle: tauri::AppHandle) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    db::reset_library(&conn)
}

// ---------------------------------------------------------------------------
// Playback commands
// ---------------------------------------------------------------------------

/// Start playing a track by its database id.
#[tauri::command]
pub fn play_track(
    track_id: i64,
    state: State<'_, Mutex<Player>>,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    let track = db::get_track_by_id(&conn, track_id)?
        .ok_or(AppError::TrackNotFound(track_id))?;

    if track.missing {
        return Err(AppError::Io("Track file not found".to_string()));
    }

    let player = state.lock().unwrap();
    player.handle().play_track(track.path, track_id, 0.0)
}

/// Toggle pause/resume. If playing → pause; if paused → resume.
#[tauri::command]
pub fn pause(state: State<'_, Mutex<Player>>) -> Result<(), AppError> {
    use crate::player::PlayerState;
    let player = state.lock().unwrap();
    match player.state() {
        PlayerState::Playing => player.handle().pause(),
        PlayerState::Paused => player.handle().resume(),
        PlayerState::Stopped => Ok(()),
    }
}

/// Stop playback entirely.
#[tauri::command]
pub fn stop(state: State<'_, Mutex<Player>>) -> Result<(), AppError> {
    let player = state.lock().unwrap();
    player.handle().stop()
}

/// Seek to a position in the current track (seconds).
#[tauri::command]
pub fn seek(
    position_secs: f64,
    state: State<'_, Mutex<Player>>,
) -> Result<(), AppError> {
    let player = state.lock().unwrap();
    player.handle().seek(position_secs)
}

/// Set the playback volume (0.0 – 1.0).
#[tauri::command]
pub fn set_volume(
    level: f32,
    state: State<'_, Mutex<Player>>,
) -> Result<(), AppError> {
    let player = state.lock().unwrap();
    player.handle().set_volume(level)
}

/// Play the next track in the same directory (sorted by track_number then filename).
#[tauri::command]
pub fn play_next(
    state: State<'_, Mutex<Player>>,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let current_id = {
        let player = state.lock().unwrap();
        player.current_track_id()
    };

    let Some(current_id) = current_id else {
        return Ok(());
    };

    let conn = db::init_db(&app_handle)?;
    let all_tracks = db::get_all_tracks(&conn)?;

    let current = all_tracks
        .iter()
        .find(|t| t.id == current_id)
        .ok_or(AppError::TrackNotFound(current_id))?;

    let dir = current.dir_path.clone();

    let mut dir_tracks: Vec<&Track> = all_tracks
        .iter()
        .filter(|t| t.dir_path == dir)
        .collect();

    dir_tracks.sort_by(|a, b| {
        a.track_number
            .cmp(&b.track_number)
            .then_with(|| a.filename.cmp(&b.filename))
    });

    let pos = dir_tracks.iter().position(|t| t.id == current_id);
    if let Some(idx) = pos {
        if let Some(next) = dir_tracks.get(idx + 1) {
            let player = state.lock().unwrap();
            return player.handle().play_track(next.path.clone(), next.id, 0.0);
        }
    }

    Ok(())
}

/// Play the previous track in the same directory (sorted by track_number then filename).
#[tauri::command]
pub fn play_previous(
    state: State<'_, Mutex<Player>>,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let current_id = {
        let player = state.lock().unwrap();
        player.current_track_id()
    };

    let Some(current_id) = current_id else {
        return Ok(());
    };

    let conn = db::init_db(&app_handle)?;
    let all_tracks = db::get_all_tracks(&conn)?;

    let current = all_tracks
        .iter()
        .find(|t| t.id == current_id)
        .ok_or(AppError::TrackNotFound(current_id))?;

    let dir = current.dir_path.clone();

    let mut dir_tracks: Vec<&Track> = all_tracks
        .iter()
        .filter(|t| t.dir_path == dir)
        .collect();

    dir_tracks.sort_by(|a, b| {
        a.track_number
            .cmp(&b.track_number)
            .then_with(|| a.filename.cmp(&b.filename))
    });

    let pos = dir_tracks.iter().position(|t| t.id == current_id);
    if let Some(idx) = pos {
        if idx > 0 {
            if let Some(prev) = dir_tracks.get(idx - 1) {
                let player = state.lock().unwrap();
                return player.handle().play_track(prev.path.clone(), prev.id, 0.0);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Waveform command
// ---------------------------------------------------------------------------

const DEFAULT_WAVEFORM_WIDTH: usize = 50_000;

#[derive(serde::Serialize, Clone)]
struct WaveformReadyPayload {
    track_id: i64,
}

/// Decode the audio file for `track_id` and return a downsampled waveform envelope.
/// Emits `waveform_chunk` events every ~20s of decoded audio for progressive rendering,
/// then emits `waveform_ready` with `{ track_id }` when computation finishes.
#[tauri::command]
pub async fn get_waveform(
    track_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<WaveformData, AppError> {
    let conn = db::init_db(&app_handle)?;
    let track = db::get_track_by_id(&conn, track_id)?
        .ok_or(AppError::TrackNotFound(track_id))?;

    let path = track.path.clone();
    let handle_clone = app_handle.clone();

    let data = tokio::task::spawn_blocking(move || {
        waveform::generate_waveform(&path, DEFAULT_WAVEFORM_WIDTH, track_id, |chunk| {
            let _ = handle_clone.emit("waveform_chunk", chunk);
        })
    })
    .await
    .map_err(|e| AppError::Decode(e.to_string()))??;

    let _ = app_handle.emit("waveform_ready", WaveformReadyPayload { track_id });

    Ok(data)
}

// ---------------------------------------------------------------------------
// Spectrogram command
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, Clone)]
struct SpectrogramReadyPayload {
    track_id: i64,
}

/// Decode the audio file for `track_id` and compute a spectrogram via STFT.
/// Emits `spectrogram_ready` with `{ track_id }` when computation finishes.
#[tauri::command]
pub async fn get_spectrogram(
    track_id: i64,
    fft_size: Option<u32>,
    hop_size: Option<u32>,
    app_handle: tauri::AppHandle,
) -> Result<SpectrogramData, AppError> {
    let conn = db::init_db(&app_handle)?;
    let track = db::get_track_by_id(&conn, track_id)?
        .ok_or(AppError::TrackNotFound(track_id))?;

    let path = track.path.clone();
    let fft = fft_size.unwrap_or(spectrogram::DEFAULT_FFT_SIZE);
    let hop = hop_size.unwrap_or(spectrogram::DEFAULT_HOP_SIZE);

    let data = tokio::task::spawn_blocking(move || {
        spectrogram::generate_spectrogram(&path, track_id, fft, hop)
    })
    .await
    .map_err(|e| AppError::Decode(e.to_string()))??;

    let _ = app_handle.emit("spectrogram_ready", SpectrogramReadyPayload { track_id });

    Ok(data)
}

// ---------------------------------------------------------------------------
// App state commands
// ---------------------------------------------------------------------------

/// Load the persisted app state from the database.
#[tauri::command]
pub async fn get_app_state(app_handle: tauri::AppHandle) -> Result<AppState, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_app_state(&conn)
}

/// Persist the current app state to the database.
#[tauri::command]
pub async fn save_app_state(
    state: AppState,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    db::save_app_state(&conn, &state)
}

// ---------------------------------------------------------------------------
// Directory tree builder
// ---------------------------------------------------------------------------

/// Build a `Vec<DirNode>` from a flat list of tracks, grouped by `dir_path`.
///
/// Each unique directory path becomes a node. Nodes are nested according to
/// the filesystem hierarchy. Tracks are placed in the leaf node that matches
/// their `dir_path`.
fn build_tree(tracks: Vec<Track>) -> Vec<DirNode> {
    use std::collections::{BTreeSet, HashMap};

    if tracks.is_empty() {
        return Vec::new();
    }

    // Map dir_path -> tracks.
    let mut dir_tracks: HashMap<String, Vec<Track>> = HashMap::new();
    for track in tracks {
        dir_tracks
            .entry(track.dir_path.clone())
            .or_default()
            .push(track);
    }

    // Sort tracks within each directory by filename.
    for tracks in dir_tracks.values_mut() {
        tracks.sort_by(|a, b| a.filename.cmp(&b.filename));
    }

    // Collect all directory paths that have tracks.
    let leaf_dirs: BTreeSet<String> = dir_tracks.keys().cloned().collect();

    // Ensure every ancestor directory is represented (even if it has no
    // direct tracks).
    let mut all_dirs: BTreeSet<String> = leaf_dirs.clone();
    for dir in &leaf_dirs {
        let mut current = Path::new(dir.as_str());
        while let Some(parent) = current.parent() {
            let p_str = parent.to_string_lossy().to_string();
            // Stop at filesystem root (empty string, single separator, or
            // Windows drive root like "C:\\").
            if p_str.is_empty()
                || p_str == "/"
                || p_str == "\\"
                || p_str == current.to_string_lossy()
                || (p_str.len() <= 3 && p_str.ends_with(":\\"))
            {
                break;
            }
            all_dirs.insert(p_str);
            current = parent;
        }
    }

    // Build a node for every directory.
    let mut nodes: HashMap<String, DirNode> = all_dirs
        .iter()
        .map(|dir| {
            let name = Path::new(dir.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(dir.as_str())
                .to_string();
            let tracks = dir_tracks.remove(dir).unwrap_or_default();
            (
                dir.clone(),
                DirNode {
                    path: dir.clone(),
                    name,
                    children: Vec::new(),
                    tracks,
                },
            )
        })
        .collect();

    // Determine root directories: those whose parent is not in `all_dirs`.
    let mut roots: Vec<String> = all_dirs
        .iter()
        .filter(|dir| {
            match Path::new(dir.as_str()).parent() {
                None => true,
                Some(p) => {
                    let p_str = p.to_string_lossy().to_string();
                    p_str.is_empty()
                        || p_str == dir.as_str()
                        || !all_dirs.contains(&p_str)
                }
            }
        })
        .cloned()
        .collect();

    roots.sort();

    // Attach children to their parents. Process deepest paths first so that
    // when we remove a child node it can be inserted into its parent.
    let sorted_dirs: Vec<String> = {
        let mut v: Vec<String> = all_dirs.iter().cloned().collect();
        v.sort_by(|a, b| {
            let da = a.chars().filter(|&c| c == '/' || c == '\\').count();
            let db_count = b.chars().filter(|&c| c == '/' || c == '\\').count();
            db_count.cmp(&da)
        });
        v
    };

    for dir in &sorted_dirs {
        if roots.contains(dir) {
            continue;
        }
        let parent_path = match Path::new(dir.as_str()).parent() {
            Some(p) => p.to_string_lossy().to_string(),
            None => continue,
        };
        if parent_path.is_empty() || parent_path == dir.as_str() {
            continue;
        }

        if let Some(child) = nodes.remove(dir) {
            if let Some(parent) = nodes.get_mut(&parent_path) {
                parent.children.push(child);
            }
        }
    }

    // Sort children within each node.
    for node in nodes.values_mut() {
        node.children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    roots
        .iter()
        .filter_map(|r| nodes.remove(r))
        .collect()
}

// ---------------------------------------------------------------------------
// Tag commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_tag(
    name: String,
    color: String,
    app_handle: tauri::AppHandle,
) -> Result<Tag, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::create_tag(&conn, &name, &color)
}

#[tauri::command]
pub async fn delete_tag(
    tag_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    db::delete_tag(&conn, tag_id)
}

#[tauri::command]
pub async fn get_tags(app_handle: tauri::AppHandle) -> Result<Vec<Tag>, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_all_tags(&conn)
}

#[tauri::command]
pub async fn assign_tag(
    track_id: i64,
    tag_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    db::assign_tag(&conn, track_id, tag_id)
}

#[tauri::command]
pub async fn remove_tag_from_track(
    track_id: i64,
    tag_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<(), AppError> {
    let conn = db::init_db(&app_handle)?;
    db::remove_tag_from_track(&conn, track_id, tag_id)
}

#[tauri::command]
pub async fn get_track_tags(
    track_id: i64,
    app_handle: tauri::AppHandle,
) -> Result<Vec<Tag>, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_tags_for_track(&conn, track_id)
}

/// Returns all track-tag assignments as a map for efficient frontend filtering.
#[tauri::command]
pub async fn get_all_track_tags(
    app_handle: tauri::AppHandle,
) -> Result<Vec<(i64, i64)>, AppError> {
    let conn = db::init_db(&app_handle)?;
    db::get_all_track_tag_ids(&conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Track;

    fn make_track(path: &str, dir_path: &str, filename: &str) -> Track {
        Track {
            id: 0,
            path: path.to_string(),
            dir_path: dir_path.to_string(),
            filename: filename.to_string(),
            title: None,
            artist: None,
            album: None,
            album_artist: None,
            year: None,
            genre: None,
            track_number: None,
            disc_number: None,
            duration_secs: None,
            cover_art_path: None,
            missing: false,
        }
    }

    #[test]
    fn test_build_tree_empty() {
        let tree = build_tree(vec![]);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_build_tree_single_dir() {
        let tracks = vec![
            make_track("/music/a.mp3", "/music", "a.mp3"),
            make_track("/music/b.mp3", "/music", "b.mp3"),
        ];
        let tree = build_tree(tracks);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].tracks.len(), 2);
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn test_build_tree_nested_dirs() {
        let tracks = vec![
            make_track("/music/rock/a.mp3", "/music/rock", "a.mp3"),
            make_track("/music/jazz/b.mp3", "/music/jazz", "b.mp3"),
        ];
        let tree = build_tree(tracks);
        // Root should be /music with two children
        assert_eq!(tree.len(), 1);
        let root = &tree[0];
        assert_eq!(root.path, "/music");
        assert_eq!(root.children.len(), 2);
        assert!(root.tracks.is_empty());
    }

    #[test]
    fn test_build_tree_tracks_sorted_by_filename() {
        let tracks = vec![
            make_track("/music/z.mp3", "/music", "z.mp3"),
            make_track("/music/a.mp3", "/music", "a.mp3"),
        ];
        let tree = build_tree(tracks);
        assert_eq!(tree[0].tracks[0].filename, "a.mp3");
        assert_eq!(tree[0].tracks[1].filename, "z.mp3");
    }

    #[test]
    fn test_build_tree_path_filter_logic() {
        // Simulate what get_directory_tree does with a path filter
        let all_tracks = vec![
            make_track("/music/rock/a.mp3", "/music/rock", "a.mp3"),
            make_track("/other/b.mp3", "/other", "b.mp3"),
        ];
        let root = "/music".to_string();
        let filtered: Vec<Track> = all_tracks
            .into_iter()
            .filter(|t| t.dir_path.starts_with(root.as_str()))
            .collect();
        let tree = build_tree(filtered);
        // Only /music subtree
        assert_eq!(tree.len(), 1);
        assert!(tree[0].path.starts_with("/music"));
    }
}
