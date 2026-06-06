use crate::db::{Database, DuplicateGroup, ScanInfo, Stats};
use crate::scanner::{hash_candidates, walk_and_collect, ScanProgress};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub phase: String,
    pub files_walked: u64,
    pub files_to_hash: u64,
    pub files_hashed: u64,
    pub bytes_hashed: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanComplete {
    pub scan_id: i64,
    pub stats: Stats,
    pub aborted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeResult {
    pub scans_merged: usize,
    pub files_merged: usize,
}

pub struct AppState {
    pub db: Mutex<Option<Arc<Database>>>,
    pub scan_id: Mutex<Option<i64>>,
    pub progress: Mutex<Option<Arc<ScanProgress>>>,
    pub db_path: Mutex<Option<PathBuf>>,
    pub use_tmp_db: bool,
}

impl AppState {
    pub fn new(use_tmp_db: bool) -> Self {
        Self {
            db: Mutex::new(None),
            scan_id: Mutex::new(None),
            progress: Mutex::new(None),
            db_path: Mutex::new(None),
            use_tmp_db,
        }
    }
}

fn get_db(state: &AppState) -> Result<Arc<Database>, String> {
    state
        .db
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "No database loaded".to_string())
}

fn get_scan_id(state: &AppState) -> Result<i64, String> {
    state
        .scan_id
        .lock()
        .unwrap()
        .ok_or_else(|| "No active scan".to_string())
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(dir)
}

fn scan_db_path(app: &AppHandle, use_tmp: bool) -> Result<PathBuf, String> {
    if use_tmp {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("duped_scan_{}.db", timestamp);
        Ok(std::env::temp_dir().join(filename))
    } else {
        let dir = app_data_dir(app)?;
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("scan_{}.db", timestamp);
        Ok(dir.join(filename))
    }
}

fn move_db_from_tmp(tmp_path: &PathBuf, app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_data_dir(app)?;
    let filename = tmp_path.file_name().ok_or("Invalid temp path")?;
    let final_path = dir.join(filename);
    std::fs::copy(tmp_path, &final_path)
        .map_err(|e| format!("Failed to copy database: {}", e))?;
    Ok(final_path)
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<i64, String> {
    let use_tmp = state.use_tmp_db;
    let db_path = scan_db_path(&app, use_tmp)?;

    let db = Arc::new(
        Database::new(&db_path).map_err(|e| format!("Failed to create database: {}", e))?,
    );

    let scan_id = db
        .create_scan(&path)
        .map_err(|e| format!("Failed to create scan: {}", e))?;

    {
        let mut db_lock = state.db.lock().unwrap();
        *db_lock = Some(db.clone());
        let mut scan_lock = state.scan_id.lock().unwrap();
        *scan_lock = Some(scan_id);
        let mut path_lock = state.db_path.lock().unwrap();
        *path_lock = Some(db_path.clone());
    }

    let progress = Arc::new(ScanProgress::new());
    {
        let mut prog_lock = state.progress.lock().unwrap();
        *prog_lock = Some(progress.clone());
    }

    let app_clone = app.clone();
    let db_clone = db.clone();
    let progress_clone = progress.clone();

    std::thread::spawn(move || {
        let app_handle = app_clone;
        let db = db_clone;
        let progress = progress_clone;

        let _ = walk_and_collect(&path, &db, scan_id, &progress);

        if !progress.is_aborted() {
            let _ = hash_candidates(&db, scan_id, &progress);
        }

        if progress.is_aborted() {
            let _ = db.abort_scan(scan_id);
        } else {
            let _ = db.complete_scan(scan_id);
            let _ = db.create_indexes();
        }

        let stats = db.get_stats(scan_id).unwrap_or(Stats {
            file_count: 0,
            total_size: 0,
            duplicate_groups: 0,
            duplicate_files: 0,
            wasted_space: 0,
        });

        let _ = app_handle.emit(
            "scan-complete",
            ScanComplete {
                scan_id,
                stats,
                aborted: progress.is_aborted(),
            },
        );
    });

    let app_clone = app.clone();
    let progress_clone = progress.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if progress_clone.is_aborted() {
                break;
            }
            let phase = if progress_clone.files_to_hash.load(std::sync::atomic::Ordering::Relaxed)
                == 0
            {
                "walking"
            } else {
                "hashing"
            };
            let _ = app_clone.emit(
                "scan-progress",
                ProgressEvent {
                    phase: phase.to_string(),
                    files_walked: progress_clone
                        .files_walked
                        .load(std::sync::atomic::Ordering::Relaxed),
                    files_to_hash: progress_clone
                        .files_to_hash
                        .load(std::sync::atomic::Ordering::Relaxed),
                    files_hashed: progress_clone
                        .files_hashed
                        .load(std::sync::atomic::Ordering::Relaxed),
                    bytes_hashed: progress_clone
                        .bytes_hashed
                        .load(std::sync::atomic::Ordering::Relaxed),
                },
            );
        }
    });

    Ok(scan_id)
}

#[tauri::command]
pub fn abort_scan(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let progress = state
        .progress
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "No active scan".to_string())?;
    progress.abort();
    Ok(())
}

