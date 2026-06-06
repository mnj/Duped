<script>
  import { formatBytes, trashFile } from "../../stores.js";

  let { groups = [] } = $props();

  let expanded = $state(new Set());
  let searchQuery = $state("");
  let sortBy = $state("size");
  let sortDir = $state("desc");

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

  function sort(key) {
    if (sortBy === key) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortBy = key;
      sortDir = "desc";
    }
  }

  let filteredGroups = $derived(
    groups.filter(group => {
      if (!searchQuery) return true;
      const query = searchQuery.toLowerCase();
      return group.files.some(f => f.path.toLowerCase().includes(query));
    })
  );

  let sortedGroups = $derived(
    [...filteredGroups].sort((a, b) => {
      let va, vb;
      if (sortBy === "size") { va = a.size; vb = b.size; }
      else if (sortBy === "count") { va = a.files.length; vb = b.files.length; }
      else if (sortBy === "wasted") { va = a.size * (a.files.length - 1); vb = b.size * (b.files.length - 1); }
      return sortDir === "asc" ? va - vb : vb - va;
    })
  );

  function sortIcon(key) {
    if (sortBy !== key) return "";
    return sortDir === "asc" ? " ↑" : " ↓";
  }
</script>

<div class="compact-list">
  <div class="compact-toolbar">
    <input
      type="text"
      class="compact-search"
      placeholder="Search files..."
      bind:value={searchQuery}
    />
    <div class="compact-sort">
      <button class="sort-btn" onclick={() => sort("size")}>
        Size{sortIcon("size")}
      </button>
      <button class="sort-btn" onclick={() => sort("count")}>
        Count{sortIcon("count")}
      </button>
      <button class="sort-btn" onclick={() => sort("wasted")}>
        Wasted{sortIcon("wasted")}
      </button>
    </div>
  </div>

  <div class="compact-content">
    {#if sortedGroups.length === 0}
      <div class="empty">
        <p>No duplicates found</p>
      </div>
    {:else}
      {#each sortedGroups as group}
        <div class="compact-group">
          <button class="compact-header" onclick={() => toggle(group.hash)}>
            <svg
              width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
              class="chevron" class:rotated={expanded.has(group.hash)}
            >
              <polyline points="9 18 15 12 9 6"/>
            </svg>
            <span class="compact-hash">{group.hash.slice(0, 8)}</span>
            <span class="compact-count">{group.files.length} files</span>
            <span class="compact-size">{formatBytes(group.size)}</span>
            <span class="compact-wasted">{formatBytes(group.size * (group.files.length - 1))} wasted</span>
          </button>

          {#if expanded.has(group.hash)}
            <div class="compact-files">
              {#each group.files as file}
                <div class="compact-file">
                  <div class="compact-file-info">
                    <span class="compact-file-name">{fileName(file.path)}</span>
                    <span class="compact-file-dir">{fileDir(file.path)}</span>
                  </div>
                  <button class="compact-trash" onclick={(e) => handleTrash(e, file.path)} aria-label="Move to trash">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <polyline points="3 6 5 6 21 6"/>
                      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>
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
</div>
