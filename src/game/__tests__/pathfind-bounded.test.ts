import { describe, expect, it } from 'vitest';
import {
  floodCost,
  type CostFn,
  type RankedGoal,
  type RoadPos,
  shortestPathToAny,
} from '../pathfind';

interface GoalValue { name: string }

function reference<T>(
  w: number,
  h: number,
  cost: CostFn,
  sources: RoadPos[],
  goals: RankedGoal<T>[],
  maxStep: number,
) {
  const flood = floodCost(w, h, cost, sources, maxStep);
  let selected: RankedGoal<T> | null = null;
  let selectedOrder = Infinity;
  let selectedDistance = Infinity;
  for (let order = 0; order < goals.length; order++) {
    const goal = goals[order];
    if (goal.x < 0 || goal.y < 0 || goal.x >= w || goal.y >= h) continue;
    const distance = flood.distanceAt(goal.x, goal.y);
    if (distance < 0) continue;
    const betterTie = selected !== null && distance === selectedDistance && (
      goal.buildingRank < selected.buildingRank
      || (goal.buildingRank === selected.buildingRank && goal.accessRank < selected.accessRank)
      || (goal.buildingRank === selected.buildingRank
        && goal.accessRank === selected.accessRank
        && order < selectedOrder)
    );
    if (distance < selectedDistance || betterTie) {
      selected = goal;
      selectedOrder = order;
      selectedDistance = distance;
    }
  }
  if (!selected) return null;
  return {
    goal: selected,
    path: flood.pathFrom(selected.x, selected.y)!,
    cost: selectedDistance,
  };
}

