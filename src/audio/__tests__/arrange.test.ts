import { describe, expect, it } from 'vitest';
import { PLAYLIST } from '../tracks';
import { blockIndexAtTime, buildSongPlan } from '../arrange';

describe('buildSongPlan', () => {
  it('is deterministic — same track builds the identical plan', () => {
    for (const t of PLAYLIST) expect(buildSongPlan(t)).toEqual(buildSongPlan(t));
  });

  for (const t of PLAYLIST) {
    describe(t.id, () => {
      const plan = buildSongPlan(t);

      it('lands near the playMs target', () => {
        expect(plan.durationS).toBeGreaterThan((t.playMs / 1000) * 0.75);
        expect(plan.durationS).toBeLessThan((t.playMs / 1000) * 1.25);
      });

      it('every block degree is a real chord in the scheme', () => {
        for (const b of plan.blocks) expect(t.chords.tones[b.degree]).toBeDefined();
      });

      it('resolves onto the tonic — last block is the cadence start degree', () => {
        const last = plan.blocks[plan.blocks.length - 1];
        expect(last.degree).toBe(t.chords.start);
        expect(last.isCadence).toBe(true);
      });

      it('block spans are contiguous and sum to totalBars', () => {
        let bar = 0;
        for (const b of plan.blocks) { expect(b.startBar).toBe(bar); bar += b.bars; }
        expect(bar).toBe(plan.totalBars);
        expect(plan.durationS).toBeCloseTo(plan.totalBars * plan.secondsPerBar, 6);
      });
    });
  }

  it('blockIndexAtTime maps a time to its containing block', () => {
    const plan = buildSongPlan(PLAYLIST[0]);
    expect(blockIndexAtTime(plan, 0)).toBe(0);
    const mid = plan.blocks[2];
    expect(blockIndexAtTime(plan, mid.startBar * plan.secondsPerBar + 0.1)).toBe(2);
    expect(blockIndexAtTime(plan, plan.durationS)).toBe(plan.blocks.length - 1);
  });
});
