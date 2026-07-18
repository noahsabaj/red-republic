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

  it('carves varied, well-formed water: a river spanning edges, no puddles, land start', () => {
    for (const seed of [1, 42, 1961, 987654]) {
      const m = generateMap(seed);
      const isWater = new Set<number>();
      for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
        if (m.tiles[y][x].terrain === 'water') isWater.add(y * MAP_W + x);
      }
      expect(isWater.size).toBeGreaterThanOrEqual(40);

      // 4-connected components: none may be a puddle
      const seen = new Set<number>();
      const components: number[][] = [];
      for (const start of isWater) {
        if (seen.has(start)) continue;
        const comp: number[] = [];
        const queue = [start];
        seen.add(start);
        while (queue.length) {
          const k = queue.pop()!;
          comp.push(k);
          const x = k % MAP_W, y = Math.floor(k / MAP_W);
          for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1]]) {
            const nk = (y + dy) * MAP_W + (x + dx);
            if (isWater.has(nk) && !seen.has(nk)) { seen.add(nk); queue.push(nk); }
          }
        }
        components.push(comp);
      }
      for (const comp of components) expect(comp.length, `seed ${seed}`).toBeGreaterThanOrEqual(5);

      // the largest component is the river: it must touch two different map edges
      const river = components.sort((a, b) => b.length - a.length)[0];
      const edges = new Set<string>();
      for (const k of river) {
        const x = k % MAP_W, y = Math.floor(k / MAP_W);
        if (y === 0) edges.add('N');
        if (y === MAP_H - 1) edges.add('S');
        if (x === 0) edges.add('W');
        if (x === MAP_W - 1) edges.add('E');
      }
      expect(edges.size, `seed ${seed} river touches ${[...edges].join()}`).toBeGreaterThanOrEqual(2);

      // guaranteed buildable starting zone
      for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
        expect(m.tiles[m.startY + dy][m.startX + dx].terrain).not.toBe('water');
      }
    }
  });

  it('water geography genuinely differs between seeds', () => {
    const mask = (seed: number) => {
      const s = new Set<number>();
      const m = generateMap(seed);
      for (let y = 0; y < MAP_H; y++) for (let x = 0; x < MAP_W; x++) {
        if (m.tiles[y][x].terrain === 'water') s.add(y * MAP_W + x);
      }
      return s;
    };
    const a = mask(7), b = mask(8);
    let diff = 0;
    for (const k of a) if (!b.has(k)) diff++;
    for (const k of b) if (!a.has(k)) diff++;
    expect(diff).toBeGreaterThanOrEqual(20); // not just a nudge of the same river
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
