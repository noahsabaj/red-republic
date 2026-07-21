import { describe, expect, it } from 'vitest';
import { makeEngine, placeBuilt, layRoad, runDays } from './helpers';

/**
 * The haulage fleet scales with the Republic instead of being bolted to
 * construction. Construction Offices give a fuel-free bootstrap fleet; Motor
 * Depots add a truck per staffed driver; Gas Stations fuel that depot fleet.
 * Build depots and keep them fuelled to grow. Offices/BALANCE untouched, so the
 * campaign-pacing tripwire (office-only) still passes unchanged.
 */

describe('fleet scaling', () => {
  it('Motor Depots add trucks (gated by Gas Station fuel); offices stay fuel-free', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    const office = placeBuilt(e, 'constructionOffice', 8, 10);
    const motor = placeBuilt(e, 'motorDepot', 11, 10);
    const gas = placeBuilt(e, 'gasStation', 15, 10);
    runDays(e, 2); // settle connectivity (pop 0 → staff 0; we crew by hand and read live)
    office.staff = 10; motor.staff = 16; gas.stock.fuel = 0;
    let f = e.fleetStatus();
    expect(f.officeTrucks).toBeGreaterThan(0); // fuel-free base fleet
    expect(f.driverTrucks).toBe(16);           // drivers crewed
    expect(f.depotTrucks).toBe(0);             // ...but no fuel → none run
    expect(f.max).toBe(f.officeTrucks);        // only the office base counts

    gas.stock.fuel = 60;                       // fuel the station
    f = e.fleetStatus();
    expect(f.depotTrucks).toBe(16);            // depot fleet now runs
    expect(f.max).toBe(f.officeTrucks + 16);

    motor.staff = 0;                           // no drivers → no depot trucks
    expect(e.fleetStatus().driverTrucks).toBe(0);
  });
});

describe('fleet fuel', () => {
  it('the depot fleet burns Gas Station fuel as it hauls', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'motorDepot', 9, 10);   // no office → every active truck is a fuel-burning depot truck
    const gas = placeBuilt(e, 'gasStation', 13, 10);
    const factory = placeBuilt(e, 'foodFactory', 22, 10);
    placeBuilt(e, 'apartment', 34, 10);
    placeBuilt(e, 'apartment', 37, 10);
    e.pop = 80;
    depot.stock.crops = 800;   // a haulable demand source; NO fuel here, so the station never refills
    factory.stock.crops = 0;
    gas.stock.fuel = 60;
    runDays(e, 20);
    expect(gas.stock.fuel).toBeLessThan(60);            // fuel consumed by the running fleet
    expect(gas.stock.fuel).toBeGreaterThanOrEqual(0);   // never negative
  });

  it('office trucks refill an empty Gas Station, bringing the depot fleet online (no deadlock)', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 40, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    placeBuilt(e, 'motorDepot', 11, 10);
    const gas = placeBuilt(e, 'gasStation', 15, 10);
    placeBuilt(e, 'apartment', 34, 10);
    placeBuilt(e, 'apartment', 37, 10);
    e.pop = 80;
    depot.stock.fuel = 400;   // domestic fuel to distribute
    gas.stock.fuel = 0;       // station starts dry — chicken-and-egg unless offices bootstrap
    runDays(e, 15);
    expect(gas.stock.fuel).toBeGreaterThan(0);              // office (fuel-free) trucks hauled fuel in
    expect(e.fleetStatus().depotTrucks).toBeGreaterThan(0); // so the depot fleet can now run
  });
});

describe('fleet advisories', () => {
  it('warns when Motor Depots are crewed but starved of fuel', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    placeBuilt(e, 'motorDepot', 11, 10);
    placeBuilt(e, 'gasStation', 15, 10); // never fuelled
    placeBuilt(e, 'apartment', 34, 10);
    e.pop = 60;
    runDays(e, 3);
    expect(e.fleetStatus().driverTrucks).toBeGreaterThan(0);
    expect(e.alerts.some(a => a.id === 'fleetFuel')).toBe(true);
  });
});
