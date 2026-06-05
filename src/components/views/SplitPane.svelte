<script>
  import { formatBytes } from "../../stores.js";
  import { appState, subscribe } from "../../stores.js";

  let { groups = [] } = $props();

  let state = $state(appState);
  subscribe((s) => { state = { ...s }; });

  let selectedIndex = $state(0);
  let selectedGroup = $derived(groups[selectedIndex] || null);

  function selectGroup(idx) {
    selectedIndex = idx;
  }

  function fileName(path) {
    return path.split("/").pop();
  }

  function fileDir(path) {
    return path.substring(0, path.lastIndexOf("/"));
  }

  function prev() {
    if (selectedIndex > 0) selectedIndex--;
  }

  function next() {
    if (selectedIndex < groups.length - 1) selectedIndex++;
  }
</script>

<div class="split-pane">
  {#if groups.length === 0}
    <div class="empty">
      <p>No duplicates found</p>
    </div>
  {:else}
    <div class="left-panel">
      <div class="panel-header">
        <span>{groups.length} groups</span>
        <div class="nav-buttons">
          <button onclick={prev} disabled={selectedIndex === 0} aria-label="Previous group">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="15 18 9 12 15 6"/>
            </svg>
          </button>
          <button onclick={next} disabled={selectedIndex >= groups.length - 1} aria-label="Next group">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="9 18 15 12 9 6"/>
            </svg>
          </button>
        </div>
      </div>
      <div class="group-list">
        {#each groups as group, i}
          <button
            class="group-item"
            class:selected={i === selectedIndex}
            onclick={() => selectGroup(i)}
          >
            <code class="hash">{group.hash.slice(0, 10)}</code>
            <span class="count">{group.files.length} files</span>
            <span class="size">{formatBytes(group.size)}</span>
          </button>
        {/each}
      </div>
    </div>

    <div class="divider"></div>

    <div class="right-panel">
      {#if selectedGroup}
        <div class="detail-header">
          <div class="detail-info">
            <code class="hash-full">{selectedGroup.hash}</code>
            <div class="detail-meta">
              <span>{selectedGroup.files.length} files</span>
              <span>{formatBytes(selectedGroup.size)} each</span>
              <span class="wasted">{formatBytes(selectedGroup.size * (selectedGroup.files.length - 1))} wasted</span>
            </div>
          </div>
        </div>

        <div class="file-strip">
          {#each selectedGroup.files as file, i}
            <div class="file-detail">
              <div class="file-detail-icon">
                <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                  <polyline points="14 2 14 8 20 8"/>
                </svg>
              </div>
              <div class="file-detail-info">
                <span class="file-name">{fileName(file.path)}</span>
                <span class="file-dir">{fileDir(file.path)}</span>
                <span class="file-size">{formatBytes(file.size)}</span>
              </div>
              <div class="file-actions">
                <button class="keep" title="Keep this file">Keep</button>
                <button class="delete" title="Delete this file">Delete</button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>
