import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt, runDays, totalOf } from './helpers';

describe('logistics', () => {
  it('picks the nearest supplier, not the first in insertion order', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 31, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);      // far supplier, created first
    placeBuilt(e, 'constructionOffice', 15, 10);
    const warehouse = placeBuilt(e, 'warehouse', 25, 10); // near supplier, created later
    const store = placeBuilt(e, 'store', 30, 10);
    depot.stock.food = 50;
    warehouse.stock.food = 40;

    runDays(e, 1);

    const truck = e.trucks.find(t => t.cargo === 'food' && t.destId === store.id);
    expect(truck).toBeDefined();
    expect(truck!.srcId).toBe(warehouse.id);
    expect(warehouse.stock.food).toBe(34); // 6 taken from the NEAR one
    expect(depot.stock.food).toBe(50);     // far one untouched
  });

  it('pins the overflowing producer as the supplier for overflow hauling', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 25, 9);
    const depot = placeBuilt(e, 'depot', 20, 10);
    placeBuilt(e, 'constructionOffice', 15, 10);
    e.tiles[10][10].deposit = 'coal';
    const mine = placeBuilt(e, 'coalMine', 10, 10);
    mine.stock.coal = 55;  // > 80% of its 60 cap
    depot.stock.coal = 30; // stocked storage that the OLD code would have drained instead

    runDays(e, 1);

    const truck = e.trucks.find(t => t.cargo === 'coal');
    expect(truck).toBeDefined();
    expect(truck!.srcId).toBe(mine.id);   // hauled FROM the overflowing mine
    expect(truck!.destId).toBe(depot.id); // TO storage
    expect(mine.stock.coal).toBe(49);
    expect(depot.stock.coal).toBe(30);
  });

  it('conserves cargo when the destination fills up mid-transit', () => {
    const e = makeEngine();
    layRoad(e, 8, 9, 22, 9);
    const depot = placeBuilt(e, 'depot', 10, 10);
    placeBuilt(e, 'constructionOffice', 13, 10);
    const store = placeBuilt(e, 'store', 20, 10);
    depot.stock.food = 100;

    runDays(e, 1); // dispatches a 6-food truck to the store
    const truck = e.trucks.find(t => t.cargo === 'food');
    expect(truck).toBeDefined();

    store.stock.food = 38; // only 2 of the 6 will fit (cap 40)
    const baseline = totalOf(e, 'food');
    for (let i = 0; i < 20; i++) {
      runDays(e, 1);
      expect(totalOf(e, 'food')).toBeCloseTo(baseline, 6); // never destroyed
    }
    expect(store.stock.food).toBe(40);
    expect(e.trucks.length).toBe(0);        // round trip finished
    expect(depot.stock.food).toBe(98);      // 6 out, 4 returned
  });

  it('does not treat a corner-diagonal road as adjacent', () => {
    const e = makeEngine();
    const house = placeBuilt(e, 'house', 10, 10);
    e.tiles[9][9].road = true; // touches only the NW corner
    expect(e.adjacentRoads(house)).toEqual([]);
    e.tiles[9][10].road = true; // orthogonal edge contact
    expect(e.adjacentRoads(house)).toEqual([{ x: 10, y: 9 }]);
  });

  it('returns cargo when the destination is bulldozed mid-transit', () => {
    const e = makeEngine();
    layRoad(e, 8, 9, 22, 9);
    const depot = placeBuilt(e, 'depot', 10, 10);
    placeBuilt(e, 'constructionOffice', 13, 10);
    const store = placeBuilt(e, 'store', 20, 10);
    depot.stock.food = 100;

    runDays(e, 1);
    expect(e.trucks.some(t => t.cargo === 'food')).toBe(true);
    e.bulldozeAt(20, 10);
    expect(e.buildings.has(store.id)).toBe(false);

    for (let i = 0; i < 20; i++) runDays(e, 1);
    expect(e.trucks.length).toBe(0);
    expect(depot.stock.food).toBe(100); // full load came home
  });
});
