<script>
  import { startScan, openDatabase, selectFolder, selectFile, listScans, dismissScan, addPathToScan, mergeDatabases, formatBytes } from "../stores.js";

  let recentScans = $state([]);
  let loading = $state(false);

  $effect(() => {
    loadRecent();
  });

  async function loadRecent() {
    recentScans = await listScans();
  }

  async function handleNewScan() {
    try {
      const folder = await selectFolder();
      if (folder) {
        loading = true;
        await startScan(folder);
      }
    } catch (err) {
      console.error("Failed to start scan:", err);
      loading = false;
    }
  }

  async function handleOpenExisting() {
    try {
      const file = await selectFile();
      if (file) {
        loading = true;
        await openDatabase(file);
      }
    } catch (err) {
      console.error("Failed to open database:", err);
    } finally {
      loading = false;
    }
  }

  async function handleOpenRecent(path) {
    try {
      loading = true;
      await openDatabase(path);
    } catch (err) {
      console.error("Failed to open recent scan:", err);
    } finally {
      loading = false;
    }
  }

  async function handleDismissScan(e, path) {
    e.stopPropagation();
    try {
      await dismissScan(path);
      await loadRecent();
    } catch (err) {
      console.error("Failed to dismiss scan:", err);
    }
  }

  async function handleAddPath(e, dbPath) {
    e.stopPropagation();
    try {
      const folder = await selectFolder();
      if (folder) {
        loading = true;
        await addPathToScan(dbPath, folder);
      }
    } catch (err) {
      console.error("Failed to add path to scan:", err);
      loading = false;
    }
  }

  async function handleMerge(e, targetDbPath) {
    e.stopPropagation();
    try {
      const sourceFile = await selectFile();
      if (sourceFile) {
        loading = true;
        const result = await mergeDatabases(sourceFile);
        alert(`Merged ${result.scans_merged} scans and ${result.files_merged} files`);
        await loadRecent();
      }
    } catch (err) {
      console.error("Failed to merge databases:", err);
      alert("Failed to merge databases: " + err.message);
    } finally {
      loading = false;
    }
  }

  function scanName(path) {
    const match = path.match(/scan_(\d{8})_(\d{6})\.db/);
    if (match) {
      const [, date, time] = match;
      return `${date.slice(0,4)}-${date.slice(4,6)}-${date.slice(6,8)} ${time.slice(0,2)}:${time.slice(2,4)}:${time.slice(4,6)}`;
    }
    return path.split("/").pop();
  }
</script>

<div class="home">
  <div class="hero">
    <h1>Duped</h1>
    <p class="tagline">Find and manage duplicate files</p>
  </div>

  <div class="actions">
    <button class="primary" onclick={handleNewScan} disabled={loading}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        <line x1="12" y1="11" x2="12" y2="17"/>
        <line x1="9" y1="14" x2="15" y2="14"/>
      </svg>
      New Scan
    </button>
    <button class="secondary" onclick={handleOpenExisting} disabled={loading}>
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
        <polyline points="14 2 14 8 20 8"/>
        <line x1="16" y1="13" x2="8" y2="13"/>
        <line x1="16" y1="17" x2="8" y2="17"/>
      </svg>
      Open Database
    </button>
  </div>

  {#if recentScans.length > 0}
    <div class="recent">
      <h2>Recent Scans</h2>
      <div class="scan-list">
        {#each recentScans as scan}
          <div class="scan-item" role="button" tabindex="0" onclick={() => handleOpenRecent(scan)} onkeydown={(e) => e.key === 'Enter' && handleOpenRecent(scan)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <ellipse cx="12" cy="5" rx="9" ry="3"/>
              <path d="M21 12c0 1.66-4 3-9 3s-9-1.34-9-3"/>
              <path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"/>
            </svg>
            <span>{scanName(scan)}</span>
            <span class="path">{scan}</span>
            <button
              class="action-btn add-btn"
              onclick={(e) => handleAddPath(e, scan)}
              aria-label="Add path to scan"
              title="Add another path to this scan"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="12" y1="5" x2="12" y2="19"/>
                <line x1="5" y1="12" x2="19" y2="12"/>
              </svg>
            </button>
            <button
              class="action-btn merge-btn"
              onclick={(e) => handleMerge(e, scan)}
              aria-label="Merge databases"
              title="Merge another database into this one"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/>
                <line x1="12" y1="8" x2="12" y2="16"/>
                <line x1="8" y1="12" x2="16" y2="12"/>
              </svg>
            </button>
            <button
              class="action-btn delete-btn"
              onclick={(e) => handleDismissScan(e, scan)}
              aria-label="Dismiss scan"
              title="Dismiss this scan from the list"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <line x1="18" y1="6" x2="6" y2="18"/>
                <line x1="6" y1="6" x2="18" y2="18"/>
              </svg>
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
