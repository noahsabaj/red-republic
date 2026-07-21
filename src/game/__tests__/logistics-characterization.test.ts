import { describe, expect, it } from 'vitest';
import { OBJECTIVES } from '../config';
import type { BuildingInst, GameEngine, Truck } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

function truckTrace(truck: Truck) {
  return {
    id: truck.id,
    srcId: truck.srcId,
    destId: truck.destId,
    cargo: truck.cargo,
    amount: truck.amount,
    points: truck.points,
    daysTotal: truck.daysTotal,
    daysDone: truck.daysDone,
    phase: truck.phase,
  };
}

function stockTrace(e: GameEngine, buildings: BuildingInst[]) {
  return buildings.map(b => ({
    id: b.id,
    food: e.stockOf(b, 'food'),
    foodIncoming: e.incomingOf(b, 'food'),
  }));
}

function horizontalRoad(x0: number, x1: number, y = 9) {
  return Array.from({ length: x1 - x0 + 1 }, (_, i) => ({ x: x0 + i, y }));
}

function carveChannel(e: GameEngine, x0: number, x1: number) {
  const patches = [];
  for (let y = 0; y < e.mapH; y++) {
    for (let x = x0; x <= x1; x++) patches.push({ x, y, terrain: 'water' as const });
  }
  e.applyTilePatches(patches);
}

function suppressObjectiveRewards(e: GameEngine) {
  e.objectivesDone = OBJECTIVES.map(o => o.id);
}

