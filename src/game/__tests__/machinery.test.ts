import { describe, expect, it } from 'vitest';
import { BALANCE, BUILDINGS, CONTRACTS, IMPORT_MARKUP, INSTANT_BUILD, RESOURCES } from '../config';
import { GameEngine, buildingWorn } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * Machinery: the industrialization tax. Wears with activity, never stalls
 * a building (worn = half efficiency), imported through customs until the
 * Machine Works closes the loop.
 */

/** Staffed food factory town. placeBuilt seeds min(bin cap, bill) spares. */
function factoryTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 33, 9);
  const depot = placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 8, 10); // trucks + builders
  const factory = placeBuilt(e, 'foodFactory', 10, 10);
  placeBuilt(e, 'apartment', 30, 10);
  e.pop = 40;
  depot.stock.crops = 120;
  factory.stock.crops = 40;
  return { e, depot, factory };
}

describe('wear', () => {
  it('running machines wear in proportion to activity; idle machines do not', () => {
    const { e, factory } = factoryTown();
    runDays(e, 1); // settle staffing
    const bin0 = factory.stock.machinery ?? 0;
    expect(bin0).toBeGreaterThan(0); // seeded from the construction bill
    const rates = e.productionRates(factory);
    expect(rates.inputs.machinery ?? 0).toBeGreaterThan(0);
    const before = factory.stock.machinery ?? 0;
    runDays(e, 1);
    expect(before - (factory.stock.machinery ?? 0)).toBeCloseTo(rates.inputs.machinery!, 9);

    // idle: an unstaffed twin wears nothing
    const idle = placeBuilt(e, 'foodFactory', 14, 10);
    idle.staff = 0;
    const idleBin = idle.stock.machinery ?? 0;
    expect(e.productionRates(idle).inputs.machinery ?? 0).toBe(0);
    runDays(e, 1);
    expect(idle.stock.machinery ?? 0).toBeCloseTo(idleBin, 9);
  });

  it('a dry bin halves efficiency — never a hard stall — and recovers on resupply', () => {
    const { e, factory } = factoryTown();
    runDays(e, 1);
    const healthy = e.productionRates(factory).outputs.food ?? 0;
    expect(healthy).toBeGreaterThan(0);

    factory.stock.machinery = 0;
    expect(buildingWorn(factory)).toBe(true);
    const worn = e.productionRates(factory).outputs.food ?? 0;
    expect(worn).toBeGreaterThan(0); // soft penalty, not a stall
    expect(worn / healthy).toBeCloseTo(BALANCE.wornEffMult, 6);

    factory.stock.machinery = 2;
    expect(buildingWorn(factory)).toBe(false);
    expect(e.productionRates(factory).outputs.food ?? 0).toBeCloseTo(healthy, 6);
  });

  it('plants wear with burn intensity; a plant with nothing to do wears nothing', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 33, 9);
    placeBuilt(e, 'depot', 5, 10);
    const plant = placeBuilt(e, 'powerPlant', 10, 10);
    placeBuilt(e, 'apartment', 30, 10);
    e.pop = 40;
    plant.stock.coal = 50;
    runDays(e, 1);
    expect(e.productionRates(plant).inputs.machinery ?? 0).toBeGreaterThan(0);
    const before = plant.stock.machinery ?? 0;
    runDays(e, 1);
    expect(plant.stock.machinery ?? 0).toBeLessThan(before);

    // a heating plant on a warm day (CALM 15°C): zero throttle, zero wear
    const heat = placeBuilt(e, 'heatingPlant', 14, 10);
    heat.stock.coal = 40;
    const heatBin = heat.stock.machinery ?? 0;
    runDays(e, 2);
    expect(heat.stock.machinery ?? 0).toBeCloseTo(heatBin, 9);
  });
});

