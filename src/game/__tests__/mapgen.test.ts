import { describe, expect, it } from 'vitest';
import { generateMap, MAP_H, MAP_W } from '../mapgen';

describe('generateMap', () => {
  it('is deterministic for a given seed', () => {
    const a = generateMap(1961);
    const b = generateMap(1961);
    expect(a.tiles).toEqual(b.tiles);
    expect([a.startX, a.startY]).toEqual([b.startX, b.startY]);
  });

  it('produces different maps for different seeds', () => {
    const a = generateMap(1);
    const b = generateMap(2);
    expect(a.tiles).not.toEqual(b.tiles);
  });

  it('generates one orthogonally-connected river in the west margin', () => {
    for (const seed of [1, 42, 1961, 987654]) {
      const m = generateMap(seed);
      const water: { x: number; y: number }[] = [];
      const isWater = new Set<number>();
      for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
        if (m.tiles[y][x].terrain === 'water') { water.push({ x, y }); isWater.add(y * MAP_W + x); }
      }
      // a real channel: at least 2 tiles per row, confined to the west side
      expect(water.length).toBeGreaterThanOrEqual(MAP_H * 2);
      expect(water.every(t => t.x <= 9)).toBe(true);
      for (let y = 0; y < MAP_H; y++) {
        const rowW = water.filter(t => t.y === y).length;
        expect(rowW).toBeGreaterThanOrEqual(2);
        expect(rowW).toBeLessThanOrEqual(6);
      }
      // fully 4-connected — no diagonal-only steps or orphan puddles
      const queue = [water[0].y * MAP_W + water[0].x];
      const seen = new Set(queue);
      while (queue.length) {
        const k = queue.pop()!;
        const x = k % MAP_W, y = Math.floor(k / MAP_W);
        for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
          const nk = (y + dy) * MAP_W + (x + dx);
          if (isWater.has(nk) && !seen.has(nk)) { seen.add(nk); queue.push(nk); }
        }
      }
      expect(seen.size).toBe(water.length);
    }
  });

  it('always yields a full-size grid with every deposit type', () => {
    for (const seed of [1, 42, 1961, 999999]) {
      const m = generateMap(seed);
      expect(m.tiles.length).toBe(MAP_H);
      expect(m.tiles[0].length).toBe(MAP_W);
      const kinds = new Set<string>();
      for (const row of m.tiles) for (const t of row) if (t.deposit) kinds.add(t.deposit);
      expect([...kinds].sort()).toEqual(['coal', 'gravel', 'ironOre', 'oil']);
    }
  });
});
