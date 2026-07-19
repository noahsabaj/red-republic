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

  it('ids allocated after a load never collide with saved entities', () => {
    const e = makeEngine();
    e.rubles = 1e9;
    layRoad(e, 10, 10, 20, 10);
    const b = placeBuilt(e, 'house', 12, 11);
    const e2 = GameEngine.fromSave(e.serialize());
    e2.rubles = 1e9;
    const res = e2.tryPlace('house', 14, 11, true);
    expect(res.ok).toBe(true);
    const placed = e2.buildingAt(14, 11)!;
    expect(placed.id).toBeGreaterThan(b.id);
  });
});
