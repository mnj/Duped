<script>
  import { invoke } from "@tauri-apps/api/core";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { formatBytes, trashFile, formatNumber } from "../../stores.js";

  let { scanId } = $props();

  let photoPairs = $state([]);
  let minSimilarity = $state(80);
  let currentIndex = $state(0);
  let loading = $state(true);

  async function loadPhotos() {
    loading = true;
    try {
      photoPairs = await invoke("get_photo_pairs", { minSimilarity: minSimilarity / 100.0 });
    } catch (err) {
      console.error("Failed to load photos:", err);
      photoPairs = [];
    }
    loading = false;
  }

  async function reloadWithThreshold() {
    currentIndex = 0;
    await loadPhotos();
  }

  $effect(() => {
    loadPhotos();
  });

  let currentPair = $derived(photoPairs[currentIndex] || null);

  function prev() {
    if (currentIndex > 0) currentIndex--;
  }

  function next() {
    if (currentIndex < photoPairs.length - 1) currentIndex++;
  }

  function fileName(path) {
    return path.split("/").pop();
  }

  function fileDir(path) {
    return path.substring(0, path.lastIndexOf("/"));
  }

  async function handleTrash(path) {
    if (confirm(`Move "${fileName(path)}" to trash?`)) {
      await trashFile(path);
      photoPairs = photoPairs.filter(p => p.file_a.path !== path && p.file_b.path !== path);
      if (currentIndex >= photoPairs.length) {
        currentIndex = Math.max(0, photoPairs.length - 1);
      }
    }
  }
</script>

<div class="photo-view">
  <div class="photo-toolbar">
    <div class="photo-filter">
      <label for="sim-slider">Similarity: {minSimilarity}%</label>
      <input
        id="sim-slider"
        type="range"
        min="50"
        max="100"
        bind:value={minSimilarity}
        onchange={reloadWithThreshold}
      />
    </div>

    {#if photoPairs.length > 0}
      <div class="photo-nav">
        <button onclick={prev} disabled={currentIndex === 0} aria-label="Previous pair">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
        <span class="photo-counter">{currentIndex + 1} / {photoPairs.length}</span>
        <button onclick={next} disabled={currentIndex >= photoPairs.length - 1} aria-label="Next pair">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="9 18 15 12 9 6"/>
          </svg>
        </button>
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="empty">
      <div class="spinner"></div>
      <p>Loading photo pairs...</p>
    </div>
  {:else if currentPair}
    <div class="photo-comparison">
      <div class="photo-header">
        <span class="photo-sim">
          {#if currentPair.similarity >= 95}
            <span class="badge green">Exact match</span>
          {:else if currentPair.similarity >= 80}
            <span class="badge amber">Near match</span>
          {:else}
            <span class="badge red">Loose match</span>
          {/if}
          {currentPair.similarity.toFixed(1)}% similar
        </span>
      </div>

      <div class="photo-panels">
        {#each [currentPair.file_a, currentPair.file_b] as file, i}
          <div class="photo-panel">
            <div class="photo-label">File {i + 1}</div>
            <div class="photo-img-wrapper">
              <img
                src={convertFileSrc(file.path)}
                alt={fileName(file.path)}
                class="photo-img"
                onerror={(e) => { e.target.style.display = 'none'; }}
              />
              <div class="photo-placeholder">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                  <circle cx="8.5" cy="8.5" r="1.5"/>
                  <polyline points="21 15 16 10 5 21"/>
                </svg>
                <span>{fileName(file.path)}</span>
              </div>
            </div>
            <div class="photo-meta">
              <span class="photo-name">{fileName(file.path)}</span>
              <span class="photo-dir">{fileDir(file.path)}</span>
              <span class="photo-size">{formatBytes(file.size)}</span>
            </div>
            <button class="photo-trash" onclick={() => handleTrash(file.path)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="3 6 5 6 21 6"/>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
              Move to Trash
            </button>
          </div>
        {/each}
      </div>
    </div>
  {:else}
    <div class="empty">
      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
        <circle cx="8.5" cy="8.5" r="1.5"/>
        <polyline points="21 15 16 10 5 21"/>
      </svg>
      <p>No photo pairs found with {minSimilarity}% similarity</p>
      <p class="hint">Try lowering the similarity threshold</p>
    </div>
  {/if}
</div>
