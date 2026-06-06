<script>
  import { formatBytes, formatNumber, trashFile, replaceWithSymlink } from "../../stores.js";

  let { groups = [] } = $props();

  let expanded = $state(new Set());

  function toggle(hash) {
    if (expanded.has(hash)) {
      expanded.delete(hash);
    } else {
      expanded.add(hash);
    }
    expanded = new Set(expanded);
  }

  function fileName(path) {
    return path.split("/").pop();
  }

  function fileDir(path) {
    return path.substring(0, path.lastIndexOf("/"));
  }

  async function handleTrash(e, path) {
    e.stopPropagation();
    if (confirm(`Move "${fileName(path)}" to trash?`)) {
      await trashFile(path);
    }
  }

  function symlinkTarget(group, path) {
    return group.files.find((file) => file.path !== path)?.path ?? null;
  }

  async function handleSymlink(e, group, path) {
    e.stopPropagation();
    const targetPath = symlinkTarget(group, path);
    if (!targetPath) return;
    if (confirm(`Replace "${fileName(path)}" with a symlink to "${fileName(targetPath)}"?`)) {
      await replaceWithSymlink(path, targetPath);
    }
  }
</script>

<div class="card-grid">
  {#if groups.length === 0}
    <div class="empty">
      <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
        <polyline points="22 4 12 14.01 9 11.01"/>
      </svg>
      <p>No duplicates found</p>
    </div>
  {:else}
    {#each groups as group}
      <div class="group-card">
        <button class="group-header" onclick={() => toggle(group.hash)}>
          <div class="group-info">
            <svg
              width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
              class="chevron" class:rotated={expanded.has(group.hash)}
            >
              <polyline points="9 18 15 12 9 6"/>
            </svg>
            <span class="hash">{group.hash.slice(0, 12)}...</span>
            <span class="badge">{group.files.length} files</span>
            <span class="size">{formatBytes(group.size)} each</span>
            <span class="wasted">{formatBytes(group.size * (group.files.length - 1))} wasted</span>
          </div>
        </button>

        {#if expanded.has(group.hash)}
          <div class="file-list">
            {#each group.files as file}
              <div class="file-card">
                <div class="file-icon">
                  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                    <polyline points="14 2 14 8 20 8"/>
                  </svg>
                </div>
                <div class="file-info">
                  <span class="file-name">{fileName(file.path)}</span>
                  <span class="file-dir">{fileDir(file.path)}</span>
                </div>
                <button class="trash-btn" onclick={(e) => handleTrash(e, file.path)} aria-label="Move to trash">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="3 6 5 6 21 6"/>
                    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
                  </svg>
                </button>
                <button class="trash-btn" onclick={(e) => handleSymlink(e, group, file.path)} aria-label="Replace with symlink">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M10 13a5 5 0 0 0 7.54.54l1.92-1.92a5 5 0 0 0-7.07-7.07L11.3 5.63"/>
                    <path d="M14 11a5 5 0 0 0-7.54-.54L4.54 12.38a5 5 0 1 0 7.07 7.07l1.88-1.88"/>
                  </svg>
                </button>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>