describe('machinery logistics', () => {
  it('trucks refill worn wear bins for factories AND plants (the urgent-repair branch)', () => {
    const { e, depot, factory } = factoryTown();
    const plant = placeBuilt(e, 'powerPlant', 14, 10);
    plant.stock.coal = 50;
    depot.stock.machinery = 20;
    factory.stock.machinery = 0;
    plant.stock.machinery = 0;
    runDays(e, 12);
    expect(factory.stock.machinery ?? 0).toBeGreaterThan(0);
    expect(plant.stock.machinery ?? 0).toBeGreaterThan(0);
  });

  it('a worn building is repaired even when lower-priority input hauls saturate the truck budget (the reported bug)', () => {
    // The prio-24 test above runs in an idle town; the real bug only bites when a
    // busy town's ~6 trucks are all consumed by higher-priority hauls first. A
    // crowd of empty factories pulling from a distant depot keeps the budget
    // saturated for weeks — pre-fix the worn bin (old prio 24) never dispatched.
    const e = makeEngine();
    layRoad(e, 2, 9, 46, 9);
    const depot = placeBuilt(e, 'depot', 40, 10);
    placeBuilt(e, 'constructionOffice', 43, 10); // ~6 trucks, even unstaffed
    depot.stock.crops = 2000;
    depot.stock.machinery = 40;
    for (let x = 10; x <= 24; x += 2) placeBuilt(e, 'foodFactory', x, 10).stock.crops = 0; // prio-20 input flood
    const worn = placeBuilt(e, 'foodFactory', 6, 10);
    worn.stock.crops = 40;    // full → no input demand, only a machinery one
    worn.stock.machinery = 0;
    expect(buildingWorn(worn)).toBe(true);
    runDays(e, 15);
    // urgent repair now outranks the flood of input hauls, so it wins a truck
    expect(worn.stock.machinery ?? 0).toBeGreaterThan(0);
    expect(buildingWorn(worn)).toBe(false);
  });

  it('supplyOf keeps a month of spares — a consumer is never fully robbed', () => {
    const { e, factory } = factoryTown();
    const needy = placeBuilt(e, 'foodFactory', 14, 10);
    needy.stock.machinery = 0;
    factory.stock.machinery = 4;
    runDays(e, 15);
    const wear = BUILDINGS.foodFactory.wear!.machinery!;
    // the donor never drops below its protected month of spares (minus its own wear)
    expect(factory.stock.machinery ?? 0).toBeGreaterThanOrEqual(wear * BALANCE.wearReserveDays - wear * 15 - 1e-6);
  });
});

describe('imports and the Machine Works', () => {
  it('buy() imports machinery through customs and counts stats.imported', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 10, 10);
    e.rubles = 2000;
    const price = e.importPriceOf('machinery', 'east');
    expect(price).toBeCloseTo(80 * IMPORT_MARKUP, 6); // normal difficulty, month 1
    const res = e.buy('machinery', 5, 'east');
    expect(res.ok).toBe(true);
    expect(e.rubles).toBeCloseTo(2000 - 5 * price, 6);
    expect(e.stats.imported.machinery).toBe(5);
  });

  it('the firstMachines objective lands at 5 imported machines', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 10, 10);
    e.rubles = 2000;
    e.buy('machinery', 5, 'east');
    runDays(e, 1);
    expect(e.objectivesDone).toContain('firstMachines');
  });

  it('the Machine Works turns steel into machinery and unlocks the arc objectives', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    placeBuilt(e, 'depot', 5, 10);
    const works = placeBuilt(e, 'machineWorks', 10, 10);
    placeBuilt(e, 'apartment', 30, 10);
    placeBuilt(e, 'apartment', 33, 10);
    e.pop = 80;
    works.stock.steel = 30;
    runDays(e, 1);
    expect(e.objectivesDone).toContain('meansOfProduction');
    const before = e.stats.produced.machinery;
    runDays(e, 10);
    expect(e.stats.produced.machinery).toBeGreaterThan(before);
    expect(Number.isFinite(e.stats.produced.machinery)).toBe(true);
  });

  it('construction sites demand machinery like any other material, and finish born with a full spare bin', () => {
    const { e, depot } = factoryTown();
    depot.stock.machinery = 1;   // the construction bill — the site must pull it to finish
    depot.stock.bricks = 60;
    depot.stock.steel = 30;
    depot.stock.planks = 30;
    expect(e.tryPlace('foodFactory', 14, 10).ok).toBe(true);
    const site = e.buildingAt(14, 10)!;
    runDays(e, 30);
    expect(site.constructed).toBe(true);
    // born maintained: a finished building is commissioned with a FULL spare bin
    // (its bill machinery is consumed by the build; the spare set is installed on
    // completion) so a new town never starts life half-broken.
    const cap = BUILDINGS.foodFactory.storage.machinery!;
    const seeded = site.stock.machinery ?? 0;
    expect(seeded).toBeGreaterThan(cap - 1);
    expect(seeded).toBeLessThanOrEqual(cap);
  });
});

