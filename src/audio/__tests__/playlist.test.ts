import { describe, expect, it } from 'vitest';
import { mulberry32 } from '../../game/mapgen';
import { autoAdvance, naturalOrder, orderPos, shuffledOrder, stepPos } from '../playlist';

describe('playlist ordering', () => {
  it('naturalOrder is 0..n-1', () => {
    expect(naturalOrder(4)).toEqual([0, 1, 2, 3]);
  });

  it('shuffledOrder is a deterministic permutation of 0..n-1', () => {
    const a = shuffledOrder(6, mulberry32(11));
    const b = shuffledOrder(6, mulberry32(11));
    expect(a).toEqual(b); // same seed → same order
    expect([...a].sort((x, y) => x - y)).toEqual([0, 1, 2, 3, 4, 5]); // no lost/dup indices
  });

  it('orderPos locates a playlist index, 0 when absent', () => {
    const order = [3, 1, 2, 0];
    expect(orderPos(order, 2)).toBe(2);
    expect(orderPos(order, 3)).toBe(0);
    expect(orderPos(order, 99)).toBe(0);
  });

  it('stepPos wraps both directions and guards empty', () => {
    expect(stepPos(0, -1, 4)).toBe(3);
    expect(stepPos(3, 1, 4)).toBe(0);
    expect(stepPos(1, 1, 4)).toBe(2);
    expect(stepPos(0, 1, 0)).toBe(0);
  });
});

describe('autoAdvance', () => {
  it("repeat 'one' holds the current track", () => {
    expect(autoAdvance(2, 5, 'one')).toEqual({ pos: 2, reshuffle: false, stop: false, sameTrack: true });
  });

  it("repeat 'all' advances, reshuffling on wrap", () => {
    expect(autoAdvance(1, 4, 'all')).toEqual({ pos: 2, reshuffle: false, stop: false, sameTrack: false });
    expect(autoAdvance(3, 4, 'all')).toEqual({ pos: 0, reshuffle: true, stop: false, sameTrack: false });
  });

  it("repeat 'off' advances then stops at the end", () => {
    expect(autoAdvance(1, 4, 'off')).toEqual({ pos: 2, reshuffle: false, stop: false, sameTrack: false });
    expect(autoAdvance(3, 4, 'off').stop).toBe(true);
  });

  it('an empty playlist stops', () => {
    expect(autoAdvance(0, 0, 'all').stop).toBe(true);
  });
});
