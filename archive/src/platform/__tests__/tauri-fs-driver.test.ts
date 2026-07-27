import { describe, expect, it } from 'vitest';
import { TauriFsDriver, keyToPath, pathToKey } from '../tauri-fs-driver';
import type { FsBackend } from '../tauri-fs-driver';
import type { KV } from '../storage';

/** In-memory FsBackend with a write log and per-path injectable failures. */
function fakeBackend(seed: Record<string, string> = {}) {
  const files = new Map(Object.entries(seed));
  const log: { op: string; path: string }[] = [];
  const failOnce = new Map<string, string>(); // path -> error message
  const backend: FsBackend = {
    exists: p => Promise.resolve(files.has(p)),
    mkdir: () => Promise.resolve(), // dirs are implicit in the fake
    readDir: p => Promise.resolve(
      [...files.keys()]
        .filter(f => f.startsWith(`${p}/`) && !f.slice(p.length + 1).includes('/'))
        .map(f => f.slice(p.length + 1)),
    ),
    readTextFile: p => {
      const v = files.get(p);
      return v === undefined ? Promise.reject(new Error('not found')) : Promise.resolve(v);
    },
    writeTextFile: (p, contents) => {
      const fail = failOnce.get(p);
      if (fail) { failOnce.delete(p); return Promise.reject(new Error(fail)); }
      files.set(p, contents);
      log.push({ op: 'write', path: p });
      return Promise.resolve();
    },
    remove: p => {
      files.delete(p);
      log.push({ op: 'remove', path: p });
      return Promise.resolve();
    },
  };
  return { backend, files, log, failOnce };
}

function fakeKV(seed: Record<string, string> = {}): KV {
  const map = new Map(Object.entries(seed));
  return {
    getItem: k => map.get(k) ?? null,
    setItem: (k, v) => { map.set(k, v); },
    removeItem: k => { map.delete(k); },
    keys: () => [...map.keys()],
  };
}

const makeDriver = async (fb: ReturnType<typeof fakeBackend>, webview: KV | null = null) => {
  const d = new TauriFsDriver(fb.backend, { flushDelayMs: 5 });
  await d.init(webview);
  return d;
};

describe('key↔path bijection', () => {
  it('maps the three namespaces and round-trips', () => {
    const cases: [string, string][] = [
      ['rr.settings.v1', 'settings.json'],
      ['redrepublic:save-index', 'save-index.json'],
      ['redrepublic:save:quicksave', 'saves/quicksave.json'],
      ['redrepublic:save:manual-abc-123', 'saves/manual-abc-123.json'],
      ['some.other.key', 'kv/some.other.key.txt'],
    ];
    for (const [key, path] of cases) {
      expect(keyToPath(key)).toBe(path);
      expect(pathToKey(path)).toBe(key);
    }
  });

  it('percent-encodes hostile names, including Windows-invalid chars', () => {
    const key = 'redrepublic:save:a/b\\c*d';
    const path = keyToPath(key);
    expect(path.startsWith('saves/')).toBe(true);
    expect(path.slice('saves/'.length)).not.toMatch(/[/\\*]/); // no raw separators/stars in the filename
    expect(pathToKey(path)).toBe(key);
  });

  it('rejects unknown paths', () => {
    expect(pathToKey('.migrated-from-localstorage')).toBeNull();
    expect(pathToKey('random.bin')).toBeNull();
  });
});

