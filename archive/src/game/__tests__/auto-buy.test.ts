import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays, totalOf } from './helpers';

/**
 * Auto-buy: pay the ₽ import bill upfront at placement; the exact materials
 * arrive as BONDED imports earmarked to that site — never entering customs
 * stock (cap-exempt) and never divertible to another site or an export.
 */
function autoBuyTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 22, 9);
  const depot = placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10);
  const customs = placeBuilt(e, 'customs', 9, 10);
  e.rubles = 100_000;
  return { e, depot, customs };
}

describe('auto-buy placement', () => {
  it('charges the shown import cost upfront and marks the site bonded', () => {
    const { e } = autoBuyTown();
    const cost = e.autoBuyImportCost('house');
    const before = e.rubles;
    expect(e.tryPlace('house', 12, 10, { autoBuy: true }).ok).toBe(true);
    expect(before - e.rubles).toBe(cost);       // shown == charged
    const site = e.buildingAt(12, 10)!;
    expect(site.autoBought).toBe(true);
    expect(site.constructed).toBe(false);        // still needs labor + time
  });

  it('builds from bonded imports with no domestic stock', () => {
    const { e } = autoBuyTown();
    // no planks/bricks anywhere domestically — only the border can supply them
    e.tryPlace('house', 12, 10, { autoBuy: true });
    runDays(e, 30);
    expect(e.buildingAt(12, 10)?.constructed).toBe(true);
  });

  it('rejects when rubles are short — nothing placed, nothing charged', () => {
    const { e } = autoBuyTown();
    e.rubles = e.autoBuyImportCost('house') - 1;
    const before = e.rubles;
    const res = e.tryPlace('house', 12, 10, { autoBuy: true });
    expect(res.ok).toBe(false);
    expect(res.reason).toMatch(/rubles/i);
    expect(e.rubles).toBe(before);
    expect(e.buildingAt(12, 10)).toBeUndefined();
  });

  it('requires a customs house to import through', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 12, 9);
    placeBuilt(e, 'constructionOffice', 7, 10);
    e.rubles = 100_000;
    const res = e.tryPlace('house', 10, 10, { autoBuy: true });
    expect(res.ok).toBe(false);
    expect(res.reason).toMatch(/customs/i);
  });

  it('plain roads ignore auto-buy — gravel is sourced domestically, no ₽ charged', () => {
    const { e } = autoBuyTown();
    const before = e.rubles;
    expect(e.tryPlace('road', 13, 11, { autoBuy: true }).ok).toBe(true); // fresh grass tile → normal road site
    expect(e.rubles).toBe(before);
  });
});

describe('auto-buy earmarking (bonded virtual source)', () => {
  it('two sites never cross-feed, and customs stock is never touched', () => {
    const { e, customs } = autoBuyTown();
    customs.stock.planks = 0; customs.stock.bricks = 0;
    e.tryPlace('house', 12, 10, { autoBuy: true }); // site A
    e.tryPlace('house', 14, 10, { autoBuy: true }); // site B
    runDays(e, 40);
    expect(e.buildingAt(12, 10)?.constructed).toBe(true);
    expect(e.buildingAt(14, 10)?.constructed).toBe(true);
    // bonded goods are virtual — they never entered (nor drained) customs stock
    expect(customs.stock.planks ?? 0).toBe(0);
    expect(customs.stock.bricks ?? 0).toBe(0);
  });

  it('a bill larger than the customs cap still delivers (cap-exempt)', () => {
    const { e, customs } = autoBuyTown();
    customs.stock.bricks = 80; // the bricks bin is FULL — a real import would be blocked
    e.tryPlace('apartment', 12, 10, { autoBuy: true }); // needs 30 bricks
    runDays(e, 70);
    expect(e.buildingAt(12, 10)?.constructed).toBe(true);
    expect(customs.stock.bricks).toBe(80); // untouched — the bonded bricks bypassed it
  });

  it('bulldozing an auto-bought site keeps the money and refunds delivered stock', () => {
    const { e, customs } = autoBuyTown();
    customs.stock.planks = 0; customs.stock.bricks = 0;
    e.setForeignLaborEnabled(false); // isolate the money check from labor charges
    e.tryPlace('house', 12, 10, { autoBuy: true });
    const site = e.buildingAt(12, 10)!;
    for (let i = 0; i < 25 && (site.stock.planks ?? 0) + (site.stock.bricks ?? 0) < 1; i++) runDays(e, 1);
    const delivered = (site.stock.planks ?? 0) + (site.stock.bricks ?? 0);
    expect(delivered).toBeGreaterThan(0);
    const rublesBeforeBulldoze = e.rubles;
    expect(e.bulldozeAt(12, 10)).toBe(true);
    expect(e.rubles).toBe(rublesBeforeBulldoze); // bulldoze refunds materials, not money
    runDays(e, 15);
    expect(totalOf(e, 'planks') + totalOf(e, 'bricks')).toBeGreaterThanOrEqual(delivered - 1e-6);
  });
});
