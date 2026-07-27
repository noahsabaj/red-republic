import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import type { SaveGameV1 } from '../save-format';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/** Freezing weather so heat demand is live; calm otherwise. */
const WINTER = () => ({ tempC: -20, condition: 'clear' as const, snowDepth: 0, riverFrozen: false });

describe('Delivery dispatch — dials', () => {
  it('starts every category neutral', () => {
    const e = new GameEngine({ seed: 1, skipStartingBase: true });
    expect(e.logisticsCategoryWeights).toEqual({ lifeline: 1, consumer: 1, industry: 1, construction: 1 });
  });

  it('clamps a dial into range and round-trips it through a save', () => {
    const e = new GameEngine({ seed: 1, skipStartingBase: true });
    e.setLogisticsCategoryWeight('construction', 999);
    expect(e.logisticsCategoryWeights.construction).toBe(4);
    e.setLogisticsCategoryWeight('consumer', 2);
    e.toggleEmergencyFuelAutoBuy();

    const save: SaveGameV1 = e.serialize();
    const hydrated = GameEngine.fromSave(save);
    expect(hydrated.logisticsCategoryWeights.construction).toBe(4);
    expect(hydrated.logisticsCategoryWeights.consumer).toBe(2);
    expect(hydrated.emergencyFuelAutoBuy).toBe(false);
  });

  it('seeds dials from a pre-dial save that only had a preset mode', () => {
    const e = new GameEngine({ seed: 1, skipStartingBase: true });
    const save: SaveGameV1 = e.serialize();
    delete (save.body as { logisticsCategoryWeights?: unknown }).logisticsCategoryWeights;
    (save.body as { logisticsPriorityMode?: string }).logisticsPriorityMode = 'construction';

    const hydrated = GameEngine.fromSave(save);
    expect(hydrated.logisticsCategoryWeights.construction).toBeGreaterThan(1);
    expect(hydrated.logisticsCategoryWeights.lifeline).toBe(1);
  });
});

