// ============================================================
// The soundtrack catalog — data only, zero WebAudio. Every "song" is a
// Track descriptor: a key root, a chord scheme (tones + Markov + start),
// a beat grid (bpm / beatsPerBar / chord length in bars), an auto-advance
// duration, and a stack of synth layers. MusicEngine reads a Track and
// voices it; nothing here touches an AudioContext, so this module (and the
// tests over it) run in the plain node harness like music-theory.ts.
//
// The layer params generalize what music.ts used to inline: pad/sub/lead
// reproduce the old ambient voice (in the track's key), while bass/arp/perc
// add a rhythm section so a march sounds nothing like a nocturne.
// ============================================================
import { CHORD_MARKOV, CHORD_TONES, PENTATONIC } from './music-theory';
import type { Degree } from './music-theory';

/** Chord tones are semitone offsets from `Track.root` (not the chord root). */
export interface ChordScheme<D extends string = string> {
  tones: Record<D, [number, number, number]>;
  markov: Record<D, [D, number][]>; // rows sum to 1 (asserted in tests)
  start: D;                          // starting degree on (re)play
}

export interface Envelope { attack: number; release: number } // seconds

/** Sustained detuned-oscillator chord bed — the old pad voice. */
export interface PadLayer {
  kind: 'pad';
  osc: OscillatorType;
  detune: number[];       // cents, one voice per entry
  env: Envelope;
  level: number;          // base gain, scaled by intensity
  cutoffBase: number;     // lowpass Hz at intensity 0
  cutoffSpan: number;     // added at intensity 1
  dropThirdWhenCold?: boolean; // winter open-fifth voicing
}

/** Sine sub an octave (or two) below the chord root. */
export interface SubLayer {
  kind: 'sub';
  osc: OscillatorType;
  env: Envelope;
  level: number;
  octave: number;         // semitones from root (usually -12)
}

/** Plucked bass on a per-bar beat pattern. */
export interface BassLayer {
  kind: 'bass';
  osc: OscillatorType;
  env: Envelope;
  level: number;
  octave: number;
  pattern: number[];      // beat indices within the bar to strike
  toneIndex?: number[];   // chord-tone index per hit (parallel to pattern; default root)
  glideToNext?: boolean;  // last hit of a block glides toward the next chord's root
}

/** Subdivided arpeggio / pulse over the chord tones. */
export interface ArpLayer {
  kind: 'arp';
  osc: OscillatorType;
  env: Envelope;
  level: number;
  octave: number;
  subdivision: number;    // steps per beat (2 = 8ths, 4 = 16ths)
  pattern: number[];      // chord-tone index sequence, cycled across steps
  gate: number;           // note length as a fraction of a step
  density: number;        // 0..1 probability a step sounds
}

/** One filtered-noise percussion voice (kick / snare / hat). */
export interface PercVoice {
  on: number[];           // beat indices within the bar
  filter: BiquadFilterType;
  freq: number;
  decay: number;          // seconds
  peak: number;
}
export interface PercLayer {
  kind: 'perc';
  level: number;          // scales every voice, times intensity
  steps: PercVoice[];
}

/** Occasional melodic fragment — the old lead voice, parameterized. */
export interface LeadLayer {
  kind: 'lead';
  osc: OscillatorType;
  env: Envelope;
  level: number;
  octaves: number[];      // candidate octave offsets (semitones)
  chance: number;         // base probability per chord block
  scale: readonly number[];
}

export type Layer = PadLayer | SubLayer | BassLayer | ArpLayer | PercLayer | LeadLayer;

export interface Track {
  id: string;               // stable key, persisted in settings
  name: string;             // Soviet-institutional display name
  root: number;             // MIDI of the key root (45 = A2)
  mode: string;             // label only
  chords: ChordScheme;
  melody: readonly number[]; // scale handed to phrase() for lead layers
  bpm: number;              // beat = 60 / bpm
  beatsPerBar: number;      // 4, or 3 for a waltz
  chordBars: [number, number]; // chord length in bars [min, max]
  playMs: number;           // auto-advance duration (~2.5–4 min)
  layers: Layer[];
}

// ---------------- shared scales & schemes ----------------

const MAJOR_PENTA = [0, 2, 4, 7, 9] as const;

/** Natural-minor progression, reusing the pinned A-minor tables. Key-agnostic
 *  (offsets from root), so Anthem / Nocturne / Menu share it in different keys. */
const MINOR_SCHEME: ChordScheme<Degree> = { tones: CHORD_TONES, markov: CHORD_MARKOV, start: 'i' };

