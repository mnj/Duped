<script>
  import { formatBytes, formatNumber } from "../../stores.js";

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
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>
