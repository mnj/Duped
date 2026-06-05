mod commands;
mod db;
mod scanner;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::abort_scan,
            commands::get_progress,
            commands::open_database,
            commands::get_duplicates,
            commands::get_stats,
            commands::list_scans,
            commands::get_db_path,
            commands::dismiss_scan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
