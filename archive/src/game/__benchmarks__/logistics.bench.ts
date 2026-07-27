import { bench, describe } from 'vitest';
import { GameEngine } from '../engine';
import type { BuildingInst } from '../engine';
import { CALM_WEATHER, flatMap, layRoad, placeBuilt } from '../__tests__/helpers';

// Timings are intentionally observational, never acceptance assertions. Each
// measured operation advances exactly one fixed simulation day; mutable demand
// state and the calendar are reset by Tinybench's unmeasured setup hook.
const OPTIONS = {
  time: 350,
  iterations: 20,
  warmupTime: 100,
  warmupIterations: 5,
};

function engineAt(size: number): GameEngine {
  const engine = new GameEngine({
    seed: 1,
    map: flatMap(size, size),
    skipStartingBase: true,
    weatherScript: CALM_WEATHER,
  });
  engine.setSpeed(1);
  return engine;
}

function resetCalendar(engine: GameEngine): void {
  engine.day = 1;
  engine.month = 3;
  engine.year = 1960;
  engine.drainEvents();
}

function shortageWorld(size: number): GameEngine {
  const engine = engineAt(size);
  layRoad(engine, 1, 1, 3, 1);
  placeBuilt(engine, 'constructionOffice', 2, 2);
  // An empty depot makes connectivity consume both component domains without
  // becoming an eligible supplier. This keeps cold-vs-warm topology meaningful.
  placeBuilt(engine, 'depot', 1, 18);
  for (let i = 0; i < 120; i++) {
    const x = 5 + (i % 20) * 2;
    const y = 5 + Math.floor(i / 20) * 2;
    placeBuilt(engine, 'sawmill', x, y);
  }
  return engine;
}

function verifyWorkload(
  label: string,
  engine: GameEngine,
  expected: { demands: number; dispatches: number; componentRejections: number },
): void {
  const diagnostics = engine.getRoutingDiagnostics();
  if (diagnostics.demandsConsidered !== expected.demands
    || diagnostics.successfulDispatches !== expected.dispatches
    || diagnostics.componentRejections !== expected.componentRejections) {
    throw new Error(`${label} benchmark fixture drifted: ${JSON.stringify(diagnostics)}`);
  }
}

interface MixedWorld {
  engine: GameEngine;
  suppliers: BuildingInst[];
  stores: BuildingInst[];
}

function mixedWorld(): MixedWorld {
  const engine = engineAt(128);

  // Two consumer rows and one supplier/fleet row on the same road component.
  layRoad(engine, 8, 9, 70, 9);
  layRoad(engine, 8, 12, 70, 12);
  layRoad(engine, 8, 24, 70, 24);
  layRoad(engine, 8, 9, 8, 24);

  const stores = Array.from({ length: 40 }, (_, i) =>
    placeBuilt(engine, 'store', 10 + (i % 20) * 3, 10 + Math.floor(i / 20) * 3));
  const suppliers = Array.from({ length: 8 }, (_, i) =>
    placeBuilt(engine, 'warehouse', 10 + i * 3, 25));
  for (let i = 0; i < 10; i++) placeBuilt(engine, 'constructionOffice', 40 + i * 3, 25);

  for (const supplier of suppliers) supplier.stock.food = 40;
  engine.advance(engine.TICK_MS); // warm topology and routing indexes once
  verifyWorkload('mixed supply', engine, { demands: 80, dispatches: 40, componentRejections: 80 });
  return { engine, suppliers, stores };
}

function resetMixed(world: MixedWorld): void {
  const { engine, suppliers, stores } = world;
  resetCalendar(engine);
  engine.trucks.length = 0;
  for (const supplier of suppliers) supplier.stock.food = 40;
  for (const store of stores) {
    store.stock.food = 0;
    store.stock.clothes = 0;
    store.incoming.food = 0;
    store.incoming.clothes = 0;
  }
}

describe('logistics routing (non-gating wall-clock benchmark)', () => {
  for (const size of [48, 96, 128]) {
    const cold = shortageWorld(size);
    let isolatedRoad = false;
    bench(`shortage ${size}x${size} — cold topology`, () => {
      cold.advance(cold.TICK_MS);
    }, {
      ...OPTIONS,
      setup: () => {
        resetCalendar(cold);
        isolatedRoad = !isolatedRoad;
        cold.applyTilePatches([{ x: size - 2, y: size - 2, road: isolatedRoad }]);
      },
    });

    const warm = shortageWorld(size);
    warm.advance(warm.TICK_MS);
    verifyWorkload(`shortage ${size}x${size}`, warm, {
      demands: 120,
      dispatches: 0,
      componentRejections: 240,
    });
    bench(`shortage ${size}x${size} — warm topology`, () => {
      warm.advance(warm.TICK_MS);
    }, {
      ...OPTIONS,
      setup: () => resetCalendar(warm),
    });
  }

  const mixed = mixedWorld();
  bench('mixed supply 128x128 — warm topology (40 dispatches + 40 rejections)', () => {
    mixed.engine.advance(mixed.engine.TICK_MS);
  }, {
    ...OPTIONS,
    setup: () => resetMixed(mixed),
  });
});
