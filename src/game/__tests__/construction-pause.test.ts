import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

function laborTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 16, 9);
  placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 7, 10);
  return e;
}

function readyHouse(e: ReturnType<typeof laborTown>, x = 10, y = 10) {
  e.tryPlace('house', x, y);
  const site = e.buildingAt(x, y)!;
  site.stock.planks = 6; site.stock.bricks = 4;
  return site;
}

describe('construction pause controls', () => {
  it('pauses and resumes an under-construction site individually', () => {
    const e = laborTown();
    e.rubles = 10_000;
    const site = readyHouse(e);

    // Pause site
    e.setSitePaused(site.id, true);
    expect(site.paused).toBe(true);

    runDays(e, 1);
    expect(site.progress).toBe(0); // no work happened

    // Resume site
    e.setSitePaused(site.id, false);
    expect(site.paused).toBe(false);

    runDays(e, 1);
    expect(site.progress).toBeGreaterThan(0); // work resumed
  });

  it('unpausing an auto-bought planned site charges auto-buy fees', () => {
    const e = laborTown();
    placeBuilt(e, 'customs', 1, 10);
    e.rubles = 10_000;

    // Place auto-bought planned site
    e.tryPlace('house', 10, 10, { autoBuy: true, plan: true });
    const site = e.buildingAt(10, 10)!;
    expect(site.paused).toBe(true);
    expect(site.autoBought).toBe(true);
    expect(site.bondedCustomsId).toBeUndefined();

    const rublesBefore = e.rubles;
    const res = e.setSitePaused(site.id, false);
    expect(res.ok).toBe(true);
    expect(site.paused).toBe(false);
    expect(site.bondedCustomsId).toBeDefined();
    expect(e.rubles).toBeLessThan(rublesBefore);
  });

  it('global construction master switch pauses all site progress without clearing site state', () => {
    const e = laborTown();
    e.rubles = 10_000;
    const site1 = readyHouse(e, 10, 10);
    const site2 = readyHouse(e, 13, 10);

    // Pause globally
    e.setGlobalConstructionEnabled(false);
    expect(e.globalConstructionEnabled).toBe(false);

    runDays(e, 2);
    expect(site1.progress).toBe(0);
    expect(site2.progress).toBe(0);

    // Resume globally
    e.setGlobalConstructionEnabled(true);
    runDays(e, 1);
    expect(site1.progress).toBeGreaterThan(0);
    expect(site2.progress).toBeGreaterThan(0);
  });
});
