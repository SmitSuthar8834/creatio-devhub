import { useSyncExternalStore } from "react";

/**
 * A small, app-wide error log. Any screen can record a failure with
 * {@link logError}; the global Errors page and SQL's Errors tab both read it
 * through {@link useErrorLog}. Entries are kept on this device (localStorage),
 * newest first, capped at {@link LIMIT}, and shared live between mounted views.
 */
export interface AppErrorEntry {
  id: string;
  at: number;
  /** Which screen produced it, e.g. "SQL", "Migration", "Packages". */
  source: string;
  /** Optional short action label, e.g. "Run", "Export", "Preview SQL". */
  context?: string;
  /** Optional environment the action targeted. */
  env?: string;
  /** Optional longer payload, e.g. the SQL text that failed. */
  detail?: string;
  message: string;
}

const KEY = "creatio-devhub.error-log.v1";
const LIMIT = 100;

const newId = () =>
  globalThis.crypto?.randomUUID?.() ?? `err-${Date.now()}-${Math.random().toString(16).slice(2)}`;

function read(): AppErrorEntry[] {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (!Array.isArray(value)) return [];
    return value.filter(
      (item): item is AppErrorEntry =>
        typeof item?.id === "string" &&
        typeof item?.at === "number" &&
        typeof item?.source === "string" &&
        typeof item?.message === "string",
    );
  } catch {
    return [];
  }
}

let entries: AppErrorEntry[] = read();
const listeners = new Set<() => void>();

function emit() {
  listeners.forEach((listener) => listener());
}

function persist(next: AppErrorEntry[]) {
  entries = next.slice(0, LIMIT);
  localStorage.setItem(KEY, JSON.stringify(entries));
  emit();
}

/** Record a failure so it collects in the Errors view. Safe to call anywhere. */
export function logError(
  source: string,
  message: string,
  opts?: { context?: string; env?: string; detail?: string },
): void {
  persist([
    {
      id: newId(),
      at: Date.now(),
      source,
      message,
      context: opts?.context,
      env: opts?.env,
      detail: opts?.detail,
    },
    ...entries,
  ]);
}

export function dismissError(id: string): void {
  persist(entries.filter((entry) => entry.id !== id));
}

/** Clear every entry, or just those from one source. */
export function clearErrors(source?: string): void {
  persist(source ? entries.filter((entry) => entry.source !== source) : []);
}

function subscribe(callback: () => void): () => void {
  listeners.add(callback);
  // Keep multiple windows of the same app in sync.
  const onStorage = (event: StorageEvent) => {
    if (event.key === KEY) {
      entries = read();
      callback();
    }
  };
  window.addEventListener("storage", onStorage);
  return () => {
    listeners.delete(callback);
    window.removeEventListener("storage", onStorage);
  };
}

const snapshot = () => entries;

/** Subscribe a component to the log, optionally filtered to one source. */
export function useErrorLog(source?: string): AppErrorEntry[] {
  const all = useSyncExternalStore(subscribe, snapshot, snapshot);
  return source ? all.filter((entry) => entry.source === source) : all;
}
