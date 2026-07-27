import { describe, expect, it } from 'vitest';
import { BALANCE, BUILDINGS } from '../config';
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
  /** Cold town: depot + staffed plant + one apartment (heat 2) at a given outdoor temp. */
  const heatTown = (tempC: number) => {
    const e = makeEngine({ weather: () => ({ tempC, condition: 'clear' as const, snowDepth: 0, riverFrozen: false }) });
    layRoad(e, 4, 9, 15, 9);
    placeBuilt(e, 'depot', 5, 10);
    const plant = placeBuilt(e, 'heatingPlant', 10, 10);
    const flat = placeBuilt(e, 'apartment', 12, 10);
    plant.stock.coal = 20;
    e.pop = 20;
    return { e, plant, flat };
  };

  it('throttles to temperature-scaled demand; coal burn matches actual output', () => {
    const { e, plant, flat } = heatTown(BALANCE.heatDesignTempC); // demand factor = 1
    const before = plant.stock.coal!;
    runDays(e, 1);
    const burned = before - plant.stock.coal!;

    // unpowered (no power plant): eff = 1 (staff) * 0.5; capacity = 8 * 0.5 = 4
    expect(plant.eff).toBeCloseTo(0.5, 9);
    expect(e.heatDemand).toBeCloseTo(2, 9);                    // apartment heat * factor 1
    expect(e.heatProduced).toBeCloseTo(2, 9);                  // throttled to demand, not capacity
    expect(e.heatProduced / 8).toBeCloseTo(burned / 1, 9);     // output and fuel agree
    expect(flat.heated).toBe(true);
  });

  it('burns more coal the colder it gets, and none when warm', () => {
    const burnAt = (tempC: number) => {
      const { e, plant } = heatTown(tempC);
      const before = plant.stock.coal!;
      runDays(e, 1);
      return before - plant.stock.coal!;
    };
    const warm = burnAt(15);   // above heatThresholdC — no heating at all
    const mild = burnAt(5);
    const design = burnAt(BALANCE.heatDesignTempC);
    const deep = burnAt(-25);  // demand over-driven past 100%
    expect(warm).toBe(0);
    expect(mild).toBeGreaterThan(0);
    expect(design).toBeGreaterThan(mild);
    expect(deep).toBeGreaterThan(design);
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

describe('togglePause', () => {
  it('resumes at the speed the game was last running at', () => {
    const e = makeEngine();
    e.setSpeed(4);
    e.togglePause();
    expect(e.speed).toBe(0);
    e.togglePause();
    expect(e.speed).toBe(4); // not 1
  });

  it('resumes the last running speed even when paused via setSpeed(0)', () => {
    const e = makeEngine();
    e.setSpeed(2);
    e.setSpeed(0); // HUD pause button
    e.togglePause(); // Space
    expect(e.speed).toBe(2);
  });

  it('defaults to 1x on a fresh game', () => {
    const e = makeEngine();
    e.setSpeed(0); // intro pause
    e.togglePause();
    expect(e.speed).toBe(1);
  });
});

describe('no domestic money', () => {
  it('an empty treasury never touches citizens — no wages exist', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 12, 9);
    placeBuilt(e, 'depot', 5, 10);
    e.pop = 20;
    e.rubles = 0;
    const before = e.happiness;
    runDays(e, 5);
    expect(e.rubles).toBe(0); // nothing domestic charges or pays the treasury
    expect(e.alerts.some(a => a.id === 'wages')).toBe(false);
    expect(e.happiness).toBeGreaterThan(before - 30); // no payroll-crisis spiral
  });
});
