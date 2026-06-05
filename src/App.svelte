<script>
  import { appState, subscribe, setView } from "./stores.js";
  import Home from "./components/Home.svelte";
  import ScanProgress from "./components/ScanProgress.svelte";
  import Results from "./components/Results.svelte";

  let state = $state(appState);

  subscribe((s) => {
    state = { ...s };
  });
</script>

<main>
  {#if state.view === "home"}
    <Home />
  {:else if state.view === "scanning"}
    <ScanProgress />
  {:else if state.view === "results"}
    <Results />
  {/if}
</main>
