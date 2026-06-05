#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let use_tmp_db = std::env::var("DUPED_TMP_DB").is_ok();
    duped_lib::run(use_tmp_db)
}
