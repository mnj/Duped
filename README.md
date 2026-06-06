# Duped

A fast duplicate file finder built with Tauri, Rust, and Svelte.

## Features

- **Fast scanning** using parallel blake3 hashing
- **Size-based pre-filtering** to skip unique files
- **Multiple view modes**: Card Grid, Table, and Split Pane
- **Scan management**: Abort scans, view recent scans, dismiss scans from list
- **Performance optimized**: Batch inserts (50K files), deferred index creation
- **Temporary storage option**: On Linux, use `/tmp` by default during scanning to reduce disk writes

## Building

```bash
# Install dependencies
bun install

# Run in development mode
bun tauri dev

# Build desktop bundles with the project's linuxdeploy workaround
bun run tauri:build

# Build only the AppImage
bun run tauri:build:appimage

# Build only the release binary in a Debian Trixie container
bun run tauri:build:release:trixie
```

On Linux, use `bun run tauri:build` or `bun run tauri:build:appimage` instead of `bun tauri build`.
These scripts set `NO_STRIP=1`, which avoids a `linuxdeploy` failure when stripping newer Arch/CachyOS system libraries during AppImage packaging.
The Debian Trixie container build uses Podman, mounts the repo read-only into the container, builds from a temporary in-container working copy, and stops at `tauri build --no-bundle`, so it compiles the optimized release app without producing platform bundles.
The compiled binary is copied to `output/trixie-release/` on the host after the container build completes.

## Usage

### Normal Mode

```bash
bun tauri dev
```

On Linux, scans use `/tmp` by default while building results, then move the finished database into the configured storage directory.
If no storage directory is configured, the app falls back to the platform app-data directory.

### Temporary Storage Mode

You can control temporary storage mode using command-line flags or environment variables:

```bash
# Disable /tmp usage, even on Linux where it is the default
./src-tauri/target/release/duped --no-tmp-db

# Store completed databases in a specific directory
./src-tauri/target/release/duped --db-dir /path/to/duped-data

# Force-enable /tmp usage on platforms where it is not the default
DUPED_TMP_DB=1 bun tauri dev
DUPED_TMP_DB=1 ./src-tauri/target/release/duped

# Force-disable /tmp usage
DUPED_NO_TMP_DB=1 bun tauri dev
DUPED_NO_TMP_DB=1 ./src-tauri/target/release/duped

# Configure the storage directory via environment
DUPED_DB_DIR=/path/to/duped-data bun tauri dev
DUPED_DB_DIR=/path/to/duped-data ./src-tauri/target/release/duped
```

When temporary storage mode is enabled:
- The SQLite database is created in `/tmp` during scanning
- This reduces disk writes on your main storage
- After scanning completes, the database is automatically moved to the configured storage directory
- The UI will show a notification that temporary storage mode is active

This is particularly useful when:
- Scanning large directories with millions of files
- You want to minimize wear on SSDs
- Your main storage is slow or network-mounted

Linux note:
- This project now defaults to `/tmp` on Linux because it is commonly backed by `tmpfs`
- macOS and other platforms do not assume the same memory-backed behavior by default
- If no storage directory is configured, it falls back to the platform app-data directory

## Architecture

### Backend (Rust)

- **db.rs**: SQLite storage with WAL mode, batch inserts, duplicate grouping queries
- **scanner.rs**: Parallel file walking (via `ignore` crate) + blake3 hashing with rayon
- **commands.rs**: Tauri IPC commands with event streaming for progress updates

### Frontend (Svelte)

- **stores.js**: Reactive state management with Tauri event listeners
- **components/Home.svelte**: New scan / open existing DB / recent scans list
- **components/ScanProgress.svelte**: Real-time progress with abort button
- **components/Results.svelte**: Stats bar + view tabs
- **components/views/**: Three view modes (CardGrid, TableView, SplitPane)

## Performance Optimizations

1. **Size pre-grouping**: Only hashes files with duplicate sizes (massive speedup)
2. **Parallel hashing**: Uses rayon across all CPU cores
3. **Batch SQLite inserts**: 50K files per transaction
4. **WAL mode**: Concurrent reads during scan
5. **Deferred indexes**: Created after scan completes, not during inserts
6. **Pagination**: Loads 100 duplicate groups at a time to prevent memory issues

## Database Schema

```sql
CREATE TABLE scans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL DEFAULT 'running'
);

CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    scan_id INTEGER NOT NULL,
    path TEXT NOT NULL UNIQUE,
    hash TEXT,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    FOREIGN KEY (scan_id) REFERENCES scans(id)
);

-- Indexes created after scan completes
CREATE INDEX idx_files_scan_size ON files(scan_id, size);
CREATE INDEX idx_files_scan_hash ON files(scan_id, hash);
CREATE INDEX idx_files_path ON files(path);
```

## License

MIT
