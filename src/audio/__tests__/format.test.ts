import { describe, expect, it } from 'vitest';
import { fmtClock } from '../format';

describe('fmtClock', () => {
  it('formats seconds as M:SS with a zero-padded seconds field', () => {
    expect(fmtClock(0)).toBe('0:00');
    expect(fmtClock(5)).toBe('0:05');
    expect(fmtClock(83)).toBe('1:23');
    expect(fmtClock(190)).toBe('3:10');
    expect(fmtClock(600)).toBe('10:00');
  });
  it('floors fractions and clamps negatives', () => {
    expect(fmtClock(83.9)).toBe('1:23');
    expect(fmtClock(-5)).toBe('0:00');
  });
});
