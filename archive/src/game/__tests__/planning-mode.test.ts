import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * Planning mode: a site placed with { plan: true } is an inert blueprint — it
 * draws no materials and no builders until commenceSite() (or commenceAllPlanned())
 * pays any deferred bill and unpauses it. An auto-buy planned site defers its ₽/$
 * import bill to commence, not placement.
 */
function planTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 20, 9);
  const depot = placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10);
  e.rubles = 100_000;
  e.dollars = 100_000;
  depot.stock.planks = 100; depot.stock.bricks = 100; // domestic supply for commenced sites
  return { e, depot };
}

describe('planning mode', () => {
  it('a planned site draws no materials and no builders until commenced', () => {
    const { e } = planTown();
    expect(e.tryPlace('house', 12, 10, { plan: true }).ok).toBe(true);
    const site = e.buildingAt(12, 10)!;
    expect(site.paused).toBe(true);
    runDays(e, 10);
    expect(site.progress).toBe(0);
    expect(site.stock.planks ?? 0).toBe(0);      // nothing delivered
    expect(site.incoming.planks ?? 0).toBe(0);   // nothing even dispatched
    expect(site.constructed).toBe(false);
  });

  it('commenceSite starts the blueprint — materials flow and it builds', () => {
    const { e } = planTown();
    e.tryPlace('house', 12, 10, { plan: true });
    const site = e.buildingAt(12, 10)!;
    expect(e.commenceSite(site.id).ok).toBe(true);
    expect(site.paused).toBe(false);
    runDays(e, 20);
    expect(site.constructed).toBe(true);
  });

  it('a planned auto-buy site pays nothing at placement, the full bill at commence', () => {
    const { e } = planTown();
    placeBuilt(e, 'customs', 9, 10);
    const before = e.rubles;
    e.tryPlace('house', 12, 10, { plan: true, autoBuy: true });
    const site = e.buildingAt(12, 10)!;
    expect(site.paused).toBe(true);
    expect(site.autoBought).toBe(true);
    expect(e.rubles).toBe(before);                       // NOT charged at placement
    const bill = e.autoBuyRemainingCost(site.id, 'east');
    expect(e.commenceSite(site.id).ok).toBe(true);
    expect(before - e.rubles).toBe(bill);                // charged at commence
    runDays(e, 30);
    expect(site.constructed).toBe(true);
  });

  it('commenceAllPlanned starts every affordable blueprint', () => {
    const { e } = planTown();
    e.tryPlace('house', 12, 10, { plan: true });
    e.tryPlace('house', 14, 10, { plan: true });
    e.tryPlace('house', 16, 10, { plan: true });
    expect(e.commenceAllPlanned()).toBe(3);
    expect([...e.buildings.values()].filter(b => b.paused)).toHaveLength(0);
  });
});
