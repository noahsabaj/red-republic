import { describe, expect, it } from 'vitest';
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
  it.each([{ size: 96 }, { size: 128 }])('rejects a $size×$size, 120-demand shortage storm without running pathfinding', ({ size }) => {
    const e = largeEngine(size);
    layRoad(e, 1, 1, 3, 1);
    placeBuilt(e, 'constructionOffice', 2, 2);
    for (let i = 0; i < 120; i++) {
      const x = 5 + (i % 30) * 3;
      const y = 5 + Math.floor(i / 30) * 3;
      placeBuilt(e, 'sawmill', x, y);
    }

    runDays(e, 1); // build and warm every derived topology/index cache
    const warmedRebuilds = e.getRoutingDiagnostics().topologyRebuilds;
    runDays(e, 1);

    expect(work(e.getRoutingDiagnostics())).toEqual({
      demandsConsidered: 120,
      successfulDispatches: 0,
      componentRejections: 240,
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
    expect(inflated.trucks).toHaveLength(1);
    expect(base.trucks).toHaveLength(1);
  });
});