describe('repair imports', () => {
  it('a worn building with no domestic machinery self-heals by importing through customs (paid, reserve-safe)', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 6, 10);
    placeBuilt(e, 'constructionOffice', 8, 10); // trucks
    const worn = placeBuilt(e, 'foodFactory', 12, 10);
    worn.stock.crops = 40;    // full → no input demand, only a machinery one
    worn.stock.machinery = 0; // worn, and the town holds no machinery anywhere else
    e.rubles = 5000;
    e.repairImportsEnabled = true;
    expect(buildingWorn(worn)).toBe(true);
    const imported0 = e.stats.imported.machinery ?? 0;
    const rubles0 = e.rubles;
    runDays(e, 12);
    expect(worn.stock.machinery ?? 0).toBeGreaterThan(0);              // healed
    expect(buildingWorn(worn)).toBe(false);
    expect(e.stats.imported.machinery ?? 0).toBeGreaterThan(imported0); // from the border
    expect(e.rubles).toBeLessThan(rubles0);                            // and paid for
    expect(e.rubles).toBeGreaterThanOrEqual(e.autoTrade.reserveRubles); // never below the reserve floor
  });

  it('domestic machinery is spent before importing — a reachable depot means no border spend', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 6, 10);
    const depot = placeBuilt(e, 'depot', 8, 10);
    placeBuilt(e, 'constructionOffice', 11, 10);
    const worn = placeBuilt(e, 'foodFactory', 14, 10);
    worn.stock.crops = 40;
    worn.stock.machinery = 0;
    depot.stock.machinery = 20; // domestic supply on hand
    e.rubles = 5000;
    const imported0 = e.stats.imported.machinery ?? 0;
    runDays(e, 12);
    expect(worn.stock.machinery ?? 0).toBeGreaterThan(0);       // healed
    expect(e.stats.imported.machinery ?? 0).toBe(imported0);    // domestically — nothing imported
  });

  it('repair imports off → a stranded worn building stays worn and the treasury is untouched', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 6, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const worn = placeBuilt(e, 'foodFactory', 12, 10);
    worn.stock.crops = 40;
    worn.stock.machinery = 0;
    e.rubles = 5000;
    e.setRepairImports(false);
    runDays(e, 12);
    expect(worn.stock.machinery ?? 0).toBe(0);   // no domestic source + imports off → stuck worn
    expect(buildingWorn(worn)).toBe(true);
    expect(e.rubles).toBe(5000);                 // not a ruble spent
  });
});

describe('save compatibility', () => {
  it('a pre-machinery save can never NaN stats.produced (merge over defaults)', () => {
    const { e } = factoryTown();
    runDays(e, 3);
    const blob = e.serialize();
    delete (blob.body.stats.produced as Partial<Record<string, number>>).machinery;
    delete (blob.body.stats as { imported?: unknown }).imported;
    const e2 = GameEngine.fromSave(blob);
    expect(e2.stats.produced.machinery).toBe(0);
    const works = placeBuilt(e2, 'machineWorks', 20, 12);
    works.stock.steel = 30;
    works.staff = 22;
    runDays(e2, 5);
    expect(Number.isFinite(e2.stats.produced.machinery)).toBe(true);
  });
});

describe('instant-build pricing', () => {
  it('matches the prefab formula: materials at Western import prices + labor, with premium', () => {
    const e = makeEngine();
    for (const defId of ['house', 'powerPlant', 'machineWorks', 'road']) {
      const def = BUILDINGS[defId];
      let mats = 0;
      for (const [r, amt] of Object.entries(def.materials)) {
        mats += (amt) * RESOURCES[r as keyof typeof RESOURCES].priceWest;
      }
      const expected = Math.max(1, Math.ceil(
        (mats * IMPORT_MARKUP + def.labor * INSTANT_BUILD.laborDollars) * INSTANT_BUILD.premium));
      expect(e.instantCost(defId)).toBe(expected);
    }
    // heavy industry is near-prohibitive; a road tile is pocket change
    expect(e.instantCost('machineWorks')).toBeGreaterThan(500);
    expect(e.instantCost('road')).toBeLessThanOrEqual(5);
  });

  it('contract offers stay value-banded across resources', () => {
    // sanity on the config bands themselves
    expect(CONTRACTS.valueBandEast[0]).toBeLessThan(CONTRACTS.valueBandEast[1]);
    expect(CONTRACTS.valueBandWest[0]).toBeLessThan(CONTRACTS.valueBandWest[1]);
    expect(CONTRACTS.minUnits).toBeGreaterThanOrEqual(2);
  });
});
