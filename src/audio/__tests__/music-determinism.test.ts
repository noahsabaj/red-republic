import { describe, expect, it } from 'vitest';
import { MusicEngine } from '../music';
import { PLAYLIST } from '../tracks';
import type { Track } from '../tracks';
import { buildSongPlan } from '../arrange';
import { RecordingContext } from './recording-context';
import type { Ev } from './recording-context';

/**
 * The tracks are now FIXED songs — the same seed produces the same note stream
 * every play. These tests are the "deaf author's" proof of that: they capture
 * every scheduled note (freq/start/stop) through a recording fake AudioContext
 * and assert the stream is bit-identical across plays, seeks, and pauses.
 */
function engineFor(track: Track) {
  const ctx = new RecordingContext();
  const eng = new MusicEngine(ctx as unknown as BaseAudioContext, ctx.createGain() as unknown as AudioNode);
  eng.playTrack(track, { crossfadeS: 0 });
  return { ctx, eng };
}

function pumpTo(ctx: RecordingContext, eng: MusicEngine, from: number, to: number, step = 0.5) {
  for (let t = from; t <= to; t += step) { ctx.currentTime = t; eng.pump(t); }
}

function capture(track: Track): Ev[] {
  const { ctx, eng } = engineFor(track);
  pumpTo(ctx, eng, 0, eng.durationS() + 1);
  eng.dispose();
  return ctx.log;
}

function captureSeek(track: Track, at: number): Ev[] {
  const { ctx, eng } = engineFor(track);
  pumpTo(ctx, eng, 0, at);
  ctx.currentTime = at; eng.seek(at);
  pumpTo(ctx, eng, at, eng.durationS() + 1);
  eng.dispose();
  return ctx.log;
}

function capturePause(track: Track): Ev[] {
  const { ctx, eng } = engineFor(track);
  const dur = eng.durationS();
  const pauseAt = Math.round(dur * 0.4);
  pumpTo(ctx, eng, 0, pauseAt);
  ctx.currentTime = pauseAt; eng.setPlaying(false);
  ctx.currentTime = pauseAt + 5; eng.setPlaying(true); // 5 s paused, then resume
  pumpTo(ctx, eng, pauseAt + 5, dur + 6);
  eng.dispose();
  return ctx.log;
}

describe('music determinism', () => {
  for (const t of PLAYLIST) {
    it(`${t.id}: bit-identical every play`, () => {
      expect(capture(t)).toEqual(capture(t));
    });
  }

  it('different tracks produce different note streams', () => {
    expect(capture(PLAYLIST[0])).not.toEqual(capture(PLAYLIST[1]));
  });

  it('schedules real notes and rings out around durationS', () => {
    for (const t of PLAYLIST) {
      const log = capture(t);
      expect(log.length).toBeGreaterThan(10);
      const lastStop = Math.max(...log.map(e => e.stop));
      const dur = buildSongPlan(t).durationS;
      expect(lastStop).toBeGreaterThan(dur - 15);
      expect(lastStop).toBeLessThan(dur + 15);
    }
  });

  it('seek is deterministic (same seek twice → identical)', () => {
    const t = PLAYLIST[0];
    const at = buildSongPlan(t).durationS * 0.5;
    expect(captureSeek(t, at)).toEqual(captureSeek(t, at));
  });

  it('pause/resume is deterministic', () => {
    const t = PLAYLIST[3]; // Tempo of the Five-Year Plan — arp + perc voicing rng
    expect(capturePause(t)).toEqual(capturePause(t));
  });
});
