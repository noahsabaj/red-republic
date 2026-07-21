import { describe, expect, it } from 'vitest';
import type { Tile } from '../mapgen';
import { shareAnyComponent, TopologyIndex, unionComponents } from '../topology';

function flatGrid(width: number, height: number): Tile[][] {
  return Array.from({ length: height }, () =>
    Array.from({ length: width }, () => ({ terrain: 'grass', variant: 0 }) satisfies Tile));
}

function makeIndex(tiles: () => Tile[][], width: number, height: number): TopologyIndex {
  return new TopologyIndex({ width, height, tiles, offRoadCost: 8 });
}

describe('TopologyIndex masks and components', () => {
  it('matches road, weighted-land, and water routing predicates exactly', () => {
    const tiles = flatGrid(5, 2);
    Object.assign(tiles[0][0], { road: true, foreign: true });
    Object.assign(tiles[0][1], { terrain: 'water', road: true });
    tiles[0][2].buildingId = 9;
    tiles[0][4].terrain = 'water';
    const topology = makeIndex(() => tiles, 5, 2);

    expect(Array.from(topology.mask('road').slice(0, 5))).toEqual([1, 1, 0, 0, 0]);
    expect(Array.from(topology.mask('land').slice(0, 5))).toEqual([0, 0, 0, 8, 0]);
    expect(Array.from(topology.mask('water').slice(0, 5))).toEqual([0, 1, 0, 0, 1]);

    // A bridge remains in both the road and water networks, but never land routing.
    expect(topology.componentAt('road', 1, 0)).toBeGreaterThan(0);
    expect(topology.componentAt('water', 1, 0)).toBeGreaterThan(0);
    expect(topology.componentAt('land', 1, 0)).toBe(0);
    expect(topology.componentCount('water')).toBe(2);
  });

  it('labels components in deterministic row-major order', () => {
    const tiles = flatGrid(6, 3);
    for (const [x, y] of [[4, 0], [5, 0], [1, 1], [1, 2]] as const) tiles[y][x].road = true;
    const topology = makeIndex(() => tiles, 6, 3);

    expect(topology.componentCount('road')).toBe(2);
    expect(topology.componentAt('road', 4, 0)).toBe(1);
    expect(topology.componentAt('road', 5, 0)).toBe(1);
    expect(topology.componentAt('road', 1, 1)).toBe(2);
    expect(topology.componentAt('road', 1, 2)).toBe(2);
    expect(topology.componentAt('road', -1, 0)).toBe(0);
  });
});

describe('TopologyIndex invalidation', () => {
  it('rebuilds domains independently and reads a replacement live grid lazily', () => {
    let tiles = flatGrid(3, 1);
    tiles[0][0].road = true;
    const topology = makeIndex(() => tiles, 3, 1);

    const firstRoadMask = topology.mask('road');
    topology.mask('road');
    topology.mask('land');
    expect(topology.getDiagnostics().rebuilds).toEqual({ road: 1, land: 1, water: 0 });

    tiles = flatGrid(3, 1);
    tiles[0][2].road = true;
    topology.invalidateRoad();

    // Invalidation is domain-specific and does no eager work.
    expect(topology.getDiagnostics()).toMatchObject({
      revisions: { road: 1, land: 0, water: 0 },
      builtRevisions: { road: 0, land: 0, water: -1 },
      rebuilds: { road: 1, land: 1, water: 0 },
    });
    expect(topology.mask('land')).not.toBe(firstRoadMask);
    expect(topology.rebuildCount('land')).toBe(1);

    const secondRoadMask = topology.mask('road');
    expect(secondRoadMask).not.toBe(firstRoadMask);
    expect(Array.from(secondRoadMask)).toEqual([0, 0, 1]);
    expect(topology.rebuildCount('road')).toBe(2);
    expect(topology.getDiagnostics().builtRevisions.road).toBe(1);
  });

  it('increments a repeated domain only once in one batched invalidation', () => {
    const tiles = flatGrid(2, 2);
    const topology = makeIndex(() => tiles, 2, 2);

    topology.invalidate('road', 'water', 'road', 'water');
    expect(topology.getDiagnostics().revisions).toEqual({ road: 1, land: 0, water: 1 });
    expect(topology.getDiagnostics().rebuilds).toEqual({ road: 0, land: 0, water: 0 });
  });
});

