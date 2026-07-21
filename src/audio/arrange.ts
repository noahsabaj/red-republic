// ============================================================
// Pure song-structure planner: turns a Track's seed + chord scheme into a
// FIXED, deterministic sequence of chord blocks that resolves on a cadence,
// with an exact duration. No WebAudio — runs in the node test harness like
// music-theory.ts. This is the "what chord plays when" skeleton; music.ts
// voices it.
//
// Determinism has two independent seeded streams:
//   - STRUCTURE (block lengths + Markov chord walk) is drawn once from a
//     single sequential mulberry32(seedOf(track)) in buildSongPlan.
//   - VOICING (arp/lead/perc note choices, in music.ts) draws from a
//     per-block stream mix(seed, blockIndex) — decorrelated per block, so
//     seeking to block K needs no replay of the blocks before it.
// ============================================================
import { mulberry32 } from '../game/mapgen';
import type { SeededRng } from '../game/mapgen';
import { nextChord } from './music-theory';
import type { Track } from './tracks';

export interface PlanBlock {
  degree: string;   // chord degree label (indexes track.chords.tones)
  bars: number;     // block length in bars
  startBar: number; // cumulative bar offset from song start
  isCadence: boolean;
}

export interface SongPlan {
  blocks: PlanBlock[];
  totalBars: number;
  secondsPerBar: number;
  durationS: number;
}

/** FNV-1a of the track id — the fallback when a Track has no explicit seed. */
function fnv1a(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 0x01000193); }
  return h >>> 0;
}

/** The structural seed for a track: its pinned `seed`, or a hash of its id. */
export function seedOf(track: Track): number {
  return (track.seed ?? fnv1a(track.id)) >>> 0;
}

/** A decorrelated per-block rng — the same golden-ratio mix the engine uses to
 *  keep independent seeded streams from lock-stepping. */
export function mix(seed: number, k: number): SeededRng {
  return mulberry32((seed ^ Math.imul(k, 0x9e3779b9)) >>> 0);
}

/**
 * Build the fixed chord-block plan for a track. Deterministic in `seedOf(track)`:
 * random block lengths + a Markov chord walk fill toward the track's `playMs`
 * target, then a cadence (default `[start]`) resolves the song onto its tonic.
 */
export function buildSongPlan(track: Track): SongPlan {
  const secondsPerBar = (track.beatsPerBar * 60) / track.bpm;
  const [lo, hi] = track.chordBars;
  const cadence = track.chords.cadence ?? [track.chords.start];
  const cadenceBars = lo;
  const targetBars = Math.max(8, Math.round(track.playMs / 1000 / secondsPerBar));
  const budget = Math.max(lo, targetBars - cadence.length * cadenceBars);

  const rng = mulberry32(seedOf(track));
  const blocks: PlanBlock[] = [];
  let degree = track.chords.start;
  let bar = 0;
  while (bar < budget) {
    const bars = lo + Math.floor(rng() * (hi - lo + 1));
    blocks.push({ degree, bars, startBar: bar, isCadence: false });
    bar += bars;
    degree = nextChord(degree, rng, track.chords.markov);
  }
  for (const d of cadence) {
    blocks.push({ degree: d, bars: cadenceBars, startBar: bar, isCadence: true });
    bar += cadenceBars;
  }
  return { blocks, totalBars: bar, secondsPerBar, durationS: bar * secondsPerBar };
}

/** Index of the block whose span contains time `t` (seconds) — for seeking. */
export function blockIndexAtTime(plan: SongPlan, t: number): number {
  const spb = plan.secondsPerBar;
  for (let i = plan.blocks.length - 1; i > 0; i--) {
    if (plan.blocks[i].startBar * spb <= t) return i;
  }
  return 0;
}
