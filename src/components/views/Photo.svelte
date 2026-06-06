<script>
  import { invoke } from "@tauri-apps/api/core";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { formatBytes, trashFile } from "../../stores.js";

  let photoGroups = $state([]);
  let minSimilarity = $state(80);
  let currentIndex = $state(0);
  let loading = $state(true);
  let loadedImages = $state({});

  function resetLoadedImages() {
    loadedImages = {};
  }

  async function loadPhotos() {
    loading = true;
    resetLoadedImages();
    try {
      photoGroups = await invoke("get_photo_groups", { minSimilarity: minSimilarity / 100.0 });
    } catch (err) {
      console.error("Failed to load photos:", err);
      photoGroups = [];
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

  let currentGroup = $derived(photoGroups[currentIndex] || null);

  function prev() {
    if (currentIndex > 0) currentIndex--;
  }

  function next() {
    if (currentIndex < photoGroups.length - 1) currentIndex++;
  }

  function fileName(path) {
    return path.split("/").pop();
  }

  function fileDir(path) {
    return path.substring(0, path.lastIndexOf("/"));
  }

  function markLoaded(path) {
    loadedImages = { ...loadedImages, [path]: true };
  }

  function markFailed(path, event) {
    event.currentTarget.style.display = "none";
    loadedImages = { ...loadedImages, [path]: false };
  }

  async function handleTrash(path) {
    if (confirm(`Move "${fileName(path)}" to trash?`)) {
      await trashFile(path);
      photoGroups = photoGroups.map(group => ({
        ...group,
        files: group.files.filter(f => f.path !== path)
      })).filter(group => group.files.length > 1);
      if (currentIndex >= photoGroups.length) {
        currentIndex = Math.max(0, photoGroups.length - 1);
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

    {#if photoGroups.length > 0}
      <div class="photo-nav">
        <button onclick={prev} disabled={currentIndex === 0} aria-label="Previous group">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
        <span class="photo-counter">{currentIndex + 1} / {photoGroups.length}</span>
        <button onclick={next} disabled={currentIndex >= photoGroups.length - 1} aria-label="Next group">
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
      <p>Loading photo groups...</p>
    </div>
  {:else if currentGroup}
    <div class="photo-comparison">
      <div class="photo-header">
        <div class="photo-sim">
          <span class="photo-group-size">{currentGroup.files.length} similar images</span>
          {#if currentGroup.avg_similarity >= 95}
            <span class="badge green">Exact match</span>
          {:else if currentGroup.avg_similarity >= 80}
            <span class="badge amber">Near match</span>
          {:else}
            <span class="badge red">Loose match</span>
          {/if}
          <span class="sim-detail">
            Min: {currentGroup.min_similarity.toFixed(1)}%
            &middot;
            Avg: {currentGroup.avg_similarity.toFixed(1)}%
          </span>
        </div>
      </div>

      <div class="photo-panels">
        {#each currentGroup.files as file, i}
          <div class="photo-panel">
            <div class="photo-label">File {i + 1}</div>
            <div class="photo-img-wrapper">
              <img
                src={convertFileSrc(file.path)}
                alt={fileName(file.path)}
                class="photo-img"
                class:loaded={loadedImages[file.path] === true}
                onload={() => markLoaded(file.path)}
                onerror={(e) => markFailed(file.path, e)}
              />
              {#if loadedImages[file.path] !== true}
              <div class="photo-placeholder">
                <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                  <circle cx="8.5" cy="8.5" r="1.5"/>
                  <polyline points="21 15 16 10 5 21"/>
                </svg>
                <span>{fileName(file.path)}</span>
              </div>
              {/if}
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
      <p>No photo groups found with {minSimilarity}% similarity</p>
      <p class="hint">Try lowering the similarity threshold</p>
    </div>
  {/if}
</div>
