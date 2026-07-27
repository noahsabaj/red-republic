// ============================================================
// Pure music math for the generative score — no WebAudio imports, so
// every table and picker is unit-testable in Node.
//
// Everything is centered on A natural minor. Semitone offsets are from
// the root A; the pad plays around A2, melodies an octave or two up.
// ============================================================

export const ROOT_MIDI = 45; // A2

export type Degree = 'i' | 'III' | 'iv' | 'v' | 'VI' | 'VII';

/** Chord tones as semitone offsets from the key root (not the chord root). */
export const CHORD_TONES: Record<Degree, [number, number, number]> = {
  i: [0, 3, 7],
  III: [3, 7, 10],
  iv: [5, 8, 12],
  v: [7, 10, 14],
  VI: [8, 12, 15],
  VII: [10, 14, 17],
};

/** Markov transitions — slow, brooding minor-key wandering that resolves home. */
export const CHORD_MARKOV: Record<Degree, [Degree, number][]> = {
  i: [['VI', 0.30], ['VII', 0.25], ['iv', 0.20], ['v', 0.15], ['III', 0.10]],
  VI: [['VII', 0.40], ['i', 0.35], ['iv', 0.25]],
  VII: [['i', 0.60], ['VI', 0.25], ['v', 0.15]],
  iv: [['i', 0.40], ['v', 0.30], ['VI', 0.30]],
  v: [['i', 0.70], ['VI', 0.30]],
  III: [['VII', 0.50], ['iv', 0.30], ['VI', 0.20]],
};

/** A-minor pentatonic — the melody fragment vocabulary. */
export const PENTATONIC = [0, 3, 5, 7, 10] as const;

export function midiToHz(midi: number): number {
  return 440 * 2 ** ((midi - 69) / 12);
}

/**
 * Fundamental for a UI click voiced in-key: the `index`-th tone of the
 * given chord (as semitone offsets from the key root), raised `octave`
 * octaves above the pad root. Pure — the click SFX pitch to the live
 * chord through this so they're always consonant with the score.
 * clickHz(CHORD_TONES.i) === 220 (A3).
 */
export function clickHz(chord: readonly number[], index = 0, octave = 1, root = ROOT_MIDI): number {
  const tone = chord[((index % chord.length) + chord.length) % chord.length] ?? 0;
  return midiToHz(root + tone + octave * 12);
}

/**
 * Weighted-random next degree. The chord table defaults to the module
 * A-minor Markov graph, so `nextChord(d, rng)` is unchanged; a track with
 * its own key/progression passes its own `markov`.
 */
export function nextChord<D extends string>(
  current: D,
  rng: () => number,
  markov: Record<D, [D, number][]> = CHORD_MARKOV as unknown as Record<D, [D, number][]>,
): D {
  const row = markov[current];
  let roll = rng();
  for (const [degree, p] of row) {
    roll -= p;
    if (roll < 0) return degree;
  }
  return row[row.length - 1][0];
}

/**
 * A short melodic fragment over a chord: 3-6 scale offsets, starting on a
 * chord tone, wandering by small steps, ending near the chord root. Tables
 * default to the module A-minor pentatonic; a track passes its own.
 */
export function phrase<D extends string>(
  degree: D,
  rng: () => number,
  tones: Record<D, [number, number, number]> = CHORD_TONES as unknown as Record<D, [number, number, number]>,
  scale: readonly number[] = PENTATONIC,
): number[] {
  const chord = tones[degree];
  const len = 3 + Math.floor(rng() * 4);
  const notes: number[] = [];
  // start on a chord tone that exists in the scale (fall back to root)
  const starts = chord.filter(t => scale.includes(t % 12));
  let idx = scale.indexOf((starts[Math.floor(rng() * starts.length)] ?? chord[0]) % 12);
  if (idx < 0) idx = 0;
  for (let i = 0; i < len; i++) {
    notes.push(scale[idx]);
    const step = rng() < 0.5 ? -1 : 1;
    idx = Math.min(scale.length - 1, Math.max(0, idx + step));
  }
  return notes;
}
