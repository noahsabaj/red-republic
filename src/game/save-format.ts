// ============================================================
// Save-game format: a versioned, JSON-serializable snapshot of the
// entire simulation. The engine owns serialize()/fromSave(); this
// module owns the blob shape, the packed tile codec, validation and
// version migration. Nothing here touches localStorage — persistence
// lives in save-slots.ts so the engine stays storage-free.
//
// Compatibility rules:
// - Entity payloads reuse the live engine types, hydrated over a
//   defaults spread — a new field with a sensible default needs NO
//   version bump; anything that can't be defaulted bumps
//   SAVE_FORMAT_VERSION and adds a MIGRATIONS step.
// - Newer-version saves are refused, never down-migrated.
// ============================================================
import { BUILDINGS, CLIMATES, DIFFICULTIES } from './config';
import type { ClimateId, DifficultyId, ResourceId } from './config';
import type { BorderEdge, Tile } from './mapgen';
import type { AutoTradeRule, BuildingInst, Contract, TradeDayLedger, Truck } from './engine';

export const SAVE_FORMAT_VERSION = 1;

/** Everything slot lists need — readable without touching the body. */
export interface SaveHeaderV1 {
  formatVersion: number;
  savedAt: number; // wall clock, Date.now()
  name: string;    // republic name
  label?: string;  // player-chosen slot label (set by the save UI, ignored by fromSave)
  seed: number;
  mapW: number;
  mapH: number;
  climate: ClimateId;
  difficulty: DifficultyId;
  day: number; month: number; year: number;
  pop: number;
  rubles: number;
  dollars: number;
}

export interface SaveBodyV1 {
  borderEdge: BorderEdge | null;
  tilesPacked: string;    // base64, mapW*mapH bytes — see the codec below
  variantsPacked: string; // base64, mapW*mapH bytes, round(variant*255)
  buildings: BuildingInst[]; // tile buildingId stamps are rebuilt from footprints
  trucks: Truck[];
  boats: Truck[];
  foreignTrucks: Truck[];
  boatOrders: { srcId: number; destId: number; r: ResourceId; amt: number }[];
  acc: number;            // sub-tick ms accumulator
  lastRunSpeed: 1 | 2 | 4; // speed itself always loads as 0 (paused)
  rngState: number;       // economy rng position — restores bit-exact price drift
  priceFactorEast: number;
  priceFactorWest: number;
  autoTrade: { enabled: boolean; reserveRubles: number; reserveDollars: number; rules: Partial<Record<ResourceId, AutoTradeRule>> };
  foreignLaborEnabled?: boolean; // optional: old saves default to true
  tradeLedger: { today: TradeDayLedger; yesterday: TradeDayLedger };
  contracts: Contract[];
  relationsPenalty: { east: number; west: number };
  objectivesDone: string[];
  stats: {
    produced: Record<ResourceId, number>;
    /** cumulative customs imports; absent in older saves (hydrates to {}) */
    imported?: Partial<Record<ResourceId, number>>;
    exportedValue: number;
    roadsBuilt: number;
  };
  happiness: number;
  sat: Record<'food' | 'clothes' | 'power' | 'heat' | 'culture' | 'health' | 'employment' | 'pollution', number>;
  streaks: { dry: number; gloom: number; sun: number; wasFrost: boolean };
  counters: { building: number; truck: number; boat: number; contract: number };
  /** Display aggregates so the paused post-load HUD reads correctly before the next tick. */
  aggregates: {
    capacity: number; workers: number; employed: number; jobs: number;
    powerProduced: number; powerDemand: number; heatProduced: number; heatDemand: number;
  };
}

export interface SaveGameV1 {
  header: SaveHeaderV1;
  body: SaveBodyV1;
}

export class SaveError extends Error {
  readonly code: 'corrupt' | 'unsupported-version' | 'missing';
  constructor(code: 'corrupt' | 'unsupported-version' | 'missing', message: string) {
    super(message);
    this.code = code;
    this.name = 'SaveError';
  }
}

