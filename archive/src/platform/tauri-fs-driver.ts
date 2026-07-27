// ============================================================
// Desktop storage driver: the synchronous KV surface over real files
// in the app-data directory.
//
// Sync reads are possible because init() hydrates EVERY file into an
// in-memory cache once at boot (a full save roster is a few MB — cheap,
// and unlike localStorage there is no 5 MB ceiling). Writes hit the
// cache immediately and flush to disk asynchronously: a debounced,
// single-flight drain writes dirty keys sequentially in insertion
// order — save-slots writes blob-then-index, and preserving that order
// on disk preserves its crash-safety invariant.
//
// The backend is injected (a 6-method adapter over @tauri-apps/plugin-fs
// lives in tauri-fs-backend.ts) so this entire class is unit-testable
// in Node with an in-memory fake.
// ============================================================
import type { KV, StorageErrorCode, StorageFlushFailure } from './storage';

export interface FsBackend {
  exists(relPath: string): Promise<boolean>;
  mkdir(relPath: string): Promise<void>; // recursive, ok-if-exists
  readDir(relPath: string): Promise<string[]>; // plain file names
  readTextFile(relPath: string): Promise<string>;
  writeTextFile(relPath: string, contents: string): Promise<void>;
  remove(relPath: string): Promise<void>;
}

const MIGRATION_MARKER = '.migrated-from-localstorage';
const SETTINGS_KEY = 'rr.settings.v1';
const INDEX_KEY = 'redrepublic:save-index';
const SAVE_PREFIX = 'redrepublic:save:';

/** Filename-safe encoding: percent-encode, including chars Windows rejects. */
function encodeName(s: string): string {
  return encodeURIComponent(s).replace(/[!'()*~]/g, c => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
}

/** Known namespaces get human-friendly files (users can back up saves/). */
export function keyToPath(key: string): string {
  if (key === SETTINGS_KEY) return 'settings.json';
  if (key === INDEX_KEY) return 'save-index.json';
  if (key.startsWith(SAVE_PREFIX)) return `saves/${encodeName(key.slice(SAVE_PREFIX.length))}.json`;
  return `kv/${encodeName(key)}.txt`;
}

export function pathToKey(relPath: string): string | null {
  if (relPath === 'settings.json') return SETTINGS_KEY;
  if (relPath === 'save-index.json') return INDEX_KEY;
  const save = /^saves\/(.+)\.json$/.exec(relPath);
  if (save) return SAVE_PREFIX + decodeURIComponent(save[1]);
  const kv = /^kv\/(.+)\.txt$/.exec(relPath);
  if (kv) return decodeURIComponent(kv[1]);
  return null;
}

function classify(e: unknown): StorageErrorCode {
  const msg = String(e).toLowerCase();
  if (msg.includes('os error 28') || msg.includes('no space left') || msg.includes('disk full')) return 'disk-full';
  if (msg.includes('os error 5') || msg.includes('access is denied') || msg.includes('permission denied')) return 'permission';
  return 'io';
}

const TOMBSTONE = Symbol('deleted');

export class TauriFsDriver implements KV {
  private backend: FsBackend;
  private flushDelayMs: number;
  private cache = new Map<string, string>();
  /** insertion-ordered; latest-wins coalescing keeps a key's position */
  private dirty = new Map<string, string | typeof TOMBSTONE>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private draining: Promise<StorageFlushFailure[]> | null = null;
  private errorListeners = new Set<(f: StorageFlushFailure) => void>();

  constructor(backend: FsBackend, opts: { flushDelayMs?: number } = {}) {
    this.backend = backend;
    this.flushDelayMs = opts.flushDelayMs ?? 100;
  }

  /** mkdirs → one-time localStorage migration → hydrate the cache. */
  async init(webviewLocalStorage: KV | null): Promise<void> {
    await this.backend.mkdir('saves');
    await this.backend.mkdir('kv');

    if (webviewLocalStorage && !(await this.backend.exists(MIGRATION_MARKER))) {
      // copy-if-missing: idempotent per key, resumes after a mid-run crash,
      // never clobbers file data that is newer than the webview copy
      for (const key of webviewLocalStorage.keys()) {
        if (!key.startsWith('redrepublic:') && !key.startsWith('rr.settings.')) continue;
        const value = webviewLocalStorage.getItem(key);
        if (value === null) continue;
        const path = keyToPath(key);
        if (!(await this.backend.exists(path))) {
          await this.backend.writeTextFile(path, value);
        }
      }
      await this.backend.writeTextFile(MIGRATION_MARKER, new Date().toISOString());
    }

    const hydrate = async (relPath: string) => {
      const key = pathToKey(relPath);
      if (key === null) return;
      try {
        this.cache.set(key, await this.backend.readTextFile(relPath));
      } catch {
        // unreadable file: leave it out — parseSave-level self-heal covers it
      }
    };
    for (const file of ['settings.json', 'save-index.json']) {
      if (await this.backend.exists(file)) await hydrate(file);
    }
    for (const name of await this.backend.readDir('saves')) await hydrate(`saves/${name}`);
    for (const name of await this.backend.readDir('kv')) await hydrate(`kv/${name}`);
  }

  onFlushError(cb: (f: StorageFlushFailure) => void): () => void {
    this.errorListeners.add(cb);
    return () => this.errorListeners.delete(cb);
  }

  // ---------- KV (sync, cache-backed) ----------

  getItem(key: string): string | null {
    return this.cache.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.cache.set(key, value);
    this.dirty.set(key, value);
    this.schedule();
  }

  removeItem(key: string): void {
    this.cache.delete(key);
    this.dirty.set(key, TOMBSTONE);
    this.schedule();
  }

  keys(): string[] {
    return [...this.cache.keys()];
  }

  // ---------- write-behind ----------

  private schedule(): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.drain();
    }, this.flushDelayMs);
  }

  /** Single-flight sequential drain in dirty-insertion order. */
  private drain(): Promise<StorageFlushFailure[]> {
    if (this.draining) return this.draining;
    if (this.dirty.size === 0) return Promise.resolve([]);
    const batch = this.dirty;
    this.dirty = new Map();
    this.draining = (async () => {
      const failures: StorageFlushFailure[] = [];
      for (const [key, value] of batch) {
        const path = keyToPath(key);
        try {
          if (value === TOMBSTONE) await this.backend.remove(path);
          else await this.backend.writeTextFile(path, value);
        } catch (e) {
          // stays served from cache; NOT auto-requeued (no retry storms) —
          // the next setItem on the key queues it again
          const failure: StorageFlushFailure = { key, code: classify(e), message: String(e) };
          failures.push(failure);
          this.errorListeners.forEach(cb => cb(failure));
        }
      }
      this.draining = null;
      return failures;
    })();
    return this.draining;
  }

  /** Drain everything now (quit paths await this before destroying the window). */
  async flush(): Promise<StorageFlushFailure[]> {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    const failures: StorageFlushFailure[] = [];
    // loop: a drain may already be in flight, and mutations may land mid-drain
    while (this.draining || this.dirty.size > 0) {
      failures.push(...await (this.draining ?? this.drain()));
    }
    return failures;
  }
}
