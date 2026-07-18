import { describe, expect, it } from 'vitest';
import { hash01, isoCompare, pickBuilding, precipParticle, screenToTile, toScreen, truckWorldPos, type Camera } from '../render';
import type { Truck } from '../engine';
import { makeEngine, placeBuilt } from './helpers';

const cam: Camera = { x: 137, y: 42, z: 1.3 };

describe('coordinate transforms', () => {
  it('screenToTile inverts toScreen for tile centers', () => {
    for (const [tx, ty] of [[0, 0], [5, 3], [47, 47], [12, 40]] as const) {
      const p = toScreen(tx + 0.5, ty + 0.5, cam); // center of tile
      expect(screenToTile(p.x, p.y, cam)).toEqual({ x: tx, y: ty });
    }
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

describe('truckWorldPos', () => {
  const truck = (over: Partial<Truck>): Truck => ({
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
