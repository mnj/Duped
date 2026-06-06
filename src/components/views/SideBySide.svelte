<script>
  import { formatBytes, trashFile } from "../../stores.js";

  let { groups = [] } = $props();

  let currentIndex = $state(0);
  let currentGroup = $derived(groups[currentIndex] || null);

  function prev() {
    if (currentIndex > 0) currentIndex--;
  }

  function next() {
    if (currentIndex < groups.length - 1) currentIndex++;
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
    }
  }
</script>

<div class="side-by-side">
  {#if groups.length === 0}
    <div class="empty">
      <p>No duplicates found</p>
    </div>
  {:else if currentGroup}
    <div class="sbs-header">
      <div class="sbs-nav">
        <button onclick={prev} disabled={currentIndex === 0} aria-label="Previous group">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="15 18 9 12 15 6"/>
          </svg>
        </button>
        <span class="sbs-counter">{currentIndex + 1} / {groups.length}</span>
        <button onclick={next} disabled={currentIndex >= groups.length - 1} aria-label="Next group">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="9 18 15 12 9 6"/>
          </svg>
        </button>
      </div>
      <div class="sbs-info">
        <code class="sbs-hash">{currentGroup.hash}</code>
        <span class="sbs-size">{formatBytes(currentGroup.size)} each</span>
      </div>
    </div>

    <div class="sbs-comparison">
      {#each currentGroup.files.slice(0, 2) as file, i}
        <div class="sbs-panel">
          <div class="sbs-panel-header">
            <span class="sbs-panel-label">File {i + 1}</span>
          </div>
          <div class="sbs-file-icon">
            <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1">
              <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
              <polyline points="14 2 14 8 20 8"/>
            </svg>
          </div>
          <div class="sbs-file-info">
            <span class="sbs-file-name">{fileName(file.path)}</span>
            <span class="sbs-file-dir">{fileDir(file.path)}</span>
            <span class="sbs-file-size">{formatBytes(file.size)}</span>
          </div>
          <div class="sbs-actions">
            <button class="sbs-keep" onclick={() => handleTrash(currentGroup.files[i === 0 ? 1 : 0].path)}>
              Keep This
            </button>
            <button class="sbs-trash" onclick={() => handleTrash(file.path)}>
              Trash This
            </button>
          </div>
        </div>
      {/each}
    </div>

    {#if currentGroup.files.length > 2}
      <div class="sbs-more">
        <span>+{currentGroup.files.length - 2} more files in this group</span>
      </div>
    {/if}
  {/if}
</div>
