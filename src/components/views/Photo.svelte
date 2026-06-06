<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { formatBytes, trashFile, replaceWithSymlink } from "../../stores.js";

  const similarityOptions = [95, 90, 85];
  let minSimilarity = $state(90);
  let currentIndex = $state(0);
  let totalGroups = $state(0);
  let currentGroup = $state(null);
  let loading = $state(true);
  let loadedImages = $state({});
  let imageUrls = $state({});
  let fileMetadata = $state({});
  let imageLoadGeneration = 0;

  function revokeImageUrls() {
    for (const url of Object.values(imageUrls)) {
      if (url) {
        URL.revokeObjectURL(url);
      }
    }
    imageUrls = {};
  }

  function resetLoadedImages() {
    revokeImageUrls();
    loadedImages = {};
    fileMetadata = {};
  }

  async function loadImagesForGroup(group) {
    const generation = ++imageLoadGeneration;
    resetLoadedImages();

    if (!group) {
      return;
    }

    await Promise.all(group.files.map(async (file) => {
      try {
        const preview = await invoke("load_image_preview", { path: file.path });
        const metadata = await invoke("load_file_metadata", { path: file.path });
        if (generation !== imageLoadGeneration) {
          return;
        }
        const bytes = new Uint8Array(preview.bytes);
        const blob = new Blob([bytes], { type: preview.mime_type });
        const url = URL.createObjectURL(blob);
        imageUrls = { ...imageUrls, [file.path]: url };
        fileMetadata = { ...fileMetadata, [file.path]: metadata };
      } catch (err) {
        console.error("Failed to load image preview:", err);
        if (generation === imageLoadGeneration) {
          loadedImages = { ...loadedImages, [file.path]: false };
        }
      }
    }));
  }

  async function loadPhotos() {
    loading = true;
    try {
      const page = await invoke("get_photo_groups_page", {
        minSimilarity: minSimilarity / 100.0,
        offset: 0,
        limit: 1,
      });
      totalGroups = page.total;
      currentGroup = page.groups[0] || null;
      currentIndex = 0;
      await loadImagesForGroup(currentGroup);
    } catch (err) {
      console.error("Failed to load photos:", err);
      totalGroups = 0;
      currentGroup = null;
      resetLoadedImages();
    }
    loading = false;
  }

  async function reloadWithThreshold() {
    await loadPhotos();
  }

  onMount(() => {
    loadPhotos();
  });

  async function loadCurrentGroup() {
    loading = true;
    try {
      currentGroup = await invoke("get_photo_group", {
        minSimilarity: minSimilarity / 100.0,
        index: currentIndex,
      });
      await loadImagesForGroup(currentGroup);
    } catch (err) {
      console.error("Failed to load current photo group:", err);
      currentGroup = null;
      resetLoadedImages();
    }
    loading = false;
  }

  async function prev() {
    if (currentIndex > 0) {
      currentIndex--;
      await loadCurrentGroup();
    }
  }

  async function next() {
    if (currentIndex < totalGroups - 1) {
      currentIndex++;
      await loadCurrentGroup();
    }
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
    loadedImages = { ...loadedImages, [path]: false };
  }

  function formatDuration(seconds) {
    if (!seconds || Number.isNaN(seconds)) return null;
    if (seconds < 60) return `${seconds.toFixed(1)}s`;
    const mins = Math.floor(seconds / 60);
    const secs = Math.round(seconds % 60);
    return `${mins}m ${secs}s`;
  }

  function symlinkTarget(path) {
    return currentGroup?.files.find((file) => file.path !== path)?.path ?? null;
  }

  async function handleSymlink(path, targetPath) {
    if (!targetPath) return;
    if (confirm(`Replace "${fileName(path)}" with a symlink to "${fileName(targetPath)}"?`)) {
      await replaceWithSymlink(path, targetPath);
    }
  }

  async function handleTrash(path) {
    if (confirm(`Move "${fileName(path)}" to trash?`)) {
      await trashFile(path);
      if (currentGroup) {
        currentGroup = {
          ...currentGroup,
          files: currentGroup.files.filter((f) => f.path !== path),
        };
        await loadImagesForGroup(currentGroup);
      }
      if (currentGroup && currentGroup.files.length <= 1) {
        totalGroups = Math.max(0, totalGroups - 1);
        if (currentIndex >= totalGroups && currentIndex > 0) {
          currentIndex--;
        }
        await loadCurrentGroup();
      }
    }
  }
</script>

<div class="photo-view">
  <div class="photo-toolbar">
    <div class="photo-filter">
      <label for="sim-select">Similarity: {minSimilarity}%</label>
      <select id="sim-select" bind:value={minSimilarity} onchange={reloadWithThreshold}>
        {#each similarityOptions as option}
          <option value={option}>{option}%</option>
        {/each}
      </select>
    </div>

    {#if totalGroups > 0}
      <div class="photo-nav">
        <button onclick={prev} disabled={currentIndex === 0} aria-label="Previous group">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
        <span class="photo-counter">{currentIndex + 1} / {totalGroups}</span>
        <button onclick={next} disabled={currentIndex >= totalGroups - 1} aria-label="Next group">
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
            <span class="badge green">Visual match</span>
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
                src={imageUrls[file.path]}
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
              {#if fileMetadata[file.path]?.width && fileMetadata[file.path]?.height}
                <span class="photo-size">{fileMetadata[file.path].width}x{fileMetadata[file.path].height}</span>
              {/if}
              {#if fileMetadata[file.path]?.duration_seconds}
                <span class="photo-size">{formatDuration(fileMetadata[file.path].duration_seconds)}</span>
              {/if}
              {#if fileMetadata[file.path]?.codec}
                <span class="photo-size">{fileMetadata[file.path].codec}</span>
              {/if}
            </div>
            {#if fileMetadata[file.path]?.ffprobe_streams_json}
              <pre class="photo-ffprobe">{fileMetadata[file.path].ffprobe_streams_json}</pre>
            {/if}
            <button class="photo-trash" onclick={() => handleTrash(file.path)}>
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <polyline points="3 6 5 6 21 6"/>
                <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
              </svg>
              Move to Trash
            </button>
            {#if currentGroup.files.length > 1}
              <button class="photo-trash" onclick={() => handleSymlink(file.path, symlinkTarget(file.path))}>
                Replace With Symlink
              </button>
            {/if}
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
