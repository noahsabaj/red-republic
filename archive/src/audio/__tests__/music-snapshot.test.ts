import { describe, expect, it } from 'vitest';
import { MusicEngine } from '../music';
import { PLAYLIST } from '../tracks';
import { buildSongPlan } from '../arrange';
import { RecordingContext } from './recording-context';

/**
 * The soundtrack's tripwire, in the spirit of mapgen-snapshot.test.ts.
 *
 * The product claim is that these are FIXED works — the same performance every
 * broadcast. Nothing held that claim: the determinism suite proves a track is
 * reproducible against ITSELF within a run, which stays true no matter how far
 * the music drifts between runs. F3 is what that gap costs — scene intensity
 * had been changing which notes played, and every determinism test passed
 * throughout, because each one constructed the engine at the same default.
 *
 * So this pins the actual notes. The recording context logs pitch and timing
 * but never gain or filter cutoff, which makes the captured stream precisely
 * the SCORE with the MIX excluded — a mix change (levels, brightness, the
 * menu/field balance) is free, and a note change is a build break.
 *
 * As with mapgen-snapshot: when a deliberate change moves the score, re-derive
 * the hashes and re-pin them. NEVER repin to make a change pass.
 *
 * The pin is a hash rather than the note list because the lists run to ~1300
 * events per track. When one fails, print the streams and diff them — or wait
 * for the score/render split, after which this can pin NoteEvent[] directly
 * and say which note at which bar moved.
 */

/** FNV-1a over the canonical note stream — stable across platforms, unlike
 *  anything involving object key order or float formatting drift. */
function hashStream(parts: string[]): string {
  let h = 0x811c9dc5;
  for (const s of parts) {
    for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
  }
  return (h >>> 0).toString(16).padStart(8, '0');
}

/** Play a track end to end at the default intensity and with no mood probe,
 *  capturing every scheduled voice as `kind:freq@start-stop`. */
function captureScore(trackIndex: number): { hash: string; notes: number } {
  const ctx = new RecordingContext();
  const eng = new MusicEngine(ctx as unknown as BaseAudioContext, ctx.createGain() as unknown as AudioNode);
  eng.playTrack(PLAYLIST[trackIndex], { crossfadeS: 0 });
  const dur = eng.durationS();
  for (let t = 0; t <= dur + 1; t += 0.5) { ctx.currentTime = t; eng.pump(t); }
  eng.dispose();
  const parts = ctx.log.map(e => `${e.kind}:${e.freq.toFixed(4)}@${e.start.toFixed(4)}-${e.stop.toFixed(4)}`);
  return { hash: hashStream(parts), notes: parts.length };
}

/** id → [hash, note count, duration seconds]. Derived 2026-07-26. */
const PINNED: Record<string, [string, number, number]> = {
  anthem:     ['88ddf6e1', 216, 190.5882],
  march:      ['1dea59e3', 1340, 175.7143],
  waltz:      ['c9212b10', 929, 176.4],
  industrial: ['b0e00a40', 2311, 172.5],
  nocturne:   ['fef8704a', 136, 216.9231],
  radio:      ['cb94ede9', 91, 212],
};

describe('the soundtrack is pinned', () => {
  it('every track plays its recorded performance, note for note', () => {
    const drift: string[] = [];
    for (const [i, track] of PLAYLIST.entries()) {
      const { hash, notes } = captureScore(i);
      const duration = Number(buildSongPlan(track).durationS.toFixed(4));
      const [pinHash, pinNotes, pinDur] = PINNED[track.id] ?? ['<unpinned>', 0, 0];
      if (hash !== pinHash || notes !== pinNotes || duration !== pinDur) {
        drift.push(`${track.id}: ['${hash}', ${notes}, ${duration}]  (pinned: ['${pinHash}', ${pinNotes}, ${pinDur}])`);
      }
    }
    expect(drift, `the score moved. If that was deliberate, re-pin to:\n${drift.join('\n')}`).toEqual([]);
  });

  it('pins every track in the programme', () => {
    // A pin that silently stops covering a track is the failure mode of every
    // guard in this repo; adding a seventh song must not slip past unpinned.
    for (const t of PLAYLIST) expect(PINNED[t.id], `${t.id} is not pinned`).toBeDefined();
    expect(Object.keys(PINNED).sort()).toEqual(PLAYLIST.map(t => t.id).sort());
  });
});