// ------------------------------------------------------------
// Tile codec — one byte per tile:
//   bits 0-1 terrain, bits 2-4 deposit (0 = none), bit 5 road, bit 6 foreign.
// Enum orders are pinned HERE, never derived from config tables — reordering
// a config object must not corrupt existing saves.
// ------------------------------------------------------------

const TERRAINS = ['grass', 'forest', 'water', 'rock'] as const;
const DEPOSITS = [undefined, 'coal', 'ironOre', 'oil', 'gravel'] as const;

function bytesToBase64(bytes: Uint8Array): string {
  let bin = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    bin += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(bin);
}

function base64ToBytes(s: string): Uint8Array {
  let bin: string;
  try {
    bin = atob(s);
  } catch {
    throw new SaveError('corrupt', 'Tile data is not valid base64');
  }
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

export function packTiles(tiles: Tile[][]): { tilesPacked: string; variantsPacked: string } {
  const h = tiles.length, w = tiles[0].length;
  const flags = new Uint8Array(w * h);
  const variants = new Uint8Array(w * h);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const t = tiles[y][x];
      const k = y * w + x;
      flags[k] = TERRAINS.indexOf(t.terrain)
        | (DEPOSITS.indexOf(t.deposit) << 2)
        | (t.road ? 0x20 : 0)
        | (t.foreign ? 0x40 : 0);
      variants[k] = Math.round(t.variant * 255);
    }
  }
  return { tilesPacked: bytesToBase64(flags), variantsPacked: bytesToBase64(variants) };
}

export function unpackTiles(tilesPacked: string, variantsPacked: string, w: number, h: number): Tile[][] {
  const flags = base64ToBytes(tilesPacked);
  const variants = base64ToBytes(variantsPacked);
  if (flags.length !== w * h || variants.length !== w * h) {
    throw new SaveError('corrupt', `Tile data length mismatch (expected ${w * h}, got ${flags.length}/${variants.length})`);
  }
  const tiles: Tile[][] = [];
  for (let y = 0; y < h; y++) {
    const row: Tile[] = [];
    for (let x = 0; x < w; x++) {
      const b = flags[y * w + x];
      const depositIdx = (b >> 2) & 0x7;
      if (depositIdx >= DEPOSITS.length) throw new SaveError('corrupt', `Unknown deposit code ${depositIdx}`);
      const t: Tile = {
        terrain: TERRAINS[b & 0x3],
        variant: variants[y * w + x] / 255,
      };
      const deposit = DEPOSITS[depositIdx];
      if (deposit) t.deposit = deposit;
      if (b & 0x20) t.road = true;
      if (b & 0x40) t.foreign = true;
      row.push(t);
    }
    tiles.push(row);
  }
  return tiles;
}

// ------------------------------------------------------------
// Validation & migration
// ------------------------------------------------------------

/** Stepwise migrations: MIGRATIONS[v] upgrades a version-v blob to v+1. */
const MIGRATIONS: Record<number, (old: unknown) => unknown> = {};

function isObj(u: unknown): u is Record<string, unknown> {
  return typeof u === 'object' && u !== null;
}

function num(u: unknown, what: string): number {
  if (typeof u !== 'number' || !Number.isFinite(u)) throw new SaveError('corrupt', `${what} is not a number`);
  return u;
}

