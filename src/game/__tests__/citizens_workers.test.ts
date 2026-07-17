import { describe, expect, it } from 'vitest';
import { BUILDINGS } from '../config';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

describe('assignWorkers', () => {
  it('never staffs a building past its job count, even with surplus labor', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 20, 9);
    placeBuilt(e, 'depot', 5, 10);      // 4 jobs
    placeBuilt(e, 'sawmill', 10, 10);   // 6 jobs
    placeBuilt(e, 'sawmill', 12, 10);   // 6 jobs
    e.pop = 100; // 70 workers vs 16 jobs — the old code gave sawmills ~29 staff each

    runDays(e, 1);

    for (const b of e.buildings.values()) {
      expect(b.staff).toBeLessThanOrEqual(BUILDINGS[b.defId].workers);
      expect(b.eff).toBeLessThanOrEqual(1);
    }
    expect(e.employed).toBe(16);
  });
});

describe('heating plant', () => {
  it('produces heat and burns coal with one consistent efficiency', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 12, 9);
    placeBuilt(e, 'depot', 5, 10);
    const plant = placeBuilt(e, 'heatingPlant', 10, 10);
    plant.stock.coal = 20;
    e.pop = 20; // fully staffs the plant

    const before = plant.stock.coal!;
    runDays(e, 1);
    const burned = before - plant.stock.coal!;

    // unpowered (no power plant): eff = 1 (staff) * 0.5 (unpowered)
    expect(plant.eff).toBeCloseTo(0.5, 9);
    expect(e.heatProduced).toBeCloseTo(8 * 0.5, 9);            // heatOutput * eff
    expect(burned).toBeCloseTo(1 * 0.5, 9);                    // inputs.coal * eff
    expect(e.heatProduced / 8).toBeCloseTo(burned / 1, 9);     // the invariant itself
  });
});

describe('migration', () => {
  it('does not respawn settlers into a miserable republic', () => {
    const e = makeEngine();
    placeBuilt(e, 'house', 10, 10);
    e.happiness = 10;
    e.sat.food = 0; e.sat.clothes = 0; e.sat.power = 0; e.sat.heat = 0;
    e.sat.culture = 0; e.sat.health = 0; e.sat.employment = 0;

    runDays(e, 10);

    expect(e.happiness).toBeLessThan(48); // precondition still holds
    expect(e.pop).toBe(0);                // old code re-seeded 6 settlers every day
  });

  it('still bootstraps a fresh republic', () => {
    const e = makeEngine();
    placeBuilt(e, 'house', 10, 10);
    runDays(e, 1);
    expect(e.pop).toBe(6);
  });
});

describe('wages', () => {
  it('flags unpaid wages instead of the unreachable debt alert', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 12, 9);
    placeBuilt(e, 'depot', 5, 10);
    e.pop = 20;
    e.rubles = 0;
    runDays(e, 1);
    expect(e.wagesUnpaid).toBe(true);
    expect(e.alerts.some(a => a.id === 'wages')).toBe(true);
    e.rubles = 1000;
    runDays(e, 1);
    expect(e.wagesUnpaid).toBe(false);
  });
});
