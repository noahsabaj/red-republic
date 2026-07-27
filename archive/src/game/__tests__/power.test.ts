import { describe, expect, it } from 'vitest';
import { BUILDINGS, OBJECTIVES, POWER_SECTORS } from '../config';
import { GameEngine } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * What a building does when the grid cannot feed it is authored per building
 * (`unpoweredEff` in config), not decided by a list of ids in the engine — and
 * whatever it does to OUTPUT, it must never zero out DEMAND, or the building
 * can never be resupplied and the outage becomes permanent.
 */
describe('unpowered behaviour is config, not a code list', () => {
  it('a mill authored unpoweredEff 0 stops dead; an extractor browns out', () => {
    expect(BUILDINGS.steelMill.unpoweredEff).toBe(0);
    expect(BUILDINGS.refinery.unpoweredEff).toBe(0);
    expect(BUILDINGS.machineWorks.unpoweredEff).toBe(0);
    // extractors have no override — they take the default brownout, like the mines
    expect(BUILDINGS.oilPump.unpoweredEff).toBeUndefined();
    expect(BUILDINGS.coalMine.unpoweredEff).toBeUndefined();
  });

  it('an unpowered steel mill produces nothing but is still resupplied', () => {
    // The deadlock: baseEff 0 → outputMultiplier 0 → nominalInputRate 0 →
    // cover Infinity → score 0. The mill reported needing nothing, so nothing
    // was ever sent, so its bins were still empty when power came back.
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const mill = placeBuilt(e, 'steelMill', 20, 10);
    placeBuilt(e, 'apartment', 27, 10);
    e.pop = 40;
    depot.stock.coal = 400;
    depot.stock.ironOre = 400;
    mill.stock.coal = 0;
    mill.stock.ironOre = 0;

    runDays(e, 20);
    expect(mill.powered).toBe(false);
    expect(e.productionRates(mill).outputs.steel ?? 0).toBe(0); // stalled, as authored
    expect(e.nominalInputRate(mill, 'coal')).toBeGreaterThan(0); // …but it still WANTS coal
    expect(mill.stock.coal ?? 0).toBeGreaterThan(0);            // …and the coal arrived
    expect(mill.stock.ironOre ?? 0).toBeGreaterThan(0);
  });

  it('the mill runs the moment the grid reaches it, on bins already full', () => {
    const e = makeEngine();
    layRoad(e, 2, 9, 30, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const mill = placeBuilt(e, 'steelMill', 20, 10);
    placeBuilt(e, 'apartment', 27, 10);
    e.pop = 40;
    depot.stock.coal = 400;
    depot.stock.ironOre = 400;
    runDays(e, 20);
    expect(e.stats.produced.steel).toBe(0);

    const plant = placeBuilt(e, 'powerPlant', 2, 10);
    plant.stock.coal = 200;
    runDays(e, 3);
    expect(mill.powered).toBe(true);
    expect(e.stats.produced.steel).toBeGreaterThan(0);
  });
});

/**
 * Who goes dark in a brownout is the PLAYER's decision, not the engine's — it
 * is their republic. The engine only orders buildings inside a sector.
 */
describe('the power grid sector order', () => {
  /**
   * A town drawing 12 MW off a grid holding ~8 — one building in each of the
   * four sectors, so the queue alone decides who misses out.
   *
   * The deficit is pinned by the plant's coal rather than its crew: staffing
   * drifts as the population grows, and the coal on hand is reset before every
   * measured day, so the shortfall is identical in each arm of the comparison.
   */
  const THROTTLE_COAL = 0.32; // 4 coal/day nameplate × 8/100 MW ⇒ ~8 MW out

  function brownoutTown() {
    const e = makeEngine();
    layRoad(e, 4, 9, 34, 9);
    placeBuilt(e, 'depot', 5, 10);          // trade     — 1 MW
    const plant = placeBuilt(e, 'powerPlant', 8, 10);
    plant.stock.coal = 400;
    placeBuilt(e, 'store', 12, 10);         // services  — 1 MW
    placeBuilt(e, 'foodFactory', 14, 10);   // industry  — 4 MW
    const apartment = placeBuilt(e, 'apartment', 20, 10); // housing — 3 MW each
    placeBuilt(e, 'apartment', 24, 10);
    e.pop = 60;                             // enough hands to crew everything
    runDays(e, 3);
    const brownout = () => { plant.stock.coal = THROTTLE_COAL; runDays(e, 1); };
    brownout();
    return { e, apartment, plant, brownout };
  }

  it('starts on the authored plan and round-trips through a save', () => {
    const e = makeEngine();
    expect(e.powerSectorOrder).toEqual([...POWER_SECTORS]);
    e.movePowerSector('housing', -1);
    const moved = [...e.powerSectorOrder];
    expect(moved).not.toEqual([...POWER_SECTORS]);
    expect(GameEngine.fromSave(e.serialize()).powerSectorOrder).toEqual(moved);
  });

  it('refuses an order that is not a full permutation, so nothing falls off the grid', () => {
    const e = makeEngine();
    const before = [...e.powerSectorOrder];
    e.setPowerSectorOrder(['housing']);                       // too short
    expect(e.powerSectorOrder).toEqual(before);
    e.setPowerSectorOrder(['housing', 'housing', 'trade', 'services']); // duplicate
    expect(e.powerSectorOrder).toEqual(before);
    e.setPowerSectorOrder(['housing', 'services', 'industry', 'trade']); // valid
    expect(e.powerSectorOrder).toEqual(['housing', 'services', 'industry', 'trade']);
  });

  it('moving housing to the front keeps the flats lit and darkens industry instead', () => {
    const dflt = brownoutTown();
    expect(dflt.e.powerProduced).toBeLessThan(dflt.e.powerDemand);
    expect(dflt.apartment.powered).toBe(false); // households last on the default plan

    const first = brownoutTown();
    first.e.setPowerSectorOrder(['housing', 'services', 'industry', 'trade']);
    first.brownout();
    expect(first.apartment.powered).toBe(true);
    // and the sector that yielded the power is visibly darker
    const industryDark = (g: ReturnType<typeof first.e.powerGridStatus>) =>
      g.sectors.find(s => s.id === 'industry')!.dark;
    expect(industryDark(first.e.powerGridStatus()))
      .toBeGreaterThanOrEqual(industryDark(dflt.e.powerGridStatus()));
  });

  it('a building flagged Priority staffing jumps the sector order entirely', () => {
    const { e, apartment, brownout } = brownoutTown();
    expect(apartment.powered).toBe(false);
    e.toggleStaffPriority(apartment.id);
    brownout();
    expect(apartment.powered).toBe(true);
  });

  it('powerGridStatus reports what each sector draws and how much it got', () => {
    const { e } = brownoutTown();
    const g = e.powerGridStatus();
    expect(g.sectors.map(s => s.id)).toEqual(e.powerSectorOrder);
    expect(g.deficit).toBeGreaterThan(0);
    for (const s of g.sectors) {
      expect(s.served).toBeLessThanOrEqual(s.draw + 1e-9);
      expect(s.dark).toBeLessThanOrEqual(s.buildings);
    }
    // the panel's headline figures agree with the engine's own totals
    const drawn = g.sectors.reduce((n, s) => n + s.draw, 0);
    expect(drawn).toBeCloseTo(e.powerDemand, 6);
  });
});

describe('the Electrification objective', () => {
  const target = 50; // what OBJECTIVES advertises

  it('advertises the threshold the engine actually enforces', () => {
    const def = OBJECTIVES.find(o => o.id === 'power')!;
    expect(def.description).toContain(String(target));
  });

  it('does not fire on a plant limping below the advertised figure', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 20, 9);
    placeBuilt(e, 'depot', 5, 10);
    const plant = placeBuilt(e, 'powerPlant', 10, 10);
    placeBuilt(e, 'apartment', 17, 10);
    e.pop = 6;               // a skeleton crew — well under 50 MW of output
    plant.stock.coal = 200;
    runDays(e, 3);
    expect(e.powerProduced).toBeGreaterThan(0);
    expect(e.powerProduced).toBeLessThan(target);
    expect(e.objectivesDone).not.toContain('power');
  });

  it('fires once the grid genuinely reaches it', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    const plant = placeBuilt(e, 'powerPlant', 10, 10);
    placeBuilt(e, 'apartment', 20, 10);
    placeBuilt(e, 'apartment', 24, 10);
    e.pop = 80;
    plant.stock.coal = 400;
    runDays(e, 4);
    expect(e.powerProduced).toBeGreaterThanOrEqual(target);
    expect(e.objectivesDone).toContain('power');
  });
});
