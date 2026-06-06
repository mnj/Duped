use crate::db::{Database, FileRecord};
use crate::phasher;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct ScanProgress {
    pub files_walked: AtomicU64,
    pub files_to_hash: AtomicU64,
    pub files_hashed: AtomicU64,
    pub bytes_hashed: AtomicU64,
    pub aborted: AtomicBool,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            files_walked: AtomicU64::new(0),
            files_to_hash: AtomicU64::new(0),
            files_hashed: AtomicU64::new(0),
            bytes_hashed: AtomicU64::new(0),
            aborted: AtomicBool::new(false),
        }
    }

    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }
}

pub fn hash_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = fs::File::open(path.as_ref())?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn walk_and_collect<P: AsRef<Path>>(
    path: P,
    db: &Database,
    scan_id: i64,
    progress: &ScanProgress,
) -> Result<(), Box<dyn Error>> {
    let walker = WalkBuilder::new(path.as_ref())
        .follow_links(false)
        .hidden(false)
        .threads(num_cpus())
        .build();

    let mut batch = Vec::with_capacity(50_000);

    for entry in walker {
        if progress.is_aborted() {
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let _ft = match entry.file_type() {
            Some(ft) if ft.is_file() => ft,
            _ => continue,
        };

        let path = entry.path();
        let metadata = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        batch.push(FileRecord {
            id: 0,
            path: path.to_string_lossy().into_owned(),
            hash: None,
            size: metadata.len() as i64,
            modified,
            phash: None,
        });

        progress.files_walked.fetch_add(1, Ordering::Relaxed);

        if batch.len() >= 50_000 {
            db.batch_insert_files(scan_id, &batch)?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        db.batch_insert_files(scan_id, &batch)?;
    }

    Ok(())
}

pub fn hash_candidates(
    db: &Database,
    scan_id: i64,
    progress: &ScanProgress,
) -> Result<(), Box<dyn Error>> {
    let size_groups = db.get_files_by_size_groups(scan_id)?;

    let total_to_hash: u64 = size_groups.iter().map(|g| g.len() as u64).sum();
    progress
        .files_to_hash
        .store(total_to_hash, Ordering::Relaxed);

    for group in &size_groups {
        if progress.is_aborted() {
            break;
        }

        let paths: Vec<String> = group.iter().map(|f| f.path.clone()).collect();

        let results: Vec<(String, Option<String>, u64)> = paths
            .par_iter()
            .filter_map(|path| {
                if progress.is_aborted() {
                    return None;
                }
                match hash_file(path) {
                    Ok(hash) => {
                        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        progress.files_hashed.fetch_add(1, Ordering::Relaxed);
                        progress.bytes_hashed.fetch_add(size, Ordering::Relaxed);
                        Some((path.clone(), Some(hash), size))
                    }
                    Err(_) => {
                        progress.files_hashed.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                }
            })
            .collect();

        for (path, hash, _) in results {
            if let Some(h) = hash {
                let _ = db.update_file_hash(&path, &h);
            }
        }
    }

    Ok(())
}

pub fn phash_images(
    db: &Database,
    scan_id: i64,
    progress: &ScanProgress,
) -> Result<(), Box<dyn Error>> {
    use std::path::PathBuf;

    let db_path = {
        let all_files = db.get_files_for_scan(scan_id)?;
        all_files
            .into_iter()
            .filter(|f| {
                let p = PathBuf::from(&f.path);
                phasher::is_image_file(&p)
            })
            .map(|f| f.path)
            .collect::<Vec<_>>()
    };

    let total = db_path.len() as u64;
    progress.files_to_hash.store(total, Ordering::Relaxed);

    let results: Vec<(String, Option<i64>)> = db_path
        .par_iter()
        .filter_map(|path| {
            if progress.is_aborted() {
                return None;
            }
            let p = Path::new(path);
            match phasher::compute_phash(p) {
                Ok(h) => {
                    progress.files_hashed.fetch_add(1, Ordering::Relaxed);
                    Some((path.clone(), Some(h)))
                }
                Err(_) => {
                    progress.files_hashed.fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        })
        .collect();

    for (path, h) in results {
        if let Some(phash) = h {
            let _ = db.update_file_phash(&path, phash);
        }
    }

    Ok(())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
