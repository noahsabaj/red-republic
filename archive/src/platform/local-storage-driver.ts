import type { KV } from './storage';

/**
 * Browser driver: a thin live view over globalThis.localStorage.
 * Reads the global FRESH on every call — tests swap it with
 * vi.stubGlobal mid-suite, and semantics (including quota
 * DOMExceptions propagating from setItem) must match direct use.
 */
export function localStorageKV(): KV | null {
  try {
    const s = globalThis.localStorage;
    if (!s) return null;
    return {
      getItem: k => s.getItem(k),
      setItem: (k, v) => s.setItem(k, v),
      removeItem: k => s.removeItem(k),
      keys: () => {
        const out: string[] = [];
        for (let i = 0; i < s.length; i++) {
          const k = s.key(i);
          if (k !== null) out.push(k);
        }
        return out;
      },
    };
  } catch {
    return null; // storage disabled (privacy mode, permissions)
  }
}