type DorianDeg = 'i' | 'III' | 'IV' | 'v' | 'VII';
const DORIAN_SCHEME: ChordScheme<DorianDeg> = {
  tones: { i: [0, 3, 7], III: [3, 7, 10], IV: [5, 9, 12], v: [7, 10, 14], VII: [10, 14, 17] },
  markov: {
    i:   [['IV', 0.30], ['VII', 0.30], ['v', 0.20], ['III', 0.20]],
    IV:  [['i', 0.50], ['v', 0.30], ['VII', 0.20]],
    v:   [['i', 0.60], ['IV', 0.40]],
    VII: [['i', 0.50], ['IV', 0.30], ['III', 0.20]],
    III: [['VII', 0.50], ['IV', 0.30], ['i', 0.20]],
  },
  start: 'i',
};

type MajorDeg = 'I' | 'ii' | 'IV' | 'V' | 'vi';
const MAJOR_SCHEME: ChordScheme<MajorDeg> = {
  tones: { I: [0, 4, 7], ii: [2, 5, 9], IV: [5, 9, 12], V: [7, 11, 14], vi: [9, 12, 16] },
  markov: {
    I:  [['V', 0.30], ['IV', 0.30], ['vi', 0.25], ['ii', 0.15]],
    ii: [['V', 0.60], ['IV', 0.40]],
    IV: [['V', 0.40], ['I', 0.35], ['ii', 0.25]],
    V:  [['I', 0.70], ['vi', 0.30]],
    vi: [['IV', 0.40], ['ii', 0.30], ['V', 0.30]],
  },
  start: 'I',
};

// ---------------- the catalog ----------------

/** Solemn, grand — A minor with a slow stately root pulse. */
const ANTHEM: Track = {
  id: 'anthem', name: 'Anthem of the Toiling Masses', root: 45, mode: 'A minor',
  chords: MINOR_SCHEME, melody: PENTATONIC,
  bpm: 68, beatsPerBar: 4, chordBars: [2, 4], playMs: 190_000,
  layers: [
    { kind: 'pad', osc: 'sawtooth', detune: [-4, 4], env: { attack: 2.5, release: 4 }, level: 0.16, cutoffBase: 320, cutoffSpan: 560, dropThirdWhenCold: true },
    { kind: 'sub', osc: 'sine', env: { attack: 2, release: 3.5 }, level: 0.09, octave: -12 },
    { kind: 'bass', osc: 'triangle', env: { attack: 0.01, release: 0.9 }, level: 0.15, octave: -12, pattern: [0], toneIndex: [0], glideToNext: true },
    { kind: 'lead', osc: 'triangle', env: { attack: 0.3, release: 1.4 }, level: 0.09, octaves: [24, 36], chance: 0.35, scale: PENTATONIC },
  ],
};

/** Industrial pulse — D dorian, driving 16th arp over kick/snare/hat, no lead. */
const INDUSTRIAL: Track = {
  id: 'industrial', name: 'Tempo of the Five-Year Plan', root: 38, mode: 'D dorian',
  chords: DORIAN_SCHEME, melody: PENTATONIC,
  bpm: 128, beatsPerBar: 4, chordBars: [2, 2], playMs: 170_000,
  layers: [
    { kind: 'pad', osc: 'sawtooth', detune: [-6, 6], env: { attack: 0.8, release: 1.5 }, level: 0.08, cutoffBase: 400, cutoffSpan: 700 },
    { kind: 'sub', osc: 'sine', env: { attack: 0.2, release: 0.8 }, level: 0.10, octave: -12 },
    { kind: 'arp', osc: 'square', env: { attack: 0.004, release: 0.09 }, level: 0.07, octave: 12, subdivision: 4, pattern: [0, 1, 2, 1, 0, 2, 1, 2], gate: 0.5, density: 0.85 },
    { kind: 'perc', level: 0.5, steps: [
      { on: [0, 2], filter: 'lowpass', freq: 130, decay: 0.14, peak: 0.55 },
      { on: [1, 3], filter: 'highpass', freq: 2000, decay: 0.05, peak: 0.22 },
      { on: [0, 1, 2, 3], filter: 'bandpass', freq: 6000, decay: 0.03, peak: 0.10 },
    ] },
  ],
};