describe('shortestPathToAny', () => {
  it('matches a complete weighted flood and ordered goal scan on seeded maps', () => {
    for (let seed = 1; seed <= 24; seed++) {
      let state = seed;
      const rnd = () => {
        state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
        return state / 0x1_0000_0000;
      };
      const w = 9, h = 7, maxStep = 5;
      const costs = Array.from({ length: w * h }, () => {
        const n = rnd();
        return n < 0.18 ? 0 : 1 + Math.floor(rnd() * maxStep);
      });
      const cost = (x: number, y: number) => costs[y * w + x];
      const sources = Array.from({ length: 3 }, () => ({
        x: Math.floor(rnd() * w),
        y: Math.floor(rnd() * h),
      }));
      const goals: RankedGoal<GoalValue>[] = Array.from({ length: 14 }, (_, order) => ({
        x: Math.floor(rnd() * w),
        y: Math.floor(rnd() * h),
        value: { name: `goal-${order}` },
        buildingRank: Math.floor(order / 2),
        accessRank: order % 2,
      }));

      const expected = reference(w, h, cost, sources, goals, maxStep);
      const actual = shortestPathToAny(w, h, cost, sources, goals, maxStep);
      expect(actual?.goal).toBe(expected?.goal);
      expect(actual?.cost).toBe(expected?.cost);
      expect(actual?.path).toEqual(expected?.path);
    }
  });

  it('settles the full winning distance before applying building and access ranks', () => {
    const cost = () => 1;
    const goals: RankedGoal<GoalValue>[] = [
      { x: 4, y: 2, value: { name: 'encountered-first' }, buildingRank: 9, accessRank: 0 },
      { x: 2, y: 4, value: { name: 'building-wins' }, buildingRank: 2, accessRank: 9 },
      { x: 0, y: 2, value: { name: 'access-wins' }, buildingRank: 2, accessRank: 1 },
    ];

    const result = shortestPathToAny(5, 5, cost, [{ x: 2, y: 2 }], goals, 1)!;
    expect(result.goal.value.name).toBe('access-wins');
    expect(result.cost).toBe(2);
    expect(result.settledNodes).toBe(13); // every node in the radius-2 diamond
  });

  it('uses input order only when both explicit ranks tie, including duplicate coordinates', () => {
    const first = { x: 2, y: 0, value: { name: 'first' }, buildingRank: 3, accessRank: 4 };
    const duplicate = { x: 2, y: 0, value: { name: 'duplicate' }, buildingRank: 3, accessRank: 4 };
    const result = shortestPathToAny(3, 1, () => 1, [{ x: 0, y: 0 }], [first, duplicate], 1)!;
    expect(result.goal).toBe(first);
    expect(result.path).toEqual([{ x: 2, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 0 }]);
  });

  it('settles and ranks every zero-distance goal supplied as a source', () => {
    const goals = [
      { x: 0, y: 0, value: 'first-source', buildingRank: 4, accessRank: 0 },
      { x: 2, y: 0, value: 'rank-winner', buildingRank: 1, accessRank: 0 },
      { x: 2, y: 0, value: 'duplicate', buildingRank: 2, accessRank: 0 },
    ];
    const result = shortestPathToAny(
      3,
      1,
      () => 1,
      [{ x: 0, y: 0 }, { x: 2, y: 0 }, { x: 2, y: 0 }],
      goals,
      1,
    )!;
    expect(result.goal.value).toBe('rank-winner');
    expect(result.cost).toBe(0);
    expect(result.path).toEqual([{ x: 2, y: 0 }]);
    expect(result.settledNodes).toBe(2); // duplicate sources are enqueued once
  });

  it('preserves source FIFO and strict-parent path ties', () => {
    const sources = [{ x: 0, y: 1 }, { x: 4, y: 1 }];
    const goals = [{
      x: 2, y: 1, value: { name: 'middle' }, buildingRank: 0, accessRank: 0,
    }];
    const expected = reference(5, 3, () => 1, sources, goals, 1)!;
    const result = shortestPathToAny(5, 3, () => 1, sources, goals, 1)!;
    expect(result.path).toEqual(expected.path);
    expect(result.path.at(-1)).toEqual(sources[0]);
  });

  it('retains entry-cost directionality for weighted endpoints', () => {
    const costs = [1, 2, 7];
    const cost = (x: number) => costs[x];
    const east = [{ x: 2, y: 0, value: 'east', buildingRank: 0, accessRank: 0 }];
    const west = [{ x: 0, y: 0, value: 'west', buildingRank: 0, accessRank: 0 }];
    expect(shortestPathToAny(3, 1, cost, [{ x: 0, y: 0 }], east, 7)?.cost).toBe(9);
    expect(shortestPathToAny(3, 1, cost, [{ x: 2, y: 0 }], west, 7)?.cost).toBe(3);
  });

  it('returns owned paths and null for empty or unreachable goal sets', () => {
    const goal = [{ x: 2, y: 0, value: 'goal', buildingRank: 0, accessRank: 0 }];
    const result = shortestPathToAny(3, 1, () => 1, [{ x: 0, y: 0 }], goal, 1)!;
    floodCost(8, 8, () => 1, [{ x: 7, y: 7 }], 1);
    expect(result.path).toEqual([{ x: 2, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 0 }]);

    expect(shortestPathToAny(3, 1, () => 1, [{ x: 0, y: 0 }], [], 1)).toBeNull();
    expect(shortestPathToAny(3, 1, x => x === 1 ? 0 : 1, [{ x: 0, y: 0 }], goal, 1)).toBeNull();
  });

  it('treats goals on impassable tiles as unreachable', () => {
    const blockedGoal = [{ x: 1, y: 0, value: 'blocked', buildingRank: 0, accessRank: 0 }];
    expect(shortestPathToAny(2, 1, x => x === 1 ? 0 : 1, [{ x: 0, y: 0 }], blockedGoal, 1)).toBeNull();
    expect(shortestPathToAny(2, 1, x => x === 1 ? 0 : 1, [{ x: 1, y: 0 }], blockedGoal, 1)).toBeNull();
  });

  it('invalidates outstanding flood views while its own result remains owned', () => {
    const flood = floodCost(3, 1, () => 1, [{ x: 0, y: 0 }], 1);
    const goal = [{ x: 2, y: 0, value: 'goal', buildingRank: 0, accessRank: 0 }];
    const result = shortestPathToAny(3, 1, () => 1, [{ x: 0, y: 0 }], goal, 1)!;
    expect(() => flood.distanceAt(2, 0)).toThrow(/Stale/);
    expect(result.path).toEqual([{ x: 2, y: 0 }, { x: 1, y: 0 }, { x: 0, y: 0 }]);
  });

  it('rejects maxStep values that cannot define a valid Dial bucket ring', () => {
    const goal = [{ x: 1, y: 0, value: 'goal', buildingRank: 0, accessRank: 0 }];
    expect(() => shortestPathToAny(2, 1, () => 1, [{ x: 0, y: 0 }], goal, 0)).toThrow(/positive integer/);
    expect(() => shortestPathToAny(2, 1, () => 1, [{ x: 0, y: 0 }], goal, 1.5)).toThrow(/positive integer/);
    expect(() => shortestPathToAny(2, 1, () => 1, [{ x: 0, y: 0 }], goal, 16)).toThrow(/exceeds/);
  });
});
