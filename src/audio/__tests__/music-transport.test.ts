import { afterEach, describe, expect, it, vi } from 'vitest';
import { MusicEngine } from '../music';
import { PLAYLIST } from '../tracks';
import { RecordingContext } from './recording-context';

/**
 * Transport behaviour — play/pause/resume/seek as a state machine, as opposed
 * to music-determinism.test.ts which asserts the note STREAM is reproducible.
 * The distinction matters: every case here passed the determinism suite while
 * being audibly wrong, because a bit-identical stream of the wrong four
 * seconds is still bit-identical.
 */
function engineFor(trackIndex = 1) {
  const ctx = new RecordingContext();
  const eng = new MusicEngine(ctx as unknown as BaseAudioContext, ctx.createGain() as unknown as AudioNode);
  eng.playTrack(PLAYLIST[trackIndex], { crossfadeS: 0 });
  return { ctx, eng };
}

function pumpTo(ctx: RecordingContext, eng: MusicEngine, from: number, to: number, step = 0.5) {
  for (let t = from; t <= to; t += step) { ctx.currentTime = t; eng.pump(t); }
}

describe('resuming a finished song (F2)', () => {
  it('restarts from the top rather than replaying the final block', () => {
    const { ctx, eng } = engineFor();
    let ended = 0;
    eng.onEnded = () => { ended++; };
    const dur = eng.durationS();

    pumpTo(ctx, eng, 0, dur + 1);
    expect(ended).toBe(1);

    // What AudioSystem.onSongEnded does when repeat is 'off' at the last track.
    eng.setPlaying(false);
    expect(eng.elapsedS()).toBeCloseTo(dur, 0); // the playhead is pinned at the end

    // The player presses Play. Before the fix this re-entered the block
    // containing durationS — 4.3 s of the closing chord, then it ended again.
    eng.setPlaying(true);
    expect(eng.elapsedS()).toBeLessThan(1);

    let reEnded = 0;
    eng.onEnded = () => { reEnded++; };
    const t0 = ctx.currentTime;
    pumpTo(ctx, eng, t0, t0 + 30);
    expect(reEnded).toBe(0); // still playing 30 s later; it used to stop after 4.5 s
    eng.dispose();
  });

  it('still resumes mid-song when merely paused', () => {
    const { ctx, eng } = engineFor();
    const dur = eng.durationS();
    const pauseAt = Math.round(dur * 0.4);

    pumpTo(ctx, eng, 0, pauseAt);
    ctx.currentTime = pauseAt;
    eng.setPlaying(false);
    expect(eng.elapsedS()).toBeCloseTo(pauseAt, 0);

    ctx.currentTime = pauseAt + 5;
    eng.setPlaying(true);
    // Back near where it paused (snapped to the containing chord block), NOT
    // restarted — the fix keys off `ended`, not off a proximity heuristic.
    expect(eng.elapsedS()).toBeGreaterThan(dur * 0.2);
    expect(eng.elapsedS()).toBeLessThanOrEqual(pauseAt);
    eng.dispose();
  });
});

describe('scheduler lifetime (F4)', () => {
  afterEach(() => { vi.useRealTimers(); });

  it('the pump interval stops when the music stops and returns on resume', () => {
    vi.useFakeTimers();
    const ctx = new RecordingContext();
    const eng = new MusicEngine(ctx as unknown as BaseAudioContext, ctx.createGain() as unknown as AudioNode);

    // crossfadeS 0 with no previous generation schedules no tail timer, so the
    // only timer in flight is the scheduler itself.
    eng.playTrack(PLAYLIST[1], { crossfadeS: 0 });
    expect(vi.getTimerCount()).toBe(1);

    eng.setPlaying(false);
    expect(vi.getTimerCount()).toBe(0); // used to run for the life of the page

    // Resume brings it back. Asserted functionally rather than by count,
    // because resuming also registers a crossfade tail timer — the point is
    // that scheduling actually happens again, not that a timer exists.
    eng.setPlaying(true);
    const before = ctx.log.length;
    ctx.currentTime += 1;
    eng.pump(ctx.currentTime);
    expect(ctx.log.length).toBeGreaterThan(before);
    eng.dispose();
  });
});