describe('TauriFsDriver', () => {
  it('hydrates every namespace at init and serves sync reads', async () => {
    const fb = fakeBackend({
      'settings.json': '{"muted":true}',
      'save-index.json': '{}',
      'saves/quicksave.json': 'BLOB-Q',
      'saves/manual-x.json': 'BLOB-M',
      'kv/extra.key.txt': 'V',
    });
    const d = await makeDriver(fb);
    expect(d.getItem('rr.settings.v1')).toBe('{"muted":true}');
    expect(d.getItem('redrepublic:save:quicksave')).toBe('BLOB-Q');
    expect(d.getItem('extra.key')).toBe('V');
    expect(d.keys().sort()).toEqual([
      'extra.key', 'redrepublic:save-index', 'redrepublic:save:manual-x',
      'redrepublic:save:quicksave', 'rr.settings.v1',
    ]);
  });

  it('coalesces rapid writes: three sets, one file write with the last value', async () => {
    const fb = fakeBackend();
    const d = await makeDriver(fb);
    d.setItem('redrepublic:save:s1', 'v1');
    d.setItem('redrepublic:save:s1', 'v2');
    d.setItem('redrepublic:save:s1', 'v3');
    await d.flush();
    expect(fb.log.filter(l => l.path === 'saves/s1.json')).toHaveLength(1);
    expect(fb.files.get('saves/s1.json')).toBe('v3');
  });

  it('drains in insertion order — blob lands before index (crash-safety invariant)', async () => {
    const fb = fakeBackend();
    const d = await makeDriver(fb);
    d.setItem('redrepublic:save:slot', 'BLOB');
    d.setItem('redrepublic:save-index', 'INDEX');
    await d.flush();
    const writes = fb.log.map(l => l.path);
    expect(writes.indexOf('saves/slot.json')).toBeLessThan(writes.indexOf('save-index.json'));
  });

  it('coalescing preserves a key\'s queue position', async () => {
    const fb = fakeBackend();
    const d = await makeDriver(fb);
    d.setItem('redrepublic:save:slot', 'BLOB-1');
    d.setItem('redrepublic:save-index', 'INDEX');
    d.setItem('redrepublic:save:slot', 'BLOB-2'); // re-set must NOT move it after the index
    await d.flush();
    const writes = fb.log.map(l => l.path);
    expect(writes.indexOf('saves/slot.json')).toBeLessThan(writes.indexOf('save-index.json'));
    expect(fb.files.get('saves/slot.json')).toBe('BLOB-2');
  });

  it('removeItem deletes the file; set-after-remove recreates it', async () => {
    const fb = fakeBackend({ 'saves/gone.json': 'OLD' });
    const d = await makeDriver(fb);
    d.removeItem('redrepublic:save:gone');
    expect(d.getItem('redrepublic:save:gone')).toBeNull();
    await d.flush();
    expect(fb.files.has('saves/gone.json')).toBe(false);
    d.setItem('redrepublic:save:gone', 'NEW');
    await d.flush();
    expect(fb.files.get('saves/gone.json')).toBe('NEW');
  });

  it('surfaces flush failures once, keeps the cache serving, and re-queues on next set', async () => {
    const fb = fakeBackend();
    const d = await makeDriver(fb);
    const seen: string[] = [];
    d.onFlushError(f => seen.push(`${f.key}:${f.code}`));

    fb.failOnce.set('saves/s1.json', 'os error 28: No space left on device');
    d.setItem('redrepublic:save:s1', 'PRECIOUS');
    const failures = await d.flush();
    expect(failures).toHaveLength(1);
    expect(failures[0].code).toBe('disk-full');
    expect(seen).toEqual(['redrepublic:save:s1:disk-full']);
    expect(d.getItem('redrepublic:save:s1')).toBe('PRECIOUS'); // cache intact
    expect(fb.files.has('saves/s1.json')).toBe(false);

    d.setItem('redrepublic:save:s1', 'PRECIOUS'); // re-queue
    expect(await d.flush()).toEqual([]);
    expect(fb.files.get('saves/s1.json')).toBe('PRECIOUS');
  });

  it('classifies permission and generic io errors', async () => {
    const fb = fakeBackend();
    const d = await makeDriver(fb);
    fb.failOnce.set('saves/a.json', 'os error 5: Access is denied');
    d.setItem('redrepublic:save:a', 'x');
    expect((await d.flush())[0].code).toBe('permission');
    fb.failOnce.set('saves/b.json', 'something exploded');
    d.setItem('redrepublic:save:b', 'x');
    expect((await d.flush())[0].code).toBe('io');
  });
});

describe('migration from webview localStorage', () => {
  it('copies namespaced keys to files, ignores others, writes the marker', async () => {
    const fb = fakeBackend();
    const webview = fakeKV({
      'rr.settings.v1': 'SETTINGS',
      'redrepublic:save:old': 'OLD-SAVE',
      'redrepublic:save-index': 'IDX',
      'unrelated-key': 'NOPE',
    });
    const d = await makeDriver(fb, webview);
    expect(fb.files.get('settings.json')).toBe('SETTINGS');
    expect(fb.files.get('saves/old.json')).toBe('OLD-SAVE');
    expect(fb.files.get('save-index.json')).toBe('IDX');
    expect([...fb.files.keys()].some(f => f.includes('unrelated'))).toBe(false);
    expect(fb.files.has('.migrated-from-localstorage')).toBe(true);
    // and the migrated data is hydrated
    expect(d.getItem('redrepublic:save:old')).toBe('OLD-SAVE');
  });

  it('is idempotent: a second init with the marker copies nothing', async () => {
    const fb = fakeBackend();
    await makeDriver(fb, fakeKV({ 'redrepublic:save:x': 'V1' }));
    const writesAfterFirst = fb.log.length;
    await makeDriver(fb, fakeKV({ 'redrepublic:save:x': 'V2-NEWER-IN-LS' }));
    expect(fb.log.length).toBe(writesAfterFirst); // zero new writes
    expect(fb.files.get('saves/x.json')).toBe('V1'); // file wins forever
  });

  it('resumes a partial migration without clobbering existing files', async () => {
    const fb = fakeBackend({ 'saves/done.json': 'FILE-TRUTH' }); // no marker yet
    const webview = fakeKV({
      'redrepublic:save:done': 'STALE-LS',
      'redrepublic:save:pending': 'FRESH',
    });
    const d = await makeDriver(fb, webview);
    expect(fb.files.get('saves/done.json')).toBe('FILE-TRUTH'); // copy-if-missing
    expect(fb.files.get('saves/pending.json')).toBe('FRESH');
    expect(d.getItem('redrepublic:save:done')).toBe('FILE-TRUTH');
  });
});
