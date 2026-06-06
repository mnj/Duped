use crate::db::{Database, DuplicateGroup, FileRecord, MaterializedPhotoGroupRecord, ScanInfo, Stats};
use crate::scanner::{hash_candidates, optimize_matching_groups, phash_images, walk_and_collect, ScanProgress};
use image::{DynamicImage, ImageFormat, Luma, imageops::FilterType};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub phase: String,
    pub detail: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct PhotoGroup {
    pub files: Vec<FileRecord>,
    pub min_similarity: f64,
    pub avg_similarity: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoGroupsPage {
    pub total: i64,
    pub groups: Vec<PhotoGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImagePreview {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_seconds: Option<f64>,
    pub codec: Option<String>,
    pub format_name: Option<String>,
    pub ffprobe_streams_json: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub struct AppState {
    pub db: Mutex<Option<Arc<Database>>>,
    pub scan_id: Mutex<Option<i64>>,
    pub progress: Mutex<Option<Arc<ScanProgress>>>,
    pub db_path: Mutex<Option<PathBuf>>,
    pub use_tmp_db: bool,
    pub storage_dir: Option<PathBuf>,
}

impl AppState {
    pub fn new(use_tmp_db: bool, storage_dir: Option<PathBuf>) -> Self {
        Self {
            db: Mutex::new(None),
            scan_id: Mutex::new(None),
            progress: Mutex::new(None),
            db_path: Mutex::new(None),
            use_tmp_db,
            storage_dir,
        }
    }
}

fn clear_active_scan_state(state: &AppState) {
    let mut progress_lock = state.progress.lock().unwrap();
    *progress_lock = None;
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

fn storage_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let state: tauri::State<'_, AppState> = app.state();

    if let Some(dir) = state.storage_dir.clone() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create configured storage dir: {}", e))?;
        return Ok(dir);
    }

    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get fallback app data dir: {}", e))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create fallback app data dir: {}", e))?;
    Ok(dir)
}

fn scan_db_path(app: &AppHandle, use_tmp: bool) -> Result<PathBuf, String> {
    if use_tmp {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("duped_scan_{}.db", timestamp);
        Ok(std::env::temp_dir().join(filename))
    } else {
        let dir = storage_dir(app)?;
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("scan_{}.db", timestamp);
        Ok(dir.join(filename))
    }
}

