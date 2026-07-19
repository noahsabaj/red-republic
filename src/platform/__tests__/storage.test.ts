import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPending, initStorage, isTauri, resetStorageForTests, storage } from '../storage';

function fakeStorage(seed: Record<string, string> = {}): Storage {
  const map = new Map(Object.entries(seed));
  return {
    get length() { return map.size; },
    key: (i: number) => [...map.keys()][i] ?? null,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => { map.set(k, v); },
    removeItem: (k: string) => { map.delete(k); },
    clear: () => map.clear(),
  };
}

describe('storage facade (browser driver)', () => {
  beforeEach(() => resetStorageForTests());
  afterEach(() => vi.unstubAllGlobals());

  it('passes through to localStorage with identical semantics', () => {
    vi.stubGlobal('localStorage', fakeStorage({ a: '1', b: '2' }));
    const s = storage()!;
    expect(s.getItem('a')).toBe('1');
    expect(s.keys().sort()).toEqual(['a', 'b']);
    s.setItem('c', '3');
    expect(localStorage.getItem('c')).toBe('3');
    s.removeItem('a');
    expect(s.getItem('a')).toBeNull();
  });

  it('reads the global live — mid-suite stubs are honored', () => {
    vi.stubGlobal('localStorage', fakeStorage({ x: 'first' }));
    expect(storage()!.getItem('x')).toBe('first');
    vi.stubGlobal('localStorage', fakeStorage({ x: 'second' }));
    expect(storage()!.getItem('x')).toBe('second');
  });

  it('propagates quota exceptions from setItem', () => {
    const s = fakeStorage();
    s.setItem = () => { throw new DOMException('full', 'QuotaExceededError'); };
    vi.stubGlobal('localStorage', s);
    expect(() => storage()!.setItem('k', 'v')).toThrow(DOMException);
  });

  it('returns null when localStorage is missing or hostile', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(storage()).toBeNull();
  });

  it('initStorage is a no-op outside Tauri and flushPending resolves empty', async () => {
    vi.stubGlobal('localStorage', fakeStorage());
    expect(isTauri()).toBe(false);
    await initStorage();
    expect(storage()!.keys()).toEqual([]); // still the passthrough driver
    expect(await flushPending()).toEqual([]);
  });

  it('isTauri reflects the injected marker', () => {
    expect(isTauri()).toBe(false);
    vi.stubGlobal('__TAURI_INTERNALS__', {});
    expect(isTauri()).toBe(true);
  });
});
