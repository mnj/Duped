mod commands;
mod db;
mod scanner;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(use_tmp_db: bool) {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new(use_tmp_db))
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::abort_scan,
            commands::get_progress,
            commands::open_database,
            commands::get_duplicates,
            commands::get_duplicates_paginated,
            commands::get_duplicate_count,
            commands::get_stats,
            commands::list_scans,
            commands::get_db_path,
            commands::dismiss_scan,
            commands::finalize_scan,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
