pub mod commands;
pub mod crossfade;
pub mod db;
pub mod eq;
pub mod metadata;
pub mod panner;
pub mod player;
pub mod queue;
pub mod scanner;
pub mod spectrogram;
pub mod types;
pub mod utils;
pub mod waveform;
pub mod watcher;

use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let queue_manager = queue::QueueManager::new(app.handle().clone());
            if let Err(e) = queue_manager.load_from_db() {
                eprintln!("[lib] Failed to load queue from DB: {}", e);
            }

            let player = player::Player::new_with_queue(app.handle().clone(), Some(queue_manager.clone()));
            app.manage(std::sync::Mutex::new(player));

            match watcher::Watcher::start(app.handle().clone()) {
                Ok(w) => {
                    app.manage(std::sync::Mutex::new(w));
                }
                Err(e) => {
                    eprintln!("[lib] Failed to start watcher: {}", e);
                }
            }

            app.manage(queue_manager);

            // Initialize Panner and load persisted pan_value from app_state.
            let mut pan = panner::Panner::new();
            if let Ok(conn) = db::init_db(app.handle()) {
                if let Ok(pan_str) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'pan_value'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    if let Ok(v) = pan_str.parse::<f32>() {
                        pan.set_pan(v);
                    }
                }
            }
            app.manage(Mutex::new(pan));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_directory,
            commands::remove_root_directory,
            commands::get_library,
            commands::get_directory_tree,
            commands::get_track_metadata,
            commands::get_cover_art,
            commands::reset_library,
            commands::play_track,
            commands::pause,
            commands::stop,
            commands::seek,
            commands::set_volume,
            commands::play_next,
            commands::play_previous,
            commands::get_waveform,
            commands::get_spectrogram,
            commands::get_app_state,
            commands::save_app_state,
            commands::create_tag,
            commands::delete_tag,
            commands::get_tags,
            commands::assign_tag,
            commands::remove_tag_from_track,
            commands::get_track_tags,
            commands::get_all_track_tags,
            commands::add_watch_directory,
            commands::remove_watch_directory,
            commands::queue_add,
            commands::queue_add_next,
            commands::queue_remove,
            commands::queue_move,
            commands::queue_clear,
            commands::queue_shuffle,
            commands::get_queue,
            get_pan,
            set_pan,
            commands::set_crossfade_duration,
            commands::set_gapless_enabled,
            commands::get_crossfade_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Return the current pan value.
#[tauri::command]
fn get_pan(state: tauri::State<'_, Mutex<panner::Panner>>) -> f32 {
    state.lock().unwrap().get_pan()
}

/// Set the pan value and persist it to the database.
#[tauri::command]
fn set_pan(
    value: f32,
    state: tauri::State<'_, Mutex<panner::Panner>>,
    app_handle: tauri::AppHandle,
) -> Result<(), types::AppError> {
    {
        let mut pan = state.lock().unwrap();
        pan.set_pan(value);
    }
    let conn = db::init_db(&app_handle)?;
    conn.execute(
        "INSERT INTO app_state (key, value) VALUES ('pan_value', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![value.to_string()],
    )
    .map_err(|e| types::AppError::Database(e.to_string()))?;
    Ok(())
}
