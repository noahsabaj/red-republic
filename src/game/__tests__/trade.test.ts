import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt } from './helpers';

function withCustoms() {
  const e = makeEngine();
  layRoad(e, 4, 9, 30, 9);
  placeBuilt(e, 'customs', 10, 10);
  return e;
}

describe('trade', () => {
  it('sell() never drains State Store food', () => {
    const e = withCustoms();
    const store = placeBuilt(e, 'store', 15, 10);
    store.stock.food = 30;
    expect(e.sellableStock('food')).toBe(0);
    const res = e.sell('food', 10, 'east');
    expect(res.ok).toBe(false);
    expect(store.stock.food).toBe(30);
  });

  it('sell() leaves a power plant its 3-day coal reserve', () => {
    const e = withCustoms();
    const plant = placeBuilt(e, 'powerPlant', 15, 10); // inputs 2 coal/day → keeps 6
    plant.stock.coal = 40;
    expect(e.sellableStock('coal')).toBe(34);
    const res = e.sell('coal', 100, 'east');
    expect(res.ok).toBe(true);
    expect(plant.stock.coal).toBe(6);
  });

  it('sell() reaches any drivable building — road or off-road', () => {
    const e = withCustoms();
    const far = placeBuilt(e, 'warehouse', 25, 15); // no road contact — reachable off-road only
    far.stock.steel = 20;
    // off-road reachability now counts: goods that can physically reach the
    // border (however slowly) are sellable
    expect(e.sellableStock('steel')).toBe(20);
    expect(e.sell('steel', 10, 'west').ok).toBe(true);
  });

  it('buy() imports the affordable partial amount instead of rejecting', () => {
    const e = withCustoms();
    e.rubles = 100;
    const price = e.importPriceOf('food', 'east'); // 4.5 * 1.6 = 7.2
    expect(price).toBeCloseTo(7.2, 9);
    const res = e.buy('food', 100, 'east');
    expect(res.ok).toBe(true);
    const customs = [...e.buildings.values()].find(b => b.defId === 'customs')!;
    expect(customs.stock.food).toBe(13); // floor(100 / 7.2)
    expect(e.rubles).toBeCloseTo(100 - 13 * 7.2, 9);
  });

  it('buy() distinguishes "no space" from "no funds"', () => {
    const e = withCustoms();
    const customs = [...e.buildings.values()].find(b => b.defId === 'customs')!;
    customs.stock.food = 80; // cap
    e.rubles = 1e6;
    expect(e.buy('food', 10, 'east').msg).toMatch(/storage is full/);
    customs.stock.food = 0;
    e.rubles = 1;
    expect(e.buy('food', 10, 'east').msg).toMatch(/Not enough rubles/);
  });

  it('sellableStock() cache invalidates when the world changes', () => {
    const e = withCustoms();
    const wh = placeBuilt(e, 'warehouse', 15, 10);
    wh.stock.steel = 20;
    expect(e.sellableStock('steel')).toBe(20);
    e.sell('steel', 5, 'west');
    expect(e.sellableStock('steel')).toBe(15);
  });
});
