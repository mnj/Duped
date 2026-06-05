<script>
  import { formatBytes } from "../../stores.js";

  let { groups = [] } = $props();

  let sortKey = $state("size");
  let sortDir = $state("desc");

  function sort(key) {
    if (sortKey === key) {
      sortDir = sortDir === "asc" ? "desc" : "asc";
    } else {
      sortKey = key;
      sortDir = "desc";
    }
  }

  let sortedGroups = $derived(
    [...groups].sort((a, b) => {
      let va, vb;
      if (sortKey === "size") { va = a.size; vb = b.size; }
      else if (sortKey === "count") { va = a.files.length; vb = b.files.length; }
      else if (sortKey === "wasted") { va = a.size * (a.files.length - 1); vb = b.size * (b.files.length - 1); }
      return sortDir === "asc" ? va - vb : vb - va;
    })
  );

  function fileName(path) {
    return path.split("/").pop();
  }

  function fileDir(path) {
    return path.substring(0, path.lastIndexOf("/"));
  }

  function sortIcon(key) {
    if (sortKey !== key) return "";
    return sortDir === "asc" ? " \u2191" : " \u2193";
  }
</script>

<div class="table-view">
  {#if groups.length === 0}
    <div class="empty">
      <p>No duplicates found</p>
    </div>
  {:else}
    <table>
      <thead>
        <tr>
          <th>Hash</th>
          <th>Files</th>
          <th>
            <button class="sort-btn" onclick={() => sort("size")}>
              Size{sortIcon("size")}
            </button>
          </th>
          <th>
            <button class="sort-btn" onclick={() => sort("wasted")}>
              Wasted{sortIcon("wasted")}
            </button>
          </th>
          <th>Paths</th>
        </tr>
      </thead>
      <tbody>
        {#each sortedGroups as group}
          {#each group.files as file, i}
            <tr class:group-start={i === 0} class:group-stripe={i > 0}>
              {#if i === 0}
                <td class="hash" rowspan={group.files.length}>
                  <code>{group.hash.slice(0, 12)}</code>
                </td>
                <td class="count" rowspan={group.files.length}>
                  {group.files.length}
                </td>
                <td class="size" rowspan={group.files.length}>
                  {formatBytes(group.size)}
                </td>
                <td class="wasted" rowspan={group.files.length}>
                  {formatBytes(group.size * (group.files.length - 1))}
                </td>
              {/if}
              <td class="path-cell">
                <span class="file-name">{fileName(file.path)}</span>
                <span class="file-dir">{fileDir(file.path)}</span>
              </td>
            </tr>
          {/each}
        {/each}
      </tbody>
    </table>
  {/if}
</div>
