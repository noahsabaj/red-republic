import { GameEngine } from '../engine';
import { BALANCE } from '../config';
import type { ResourceId } from '../config';
import type { MapData, Tile } from '../mapgen';
import { DEFAULT_MAP_W, DEFAULT_MAP_H } from '../mapgen';
import type { DayWeather } from '../weather';

/** Calm weather for deterministic tests: no slowdowns, no frost, no heating need. */
export const CALM_WEATHER = (): Partial<DayWeather> =>
  ({ tempC: 15, condition: 'clear', snowDepth: 0, riverFrozen: false });

/** All-grass map with no water/forest/deposits — deterministic test terrain. */
export function flatMap(w = DEFAULT_MAP_W, h = DEFAULT_MAP_H): MapData {
  const tiles: Tile[][] = [];
  for (let y = 0; y < h; y++) {
    const row: Tile[] = [];
    for (let x = 0; x < w; x++) row.push({ terrain: 'grass', variant: 0.5 });
    tiles.push(row);
  }
  return { tiles, startX: Math.floor(w / 2), startY: Math.floor(h / 2) };
}

/** Flat map with a western national border strip — opts border tests into the border rules. */
export function flatBorderMap(): MapData {
  const map = flatMap();
  for (let y = 0; y < map.tiles.length; y++) {
    for (let x = 0; x < BALANCE.borderDepth; x++) map.tiles[y][x].foreign = true;
  }
  return { ...map, border: 'W', crossX: BALANCE.borderDepth, crossY: Math.floor(map.tiles.length / 2) };
}

/** Engine on a flat map, calm weather, no pre-seeded starting base (unless asked for). */
export function makeEngine(opts: { withBase?: boolean; weather?: (dayIndex: number) => Partial<DayWeather> } = {}): GameEngine {
  return new GameEngine({
    seed: 1, map: flatMap(), skipStartingBase: !opts.withBase,
    weatherScript: opts.weather ?? CALM_WEATHER,
  });
}

/** Instant-build a constructed building, throwing on invalid placement. */
export function placeBuilt(e: GameEngine, defId: string, x: number, y: number) {
  e.dollars = 1e9;
  const res = e.tryPlace(defId, x, y, { instant: true });
  if (!res.ok) throw new Error(`placeBuilt ${defId}@${x},${y}: ${res.reason}`);
  return e.buildingAt(x, y)!;
}

/** Paint a road rectangle directly (no cost, no stats). */
export function layRoad(e: GameEngine, x0: number, y0: number, x1: number, y1: number) {
  for (let y = Math.min(y0, y1); y <= Math.max(y0, y1); y++)
    for (let x = Math.min(x0, x1); x <= Math.max(x0, x1); x++)
      e.tiles[y][x].road = true;
}

export function runDays(e: GameEngine, n: number) {
  e.setSpeed(1);
  for (let i = 0; i < n; i++) e.advance(e.TICK_MS);
}

/** Total of a resource across all buildings AND in-transit trucks. */
export function totalOf(e: GameEngine, r: ResourceId): number {
  let total = 0;
  for (const b of e.buildings.values()) total += b.stock[r] ?? 0;
  for (const t of e.trucks) if (t.cargo === r) total += t.amount;
  return total;
}
