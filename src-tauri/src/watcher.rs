use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecursiveMode, Watcher as NotifyWatcher};
use tauri::{AppHandle, Emitter, Manager};

use crate::{db, metadata, types::AppError};

/// Messages sent to the watcher control thread.
enum WatchCmd {
    Add(PathBuf),
    Remove(PathBuf),
}

/// Wraps a `notify::RecommendedWatcher` and manages filesystem event handling
/// with a 500 ms debounce. Stored as Tauri managed state.
pub struct Watcher {
    /// Channel to send add/remove directory commands to the watcher thread.
    cmd_tx: Sender<WatchCmd>,
}

impl Watcher {
    /// Start the watcher: reads `root_directories` from the DB and registers
    /// each with a 500 ms debounced event handler.
    pub fn start(app_handle: AppHandle) -> Result<Self, AppError> {
        // Channel for filesystem events from notify.
        let (event_tx, event_rx) = mpsc::channel::<notify::Result<Event>>();
        // Channel for add/remove directory commands.
        let (cmd_tx, cmd_rx) = mpsc::channel::<WatchCmd>();

        // Create the notify watcher that sends events to event_tx.
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = event_tx.send(res);
        })
        .map_err(|e| AppError::Io(format!("Failed to create watcher: {}", e)))?;

        // Register initial directories from DB.
        let conn = db::init_db(&app_handle)?;
        let state = db::get_app_state(&conn)?;
        for dir in &state.root_directories {
            let path = PathBuf::from(dir);
            if path.exists() {
                if let Err(e) = watcher.watch(&path, RecursiveMode::Recursive) {
                    eprintln!("[watcher] Failed to watch {:?}: {}", path, e);
                }
            }
        }

        let app_handle_clone = app_handle.clone();
        let cmd_tx_clone = cmd_tx.clone();

        // Spawn a single thread that owns the watcher and processes both events and commands.
        std::thread::spawn(move || {
            let mut w = watcher;
            let mut debounce: HashMap<PathBuf, (EventKind, Instant)> = HashMap::new();
            let debounce_duration = Duration::from_millis(500);

            loop {
                // Try to receive an event with a timeout so we can flush debounced events.
                match event_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(Ok(event)) => {
                        let kind = event.kind;
                        for path in event.paths {
                            if is_audio_file(&path) {
                                debounce.insert(path, (kind.clone(), Instant::now()));
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        eprintln!("[watcher] notify error: {}", e);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // Timeout is expected; we use it to flush debounced events.
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }

                // Process any pending add/remove commands (non-blocking).
                while let Ok(cmd) = cmd_rx.try_recv() {
                    match cmd {
                        WatchCmd::Add(path) => {
                            if path.exists() {
                                if let Err(e) = w.watch(&path, RecursiveMode::Recursive) {
                                    eprintln!("[watcher] Failed to watch {:?}: {}", path, e);
                                }
                            }
                        }
                        WatchCmd::Remove(path) => {
                            if let Err(e) = w.unwatch(&path) {
                                eprintln!("[watcher] Failed to unwatch {:?}: {}", path, e);
                            }
                        }
                    }
                }

                // Flush debounced events that have passed the debounce window.
                let now = Instant::now();
                let ready: Vec<(PathBuf, EventKind)> = debounce
                    .iter()
                    .filter(|(_, (_, t))| now.duration_since(*t) >= debounce_duration)
                    .map(|(p, (k, _))| (p.clone(), k.clone()))
                    .collect();

                for (path, kind) in ready {
                    debounce.remove(&path);
                    handle_event(&app_handle_clone, &path, &kind);
                }
            }
        });

        Ok(Watcher {
            cmd_tx: cmd_tx_clone,
        })
    }

    /// Add a directory to the watch list at runtime.
    pub fn add_directory(&self, path: &str) -> Result<(), AppError> {
        self.cmd_tx
            .send(WatchCmd::Add(PathBuf::from(path)))
            .map_err(|e| AppError::Io(format!("Watcher channel error: {}", e)))
    }

    /// Remove a directory from the watch list at runtime.
    pub fn remove_directory(&self, path: &str) -> Result<(), AppError> {
        self.cmd_tx
            .send(WatchCmd::Remove(PathBuf::from(path)))
            .map_err(|e| AppError::Io(format!("Watcher channel error: {}", e)))
    }
}

/// Returns true if the path has an audio file extension.
fn is_audio_file(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext.to_lowercase().as_str(),
            "mp3" | "flac" | "ogg" | "wav" | "aac" | "m4a" | "opus" | "wma" | "aiff" | "aif"
        ),
        None => false,
    }
}

/// Handle a debounced filesystem event for an audio file.
fn handle_event(app_handle: &AppHandle, path: &Path, kind: &EventKind) {
    match kind {
        // File created or renamed into the directory.
        EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::To)) => {
            handle_create_or_modify(app_handle, path);
        }
        // File removed or renamed out of the directory.
        EventKind::Remove(_) | EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::From)) => {
            handle_remove(app_handle, path);
        }
        // File content modified.
        EventKind::Modify(_) => {
            handle_create_or_modify(app_handle, path);
        }
        _ => {}
    }
}

fn handle_create_or_modify(app_handle: &AppHandle, path: &Path) {
    let cache_dir = app_handle
        .path()
        .app_cache_dir()
        .ok();

    let track = match metadata::extract_metadata(path, cache_dir.as_deref()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[watcher] Failed to extract metadata for {:?}: {}", path, e);
            return;
        }
    };

    let conn = match db::init_db(app_handle) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[watcher] Failed to open DB: {}", e);
            return;
        }
    };

    // Try insert first; if it already exists, update instead.
    match db::insert_track(&conn, &track) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("[watcher] insert_track failed for {:?}: {}", path, e);
            // Try update as fallback.
            if let Err(e2) = db::update_track(&conn, &track) {
                eprintln!("[watcher] update_track also failed for {:?}: {}", path, e2);
                return;
            }
        }
    }

    let _ = app_handle.emit("library_changed", ());
}

fn handle_remove(app_handle: &AppHandle, path: &Path) {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            eprintln!("[watcher] Non-UTF-8 path on remove: {:?}", path);
            return;
        }
    };

    let conn = match db::init_db(app_handle) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[watcher] Failed to open DB: {}", e);
            return;
        }
    };

    if let Err(e) = db::mark_missing(&conn, path_str, true) {
        eprintln!("[watcher] mark_missing failed for {:?}: {}", path, e);
        return;
    }

    let _ = app_handle.emit("library_changed", ());
}
