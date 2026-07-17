import { describe, expect, it } from 'vitest';
import { updateSelection, type SelectionItem } from '../selection';
import { layRoad, makeEngine, placeBuilt } from './helpers';

const bld = (id: number): SelectionItem => ({ kind: 'building', id });
const dep = (x: number, y: number): SelectionItem => ({ kind: 'deposit', x, y });

describe('updateSelection', () => {
  it('plain click replaces; empty ground clears', () => {
    expect(updateSelection([bld(1), dep(2, 3)], bld(9), false)).toEqual([bld(9)]);
    expect(updateSelection([bld(1)], null, false)).toEqual([]);
  });

  it('additive click adds new items and keeps order', () => {
    const s1 = updateSelection([], bld(1), false);
    const s2 = updateSelection(s1, dep(4, 4), true);
    const s3 = updateSelection(s2, bld(2), true);
    expect(s3).toEqual([bld(1), dep(4, 4), bld(2)]);
  });

  it('additive click on a selected item removes it (toggle)', () => {
    const sel = [bld(1), dep(4, 4), bld(2)];
    expect(updateSelection(sel, dep(4, 4), true)).toEqual([bld(1), bld(2)]);
    expect(updateSelection(sel, bld(1), true)).toEqual([dep(4, 4), bld(2)]);
  });

  it('additive click on empty ground keeps the selection', () => {
    const sel = [bld(1), dep(4, 4)];
    expect(updateSelection(sel, null, true)).toBe(sel);
  });

  it('deposit identity is per tile', () => {
    const sel = updateSelection([dep(4, 4)], dep(4, 5), true);
    expect(sel).toEqual([dep(4, 4), dep(4, 5)]);
  });
});

describe('setStaffPriorityMany', () => {
  it('sets priority on all given buildings with a single version bump', () => {
    const e = makeEngine();
    layRoad(e, 8, 9, 16, 9);
    const a = placeBuilt(e, 'sawmill', 10, 10);
    const b = placeBuilt(e, 'sawmill', 12, 10);
    const c = placeBuilt(e, 'sawmill', 14, 10);
    c.priorityHigh = true;

    const v0 = e.getVersion();
    e.setStaffPriorityMany([a.id, b.id, c.id], true);
    expect([a.priorityHigh, b.priorityHigh, c.priorityHigh]).toEqual([true, true, true]);
    expect(e.getVersion()).toBe(v0 + 1); // one bump, not three

    e.setStaffPriorityMany([a.id, b.id, c.id], true); // no-op → no bump
    expect(e.getVersion()).toBe(v0 + 1);

    e.setStaffPriorityMany([a.id, 999999], false); // unknown ids are ignored
    expect(a.priorityHigh).toBe(false);
  });
});