describe('Delivery dispatch — value is downtime prevented', () => {
  /** Staffed town: buildings only demand what they are actually manned to consume. */
  function town(roadTo = 40) {
    const e = makeEngine();
    layRoad(e, 4, 9, roadTo, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    placeBuilt(e, 'apartment', 4, 12);
    placeBuilt(e, 'apartment', 7, 12);
    e.pop = 80;
    return { e, depot };
  }

  it('a far, starving power plant outranks a near, well-stocked factory', () => {
    // The whole reason round-trip cost can divide the score unconditionally:
    // a building that was not going to stall prevents no downtime, however
    // close it is, so no distance ratio can flip a genuine emergency.
    const { e, depot } = town(44);
    const nearFactory = placeBuilt(e, 'foodFactory', 10, 10);
    const farPlant = placeBuilt(e, 'powerPlant', 40, 10);
    runDays(e, 1); // settle staffing

    depot.stock.coal = 200;
    depot.stock.crops = 200;
    farPlant.stock.coal = 0;         // stalls immediately
    farPlant.stock.machinery = 8;
    nearFactory.stock.crops = 40;    // comfortable, but plenty of free bin space
    nearFactory.stock.machinery = 8;

    const next = e.logisticsPriorityPreview().next;
    const plantAt = next.findIndex(n => n.destId === farPlant.id && n.resource === 'coal');
    const factoryAt = next.findIndex(n => n.destId === nearFactory.id && n.resource === 'crops');
    expect(plantAt).toBe(0);
    if (factoryAt >= 0) expect(plantAt).toBeLessThan(factoryAt);
  });

  it('a comfortable bin with lots of free space scores nothing', () => {
    const { e, depot } = town(30);
    const factory = placeBuilt(e, 'foodFactory', 12, 10);
    runDays(e, 1);
    depot.stock.crops = 200;
    factory.stock.machinery = 8;

    factory.stock.crops = 0;
    expect(e.logisticsPriorityPreview().next.find(n => n.destId === factory.id)).toBeDefined();

    // Same bin, now holding far more than the horizon could consume.
    factory.stock.crops = 1000;
    expect(e.logisticsPriorityPreview().next.find(n => n.destId === factory.id)).toBeUndefined();
  });

  it('does not haul an abundant input to a factory bottlenecked on another', () => {
    // A steel mill with no coal cannot use more iron ore — delivering it
    // prevents zero downtime, so it must not win a truck.
    const { e, depot } = town(30);
    const mill = placeBuilt(e, 'steelMill', 14, 10);
    const plant = placeBuilt(e, 'powerPlant', 20, 10); // a mill with no power consumes nothing
    plant.stock.coal = 500;   // fuelled BEFORE settling, so the grid is live
    plant.stock.machinery = 8;
    mill.stock.machinery = 8;
    mill.stock.ironOre = 30;
    mill.stock.coal = 20;
    runDays(e, 1);            // settle staffing + power

    depot.stock.ironOre = 200;
    depot.stock.coal = 200;
    mill.stock.coal = 0;      // the binding constraint
    mill.stock.ironOre = 30;  // abundant relative to coal

    const next = e.logisticsPriorityPreview().next;
    const coalAt = next.findIndex(n => n.destId === mill.id && n.resource === 'coal');
    const oreAt = next.findIndex(n => n.destId === mill.id && n.resource === 'ironOre');
    expect(coalAt).toBeGreaterThanOrEqual(0);
    expect(oreAt).toBe(-1); // iron ore prevents nothing while coal is the bottleneck
  });

  it('heat competes in deep winter and goes silent in summer', () => {
    // heatDemandFactor() is 0 above the heating threshold, so heat fuel stops
    // competing on its own — no seasonal mode, no player toggle.
    const build = (weather: () => { tempC: number; condition: 'clear'; snowDepth: number; riverFrozen: boolean }) => {
      const e = makeEngine({ weather });
      layRoad(e, 4, 9, 30, 9);
      const depot = placeBuilt(e, 'depot', 5, 10);
      placeBuilt(e, 'constructionOffice', 8, 10);
      const boiler = placeBuilt(e, 'heatingPlant', 12, 10);
      placeBuilt(e, 'apartment', 16, 10);
      placeBuilt(e, 'apartment', 20, 10);
      e.pop = 80;
      runDays(e, 1);
      depot.stock.coal = 200;
      boiler.stock.coal = 0;
      return e.logisticsPriorityPreview().next.find(n => n.destId === boiler.id);
    };

    expect(build(WINTER)).toBeDefined();
    expect(build(() => ({ tempC: 20, condition: 'clear', snowDepth: 0, riverFrozen: false }))).toBeUndefined();
  });

  it('export staging never moves while anything is running dry', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const customs = placeBuilt(e, 'customs', 12, 10);
    const store = placeBuilt(e, 'store', 22, 10);
    placeBuilt(e, 'apartment', 26, 10);
    placeBuilt(e, 'apartment', 29, 10);
    e.pop = 80;

    store.stock.food = 0;
    depot.stock.food = 200;
    depot.stock.coal = 400;
    customs.stock.coal = 0;
    e.autoTrade.enabled = true;
    e.setAutoTradeRule('coal', { mode: 'export', level: 0, currency: 'east' });

    runDays(e, 1);

    const foodTruck = e.trucks.find(t => t.cargo === 'food' && t.destId === store.id);
    const exportTruck = e.trucks.find(t => t.cargo === 'coal' && t.destId === customs.id);
    expect(foodTruck).toBeDefined();
    if (exportTruck) {
      // Housekeeping may use leftover trucks, but never before the shop is served.
      expect(foodTruck!.id).toBeLessThan(exportTruck.id);
    }
  });

  it('spreads trucks across rival destinations instead of draining one first', () => {
    // Marginal re-scoring stands in for the deleted fair-share pass: serving a
    // destination raises its incoming, which drops what its next load is worth.
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const a = placeBuilt(e, 'store', 14, 10);
    const b = placeBuilt(e, 'store', 20, 10);
    placeBuilt(e, 'apartment', 26, 10);
    placeBuilt(e, 'apartment', 29, 10);
    e.pop = 80;
    depot.stock.food = 400;
    depot.stock.clothes = 200;
    a.stock.food = 0;
    b.stock.food = 0;

    runDays(e, 1);

    const toA = e.trucks.filter(t => t.destId === a.id).length;
    const toB = e.trucks.filter(t => t.destId === b.id).length;
    expect(toA).toBeGreaterThan(0);
    expect(toB).toBeGreaterThan(0);
  });

  it('a nearly-finished site is not starved by a freshly placed one', () => {
    // Regression from the previous design: construction demands briefly carried
    // an emptiness urgency, so every fresh site outranked every half-delivered
    // one and the republic filled up with 90%-complete sites and no roofs.
    // Sites consume nothing, so they earn no urgency at all now.
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    depot.stock.planks = 200;
    depot.stock.bricks = 200;
    e.dollars = 1e9;

    expect(e.tryPlace('house', 14, 10).ok).toBe(true);
    const older = e.buildingAt(14, 10)!;
    older.stock.planks = 5; // needs 6 → 1 missing

    expect(e.tryPlace('house', 18, 10).ok).toBe(true);
    const fresher = e.buildingAt(18, 10)!;
    expect(fresher.constructed).toBe(false);

    const ids = e.logisticsPriorityPreview().next.map(n => n.destId);
    const olderAt = ids.indexOf(older.id);
    const fresherAt = ids.indexOf(fresher.id);
    if (olderAt >= 0 && fresherAt >= 0) expect(olderAt).toBeLessThan(fresherAt);
  });

  it('urgency beats construction at neutral dials, and the dials are a real lever', () => {
    const build = (construction: number, lifeline: number) => {
      const { e, depot } = town(40);
      const plant = placeBuilt(e, 'powerPlant', 14, 10);
      runDays(e, 1);
      depot.stock.coal = 200;
      depot.stock.gravel = 200;
      depot.stock.planks = 200;
      depot.stock.bricks = 200;
      plant.stock.coal = 0;   // about to go dark
      plant.stock.machinery = 8;
      e.dollars = 1e9;
      expect(e.tryPlace('apartment', 24, 10).ok).toBe(true);
      e.setLogisticsCategoryWeight('construction', construction);
      e.setLogisticsCategoryWeight('lifeline', lifeline);
      return { plant, next: e.logisticsPriorityPreview().next };
    };

    // Neutral: a plant about to stall outranks starting a building.
    const neutral = build(1, 1);
    expect(neutral.next[0].destId).toBe(neutral.plant.id);
    expect(neutral.next[0].category).toBe('lifeline');

    // Deliberately inverted: the player said construction matters most and
    // lifeline least. That IS allowed to change the order — a dial nobody can
    // feel is the decorative panel this system replaced. What no setting can do
    // is conjure value that is not there (see the guarantees below).
    const inverted = build(4, 0.5);
    expect(inverted.next[0].category).toBe('construction');
  });

  it('no dial setting lets work that prevents nothing preempt a real need', () => {
    // The structural guarantee, and the one that actually matters: a
    // comfortable bin and an export haul both avert zero downtime, so they
    // score zero however the dials are set — they can only ever use trucks
    // that nothing preventable needed.
    const { e, depot } = town(40);
    const customs = placeBuilt(e, 'customs', 12, 10);
    const store = placeBuilt(e, 'store', 22, 10);
    const factory = placeBuilt(e, 'foodFactory', 30, 10);
    runDays(e, 1);
    depot.stock.food = 200;
    depot.stock.coal = 400;
    store.stock.food = 0;          // genuinely running dry
    factory.stock.machinery = 8;
    factory.stock.crops = 1000;    // comfortable — averts nothing
    customs.stock.coal = 0;
    e.autoTrade.enabled = true;
    e.setAutoTradeRule('coal', { mode: 'export', level: 0, currency: 'east' });

    for (const w of [0.5, 1, 2, 4] as const) {
      e.setLogisticsCategoryWeight('industry', w);
      e.setLogisticsCategoryWeight('consumer', 0.5); // even actively deprioritised
      const next = e.logisticsPriorityPreview().next;
      expect(next.some(n => n.destId === customs.id)).toBe(false);   // export staging
      expect(next.some(n => n.destId === factory.id)).toBe(false);   // comfortable bin
      expect(next.some(n => n.destId === store.id)).toBe(true);      // the real need
    }
  });

  it('preview explains each dispatch in terms the engine actually sorts by', () => {
    const { e, depot } = town(30);
    const plant = placeBuilt(e, 'powerPlant', 14, 10);
    runDays(e, 1);
    depot.stock.coal = 200;
    plant.stock.coal = 0;
    plant.stock.machinery = 8;

    const top = e.logisticsPriorityPreview().next[0];
    expect(top).toBeDefined();
    expect(top.reason).toContain('cover');
    expect(top.reason).toContain('trip');
    expect(top.avertedDays).toBeGreaterThan(0);
    expect(top.score).toBeGreaterThan(0);
    expect(top.etaDays).toBeGreaterThan(0);
  });
});
