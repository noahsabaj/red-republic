import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays, totalOf } from './helpers';

/**
 * Roads are real construction: a painted tile is a 1×1 site that needs
 * gravel trucked in and a crew to lay it, and is not drivable until done.
 */

/** Depot + construction office on a road row, gravel in the depot. */
function roadTown(gravel = 40) {
  const e = makeEngine();
  layRoad(e, 4, 9, 14, 9);
  const depot = placeBuilt(e, 'depot', 5, 10);
  placeBuilt(e, 'constructionOffice', 10, 10);
  depot.stock.gravel = gravel;
  return { e, depot };
}

describe('road construction lifecycle', () => {
  it('paint → site → gravel delivery → crew → drivable tile, counted at completion', () => {
    const { e } = roadTown();
    const before = e.stats.roadsBuilt;
    const res = e.tryPlace('road', 15, 9); // extends the row eastward
    expect(res.ok).toBe(true);
    const site = e.buildingAt(15, 9)!;
    expect(site.defId).toBe('road');
    expect(site.constructed).toBe(false);
    expect(e.tiles[9][15].road).toBeFalsy();       // not drivable yet
    expect(e.stats.roadsBuilt).toBe(before);       // counted at COMPLETION, not placement

    runDays(e, 15);
    expect(e.buildingAt(15, 9)).toBeUndefined();   // the site dissolved…
    expect(e.tiles[9][15].road).toBe(true);        // …into a road tile
    expect(e.stats.roadsBuilt).toBe(before + 1);
    // silent completion: no 'completed!' toast for road tiles
    expect(e.drainEvents().every(ev => !ev.text.includes('Road completed'))).toBe(true);
  });

  it('an unfinished tile is not drivable: deliveries flow only after completion', () => {
    const { e, depot } = roadTown();
    depot.stock.food = 60;
    const store = placeBuilt(e, 'store', 16, 9); // reachable ONLY through tile (15,9)
    e.tryPlace('road', 15, 9);
    runDays(e, 2); // site placed, likely still incomplete this early
    if (!e.tiles[9][15].road) {
      expect(store.stock.food ?? 0).toBe(0); // nothing crossed the gap
    }
    runDays(e, 20);
    expect(e.tiles[9][15].road).toBe(true);
    expect(store.stock.food ?? 0).toBeGreaterThan(0);
  });

  it('a painted spur crawls outward: frontier tiles complete network-first', () => {
    const { e } = roadTown();
    for (const x of [15, 16, 17]) expect(e.tryPlace('road', x, 9).ok).toBe(true);
    const doneDay: Record<number, number> = {};
    for (let d = 1; d <= 45; d++) {
      runDays(e, 1);
      for (const x of [15, 16, 17]) {
        if (e.tiles[9][x].road && doneDay[x] === undefined) doneDay[x] = d;
      }
    }
    expect(doneDay[15]).toBeDefined();
    expect(doneDay[16]).toBeDefined();
    expect(doneDay[17]).toBeDefined();
    expect(doneDay[15]).toBeLessThan(doneDay[16]);
    expect(doneDay[16]).toBeLessThan(doneDay[17]);
  });

  it('painting over an in-flight site is rejected', () => {
    const { e } = roadTown();
    expect(e.tryPlace('road', 15, 9).ok).toBe(true);
    const again = e.tryPlace('road', 15, 9);
    expect(again.ok).toBe(false);
    expect(again.reason).toMatch(/Occupied/i);
  });

  it('bulldozing a site mid-delivery conserves gravel (the truck turns back)', () => {
    const { e } = roadTown();
    const total = totalOf(e, 'gravel');
    e.tryPlace('road', 15, 9);
    // step until a truck is actually en route with gravel
    for (let i = 0; i < 10 && !e.trucks.some(t => t.cargo === 'gravel'); i++) runDays(e, 1);
    expect(e.trucks.some(t => t.cargo === 'gravel')).toBe(true);
    expect(e.bulldozeAt(15, 9)).toBe(true);
    runDays(e, 10); // truck returns its load
    expect(totalOf(e, 'gravel')).toBeCloseTo(total, 6);
    expect(e.canPlace('road', 15, 9).ok).toBe(true); // tile is placeable again
  });

  it('bulldozing a site returns its ALREADY-DELIVERED stock to storage (no vanish)', () => {
    const { e, depot } = roadTown();
    depot.stock.bricks = 0;
    e.tryPlace('sawmill', 8, 10);
    const site = e.buildingAt(8, 10)!;
    site.stock.bricks = 5; // materials already trucked into the site
    const total = totalOf(e, 'bricks');
    expect(e.bulldozeAt(8, 10)).toBe(true);
    expect(e.buildingAt(8, 10)).toBeUndefined();
    expect(e.trucks.some(t => t.cargo === 'bricks')).toBe(true); // a refund truck is hauling them back
    runDays(e, 12);
    expect(totalOf(e, 'bricks')).toBeCloseTo(total, 6); // conserved — not dropped into the void
    expect(depot.stock.bricks ?? 0).toBeGreaterThan(0); // and landed in storage
  });

  it('a bill larger than one truck refunds in multiple loads', () => {
    const { e, depot } = roadTown();
    depot.stock.planks = 0;
    e.tryPlace('apartment', 7, 11);
    const site = e.buildingAt(7, 11)!;
    site.stock.planks = 10; // > truckCapacity (6) → needs two return trucks
    const total = totalOf(e, 'planks');
    e.bulldozeAt(7, 11);
    expect(e.trucks.filter(t => t.cargo === 'planks').length).toBeGreaterThanOrEqual(2);
    runDays(e, 14);
    expect(totalOf(e, 'planks')).toBeCloseTo(total, 6);
  });

  it('instant mode imports the prefab: dollars, immediate tile, no site', () => {
    const { e } = roadTown();
    e.dollars = 100;
    const cost = e.instantCost('road');
    expect(e.tryPlace('road', 15, 9, { instant: true }).ok).toBe(true);
    expect(e.dollars).toBe(100 - cost);
    expect(e.tiles[9][15].road).toBe(true);
    expect(e.buildingAt(15, 9)).toBeUndefined();
  });

  it('nothing domestic ever charges rubles', () => {
    const { e } = roadTown();
    e.rubles = 0;
    expect(e.tryPlace('road', 15, 9).ok).toBe(true);
    expect(e.tryPlace('house', 5, 12).ok).toBe(true);
    expect(e.tryPlace('sawmill', 8, 12).ok).toBe(true);
    expect(e.tryPlace('machineWorks', 11, 12).ok).toBe(true);
    expect(e.rubles).toBe(0);
  });

  it('a frontier road chain stays quiet; an off-road-only building advises a road', () => {
    const { e } = roadTown();
    // a chain extending the network stays quiet
    for (const x of [15, 16, 17]) e.tryPlace('road', x, 9);
    runDays(e, 1);
    expect(e.alerts.some(a => a.id === 'sites')).toBe(false);
    // a finished building off the road network is reachable off-road (slow):
    // not "stranded", but the soft advisory to lay a road
    placeBuilt(e, 'house', 30, 30);
    runDays(e, 1);
    expect(e.alerts.some(a => a.id === 'sites')).toBe(false);   // not truly unreachable
    expect(e.alerts.some(a => a.id === 'offroad')).toBe(true);  // "lay a road" advisory
  });
});

describe('fractional deliveries', () => {
  it('a dribble-fed site still receives its sub-1 remainder and completes', () => {
    const { e } = roadTown();
    e.tryPlace('road', 15, 9);
    const site = e.buildingAt(15, 9)!;
    site.stock.gravel = 1.4; // a supply-starved truck delivered a fraction
    runDays(e, 15);
    expect(e.tiles[9][15].road).toBe(true); // the 0.6 arrived; the road finished
  });
});

describe('road sites and the world', () => {
  it('buildings cannot be placed on a road site, nor sites on roads', () => {
    const { e } = roadTown();
    e.tryPlace('road', 15, 9);
    expect(e.canPlace('house', 15, 9).ok).toBe(false);
    expect(e.canPlace('road', 10, 9).ok).toBe(false); // finished road blocks a site
  });
});
