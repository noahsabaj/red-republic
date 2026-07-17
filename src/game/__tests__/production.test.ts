import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import { seedDemoTown } from '../demo';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

describe('farm placement', () => {
  it('does not count its own footprint as fields', () => {
    const e = makeEngine();
    // Wall off the farm's 7x7 scan window; leave the 2x2 footprint grass
    // plus exactly 2 open tiles — the OLD code counted 4+2=6 and passed.
    for (let y = 7; y <= 13; y++) for (let x = 7; x <= 13; x++) e.tiles[y][x].terrain = 'rock';
    for (let y = 10; y <= 11; y++) for (let x = 10; x <= 11; x++) e.tiles[y][x].terrain = 'grass';
    e.tiles[7][7].terrain = 'grass';
    e.tiles[7][8].terrain = 'grass';
    expect(e.canPlace('farm', 10, 10).ok).toBe(false);

    // 6 open tiles OUTSIDE the footprint → legal
    for (const [x, y] of [[9, 7], [10, 7], [11, 7], [12, 7]] as const) e.tiles[y][x].terrain = 'grass';
    expect(e.canPlace('farm', 10, 10).ok).toBe(true);
  });
});

describe('productionRates', () => {
  it('matches exactly what production() applies (farm, seasonal)', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 33, 9);
    placeBuilt(e, 'depot', 5, 10);
    const farm = placeBuilt(e, 'farm', 10, 10);
    placeBuilt(e, 'apartment', 30, 10); // beds keep pop from clamping to 0
    placeBuilt(e, 'apartment', 27, 10);
    e.pop = 80; // exactly the beds placed — migration cannot change staffing between days
    runDays(e, 1); // settle staffing

    const rates = e.productionRates(farm);
    // March (month 3), 12 fields, staffed, unpowered: 6 * 0.5 * 1 * 0.2 * 2.2
    expect(rates.outputs.crops).toBeCloseTo(6 * 0.5 * 0.2 * 2.2, 9);

    const before = farm.stock.crops ?? 0;
    runDays(e, 1);
    expect((farm.stock.crops ?? 0) - before).toBeCloseTo(rates.outputs.crops!, 9);
  });

  it('reports zero farm output in January (the winter-farm display lie)', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 33, 9);
    placeBuilt(e, 'depot', 5, 10);
    const farm = placeBuilt(e, 'farm', 10, 10);
    placeBuilt(e, 'apartment', 30, 10);
    e.pop = 40; // exactly the beds placed
    runDays(e, 1);
    e.month = 1;
    expect(e.productionRates(farm).outputs.crops ?? 0).toBe(0);
  });

  it('accounts for input starvation (sawmill)', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 33, 9);
    placeBuilt(e, 'depot', 5, 10);
    const mill = placeBuilt(e, 'sawmill', 10, 10);
    placeBuilt(e, 'apartment', 30, 10);
    e.pop = 40; // exactly the beds placed
    runDays(e, 1);

    mill.stock.wood = 0.5; // needs 2/day at current eff — starved
    const rates = e.productionRates(mill);
    expect(rates.inputs.wood).toBeCloseTo(0.5, 9);          // consumes only what exists
    expect(rates.outputs.planks).toBeCloseTo(3 * (0.5 / 2), 9);

    const beforeWood = mill.stock.wood;
    const beforePlanks = mill.stock.planks ?? 0;
    runDays(e, 1);
    expect(beforeWood - mill.stock.wood).toBeCloseTo(rates.inputs.wood!, 9);
    expect((mill.stock.planks ?? 0) - beforePlanks).toBeCloseTo(rates.outputs.planks!, 9);
  });

  it('scales woodcutter output by unoccupied forest tiles', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 33, 9);
    placeBuilt(e, 'depot', 5, 10);
    for (const [x, y] of [[9, 11], [9, 12], [11, 11], [11, 12]] as const) e.tiles[y][x].terrain = 'forest';
    const wc = placeBuilt(e, 'woodcutter', 10, 10); // road-adjacent, forest in reach
    placeBuilt(e, 'apartment', 30, 10);
    e.pop = 40; // exactly the beds placed
    runDays(e, 1);
    // 4 forest tiles → factor 4/6; fully staffed, no power need → eff 1
    expect(e.productionRates(wc).outputs.wood).toBeCloseTo(4 * (4 / 6), 9);
  });
});

describe('demo town', () => {
  it('seeds and settles 100 days without breaking invariants', () => {
    const e = new GameEngine({ seed: 1961 });
    seedDemoTown(e);
    expect(e.pop).toBeGreaterThan(0);
    expect(Number.isFinite(e.happiness)).toBe(true);
    for (const b of e.buildings.values()) {
      for (const [r, v] of Object.entries(b.stock)) {
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThanOrEqual(e.capOf(b, r as never) + 1e-9);
      }
    }
    runDays(e, 30); // keeps running fine
    expect(e.pop).toBeGreaterThan(0);
  });
});
