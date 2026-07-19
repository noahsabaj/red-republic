import { describe, expect, it } from 'vitest';
import { mulberry32 } from '../../game/mapgen';
import {
  CHORD_MARKOV, CHORD_TONES, PENTATONIC, ROOT_MIDI, clickHz, midiToHz, nextChord, phrase,
} from '../music-theory';
import type { Degree } from '../music-theory';

const DEGREES = Object.keys(CHORD_TONES) as Degree[];

describe('music theory tables', () => {
  it('every Markov row sums to 1 and only names real degrees', () => {
    for (const d of DEGREES) {
      const row = CHORD_MARKOV[d];
      expect(row.reduce((a, [, p]) => a + p, 0)).toBeCloseTo(1, 9);
      for (const [target] of row) expect(DEGREES).toContain(target);
    }
  });

  it('midiToHz hits the anchors', () => {
    expect(midiToHz(69)).toBeCloseTo(440, 9);   // A4
    expect(midiToHz(ROOT_MIDI)).toBeCloseTo(110, 6); // A2
    expect(midiToHz(57)).toBeCloseTo(220, 6);
  });

  it('clickHz voices chord tones in-key, positive for every degree/index', () => {
    expect(clickHz(CHORD_TONES.i)).toBeCloseTo(220, 6);              // root, +1 octave = A3
    expect(clickHz(CHORD_TONES.i, 0, 0)).toBeCloseTo(110, 6);        // root at the pad octave = A2
    expect(clickHz(CHORD_TONES.i, 2, 1)).toBeCloseTo(midiToHz(ROOT_MIDI + 7 + 12), 6); // fifth
    // must stay finite & positive — clicks feed exponential frequency ramps
    for (const d of DEGREES) {
      for (let i = 0; i < 3; i++) {
        const hz = clickHz(CHORD_TONES[d], i, 1);
        expect(Number.isFinite(hz)).toBe(true);
        expect(hz).toBeGreaterThan(0);
      }
    }
    expect(clickHz(CHORD_TONES.i, 5)).toBeGreaterThan(0); // index wraps, never NaN
  });
});

describe('generative pickers', () => {
  it('nextChord is deterministic under a seeded rng and stays in the degree set', () => {
    const walk = (seed: number) => {
      const rng = mulberry32(seed);
      let d: Degree = 'i';
      const path: Degree[] = [];
      for (let i = 0; i < 50; i++) { d = nextChord(d, rng); path.push(d); }
      return path;
    };
    expect(walk(7)).toEqual(walk(7));
    for (const d of walk(42)) expect(DEGREES).toContain(d);
  });

  it('phrases stay inside the pentatonic set, 3-6 notes', () => {
    const rng = mulberry32(3);
    for (let i = 0; i < 30; i++) {
      const notes = phrase(DEGREES[i % DEGREES.length], rng);
      expect(notes.length).toBeGreaterThanOrEqual(3);
      expect(notes.length).toBeLessThanOrEqual(6);
      for (const n of notes) expect(PENTATONIC).toContain(n);
    }
  });
});
