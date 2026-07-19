import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * Construction labor is domestic-first: citizens manning offices build free,
 * builders beyond them are imported and cost ₽/builder-day, capped by what
 * the treasury can afford (broke → construction slows, never goes negative).
 */
function laborTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 16, 9);
  placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10);
  return e;
}

/** A road-adjacent site with its full material bill already delivered. */
function readyHouse(e: ReturnType<typeof laborTown>, x = 10, y = 10) {
  e.tryPlace('house', x, y, false);
  const site = e.buildingAt(x, y)!;
  site.stock.planks = 6; site.stock.bricks = 4; // house bill → ready to build
  return site;
}

describe('foreign labor', () => {
  it('charges ₽ for foreign builders at population 0', () => {
    const e = laborTown();
    e.rubles = 10_000;
    readyHouse(e);
    const before = e.rubles;
    runDays(e, 1);
    expect(e.rubles).toBeLessThan(before);                       // paid the imported crew
    expect(e.tradeLedger.today.foreignLabor).toBeLessThan(0);    // recorded on the ledger
    expect(e.buildingAt(10, 10)!.progress).toBeGreaterThan(0);   // and work happened
  });

  it('staffed citizens build for free (domestic-first)', () => {
    const e = laborTown();
    e.rubles = 10_000;
    const office = [...e.buildings.values()].find(b => b.defId === 'constructionOffice')!;
    e.setStaffPriorityMany([office.id], true); // win the job queue
    placeBuilt(e, 'apartment', 10, 12); // housing so citizens exist
    e.pop = 20;                          // enough to fully staff the 10-worker office
    readyHouse(e, 13, 10);
    const before = e.rubles;
    runDays(e, 1);
    expect(e.buildingAt(13, 10)!.progress).toBeGreaterThan(0);   // built
    expect(e.tradeLedger.today.foreignLabor).toBe(0);            // for free — no foreign crew charged
    expect(e.rubles).toBeGreaterThanOrEqual(before);             // labor never drained the treasury
  });

  it('the affordability cap stalls construction but never overdraws', () => {
    const e = laborTown();
    const perDay = 1.5; // BALANCE.foreignLaborPerDay × normal importPriceMult(1.0)
    e.rubles = perDay * 4; // enough for ~4 builder-days, then broke
    readyHouse(e);
    for (let d = 0; d < 6; d++) {
      runDays(e, 1);
      expect(e.rubles).toBeGreaterThanOrEqual(0); // never negative from labor
    }
    // spent down to (near) zero and then stalled — didn't build the whole house on credit
    expect(e.rubles).toBeLessThan(perDay);
    expect(e.buildingAt(10, 10)!.constructed).toBe(false);
  });

  it('the toggle stops hiring (and paying) — construction stalls at pop 0', () => {
    const e = laborTown();
    e.rubles = 10_000;
    e.setForeignLaborEnabled(false);
    readyHouse(e);
    const before = e.rubles;
    runDays(e, 3);
    expect(e.buildingAt(10, 10)!.progress).toBe(0); // no domestic builders, none hired
    expect(e.rubles).toBe(before);                  // nothing charged
    // turn it back on → work resumes and the ledger shows the spend
    e.setForeignLaborEnabled(true);
    runDays(e, 1);
    expect(e.buildingAt(10, 10)!.progress).toBeGreaterThan(0);
    expect(e.tradeLedger.today.foreignLabor).toBeLessThan(0);
  });
});
