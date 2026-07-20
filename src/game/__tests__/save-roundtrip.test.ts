import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import { seedDemoTown } from '../demo';
import type { SaveGameV1 } from '../save-format';
import { CALM_WEATHER, flatMap, layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/** Serialized snapshots minus the wall-clock stamp. */
function stable(s: SaveGameV1): SaveGameV1 {
  return { ...s, header: { ...s.header, savedAt: 0 } };
}

describe('save round-trip', () => {
  it('a mature demo town survives serialize→fromSave bit-exact, including 90 more days', () => {
    const e = new GameEngine({ seed: 7 });
    seedDemoTown(e); // full town, 100 settled days, trucks in flight
    e.setAutoTradeEnabled(true);
    e.setAutoTradeRule('steel', { mode: 'export', level: 10, currency: 'west' });
    e.setAutoTradeRule('food', { mode: 'import', level: 20, currency: 'east' });
    const offer = e.contracts.find(c => c.state === 'offer');
    if (offer) e.acceptContract(offer.id);
    runDays(e, 20);

    const snap = e.serialize();
    const e2 = GameEngine.fromSave(snap);

    // identity block: readonly construction state came through
    expect(e2.seed).toBe(7);
    expect(e2.name).toBe(e.name);
    expect(e2.climate).toBe(e.climate);
    expect(e2.difficulty).toBe(e.difficulty);
    expect(e2.mapW).toBe(e.mapW);
    expect(e2.borderEdge).toBe(e.borderEdge);
    expect(e2.speed).toBe(0); // always loads paused

    // re-serializing the loaded engine reproduces the snapshot exactly
    expect(stable(e2.serialize())).toEqual(stable(snap));

    // the loaded engine's future is the original's future: 90 more days
    // (3 month boundaries → restored economy-rng drift + contract offers)
    runDays(e, 90);
    runDays(e2, 90);
    const a = stable(e.serialize());
    const b = stable(e2.serialize());
    expect(b.header).toEqual(a.header);
    expect(b.body.priceFactorEast).toBe(a.body.priceFactorEast);
    expect(b.body.priceFactorWest).toBe(a.body.priceFactorWest);
    expect(b.body.rngState).toBe(a.body.rngState);
    expect(b).toEqual(a); // and everything else, bit for bit
  });

  it('round-trips a borderless flat test map with a weather script', () => {
    const e = makeEngine();
    e.rubles = 1e9;
    layRoad(e, 10, 10, 30, 10);
    placeBuilt(e, 'depot', 12, 11);
    placeBuilt(e, 'constructionOffice', 15, 11);
    placeBuilt(e, 'house', 17, 11);
    placeBuilt(e, 'house', 18, 11);
    placeBuilt(e, 'powerPlant', 20, 11);
    placeBuilt(e, 'farm', 24, 11);
    runDays(e, 50);

    const snap = e.serialize();
    const e2 = GameEngine.fromSave(snap, { weatherScript: CALM_WEATHER });
    expect(e2.borderEdge).toBeNull();
    expect(stable(e2.serialize())).toEqual(stable(snap));

    runDays(e, 30);
    runDays(e2, 30);
    expect(stable(e2.serialize())).toEqual(stable(e.serialize()));
  });

  it('restores mid-winter weather state exactly (snow depth, river freeze, forecast)', () => {
    const e = new GameEngine({ seed: 7, climate: 'taiga', map: flatMap(), skipStartingBase: true });
    runDays(e, 330); // deep into the first winter
    const e2 = GameEngine.fromSave(e.serialize());
    expect(e2.weather).toEqual(e.weather);
    expect(e2.forecast(5)).toEqual(e.forecast(5));
  });

  it('round-trips in-flight road sites, a Machine Works, and a worn factory bit-exact', () => {
    const e = makeEngine();
    e.rubles = 1e9;
    layRoad(e, 4, 9, 20, 9);
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 8, 10);
    const works = placeBuilt(e, 'machineWorks', 10, 10);
    const factory = placeBuilt(e, 'foodFactory', 14, 10);
    placeBuilt(e, 'apartment', 18, 10);
    e.pop = 40;
    depot.stock.gravel = 40;
    depot.stock.crops = 100;
    works.stock.steel = 30;
    factory.stock.crops = 40;
    factory.stock.machinery = 0; // born worn — the penalty must survive the trip
    for (const x of [21, 22, 23]) e.tryPlace('road', x, 9); // a paint mid-flight
    runDays(e, 4); // trucks en route, some tiles complete, wear ticking

    const snap = e.serialize();
    const e2 = GameEngine.fromSave(snap, { weatherScript: CALM_WEATHER }); // test maps script their weather
    expect(stable(e2.serialize())).toEqual(stable(snap));

    runDays(e, 30);
    runDays(e2, 30);
    expect(stable(e2.serialize())).toEqual(stable(e.serialize()));
    // and the paint finished identically in both worlds
    expect(e.tiles[9][23].road).toBe(e2.tiles[9][23].road);
  });

  it('round-trips per-site policy: a planned blueprint, a $-West bonded site, the global switch off', () => {
    const e = makeEngine();
    e.rubles = 1e9; e.dollars = 1e9;
    layRoad(e, 4, 9, 22, 9);
    placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 7, 10);
    placeBuilt(e, 'customs', 9, 10);
    e.setForeignLaborEnabled(false);                                   // global master switch state
    e.tryPlace('house', 12, 10, { plan: true });                      // paused blueprint
    e.tryPlace('house', 14, 10, { autoBuy: true, currency: 'west' }); // $-West bonded site, trucks en route
    e.tryPlace('house', 16, 10, { foreignLabor: false });             // domestic-only site
    runDays(e, 3);

    const snap = e.serialize();
    const e2 = GameEngine.fromSave(snap, { weatherScript: CALM_WEATHER });
    expect(stable(e2.serialize())).toEqual(stable(snap));
    expect(e2.foreignLaborEnabled).toBe(false);
    expect(e2.buildingAt(12, 10)!.paused).toBe(true);
    expect(e2.buildingAt(14, 10)!.importCurrency).toBe('west');
    expect(e2.buildingAt(16, 10)!.foreignLabor).toBe(false);

    runDays(e, 30); runDays(e2, 30);
    expect(stable(e2.serialize())).toEqual(stable(e.serialize()));
  });

  it('ids allocated after a load never collide with saved entities', () => {
    const e = makeEngine();
    e.rubles = 1e9;
    layRoad(e, 10, 10, 20, 10);
    const b = placeBuilt(e, 'house', 12, 11);
    const e2 = GameEngine.fromSave(e.serialize());
    e2.rubles = 1e9;
    const res = e2.tryPlace('house', 14, 11, { instant: true });
    expect(res.ok).toBe(true);
    const placed = e2.buildingAt(14, 11)!;
    expect(placed.id).toBeGreaterThan(b.id);
  });
});
