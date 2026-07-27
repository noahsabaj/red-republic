import { describe, expect, it } from 'vitest';
import { BUILDINGS } from '../config';
import { makeEngine, placeBuilt } from './helpers';

describe('storage display data model', () => {
  it('defines storage capacity for all goods on warehouse and depot', () => {
    const warehouseDef = BUILDINGS.warehouse;
    const depotDef = BUILDINGS.depot;

    expect(Object.keys(warehouseDef.storage).length).toBeGreaterThanOrEqual(13);
    expect(Object.keys(depotDef.storage).length).toBeGreaterThanOrEqual(13);

    expect(warehouseDef.storage.food).toBe(40);
    expect(depotDef.storage.food).toBe(120);
  });

  it('calculates total stock and capacity for storage buildings', () => {
    const e = makeEngine();
    const warehouse = placeBuilt(e, 'warehouse', 10, 10);
    const def = BUILDINGS.warehouse;

    warehouse.stock.food = 15;
    warehouse.stock.planks = 25;

    const allResources = Object.keys(def.storage) as (keyof typeof def.storage)[];
    const totalStock = allResources.reduce((sum, r) => sum + (warehouse.stock[r] ?? 0), 0);
    const totalCap = allResources.reduce((sum, r) => sum + (def.storage[r] ?? 0), 0);

    expect(totalStock).toBe(40);
    expect(totalCap).toBe(500); // 12 * 40 + 20 machinery
  });
});
