import { describe, expect, it } from 'vitest';
import { STATUS_PALETTES, centerCameraOn, hash01, isoCompare, pickBuilding, pickEntity, precipParticle, screenToTile, shouldRenderBackdropFrame, shouldRenderFrame, toScreen, truckWorldPos, type Camera, type FrameInvalidation } from '../render';
import type { Mover } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

const cam: Camera = { x: 137, y: 42, z: 1.3 };

describe('coordinate transforms', () => {
  it('screenToTile inverts toScreen for tile centers', () => {
    for (const [tx, ty] of [[0, 0], [5, 3], [47, 47], [12, 40]] as const) {
      const p = toScreen(tx + 0.5, ty + 0.5, cam); // center of tile
      expect(screenToTile(p.x, p.y, cam)).toEqual({ x: tx, y: ty });
    }
  });
});

describe('status palettes', () => {
  it('every variant defines exactly the same keys, and colorblind adds the shape cue', () => {
    const keySets = Object.values(STATUS_PALETTES).map(p => Object.keys(p).sort().join(','));
    expect(new Set(keySets).size).toBe(1);
    expect(STATUS_PALETTES.default.crossInvalid).toBe(false);
    expect(STATUS_PALETTES.colorblind.crossInvalid).toBe(true);
  });
});

describe('paused reduced-motion render scheduling', () => {
  const stable: FrameInvalidation = {
    hasRendered: true,
    cameraChanged: false,
    hoverChanged: false,
    engineChanged: false,
    externalChanged: false,
  };

  it('sleeps only when a paused reduced-motion frame is fully unchanged', () => {
    expect(shouldRenderFrame(0, true, stable)).toBe(false);
    expect(shouldRenderFrame(1, true, stable)).toBe(true);
    expect(shouldRenderFrame(0, false, stable)).toBe(true);
    expect(shouldRenderFrame(0, true, { ...stable, hasRendered: false })).toBe(true);
  });

  it.each(['cameraChanged', 'hoverChanged', 'engineChanged', 'externalChanged'] as const)(
    'repaints when %s',
    (key) => expect(shouldRenderFrame(0, true, { ...stable, [key]: true })).toBe(true),
  );

  it('repaints a reduced-motion backdrop after resize and on each transition into reduced motion', () => {
    expect(shouldRenderBackdropFrame(true, true, true, 0)).toBe(true);   // dirty backing buffer
    expect(shouldRenderBackdropFrame(true, false, false, 0)).toBe(true); // transition
    expect(shouldRenderBackdropFrame(true, false, true, 1000)).toBe(false);
    expect(shouldRenderBackdropFrame(false, false, false, 31)).toBe(false);
    expect(shouldRenderBackdropFrame(false, false, false, 32)).toBe(true);
  });
});

describe('isoCompare (draw order)', () => {
  // the reported artifact: 1x1 plant in the gap between two 2x2s.
  // depot (0,0,2x2) | plant (2,0,1x1) | customs (3,0,2x2)
  const depot = { x: 0, y: 0, w: 2, h: 2 };
  const plant = { x: 2, y: 0, w: 1, h: 1 };
  const customs = { x: 3, y: 0, w: 2, h: 2 };

  it('a 1x1 east of a 2x2 is in front despite its earlier row', () => {
    expect(isoCompare(plant, depot)).toBeGreaterThan(0);  // plant over depot
    expect(isoCompare(depot, plant)).toBeLessThan(0);
    expect(isoCompare(customs, plant)).toBeGreaterThan(0); // customs over plant
  });

  it('sorts the gap scenario back-to-front: depot, plant, customs', () => {
    const sorted = [customs, plant, depot].sort(isoCompare);
    expect(sorted).toEqual([depot, plant, customs]);
  });

  it('a 1x1 north of a 2x2 stays behind it (store behind customs)', () => {
    const store = { x: 2, y: 1, w: 1, h: 1 };
    expect(isoCompare(store, customs)).toBeLessThan(0);
    expect(isoCompare(store, depot)).toBeGreaterThan(0); // but in front of the depot
  });

  it('points (trucks/citizens) follow the same relation', () => {
    const truckEast = { x: 2.4, y: 0.5, w: 0, h: 0 };  // on the gap road, east of depot
    const truckNorth = { x: 0.5, y: -0.5, w: 0, h: 0 }; // on the road behind depot
    expect(isoCompare(truckEast, depot)).toBeGreaterThan(0);
    expect(isoCompare(truckNorth, depot)).toBeLessThan(0);
  });
});

describe('pickBuilding', () => {
  it('clicking the overlap region selects the front 1x1, not the 2x2 behind it', () => {
    const e = makeEngine();
    const depot = placeBuilt(e, 'depot', 10, 10);          // 2x2, boxHeight 18
    const plant = placeBuilt(e, 'heatingPlant', 12, 10);   // 1x1 in the gap, boxHeight 16
    const cam: Camera = { x: 0, y: 0, z: 1 };
    // a point on the plant's top face, inside the screen region the depot's
    // right wall also covers — the old front-most metric returned the depot
    const p = toScreen(12.25, 10.5, cam);
    const hit = pickBuilding(e, p.x, p.y - 16, cam);
    expect(hit?.id).toBe(plant.id);
    expect(hit?.id).not.toBe(depot.id);
  });
});

