import { describe, expect, it } from 'vitest';
import { BALANCE } from '../config';
import { GameEngine } from '../engine';
import { makeEngine, placeBuilt, layRoad, runDays } from './helpers';

/**
 * The road fleet is made of VEHICLES, not shipments: a Construction Office or
 * Motor Depot owns its lorries, each carries its own tank, and fuel leaves a
 * building's bin exactly once — when a lorry pumps it. These tests pin the
 * properties that distinguish that from the old pooled model, where a "truck"
 * was created at dispatch, deleted on return, and fuel was levied per day
 * against a town-wide total.
 */

/** Total fuel in the republic: every bin plus every tank. */
function fuelEverywhere(e: ReturnType<typeof makeEngine>): number {
  let total = 0;
  for (const b of e.buildings.values()) total += b.stock.fuel ?? 0;
  for (const v of e.trucks) total += v.fuel;
  return total;
}

describe('fleet composition', () => {
  it('a garage owns its lorries: staffing creates them, demolition retires them', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    runDays(e, 2);
    const officeFleet = e.trucks.filter(v => v.homeId === office.id).length;
    expect(officeFleet).toBe(e.trucksFrom(office));
    expect(officeFleet).toBeGreaterThan(0);

    const motor = placeBuilt(e, 'motorDepot', 11, 10);
    motor.staff = 16;
    runDays(e, 1);
    expect(e.trucks.filter(v => v.homeId === motor.id).length).toBe(e.trucksFrom(motor));

    e.bulldozeAt(11, 10);
    runDays(e, 1);
    expect(e.trucks.some(v => v.homeId === motor.id)).toBe(false);
  });

  it('fleetStatus counts real vehicles, not a fuel-derived capacity', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    runDays(e, 2);
    const f = e.fleetStatus();
    expect(f.max).toBe(e.trucks.length);
    expect(f.active + f.idle).toBe(f.max);
  });
});

describe('per-vehicle fuel', () => {
  it('a new lorry arrives dry, fills at a pump, and the pump loses exactly what went in the tank', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    office.stock.fuel = 0; // a garage with a dry bin — the helper's ration removed
    runDays(e, 1);
    expect(e.trucks.length).toBeGreaterThan(0);
    expect(e.trucks.every(v => v.fuel === 0)).toBe(true); // no fairy-dust tanks

    office.stock.fuel = 60;
    const before = fuelEverywhere(e);
    runDays(e, 1);
    expect(e.trucks.some(v => v.fuel > 0)).toBe(true);
    // filling moves fuel, it does not create or destroy it (bar what was driven)
    expect(fuelEverywhere(e)).toBeLessThanOrEqual(before + 1e-9);
  });

  it('driving is the ONLY thing that consumes fuel — no second, pooled levy', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const factory = placeBuilt(e, 'foodFactory', 22, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 60;
    runDays(e, 3);

    const fuelBefore = fuelEverywhere(e);
    const odoBefore = e.trucks.reduce((n, v) => n + v.odometer, 0);
    runDays(e, 10);
    const burned = fuelBefore - fuelEverywhere(e);
    const driven = e.trucks.reduce((n, v) => n + v.odometer, 0) - odoBefore;
    expect(driven).toBeGreaterThan(0);
    // every unit burned is accounted for by a tile driven, to floating slop.
    // Under the old double-charging model this ratio was ~2x.
    expect(burned).toBeCloseTo(driven * BALANCE.vehicleFuelPerTile, 6);
  });

  it('an idle lorry burns nothing', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    office.stock.fuel = 60;
    runDays(e, 3); // fleet fills up, nothing to haul
    const fuel = fuelEverywhere(e);
    runDays(e, 20);
    expect(fuelEverywhere(e)).toBeCloseTo(fuel, 6);
  });

  it('a lorry will not accept a job it has not the fuel to finish', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const factory = placeBuilt(e, 'foodFactory', 30, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 0;
    runDays(e, 4); // no fuel anywhere at all

    expect(e.trucks.length).toBeGreaterThan(0);
    expect(e.trucks.every(v => v.state === 'idle')).toBe(true); // grounded, never stranded
    expect(e.fleetStatus().grounded).toBe(e.trucks.length);
    expect(e.alerts.some(a => a.id === 'fleetFuel')).toBe(true);
  });

  it('a grounded fleet recovers once fuel reaches a pump', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const factory = placeBuilt(e, 'foodFactory', 22, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 0;
    runDays(e, 3);
    expect(e.fleetStatus().grounded).toBeGreaterThan(0);

    office.stock.fuel = 60;
    runDays(e, 6);
    expect(e.fleetStatus().grounded).toBe(0);
    expect(factory.stock.crops ?? 0).toBeGreaterThan(0); // and hauling resumed
  });
});

