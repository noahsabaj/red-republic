import { afterEach, describe, expect, it, vi } from 'vitest';
import { bootFromUrl, createSession, randomRepublicName, randomSeed, sessionFromSave } from '../session';
import { MAP_SIZES } from '@/game/config';

function stubSearch(search: string) {
  vi.stubGlobal('window', { location: { search } });
}

afterEach(() => vi.unstubAllGlobals());

describe('bootFromUrl', () => {
  it('returns null with no relevant params (normal boot → menu)', () => {
    stubSearch('');
    expect(bootFromUrl()).toBeNull();
    stubSearch('?foo=1');
    expect(bootFromUrl()).toBeNull();
  });

  it('?seed=N reproduces that seed on a medium map, skipping the briefing', () => {
    stubSearch('?seed=77');
    const s = bootFromUrl()!;
    expect(s.engine.seed).toBe(77);
    expect(s.engine.mapW).toBe(MAP_SIZES.medium.tiles);
    expect(s.isNew).toBe(false);
    expect(s.config.seed).toBe(77);
  });

  it('?demo pins seed 1961 and seeds the demo town', () => {
    stubSearch('?demo');
    const s = bootFromUrl()!;
    expect(s.engine.seed).toBe(1961);
    expect(s.engine.pop).toBeGreaterThan(0); // the demo town settled citizens
  });

  it('?climate=taiga picks the region; invalid ids fall back', () => {
    stubSearch('?seed=5&climate=taiga');
    expect(bootFromUrl()!.engine.climate).toBe('taiga');
    stubSearch('?seed=5&climate=lunar');
    expect(bootFromUrl()!.engine.climate).toBe('plains');
  });
});

describe('sessions', () => {
  it('createSession forwards the whole config to the engine', () => {
    const s = createSession({ name: 'Testgrad', seed: 9, mapSize: 'small', climate: 'steppe', difficulty: 'hard' }, 3);
    expect(s.id).toBe(3);
    expect(s.isNew).toBe(true);
    expect(s.engine.name).toBe('Testgrad');
    expect(s.engine.seed).toBe(9);
    expect(s.engine.mapW).toBe(32);
    expect(s.engine.climate).toBe('steppe');
    expect(s.engine.difficulty).toBe('hard');
  });

  it('sessionFromSave reconstructs the config from the engine', () => {
    const a = createSession({ name: 'Roundtrip', seed: 11, mapSize: 'large', climate: 'maritime', difficulty: 'easy' }, 1);
    const b = sessionFromSave(a.engine.serialize(), 2);
    expect(b.isNew).toBe(false);
    expect(b.config).toEqual(a.config);
    expect(b.engine.speed).toBe(0); // loads paused
  });
});

describe('ui randomness', () => {
  it('names and seeds are well-formed', () => {
    for (let i = 0; i < 20; i++) {
      const n = randomRepublicName();
      expect(n.length).toBeGreaterThan(4);
      expect(n.length).toBeLessThanOrEqual(24);
      const s = randomSeed();
      expect(Number.isInteger(s)).toBe(true);
      expect(s).toBeGreaterThanOrEqual(0);
    }
  });
});
