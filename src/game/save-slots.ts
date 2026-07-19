// ============================================================
// Save-slot storage: localStorage-backed slots for save blobs, plus
// export/import to .json files. The engine never imports this module —
// persistence is orchestration, owned by the UI layer.
//
// Layout: each slot is one key (PREFIX + slotId); a separate index key
// holds every slot's header so menus list saves without parsing bodies.
// The blob is written before the index (crash-safe), and a missing or
// corrupt index self-heals by rescanning the prefixed keys.
// ============================================================
import { SaveError, parseSave } from './save-format';
import type { SaveGameV1, SaveHeaderV1 } from './save-format';

const PREFIX = 'redrepublic:save:';
const INDEX_KEY = 'redrepublic:save-index';

export const QUICKSAVE_SLOT = 'quicksave';
export const AUTOSAVE_SLOTS = ['autosave-0', 'autosave-1', 'autosave-2'] as const;

export type SlotKind = 'manual' | 'auto' | 'quick';

export interface SlotMeta {
  slotId: string;
  kind: SlotKind;
  header: SaveHeaderV1;
  sizeBytes: number;
}

export type WriteResult =
  | { ok: true; slotId: string }
  | { ok: false; error: 'quota' | 'storage-unavailable' | 'unknown'; message: string };

export function slotKind(slotId: string): SlotKind {
  if (slotId === QUICKSAVE_SLOT) return 'quick';
  if ((AUTOSAVE_SLOTS as readonly string[]).includes(slotId)) return 'auto';
  return 'manual';
}

/** Fresh id for a manual slot. Uniqueness beyond the timestamp via a suffix. */
export function newSlotId(): string {
  return `manual-${Date.now().toString(36)}-${Math.floor(Math.random() * 36 ** 4).toString(36)}`;
}

function storage(): Storage | null {
  try {
    const s = globalThis.localStorage;
    if (!s) return null;
    return s;
  } catch {
    return null; // storage disabled (private mode, permissions)
  }
}

type Index = Record<string, { header: SaveHeaderV1; sizeBytes: number }>;

function readIndex(s: Storage): Index | null {
  try {
    const raw = s.getItem(INDEX_KEY);
    if (!raw) return null;
    const idx = JSON.parse(raw) as Index;
    if (typeof idx !== 'object' || idx === null || Array.isArray(idx)) return null;
    return idx;
  } catch {
    return null;
  }
}

/** Rebuild the index by scanning prefixed keys (torn writes, cleared index). */
function rebuildIndex(s: Storage): Index {
  const idx: Index = {};
  for (let i = 0; i < s.length; i++) {
    const key = s.key(i);
    if (!key?.startsWith(PREFIX)) continue;
    const raw = s.getItem(key);
    if (!raw) continue;
    try {
      const save = parseSave(raw);
      idx[key.slice(PREFIX.length)] = { header: save.header, sizeBytes: raw.length };
    } catch {
      // unreadable blob: leave it out of the index (readSlot will surface the error)
    }
  }
  try {
    s.setItem(INDEX_KEY, JSON.stringify(idx));
  } catch {
    // index persists best-effort; listing still works from the in-memory copy
  }
  return idx;
}

/** All slots, newest first. Never throws; empty when storage is unavailable. */
export function listSlots(): SlotMeta[] {
  const s = storage();
  if (!s) return [];
  const idx = readIndex(s) ?? rebuildIndex(s);
  return Object.entries(idx)
    .map(([slotId, v]) => ({ slotId, kind: slotKind(slotId), header: v.header, sizeBytes: v.sizeBytes }))
    .sort((a, b) => b.header.savedAt - a.header.savedAt);
}

/** Read and validate one slot. Throws SaveError ('missing' | 'corrupt' | 'unsupported-version'). */
export function readSlot(slotId: string): SaveGameV1 {
  const s = storage();
  if (!s) throw new SaveError('missing', 'Save storage is unavailable in this browser');
  const raw = s.getItem(PREFIX + slotId);
  if (!raw) throw new SaveError('missing', 'This save slot is empty');
  return parseSave(raw);
}

function isQuotaError(e: unknown): boolean {
  return e instanceof DOMException &&
    (e.name === 'QuotaExceededError' || e.name === 'NS_ERROR_DOM_QUOTA_REACHED');
}

/** Write a blob into a slot. Never throws — storage failures come back as results. */
export function writeSlot(slotId: string, save: SaveGameV1): WriteResult {
  const s = storage();
  if (!s) return { ok: false, error: 'storage-unavailable', message: 'Save storage is unavailable in this browser.' };
  const raw = JSON.stringify(save);
  try {
    s.setItem(PREFIX + slotId, raw); // blob first, index second (crash-safe)
  } catch (e) {
    if (isQuotaError(e)) return { ok: false, error: 'quota', message: 'Storage full — delete a save slot and try again.' };
    return { ok: false, error: 'unknown', message: 'Saving failed unexpectedly.' };
  }
  const idx = readIndex(s) ?? rebuildIndex(s);
  idx[slotId] = { header: save.header, sizeBytes: raw.length };
  try {
    s.setItem(INDEX_KEY, JSON.stringify(idx));
  } catch {
    // blob is safe; a stale index self-heals on the next list
  }
  return { ok: true, slotId };
}

/** Delete a slot. Idempotent; silent when storage is unavailable. */
export function deleteSlot(slotId: string): void {
  const s = storage();
  if (!s) return;
  s.removeItem(PREFIX + slotId);
  const idx = readIndex(s);
  if (idx && slotId in idx) {
    delete idx[slotId];
    try {
      s.setItem(INDEX_KEY, JSON.stringify(idx));
    } catch {
      // self-heals on next list
    }
  }
}

/** Rotate the save into the autosave slot that is empty or oldest. */
export function writeAutosave(save: SaveGameV1): WriteResult {
  const slots = listSlots();
  let target: string = AUTOSAVE_SLOTS[0];
  let oldest = Infinity;
  for (const id of AUTOSAVE_SLOTS) {
    const existing = slots.find(m => m.slotId === id);
    if (!existing) { target = id; break; }
    if (existing.header.savedAt < oldest) { oldest = existing.header.savedAt; target = id; }
  }
  return writeSlot(target, save);
}

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'republic';
}

/** Download a save as a .json file via a transient anchor. */
export function exportSave(save: SaveGameV1): void {
  const blob = new Blob([JSON.stringify(save)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `red-republic-${slug(save.header.name)}-y${save.header.year}m${save.header.month}.json`;
  a.click();
  URL.revokeObjectURL(url);
}

/** Parse and validate an imported .json file. Throws SaveError on anything invalid. */
export async function importSave(file: File): Promise<SaveGameV1> {
  let text: string;
  try {
    text = await file.text();
  } catch {
    throw new SaveError('corrupt', 'Could not read the selected file');
  }
  return parseSave(text);
}
