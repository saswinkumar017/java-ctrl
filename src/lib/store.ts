import type { AppInfo, AppSettings, JdkInfo, JvmProcess } from "../types";
import { api } from "./api";
import { friendlyError, notify } from "./toast";

export interface AppState {
  info: AppInfo | null;
  jdks: JdkInfo[];
  settings: AppSettings;
  processes: JvmProcess[];
  selectedPid: number | null;
  processesLoading: boolean;
  backendError: string | null;
}

type Listener = (state: Readonly<AppState>, changed: ReadonlySet<keyof AppState>) => void;

const initial: AppState = {
  info: null,
  jdks: [],
  settings: { defaultJdk: null, refreshIntervalMs: 2000, retentionDays: 7, sampleRetentionMinutes: 30 },
  processes: [],
  selectedPid: null,
  processesLoading: true,
  backendError: null,
};

class Store {
  private state: AppState = { ...initial };
  private listeners = new Set<Listener>();

  get(): Readonly<AppState> {
    return this.state;
  }

  set(patch: Partial<AppState>): void {
    const changed = new Set<keyof AppState>();
    for (const [key, value] of Object.entries(patch) as [keyof AppState, never][]) {
      if (this.state[key] !== value) {
        this.state[key] = value;
        changed.add(key);
      }
    }
    if (changed.size === 0) return;
    for (const listener of this.listeners) listener(this.state, changed);
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}

export const store = new Store();
let selectionInitialised = false;

export function selectedProcess(): JvmProcess | null {
  const { processes, selectedPid } = store.get();
  return processes.find((process) => process.pid === selectedPid) ?? null;
}

export function requirePid(): number | null {
  return store.get().selectedPid;
}

export function selectProcess(pid: number | null): void {
  store.set({ selectedPid: pid });
}

export async function refreshProcesses(silent = true): Promise<void> {
  try {
    const processes = await api.listProcesses();
    const current = store.get().selectedPid;
    let next = current;
    if (!selectionInitialised) {
      const remembered = Number(window.localStorage.getItem("jcc.lastPid") ?? "0");
      next = processes.find((process) => process.pid === remembered)?.pid
        ?? processes.find((process) => process.attachable)?.pid
        ?? processes[0]?.pid
        ?? null;
      selectionInitialised = true;
    }
    if (next != null && !processes.some((process) => process.pid === next)) next = null;
    store.set({ processes, selectedPid: next, processesLoading: false, backendError: null });
    if (next != null) window.localStorage.setItem("jcc.lastPid", String(next));
    else window.localStorage.removeItem("jcc.lastPid");
  } catch (error) {
    store.set({ processesLoading: false });
    if (!silent) notify.error("Could not refresh processes", friendlyError(error));
    else store.set({ backendError: friendlyError(error) });
  }
}

export async function refreshJdks(force = false): Promise<void> {
  const scan = await api.scanJdks(force);
  store.set({ jdks: scan.jdks });
}

export async function bootstrap(): Promise<void> {
  try {
    const [info, settings, processes, jdks] = await Promise.all([
      api.info(),
      api.getSettings(),
      api.listProcesses(),
      api.scanJdks(false),
    ]);
    store.set({
      info,
      settings,
      processes,
      jdks: jdks.jdks,
      processesLoading: false,
      backendError: null,
    });
    const remembered = Number(window.localStorage.getItem("jcc.lastPid") ?? "0");
    const selected =
      processes.find((process) => process.pid === remembered)?.pid
      ?? processes.find((process) => process.attachable)?.pid
      ?? processes[0]?.pid
      ?? null;
    selectionInitialised = true;
    store.set({ selectedPid: selected });
  } catch (error) {
    store.set({ processesLoading: false, backendError: friendlyError(error) });
  }
}
