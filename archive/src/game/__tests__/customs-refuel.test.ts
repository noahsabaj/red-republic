import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import { BALANCE, OBJECTIVES } from '../config';
import { flatBorderMap, CALM_WEATHER, layRoad, placeBuilt, runDays } from './helpers';

/**
 * Emergency fuel is the bootstrap breaker: fuel is hauled BY lorries, so a
 * republic whose pumps all ran dry cannot haul itself out. It buys a few tons
 * across the border where a grounded lorry can still reach them.
 *
 * Every assertion here drives the PUBLIC surface — `runDays`, engine fields —
 * because the bug this feature shipped with was a gate in `logistics()` that a
 * test calling the private method directly could never have seen.
 */
function borderTown() {
  const e = new GameEngine({
    seed: 1, map: flatBorderMap(), skipStartingBase: true, weatherScript: CALM_WEATHER,
  });
  const D = BALANCE.borderDepth;
  layRoad(e, D, 24, 30, 24);
  const customs = placeBuilt(e, 'customs', D, 22);
  const depot = placeBuilt(e, 'depot', 8, 25);
  const office = placeBuilt(e, 'constructionOffice', 12, 25);
  // An emergency import is still an import: it clears the border like any
  // other, so the customs house has to be crewed to pass anything through.
  placeBuilt(e, 'apartment', 20, 25);
  placeBuilt(e, 'apartment', 24, 25);
  e.pop = 80;
  office.stock.fuel = 0; // the helper's garage ration removed — these are fuel tests
  return { e, customs, depot, office };
}

describe('emergency fuel auto-buy', () => {
  it('fires when every pump is dry, even though the fleet is grounded and dispatch has no budget', () => {
    // The bug: the call sat AFTER `if (budget <= 0) return;`, so the one state
    // it exists to rescue — no fuel, therefore no working fleet — disarmed it.
    const { e, customs, office } = borderTown();
    e.rubles = 100_000;
    office.stock.fuel = 0;
    customs.stock.fuel = 0;
    runDays(e, 1);

    expect(e.trucks.length).toBeGreaterThan(0);
    expect(e.fleetStatus().pumpFuel).toBe(0);          // nothing could move…
    expect(customs.stock.fuel ?? 0).toBeGreaterThan(0); // …and fuel still arrived
  });

  it('books like any other import: treasury, ledger and customs throughput all agree', () => {
    const { e, customs, office } = borderTown();
    e.objectivesDone = OBJECTIVES.map(o => o.id); // reward income would muddy the treasury delta
    e.rubles = 100_000;
    office.stock.fuel = 0;
    customs.stock.fuel = 0;
    const before = e.rubles;
    runDays(e, 1);

    const led = e.tradeLedger.today;
    const bought = led.imports.fuel ?? 0;
    expect(bought).toBeGreaterThan(0);
    const cost = bought * e.importPriceOf('fuel', 'east');
    expect(before - e.rubles).toBeCloseTo(cost, 6);   // the treasury actually paid
    expect(led.rubles).toBeCloseTo(-cost, 6);         // and the day's page says so
    expect(led.used).toBeGreaterThanOrEqual(bought);  // it consumed border capacity
    expect(e.stats.imported.fuel ?? 0).toBeGreaterThanOrEqual(bought);
  });

  it('respects the treasury reserve and never overstocks the customs house', () => {
    const { e, customs, office } = borderTown();
    e.objectivesDone = OBJECTIVES.map(o => o.id); // no reward income to muddy the floor
    e.rubles = e.autoTrade.reserveRubles;         // nothing spendable above it
    office.stock.fuel = 0;
    customs.stock.fuel = 0;
    runDays(e, 3);
    expect(customs.stock.fuel ?? 0).toBe(0);
    expect(e.rubles).toBe(e.autoTrade.reserveRubles);

    e.rubles = 100_000;
    runDays(e, 30);
    expect(customs.stock.fuel ?? 0).toBeLessThanOrEqual(BALANCE.emergencyFuelTarget + 1e-6);
  });

  it('stays out of the way once the republic fuels itself', () => {
    const { e, customs, office } = borderTown();
    e.rubles = 100_000;
    office.stock.fuel = 60; // pumps are supplied
    customs.stock.fuel = 0;
    runDays(e, 5);
    expect(customs.stock.fuel ?? 0).toBe(0);
    expect(e.stats.imported.fuel ?? 0).toBe(0);
  });

  it('can be switched off, and the switch round-trips through a save', () => {
    const { e, customs, office } = borderTown();
    e.rubles = 100_000;
    office.stock.fuel = 0;
    customs.stock.fuel = 0;
    e.toggleEmergencyFuelAutoBuy();
    expect(e.emergencyFuelAutoBuy).toBe(false);
    runDays(e, 5);
    expect(customs.stock.fuel ?? 0).toBe(0);

    expect(GameEngine.fromSave(e.serialize()).emergencyFuelAutoBuy).toBe(false);
  });
});

describe('the border reserve as a last-resort pump', () => {
  it('a grounded lorry drives to the Customs House when nothing else has fuel', () => {
    const { e, customs, office } = borderTown();
    e.objectivesDone = OBJECTIVES.map(o => o.id); // no reward income to buy more fuel with
    e.rubles = 0; // no emergency buying — the fuel is already there
    office.stock.fuel = 0;
    customs.stock.fuel = 40;
    runDays(e, 1); // connectivity is computed on the first simulated day
    expect(e.fleetFuelInfo().usingCustomsFuel).toBe(true);

    runDays(e, 12);
    expect(e.trucks.some(v => v.fuel > 0)).toBe(true);
    expect(customs.stock.fuel ?? 0).toBeLessThan(40);
  });
});