fn move_db_from_tmp(tmp_path: &PathBuf, app: &AppHandle) -> Result<PathBuf, String> {
    let dir = storage_dir(app)?;
    let filename = tmp_path.file_name().ok_or("Invalid temp path")?;
    let final_path = dir.join(filename);
    eprintln!(
        "finalize_scan: moving database from '{}' to '{}'",
        tmp_path.display(),
        final_path.display()
    );
    match std::fs::rename(tmp_path, &final_path) {
        Ok(_) => {
            eprintln!("finalize_scan: moved database with rename");
        }
        Err(_) => {
            eprintln!("finalize_scan: rename failed, falling back to copy");
            std::fs::copy(tmp_path, &final_path)
                .map_err(|e| format!("Failed to copy database: {}", e))?;
            let _ = std::fs::remove_file(tmp_path);
            eprintln!("finalize_scan: copied database and removed temp file");
        }
    }
    Ok(final_path)
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<i64, String> {
    clear_active_scan_state(&state);

    let use_tmp = state.use_tmp_db;
    let db_path = scan_db_path(&app, use_tmp)?;

    let db = Arc::new(
        Database::new(&db_path).map_err(|e| format!("Failed to create database: {}", e))?,
    );

    let scan_id = db
        .get_or_create_scan(&path)
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

        if !progress.is_aborted() {
            let _ = phash_images(&db, scan_id, &progress);
        }

        if !progress.is_aborted() {
            let _ = optimize_matching_groups(&db, scan_id, &progress);
        }

        if progress.is_aborted() {
            let _ = db.abort_scan(scan_id);
        } else {
            let _ = db.create_indexes();
            let _ = db.complete_scan(scan_id);
        }

        progress.complete();

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
            if progress_clone.is_aborted() || progress_clone.is_completed() {
                break;
            }
            let _ = app_clone.emit(
                "scan-progress",
                ProgressEvent {
                    phase: progress_clone.phase_name().to_string(),
                    detail: progress_clone.detail(),
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

    Ok(ProgressEvent {
        phase: progress.phase_name().to_string(),
        detail: progress.detail(),
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
    clear_active_scan_state(&state);

    let db =
        Arc::new(Database::new(&path).map_err(|e| format!("Failed to open database: {}", e))?);

    let scan_id = db
        .get_latest_scan_info()
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
    let dir = storage_dir(&app)?;
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
        eprintln!("finalize_scan: tmp db disabled, nothing to finalize");
        return Ok(None);
    }

    let tmp_path = {
        let path_lock = state.db_path.lock().unwrap();
        path_lock.clone().ok_or("No database path")?
    };
    eprintln!("finalize_scan: current database path is '{}'", tmp_path.display());

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

    eprintln!("finalize_scan: reopened finalized database at '{}'", final_path_str);

    Ok(Some(final_path_str))
}

#[tauri::command]
pub fn dismiss_scan(app: AppHandle, path: String) -> Result<(), String> {
    let dir = storage_dir(&app)?;
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

#[tauri::command]
pub fn replace_with_symlink(path: String, target_path: String) -> Result<(), String> {
    let path_buf = PathBuf::from(&path);
    let target_buf = PathBuf::from(&target_path);

    if path_buf == target_buf {
        return Err("Cannot replace a file with a symlink to itself".to_string());
    }

    if !target_buf.exists() {
        return Err("Target file does not exist".to_string());
    }

    trash::delete(&path).map_err(|e| format!("Failed to remove file before symlink: {}", e))?;

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target_buf, &path_buf)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(&target_buf, &path_buf)
            .map_err(|e| format!("Failed to create symlink: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub fn load_image_preview(path: String) -> Result<ImagePreview, String> {
    let bytes = fs::read(&path).map_err(|e| format!("Failed to read image: {}", e))?;
    let mime_type = match PathBuf::from(&path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
    .to_string();

    Ok(ImagePreview { bytes, mime_type })
}

fn prune_ffprobe_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("disposition");
            for child in map.values_mut() {
                prune_ffprobe_value(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                prune_ffprobe_value(item);
            }
        }
        _ => {}
    }
}

fn grayscale_values(image: &DynamicImage, width: u32, height: u32) -> Vec<f32> {
    image
        .resize_exact(width, height, FilterType::Triangle)
        .to_luma8()
        .pixels()
        .map(|p| p[0] as f32)
        .collect()
}

fn build_integral(values: &[f32], width: u32, height: u32) -> Vec<f64> {
    let stride = width as usize + 1;
    let mut integral = vec![0.0f64; stride * (height as usize + 1)];
    for y in 0..height as usize {
        let mut row_sum = 0.0f64;
        for x in 0..width as usize {
            row_sum += values[y * width as usize + x] as f64;
            integral[(y + 1) * stride + (x + 1)] = integral[y * stride + (x + 1)] + row_sum;
        }
    }
    integral
}

fn rect_sum(integral: &[f64], width: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let stride = width as usize + 1;
    let a = integral[y0 as usize * stride + x0 as usize];
    let b = integral[y0 as usize * stride + x1 as usize];
    let c = integral[y1 as usize * stride + x0 as usize];
    let d = integral[y1 as usize * stride + x1 as usize];
    d - b - c + a
}

fn otsu_threshold(values: &[u8]) -> u8 {
    let mut histogram = [0u32; 256];
    for &value in values {
        histogram[value as usize] += 1;
    }

    let total = values.len() as f32;
    let mut sum = 0.0f32;
    for (i, &count) in histogram.iter().enumerate() {
        sum += i as f32 * count as f32;
    }

    let mut sum_background = 0.0f32;
    let mut weight_background = 0.0f32;
    let mut best_variance = 0.0f32;
    let mut best_threshold = 0u8;

    for (threshold, &count) in histogram.iter().enumerate() {
        weight_background += count as f32;
        if weight_background == 0.0 {
            continue;
        }

        let weight_foreground = total - weight_background;
        if weight_foreground == 0.0 {
            break;
        }

        sum_background += threshold as f32 * count as f32;
        let mean_background = sum_background / weight_background;
        let mean_foreground = (sum - sum_background) / weight_foreground;
        let variance = weight_background
            * weight_foreground
            * (mean_background - mean_foreground)
            * (mean_background - mean_foreground);

        if variance > best_variance {
            best_variance = variance;
            best_threshold = threshold as u8;
        }
    }

    best_threshold
}

fn dilate(mask: &[bool], width: u32, height: u32) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut on = false;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    if mask[ny as usize * width as usize + nx as usize] {
                        on = true;
                        break;
                    }
                }
                if on {
                    break;
                }
            }
            out[y as usize * width as usize + x as usize] = on;
        }
    }
    out
}

