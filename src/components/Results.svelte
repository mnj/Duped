<script>
  import { appState, subscribe, setView, loadDuplicates, formatBytes, formatNumber } from "../stores.js";
  import CardGrid from "./views/CardGrid.svelte";
  import TableView from "./views/TableView.svelte";
  import SplitPane from "./views/SplitPane.svelte";
  import CompactList from "./views/CompactList.svelte";
  import SideBySide from "./views/SideBySide.svelte";

  let state = $state(appState);

  subscribe((s) => {
    state = { ...s };
  });

  const views = [
    { id: "grid", label: "Card Grid", icon: "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z" },
    { id: "table", label: "Table", icon: "M3 3h18v18H3zM3 9h18M3 15h18M9 3v18M15 3v18" },
    { id: "split", label: "Split Pane", icon: "M3 3h18v18H3zM12 3v18" },
    { id: "compact", label: "Compact List", icon: "M3 6h18M3 12h18M3 18h18" },
    { id: "sidebyside", label: "Side-by-Side", icon: "M3 3h18v18H3zM12 3v18" },
  ];

  function switchView(id) {
    appState.activeView = id;
    appState.selectedGroup = null;
    subscribe((s) => { state = { ...s }; })();
  }

  function goHome() {
    setView("home");
  }
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

  <div class="view-content">
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
    {/if}
  </div>
</div>