#[tauri::command]
pub fn get_progress(state: tauri::State<'_, AppState>) -> Result<ProgressEvent, String> {
    let progress = state
        .progress
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .ok_or_else(|| "No active scan".to_string())?;

    let phase = if progress.files_to_hash.load(std::sync::atomic::Ordering::Relaxed) == 0 {
        "walking"
    } else {
        "hashing"
    };

    Ok(ProgressEvent {
        phase: phase.to_string(),
        files_walked: progress
            .files_walked
            .load(std::sync::atomic::Ordering::Relaxed),
        files_to_hash: progress
            .files_to_hash
            .load(std::sync::atomic::Ordering::Relaxed),
        files_hashed: progress
            .files_hashed
            .load(std::sync::atomic::Ordering::Relaxed),
        bytes_hashed: progress
            .bytes_hashed
            .load(std::sync::atomic::Ordering::Relaxed),
    })
}

#[tauri::command]
pub fn open_database(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ScanInfo, String> {
    let db =
        Arc::new(Database::new(&path).map_err(|e| format!("Failed to open database: {}", e))?);

    let scan_id = db
        .get_scan_info(1)
        .map_err(|e| format!("Failed to get scan info: {}", e))?
        .id;

    {
        let mut db_lock = state.db.lock().unwrap();
        *db_lock = Some(db);
        let mut scan_lock = state.scan_id.lock().unwrap();
        *scan_lock = Some(scan_id);
        let mut path_lock = state.db_path.lock().unwrap();
        *path_lock = Some(PathBuf::from(path));
    }

    let db = get_db(&state)?;
    db.get_scan_info(scan_id)
        .map_err(|e| format!("Failed to get scan info: {}", e))
}

#[tauri::command]
pub fn get_duplicates(state: tauri::State<'_, AppState>) -> Result<Vec<DuplicateGroup>, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    db.get_duplicate_groups(scan_id)
        .map_err(|e| format!("Failed to get duplicates: {}", e))
}

#[tauri::command]
pub fn get_duplicates_paginated(
    state: tauri::State<'_, AppState>,
    offset: i64,
    limit: i64,
) -> Result<Vec<DuplicateGroup>, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    db.get_duplicate_groups_paginated(scan_id, offset, limit)
        .map_err(|e| format!("Failed to get duplicates: {}", e))
}

#[tauri::command]
pub fn get_duplicate_count(state: tauri::State<'_, AppState>) -> Result<i64, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    db.get_duplicate_group_count(scan_id)
        .map_err(|e| format!("Failed to get duplicate count: {}", e))
}

#[tauri::command]
pub fn get_stats(state: tauri::State<'_, AppState>) -> Result<Stats, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    db.get_stats(scan_id)
        .map_err(|e| format!("Failed to get stats: {}", e))
}

#[tauri::command]
pub fn list_scans(app: AppHandle) -> Result<Vec<PathBuf>, String> {
    let dir = app_data_dir(&app)?;
    let dismissed = get_dismissed_scans(&dir)?;
    
    let mut scans = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("db")
                && !dismissed.contains(&path)
            {
                scans.push(path);
            }
        }
    }
    scans.sort();
    scans.reverse();
    Ok(scans)
}

#[tauri::command]
pub fn finalize_scan(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let use_tmp = state.use_tmp_db;
    if !use_tmp {
        return Ok(None);
    }

    let tmp_path = {
        let path_lock = state.db_path.lock().unwrap();
        path_lock.clone().ok_or("No database path")?
    };

    {
        let mut db_lock = state.db.lock().unwrap();
        *db_lock = None;
    }

    let final_path = move_db_from_tmp(&tmp_path, &app)?;
    let final_path_str = final_path.to_string_lossy().to_string();

    let db = Arc::new(
        Database::new(&final_path).map_err(|e| format!("Failed to reopen database: {}", e))?,
    );

    {
        let mut db_lock = state.db.lock().unwrap();
        *db_lock = Some(db);
        let mut path_lock = state.db_path.lock().unwrap();
        *path_lock = Some(final_path);
    }

    Ok(Some(final_path_str))
}

#[tauri::command]
pub fn dismiss_scan(app: AppHandle, path: String) -> Result<(), String> {
    let dir = app_data_dir(&app)?;
    let mut dismissed = get_dismissed_scans(&dir)?;
    let path_buf = PathBuf::from(&path);
    if !dismissed.contains(&path_buf) {
        dismissed.push(path_buf);
        save_dismissed_scans(&dir, &dismissed)?;
    }
    Ok(())
}

#[tauri::command]
pub fn trash_file(path: String) -> Result<(), String> {
    trash::delete(&path).map_err(|e| format!("Failed to trash file: {}", e))
}

