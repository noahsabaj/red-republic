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