fn erode(mask: &[bool], width: u32, height: u32) -> Vec<bool> {
    let mut out = vec![false; mask.len()];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut on = true;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        on = false;
                        break;
                    }
                    if !mask[ny as usize * width as usize + nx as usize] {
                        on = false;
                        break;
                    }
                }
                if !on {
                    break;
                }
            }
            out[y as usize * width as usize + x as usize] = on;
        }
    }
    out
}

fn detect_difference_boxes(left: &DynamicImage, right: &DynamicImage) -> Vec<DiffBox> {
    let base_width = 384u32;
    let left_height = ((left.height() as f32 / left.width() as f32) * base_width as f32).round().max(1.0) as u32;
    let right_height = ((right.height() as f32 / right.width() as f32) * base_width as f32).round().max(1.0) as u32;
    let base_height = left_height.min(right_height).max(32);
    let left_values = grayscale_values(left, base_width, base_height);
    let right_values = grayscale_values(right, base_width, base_height);

    let left_sq: Vec<f32> = left_values.iter().map(|v| v * v).collect();
    let right_sq: Vec<f32> = right_values.iter().map(|v| v * v).collect();
    let cross: Vec<f32> = left_values
        .iter()
        .zip(right_values.iter())
        .map(|(l, r)| l * r)
        .collect();

    let left_int = build_integral(&left_values, base_width, base_height);
    let right_int = build_integral(&right_values, base_width, base_height);
    let left_sq_int = build_integral(&left_sq, base_width, base_height);
    let right_sq_int = build_integral(&right_sq, base_width, base_height);
    let cross_int = build_integral(&cross, base_width, base_height);

    let mut diff_map = vec![0u8; (base_width * base_height) as usize];

    const C1: f32 = 6.5025;
    const C2: f32 = 58.5225;

    let radius = 3u32;
    for y in 0..base_height {
        for x in 0..base_width {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(base_width);
            let y1 = (y + radius + 1).min(base_height);
            let samples = ((x1 - x0) * (y1 - y0)).max(1) as f32;

            let left_sum = rect_sum(&left_int, base_width, x0, y0, x1, y1) as f32;
            let right_sum = rect_sum(&right_int, base_width, x0, y0, x1, y1) as f32;
            let left_sq_sum = rect_sum(&left_sq_int, base_width, x0, y0, x1, y1) as f32;
            let right_sq_sum = rect_sum(&right_sq_int, base_width, x0, y0, x1, y1) as f32;
            let cross_sum = rect_sum(&cross_int, base_width, x0, y0, x1, y1) as f32;

            let mean_left = left_sum / samples;
            let mean_right = right_sum / samples;
            let variance_left = (left_sq_sum / samples) - mean_left * mean_left;
            let variance_right = (right_sq_sum / samples) - mean_right * mean_right;
            let covariance = (cross_sum / samples) - mean_left * mean_right;

            let numerator = (2.0 * mean_left * mean_right + C1) * (2.0 * covariance + C2);
            let denominator = (mean_left * mean_left + mean_right * mean_right + C1)
                * (variance_left + variance_right + C2);
            let ssim = if denominator.abs() < f32::EPSILON {
                1.0
            } else {
                (numerator / denominator).clamp(-1.0, 1.0)
            };

            diff_map[(y * base_width + x) as usize] = ((1.0 - ssim) * 255.0).clamp(0.0, 255.0) as u8;
        }
    }

    let threshold = otsu_threshold(&diff_map).max(28);
    let mut changed = vec![false; diff_map.len()];
    for (idx, &score) in diff_map.iter().enumerate() {
        changed[idx] = score >= threshold;
    }

    let changed = dilate(
        &erode(&dilate(&changed, base_width, base_height), base_width, base_height),
        base_width,
        base_height,
    );

    let mut visited = vec![false; changed.len()];
    let mut boxes = Vec::new();

    for gy in 0..base_height {
        for gx in 0..base_width {
            let idx = (gy * base_width + gx) as usize;
            if !changed[idx] || visited[idx] {
                continue;
            }

            let mut queue = std::collections::VecDeque::from([(gx, gy)]);
            visited[idx] = true;
            let mut min_x = gx;
            let mut min_y = gy;
            let mut max_x = gx;
            let mut max_y = gy;
            let mut cells = 0u32;

            while let Some((cx, cy)) = queue.pop_front() {
                cells += 1;
                min_x = min_x.min(cx);
                min_y = min_y.min(cy);
                max_x = max_x.max(cx);
                max_y = max_y.max(cy);

                for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= base_width as i32 || ny >= base_height as i32 {
                        continue;
                    }
                    let nidx = (ny as u32 * base_width + nx as u32) as usize;
                    if changed[nidx] && !visited[nidx] {
                        visited[nidx] = true;
                        queue.push_back((nx as u32, ny as u32));
                    }
                }
            }

            if cells < 40 {
                continue;
            }

            let width = max_x - min_x + 1;
            let height = max_y - min_y + 1;
            let area_ratio = (width * height) as f32 / (base_width * base_height) as f32;
            if area_ratio > 0.30 {
                continue;
            }

            let pad = 4u32;
            let box_x = min_x.saturating_sub(pad);
            let box_y = min_y.saturating_sub(pad);
            let box_w = (width + pad * 2).min(base_width - box_x);
            let box_h = (height + pad * 2).min(base_height - box_y);

            boxes.push(DiffBox {
                x: ((box_x as f32 / base_width as f32) * right.width() as f32).round() as u32,
                y: ((box_y as f32 / base_height as f32) * right.height() as f32).round() as u32,
                width: ((box_w as f32 / base_width as f32) * right.width() as f32).round() as u32,
                height: ((box_h as f32 / base_height as f32) * right.height() as f32).round() as u32,
            });
        }
    }

    boxes.sort_by_key(|b| std::cmp::Reverse(b.width * b.height));
    boxes.truncate(4);
    boxes
}