fn get_dismissed_scans(dir: &PathBuf) -> Result<Vec<PathBuf>, String> {
    let config_path = dir.join("dismissed.json");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read dismissed scans: {}", e))?;
    let paths: Vec<String> = serde_json::from_str(&content).unwrap_or_default();
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

fn save_dismissed_scans(dir: &PathBuf, dismissed: &[PathBuf]) -> Result<(), String> {
    let config_path = dir.join("dismissed.json");
    let paths: Vec<String> = dismissed
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let content = serde_json::to_string_pretty(&paths)
        .map_err(|e| format!("Failed to serialize dismissed scans: {}", e))?;
    std::fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write dismissed scans: {}", e))?;
    Ok(())
}

#[tauri::command]
pub async fn add_path_to_scan(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    db_path: String,
    new_path: String,
) -> Result<i64, String> {
    let db = Arc::new(
        Database::new(&db_path).map_err(|e| format!("Failed to open database: {}", e))?,
    );

    let scan_id = db
        .create_scan(&new_path)
        .map_err(|e| format!("Failed to create scan: {}", e))?;

    {
        let mut db_lock = state.db.lock().unwrap();
        *db_lock = Some(db.clone());
        let mut scan_lock = state.scan_id.lock().unwrap();
        *scan_lock = Some(scan_id);
        let mut path_lock = state.db_path.lock().unwrap();
        *path_lock = Some(PathBuf::from(&db_path));
    }

    let progress = Arc::new(ScanProgress::new());
    {
        let mut prog_lock = state.progress.lock().unwrap();
        *prog_lock = Some(progress.clone());
    }

    let app_clone = app.clone();
    let db_clone = db.clone();
    let progress_clone = progress.clone();

    std::thread::spawn(move || {
        let app_handle = app_clone;
        let db = db_clone;
        let progress = progress_clone;

        let _ = walk_and_collect(&new_path, &db, scan_id, &progress);

        if !progress.is_aborted() {
            let _ = hash_candidates(&db, scan_id, &progress);
        }

        if progress.is_aborted() {
            let _ = db.abort_scan(scan_id);
        } else {
            let _ = db.complete_scan(scan_id);
            let _ = db.create_indexes();
        }

        let stats = db.get_stats(scan_id).unwrap_or(Stats {
            file_count: 0,
            total_size: 0,
            duplicate_groups: 0,
            duplicate_files: 0,
            wasted_space: 0,
        });

        let _ = app_handle.emit(
            "scan-complete",
            ScanComplete {
                scan_id,
                stats,
                aborted: progress.is_aborted(),
            },
        );
    });

    let app_clone = app.clone();
    let progress_clone = progress.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if progress_clone.is_aborted() {
                break;
            }
            let phase = if progress_clone.files_to_hash.load(std::sync::atomic::Ordering::Relaxed)
                == 0
            {
                "walking"
            } else {
                "hashing"
            };
            let _ = app_clone.emit(
                "scan-progress",
                ProgressEvent {
                    phase: phase.to_string(),
                    files_walked: progress_clone
                        .files_walked
                        .load(std::sync::atomic::Ordering::Relaxed),
                    files_to_hash: progress_clone
                        .files_to_hash
                        .load(std::sync::atomic::Ordering::Relaxed),
                    files_hashed: progress_clone
                        .files_hashed
                        .load(std::sync::atomic::Ordering::Relaxed),
                    bytes_hashed: progress_clone
                        .bytes_hashed
                        .load(std::sync::atomic::Ordering::Relaxed),
                },
            );
        }
    });

    Ok(scan_id)
}

#[tauri::command]
pub fn merge_databases(
    state: tauri::State<'_, AppState>,
    source_db_path: String,
) -> Result<MergeResult, String> {
    let _target_db_path = {
        let path_lock = state.db_path.lock().unwrap();
        path_lock
            .as_ref()
            .ok_or("No target database loaded")?
            .clone()
    };

    let source_db = Database::new(&source_db_path)
        .map_err(|e| format!("Failed to open source database: {}", e))?;

    let target_db = {
        let db_lock = state.db.lock().unwrap();
        db_lock
            .as_ref()
            .ok_or("No target database loaded")?
            .clone()
    };

    let source_scans = source_db
        .get_all_scans()
        .map_err(|e| format!("Failed to get source scans: {}", e))?;

    let mut files_merged = 0;
    let mut scans_merged = 0;

    for source_scan in source_scans {
        let new_scan_id = target_db
            .create_scan(&source_scan.path)
            .map_err(|e| format!("Failed to create scan: {}", e))?;

        let source_files = source_db
            .get_files_for_scan(source_scan.id)
            .map_err(|e| format!("Failed to get source files: {}", e))?;

        for file in source_files {
            target_db
                .insert_file(
                    new_scan_id,
                    &file.path,
                    file.hash.as_deref(),
                    file.size,
                    file.modified,
                )
                .map_err(|e| format!("Failed to insert file: {}", e))?;
            files_merged += 1;
        }

        if source_scan.status == "completed" {
            let _ = target_db.complete_scan(new_scan_id);
        }

        scans_merged += 1;
    }

    let _ = target_db.create_indexes();

    Ok(MergeResult {
        scans_merged,
        files_merged,
    })
}

#[tauri::command]
pub fn get_db_path(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let path = state.db_path.lock().unwrap();
    Ok(path.as_ref().map(|p| p.to_string_lossy().into_owned()))
}
