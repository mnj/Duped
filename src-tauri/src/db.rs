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
                FOREIGN KEY (scan_id) REFERENCES scans(id)
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
            ",
        )?;
        Ok(())
    }

    pub fn create_scan(&self, path: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO scans (path, started_at, status) VALUES (?1, ?2, 'running')",
            params![path, now],
        )?;
        Ok(conn.last_insert_rowid())
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
            "INSERT OR IGNORE INTO files (scan_id, path, hash, size, modified) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut count = 0;
        for f in files {
            count += stmt.execute(params![scan_id, f.path, f.hash, f.size, f.modified])?;
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
            "SELECT hash, size FROM files
             WHERE scan_id = ?1 AND hash IS NOT NULL
             GROUP BY hash HAVING COUNT(*) > 1
             ORDER BY size DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let groups: Vec<(String, i64)> = stmt
            .query_map(params![scan_id, limit, offset], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        let mut result = Vec::new();
        for (hash, size) in groups {
            let mut file_stmt = conn.prepare(
                "SELECT id, path, hash, size, modified FROM files
                 WHERE scan_id = ?1 AND hash = ?2
                 ORDER BY path",
            )?;

            let files: Vec<FileRecord> = file_stmt
                .query_map(params![scan_id, hash], |row| {
                    Ok(FileRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        hash: row.get(2)?,
                        size: row.get(3)?,
                        modified: row.get(4)?,
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
            "SELECT COUNT(*) FROM (
                SELECT hash FROM files
                WHERE scan_id = ?1 AND hash IS NOT NULL
                GROUP BY hash HAVING COUNT(*) > 1
            )",
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

    pub fn get_files_by_size_groups(&self, scan_id: i64) -> Result<Vec<Vec<FileRecord>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, hash, size, modified FROM files
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
}
