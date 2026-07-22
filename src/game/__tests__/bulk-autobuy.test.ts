import { describe, expect, it } from 'vitest';
import { makeEngine, placeBuilt } from './helpers';

describe('bulk construction site auto-buy (setSiteImportMany)', () => {
  it('enables auto-buy and charges remaining material bills for multiple sites', () => {
    const engine = makeEngine();
    // Build a customs house for border imports
    const customs = placeBuilt(engine, 'customs', 0, 0);
    customs.eff = 1.0;
    engine.rubles = 50000;

    // Place 3 unconstructed sites without autoBuy
    engine.tryPlace('house', 10, 10);
    engine.tryPlace('apartment', 15, 15);
    engine.tryPlace('sawmill', 20, 20);

    const b1 = engine.buildingAt(10, 10)!;
    const b2 = engine.buildingAt(15, 15)!;
    const b3 = engine.buildingAt(20, 20)!;

    expect(b1.autoBought).toBeFalsy();
    expect(b2.autoBought).toBeFalsy();
    expect(b3.autoBought).toBeFalsy();

    const ids = [b1.id, b2.id, b3.id];
    const res = engine.setSiteImportMany(ids, 'east');

    expect(res.succeeded).toBe(3);
    expect(res.failed).toBe(0);
    expect(res.totalCost).toBeGreaterThan(0);
    expect(engine.rubles).toBe(50000 - res.totalCost);

    expect(b1.autoBought).toBe(true);
    expect(b2.autoBought).toBe(true);
    expect(b3.autoBought).toBe(true);
    expect(b1.bondedCustomsId).toBe(customs.id);
  });

  it('respects construction priority ordering when treasury funds are limited', () => {
    const engine = makeEngine();
    const customs = placeBuilt(engine, 'customs', 0, 0);
    customs.eff = 1.0;

    engine.tryPlace('apartment', 10, 10);
    const bLow = engine.buildingAt(10, 10)!;
    bLow.buildPriority = -1; // Low priority

    engine.tryPlace('house', 15, 15);
    const bHigh = engine.buildingAt(15, 15)!;
    bHigh.buildPriority = 1; // High priority

    // Calculate cost for bHigh
    const highCost = engine.autoBuyRemainingCost(bHigh.id, 'east');
    // Set rubles to only cover bHigh
    engine.rubles = highCost;

    const res = engine.setSiteImportMany([bLow.id, bHigh.id], 'east');

    expect(res.succeeded).toBe(1);
    expect(res.failed).toBe(1);
    expect(bHigh.autoBought).toBe(true);
    expect(bLow.autoBought).toBeFalsy();
  });

  it('disables auto-buy for multiple sites when currency is null', () => {
    const engine = makeEngine();
    const customs = placeBuilt(engine, 'customs', 0, 0);
    customs.eff = 1.0;
    engine.rubles = 50000;

    engine.tryPlace('house', 10, 10);
    engine.tryPlace('apartment', 15, 15);
    const b1 = engine.buildingAt(10, 10)!;
    const b2 = engine.buildingAt(15, 15)!;

    engine.setSiteImport(b1.id, 'east');
    engine.setSiteImport(b2.id, 'east');

    expect(b1.autoBought).toBe(true);
    expect(b2.autoBought).toBe(true);

    const res = engine.setSiteImportMany([b1.id, b2.id], null);
    expect(res.succeeded).toBe(2);
    expect(b1.autoBought).toBe(false);
    expect(b2.autoBought).toBe(false);
  });
});
