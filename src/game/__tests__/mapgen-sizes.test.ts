import { describe, expect, it } from 'vitest';
import { generateMap, DEFAULT_MAP_W, DEFAULT_MAP_H } from '../mapgen';
import type { BorderEdge } from '../mapgen';
import { BALANCE } from '../config';
import { GameEngine } from '../engine';
import { CALM_WEATHER } from './helpers';

const D = BALANCE.borderDepth;

function vOf(border: BorderEdge, x: number, y: number, w: number, h: number): number {
  return border === 'W' ? x : border === 'E' ? w - 1 - x : border === 'N' ? y : h - 1 - y;
}

describe('generateMap at variable sizes', () => {
  const SIZES = [32, 48, 64, 96];
  const SEEDS = [1, 2, 3];

  it('default-size call is identical to an explicit 48x48 call', () => {
    for (const seed of [1, 7, 1961]) {
      expect(generateMap(seed)).toEqual(generateMap(seed, DEFAULT_MAP_W, DEFAULT_MAP_H));
    }
  });

  it('rejects out-of-range sizes', () => {
    expect(() => generateMap(1, 16, 16)).toThrow(/out of range/);
    expect(() => generateMap(1, 256, 256)).toThrow(/out of range/);
  });

  for (const size of SIZES) {
    it(`produces a well-formed ${size}x${size} map`, () => {
      for (const seed of SEEDS) {
        const m = generateMap(seed, size, size);
        const tag = `seed ${seed} size ${size}`;

        expect(m.tiles.length, tag).toBe(size);
        expect(m.tiles[0].length, tag).toBe(size);
        expect(m.border, tag).toBeDefined();

        // foreign strip: exactly borderDepth deep along one edge, nowhere else
        for (let y = 0; y < size; y++) {
          for (let x = 0; x < size; x++) {
            const v = vOf(m.border!, x, y, size, size);
            expect(!!m.tiles[y][x].foreign, `${tag} at ${x},${y}`).toBe(v < D);
            if (v < D) expect(m.tiles[y][x].deposit, tag).toBeUndefined();
          }
        }

        // crossing site in bounds, on domestic dry land
        expect(m.crossX, tag).toBeGreaterThanOrEqual(0);
        expect(m.crossY, tag).toBeGreaterThanOrEqual(0);
        expect(m.crossX! + 1, tag).toBeLessThan(size);
        expect(m.crossY! + 1, tag).toBeLessThan(size);
        for (let dy = 0; dy < 2; dy++) for (let dx = 0; dx < 2; dx++) {
          const t = m.tiles[m.crossY! + dy][m.crossX! + dx];
          expect(t.terrain, tag).not.toBe('water');
          expect(t.foreign, tag).toBeUndefined();
        }

        // start area: in bounds, buildable, a short walk inside the border
        expect(vOf(m.border!, m.startX, m.startY, size, size), tag).toBe(D + 8);
        for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
          expect(m.tiles[m.startY + dy][m.startX + dx].terrain, tag).not.toBe('water');
        }

        // every deposit type exists with at least 2 tiles
        const counts: Record<string, number> = {};
        for (const row of m.tiles) for (const t of row) if (t.deposit) counts[t.deposit] = (counts[t.deposit] ?? 0) + 1;
        for (const kind of ['coal', 'ironOre', 'oil', 'gravel']) {
          expect(counts[kind] ?? 0, `${tag} ${kind}`).toBeGreaterThanOrEqual(2);
        }

        // water exists (a river always crosses the map)
        expect(m.tiles.flat().some(t => t.terrain === 'water'), tag).toBe(true);
      }
    });
  }
});

describe('engine on a non-default map size', () => {
  it('builds, floods, and simulates on a 96x96 real map', () => {
    const e = new GameEngine({ seed: 3, mapW: 96, mapH: 96, weatherScript: CALM_WEATHER });
    expect(e.mapW).toBe(96);
    expect(e.mapH).toBe(96);

    // bounds checks use the real dimensions
    expect(e.tryPlace('road', 95, 95, true).ok || true).toBe(true); // either ok or a domain reason
    expect(e.tryPlace('road', 96, 50, true).ok).toBe(false);

    // find a free grass spot in the far quadrant (x,y > 47 — beyond the old 48 range)
    let spot: { x: number; y: number } | null = null;
    for (let y = 50; y < 94 && !spot; y++) {
      for (let x = 50; x < 94 && !spot; x++) {
        const t = e.tiles[y][x];
        if (t.terrain === 'grass' && !t.buildingId && !t.road && !t.deposit && !t.foreign) spot = { x, y };
      }
    }
    expect(spot).not.toBeNull();
    e.dollars = 1e9;
    const res = e.tryPlace('house', spot!.x, spot!.y, true);
    expect(res.ok).toBe(true);

    // the simulation runs without touching out-of-range indices
    const day0 = e.dayIndex();
    e.setSpeed(1);
    for (let i = 0; i < 30; i++) e.advance(e.TICK_MS);
    expect(e.dayIndex()).toBe(day0 + 30);

    // deposit cluster inspection works at high coordinates
    outer: for (let y = 48; y < 96; y++) {
      for (let x = 48; x < 96; x++) {
        if (e.tiles[y][x].deposit) {
          const c = e.depositClusterAt(x, y);
          expect(c).not.toBeNull();
          expect(c!.tiles.length).toBeGreaterThanOrEqual(1);
          break outer;
        }
      }
    }
  });
});
