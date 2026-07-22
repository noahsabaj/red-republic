import { describe, expect, it } from 'vitest';
import type { GameEngine, TilePatch } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/** Paint a vertical water channel. */
function carveChannel(e: GameEngine, x0: number, x1: number, y0: number, y1: number) {
  const patches: TilePatch[] = [];
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) patches.push({ x, y, terrain: 'water' });
  e.applyTilePatches(patches);
}

describe('bridges', () => {
  it('a road painted on water becomes a bridge construction site — no money charged', () => {
    const e = makeEngine();
    carveChannel(e, 20, 20, 5, 15);
    expect(e.canPlace('road', 20, 10).ok).toBe(true);
    e.rubles = 1000;
    e.tryPlace('road', 20, 10);
    expect(e.rubles).toBe(1000); // domestic construction never touches the treasury
    const site = e.buildingAt(20, 10)!;
    expect(site.defId).toBe('bridge'); // plank+steel bill, not gravel
    expect(site.constructed).toBe(false);
    expect(e.tiles[10][20].road).toBeFalsy(); // not drivable until built
    // instant mode imports the prefab for dollars, priced per-tile as a bridge
    e.dollars = 1000;
    const cost = e.instantCost('road', 20, 11);
    expect(cost).toBeGreaterThan(e.instantCost('road')); // bridge > land road
    e.tryPlace('road', 20, 11, { instant: true });
    expect(e.dollars).toBe(1000 - cost);
    expect(e.tiles[11][20].road).toBe(true); // instant = built immediately
  });

  it('trucks deliver across a bridge', () => {
    const e = makeEngine();
    carveChannel(e, 20, 20, 0, 47);
    const depot = placeBuilt(e, 'depot', 5, 11);
    placeBuilt(e, 'constructionOffice', 10, 11);
    const store = placeBuilt(e, 'store', 28, 11);
    depot.stock.food = 60;
    layRoad(e, 4, 10, 19, 10);
    layRoad(e, 21, 10, 29, 10);
    e.applyTilePatches([{ x: 20, y: 10, road: true }]); // the bridge tile
    runDays(e, 12);
    expect(store.stock.food ?? 0).toBeGreaterThan(0);
  });

  it('a bridge joins a road-connected island to the depot network (no longer "isolated")', () => {
    const e = makeEngine();
    carveChannel(e, 20, 20, 0, 47);            // 1-wide river down the whole map, no crossing yet
    const depot = placeBuilt(e, 'depot', 5, 11);
    placeBuilt(e, 'constructionOffice', 10, 11);
    const store = placeBuilt(e, 'store', 28, 11);   // east ("island") shore
    depot.stock.food = 60;
    layRoad(e, 4, 10, 19, 10);                  // west approach
    layRoad(e, 21, 10, 29, 10);                 // island roads
    e.applyTilePatches([{ x: 20, y: 10, road: true }]); // the bridge tile
    runDays(e, 1);

    // The land domain now crosses the bridge, so the island shares the depot's land
    // component. Pre-fix this read connected=false while roadConnected=true — the exact
    // contradiction that produced the false "N buildings isolated" warning.
    expect(store.roadConnected).toBe(true);
    expect(store.connected).toBe(true);
    expect(e.alerts.some(a => /isolated/.test(a.text))).toBe(false);

    runDays(e, 12);
    expect(store.stock.food ?? 0).toBeGreaterThan(0); // and deliveries keep crossing it
  });

  it('a bridge carries off-road delivery to an island with no local roads', () => {
    const e = makeEngine();
    carveChannel(e, 20, 20, 0, 47);
    const depot = placeBuilt(e, 'depot', 5, 11);
    placeBuilt(e, 'constructionOffice', 10, 11);
    depot.stock.food = 60;
    layRoad(e, 4, 10, 19, 10);                  // west approach only — no island roads
    e.applyTilePatches([{ x: 20, y: 10, road: true }]); // the bridge tile
    const store = placeBuilt(e, 'store', 23, 11); // far bank, reachable only off-road across the bridge
    runDays(e, 1);

    expect(store.connected).toBe(true);        // off-road land now crosses the bridge
    expect(store.roadConnected).toBe(false);   // but there are no island roads
    expect(e.alerts.some(a => /isolated/.test(a.text))).toBe(false);

    runDays(e, 30);
    expect(store.stock.food ?? 0).toBeGreaterThan(0); // the off-road fallback reaches across the bridge
  });
});

