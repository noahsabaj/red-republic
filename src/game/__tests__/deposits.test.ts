import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt } from './helpers';

describe('depositClusterAt', () => {
  it('returns the contiguous cluster including diagonal contact', () => {
    const e = makeEngine();
    // L-shaped coal cluster with one diagonal link, plus a separate cluster
    for (const [x, y] of [[10, 10], [11, 10], [11, 11], [12, 12]] as const) e.tiles[y][x].deposit = 'coal';
    for (const [x, y] of [[20, 10], [21, 10]] as const) e.tiles[y][x].deposit = 'coal';

    const cluster = e.depositClusterAt(10, 10)!;
    expect(cluster.kind).toBe('coal');
    expect(cluster.tiles).toHaveLength(4); // (12,12) joins via the diagonal
    expect(cluster.exploitedBy).toBeNull();
    expect(e.depositClusterAt(20, 10)!.tiles).toHaveLength(2); // separate cluster
  });

  it('does not merge different deposit types', () => {
    const e = makeEngine();
    e.tiles[10][10].deposit = 'coal';
    e.tiles[10][11].deposit = 'ironOre';
    expect(e.depositClusterAt(10, 10)!.tiles).toHaveLength(1);
  });

  it('reports the mine working the cluster', () => {
    const e = makeEngine();
    layRoad(e, 8, 9, 14, 9);
    for (const [x, y] of [[10, 10], [11, 10], [11, 11]] as const) e.tiles[y][x].deposit = 'coal';
    const mine = placeBuilt(e, 'coalMine', 11, 10);
    // clicking any free tile of the cluster reports the same mine
    expect(e.depositClusterAt(10, 10)!.exploitedBy?.id).toBe(mine.id);
    expect(e.depositClusterAt(11, 11)!.exploitedBy?.id).toBe(mine.id);
  });

  it('returns null off-deposit and ignores unrelated buildings', () => {
    const e = makeEngine();
    expect(e.depositClusterAt(5, 5)).toBeNull();
    e.tiles[10][10].deposit = 'gravel';
    e.tiles[10][10].terrain = 'rock';
    // a house on a neighboring tile is not an exploiter
    placeBuilt(e, 'house', 11, 10);
    e.tiles[10][11].deposit = 'gravel'; // extend cluster under nothing
    expect(e.depositClusterAt(10, 10)!.exploitedBy).toBeNull();
  });
});