fn build_difference_mask(left: &DynamicImage, right: &DynamicImage) -> Result<Vec<u8>, String> {
    let target_width = right.width().max(1);
    let target_height = right.height().max(1);
    let left_gray = left
        .resize_exact(target_width, target_height, FilterType::Triangle)
        .to_luma8();
    let right_gray = right
        .resize_exact(target_width, target_height, FilterType::Triangle)
        .to_luma8();

    let mut mask = image::GrayImage::new(target_width, target_height);
    for y in 0..target_height {
        for x in 0..target_width {
            let left_value = left_gray.get_pixel(x, y)[0];
            let right_value = right_gray.get_pixel(x, y)[0];
            let diff = left_value.abs_diff(right_value);
            let value = 255u8.saturating_sub(diff);
            mask.put_pixel(x, y, Luma([value]));
        }
    }

    let mut cursor = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageLuma8(mask)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode difference mask: {}", e))?;
    Ok(cursor.into_inner())
}

#[tauri::command]
pub fn load_file_metadata(path: String) -> Result<FileMetadata, String> {
    let path_buf = PathBuf::from(&path);

    let mut metadata = FileMetadata {
        width: None,
        height: None,
        duration_seconds: None,
        codec: None,
        format_name: None,
        ffprobe_streams_json: None,
    };

    if let Ok((width, height)) = image::image_dimensions(&path_buf) {
        metadata.width = Some(width);
        metadata.height = Some(height);
    }

    let ffprobe_output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-output_format",
            "json",
            "-show_streams",
            &path,
        ])
        .output();

    let Ok(output) = ffprobe_output else {
        return Ok(metadata);
    };

    if !output.status.success() {
        return Ok(metadata);
    }

    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    let mut pruned_value = value.clone();
    prune_ffprobe_value(&mut pruned_value);
    metadata.ffprobe_streams_json = serde_json::to_string_pretty(&pruned_value).ok();

    if let Some(format) = value.get("format") {
        if metadata.duration_seconds.is_none() {
            metadata.duration_seconds = format
                .get("duration")
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse::<f64>().ok());
        }
        metadata.format_name = format
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
    }

    if let Some(streams) = value.get("streams").and_then(|s| s.as_array()) {
        if let Some(stream) = streams.iter().find(|stream| {
            matches!(
                stream.get("codec_type").and_then(|v| v.as_str()),
                Some("video") | Some("audio")
            )
        }) {
            if metadata.width.is_none() {
                metadata.width = stream
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok());
            }
            if metadata.height.is_none() {
                metadata.height = stream
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok());
            }
            if metadata.duration_seconds.is_none() {
                metadata.duration_seconds = stream
                    .get("duration")
                    .and_then(|d| d.as_str())
                    .and_then(|d| d.parse::<f64>().ok());
            }
            metadata.codec = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }

    Ok(metadata)
}