/** Structural validation (pragmatic, not exhaustive) + counter repair. Throws SaveError. */
export function validateSave(u: unknown): SaveGameV1 {
  if (!isObj(u) || !isObj(u.header) || !isObj(u.body)) throw new SaveError('corrupt', 'Save is not a header/body object');
  const h = u.header;
  const b = u.body;

  const version = num(h.formatVersion, 'formatVersion');
  if (version !== SAVE_FORMAT_VERSION) throw new SaveError('corrupt', `validateSave expects version ${SAVE_FORMAT_VERSION}, got ${version} (migrate first)`);

  if (typeof h.name !== 'string') throw new SaveError('corrupt', 'Republic name missing');
  num(h.seed, 'seed'); num(h.savedAt, 'savedAt');
  num(h.day, 'day'); num(h.month, 'month'); num(h.year, 'year');
  num(h.pop, 'pop'); num(h.rubles, 'rubles'); num(h.dollars, 'dollars');
  const mapW = num(h.mapW, 'mapW'), mapH = num(h.mapH, 'mapH');
  if (mapW < 16 || mapH < 16 || mapW > 256 || mapH > 256) throw new SaveError('corrupt', `Map size ${mapW}x${mapH} out of range`);
  if (typeof h.climate !== 'string' || !(h.climate in CLIMATES)) throw new SaveError('corrupt', `Unknown climate '${String(h.climate)}'`);
  if (typeof h.difficulty !== 'string' || !(h.difficulty in DIFFICULTIES)) throw new SaveError('corrupt', `Unknown difficulty '${String(h.difficulty)}'`);

  if (typeof b.tilesPacked !== 'string' || typeof b.variantsPacked !== 'string') throw new SaveError('corrupt', 'Tile data missing');
  for (const key of ['buildings', 'trucks', 'boats', 'foreignTrucks', 'boatOrders', 'contracts', 'objectivesDone'] as const) {
    if (!Array.isArray(b[key])) throw new SaveError('corrupt', `${key} is not an array`);
  }
  for (const key of ['autoTrade', 'tradeLedger', 'relationsPenalty', 'stats', 'sat', 'streaks', 'counters', 'aggregates'] as const) {
    if (!isObj(b[key])) throw new SaveError('corrupt', `${key} is missing`);
  }
  num(b.rngState, 'rngState');

  const buildings = b.buildings as BuildingInst[];
  let maxBuildingId = 0;
  for (const inst of buildings) {
    if (!isObj(inst)) throw new SaveError('corrupt', 'Building entry is not an object');
    if (typeof inst.defId !== 'string' || !(inst.defId in BUILDINGS)) {
      throw new SaveError('corrupt', `Unknown building type '${String(inst.defId)}'`);
    }
    num(inst.id, 'building id'); num(inst.x, 'building x'); num(inst.y, 'building y');
    maxBuildingId = Math.max(maxBuildingId, inst.id);
  }

  // counter repair: clamp up so post-load placements can never reuse a live id
  const counters = b.counters as SaveBodyV1['counters'];
  counters.building = Math.max(num(counters.building, 'counters.building'), maxBuildingId + 1);
  const fleetMax = (fleet: Truck[]) => fleet.reduce((m, t) => Math.max(m, isObj(t) ? num(t.id, 'fleet id') : 0), 0);
  counters.truck = Math.max(num(counters.truck, 'counters.truck'), fleetMax(b.trucks as Truck[]) + 1, fleetMax(b.foreignTrucks as Truck[]) + 1);
  counters.boat = Math.max(num(counters.boat, 'counters.boat'), fleetMax(b.boats as Truck[]) + 1);
  counters.contract = Math.max(num(counters.contract, 'counters.contract'),
    (b.contracts as Contract[]).reduce((m, c) => Math.max(m, isObj(c) ? num(c.id, 'contract id') : 0), 0) + 1);

  return u as unknown as SaveGameV1;
}

/** JSON string → validated, migrated SaveGameV1. Throws SaveError, never anything else. */
export function parseSave(json: string): SaveGameV1 {
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    throw new SaveError('corrupt', 'Save file is not valid JSON');
  }
  if (!isObj(raw) || !isObj(raw.header)) throw new SaveError('corrupt', 'Save has no header');
  let version = (raw.header).formatVersion;
  if (typeof version !== 'number') throw new SaveError('corrupt', 'Save has no format version');
  if (version > SAVE_FORMAT_VERSION) {
    throw new SaveError('unsupported-version', 'This save was made by a newer version of the game.');
  }
  while (version < SAVE_FORMAT_VERSION) {
    const step = MIGRATIONS[version];
    if (!step) throw new SaveError('corrupt', `No migration path from save version ${version}`);
    raw = step(raw);
    version++;
  }
  return validateSave(raw);
}
