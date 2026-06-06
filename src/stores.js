import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export const appState = {
  view: "home",
  scanning: false,
  progress: null,
  duplicates: [],
  duplicatesPageSize: 100,
  duplicatesLoaded: 0,
  loadingMoreDuplicates: false,
  hasMoreDuplicates: false,
  totalGroups: 0,
  stats: null,
  dbPath: null,
  activeView: "grid",
  selectedGroup: null,
};

export const listeners = new Set();
let scanProgressUnlisten = null;
let scanCompleteUnlisten = null;

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function notify() {
  listeners.forEach((fn) => fn(appState));
}

function clearScanListeners() {
  scanProgressUnlisten?.();
  scanCompleteUnlisten?.();
  scanProgressUnlisten = null;
  scanCompleteUnlisten = null;
}

function resetScanState() {
  appState.scanning = false;
  appState.progress = null;
}

function resetDuplicateState() {
  appState.duplicates = [];
  appState.duplicatesLoaded = 0;
  appState.loadingMoreDuplicates = false;
  appState.hasMoreDuplicates = false;
  appState.totalGroups = 0;
}

async function attachScanListeners({ finalizeTmpDb }) {
  clearScanListeners();

  scanProgressUnlisten = await listen("scan-progress", (event) => {
    appState.progress = event.payload;
    notify();
  });

  scanCompleteUnlisten = await listen("scan-complete", async (event) => {
    clearScanListeners();
    resetScanState();
    appState.stats = event.payload.stats;

    if (!event.payload.aborted) {
      if (finalizeTmpDb) {
        const newPath = await invoke("finalize_scan");
        if (newPath) {
          appState.dbPath = newPath;
        }
      }
      await loadDuplicates();
      appState.view = "results";
    } else {
      appState.view = "home";
    }

    notify();
  });
}

export function setView(view) {
  appState.view = view;
  notify();
}

export function setActiveView(view) {
  appState.activeView = view;
  appState.selectedGroup = null;
  notify();
}

export async function startScan(path) {
  clearScanListeners();
  appState.scanning = true;
  appState.view = "scanning";
  appState.progress = { phase: "walking", files_walked: 0, files_to_hash: 0, files_hashed: 0, bytes_hashed: 0 };
  notify();

  await attachScanListeners({ finalizeTmpDb: true });

  await invoke("start_scan", { path });
}

export async function abortScan() {
  await invoke("abort_scan");
}

export async function openDatabase(path) {
  clearScanListeners();
  resetScanState();
  resetDuplicateState();
  await invoke("open_database", { path });
  appState.dbPath = path;
  appState.stats = await invoke("get_stats");
  await loadDuplicates();
  appState.view = "results";
  notify();
}

export async function loadDuplicates() {
  const count = await invoke("get_duplicate_count");
  appState.totalGroups = count;
  const limit = appState.duplicatesPageSize;
  appState.duplicates = await invoke("get_duplicates_paginated", { offset: 0, limit });
  appState.duplicatesLoaded = appState.duplicates.length;
  appState.hasMoreDuplicates = appState.duplicatesLoaded < appState.totalGroups;
  appState.stats = await invoke("get_stats");
  notify();
}

export async function loadMoreDuplicates() {
  if (appState.loadingMoreDuplicates || !appState.hasMoreDuplicates) {
    return 0;
  }

  appState.loadingMoreDuplicates = true;
  notify();

  try {
    const offset = appState.duplicatesLoaded;
    const limit = appState.duplicatesPageSize;
    const more = await invoke("get_duplicates_paginated", { offset, limit });
    appState.duplicates = [...appState.duplicates, ...more];
    appState.duplicatesLoaded += more.length;
    appState.hasMoreDuplicates = appState.duplicatesLoaded < appState.totalGroups;
    notify();
    return more.length;
  } finally {
    appState.loadingMoreDuplicates = false;
    notify();
  }
}

export async function trashFile(path) {
  try {
    await invoke("trash_file", { path });
    appState.duplicates = appState.duplicates.map(group => ({
      ...group,
      files: group.files.filter(f => f.path !== path)
    })).filter(group => group.files.length > 1);
    notify();
    return true;
  } catch (err) {
    console.error("Failed to trash file:", err);
    return false;
  }
}

export async function replaceWithSymlink(path, targetPath) {
  try {
    await invoke("replace_with_symlink", { path, targetPath });
    appState.duplicates = appState.duplicates.map(group => ({
      ...group,
      files: group.files.map((f) => (
        f.path === path
          ? { ...f, path }
          : f
      ))
    }));
    notify();
    return true;
  } catch (err) {
    console.error("Failed to replace with symlink:", err);
    return false;
  }
}

export async function addPathToScan(dbPath, newPath) {
  clearScanListeners();
  resetDuplicateState();
  appState.scanning = true;
  appState.view = "scanning";
  appState.progress = { phase: "walking", files_walked: 0, files_to_hash: 0, files_hashed: 0, bytes_hashed: 0 };
  notify();

  await attachScanListeners({ finalizeTmpDb: false });

  await invoke("add_path_to_scan", { dbPath, newPath });
}

export async function mergeDatabases(sourceDbPath) {
  try {
    const result = await invoke("merge_databases", { sourceDbPath });
    await loadDuplicates();
    notify();
    return result;
  } catch (err) {
    console.error("Failed to merge databases:", err);
    throw err;
  }
}

export async function listScans() {
  return await invoke("list_scans");
}

export async function dismissScan(path) {
  await invoke("dismiss_scan", { path });
}

export async function selectFolder() {
  const selected = await open({ directory: true, multiple: false });
  return selected;
}

export async function selectFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Database", extensions: ["db"] }],
  });
  return selected;
}

export function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

export function formatNumber(n) {
  return n.toLocaleString();
}
