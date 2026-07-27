import { describe, expect, it } from 'vitest';
import { layRoad, makeEngine, placeBuilt } from './helpers';

describe('depositClusterAt', () => {
  it('returns the contiguous cluster including diagonal contact', () => {
    const e = makeEngine();
    // L-shaped coal cluster with one diagonal link, plus a separate cluster
    e.applyTilePatches([
      ...([[10, 10], [11, 10], [11, 11], [12, 12]] as const).map(([x, y]) => ({ x, y, deposit: 'coal' as const })),
      ...([[20, 10], [21, 10]] as const).map(([x, y]) => ({ x, y, deposit: 'coal' as const })),
    ]);

    const cluster = e.depositClusterAt(10, 10)!;
    expect(cluster.kind).toBe('coal');
    expect(cluster.tiles).toHaveLength(4); // (12,12) joins via the diagonal
    expect(cluster.exploitedBy).toBeNull();
    expect(e.depositClusterAt(20, 10)!.tiles).toHaveLength(2); // separate cluster
  });

  it('does not merge different deposit types', () => {
    const e = makeEngine();
    e.applyTilePatches([
      { x: 10, y: 10, deposit: 'coal' },
      { x: 11, y: 10, deposit: 'ironOre' },
    ]);
    expect(e.depositClusterAt(10, 10)!.tiles).toHaveLength(1);
  });

  it('reports the mine working the cluster', () => {
    const e = makeEngine();
    layRoad(e, 8, 9, 14, 9);
    e.applyTilePatches(([[10, 10], [11, 10], [11, 11]] as const)
      .map(([x, y]) => ({ x, y, deposit: 'coal' as const })));
    const mine = placeBuilt(e, 'coalMine', 11, 10);
    // clicking any free tile of the cluster reports the same mine
    expect(e.depositClusterAt(10, 10)!.exploitedBy?.id).toBe(mine.id);
    expect(e.depositClusterAt(11, 11)!.exploitedBy?.id).toBe(mine.id);
  });

  it('returns null off-deposit and ignores unrelated buildings', () => {
    const e = makeEngine();
    expect(e.depositClusterAt(5, 5)).toBeNull();
    e.applyTilePatches([{ x: 10, y: 10, deposit: 'gravel', terrain: 'rock' }]);
    // a house on a neighboring tile is not an exploiter
    placeBuilt(e, 'house', 11, 10);
    e.applyTilePatches([{ x: 11, y: 10, deposit: 'gravel' }]); // extend cluster under nothing
    expect(e.depositClusterAt(10, 10)!.exploitedBy).toBeNull();
  });
});

describe('applyTilePatches deposit semantics', () => {
  it('treats an explicit undefined as a no-op and null as an explicit clear', () => {
    const e = makeEngine();
    e.applyTilePatches([{ x: 5, y: 5, deposit: 'coal' }]);
    expect(e.tiles[5][5].deposit).toBe('coal');

    e.applyTilePatches([{ x: 5, y: 5, deposit: undefined }]); // no-op: field present but undefined
    expect(e.tiles[5][5].deposit).toBe('coal'); // still there

    e.applyTilePatches([{ x: 5, y: 5, deposit: null }]); // explicit clear
    expect(e.tiles[5][5].deposit).toBeUndefined();
  });
});