describe('ports and barges', () => {
  it('ports must touch water', () => {
    const e = makeEngine();
    expect(e.canPlace('port', 10, 10).ok).toBe(false);
    expect(e.canPlace('port', 10, 10).reason).toMatch(/shore/i);
    carveChannel(e, 20, 22, 0, 47);
    expect(e.canPlace('port', 18, 10).ok).toBe(true); // east edge touches x=20 water
  });

  it('relays goods across water: truck to port, barge over, truck onward', () => {
    const e = makeEngine();
    carveChannel(e, 20, 22, 0, 47); // 3-wide river splits the map — no bridge
    // west shore: supplies + trucks + port
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    const portW = placeBuilt(e, 'port', 18, 10);
    layRoad(e, 4, 9, 19, 9);
    depot.stock.planks = 60;
    depot.stock.bricks = 60;
    // east shore: a construction site that nothing on its network can supply
    const portE = placeBuilt(e, 'port', 23, 10);
    layRoad(e, 23, 9, 32, 9);
    e.rubles = 10000;
    const placed = e.tryPlace('house', 30, 10); // needs 6 planks + 4 bricks
    expect(placed.ok).toBe(true);
    const site = e.buildingAt(30, 10)!;
    expect(site.constructed).toBe(false);

    let sawBoat = false;
    for (let i = 0; i < 60 && !site.constructed; i++) {
      runDays(e, 1);
      if (e.boats.length > 0) sawBoat = true;
    }
    expect(sawBoat).toBe(true);               // a barge actually sailed
    expect(site.constructed).toBe(true);      // and the house got built
    expect(e.buildings.get(portW.id)).toBeDefined();
    expect(e.buildings.get(portE.id)).toBeDefined();
  });

  it('barges auto-buy IMPORT materials across water to an island site', () => {
    const e = makeEngine();
    carveChannel(e, 20, 22, 0, 47);           // 3-wide river splits the map — no bridge
    // WEST shore: a customs to import through, an office for trucks, a port — all road-linked
    const customs = placeBuilt(e, 'customs', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    const portW = placeBuilt(e, 'port', 18, 10);
    layRoad(e, 6, 9, 19, 9);
    customs.stock.planks = 40; customs.stock.bricks = 40; // REAL stock that bonded imports must never touch
    // EAST island: a port + road + the auto-buy site; NO domestic planks/bricks anywhere
    const portE = placeBuilt(e, 'port', 23, 10);
    layRoad(e, 23, 9, 32, 9);
    e.rubles = 100_000;
    const cost = e.autoBuyImportCost('house');
    const before = e.rubles;
    const placed = e.tryPlace('house', 30, 10, { autoBuy: true, currency: 'east' }); // 6 planks + 4 bricks
    expect(placed.ok).toBe(true);
    const site = e.buildingAt(30, 10)!;
    expect(site.autoBought).toBe(true);
    expect(before - e.rubles).toBe(cost);      // the IMPORT bill is charged once, at placement

    let sawBoat = false;
    for (let i = 0; i < 60 && !site.constructed; i++) {
      runDays(e, 1);
      if (e.boats.length > 0) sawBoat = true;
    }
    expect(sawBoat).toBe(true);                 // a barge actually ferried the paid import
    expect(site.constructed).toBe(true);        // and the island house built out

    // leak-safety: the paid import is booked exactly once and never drains the source.
    // (Any post-placement ₽ spend is foreign construction labor — pop is 0, so builders
    // are paid — a separate, expected sink, not a re-charged import.)
    expect(e.stats.imported.planks ?? 0).toBe(6);  // the exact bill, counted once — not per relay leg
    expect(e.stats.imported.bricks ?? 0).toBe(4);
    expect(e.stockOf(customs, 'planks')).toBe(40); // bonded source (customs real stock) untouched
    expect(e.stockOf(customs, 'bricks')).toBe(40);
    // the ferried imports flowed through to the site — none duplicated or stranded at the ports
    expect((e.stockOf(portW, 'planks') ?? 0) + (e.stockOf(portE, 'planks') ?? 0)).toBeLessThan(1);
    expect((e.stockOf(portW, 'bricks') ?? 0) + (e.stockOf(portE, 'bricks') ?? 0)).toBeLessThan(1);
  });
});
