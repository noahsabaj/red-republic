import { describe, expect, it } from 'vitest';
import { floodRoads } from '../pathfind';
import { DEFAULT_MAP_W, DEFAULT_MAP_H } from '../mapgen';

// L-shaped road: (2,2)→(2,6) then (2,6)→(8,6)
const roads = new Set<string>();
for (let y = 2; y <= 6; y++) roads.add(`2,${y}`);
for (let x = 2; x <= 8; x++) roads.add(`${x},6`);
const isRoad = (x: number, y: number) => roads.has(`${x},${y}`);

const flood48 = (sources: { x: number; y: number }[]) =>
  floodRoads(DEFAULT_MAP_W, DEFAULT_MAP_H, isRoad, sources);

describe('floodRoads', () => {
  it('computes multi-source distances along the network only', () => {
    const flood = flood48([{ x: 2, y: 2 }]);
    expect(flood.distanceAt(2, 2)).toBe(0);
    expect(flood.distanceAt(2, 6)).toBe(4);
    expect(flood.distanceAt(8, 6)).toBe(10);
    expect(flood.distanceAt(5, 5)).toBe(-1); // off-network
    expect(flood.distanceAt(3, 2)).toBe(-1); // adjacent but not road
  });

  it('returns a contiguous orthogonal path ordered query→source', () => {
    const flood = flood48([{ x: 2, y: 2 }]);
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
    const flood = flood48([{ x: 20, y: 20 }]);
    expect(flood.pathFrom(2, 2)).toBeNull();
  });

  it('reuses scratch buffers safely; stale views throw', () => {
    const a = flood48([{ x: 2, y: 2 }]);
    expect(a.distanceAt(8, 6)).toBe(10);
    const snap = a.snapshot();
    const b = flood48([{ x: 8, y: 6 }]);
    expect(b.distanceAt(2, 2)).toBe(10);
    expect(b.distanceAt(8, 6)).toBe(0);
    expect(() => a.distanceAt(2, 2)).toThrow(/Stale/);
    expect(snap.distanceAt(8, 6)).toBe(10); // snapshot survives the second flood
  });

  it('floods correctly on non-default map sizes, interleaved', () => {
    // a long road on a 96x96 map, beyond the 48x48 index range
    const bigRoad = (x: number, y: number) => y === 90 && x >= 50 && x <= 90;
    const big = floodRoads(96, 96, bigRoad, [{ x: 50, y: 90 }]);
    expect(big.distanceAt(90, 90)).toBe(40);
    expect(big.distanceAt(50, 90)).toBe(0);
    expect(big.distanceAt(49, 90)).toBe(-1);

    // back down to 48 — small flood still correct after the resize
    const small = flood48([{ x: 2, y: 2 }]);
    expect(small.distanceAt(8, 6)).toBe(10);

    // and up again on a rectangular map
    const rect = floodRoads(64, 32, (x, y) => y === 10 && x >= 0 && x <= 60, [{ x: 0, y: 10 }]);
    expect(rect.distanceAt(60, 10)).toBe(60);
  });

  it('a resize invalidates outstanding views but not snapshots', () => {
    const a = flood48([{ x: 2, y: 2 }]);
    const snap = a.snapshot();
    floodRoads(96, 96, () => false, []); // resize happens here
    expect(() => a.distanceAt(2, 2)).toThrow(/Stale/);
    expect(snap.distanceAt(8, 6)).toBe(10); // snapshot owns its storage and width
    expect(snap.distanceAt(2, 2)).toBe(0);
  });
});
