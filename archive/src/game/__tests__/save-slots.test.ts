import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  AUTOSAVE_SLOTS, QUICKSAVE_SLOT, deleteSlot, importSave, listSlots, newSlotId,
  readSlot, slotKind, writeAutosave, writeSlot,
} from '../save-slots';
import { SaveError } from '../save-format';
import type { SaveGameV1 } from '../save-format';
import { makeEngine } from './helpers';

/** Minimal in-memory Storage double. */
function fakeStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() { return map.size; },
    key: (i: number) => [...map.keys()][i] ?? null,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => { map.set(k, v); },
    removeItem: (k: string) => { map.delete(k); },
    clear: () => map.clear(),
  };
}

function blobAt(savedAt: number): SaveGameV1 {
  const s = makeEngine().serialize();
  s.header.savedAt = savedAt;
  return s;
}

describe('save slots', () => {
  let store: Storage;

  beforeEach(() => {
    store = fakeStorage();
    vi.stubGlobal('localStorage', store);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('writes, lists (newest first), reads back and deletes', () => {
    const idA = newSlotId();
    expect(writeSlot(idA, blobAt(1000)).ok).toBe(true);
    expect(writeSlot(QUICKSAVE_SLOT, blobAt(3000)).ok).toBe(true);
    const idB = newSlotId();
    expect(writeSlot(idB, blobAt(2000)).ok).toBe(true);

    const slots = listSlots();
    expect(slots.map(s => s.slotId)).toEqual([QUICKSAVE_SLOT, idB, idA]);
    expect(slots[0].kind).toBe('quick');
    expect(slots[1].kind).toBe('manual');
    expect(slots[0].sizeBytes).toBeGreaterThan(1000);

    const back = readSlot(idA);
    expect(back.header.savedAt).toBe(1000);
    expect(back.header.seed).toBe(1);

    deleteSlot(idA);
    expect(listSlots().map(s => s.slotId)).toEqual([QUICKSAVE_SLOT, idB]);
    expect(() => readSlot(idA)).toThrow(SaveError);
    deleteSlot(idA); // idempotent
  });

  it('self-heals a missing index by rescanning blobs', () => {
    const id = newSlotId();
    writeSlot(id, blobAt(500));
    store.removeItem('redrepublic:save-index');
    const slots = listSlots();
    expect(slots).toHaveLength(1);
    expect(slots[0].slotId).toBe(id);
    expect(slots[0].header.savedAt).toBe(500);
  });

  it('reports quota exhaustion as a result, not a throw', () => {
    const original = store.setItem.bind(store);
    store.setItem = (k: string, v: string) => {
      if (k.startsWith('redrepublic:save:')) throw new DOMException('full', 'QuotaExceededError');
      original(k, v);
    };
    const res = writeSlot(newSlotId(), blobAt(1));
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error).toBe('quota');
  });

  it('returns storage-unavailable when localStorage is missing', () => {
    vi.stubGlobal('localStorage', undefined);
    const res = writeSlot('x', blobAt(1));
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error).toBe('storage-unavailable');
    expect(listSlots()).toEqual([]);
  });

  it('rotates autosaves into the empty-or-oldest slot', () => {
    expect(writeAutosave(blobAt(100)).ok).toBe(true);
    expect(writeAutosave(blobAt(200)).ok).toBe(true);
    expect(writeAutosave(blobAt(300)).ok).toBe(true);
    // all three autosave slots used; the fourth write replaces the oldest (100)
    expect(writeAutosave(blobAt(400)).ok).toBe(true);
    const autos = listSlots().filter(s => s.kind === 'auto');
    expect(autos).toHaveLength(AUTOSAVE_SLOTS.length);
    expect(autos.map(a => a.header.savedAt).sort((a, b) => a - b)).toEqual([200, 300, 400]);
  });

  it('slotKind classifies ids', () => {
    expect(slotKind(QUICKSAVE_SLOT)).toBe('quick');
    expect(slotKind('autosave-1')).toBe('auto');
    expect(slotKind(newSlotId())).toBe('manual');
  });

  it('importSave rejects garbage files with a SaveError', async () => {
    const bad = new File(['{ not json'], 'save.json', { type: 'application/json' });
    await expect(importSave(bad)).rejects.toThrow(SaveError);
    const good = new File([JSON.stringify(blobAt(7))], 'save.json', { type: 'application/json' });
    const save = await importSave(good);
    expect(save.header.savedAt).toBe(7);
  });
});
