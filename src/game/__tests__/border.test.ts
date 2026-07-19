import { describe, expect, it } from 'vitest';
import { generateMap, DEFAULT_MAP_W as MAP_W, DEFAULT_MAP_H as MAP_H } from '../mapgen';
import type { BorderEdge, MapData } from '../mapgen';
import { GameEngine } from '../engine';
import { BALANCE } from '../config';
import { CALM_WEATHER, flatBorderMap, makeEngine } from './helpers';

const D = BALANCE.borderDepth;

/** Distance of (x, y) from the border edge (v-coordinate; 0 = outermost). */
function vOf(border: BorderEdge, x: number, y: number): number {
  return border === 'W' ? x : border === 'E' ? MAP_W - 1 - x : border === 'N' ? y : MAP_H - 1 - y;
}

describe('national border generation', () => {
  it('is deterministic per seed, including edge, spawn and crossing', () => {
    const a = generateMap(7);
    const b = generateMap(7);
    expect(a.border).toBe(b.border);
    expect([a.startX, a.startY, a.crossX, a.crossY]).toEqual([b.startX, b.startY, b.crossX, b.crossY]);
    for (let y = 0; y < MAP_H; y++) {
      for (let x = 0; x < MAP_W; x++) {
        expect(a.tiles[y][x].terrain).toBe(b.tiles[y][x].terrain);
        expect(!!a.tiles[y][x].foreign).toBe(!!b.tiles[y][x].foreign);
      }
    }
  });

  for (const seed of [1, 7, 42, 1961]) {
    it(`seed ${seed}: exact foreign strip, deposit-free, border-town spawn, dry crossing`, () => {
      const m = generateMap(seed) as Required<MapData>;
      expect(m.border).toBeDefined();
      for (let y = 0; y < MAP_H; y++) {
        for (let x = 0; x < MAP_W; x++) {
          const t = m.tiles[y][x];
          expect(!!t.foreign).toBe(vOf(m.border, x, y) < D);
          if (t.foreign) expect(t.deposit).toBeUndefined();
        }
      }
      // the town spawns a fixed short walk inside the border
      expect(vOf(m.border, m.startX, m.startY)).toBe(D + 8);
      // the 2x2 crossing site is buildable land hugging the strip
      const vs: number[] = [];
      for (let dy = 0; dy < 2; dy++) {
        for (let dx = 0; dx < 2; dx++) {
          const t = m.tiles[m.crossY + dy][m.crossX + dx];
          expect(t.terrain).not.toBe('water');
          expect(t.foreign).toBeUndefined();
          vs.push(vOf(m.border, m.crossX + dx, m.crossY + dy));
        }
      }
      expect(Math.min(...vs)).toBe(D); // touches the strip
    });
  }
});

describe('starting base at the border', () => {
  for (const seed of [1, 7, 42, 1961]) {
    it(`seed ${seed}: customs at the crossing, lane to the map edge, road link to the depot`, () => {
      const e = new GameEngine({ seed, weatherScript: CALM_WEATHER });
      const customs = [...e.buildings.values()].find(b => b.defId === 'customs')!;
      const depot = [...e.buildings.values()].find(b => b.defId === 'depot')!;
      expect(customs.constructed).toBe(true);
      // hugs the strip: some orthogonally adjacent tile is foreign
      let touchesBorder = false;
      for (let dy = -1; dy <= customs.h; dy++) {
        for (let dx = -1; dx <= customs.w; dx++) {
          const onEdge = dx === -1 || dx === customs.w || dy === -1 || dy === customs.h;
          const onCorner = (dx === -1 || dx === customs.w) && (dy === -1 || dy === customs.h);
          if (!onEdge || onCorner) continue;
          if (e.tiles[customs.y + dy]?.[customs.x + dx]?.foreign) touchesBorder = true;
        }
      }
      expect(touchesBorder).toBe(true);
      // crossing lane: road tiles through the strip all the way to the map edge
      const edge = e.borderEdge!;
      const lane: { x: number; y: number }[] = [];
      if (edge === 'W') for (let x = 0; x < customs.x; x++) lane.push({ x, y: customs.y });
      if (edge === 'E') for (let x = customs.x + 2; x < MAP_W; x++) lane.push({ x, y: customs.y });
      if (edge === 'N') for (let y = 0; y < customs.y; y++) lane.push({ x: customs.x, y });
      if (edge === 'S') for (let y = customs.y + 2; y < MAP_H; y++) lane.push({ x: customs.x, y });
      expect(lane.length).toBeGreaterThan(0);
      for (const p of lane) expect(e.tiles[p.y][p.x].road).toBe(true);
      // the domestic road network links customs and depot
      expect(e.findPath(e.adjacentRoads(customs), e.adjacentRoads(depot))).not.toBeNull();
    });
  }
});

describe('border placement rules', () => {
  it('rejects buildings and roads on foreign soil and customs away from the border', () => {
    const e = new GameEngine({ map: flatBorderMap(), skipStartingBase: true, weatherScript: CALM_WEATHER });
    expect(e.canPlace('house', 0, 10).reason).toBe('Beyond the state border');
    expect(e.canPlace('road', 1, 10).reason).toBe('Beyond the state border');
    expect(e.canPlace('customs', 10, 10).reason).toBe('A Customs House must stand at the national border');
    expect(e.canPlace('customs', D, 10).ok).toBe(true); // hugging the strip
    expect(e.canPlace('house', 10, 10).ok).toBe(true);  // homeland is unrestricted
  });

  it('keeps borderless test maps unrestricted (whole existing suite unaffected)', () => {
    const e = makeEngine();
    expect(e.borderEdge).toBeNull();
    expect(e.canPlace('customs', 10, 10).ok).toBe(true);
  });

  it('foreign soil cannot be bulldozed (the crossing lane is safe)', () => {
    const e = new GameEngine({ map: flatBorderMap(), skipStartingBase: true, weatherScript: CALM_WEATHER });
    e.tiles[10][0].road = true; // engine-laid lane
    expect(e.bulldozeAt(0, 10)).toBe(false);
    expect(e.tiles[10][0].road).toBe(true);
  });
});
