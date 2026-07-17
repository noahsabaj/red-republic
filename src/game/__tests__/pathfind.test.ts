import { describe, expect, it } from 'vitest';
import { floodRoads } from '../pathfind';

// L-shaped road: (2,2)→(2,6) then (2,6)→(8,6)
const roads = new Set<string>();
for (let y = 2; y <= 6; y++) roads.add(`2,${y}`);
for (let x = 2; x <= 8; x++) roads.add(`${x},6`);
const isRoad = (x: number, y: number) => roads.has(`${x},${y}`);

describe('floodRoads', () => {
  it('computes multi-source distances along the network only', () => {
    const flood = floodRoads(isRoad, [{ x: 2, y: 2 }]);
    expect(flood.distanceAt(2, 2)).toBe(0);
    expect(flood.distanceAt(2, 6)).toBe(4);
    expect(flood.distanceAt(8, 6)).toBe(10);
    expect(flood.distanceAt(5, 5)).toBe(-1); // off-network
    expect(flood.distanceAt(3, 2)).toBe(-1); // adjacent but not road
  });

  it('returns a contiguous orthogonal path ordered query→source', () => {
    const flood = floodRoads(isRoad, [{ x: 2, y: 2 }]);
    const path = flood.pathFrom(8, 6)!;
    expect(path[0]).toEqual({ x: 8, y: 6 });
    expect(path[path.length - 1]).toEqual({ x: 2, y: 2 });
    expect(path.length).toBe(11);
    for (let i = 1; i < path.length; i++) {
      const d = Math.abs(path[i].x - path[i - 1].x) + Math.abs(path[i].y - path[i - 1].y);
      expect(d).toBe(1);
      expect(isRoad(path[i].x, path[i].y)).toBe(true);
    }
  });

  it('ignores non-road sources and reports unreachable targets as null', () => {
    const flood = floodRoads(isRoad, [{ x: 20, y: 20 }]);
    expect(flood.pathFrom(2, 2)).toBeNull();
  });

  it('reuses scratch buffers safely; stale views throw', () => {
    const a = floodRoads(isRoad, [{ x: 2, y: 2 }]);
    expect(a.distanceAt(8, 6)).toBe(10);
    const snap = a.snapshot();
    const b = floodRoads(isRoad, [{ x: 8, y: 6 }]);
    expect(b.distanceAt(2, 2)).toBe(10);
    expect(b.distanceAt(8, 6)).toBe(0);
    expect(() => a.distanceAt(2, 2)).toThrow(/Stale/);
    expect(snap.distanceAt(8, 6)).toBe(10); // snapshot survives the second flood
  });
});
