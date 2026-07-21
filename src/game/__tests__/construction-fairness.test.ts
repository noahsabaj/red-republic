import { describe, expect, it } from 'vitest';
import { BALANCE, DIFFICULTIES } from '../config';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * Construction spreads its daily builder pool across EVERY ready site by
 * max-min fair-share — the old code fed only the lowest-id site, so a town full
 * of stocked sites crawled one-at-a-time. Construction priority tiers let the
 * player concentrate the crews: higher tiers are fully staffed (and their
 * materials hauled) before lower ones (Strict). Total builder-days per day is
 * conserved; only the split across sites changed.
 */
function town() {
  const e = makeEngine();
  layRoad(e, 4, 9, 20, 9);
  placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10); // one office → a 10-builder pool
  e.rubles = 100_000;
  return e;
}

/** A road-adjacent house with its full bill already delivered → ready to build. */
function ready(e: ReturnType<typeof town>, x: number, tier: -1 | 0 | 1 = 0) {
  e.tryPlace('house', x, 10);
  const site = e.buildingAt(x, 10)!;
  site.stock.planks = 6; site.stock.bricks = 4; // house bill → ready
  if (tier !== 0) e.setSitePriority(site.id, tier);
  return site;
}

describe('construction fair-share', () => {
  it('every ready site progresses in one day, not just the first', () => {
    const e = town();
    const houses = [10, 11, 12, 13].map(x => ready(e, x));
    runDays(e, 1);
    for (const h of houses) expect(h.progress).toBeGreaterThan(0);   // all four moved (the bug: only #1 did)
    for (const h of houses) expect(h.progress).toBeCloseTo(2.5, 6);  // 10-builder pool split evenly over 4
  });

  it('conserves total throughput and pays the foreign crew once', () => {
    const e = town();
    const houses = [10, 11, 12, 13].map(x => ready(e, x));
    runDays(e, 1);
    const applied = houses.reduce((s, h) => s + h.progress, 0);
    expect(applied).toBeCloseTo(10, 6);                              // the whole pool, no more, no less
    const perDay = BALANCE.foreignLaborPerDay * DIFFICULTIES[e.difficulty].importPriceMult;
    expect(e.tradeLedger.today.foreignLabor).toBeCloseTo(-(10 * perDay), 6);
  });

  it('Strict priority: High is fully crewed before Normal', () => {
    const e = town();
    const hi = ready(e, 10, 1);
    const norm = ready(e, 11, 0);
    runDays(e, 1);
    expect(hi.progress).toBeCloseTo(10, 6); // High took the whole pool
    expect(norm.progress).toBe(0);          // Normal waited its turn
  });

  it('Low yields to Normal', () => {
    const e = town();
    const norm = ready(e, 10, 0);
    const low = ready(e, 11, -1);
    runDays(e, 1);
    expect(norm.progress).toBeCloseTo(10, 6);
    expect(low.progress).toBe(0);
  });

  it('same-tier sites share the pool evenly', () => {
    const e = town();
    const a = ready(e, 10, 1);
    const b = ready(e, 11, 1); // both High
    runDays(e, 1);
    expect(a.progress).toBeCloseTo(5, 6);
    expect(b.progress).toBeCloseTo(5, 6);
  });

  it('surplus from a near-done High site spills down to Normal', () => {
    const e = town();
    const hi = ready(e, 10, 1);
    hi.progress = 57; // 3 builder-days from done (house labor 60) → cap 3
    const norm = ready(e, 11, 0);
    runDays(e, 1);
    expect(hi.constructed).toBe(true);       // finished with its 3
    expect(norm.progress).toBeCloseTo(7, 6); // the other 7 spilled to the Normal tier
  });
});

describe('batch-approve planned sites', () => {
  function planTown() {
    const e = makeEngine();
    layRoad(e, 4, 9, 20, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 7, 10);
    e.rubles = 100_000; e.dollars = 100_000;
    depot.stock.planks = 100; depot.stock.bricks = 100;
    return { e, depot };
  }

  it('plannedCount tracks paused sites; cost is zero without auto-buy', () => {
    const { e } = planTown();
    e.tryPlace('house', 12, 10, { plan: true });
    e.tryPlace('house', 14, 10, { plan: true });
    expect(e.plannedCount()).toBe(2);
    expect(e.plannedCommenceCost()).toEqual({ rubles: 0, dollars: 0 });
    expect(e.commenceAllPlanned()).toBe(2);
    expect(e.plannedCount()).toBe(0);
  });

  it('plannedCommenceCost sums the auto-buy bill, bucketed by bloc', () => {
    const { e } = planTown();
    placeBuilt(e, 'customs', 9, 10);
    e.tryPlace('house', 12, 10, { plan: true, autoBuy: true, currency: 'east' });
    e.tryPlace('house', 14, 10, { plan: true, autoBuy: true, currency: 'west' });
    const a = e.buildingAt(12, 10)!, b = e.buildingAt(14, 10)!;
    const cost = e.plannedCommenceCost();
    expect(cost.rubles).toBe(e.autoBuyRemainingCost(a.id, 'east'));
    expect(cost.dollars).toBe(e.autoBuyRemainingCost(b.id, 'west'));
    expect(cost.rubles).toBeGreaterThan(0);
    expect(cost.dollars).toBeGreaterThan(0);
  });

  it('commences the highest-priority plan first when funds are short', () => {
    const { e } = planTown();
    placeBuilt(e, 'customs', 9, 10);
    e.tryPlace('house', 12, 10, { plan: true, autoBuy: true, currency: 'east' }); // Normal, lower id
    e.tryPlace('house', 14, 10, { plan: true, autoBuy: true, currency: 'east' }); // High, higher id
    const normal = e.buildingAt(12, 10)!, high = e.buildingAt(14, 10)!;
    e.setSitePriority(high.id, 1);
    e.rubles = e.autoBuyRemainingCost(high.id, 'east'); // affords exactly one bill
    expect(e.commenceAllPlanned()).toBe(1);
    expect(high.paused).toBe(false);  // High commenced despite its higher id
    expect(normal.paused).toBe(true); // Normal left planned — the treasury ran out
  });
});