describe('pickEntity (trucks vs buildings)', () => {
  /** Park a lorry at an exact world position by giving it a zero-length route. */
  function parkLorryAt(e: ReturnType<typeof makeEngine>, wx: number, wy: number) {
    const v = e.trucks[0];
    if (!v) throw new Error('no lorry in the fleet');
    v.points = [{ x: wx, y: wy }];
    v.daysDone = 0;
    v.daysTotal = 0;
    return v;
  }

  function fleetTown() {
    const e = makeEngine();
    layRoad(e, 8, 9, 20, 9);
    placeBuilt(e, 'depot', 10, 10);
    placeBuilt(e, 'constructionOffice', 14, 10);
    runDays(e, 1); // the garage's lorries exist
    return e;
  }

  it('a lorry on open ground is selectable', () => {
    const e = fleetTown();
    const v = parkLorryAt(e, 18.5, 6.5); // empty tile, nothing drawn behind it
    const cam: Camera = { x: 0, y: 0, z: 1 };
    const p = toScreen(18.5, 6.5, cam);
    const hit = pickEntity(e, p.x, p.y - 6, cam);
    expect(hit).toEqual({ kind: 'truck', truck: v });
  });

  it('a lorry BEHIND a building does not steal the click from it', () => {
    // The regression: pickTruck ran first with a 15z-radius circle and no
    // occlusion test, so a lorry the player could not even see won the click.
    const e = fleetTown();
    const depot = e.buildingAt(10, 10)!;
    parkLorryAt(e, depot.x - 0.5, depot.y - 0.5); // north-west of it = drawn behind
    const cam: Camera = { x: 0, y: 0, z: 1 };
    const p = toScreen(depot.x + 0.5, depot.y + 0.5, cam);
    const hit = pickEntity(e, p.x, p.y - 10, cam);
    expect(hit?.kind).toBe('building');
  });

  it('picks the front-most lorry when two overlap, not the first in the array', () => {
    const e = fleetTown();
    const [a, b] = e.trucks;
    a.points = [{ x: 17.5, y: 5.5 }]; a.daysTotal = 0; a.daysDone = 0; // behind
    b.points = [{ x: 17.6, y: 5.6 }]; b.daysTotal = 0; b.daysDone = 0; // in front
    const cam: Camera = { x: 0, y: 0, z: 1 };
    const p = toScreen(17.6, 5.6, cam);
    const hit = pickEntity(e, p.x, p.y - 6, cam);
    expect(hit).toEqual({ kind: 'truck', truck: b });
  });

  it('empty ground picks nothing', () => {
    const e = fleetTown();
    const cam: Camera = { x: 0, y: 0, z: 1 };
    const p = toScreen(40.5, 40.5, cam);
    expect(pickEntity(e, p.x, p.y, cam)).toBeNull();
  });
});

describe('centerCameraOn', () => {
  it('puts the requested world position in the middle of the viewport', () => {
    const c: Camera = { x: 0, y: 0, z: 1.4 };
    centerCameraOn(c, 12.5, 30.5, 900, 600);
    const p = toScreen(12.5, 30.5, c);
    expect(p.x).toBeCloseTo(450, 9);
    expect(p.y).toBeCloseTo(300, 9);
  });
});

describe('truckWorldPos', () => {
  const truck = (over: Partial<Mover>): Mover => ({
    id: 1, points: [{ x: 0, y: 0 }, { x: 4, y: 0 }, { x: 4, y: 3 }],
    cargo: 'coal', amount: 6, daysTotal: 2, daysDone: 0,
    phase: 'go', destId: 1, srcId: 2, ...over,
  });

  it('interpolates along the polyline going out', () => {
    expect(truckWorldPos(truck({ daysDone: 0 }))).toEqual({ wx: 0, wy: 0 });
    expect(truckWorldPos(truck({ daysDone: 1 }))).toEqual({ wx: 4, wy: 0 });    // halfway = 1st segment done
    expect(truckWorldPos(truck({ daysDone: 2 }))).toEqual({ wx: 4, wy: 3 });    // arrived
    expect(truckWorldPos(truck({ daysDone: 99 }))).toEqual({ wx: 4, wy: 3 });   // clamped
  });

  it('reverses the polyline on the way back', () => {
    expect(truckWorldPos(truck({ phase: 'back', daysDone: 0 }))).toEqual({ wx: 4, wy: 3 });
    expect(truckWorldPos(truck({ phase: 'back', daysDone: 2 }))).toEqual({ wx: 0, wy: 0 });
  });
});

describe('weather particles', () => {
  it('are a pure function of (index, time) — deterministic and stateless', () => {
    const a = precipParticle(17, 12345, 800, 600, 'rain', 0.5, 950);
    const b = precipParticle(17, 12345, 800, 600, 'rain', 0.5, 950);
    expect(a).toEqual(b);
    expect(hash01(42)).toBe(hash01(42));
  });

  it('stay inside the padded viewport at any time', () => {
    for (const t of [0, 999, 123456, 9876543]) {
      for (let i = 0; i < 50; i++) {
        for (const kind of ['rain', 'snow'] as const) {
          const p = precipParticle(i, t, 800, 600, kind, kind === 'snow' ? 0.75 : 0.14, 300);
          expect(p.x).toBeGreaterThanOrEqual(-100);
          expect(p.x).toBeLessThanOrEqual(900);
          expect(p.y).toBeGreaterThanOrEqual(-20);
          expect(p.y).toBeLessThanOrEqual(620);
        }
      }
    }
  });

  it('particles actually fall between frames', () => {
    const p0 = precipParticle(3, 10000, 800, 600, 'rain', 0.14, 640);
    const p1 = precipParticle(3, 10016, 800, 600, 'rain', 0.14, 640);
    expect(p1.y).not.toBe(p0.y);
  });
});
