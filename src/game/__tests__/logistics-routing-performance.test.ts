import { describe, expect, it } from 'vitest';
import { BALANCE } from '../config';
import { GameEngine } from '../engine';
import type { RoutingDiagnostics } from '../engine';
import { CALM_WEATHER, flatMap, layRoad, placeBuilt, runDays } from './helpers';

function largeEngine(size: number) {
  return new GameEngine({
    seed: 1,
    map: flatMap(size, size),
    skipStartingBase: true,
    weatherScript: CALM_WEATHER,
  });
}

function work(d: RoutingDiagnostics) {
  return {
    demandsConsidered: d.demandsConsidered,
    successfulDispatches: d.successfulDispatches,
    componentRejections: d.componentRejections,
    roadSearches: d.roadSearches,
    landSearches: d.landSearches,
    waterSearches: d.waterSearches,
    supplierCandidatesChecked: d.supplierCandidatesChecked,
    settledTiles: d.settledTiles,
    pathsMaterialized: d.pathsMaterialized,
  };
}

describe('logistics routing performance regressions', () => {
  it.each([{ size: 96, demands: 120 }, { size: 128, demands: 120 }, { size: 128, demands: 400 }])(
    'bounds a $size×$size, $demands-demand shortage storm to a few attempts per free lorry',
    ({ size, demands }) => {
      const e = largeEngine(size);
      layRoad(e, 1, 1, 3, 1);
      const office = placeBuilt(e, 'constructionOffice', 2, 2);
      office.stock.fuel = 60;
      for (let i = 0; i < demands; i++) {
        const x = 5 + (i % 30) * 3;
        const y = 5 + Math.floor(i / 30) * 3;
        placeBuilt(e, 'sawmill', x, y);
      }

      runDays(e, 1); // build and warm every derived topology/index cache
      const warmedRebuilds = e.getRoutingDiagnostics().topologyRebuilds;
      runDays(e, 1);

      // The cap is per FREE LORRY, not per demand: both passes share one
      // attempt counter, so a longer queue costs nothing extra. Before that,
      // pass 2 walked the whole list and ran a full pathfinding flood on
      // every entry pass 1 had not already retired.
      const fleet = e.trucks.length;
      const maxAttempts = Math.max(8, fleet * BALANCE.logisticsCandidateFactor);
      const d = work(e.getRoutingDiagnostics());
      expect(fleet).toBeGreaterThan(0);
      expect(d.demandsConsidered).toBeLessThanOrEqual(maxAttempts);
      expect(d).toMatchObject({
        successfulDispatches: 0,
        roadSearches: 0,
        landSearches: 0,
        waterSearches: 0,
        supplierCandidatesChecked: 0,
        settledTiles: 0,
        pathsMaterialized: 0,
      });

      runDays(e, 30);
      expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual(warmedRebuilds);
    });

  it('the attempt cap does not grow with the size of the demand queue', () => {
    const storm = (demands: number) => {
      const e = largeEngine(128);
      layRoad(e, 1, 1, 3, 1);
      const office = placeBuilt(e, 'constructionOffice', 2, 2);
      office.stock.fuel = 60;
      for (let i = 0; i < demands; i++) {
        placeBuilt(e, 'sawmill', 5 + (i % 30) * 3, 5 + Math.floor(i / 30) * 3);
      }
      runDays(e, 2);
      return e.getRoutingDiagnostics().demandsConsidered;
    };
    expect(storm(400)).toBe(storm(120)); // same fleet ⇒ same work, whatever the queue
  });

  it('does not inspect or settle more routing work when 200 irrelevant buildings are added', () => {
    const town = (inflate: boolean) => {
      const e = largeEngine(64);
      layRoad(e, 4, 9, 30, 9);
      const warehouse = placeBuilt(e, 'warehouse', 5, 10);
      placeBuilt(e, 'constructionOffice', 8, 10);
      placeBuilt(e, 'store', 20, 10);
      warehouse.stock.food = 20;
      if (inflate) {
        for (let i = 0; i < 200; i++) {
          const x = 2 + (i % 25) * 2;
          const y = 20 + Math.floor(i / 25) * 2;
          placeBuilt(e, 'house', x, y);
        }
      }
      runDays(e, 1);
      return e;
    };

    const base = town(false);
    const inflated = town(true);

    expect(work(inflated.getRoutingDiagnostics())).toEqual(work(base.getRoutingDiagnostics()));
    // one lorry left the garage; the rest of the fleet is parked, not absent
    expect(inflated.trucks.filter(v => v.state !== 'idle')).toHaveLength(1);
    expect(base.trucks.filter(v => v.state !== 'idle')).toHaveLength(1);
  });
});
