import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export const appState = {
  view: "home",
  scanning: false,
  progress: null,
  duplicates: [],
  stats: null,
  dbPath: null,
  activeView: "grid",
  selectedGroup: null,
};

export const listeners = new Set();

export function subscribe(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function notify() {
  listeners.forEach((fn) => fn(appState));
}

export function setView(view) {
  appState.view = view;
  notify();
}

export async function startScan(path) {
  appState.scanning = true;
  appState.view = "scanning";
  appState.progress = { phase: "walking", files_walked: 0, files_to_hash: 0, files_hashed: 0, bytes_hashed: 0 };
  notify();

  await listen("scan-progress", (event) => {
    appState.progress = event.payload;
    notify();
  });

  await listen("scan-complete", async (event) => {
    appState.scanning = false;
    appState.progress = null;
    appState.stats = event.payload.stats;
    if (!event.payload.aborted) {
      await loadDuplicates();
      appState.view = "results";
    } else {
      appState.view = "home";
    }
    notify();
  });

  await invoke("start_scan", { path });
}

export async function abortScan() {
  await invoke("abort_scan");
}

export async function openDatabase(path) {
  const info = await invoke("open_database", { path });
  appState.dbPath = path;
  appState.stats = await invoke("get_stats");
  await loadDuplicates();
  appState.view = "results";
  notify();
}

export async function loadDuplicates() {
  appState.duplicates = await invoke("get_duplicates");
  appState.stats = await invoke("get_stats");
  notify();
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
