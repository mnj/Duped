<script>
  import { appState, subscribe, setView, setActiveView, loadMoreDuplicates, formatBytes, formatNumber } from "../stores.js";
  import CardGrid from "./views/CardGrid.svelte";
  import TableView from "./views/TableView.svelte";
  import SplitPane from "./views/SplitPane.svelte";
  import CompactList from "./views/CompactList.svelte";
  import SideBySide from "./views/SideBySide.svelte";
  import Photo from "./views/Photo.svelte";
  import { onMount } from "svelte";

  let state = $state(appState);
  let loadMoreTrigger = $state();
  let observer;
  let viewContent = $state();

  subscribe((s) => {
    state = { ...s };
  });

  const views = [
    { id: "grid", label: "Card Grid", icon: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z" },
    { id: "table", label: "Table", icon: "M3 3h18v18H3zM3 9h18M3 15h18M9 3v18M15 3v18" },
    { id: "split", label: "Split Pane", icon: "M3 3h18v18H3zM12 3v18" },
    { id: "compact", label: "Compact List", icon: "M3 6h18M3 12h18M3 18h18" },
    { id: "sidebyside", label: "Side-by-Side", icon: "M3 3h18v18H3zM12 3v18" },
    { id: "photo", label: "Photo", icon: "M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" },
  ];

  function switchView(id) {
    setActiveView(id);
  }

  function goHome() {
    setView("home");
  }

  async function maybeLoadMore() {
    if (state.activeView === "photo") {
      return;
    }
    await loadMoreDuplicates();
  }

  onMount(() => {
    observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        maybeLoadMore();
      }
    }, {
      root: viewContent,
      rootMargin: "600px 0px",
    });

    if (loadMoreTrigger) {
      observer.observe(loadMoreTrigger);
    }

    return () => observer?.disconnect();
  });

  $effect(() => {
    if (observer && loadMoreTrigger) {
      observer.disconnect();
      observer.observe(loadMoreTrigger);
    }
  });
</script>

<div class="results">
  <header class="results-header">
    <button class="back" onclick={goHome} aria-label="Back to home">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <line x1="19" y1="12" x2="5" y2="12"/>
        <polyline points="12 19 5 12 12 5"/>
      </svg>
    </button>
    <h1>Duped</h1>
    <div class="view-tabs">
      {#each views as v}
        <button
          class="view-tab"
          class:active={state.activeView === v.id}
          onclick={() => switchView(v.id)}
          title={v.label}
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d={v.icon}/>
          </svg>
          <span>{v.label}</span>
        </button>
      {/each}
    </div>
  </header>

  {#if state.stats}
    <div class="stats-bar">
      <div class="stat-item">
        <strong>{formatNumber(state.stats.file_count)}</strong>
        <span>Files</span>
      </div>
      <div class="stat-item">
        <strong>{formatBytes(state.stats.total_size)}</strong>
        <span>Total Size</span>
      </div>
      <div class="stat-item accent">
        <strong>{formatNumber(state.totalGroups)}</strong>
        <span>Duplicate Groups</span>
      </div>
      <div class="stat-item">
        <strong>{formatNumber(state.duplicatesLoaded)}</strong>
        <span>Loaded</span>
      </div>
      <div class="stat-item accent">
        <strong>{formatNumber(state.stats.duplicate_files)}</strong>
        <span>Duplicate Files</span>
      </div>
      <div class="stat-item warn">
        <strong>{formatBytes(state.stats.wasted_space)}</strong>
        <span>Wasted Space</span>
      </div>
    </div>
  {/if}

  <div class="view-content" bind:this={viewContent}>
    {#if state.activeView === "grid"}
      <CardGrid groups={state.duplicates} />
    {:else if state.activeView === "table"}
      <TableView groups={state.duplicates} />
    {:else if state.activeView === "split"}
      <SplitPane groups={state.duplicates} />
    {:else if state.activeView === "compact"}
      <CompactList groups={state.duplicates} />
    {:else if state.activeView === "sidebyside"}
      <SideBySide groups={state.duplicates} />
    {:else if state.activeView === "photo"}
      <Photo />
    {/if}

    {#if state.activeView !== "photo"}
      <div class="results-pagination">
        {#if state.loadingMoreDuplicates}
          <div class="results-loading">Loading more groups...</div>
        {:else if state.hasMoreDuplicates}
          <div class="results-loading">Scroll to load more groups</div>
        {/if}
        <div bind:this={loadMoreTrigger} class="results-sentinel" aria-hidden="true"></div>
      </div>
    {/if}
  </div>
</div>
