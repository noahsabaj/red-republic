// ============================================================
// Storage facade: ONE seam through which all persistence flows.
//
// Browser (and vitest): a live localStorage passthrough — semantics
// identical to touching localStorage directly, including quota
// exceptions and test stubbing via vi.stubGlobal.
//
// Desktop (Tauri): initStorage() swaps in a file-backed driver that
// keeps saves/settings as real files under the app-data directory,
// hydrated once at boot so every read/write stays synchronous for
// callers (save slots are listed during React render). Disk writes
// happen asynchronously behind the cache; failures surface on the
// error bus (App wires it to a toast) — never silently.
//
// Rule (see CLAUDE.md): nothing outside src/platform touches
// localStorage or @tauri-apps APIs directly — always go through
// storage()/initStorage()/flushPending().
// ============================================================
import { localStorageKV } from './local-storage-driver';
import type { TauriFsDriver } from './tauri-fs-driver';

/** Minimal synchronous key-value surface both drivers implement. */
export interface KV {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void; // browser: throws DOMException on quota
  removeItem(key: string): void;
  keys(): string[];
}

export type StorageErrorCode = 'disk-full' | 'permission' | 'io';

export interface StorageFlushFailure {
  key: string;
  code: StorageErrorCode;
  message: string;
}

/** True when running inside a Tauri webview (v2 injects this marker). */
export function isTauri(): boolean {
  return typeof globalThis !== 'undefined' && '__TAURI_INTERNALS__' in globalThis;
}

let fileDriver: TauriFsDriver | null = null;
let initialized = false;
const errorListeners = new Set<(f: StorageFlushFailure) => void>();

function emitError(f: StorageFlushFailure) {
  errorListeners.forEach(cb => cb(f));
}

/**
 * The active driver, or null when no storage exists at all (private
 * mode with storage disabled). Resolved fresh on every call so test
 * stubs of globalThis.localStorage keep working.
 */
export function storage(): KV | null {
  if (fileDriver) return fileDriver;
  if (import.meta.env.DEV && isTauri() && !initialized) {
    // only settings' import-time load may legitimately land here (main.tsx
    // re-hydrates it after init); anything else is a boot-order bug
    console.warn('[storage] pre-init read on desktop');
  }
  return localStorageKV();
}

/**
 * Boot-time init, awaited by main.tsx before the first render.
 * Browser: instant no-op. Desktop: builds the file driver, migrates any
 * webview-localStorage data, hydrates the cache, and swaps it in. Never
 * rejects — on failure the app degrades to webview localStorage and the
 * error bus reports it.
 */
export async function initStorage(): Promise<void> {
  initialized = true;
  if (!isTauri()) return;
  try {
    const [{ TauriFsDriver }, { makeTauriFsBackend }] = await Promise.all([
      import('./tauri-fs-driver'),
      import('./tauri-fs-backend'),
    ]);
    const driver = new TauriFsDriver(makeTauriFsBackend());
    await driver.init(localStorageKV());
    driver.onFlushError(emitError);
    fileDriver = driver;
  } catch (e) {
    emitError({
      key: '*',
      code: 'io',
      message: `File storage unavailable — falling back to browser storage. ${String(e)}`,
    });
  }
}

/**
 * Drain the write-behind queue now (quit paths call this before the
 * window is destroyed). Resolves with any failures; browser: [].
 */
export async function flushPending(): Promise<StorageFlushFailure[]> {
  if (!fileDriver) return [];
  return fileDriver.flush();
}

/** Async-write failure bus; returns unsubscribe. */
export function onStorageFlushError(cb: (f: StorageFlushFailure) => void): () => void {
  errorListeners.add(cb);
  return () => errorListeners.delete(cb);
}

/** Test seam: drop the file driver and re-arm init. */
export function resetStorageForTests(): void {
  fileDriver = null;
  initialized = false;
  errorListeners.clear();
}
