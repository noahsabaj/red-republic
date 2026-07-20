import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * The build-menu toggles stamp defaults onto a new site; these engine methods
 * edit THAT site's policy after placement: import currency (₽ East / $ West),
 * foreign-labor permission, and an instant $ finish of the work that remains.
 */
function siteTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 22, 9);
  placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10);
  const customs = placeBuilt(e, 'customs', 9, 10);
  e.rubles = 100_000; e.dollars = 100_000;
  return { e, customs };
}

describe('per-site import toggle', () => {
  it('enabling import charges the remaining bill and builds from bonded stock', () => {
    const { e, customs } = siteTown();
    customs.stock.planks = 0; customs.stock.bricks = 0;
    e.tryPlace('house', 12, 10);                 // normal site — domestic materials
    const site = e.buildingAt(12, 10)!;
    expect(site.autoBought).toBeFalsy();
    const bill = e.autoBuyRemainingCost(site.id, 'east');
    const before = e.rubles;
    expect(e.setSiteImport(site.id, 'east').ok).toBe(true);
    expect(before - e.rubles).toBe(bill);        // shown == charged
    expect(site.autoBought).toBe(true);
    expect(site.importCurrency).toBe('east');
    runDays(e, 30);
    expect(site.constructed).toBe(true);
    expect(customs.stock.planks ?? 0).toBe(0);   // bonded — never touched customs stock
  });

  it('west sourcing pays dollars, leaves rubles alone', () => {
    const { e } = siteTown();
    e.tryPlace('house', 12, 10);
    const site = e.buildingAt(12, 10)!;
    const billW = e.autoBuyRemainingCost(site.id, 'west');
    const rublesBefore = e.rubles, dollarsBefore = e.dollars;
    expect(e.setSiteImport(site.id, 'west').ok).toBe(true);
    expect(dollarsBefore - e.dollars).toBe(billW);
    expect(e.rubles).toBe(rublesBefore);
    expect(site.importCurrency).toBe('west');
  });

  it('disabling import stops the bond without refunding paid cargo', () => {
    const { e } = siteTown();
    e.tryPlace('house', 12, 10, { autoBuy: true }); // paid at placement
    const site = e.buildingAt(12, 10)!;
    expect(site.autoBought).toBe(true);
    const before = e.rubles;
    e.setSiteImport(site.id, null);
    expect(site.autoBought).toBe(false);
    expect(e.rubles).toBe(before);                  // no refund
  });
});

describe('per-site foreign labor', () => {
  it('a domestic-only site stalls at pop 0 while its neighbour builds', () => {
    const { e } = siteTown();
    e.tryPlace('house', 12, 10);
    e.tryPlace('house', 14, 10);
    const a = e.buildingAt(12, 10)!, b = e.buildingAt(14, 10)!;
    a.stock.planks = 6; a.stock.bricks = 4;         // both fully stocked → ready
    b.stock.planks = 6; b.stock.bricks = 4;
    e.setSiteForeignLabor(a.id, false);             // A refuses paid foreign builders
    runDays(e, 4);                                  // a window while population is still 0
    expect(a.progress).toBe(0);                     // no citizens, no foreign → stalled
    expect(b.progress).toBeGreaterThan(0);          // foreign builders are working B
  });
});

describe('per-site instant finish', () => {
  it('finishes a partial site for dollars, prorated to the work remaining', () => {
    const { e } = siteTown();
    e.tryPlace('house', 12, 10);
    const site = e.buildingAt(12, 10)!;
    site.stock.planks = 6; site.stock.bricks = 4;   // materials delivered
    runDays(e, 2);                                  // some labor done, not finished
    expect(site.progress).toBeGreaterThan(0);
    expect(site.constructed).toBe(false);
    const remaining = e.instantFinishCost(site.id);
    expect(remaining).toBeLessThan(e.instantCost('house')); // cheaper than a from-scratch prefab
    const before = e.dollars;
    expect(e.finishSiteInstant(site.id).ok).toBe(true);
    expect(before - e.dollars).toBe(remaining);
    expect(site.constructed).toBe(true);
  });
});
