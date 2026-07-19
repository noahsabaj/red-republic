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
export function clickHz(chord: readonly number[], index = 0, octave = 1): number {
  const tone = chord[((index % chord.length) + chord.length) % chord.length] ?? 0;
  return midiToHz(ROOT_MIDI + tone + octave * 12);
}

export function nextChord(current: Degree, rng: () => number): Degree {
  const row = CHORD_MARKOV[current];
  let roll = rng();
  for (const [degree, p] of row) {
    roll -= p;
    if (roll < 0) return degree;
  }
  return row[row.length - 1][0];
}

/**
 * A short melodic fragment over a chord: 3-6 pentatonic offsets, starting
 * on a chord tone, wandering by small steps, ending near the chord root.
 */
export function phrase(degree: Degree, rng: () => number): number[] {
  const tones = CHORD_TONES[degree];
  const len = 3 + Math.floor(rng() * 4);
  const notes: number[] = [];
  // start on a chord tone that exists in the pentatonic set (fall back to root)
  const starts = tones.filter(t => (PENTATONIC as readonly number[]).includes(t % 12));
  let idx = PENTATONIC.indexOf(((starts[Math.floor(rng() * starts.length)] ?? tones[0]) % 12) as typeof PENTATONIC[number]);
  if (idx < 0) idx = 0;
  for (let i = 0; i < len; i++) {
    notes.push(PENTATONIC[idx]);
    const step = rng() < 0.5 ? -1 : 1;
    idx = Math.min(PENTATONIC.length - 1, Math.max(0, idx + step));
  }
  return notes;
}