describe('vehicle lifecycle', () => {
  it('a lorry parks where it finished and takes its next job from there', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const factory = placeBuilt(e, 'foodFactory', 22, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 60;
    runDays(e, 12);

    // Vehicles are never spliced away: the fleet size is the garage's, always.
    expect(e.trucks.length).toBe(e.trucksFrom(office));
    // and a parked one is standing at a real building, not mid-road
    for (const v of e.trucks) {
      if (v.state === 'idle') expect(e.buildings.has(v.atId)).toBe(true);
    }
  });

  it('never delivers a load into the pump it refuelled at', () => {
    // The old refuel router re-addressed a LOADED truck to a fuel station, so
    // its cargo was dumped there and the real destination kept a phantom
    // `incoming` reservation for ever. Refuelling is its own state now.
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const gas = placeBuilt(e, 'gasStation', 15, 10);
    const factory = placeBuilt(e, 'foodFactory', 30, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 8;
    gas.stock.fuel = 60;
    runDays(e, 40);

    expect(gas.stock.crops ?? 0).toBe(0);       // no cargo was ever dumped at the pump
    expect(office.stock.crops ?? 0).toBe(0);
    // and no destination is left holding a reservation nothing will ever fill
    const phantom = (factory.incoming.crops ?? 0) > 0
      && !e.trucks.some(v => v.destId === factory.id && v.state !== 'idle');
    expect(phantom).toBe(false);
  });

  it('survives a save round-trip with its tank, odometer and garage intact', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const factory = placeBuilt(e, 'foodFactory', 22, 10);
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 40;
    depot.stock.crops = 800;
    factory.stock.crops = 0;
    office.stock.fuel = 60;
    runDays(e, 8);

    const before = e.trucks.map(v => ({ id: v.id, homeId: v.homeId, fuel: v.fuel, odometer: v.odometer, state: v.state }));
    expect(before.some(v => v.odometer > 0)).toBe(true);

    const reloaded = GameEngine.fromSave(e.serialize());
    expect(reloaded.trucks.map(v => ({ id: v.id, homeId: v.homeId, fuel: v.fuel, odometer: v.odometer, state: v.state })))
      .toEqual(before);
  });
});

describe('off-road haulage', () => {
  it('costs offRoadStepCost× the fuel per map tile, exactly as it costs the time', () => {
    // Same haul, once on a road and once across open ground: the off-road run
    // burns proportionally more because legTiles IS the weighted travel cost.
    const run = (road: boolean) => {
      const e = makeEngine();
      if (road) layRoad(e, 4, 9, 20, 9);
      else layRoad(e, 4, 9, 9, 9); // just enough road to connect the base
      const depot = placeBuilt(e, 'depot', 5, 10);
      const office = placeBuilt(e, 'constructionOffice', 8, 10);
      const factory = placeBuilt(e, 'foodFactory', 18, 10);
      placeBuilt(e, 'apartment', 6, 12);
      e.pop = 40;
      depot.stock.crops = 400;
      factory.stock.crops = 0;
      office.stock.fuel = 60;
      runDays(e, 25);
      const delivered = factory.stock.crops ?? 0;
      const burned = e.trucks.reduce((n, v) => n + v.odometer, 0) * BALANCE.vehicleFuelPerTile;
      return { delivered, burned };
    };
    const onRoad = run(true);
    const offRoad = run(false);
    expect(onRoad.delivered).toBeGreaterThan(0);
    expect(offRoad.delivered).toBeGreaterThan(0);
    // fuel per unit delivered is dramatically worse without a road
    expect(offRoad.burned / offRoad.delivered).toBeGreaterThan(onRoad.burned / onRoad.delivered);
  });
});
