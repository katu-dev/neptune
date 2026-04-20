use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Emitter, Manager};
use walkdir::WalkDir;

use crate::{
    db,
    metadata,
    types::{AppError, ScanResult},
};

/// Audio file extensions the scanner recognises (case-insensitive).
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "aac", "ogg", "wav", "aiff", "opus",
];

/// Payload emitted as a `scan_progress` Tauri event.
#[derive(Debug, Clone, Serialize)]
struct ScanProgress {
    files_found: u32,
    current_dir: String,
    complete: bool,
}

/// Returns `true` when the file extension is a supported audio format.
fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_EXTENSIONS.iter().any(|&s| s == lower)
        })
        .unwrap_or(false)
}

/// Recursively scan `path`, index every supported audio file, and mark
/// previously-indexed tracks as missing when their file no longer exists.
///
/// Progress is streamed to the frontend via `scan_progress` Tauri events.
pub async fn scan_directory(
    path: String,
    app_handle: tauri::AppHandle,
) -> Result<ScanResult, AppError> {
    let root = PathBuf::from(&path);

    // --- Phase 1: collect all supported file paths (blocking I/O) ----------
    let root_clone = root.clone();
    let app_handle_clone = app_handle.clone();

    let audio_files: Vec<PathBuf> = tokio::task::spawn_blocking(move || {
        let mut found: Vec<PathBuf> = Vec::new();
        let mut last_dir = String::new();

        for entry in WalkDir::new(&root_clone)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let entry_path = entry.path().to_path_buf();

            if entry_path.is_dir() {
                let dir_str = entry_path.to_string_lossy().to_string();
                if dir_str != last_dir {
                    last_dir = dir_str.clone();
                    let _ = app_handle_clone.emit(
                        "scan_progress",
                        ScanProgress {
                            files_found: found.len() as u32,
                            current_dir: dir_str,
                            complete: false,
                        },
                    );
                }
                continue;
            }

            if is_supported(&entry_path) {
                found.push(entry_path);
            }
        }

        found
    })
    .await
    .map_err(|e| AppError::Io(format!("Scan task panicked: {}", e)))?;

    let total_found = audio_files.len() as u32;

    // --- Phase 2: open DB and upsert each discovered file ------------------
    let conn = db::init_db(&app_handle)?;

    // Resolve the app cache directory for cover art extraction.
    let cache_dir: Option<PathBuf> = app_handle
        .path()
        .app_cache_dir()
        .ok();

    let mut total_indexed: u32 = 0;
    let mut total_updated: u32 = 0;

    for file_path in &audio_files {
        match metadata::extract_metadata(file_path, cache_dir.as_deref()) {
            Ok(mut track) => {
                // Check whether the track already exists in the DB.
                let existing = db::get_all_tracks(&conn)?;
                let path_str = track.path.clone();
                let already_exists = existing.iter().any(|t| t.path == path_str);

                if already_exists {
                    // Ensure missing flag is cleared for files that are present.
                    track.missing = false;
                    db::update_track(&conn, &track)?;
                    total_updated += 1;
                } else {
                    db::insert_track(&conn, &track)?;
                    total_indexed += 1;
                }
            }
            Err(_) => {
                // Skip files that fail metadata extraction without interrupting the scan.
            }
        }
    }

    // --- Phase 3: mark tracks missing whose files no longer exist ----------
    let all_tracks = db::get_all_tracks(&conn)?;
    let mut total_missing: u32 = 0;

    for track in &all_tracks {
        // Only consider tracks under the scanned root.
        if !track.path.starts_with(&path) {
            continue;
        }

        let on_disk = Path::new(&track.path).exists();
        if !on_disk && !track.missing {
            db::mark_missing(&conn, &track.path, true)?;
            total_missing += 1;
        } else if on_disk && track.missing {
            // File reappeared — clear the missing flag (handled above in upsert,
            // but guard here for tracks not re-encountered during this scan).
            db::mark_missing(&conn, &track.path, false)?;
        }
    }

    // --- Phase 4: emit completion event ------------------------------------
    let _ = app_handle.emit(
        "scan_progress",
        ScanProgress {
            files_found: total_found,
            current_dir: path.clone(),
            complete: true,
        },
    );

    Ok(ScanResult {
        total_found,
        total_indexed,
        total_updated,
        total_missing,
    })
}
