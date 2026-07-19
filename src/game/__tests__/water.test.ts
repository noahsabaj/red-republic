import { describe, expect, it } from 'vitest';
import type { GameEngine } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/** Paint a vertical water channel. */
function carveChannel(e: GameEngine, x0: number, x1: number, y0: number, y1: number) {
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) e.tiles[y][x].terrain = 'water';
}

describe('bridges', () => {
  it('a road painted on water becomes a bridge construction site — no money charged', () => {
    const e = makeEngine();
    carveChannel(e, 20, 20, 5, 15);
    expect(e.canPlace('road', 20, 10).ok).toBe(true);
    e.rubles = 1000;
    e.tryPlace('road', 20, 10, false);
    expect(e.rubles).toBe(1000); // domestic construction never touches the treasury
    const site = e.buildingAt(20, 10)!;
    expect(site.defId).toBe('bridge'); // plank+steel bill, not gravel
    expect(site.constructed).toBe(false);
    expect(e.tiles[10][20].road).toBeFalsy(); // not drivable until built
    // instant mode imports the prefab for dollars, priced per-tile as a bridge
    e.dollars = 1000;
    const cost = e.instantCost('road', 20, 11);
    expect(cost).toBeGreaterThan(e.instantCost('road')); // bridge > land road
    e.tryPlace('road', 20, 11, true);
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
    e.tiles[10][20].road = true; // the bridge tile
    runDays(e, 12);
    expect(store.stock.food ?? 0).toBeGreaterThan(0);
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
    const placed = e.tryPlace('house', 30, 10, false); // needs 6 planks + 4 bricks
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
});
