import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import type { SaveGameV1 } from '../save-format';
import { CALM_WEATHER, flatBorderMap, layRoad, makeEngine, placeBuilt, runDays } from './helpers';

function stable(save: SaveGameV1): SaveGameV1 {
  return { ...save, header: { ...save.header, savedAt: 0 } };
}

function expectSameState(a: GameEngine, b: GameEngine) {
  expect(stable(b.serialize())).toEqual(stable(a.serialize()));
}

describe('routing topology cache invalidation', () => {
  it('rebuilds road and land topology once after road completion and again after removal', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    e.dollars = 1e9;
    expect(e.tryPlace('road', 15, 9, { plan: true }).ok).toBe(true);
    const site = e.buildingAt(15, 9)!;

    runDays(e, 1);
    const beforeCompletion = e.getRoutingDiagnostics().topologyRebuilds;
    expect(e.finishSiteInstant(site.id).ok).toBe(true);
    expect(e.tiles[9][15].road).toBe(true);
    runDays(e, 1);
    const afterCompletion = e.getRoutingDiagnostics().topologyRebuilds;
    expect(afterCompletion).toEqual({
      road: beforeCompletion.road + 1,
      land: beforeCompletion.land + 1,
      water: beforeCompletion.water,
    });

    expect(e.bulldozeAt(15, 9)).toBe(true);
    expect(e.tiles[9][15].road).toBe(false);
    runDays(e, 1);
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual({
      road: afterCompletion.road + 1,
      land: afterCompletion.land + 1,
      water: afterCompletion.water,
    });
  });

  it('invalidates only land topology when a planned footprint occupies open terrain', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    runDays(e, 1);
    const before = e.getRoutingDiagnostics().topologyRebuilds;

    expect(e.tryPlace('house', 20, 20, { plan: true }).ok).toBe(true);
    const site = e.buildingAt(20, 20)!;
    runDays(e, 1);

    expect(site.connected).toBe(true);
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual({
      road: before.road,
      land: before.land + 1,
      water: before.water,
    });
  });

  it('invalidates warm road and land caches once when customs placement lays its border lane', () => {
    const map = flatBorderMap();
    const e = new GameEngine({ seed: 1, map, skipStartingBase: true, weatherScript: CALM_WEATHER });
    layRoad(e, 8, 9, 15, 9);
    placeBuilt(e, 'depot', 10, 10);
    runDays(e, 1);
    const before = e.getRoutingDiagnostics().topologyRebuilds;
    expect(before.road).toBeGreaterThan(0);
    expect(before.land).toBeGreaterThan(0);

    const customsX = map.crossX!;
    const customsY = 10;
    e.dollars = 1e9;
    expect(e.canPlace('customs', customsX, customsY)).toEqual({ ok: true });
    expect(e.tryPlace('customs', customsX, customsY, { instant: true }).ok).toBe(true);
    for (let x = 0; x < customsX; x++) {
      expect(e.tiles[customsY][x]).toMatchObject({ foreign: true, road: true });
    }
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual(before); // invalidated, still lazy

    runDays(e, 1);
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual({
      road: before.road + 1,
      land: before.land + 1,
      water: before.water,
    });
  });

  it('recomputes connectivity on facility completion without rebuilding unchanged topology', () => {
    const e = makeEngine();
    e.applyTilePatches(Array.from({ length: e.mapH }, (_, y) => ({ x: 20, y, terrain: 'water' as const })));
    layRoad(e, 4, 9, 15, 9);
    layRoad(e, 25, 9, 35, 9);
    e.dollars = 1e9;
    expect(e.tryPlace('depot', 5, 10, { plan: true }).ok).toBe(true);
    const depotSite = e.buildingAt(5, 10)!;
    const isolated = placeBuilt(e, 'store', 30, 10);

    runDays(e, 1);
    expect(isolated.connected).toBe(true); // legacy no-depot fallback
    const before = e.getRoutingDiagnostics().topologyRebuilds;

    expect(e.finishSiteInstant(depotSite.id).ok).toBe(true);
    runDays(e, 1);

    expect(depotSite.constructed).toBe(true);
    expect(isolated.connected).toBe(false);
    expect(isolated.roadConnected).toBe(false);
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual(before);
  });

  it('routes delivered site stock after bulldozing a warm-cache footprint', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    expect(e.tryPlace('sawmill', 12, 12, { plan: true }).ok).toBe(true);
    const site = e.buildingAt(12, 12)!;
    site.stock.bricks = 5;
    runDays(e, 1);
    const before = e.getRoutingDiagnostics().topologyRebuilds;

    expect(e.bulldozeAt(12, 12)).toBe(true);
    const refund = e.trucks.find(t => t.cargo === 'bricks' && t.destId === depot.id);
    expect(refund).toBeDefined();
    expect(refund).toMatchObject({ amount: 5, srcId: depot.id, phase: 'go' });
    expect(e.stockOf(depot, 'bricks')).toBe(0); // routed, not direct-salvaged
    expect(e.incomingOf(depot, 'bricks')).toBe(5);

    runDays(e, 1);
    expect(e.getRoutingDiagnostics().topologyRebuilds.land).toBe(before.land + 1);
  });

  it('a bridge (road on water) rebuilds only the road domain — cost-derived invalidation', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 14, 9);
    placeBuilt(e, 'depot', 5, 10);
    e.applyTilePatches([{ x: 20, y: 9, terrain: 'water' as const }]); // a lone water tile
    runDays(e, 1); // warm road + land caches (the carve already rebuilt land once)
    const before = e.getRoutingDiagnostics().topologyRebuilds;

    // Lay a bridge (road) on the water tile. Land stays impassable there (still water),
    // so the land network is unchanged — the hand-coded map used to dirty land on ANY
    // road change; the cost functions now skip it.
    e.applyTilePatches([{ x: 20, y: 9, road: true }]);
    runDays(e, 1);
    expect(e.getRoutingDiagnostics().topologyRebuilds).toEqual({
      road: before.road + 1,
      land: before.land, // NOT before.land + 1 — the bridge never touches land routing
      water: before.water,
    });
  });
});

describe('routing caches and deterministic save futures', () => {
  it('loads warm source caches cold, then stays bit-exact across routing and a shared topology mutation', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    placeBuilt(e, 'store', 25, 10);
    depot.stock.food = 100;
    runDays(e, 1);

    expect(e.trucks.some(t => t.phase === 'go' && t.cargo === 'food')).toBe(true);
    const sourceRebuilds = e.getRoutingDiagnostics().topologyRebuilds;
    expect(sourceRebuilds.road).toBeGreaterThan(0);
    expect(sourceRebuilds.land).toBeGreaterThan(0);

    const snapshot = e.serialize();
    const loaded = GameEngine.fromSave(snapshot, { weatherScript: CALM_WEATHER });
    expect(loaded.getRoutingDiagnostics().topologyRebuilds).toEqual({ road: 0, land: 0, water: 0 });
    expectSameState(e, loaded);

    for (let day = 0; day < 30; day++) {
      runDays(e, 1);
      runDays(loaded, 1);
      expectSameState(e, loaded);
    }
    expect(loaded.getRoutingDiagnostics().topologyRebuilds.road).toBeGreaterThan(0);
    expect(loaded.getRoutingDiagnostics().topologyRebuilds.land).toBeGreaterThan(0);

    expect(e.bulldozeAt(15, 9)).toBe(true);
    expect(loaded.bulldozeAt(15, 9)).toBe(true);
    for (let day = 0; day < 15; day++) {
      runDays(e, 1);
      runDays(loaded, 1);
      expectSameState(e, loaded);
    }
  });
});
