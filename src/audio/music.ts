// ============================================================
// Generative ambient score: slow minor-key pads with occasional
// pentatonic fragments, scheduled with the standard lookahead pattern
// (all timing on the WebAudio clock; the JS interval only tops up the
// schedule). Reads a mood probe (season/weather) but never mutates
// anything — audio listens to the simulation, it never participates.
// ============================================================
import { CHORD_TONES, ROOT_MIDI, midiToHz, nextChord, phrase } from './music-theory';
import type { Degree } from './music-theory';

export interface EngineMood {
  season: 'winter' | 'spring' | 'summer' | 'autumn';
  tempC: number;
  condition: string;
}

const LOOKAHEAD_S = 2;
const TICK_MS = 100;

export class MusicEngine {
  private ctx: BaseAudioContext;
  private dest: AudioNode;
  private timer: ReturnType<typeof setInterval> | null = null;
  private degree: Degree = 'i';
  private nextChordTime = 0;
  private intensity = 0.5;      // menu 0.35, game 0.55; bad events nudge briefly
  private mood: (() => EngineMood) | null = null;

  constructor(ctx: BaseAudioContext, dest: AudioNode) {
    this.ctx = ctx;
    this.dest = dest;
  }

  start() {
    if (this.timer) return;
    this.nextChordTime = this.ctx.currentTime + 0.2;
    this.timer = setInterval(() => this.tick(), TICK_MS);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  setIntensity(v: number) {
    this.intensity = v;
  }

  setMood(probe: (() => EngineMood) | null) {
    this.mood = probe;
  }

  /** The chord currently voiced by the pads — UI clicks pitch to these
   *  tones so interface sounds stay consonant with the score. */
  currentChord(): readonly [number, number, number] {
    return CHORD_TONES[this.degree];
  }

  private tick() {
    while (this.nextChordTime < this.ctx.currentTime + LOOKAHEAD_S) {
      const duration = 8 + Math.random() * 8;
      this.scheduleChord(this.nextChordTime, duration);
      this.nextChordTime += duration;
      this.degree = nextChord(this.degree, Math.random);
    }
  }

  private scheduleChord(t0: number, duration: number) {
    const mood = this.mood?.();
    const cold = mood ? mood.season === 'winter' || mood.tempC < 0 : false;
    const stormy = mood ? mood.condition === 'storm' || mood.condition === 'blizzard' : false;

    const tones = CHORD_TONES[this.degree];
    // winter voicing: drop the third — open fifths sound barer, colder
    const voiced = cold ? [tones[0], tones[2]] : [...tones];
    const cutoff = (300 + 600 * this.intensity) * (cold ? 0.7 : 1);
    const level = 0.16 * this.intensity * (stormy ? 0.5 : 1);
    const release = 4;

    // pad voices: two detuned saws per tone through a slow lowpass
    for (const [vi, offset] of voiced.entries()) {
      const freq = midiToHz(ROOT_MIDI + offset + (vi === 2 ? 12 : 0));
      const lp = this.ctx.createBiquadFilter();
      lp.type = 'lowpass';
      lp.frequency.setValueAtTime(cutoff * (1 + vi * 0.25), t0);
      const g = this.ctx.createGain();
      g.gain.setValueAtTime(0.0001, t0);
      g.gain.linearRampToValueAtTime(level / voiced.length, t0 + 3);
      g.gain.setValueAtTime(level / voiced.length, t0 + duration);
      g.gain.linearRampToValueAtTime(0.0001, t0 + duration + release);
      lp.connect(g);
      g.connect(this.dest);
      for (const detune of [-4, 4]) {
        const osc = this.ctx.createOscillator();
        osc.type = 'sawtooth';
        osc.frequency.value = freq;
        osc.detune.value = detune;
        osc.connect(lp);
        osc.start(t0);
        osc.stop(t0 + duration + release + 0.1);
      }
    }

    // sub root an octave down, sine
    const sub = this.ctx.createOscillator();
    sub.type = 'sine';
    sub.frequency.value = midiToHz(ROOT_MIDI + tones[0] - 12);
    const subG = this.ctx.createGain();
    subG.gain.setValueAtTime(0.0001, t0);
    subG.gain.linearRampToValueAtTime(level * 0.5, t0 + 3);
    subG.gain.setValueAtTime(level * 0.5, t0 + duration);
    subG.gain.linearRampToValueAtTime(0.0001, t0 + duration + release);
    sub.connect(subG);
    subG.connect(this.dest);
    sub.start(t0);
    sub.stop(t0 + duration + release + 0.1);

    // occasional melody fragment (rarer when cold, richer when clear)
    const melodyChance = (cold ? 0.2 : 0.4) * this.intensity * 2;
    if (Math.random() < melodyChance) {
      const notes = phrase(this.degree, Math.random);
      const octave = Math.random() < 0.5 ? 24 : 36;
      let t = t0 + 2 + Math.random() * (duration / 2);
      for (const n of notes) {
        const osc = this.ctx.createOscillator();
        osc.type = 'triangle';
        osc.frequency.value = midiToHz(ROOT_MIDI + octave + n);
        const g = this.ctx.createGain();
        g.gain.setValueAtTime(0.0001, t);
        g.gain.linearRampToValueAtTime(0.09 * this.intensity, t + 0.3);
        g.gain.exponentialRampToValueAtTime(0.0001, t + 1.4);
        osc.connect(g);
        g.connect(this.dest);
        osc.start(t);
        osc.stop(t + 1.5);
        t += 1.0 + Math.random() * 0.5;
      }
    }
  }
}