describe('logistics compatibility characterization', () => {
  it('pins dispatch IDs, order, routes, timing, stock, and incoming reservations', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 35, 9);
    const warehouse = placeBuilt(e, 'warehouse', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const firstStore = placeBuilt(e, 'store', 20, 10);
    const secondStore = placeBuilt(e, 'store', 30, 10);
    warehouse.stock.food = 12;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      buildings: stockTrace(e, [warehouse, firstStore, secondStore]),
    }).toEqual({
      trucks: [
        {
          id: 1, srcId: 1, destId: 3, cargo: 'food', amount: 6,
          points: [{ x: 5.5, y: 10.5 }, ...horizontalRoad(5, 20), { x: 20.5, y: 10.5 }],
          daysTotal: 2.88, daysDone: 0, phase: 'go',
        },
        {
          id: 2, srcId: 1, destId: 4, cargo: 'food', amount: 6,
          points: [{ x: 5.5, y: 10.5 }, ...horizontalRoad(5, 30), { x: 30.5, y: 10.5 }],
          daysTotal: 4.68, daysDone: 0, phase: 'go',
        },
      ],
      buildings: [
        { id: 1, food: 0, foodIncoming: 0 },
        { id: 3, food: 0, foodIncoming: 6 },
        { id: 4, food: 0, foodIncoming: 6 },
      ],
    });
  });

  it('uses building insertion order when suppliers are equally distant', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 26, 9);
    const firstWarehouse = placeBuilt(e, 'warehouse', 10, 10);
    const secondWarehouse = placeBuilt(e, 'warehouse', 20, 10);
    placeBuilt(e, 'constructionOffice', 5, 10);
    const store = placeBuilt(e, 'store', 15, 10);
    firstWarehouse.stock.food = 20;
    secondWarehouse.stock.food = 20;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      buildings: stockTrace(e, [firstWarehouse, secondWarehouse, store]),
    }).toEqual({
      trucks: [{
        id: 1, srcId: 1, destId: 4, cargo: 'food', amount: 6,
        points: [{ x: 10.5, y: 10.5 }, ...horizontalRoad(10, 15), { x: 15.5, y: 10.5 }],
        daysTotal: 1.08, daysDone: 0, phase: 'go',
      }],
      buildings: [
        { id: 1, food: 14, foodIncoming: 0 },
        { id: 2, food: 20, foodIncoming: 0 },
        { id: 4, food: 0, foodIncoming: 6 },
      ],
    });
  });

  it('uses ordered access-tile ties and the existing path traversal', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    layRoad(e, 10, 11, 14, 11);
    const warehouse = placeBuilt(e, 'warehouse', 10, 10);
    placeBuilt(e, 'constructionOffice', 5, 10);
    const store = placeBuilt(e, 'store', 14, 10);
    warehouse.stock.food = 20;

    runDays(e, 1);

    expect({
      access: e.adjacentRoads(warehouse),
      trucks: e.trucks.map(truckTrace),
      buildings: stockTrace(e, [warehouse, store]),
    }).toEqual({
      access: [{ x: 10, y: 9 }, { x: 10, y: 11 }],
      trucks: [{
        id: 1, srcId: 1, destId: 3, cargo: 'food', amount: 6,
        points: [{ x: 10.5, y: 10.5 }, ...horizontalRoad(10, 14), { x: 14.5, y: 10.5 }],
        daysTotal: 0.8999999999999999, daysDone: 0, phase: 'go',
      }],
      buildings: [
        { id: 1, food: 14, foodIncoming: 0 },
        { id: 3, food: 0, foodIncoming: 6 },
      ],
    });
  });

  it('prefers any road-reachable supplier over a much nearer off-road supplier', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 20, 9);
    const nearOffRoad = placeBuilt(e, 'warehouse', 18, 12);
    const farOnRoad = placeBuilt(e, 'warehouse', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const store = placeBuilt(e, 'store', 20, 10);
    nearOffRoad.stock.food = 20;
    farOnRoad.stock.food = 20;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      buildings: stockTrace(e, [nearOffRoad, farOnRoad, store]),
    }).toEqual({
      trucks: [{
        id: 1, srcId: 2, destId: 4, cargo: 'food', amount: 6,
        points: [{ x: 5.5, y: 10.5 }, ...horizontalRoad(5, 20), { x: 20.5, y: 10.5 }],
        daysTotal: 2.88, daysDone: 0, phase: 'go',
      }],
      buildings: [
        { id: 1, food: 20, foodIncoming: 0 },
        { id: 2, food: 14, foodIncoming: 0 },
        { id: 4, food: 0, foodIncoming: 6 },
      ],
    });
  });

  it('re-evaluates supply after an earlier equal-priority dispatch depletes it', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 35, 9);
    const closeWarehouse = placeBuilt(e, 'warehouse', 15, 10);
    const backupWarehouse = placeBuilt(e, 'warehouse', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const firstStore = placeBuilt(e, 'store', 20, 10);
    const secondStore = placeBuilt(e, 'store', 30, 10);
    closeWarehouse.stock.food = 6;
    backupWarehouse.stock.food = 20;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      buildings: stockTrace(e, [closeWarehouse, backupWarehouse, firstStore, secondStore]),
    }).toEqual({
      trucks: [
        {
          id: 1, srcId: 1, destId: 4, cargo: 'food', amount: 6,
          points: [{ x: 15.5, y: 10.5 }, ...horizontalRoad(15, 20), { x: 20.5, y: 10.5 }],
          daysTotal: 1.08, daysDone: 0, phase: 'go',
        },
        {
          id: 2, srcId: 2, destId: 5, cargo: 'food', amount: 6,
          points: [{ x: 5.5, y: 10.5 }, ...horizontalRoad(5, 30), { x: 30.5, y: 10.5 }],
          daysTotal: 4.68, daysDone: 0, phase: 'go',
        },
      ],
      buildings: [
        { id: 1, food: 0, foodIncoming: 0 },
        { id: 2, food: 14, foodIncoming: 0 },
        { id: 4, food: 0, foodIncoming: 6 },
        { id: 5, food: 0, foodIncoming: 6 },
      ],
    });
  });

  it('charges a pinned bonded machinery repair exactly once at dispatch', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    const customs = placeBuilt(e, 'customs', 6, 10);
    placeBuilt(e, 'constructionOffice', 9, 10);
    const worn = placeBuilt(e, 'foodFactory', 12, 10);
    worn.stock.crops = 40;
    worn.stock.machinery = 0;
    e.rubles = 5_000;
    e.repairImportsEnabled = true;
    suppressObjectiveRewards(e);

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      customsMachinery: e.stockOf(customs, 'machinery'),
      wornMachinery: e.stockOf(worn, 'machinery'),
      wornIncoming: e.incomingOf(worn, 'machinery'),
      rubles: e.rubles,
      imported: e.stats.imported.machinery,
      repairImports: e.tradeLedger.today.repairImports,
    }).toEqual({
      trucks: [{
        id: 1, srcId: 1, destId: 3, cargo: 'machinery', amount: 3,
        points: [{ x: 7, y: 11 }, ...horizontalRoad(7, 12), { x: 12.5, y: 10.5 }],
        daysTotal: 1.08, daysDone: 0, phase: 'go',
      }],
      customsMachinery: 0,
      wornMachinery: 0,
      wornIncoming: 3,
      rubles: 4_616,
      imported: 3,
      repairImports: -384,
    });
  });

  it('keeps auto-bought materials pinned to their bonded customs without touching real stock', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 22, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 7, 10);
    const customs = placeBuilt(e, 'customs', 9, 10);
    depot.stock.planks = 50;
    depot.stock.bricks = 50;
    customs.stock.planks = 77;
    customs.stock.bricks = 79;
    e.rubles = 100_000;
    suppressObjectiveRewards(e);
    expect(e.tryPlace('house', 12, 10, { autoBuy: true }).ok).toBe(true);
    const site = e.buildingAt(12, 10)!;
    const afterPlacement = e.rubles;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      rubles: e.rubles,
      afterPlacement,
      depot: { planks: e.stockOf(depot, 'planks'), bricks: e.stockOf(depot, 'bricks') },
      customs: { planks: e.stockOf(customs, 'planks'), bricks: e.stockOf(customs, 'bricks') },
      site: { planks: e.stockOf(site, 'planks'), bricks: e.stockOf(site, 'bricks'), incoming: { ...site.incoming } },
    }).toEqual({
      trucks: [
        {
          id: 1, srcId: 3, destId: 4, cargo: 'planks', amount: 6,
          points: [{ x: 10, y: 11 }, ...horizontalRoad(10, 12), { x: 12.5, y: 10.5 }],
          daysTotal: 0.6, daysDone: 0, phase: 'go',
        },
        {
          id: 2, srcId: 3, destId: 4, cargo: 'bricks', amount: 4,
          points: [{ x: 10, y: 11 }, ...horizontalRoad(10, 12), { x: 12.5, y: 10.5 }],
          daysTotal: 0.6, daysDone: 0, phase: 'go',
        },
      ],
      rubles: 99_914,
      afterPlacement: 99_914,
      depot: { planks: 50, bricks: 50 },
      customs: { planks: 77, bricks: 79 },
      site: { planks: 0, bricks: 0, incoming: { planks: 6, bricks: 4 } },
    });
  });

  it('auto-export excludes both the destination itself and every other customs source, then stops at the truck budget', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    const destinationCustoms = placeBuilt(e, 'customs', 5, 10);
    const otherCustoms = placeBuilt(e, 'customs', 11, 10);
    const warehouse = placeBuilt(e, 'depot', 20, 10);
    placeBuilt(e, 'constructionOffice', 25, 10);
    destinationCustoms.stock.steel = 30;
    otherCustoms.stock.steel = 30;
    warehouse.stock.steel = 60;
    e.setAutoTradeEnabled(true);
    e.setAutoTradeRule('steel', { mode: 'export', level: 20, currency: 'east' });
    suppressObjectiveRewards(e);

    runDays(e, 1);

    const expectedPath = [
      { x: 21, y: 11 },
      ...horizontalRoad(6, 20).reverse(),
      { x: 6, y: 11 },
    ];
    expect({
      trucks: e.trucks.map(truckTrace),
      destination: { stock: e.stockOf(destinationCustoms, 'steel'), incoming: e.incomingOf(destinationCustoms, 'steel') },
      otherCustoms: e.stockOf(otherCustoms, 'steel'),
      warehouse: e.stockOf(warehouse, 'steel'),
      fleet: e.fleetStatus(),
    }).toEqual({
      trucks: Array.from({ length: 6 }, (_, i) => ({
        id: i + 1, srcId: 3, destId: 1, cargo: 'steel', amount: 6,
        points: expectedPath,
        daysTotal: 2.6999999999999997, daysDone: 0, phase: 'go',
      })),
      destination: { stock: 30, incoming: 36 },
      otherCustoms: 30,
      warehouse: 24,
      fleet: {
        active: 6, max: 6, officeTrucks: 6, driverTrucks: 0,
        depotTrucks: 0, fuelCap: 0, gasFuel: 0, fuelDaysLeft: Infinity,
      },
    });
  });

  it('pins an overflow haul to its producer rather than another stocked source', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 25, 9);
    const depot = placeBuilt(e, 'depot', 20, 10);
    placeBuilt(e, 'constructionOffice', 15, 10);
    e.applyTilePatches([{ x: 10, y: 10, deposit: 'coal' }]);
    const mine = placeBuilt(e, 'coalMine', 10, 10);
    mine.stock.coal = 55;
    depot.stock.coal = 30;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      mine: { stock: e.stockOf(mine, 'coal'), incoming: e.incomingOf(mine, 'coal') },
      depot: { stock: e.stockOf(depot, 'coal'), incoming: e.incomingOf(depot, 'coal') },
    }).toEqual({
      trucks: [{
        id: 1, srcId: 3, destId: 1, cargo: 'coal', amount: 6,
        points: [{ x: 10.5, y: 10.5 }, ...horizontalRoad(10, 20), { x: 21, y: 11 }],
        daysTotal: 1.98, daysDone: 0, phase: 'go',
      }],
      mine: { stock: 49, incoming: 0 },
      depot: { stock: 30, incoming: 6 },
    });
  });

  it('dispatches a construction remainder below one unit without rounding it away', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    depot.stock.gravel = 40;
    expect(e.tryPlace('road', 15, 9).ok).toBe(true);
    const site = e.buildingAt(15, 9)!;
    site.stock.gravel = 1.4;

    runDays(e, 1);

    expect({
      trucks: e.trucks.map(truckTrace),
      depot: e.stockOf(depot, 'gravel'),
      site: { stock: e.stockOf(site, 'gravel'), incoming: e.incomingOf(site, 'gravel') },
    }).toEqual({
      trucks: [{
        id: 1, srcId: 1, destId: 3, cargo: 'gravel', amount: 0.6000000000000001,
        points: [{ x: 6, y: 11 }, ...horizontalRoad(6, 14), { x: 15.5, y: 9.5 }],
        daysTotal: 1.6199999999999999, daysDone: 0, phase: 'go',
      }],
      depot: 39.4,
      site: { stock: 1.4, incoming: 0.6000000000000001 },
    });
  });

  it('pins relay orders, far-shore truck legs, and reverse-order boat dispatch traces', () => {
    const e = makeEngine();
    carveChannel(e, 20, 22);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    const westPort = placeBuilt(e, 'port', 18, 10);
    layRoad(e, 4, 9, 19, 9);
    depot.stock.planks = 60;
    depot.stock.bricks = 60;
    const eastPort = placeBuilt(e, 'port', 23, 10);
    layRoad(e, 23, 9, 32, 9);
    expect(e.tryPlace('house', 30, 10).ok).toBe(true);
    const site = e.buildingAt(30, 10)!;

    runDays(e, 1);
    expect({
      orders: e.serialize().body.boatOrders,
      trucks: e.trucks.map(truckTrace),
      westIncoming: { ...westPort.incoming },
      eastIncoming: { ...eastPort.incoming },
    }).toEqual({
      orders: [
        { srcId: 3, destId: 4, r: 'planks', amt: 6 },
        { srcId: 3, destId: 4, r: 'bricks', amt: 4 },
      ],
      trucks: [
        {
          id: 1, srcId: 1, destId: 3, cargo: 'planks', amount: 6,
          points: [{ x: 6, y: 11 }, ...horizontalRoad(6, 18), { x: 19, y: 11 }],
          daysTotal: 2.34, daysDone: 0, phase: 'go',
        },
        {
          id: 2, srcId: 1, destId: 3, cargo: 'bricks', amount: 4,
          points: [{ x: 6, y: 11 }, ...horizontalRoad(6, 18), { x: 19, y: 11 }],
          daysTotal: 2.34, daysDone: 0, phase: 'go',
        },
      ],
      westIncoming: { planks: 6, bricks: 4 },
      eastIncoming: {},
    });

    runDays(e, 3);
    expect({
      orders: e.serialize().body.boatOrders,
      boats: e.boats.map(truckTrace),
      west: { planks: e.stockOf(westPort, 'planks'), bricks: e.stockOf(westPort, 'bricks'), incoming: { ...westPort.incoming } },
      east: { planks: e.stockOf(eastPort, 'planks'), bricks: e.stockOf(eastPort, 'bricks'), incoming: { ...eastPort.incoming } },
      siteIncoming: { ...site.incoming },
    }).toEqual({
      orders: [],
      boats: [
        {
          id: 1, srcId: 3, destId: 4, cargo: 'bricks', amount: 4,
          points: [{ x: 19, y: 11 }, { x: 20, y: 10 }, { x: 21, y: 10 }, { x: 22, y: 10 }, { x: 24, y: 11 }],
          daysTotal: 1, daysDone: 0, phase: 'go',
        },
        {
          id: 2, srcId: 3, destId: 4, cargo: 'planks', amount: 6,
          points: [{ x: 19, y: 11 }, { x: 20, y: 10 }, { x: 21, y: 10 }, { x: 22, y: 10 }, { x: 24, y: 11 }],
          daysTotal: 1, daysDone: 0, phase: 'go',
        },
      ],
      west: { planks: 0, bricks: 0, incoming: { planks: 0, bricks: 0 } },
      east: { planks: 0, bricks: 0, incoming: { bricks: 4, planks: 6 } },
      siteIncoming: {},
    });
  });

  it('dispatches off-road (weighted land) when no road connects supplier to destination', () => {
    const e = makeEngine();
    // No roads anywhere: the whole flat map is one off-road land component, so the
    // road-first search finds nothing and dispatch falls back to weighted terrain.
    const supplier = placeBuilt(e, 'warehouse', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10); // provides the truck budget
    const store = placeBuilt(e, 'store', 12, 10);
    supplier.stock.food = 20;

    runDays(e, 1);

    // The only dispatch path is off-road land. It threads grass tiles at y=9 (the
    // buildings' top-edge access), NOT a road: daysTotal is the weighted travel
    // (7 steps × offRoadStepCost 8 × truckDaysPerTile 0.18 = 10.08), far above the
    // ~1.4 a road hop-count would give — this is the branch no road test exercises.
    expect(e.trucks.map(truckTrace)).toEqual([{
      id: 1, srcId: supplier.id, destId: store.id, cargo: 'food', amount: 6,
      points: [
        { x: 5.5, y: 10.5 },
        ...Array.from({ length: 8 }, (_, i) => ({ x: 5 + i, y: 9 })),
        { x: 12.5, y: 10.5 },
      ],
      daysTotal: 10.08, daysDone: 0, phase: 'go',
    }]);
    expect(e.incomingOf(store, 'food')).toBe(6);
    expect(e.stockOf(supplier, 'food')).toBe(14);
  });
});
