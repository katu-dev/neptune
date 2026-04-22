pub mod bpm;
pub mod commands;
pub mod crossfade;
pub mod db;
pub mod discord;
pub mod eq;
pub mod genre;
pub mod keybinds;
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

            // Initialize Equalizer and load persisted eq_gains from app_state.
            let mut equalizer = eq::Equalizer::new(48000);
            if let Ok(conn) = db::init_db(app.handle()) {
                if let Ok(gains_str) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'eq_gains'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    if let Ok(gains) = serde_json::from_str::<Vec<f32>>(&gains_str) {
                        for (band, &gain) in gains.iter().enumerate().take(8) {
                            equalizer.set_gain(band, gain);
                        }
                    }
                }
            }
            let equalizer_arc = std::sync::Arc::new(Mutex::new(equalizer));
            app.manage(equalizer_arc.clone());

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
            let panner_arc = std::sync::Arc::new(Mutex::new(pan));
            app.manage(panner_arc.clone());

            let player = player::Player::new_with_queue_and_dsp(
                app.handle().clone(),
                Some(queue_manager.clone()),
                Some(equalizer_arc),
                Some(panner_arc),
            );
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

            // Initialize KeybindRegistry and load persisted keybinds.
            let keybind_registry = keybinds::KeybindRegistry::new(app.handle().clone());
            if let Err(e) = keybind_registry.load_from_db() {
                eprintln!("[lib] Failed to load keybinds from DB: {}", e);
            }
            app.manage(keybind_registry);

            // Initialize BpmAnalyzer.
            let bpm_analyzer = bpm::BpmAnalyzer::new(app.handle().clone());

            // Initialize GenreClassifier.
            let genre_classifier = genre::GenreClassifier::new(app.handle().clone());

            // At startup, schedule BPM and genre analysis for any tracks missing them.
            if let Ok(conn) = db::init_db(app.handle()) {
                if let Ok(tracks) = db::get_all_tracks(&conn) {
                    for track in tracks {
                        if track.missing {
                            continue;
                        }
                        if track.bpm.is_none() {
                            bpm_analyzer.schedule(track.id, track.path.clone());
                        }
                        if track.genre.is_none() {
                            genre_classifier.schedule(track.id, track.path.clone());
                        }
                    }
                }
            }

            app.manage(bpm_analyzer);
            app.manage(genre_classifier);

            // Initialize DiscordPresence and load persisted enabled state.
            let mut discord_presence = discord::DiscordPresence::new();
            if let Ok(conn) = db::init_db(app.handle()) {
                if let Ok(val) = conn.query_row(
                    "SELECT value FROM app_state WHERE key = 'discord_enabled'",
                    [],
                    |row| row.get::<_, String>(0),
                ) {
                    if let Ok(enabled) = val.parse::<bool>() {
                        if !enabled {
                            discord_presence.set_enabled(false);
                        }
                    }
                }
            }
            // Attempt initial connection; schedule reconnect if it fails.
            if discord_presence.try_connect() {
                // Connected — nothing more to do.
            } else {
                discord_presence.schedule_reconnect(app.handle().clone());
            }
            app.manage(Mutex::new(discord_presence));

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
            get_eq_gains,
            set_eq_gain,
            set_eq_bypassed,
            reset_eq,
            commands::set_crossfade_duration,
            commands::set_gapless_enabled,
            commands::get_crossfade_settings,
            commands::analyze_bpm,
            commands::analyze_genre,
            keybinds::get_keybinds,
            keybinds::set_keybind,
            keybinds::reset_keybinds,
            keybinds::dispatch_keybind,
            set_discord_enabled,
            get_discord_enabled,
            commands::get_recommendations,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Return the current pan value.
#[tauri::command]
fn get_pan(state: tauri::State<'_, std::sync::Arc<Mutex<panner::Panner>>>) -> f32 {
    state.lock().unwrap().get_pan()
}

/// Set the pan value and persist it to the database.
#[tauri::command]
fn set_pan(
    value: f32,
    state: tauri::State<'_, std::sync::Arc<Mutex<panner::Panner>>>,
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

/// Return the current EQ band gains as an array of 8 floats.
#[tauri::command]
fn get_eq_gains(state: tauri::State<'_, std::sync::Arc<Mutex<eq::Equalizer>>>) -> [f32; 8] {
    state.lock().unwrap().get_gains()
}

/// Set the gain for a single EQ band and persist all gains to the database.
#[tauri::command]
fn set_eq_gain(
    band: usize,
    gain_db: f32,
    state: tauri::State<'_, std::sync::Arc<Mutex<eq::Equalizer>>>,
    app_handle: tauri::AppHandle,
) -> Result<(), types::AppError> {
    let gains = {
        let mut eq = state.lock().unwrap();
        eq.set_gain(band, gain_db);
        eq.get_gains()
    };
    persist_eq_gains(&app_handle, &gains)
}

/// Enable or disable the EQ bypass.
#[tauri::command]
fn set_eq_bypassed(
    bypassed: bool,
    state: tauri::State<'_, std::sync::Arc<Mutex<eq::Equalizer>>>,
) {
    state.lock().unwrap().set_bypassed(bypassed);
}

/// Reset all EQ bands to 0 dB and persist the flat gains.
#[tauri::command]
fn reset_eq(
    state: tauri::State<'_, std::sync::Arc<Mutex<eq::Equalizer>>>,
    app_handle: tauri::AppHandle,
) -> Result<(), types::AppError> {
    let gains = {
        let mut eq = state.lock().unwrap();
        eq.reset();
        eq.get_gains()
    };
    persist_eq_gains(&app_handle, &gains)
}

fn persist_eq_gains(app_handle: &tauri::AppHandle, gains: &[f32; 8]) -> Result<(), types::AppError> {
    let json = serde_json::to_string(gains)
        .map_err(|e| types::AppError::Database(e.to_string()))?;
    let conn = db::init_db(app_handle)?;
    conn.execute(
        "INSERT INTO app_state (key, value) VALUES ('eq_gains', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![json],
    )
    .map_err(|e| types::AppError::Database(e.to_string()))?;
    Ok(())
}

/// Enable or disable Discord Rich Presence and persist the setting.
#[tauri::command]
fn set_discord_enabled(
    enabled: bool,
    state: tauri::State<'_, Mutex<discord::DiscordPresence>>,
    app_handle: tauri::AppHandle,
) -> Result<(), types::AppError> {
    {
        let mut dp = state.lock().unwrap();
        dp.set_enabled(enabled);
        // If re-enabling, attempt to connect and schedule reconnect if needed.
        if enabled {
            if !dp.try_connect() {
                dp.schedule_reconnect(app_handle.clone());
            }
        }
    }
    let conn = db::init_db(&app_handle)?;
    conn.execute(
        "INSERT INTO app_state (key, value) VALUES ('discord_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![enabled.to_string()],
    )
    .map_err(|e| types::AppError::Database(e.to_string()))?;
    Ok(())
}

/// Return whether Discord Rich Presence is currently enabled.
#[tauri::command]
fn get_discord_enabled(state: tauri::State<'_, Mutex<discord::DiscordPresence>>) -> bool {
    state.lock().unwrap().enabled
}
