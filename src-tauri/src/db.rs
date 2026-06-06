use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub hash: Option<String>,
    pub size: i64,
    pub modified: i64,
    pub phash: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: i64,
    pub files: Vec<FileRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanInfo {
    pub id: i64,
    pub path: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub file_count: i64,
    pub total_size: i64,
    pub duplicate_groups: i64,
    pub duplicate_files: i64,
    pub wasted_space: i64,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS scans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                status TEXT NOT NULL DEFAULT 'running'
            );

            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id INTEGER NOT NULL,
                path TEXT NOT NULL UNIQUE,
                hash TEXT,
                size INTEGER NOT NULL,
                modified INTEGER NOT NULL,
                phash INTEGER,
                FOREIGN KEY (scan_id) REFERENCES scans(id)
            );

            CREATE TABLE IF NOT EXISTS duplicate_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id INTEGER NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                wasted_space INTEGER NOT NULL,
                FOREIGN KEY (scan_id) REFERENCES scans(id)
            );

            CREATE TABLE IF NOT EXISTS duplicate_group_files (
                group_id INTEGER NOT NULL,
                file_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                PRIMARY KEY (group_id, file_id),
                FOREIGN KEY (group_id) REFERENCES duplicate_groups(id),
                FOREIGN KEY (file_id) REFERENCES files(id)
            );

            CREATE TABLE IF NOT EXISTS photo_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id INTEGER NOT NULL,
                threshold INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                min_similarity REAL NOT NULL,
                avg_similarity REAL NOT NULL,
                FOREIGN KEY (scan_id) REFERENCES scans(id)
            );

            CREATE TABLE IF NOT EXISTS photo_group_files (
                group_id INTEGER NOT NULL,
                file_id INTEGER NOT NULL,
                ordinal INTEGER NOT NULL,
                PRIMARY KEY (group_id, file_id),
                FOREIGN KEY (group_id) REFERENCES photo_groups(id),
                FOREIGN KEY (file_id) REFERENCES files(id)
            );

            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            ",
        )?;
        Ok(())
    }

    pub fn create_indexes(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_files_scan_size ON files(scan_id, size);
            CREATE INDEX IF NOT EXISTS idx_files_scan_hash ON files(scan_id, hash);
            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_duplicate_groups_scan_size ON duplicate_groups(scan_id, size DESC);
            CREATE INDEX IF NOT EXISTS idx_duplicate_group_files_group ON duplicate_group_files(group_id, ordinal);
            CREATE INDEX IF NOT EXISTS idx_photo_groups_scan_threshold ON photo_groups(scan_id, threshold, file_count DESC);
            CREATE INDEX IF NOT EXISTS idx_photo_group_files_group ON photo_group_files(group_id, ordinal);
            ",
        )?;
        Ok(())
    }

    pub fn get_or_create_scan(&self, path: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id FROM scans ORDER BY id DESC LIMIT 1",
        )?;

        let existing = stmt.query_row([], |row| row.get(0));
        match existing {
            Ok(id) => Ok(id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let now = chrono::Utc::now().timestamp();
                conn.execute(
                    "INSERT INTO scans (path, started_at, status) VALUES (?1, ?2, 'running')",
                    params![path, now],
                )?;
                Ok(conn.last_insert_rowid())
            }
            Err(err) => Err(err),
        }
    }

    pub fn complete_scan(&self, scan_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE scans SET completed_at = ?1, status = 'completed' WHERE id = ?2",
            params![now, scan_id],
        )?;
        Ok(())
    }

    pub fn abort_scan(&self, scan_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE scans SET completed_at = ?1, status = 'aborted' WHERE id = ?2",
            params![now, scan_id],
        )?;
        Ok(())
    }

    pub fn batch_insert_files(&self, scan_id: i64, files: &[FileRecord]) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare_cached(
            "INSERT INTO files (scan_id, path, hash, size, modified, phash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                scan_id = excluded.scan_id,
                hash = excluded.hash,
                size = excluded.size,
                modified = excluded.modified,
                phash = excluded.phash",
        )?;
        let mut count = 0;
        for f in files {
            count += stmt.execute(params![scan_id, f.path, f.hash, f.size, f.modified, f.phash])?;
        }
        Ok(count)
    }

    pub fn update_file_hash(&self, path: &str, hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET hash = ?1 WHERE path = ?2",
            params![hash, path],
        )?;
        Ok(())
    }

    pub fn update_file_phash(&self, path: &str, phash: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE files SET phash = ?1 WHERE path = ?2",
            params![phash, path],
        )?;
        Ok(())
    }

    pub fn get_duplicate_groups(&self, scan_id: i64) -> Result<Vec<DuplicateGroup>> {
        self.get_duplicate_groups_paginated(scan_id, 0, 100)
    }

    pub fn get_duplicate_groups_paginated(
        &self,
        scan_id: i64,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<DuplicateGroup>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, hash, size FROM duplicate_groups
             WHERE scan_id = ?1
             ORDER BY size DESC, file_count DESC, hash ASC
             LIMIT ?2 OFFSET ?3",
        )?;

        let groups: Vec<(i64, String, i64)> = stmt
            .query_map(params![scan_id, limit, offset], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut result = Vec::new();
        for (group_id, hash, size) in groups {
            let mut file_stmt = conn.prepare(
                "SELECT f.id, f.path, f.hash, f.size, f.modified, f.phash
                 FROM duplicate_group_files dgf
                 JOIN files f ON f.id = dgf.file_id
                 WHERE dgf.group_id = ?1
                 ORDER BY dgf.ordinal ASC, f.path ASC",
            )?;

            let files: Vec<FileRecord> = file_stmt
                .query_map(params![group_id], |row| {
                    Ok(FileRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        hash: row.get(2)?,
                        size: row.get(3)?,
                        modified: row.get(4)?,
                        phash: row.get(5)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            result.push(DuplicateGroup { hash, size, files });
        }

        Ok(result)
    }

    pub fn get_duplicate_group_count(&self, scan_id: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM duplicate_groups WHERE scan_id = ?1",
            params![scan_id],
            |row| row.get(0),
        )
    }

    pub fn get_stats(&self, scan_id: i64) -> Result<Stats> {
        let conn = self.conn.lock().unwrap();

        let file_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM files WHERE scan_id = ?1",
            params![scan_id],
            |row| row.get(0),
        )?;

        let total_size: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size), 0) FROM files WHERE scan_id = ?1",
            params![scan_id],
            |row| row.get(0),
        )?;

        let duplicate_groups: i64 = conn.query_row(
            "SELECT COUNT(*) FROM (
                SELECT hash FROM files
                WHERE scan_id = ?1 AND hash IS NOT NULL
                GROUP BY hash HAVING COUNT(*) > 1
            )",
            params![scan_id],
            |row| row.get(0),
        )?;

        let duplicate_files: i64 = conn.query_row(
            "SELECT COALESCE(SUM(cnt), 0) FROM (
                SELECT COUNT(*) - 1 as cnt FROM files
                WHERE scan_id = ?1 AND hash IS NOT NULL
                GROUP BY hash HAVING COUNT(*) > 1
            )",
            params![scan_id],
            |row| row.get(0),
        )?;

        let wasted_space: i64 = conn.query_row(
            "SELECT COALESCE(SUM(size * (cnt - 1)), 0) FROM (
                SELECT size, COUNT(*) as cnt FROM files
                WHERE scan_id = ?1 AND hash IS NOT NULL
                GROUP BY hash HAVING COUNT(*) > 1
            )",
            params![scan_id],
            |row| row.get(0),
        )?;

        Ok(Stats {
            file_count,
            total_size,
            duplicate_groups,
            duplicate_files,
            wasted_space,
        })
    }

    pub fn get_scan_info(&self, scan_id: i64) -> Result<ScanInfo> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, path, started_at, completed_at, status FROM scans WHERE id = ?1",
            params![scan_id],
            |row| {
                Ok(ScanInfo {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    started_at: row.get(2)?,
                    completed_at: row.get(3)?,
                    status: row.get(4)?,
                })
            },
        )
    }

    pub fn get_latest_scan_info(&self) -> Result<ScanInfo> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, path, started_at, completed_at, status FROM scans ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok(ScanInfo {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    started_at: row.get(2)?,
                    completed_at: row.get(3)?,
                    status: row.get(4)?,
                })
            },
        )
    }

    pub fn get_files_by_size_groups(&self, scan_id: i64) -> Result<Vec<Vec<FileRecord>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, hash, size, modified, phash FROM files
             WHERE scan_id = ?1 AND size IN (
                SELECT size FROM files WHERE scan_id = ?1 GROUP BY size HAVING COUNT(*) > 1
             )
             ORDER BY size DESC, path",
        )?;

        let files: Vec<FileRecord> = stmt
            .query_map(params![scan_id], |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    modified: row.get(4)?,
                    phash: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut groups: Vec<Vec<FileRecord>> = Vec::new();
        let mut current_size: Option<i64> = None;
        let mut current_group: Vec<FileRecord> = Vec::new();

        for file in files {
            if current_size != Some(file.size) {
                if !current_group.is_empty() {
                    groups.push(current_group);
                }
                current_group = Vec::new();
                current_size = Some(file.size);
            }
            current_group.push(file);
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        Ok(groups)
    }

    pub fn get_files_for_scan(&self, scan_id: i64) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, hash, size, modified, phash FROM files WHERE scan_id = ?1 ORDER BY path",
        )?;

        let files: Vec<FileRecord> = stmt
            .query_map(params![scan_id], |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    modified: row.get(4)?,
                    phash: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(files)
    }

    pub fn insert_or_replace_file(
        &self,
        scan_id: i64,
        path: &str,
        hash: Option<&str>,
        size: i64,
        modified: i64,
        phash: Option<i64>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO files (scan_id, path, hash, size, modified, phash) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                scan_id = excluded.scan_id,
                hash = excluded.hash,
                size = excluded.size,
                modified = excluded.modified,
                phash = excluded.phash",
            params![scan_id, path, hash, size, modified, phash],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_photo_groups(
        &self,
        scan_id: i64,
        min_similarity: f64,
    ) -> Result<Vec<Vec<FileRecord>>> {
        let conn = self.conn.lock().unwrap();
        let threshold = nearest_photo_threshold(min_similarity);
        let mut stmt = conn.prepare(
            "SELECT id FROM photo_groups
             WHERE scan_id = ?1 AND threshold = ?2
             ORDER BY file_count DESC, avg_similarity DESC, id ASC",
        )?;

        let group_ids: Vec<i64> = stmt
            .query_map(params![scan_id, threshold], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut groups = Vec::new();
        for group_id in group_ids {
            let mut file_stmt = conn.prepare(
                "SELECT f.id, f.path, f.hash, f.size, f.modified, f.phash
                 FROM photo_group_files pgf
                 JOIN files f ON f.id = pgf.file_id
                 WHERE pgf.group_id = ?1
                 ORDER BY pgf.ordinal ASC, f.path ASC",
            )?;

            let files: Vec<FileRecord> = file_stmt
                .query_map(params![group_id], |row| {
                    Ok(FileRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        hash: row.get(2)?,
                        size: row.get(3)?,
                        modified: row.get(4)?,
                        phash: row.get(5)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();

            groups.push(files);
        }

        Ok(groups)
    }

    pub fn rebuild_duplicate_groups(&self, scan_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM duplicate_group_files WHERE group_id IN (SELECT id FROM duplicate_groups WHERE scan_id = ?1)", params![scan_id])?;
        conn.execute("DELETE FROM duplicate_groups WHERE scan_id = ?1", params![scan_id])?;

        let mut group_stmt = conn.prepare(
            "SELECT hash, MIN(size) as size, COUNT(*) as file_count
             FROM files
             WHERE scan_id = ?1 AND hash IS NOT NULL
             GROUP BY hash
             HAVING COUNT(*) > 1
             ORDER BY size DESC, hash ASC",
        )?;

        let groups: Vec<(String, i64, i64)> = group_stmt
            .query_map(params![scan_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut insert_group = conn.prepare_cached(
            "INSERT INTO duplicate_groups (scan_id, hash, size, file_count, wasted_space)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut insert_member = conn.prepare_cached(
            "INSERT INTO duplicate_group_files (group_id, file_id, ordinal)
             VALUES (?1, ?2, ?3)",
        )?;
        let mut files_stmt = conn.prepare(
            "SELECT id FROM files WHERE scan_id = ?1 AND hash = ?2 ORDER BY path ASC",
        )?;

        for (hash, size, file_count) in groups {
            insert_group.execute(params![scan_id, hash, size, file_count, size * (file_count - 1)])?;
            let group_id = conn.last_insert_rowid();
            let file_ids: Vec<i64> = files_stmt
                .query_map(params![scan_id, hash], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            for (ordinal, file_id) in file_ids.iter().enumerate() {
                insert_member.execute(params![group_id, file_id, ordinal as i64])?;
            }
        }

        Ok(())
    }

    pub fn rebuild_photo_groups(&self, scan_id: i64, threshold: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM photo_group_files WHERE group_id IN (SELECT id FROM photo_groups WHERE scan_id = ?1 AND threshold = ?2)",
            params![scan_id, threshold],
        )?;
        conn.execute(
            "DELETE FROM photo_groups WHERE scan_id = ?1 AND threshold = ?2",
            params![scan_id, threshold],
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, path, hash, size, modified, phash FROM files
             WHERE scan_id = ?1 AND phash IS NOT NULL
             ORDER BY path",
        )?;

        let files: Vec<FileRecord> = stmt
            .query_map(params![scan_id], |row| {
                Ok(FileRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    hash: row.get(2)?,
                    size: row.get(3)?,
                    modified: row.get(4)?,
                    phash: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        let groups = build_photo_groups_for_threshold(&files, threshold as f64)?;

        let mut insert_group = conn.prepare_cached(
            "INSERT INTO photo_groups (scan_id, threshold, file_count, min_similarity, avg_similarity)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut insert_member = conn.prepare_cached(
            "INSERT INTO photo_group_files (group_id, file_id, ordinal)
             VALUES (?1, ?2, ?3)",
        )?;

        for group in groups {
            insert_group.execute(params![
                scan_id,
                threshold,
                group.files.len() as i64,
                group.min_similarity,
                group.avg_similarity
            ])?;
            let group_id = conn.last_insert_rowid();
            for (ordinal, file) in group.files.iter().enumerate() {
                insert_member.execute(params![group_id, file.id, ordinal as i64])?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct MaterializedPhotoGroup {
    files: Vec<FileRecord>,
    min_similarity: f64,
    avg_similarity: f64,
}

fn nearest_photo_threshold(min_similarity: f64) -> i64 {
    let pct = if min_similarity <= 1.0 {
        (min_similarity * 100.0).round() as i64
    } else {
        min_similarity.round() as i64
    };

    match pct {
        x if x >= 95 => 95,
        x if x >= 90 => 90,
        _ => 85,
    }
}

fn build_photo_groups_for_threshold(
    files: &[FileRecord],
    threshold: f64,
) -> Result<Vec<MaterializedPhotoGroup>> {
    let n = files.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0usize; n];

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }

    fn union(parent: &mut [usize], rank: &mut [usize], x: usize, y: usize) {
        let rx = find(parent, x);
        let ry = find(parent, y);
        if rx != ry {
            if rank[rx] < rank[ry] {
                parent[rx] = ry;
            } else if rank[rx] > rank[ry] {
                parent[ry] = rx;
            } else {
                parent[ry] = rx;
                rank[rx] += 1;
            }
        }
    }

    for i in 0..n {
        if let Some(phash_a) = files[i].phash {
            for j in (i + 1)..n {
                if let Some(phash_b) = files[j].phash {
                    let sim = crate::phasher::similarity_pct(phash_a, phash_b);
                    if sim >= threshold {
                        union(&mut parent, &mut rank, i, j);
                    }
                }
            }
        }
    }

    let mut groups_map: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups_map.entry(root).or_default().push(i);
    }

    let mut groups: Vec<MaterializedPhotoGroup> = groups_map
        .into_values()
        .filter(|indices| indices.len() > 1)
        .map(|indices| {
            let mut group_files: Vec<FileRecord> = indices.into_iter().map(|i| files[i].clone()).collect();
            group_files.sort_by_key(|f| f.path.clone());

            let m = group_files.len();
            let mut min_sim = 100.0f64;
            let mut total_sim = 0.0f64;
            let mut count = 0i64;

            for i in 0..m {
                if let Some(hash_a) = group_files[i].phash {
                    for j in (i + 1)..m {
                        if let Some(hash_b) = group_files[j].phash {
                            let sim = crate::phasher::similarity_pct(hash_a, hash_b);
                            if sim < min_sim {
                                min_sim = sim;
                            }
                            total_sim += sim;
                            count += 1;
                        }
                    }
                }
            }

            MaterializedPhotoGroup {
                files: group_files,
                min_similarity: min_sim,
                avg_similarity: if count > 0 { total_sim / count as f64 } else { 0.0 },
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.files
            .len()
            .cmp(&a.files.len())
            .then_with(|| b.avg_similarity.partial_cmp(&a.avg_similarity).unwrap_or(std::cmp::Ordering::Equal))
    });

    Ok(groups)
}
