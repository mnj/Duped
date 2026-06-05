<script>
  import { appState, subscribe, abortScan, formatBytes, formatNumber } from "../stores.js";

  let state = $state(appState);

  subscribe((s) => {
    state = { ...s };
  });

  let progress = $derived(state.progress);

  let hashPercent = $derived(
    progress && progress.files_to_hash > 0
      ? Math.round((progress.files_hashed / progress.files_to_hash) * 100)
      : 0
  );

  let skippedFiles = $derived(
    progress && progress.files_to_hash > 0
      ? progress.files_walked - progress.files_to_hash
      : 0
  );
</script>

<div class="scan-progress">
  <div class="scan-header">
    <div class="spinner"></div>
    <h1>
      {#if progress?.phase === "walking"}
        Scanning files...
      {:else}
        Hashing files...
      {/if}
    </h1>
  </div>

  {#if state.dbPath?.includes('/tmp/')}
    <div class="info-note tmp-mode">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 2v20M2 12h20"/>
      </svg>
      <span>
        Using temporary storage in /tmp to reduce disk writes. Database will be moved to app data after scanning completes.
      </span>
    </div>
  {/if}

  <div class="stats-grid">
    <div class="stat">
      <span class="value">{progress ? formatNumber(progress.files_walked) : "0"}</span>
      <span class="label">Files Found</span>
    </div>
    {#if progress?.phase === "hashing"}
      <div class="stat">
        <span class="value">{formatNumber(progress.files_hashed)} / {formatNumber(progress.files_to_hash)}</span>
        <span class="label">Files Hashed</span>
      </div>
      <div class="stat">
        <span class="value">{formatBytes(progress.bytes_hashed)}</span>
        <span class="label">Data Processed</span>
      </div>
    {/if}
  </div>

  {#if progress?.phase === "hashing" && skippedFiles > 0}
    <div class="info-note">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="10"/>
        <line x1="12" y1="16" x2="12" y2="12"/>
        <line x1="12" y1="8" x2="12.01" y2="8"/>
      </svg>
      <span>
        {formatNumber(skippedFiles)} files skipped — only files with matching sizes are hashed for duplicates
      </span>
    </div>
  {/if}

  {#if progress?.phase === "hashing"}
    <div class="progress-bar">
      <div class="progress-fill" style="width: {hashPercent}%"></div>
    </div>
    <span class="progress-text">{hashPercent}%</span>
  {/if}

  <button class="abort" onclick={abortScan}>
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
    </svg>
    Abort Scan
  </button>
</div>
