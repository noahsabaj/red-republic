import { describe, expect, it } from 'vitest';
import { generateMap } from '../mapgen';
import { SaveError, packTiles, parseSave, unpackTiles } from '../save-format';
import { makeEngine } from './helpers';

describe('tile codec', () => {
  it('round-trips a generated map (variant to 8-bit precision, idempotent after that)', () => {
    const m = generateMap(42);
    const packed = packTiles(m.tiles);
    const tiles = unpackTiles(packed.tilesPacked, packed.variantsPacked, 48, 48);
    for (let y = 0; y < 48; y++) {
      for (let x = 0; x < 48; x++) {
        const a = m.tiles[y][x], b = tiles[y][x];
        expect(b.terrain).toBe(a.terrain);
        expect(b.deposit).toBe(a.deposit);
        expect(!!b.road).toBe(!!a.road);
        expect(!!b.foreign).toBe(!!a.foreign);
        expect(Math.abs(b.variant - a.variant)).toBeLessThanOrEqual(1 / 255);
      }
    }
    // second pass is byte-stable — quantization is idempotent
    expect(packTiles(tiles)).toEqual(packed);
  });

  it('rejects length mismatches and invalid deposit codes', () => {
    const m = generateMap(1);
    const packed = packTiles(m.tiles);
    expect(() => unpackTiles(packed.tilesPacked, packed.variantsPacked, 96, 96)).toThrow(SaveError);
    // craft a byte with deposit code 7 (invalid)
    const bad = btoa(String.fromCharCode(7 << 2));
    expect(() => unpackTiles(bad, btoa('\0'), 1, 1)).toThrow(/deposit/);
    expect(() => unpackTiles('%%%not-base64%%%', packed.variantsPacked, 48, 48)).toThrow(SaveError);
  });
});

describe('parseSave validation', () => {
  const validJson = () => JSON.stringify(makeEngine({ withBase: true }).serialize());

  /** Parsed-JSON view loose enough to corrupt freely. */
  type LooseSave = {
    header: Record<string, unknown>;
    body: Record<string, unknown> & {
      buildings: Record<string, unknown>[];
      counters: Record<string, number>;
    };
  };
  const loose = () => JSON.parse(validJson()) as LooseSave;

  it('accepts a valid save', () => {
    const save = parseSave(validJson());
    expect(save.header.seed).toBe(1);
  });

  const corrupt = (mutate: (s: LooseSave) => void, pattern?: RegExp) => {
    const s = loose();
    mutate(s);
    try {
      parseSave(JSON.stringify(s));
      expect.unreachable('should have thrown');
    } catch (e) {
      expect(e).toBeInstanceOf(SaveError);
      if (pattern) expect((e as SaveError).message).toMatch(pattern);
      return e as SaveError;
    }
  };

  it('rejects truncated JSON', () => {
    expect(() => parseSave(validJson().slice(0, 100))).toThrow(/valid JSON/);
  });

  it('refuses newer versions without corrupting them', () => {
    const e = corrupt(s => { s.header.formatVersion = 999; });
    expect(e.code).toBe('unsupported-version');
  });

  it('rejects unknown migration sources', () => {
    const e = corrupt(s => { s.header.formatVersion = 0; });
    expect(e.code).toBe('corrupt');
  });

  it('rejects unknown building types', () => {
    corrupt(s => { s.body.buildings[0].defId = 'nuclearPlant'; }, /Unknown building/);
  });

  it('rejects unknown climates and non-numeric headers', () => {
    corrupt(s => { s.header.climate = 'tropical'; }, /climate/);
    corrupt(s => { s.header.rubles = 'lots'; }, /rubles/);
  });

  it('rejects missing bodies and bad map sizes', () => {
    corrupt(s => { delete s.body.tilesPacked; }, /Tile data/);
    corrupt(s => { s.header.mapW = 4; }, /out of range/);
  });

  it('repairs undersized id counters instead of rejecting', () => {
    const s = loose();
    s.body.buildings = [{
      id: 900, defId: 'house', x: 5, y: 5, w: 1, h: 1, constructed: true, progress: 60,
      stock: {}, incoming: {}, staff: 0, eff: 0, powered: false, heated: false,
      connected: false, roadConnected: false, coalFactor: 0, farmFields: 0,
    }];
    s.body.counters.building = 1;
    const save = parseSave(JSON.stringify(s));
    expect(save.body.counters.building).toBe(901);
  });
});
