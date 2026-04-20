pub mod commands;
pub mod db;
pub mod metadata;
pub mod player;
pub mod scanner;
pub mod spectrogram;
pub mod types;
pub mod utils;
pub mod waveform;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let player = player::Player::new(app.handle().clone());
            app.manage(std::sync::Mutex::new(player));
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