#[tauri::command]
pub fn load_difference_boxes(reference_path: String, candidate_path: String) -> Result<Vec<DiffBox>, String> {
    let left = image::open(&reference_path)
        .map_err(|e| format!("Failed to open reference image: {}", e))?;
    let right = image::open(&candidate_path)
        .map_err(|e| format!("Failed to open candidate image: {}", e))?;
    Ok(detect_difference_boxes(&left, &right))
}

#[tauri::command]
pub fn load_difference_mask(reference_path: String, candidate_path: String) -> Result<ImagePreview, String> {
    let left = image::open(&reference_path)
        .map_err(|e| format!("Failed to open reference image: {}", e))?;
    let right = image::open(&candidate_path)
        .map_err(|e| format!("Failed to open candidate image: {}", e))?;
    let bytes = build_difference_mask(&left, &right)?;
    Ok(ImagePreview {
        bytes,
        mime_type: "image/png".to_string(),
    })
}

fn to_photo_groups(groups: Vec<MaterializedPhotoGroupRecord>) -> Vec<PhotoGroup> {
    groups
        .into_iter()
        .map(|group| {
            PhotoGroup {
                files: group.files,
                min_similarity: group.min_similarity,
                avg_similarity: group.avg_similarity,
            }
        })
        .collect()
}

#[tauri::command]
pub fn get_photo_groups_page(
    state: tauri::State<'_, AppState>,
    min_similarity: f64,
    offset: i64,
    limit: i64,
) -> Result<PhotoGroupsPage, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    let total = db
        .get_photo_group_count(scan_id, min_similarity)
        .map_err(|e| format!("Failed to get photo group count: {}", e))?;
    let groups = db
        .get_photo_group_page(scan_id, min_similarity, offset, limit)
        .map_err(|e| format!("Failed to get photo groups: {}", e))?;

    Ok(PhotoGroupsPage {
        total,
        groups: to_photo_groups(groups),
    })
}

#[tauri::command]
pub fn get_photo_group(
    state: tauri::State<'_, AppState>,
    min_similarity: f64,
    index: i64,
) -> Result<Option<PhotoGroup>, String> {
    let db = get_db(&state)?;
    let scan_id = get_scan_id(&state)?;
    let groups = db
        .get_photo_group_page(scan_id, min_similarity, index, 1)
        .map_err(|e| format!("Failed to get photo group: {}", e))?;
    Ok(to_photo_groups(groups).into_iter().next())
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
    clear_active_scan_state(&state);

    let db = Arc::new(
        Database::new(&db_path).map_err(|e| format!("Failed to open database: {}", e))?,
    );

    let scan_id = db
        .get_or_create_scan(&new_path)
        .map_err(|e| format!("Failed to get scan: {}", e))?;

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

        if !progress.is_aborted() {
            let _ = phash_images(&db, scan_id, &progress);
        }

        if !progress.is_aborted() {
            let _ = optimize_matching_groups(&db, scan_id, &progress);
        }

        if progress.is_aborted() {
            let _ = db.abort_scan(scan_id);
        } else {
            let _ = db.create_indexes();
            let _ = db.complete_scan(scan_id);
        }

        progress.complete();

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
            if progress_clone.is_aborted() || progress_clone.is_completed() {
                break;
            }
            let _ = app_clone.emit(
                "scan-progress",
                ProgressEvent {
                    phase: progress_clone.phase_name().to_string(),
                    detail: progress_clone.detail(),
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

    let target_scan_id = target_db
        .get_or_create_scan("merged")
        .map_err(|e| format!("Failed to get target scan: {}", e))?;

    let mut files_merged = 0;

    let source_scan = source_db
        .get_latest_scan_info()
        .map_err(|e| format!("Failed to get source scan: {}", e))?;

    let source_files = source_db
        .get_files_for_scan(source_scan.id)
        .map_err(|e| format!("Failed to get source files: {}", e))?;

    for file in source_files {
        target_db
            .insert_or_replace_file(
                target_scan_id,
                &file.path,
                file.hash.as_deref(),
                file.size,
                file.modified,
                file.phash,
            )
            .map_err(|e| format!("Failed to insert file: {}", e))?;
        files_merged += 1;
    }

    if source_scan.status == "completed" {
        let _ = target_db.complete_scan(target_scan_id);
    }

    let _ = target_db.create_indexes();

    Ok(MergeResult {
        scans_merged: 1,
        files_merged,
    })
}

#[tauri::command]
pub fn get_db_path(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let path = state.db_path.lock().unwrap();
    Ok(path.as_ref().map(|p| p.to_string_lossy().into_owned()))
}