/** Folk waltz — C major in 3/4, oom-pah broken chord + lilting lead. */
const WALTZ: Track = {
  id: 'waltz', name: 'Waltz of the Collective Harvest', root: 48, mode: 'C major',
  chords: MAJOR_SCHEME, melody: MAJOR_PENTA,
  bpm: 150, beatsPerBar: 3, chordBars: [2, 4], playMs: 175_000,
  layers: [
    { kind: 'pad', osc: 'triangle', detune: [-4, 4], env: { attack: 0.6, release: 1.4 }, level: 0.11, cutoffBase: 600, cutoffSpan: 800 },
    { kind: 'bass', osc: 'triangle', env: { attack: 0.01, release: 0.5 }, level: 0.16, octave: -12, pattern: [0], toneIndex: [0] },
    { kind: 'arp', osc: 'sine', env: { attack: 0.01, release: 0.35 }, level: 0.09, octave: 12, subdivision: 1, pattern: [0, 1, 2], gate: 0.8, density: 0.9 },
    { kind: 'lead', osc: 'triangle', env: { attack: 0.05, release: 0.8 }, level: 0.10, octaves: [12, 24], chance: 0.5, scale: MAJOR_PENTA },
  ],
};

/** Cold nocturne — B minor, sparse pads + rare lead (the old ambient voice). */
const NOCTURNE: Track = {
  id: 'nocturne', name: 'Nocturne of the Northern Watch', root: 47, mode: 'B minor',
  chords: MINOR_SCHEME, melody: PENTATONIC,
  bpm: 52, beatsPerBar: 4, chordBars: [2, 4], playMs: 210_000,
  layers: [
    { kind: 'pad', osc: 'sawtooth', detune: [-4, 4], env: { attack: 3, release: 4 }, level: 0.16, cutoffBase: 260, cutoffSpan: 500, dropThirdWhenCold: true },
    { kind: 'sub', osc: 'sine', env: { attack: 3, release: 4 }, level: 0.09, octave: -12 },
    { kind: 'lead', osc: 'triangle', env: { attack: 0.3, release: 1.4 }, level: 0.09, octaves: [24, 36], chance: 0.35, scale: PENTATONIC },
  ],
};

/** Triumphant march — G major, full rhythm section (root/fifth bass, 8th arp, kick/snare). */
const MARCH: Track = {
  id: 'march', name: 'March of the Vanguard', root: 43, mode: 'G major',
  chords: MAJOR_SCHEME, melody: MAJOR_PENTA,
  bpm: 112, beatsPerBar: 4, chordBars: [2, 4], playMs: 175_000,
  layers: [
    { kind: 'pad', osc: 'sawtooth', detune: [-3, 3], env: { attack: 1, release: 2.2 }, level: 0.12, cutoffBase: 520, cutoffSpan: 900 },
    { kind: 'sub', osc: 'sine', env: { attack: 0.5, release: 1.4 }, level: 0.10, octave: -12 },
    { kind: 'bass', osc: 'triangle', env: { attack: 0.005, release: 0.16 }, level: 0.20, octave: -12, pattern: [0, 2], toneIndex: [0, 2], glideToNext: true },
    { kind: 'arp', osc: 'square', env: { attack: 0.005, release: 0.10 }, level: 0.07, octave: 12, subdivision: 2, pattern: [0, 1, 2, 1], gate: 0.5, density: 0.9 },
    { kind: 'perc', level: 0.5, steps: [
      { on: [0, 2], filter: 'lowpass', freq: 140, decay: 0.12, peak: 0.5 },
      { on: [1, 3], filter: 'highpass', freq: 1800, decay: 0.06, peak: 0.25 },
    ] },
    { kind: 'lead', osc: 'triangle', env: { attack: 0.05, release: 0.9 }, level: 0.11, octaves: [12, 24], chance: 0.55, scale: MAJOR_PENTA },
  ],
};

/** Quiet menu bed — E minor, pads + sub only. The unobtrusive default. */
const MENU_THEME: Track = {
  id: 'radio', name: 'The State Radio Orchestra', root: 40, mode: 'E minor',
  chords: MINOR_SCHEME, melody: PENTATONIC,
  bpm: 60, beatsPerBar: 4, chordBars: [3, 5], playMs: 200_000,
  layers: [
    { kind: 'pad', osc: 'sawtooth', detune: [-4, 4], env: { attack: 3.5, release: 4.5 }, level: 0.14, cutoffBase: 280, cutoffSpan: 450, dropThirdWhenCold: true },
    { kind: 'sub', osc: 'sine', env: { attack: 3, release: 4 }, level: 0.08, octave: -12 },
  ],
};

export const PLAYLIST: readonly Track[] = [ANTHEM, MARCH, WALTZ, INDUSTRIAL, NOCTURNE, MENU_THEME];

/** First track on a fresh install — the quiet menu bed. */
export const DEFAULT_TRACK_ID = MENU_THEME.id;

/** Resolve a persisted id; unknown/empty ids fall back to the first track. */
export function trackById(id: string): Track {
  return PLAYLIST.find(t => t.id === id) ?? PLAYLIST[0];
}
