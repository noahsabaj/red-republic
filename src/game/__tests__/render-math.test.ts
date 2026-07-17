import { describe, expect, it } from 'vitest';
import { screenToTile, toScreen, truckWorldPos, type Camera } from '../render';
import type { Truck } from '../engine';

const cam: Camera = { x: 137, y: 42, z: 1.3 };

describe('coordinate transforms', () => {
  it('screenToTile inverts toScreen for tile centers', () => {
    for (const [tx, ty] of [[0, 0], [5, 3], [47, 47], [12, 40]] as const) {
      const p = toScreen(tx + 0.5, ty + 0.5, cam); // center of tile
      expect(screenToTile(p.x, p.y, cam)).toEqual({ x: tx, y: ty });
    }
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
