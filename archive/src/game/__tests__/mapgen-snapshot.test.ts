import { describe, expect, it } from 'vitest';
import { generateMap } from '../mapgen';

// ============================================================
// Determinism tripwire: the 48x48 output of generateMap for these seeds is
// pinned forever. `?seed=N` reproducibility is a player-facing promise, so any
// refactor of mapgen (variable sizes, new features) must keep the default-size
// generation byte-identical — same rnd() call order, same literals. If this
// test fails, the change altered existing maps: fix the change, don't repin.
// ============================================================

function fnv1a(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function mapHash(seed: number): number {
  const m = generateMap(seed);
  const parts: string[] = [
    `${m.startX},${m.startY},${m.border},${m.crossX},${m.crossY}`,
  ];
  for (const row of m.tiles) {
    for (const t of row) {
      parts.push(`${t.terrain}|${t.deposit ?? ''}|${t.foreign ? 1 : 0}|${Math.round(t.variant * 1e6)}`);
    }
  }
  return fnv1a(parts.join(';'));
}

const PINNED: Record<number, number> = {
  1961: 0x0d45b8ba,
  1: 0xe834e06b,
  7: 0x743cb1e8,
  42: 0xf3c9d19a,
  99999: 0x25314b13,
};

describe('mapgen snapshot (48x48 byte-identity across refactors)', () => {
  for (const [seed, hash] of Object.entries(PINNED)) {
    it(`seed ${seed} generates the pinned map`, () => {
      expect(mapHash(Number(seed))).toBe(hash);
    });
  }
});
