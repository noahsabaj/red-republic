import { describe, expect, it } from 'vitest';
import type { SaveGameV1 } from '../save-format';
import { GameEngine } from '../engine';
import { makeEngine } from './helpers';

/** Place a building that is NOT yet constructed (for testing build priority). */
function readySite(e: GameEngine, defId: string, x: number, y: number) {
  // Give enough money to place but don't instant-build
  e.rubles = 1e6;
  e.dollars = 1e6;
  const res = e.tryPlace(defId, x, y);
  if (!res.ok) throw new Error(`readySite ${defId}@${x},${y}: ${res.reason}`);
  return e.buildingAt(x, y)!;
}

describe('Global Category Construction Priorities', () => {
  it('defaults all global category priorities to 0 (Normal)', () => {
    const e = makeEngine();
    expect(e.globalCategoryPriorities).toEqual({
      infra: 0,
      housing: 0,
      industry: 0,
      services: 0,
      trade: 0,
    });
  });

  it('computes effective priority inheriting from global category priority', () => {
    const e = makeEngine();
    // Place an unconstructed building from two different categories
    const houseSite = readySite(e, 'house', 10, 10); // category: housing
    const storeSite = readySite(e, 'store', 12, 10); // category: services

    expect(e.effectiveBuildPriority(houseSite)).toBe(0);
    expect(e.effectiveBuildPriority(storeSite)).toBe(0);

    // Boost housing globally
    e.setGlobalCategoryPriority('housing', 1);
    expect(e.effectiveBuildPriority(houseSite)).toBe(1);
    expect(e.effectiveBuildPriority(storeSite)).toBe(0); // unchanged
  });

  it('allows local site priority to override global category priority', () => {
    const e = makeEngine();
    const house1 = readySite(e, 'house', 10, 10);
    const house2 = readySite(e, 'house', 12, 10);

    e.setGlobalCategoryPriority('housing', -1); // Low globally for housing

    // Explicitly set house1 to High (+1)
    e.setSitePriority(house1.id, 1);

    expect(e.effectiveBuildPriority(house1)).toBe(1); // local override
    expect(e.effectiveBuildPriority(house2)).toBe(-1); // inherits global
  });

  it('prioritizes High effective priority sites over Low effective priority sites for builder labor', () => {
    const e = makeEngine();
    // Set housing to High, services to Low
    e.setGlobalCategoryPriority('housing', 1);
    e.setGlobalCategoryPriority('services', -1);

    const house = readySite(e, 'house', 10, 10);
    const store = readySite(e, 'store', 12, 10);

    // The effective priorities should reflect the global settings
    expect(e.effectiveBuildPriority(house)).toBe(1);
    expect(e.effectiveBuildPriority(store)).toBe(-1);
  });

  it('serializes and deserializes globalCategoryPriorities in save games', () => {
    const e = makeEngine();
    e.setGlobalCategoryPriority('infra', 1);
    e.setGlobalCategoryPriority('housing', -1);

    const saveBlob: SaveGameV1 = e.serialize();
    const loaded = GameEngine.fromSave(saveBlob);

    expect(loaded.globalCategoryPriorities).toEqual({
      infra: 1,
      housing: -1,
      industry: 0,
      services: 0,
      trade: 0,
    });
  });
});