describe('TopologyIndex footprint access', () => {
  it('caches exact perimeter order and retains every touched component', () => {
    const tiles = flatGrid(7, 7);
    const expected = [
      { x: 2, y: 1 }, { x: 3, y: 1 },
      { x: 1, y: 2 }, { x: 4, y: 2 },
      { x: 1, y: 3 }, { x: 4, y: 3 },
      { x: 2, y: 4 }, { x: 3, y: 4 },
    ];
    for (const { x, y } of expected) tiles[y][x].road = true;
    const topology = makeIndex(() => tiles, 7, 7);
    const footprint = { x: 2, y: 2, w: 2, h: 2 };

    const first = topology.access('road', footprint);
    expect(first.tiles).toEqual(expected);
    expect(first.components).toEqual([1, 2, 3, 4]);
    expect(topology.access('road', footprint)).toBe(first);

    // A non-access corner may connect two access segments in the same component.
    tiles[1][4].road = true;
    topology.invalidateRoad();
    const rebuilt = topology.access('road', footprint);
    expect(rebuilt).not.toBe(first);
    expect(rebuilt.tiles).toEqual(expected);
    expect(rebuilt.components).toEqual([1, 2, 3]);
  });

  it('exposes component overlap via access().components + shareAnyComponent', () => {
    const tiles = flatGrid(9, 5);
    for (let x = 0; x < 9; x++) tiles[1][x].road = true; // component 1
    for (let x = 5; x < 9; x++) tiles[4][x].road = true; // component 2
    const topology = makeIndex(() => tiles, 9, 5);
    const a = topology.access('road', { x: 1, y: 2, w: 1, h: 1 }); // touches row 1
    const b = topology.access('road', { x: 7, y: 2, w: 1, h: 1 }); // touches row 1
    const isolated = topology.access('road', { x: 7, y: 3, w: 1, h: 1 }); // touches row 4

    expect(shareAnyComponent(a.components, b.components)).toBe(true);
    expect(shareAnyComponent(a.components, isolated.components)).toBe(false);
    expect(shareAnyComponent([2, 1], [3, 2])).toBe(true);
    expect(shareAnyComponent([], [1])).toBe(false);
  });

  it('unions distinct component lists in first-seen order, dropping zeros', () => {
    expect(unionComponents([3, 1], [1, 2], [2, 4])).toEqual([3, 1, 2, 4]);
    expect(unionComponents([], [5], [])).toEqual([5]);
    expect(unionComponents([0, 3], [0])).toEqual([3]);
    expect(unionComponents()).toEqual([]);
  });
});

describe('TopologyIndex maxStep derivation', () => {
  it('derives the Dial bound from the mask: road/water 1, land = offRoadCost', () => {
    const tiles = flatGrid(3, 1);
    tiles[0][0].road = true; // one road tile; the other two are off-road grass
    const topology = makeIndex(() => tiles, 3, 1);
    expect(topology.maxStep('road')).toBe(1); // only cost-1 road tiles
    expect(topology.maxStep('water')).toBe(1); // no water → floor 1
    expect(topology.maxStep('land')).toBe(8); // grass off-road costs offRoadCost
  });

  it('drops the land bound to 1 when every land-passable tile is a road', () => {
    const tiles = flatGrid(3, 1);
    tiles[0][0].road = true;
    tiles[0][1].terrain = 'water'; // impassable to land
    tiles[0][2].terrain = 'water';
    const topology = makeIndex(() => tiles, 3, 1);
    expect(topology.maxStep('land')).toBe(1); // only the cost-1 road tile is land-passable
  });

  it('floors an all-impassable domain to 1 and recomputes after invalidation', () => {
    const tiles = flatGrid(2, 1);
    tiles[0][0].terrain = 'water';
    tiles[0][1].terrain = 'water'; // land has no passable tile
    const topology = makeIndex(() => tiles, 2, 1);
    expect(topology.maxStep('land')).toBe(1); // floor
    tiles[0][0].terrain = 'grass'; // an off-road grass tile appears
    topology.invalidateLand();
    expect(topology.maxStep('land')).toBe(8);
  });
});

describe('TopologyIndex construction bounds', () => {
  it('rejects an offRoadCost the pathfinder cannot represent', () => {
    const tiles = flatGrid(2, 1);
    expect(() => new TopologyIndex({ width: 2, height: 1, tiles: () => tiles, offRoadCost: 16 }))
      .toThrow(/offRoadCost must be an integer in \[1, 15\]/);
    expect(() => new TopologyIndex({ width: 2, height: 1, tiles: () => tiles, offRoadCost: 15 }))
      .not.toThrow(); // the supported ceiling still constructs
  });
});

