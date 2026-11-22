import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import type {
  AppInfo,
  AppSettings,
  ClassLoadingSnapshot,
  DumpInfo,
  GcEvent,
  HeapAnalysis,
  JdkScan,
  JfrRecording,
  JitSnapshot,
  JvmProcess,
  MonitorSample,
  RuntimeDetails,
  SamplePoint,
  SpringCandidate,
  SpringConnection,
  SpringSnapshot,
  TerminationResult,
  ThreadSnapshot,
} from "../types";

export function backendAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!backendAvailable()) {
    throw new Error("The native backend is unavailable. Launch the Java Control Centre desktop app.");
  }
  return invoke<T>(command, args);
}

export const api = {
  info: () => call<AppInfo>("get_app_info"),
  scanJdks: (force = false) => call<JdkScan>("scan_jdks", { force }),
  listProcesses: () => call<JvmProcess[]>("list_processes"),
  getProcess: (pid: number) => call<JvmProcess>("get_process", { pid }),
  terminateProcess: (pid: number, force: boolean) =>
    call<TerminationResult>("terminate_process", { pid, force }),
  monitorSample: (pid: number) => call<MonitorSample>("monitor_sample", { pid }),
  getSamples: (pid: number, minutes = 30, limit = 600) =>
    call<SamplePoint[]>("get_samples", { pid, minutes, limit }),
  getGcEvents: (pid: number, limit = 100) => call<GcEvent[]>("get_gc_events", { pid, limit }),
  getThreads: (pid: number) => call<ThreadSnapshot>("get_threads", { pid }),
  getClassLoading: (pid: number) => call<ClassLoadingSnapshot>("get_class_loading", { pid }),
  getJit: (pid: number) => call<JitSnapshot>("get_jit", { pid }),
  getRuntimeDetails: (pid: number) => call<RuntimeDetails>("get_runtime_details", { pid }),
  checkJfr: (pid: number) => call<JfrRecording[]>("check_jfr", { pid }),
  startJfr: (pid: number, name?: string, durationSecs?: number) =>
    call<string>("start_jfr", { pid, name, durationSecs }),
  dumpJfr: (pid: number, name: string) => call<DumpInfo>("dump_jfr", { pid, name }),
  stopJfr: (pid: number, name: string) => call<string>("stop_jfr", { pid, name }),
  captureHeapDump: (pid: number) => call<DumpInfo>("capture_heap_dump", { pid }),
  analyzeHeap: (path: string) => call<HeapAnalysis>("analyze_heap", { path }),
  listDumps: () => call<DumpInfo[]>("list_dumps"),
  deleteDump: (id: string, deleteFile = false) =>
    call<void>("delete_dump", { id, deleteFile }),
  openPath: (path: string) => call<void>("open_path", { path }),
  probeSpring: (baseUrl: string, username?: string, password?: string) =>
    call<SpringSnapshot>("probe_spring", { baseUrl, username, password }),
  discoverSpring: () => call<SpringCandidate[]>("discover_spring"),
  listSpringConnections: () => call<SpringConnection[]>("list_spring_connections"),
  saveSpringConnection: (name: string, baseUrl: string, username?: string, password?: string) =>
    call<SpringConnection>("save_spring_connection", { name, baseUrl, username, password }),
  deleteSpringConnection: (id: number) => call<void>("delete_spring_connection", { id }),
  checkSpringConnection: (id: number) => call<SpringSnapshot>("check_spring_connection", { id }),
  getSettings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
  pruneStorage: () => call<void>("prune_storage"),
};

export async function pickHprofFile(): Promise<string | null> {
  if (!backendAvailable()) return null;
  const selection = await openDialog({
    title: "Select an HPROF heap dump",
    filters: [{ name: "Heap dumps", extensions: ["hprof", "bin"] }],
    multiple: false,
    directory: false,
  });
  return typeof selection === "string" ? selection : null;
}

export function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(text);
  const area = document.createElement("textarea");
  area.value = text;
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  document.execCommand("copy");
  area.remove();
  return Promise.resolve();
}
