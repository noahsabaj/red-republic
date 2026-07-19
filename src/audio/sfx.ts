// ============================================================
// Procedural sound effects — every sound is synthesized on demand from
// oscillators and filtered noise, so the repo ships zero audio assets.
// Each recipe schedules against absolute context time and cleans itself
// up via osc.stop()/source.stop().
// ============================================================
import { clickHz } from './music-theory';
import type { UiFamily } from './ui-catalog';

// Outcome effects — a *result* rang out (build finished, trade cleared,
// quicksave). Interaction/press sounds live in UI_VOICES below.
export type SfxName =
  | 'buildPlace' | 'roadPaint' | 'bulldoze' | 'error'
  | 'coin' | 'complete' | 'objective'
  | 'contractOffer' | 'contractDone' | 'alertBad'
  | 'quicksave';

/** Live pitch + humanization threaded into every recipe. `chord` = the
 *  score's current chord tones (semitone offsets from the key root);
 *  `jitter` ∈ [0,1) nudges pitch so identical clicks never phase into a
 *  robotic tone. World recipes that don't pitch simply ignore it. */
export interface VoiceCtx { chord: readonly number[]; jitter: number }

/** ±`cents` detune from a jitter value. */
const jit = (hz: number, j: number, cents = 15): number => hz * 2 ** (((j * 2 - 1) * cents) / 1200);

// one cached second of white noise per AudioContext
const noiseBuffers = new WeakMap<BaseAudioContext, AudioBuffer>();
function noiseBuffer(ctx: BaseAudioContext): AudioBuffer {
  let buf = noiseBuffers.get(ctx);
  if (!buf) {
    buf = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    noiseBuffers.set(ctx, buf);
  }
  return buf;
}

interface Env { attack?: number; peak: number; duration: number }

/** Gain node with a linear attack and exponential release, auto-scheduled. */
function envGain(ctx: BaseAudioContext, dest: AudioNode, t0: number, env: Env): GainNode {
  const g = ctx.createGain();
  g.gain.setValueAtTime(0.0001, t0);
  g.gain.linearRampToValueAtTime(env.peak, t0 + (env.attack ?? 0.005));
  g.gain.exponentialRampToValueAtTime(0.0001, t0 + env.duration);
  g.connect(dest);
  return g;
}

function tone(
  ctx: BaseAudioContext, dest: AudioNode, t0: number,
  type: OscillatorType, freq: number, env: Env, glideTo?: number,
) {
  const osc = ctx.createOscillator();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, t0);
  if (glideTo !== undefined) osc.frequency.exponentialRampToValueAtTime(glideTo, t0 + env.duration);
  osc.connect(envGain(ctx, dest, t0, env));
  osc.start(t0);
  osc.stop(t0 + env.duration + 0.05);
}

function noise(
  ctx: BaseAudioContext, dest: AudioNode, t0: number,
  filterType: BiquadFilterType, freq: number, env: Env, freqTo?: number,
) {
  const src = ctx.createBufferSource();
  src.buffer = noiseBuffer(ctx);
  src.loop = true;
  const filter = ctx.createBiquadFilter();
  filter.type = filterType;
  filter.frequency.setValueAtTime(freq, t0);
  if (freqTo !== undefined) filter.frequency.exponentialRampToValueAtTime(freqTo, t0 + env.duration);
  src.connect(filter);
  filter.connect(envGain(ctx, dest, t0, env));
  src.start(t0, Math.random());
  src.stop(t0 + env.duration + 0.05);
}

export const SFX_DEFS: Record<SfxName, (ctx: BaseAudioContext, dest: AudioNode, t0: number, v: VoiceCtx) => void> = {
  buildPlace: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'sine', 90, { peak: 0.5, duration: 0.18 }, 55);
    noise(ctx, dest, t0, 'lowpass', 900, { peak: 0.3, duration: 0.06 });
  },
  roadPaint: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'bandpass', 1200, { peak: 0.12, duration: 0.025 });
  },
  bulldoze: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'lowpass', 900, { peak: 0.35, duration: 0.15 }, 200);
    tone(ctx, dest, t0, 'sawtooth', 70, { peak: 0.2, duration: 0.15 }, 45);
  },
  error: (ctx, dest, t0) => {
    // two close squares beat dissonantly — unmistakably "no"
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 1500;
    lp.connect(dest);
    tone(ctx, lp, t0, 'square', 220, { peak: 0.12, duration: 0.12 });
    tone(ctx, lp, t0, 'square', 233, { peak: 0.12, duration: 0.12 });
  },
  coin: (ctx, dest, t0, v) => {
    // a bright in-key double-ring — "trade cleared" — now consonant
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 2, 2), v.jitter), { peak: 0.18, duration: 0.09 });
    tone(ctx, dest, t0 + 0.06, 'triangle', jit(clickHz(v.chord, 0, 3), v.jitter), { peak: 0.18, duration: 0.12 });
  },
  complete: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'triangle', 659, { peak: 0.22, duration: 0.16 });        // E5
    tone(ctx, dest, t0 + 0.12, 'triangle', 880, { peak: 0.22, duration: 0.22 }); // A5
  },
  objective: (ctx, dest, t0) => {
    // small minor fanfare: A3-C4-E4 arpeggio into a held chord, brass-ish saws
    const lp = ctx.createBiquadFilter();
    lp.type = 'lowpass';
    lp.frequency.value = 1800;
    lp.connect(dest);
    const notes = [220, 261.6, 329.6];
    notes.forEach((f, i) => tone(ctx, lp, t0 + i * 0.11, 'sawtooth', f, { attack: 0.02, peak: 0.14, duration: 0.18 }));
    notes.forEach(f => tone(ctx, lp, t0 + 0.34, 'sawtooth', f, { attack: 0.02, peak: 0.11, duration: 0.42 }));
  },
  contractOffer: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'square', 880, { peak: 0.08, duration: 0.04 });
    tone(ctx, dest, t0 + 0.09, 'square', 880, { peak: 0.08, duration: 0.04 });
  },
  contractDone: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'sawtooth', 329.6, { attack: 0.02, peak: 0.1, duration: 0.2 });  // E4
    tone(ctx, dest, t0 + 0.15, 'sawtooth', 440, { attack: 0.02, peak: 0.12, duration: 0.3 }); // A4
  },
  alertBad: (ctx, dest, t0) => {
    // low minor-second rumble, slow decay
    tone(ctx, dest, t0, 'sine', 110, { attack: 0.01, peak: 0.3, duration: 0.5 });
    tone(ctx, dest, t0, 'sine', 116.5, { attack: 0.01, peak: 0.3, duration: 0.5 });
  },
  quicksave: (ctx, dest, t0, v) => {
    // a quick mechanical chirp, third→root in-key
    noise(ctx, dest, t0, 'highpass', 2200, { peak: 0.12, duration: 0.02 });
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 1, 2), v.jitter), { peak: 0.12, duration: 0.07 }, jit(clickHz(v.chord, 0, 2), v.jitter));
  },
};

/** Which bus each fixed effect rides. quicksave is an interface chirp;
 *  everything else is world/economy (Effects bus). */
export const SFX_BUS: Record<SfxName, 'interface' | 'effects'> = {
  buildPlace: 'effects', roadPaint: 'effects', bulldoze: 'effects', error: 'effects',
  coin: 'effects', complete: 'effects', objective: 'effects',
  contractOffer: 'effects', contractDone: 'effects', alertBad: 'effects',
  quicksave: 'interface',
};

// ============================================================
// Interaction voices — mechanical clicks pitched to the LIVE chord via
// clickHz, so the interface is always consonant with the score. Only the
// tonal body tracks the chord; noise ticks stay unpitched, so clicks read
// as machine-tactile, not melodic. Menu families ride the Interface bus;
// world cues (select / tool*) ride Effects — see FAMILY_BUS in ui-catalog.
// ============================================================
export const UI_VOICES: Record<UiFamily, (ctx: BaseAudioContext, dest: AudioNode, t0: number, v: VoiceCtx) => void> = {
  neutral: (ctx, dest, t0, v) => {
    noise(ctx, dest, t0, 'bandpass', 2600, { peak: 0.14, duration: 0.026 });
    tone(ctx, dest, t0, 'sine', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.09, duration: 0.045 });
  },
  confirm: (ctx, dest, t0, v) => {
    // root → fifth, rising — an affirmative stamp
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.13, duration: 0.05 });
    tone(ctx, dest, t0 + 0.045, 'triangle', jit(clickHz(v.chord, 2, 1), v.jitter), { peak: 0.13, duration: 0.08 });
  },
  back: (ctx, dest, t0, v) => {
    // fifth → root, falling — de-latch / dismiss
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 2, 1), v.jitter), { peak: 0.11, duration: 0.05 });
    tone(ctx, dest, t0 + 0.04, 'triangle', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.11, duration: 0.07 });
  },
  open: (ctx, dest, t0, v) => {
    noise(ctx, dest, t0, 'bandpass', 500, { peak: 0.12, duration: 0.09 }, 1300);
    tone(ctx, dest, t0, 'sine', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.06, duration: 0.06 });
  },
  toggleOn: (ctx, dest, t0, v) => {
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.12, duration: 0.04 });
    tone(ctx, dest, t0 + 0.035, 'triangle', jit(clickHz(v.chord, 0, 2), v.jitter), { peak: 0.12, duration: 0.06 });
  },
  toggleOff: (ctx, dest, t0, v) => {
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 0, 2), v.jitter), { peak: 0.11, duration: 0.04 });
    tone(ctx, dest, t0 + 0.035, 'triangle', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.11, duration: 0.06 });
  },
  tab: (ctx, dest, t0, v) => {
    tone(ctx, dest, t0, 'triangle', jit(clickHz(v.chord, 1, 2), v.jitter), { peak: 0.11, duration: 0.03 });
    noise(ctx, dest, t0, 'bandpass', 3000, { peak: 0.05, duration: 0.014 });
  },
  arm: (ctx, dest, t0, v) => {
    // low, tense — "safety off"
    const lp = ctx.createBiquadFilter(); lp.type = 'lowpass'; lp.frequency.value = 700; lp.connect(dest);
    tone(ctx, lp, t0, 'sawtooth', jit(clickHz(v.chord, 0, 0), v.jitter), { peak: 0.14, duration: 0.14 });
  },
  commit: (ctx, dest, t0, v) => {
    // the strike — firm low root + a stamp
    const lp = ctx.createBiquadFilter(); lp.type = 'lowpass'; lp.frequency.value = 800; lp.connect(dest);
    tone(ctx, lp, t0, 'sawtooth', jit(clickHz(v.chord, 0, 0), v.jitter), { peak: 0.2, duration: 0.12 });
    noise(ctx, dest, t0, 'lowpass', 1000, { peak: 0.16, duration: 0.05 });
  },
  speed: (ctx, dest, t0, v) => {
    tone(ctx, dest, t0, 'sine', jit(clickHz(v.chord, 0, 2), v.jitter), { peak: 0.11, duration: 0.03 });
  },
  // ---- world-flavored cues (Effects bus): earthy, kin to buildPlace ----
  select: (ctx, dest, t0, v) => {
    const f = jit(clickHz(v.chord, 0, 0), v.jitter);
    tone(ctx, dest, t0, 'sine', f, { peak: 0.2, duration: 0.09 }, f * 0.7);
    noise(ctx, dest, t0, 'lowpass', 800, { peak: 0.1, duration: 0.04 });
  },
  toolArm: (ctx, dest, t0, v) => {
    // earthy rising thunk — tool picked up
    tone(ctx, dest, t0, 'sine', jit(clickHz(v.chord, 0, 0), v.jitter), { peak: 0.26, duration: 0.11 }, jit(clickHz(v.chord, 0, 1), v.jitter));
    noise(ctx, dest, t0, 'lowpass', 700, { peak: 0.13, duration: 0.05 });
  },
  toolCancel: (ctx, dest, t0, v) => {
    // earthy falling thunk — tool put down
    tone(ctx, dest, t0, 'sine', jit(clickHz(v.chord, 0, 1), v.jitter), { peak: 0.22, duration: 0.11 }, jit(clickHz(v.chord, 0, 0), v.jitter));
    noise(ctx, dest, t0, 'lowpass', 600, { peak: 0.11, duration: 0.05 });
  },
  hover: (ctx, dest, t0) => {
    // whisper — well below click level
    noise(ctx, dest, t0, 'bandpass', 3200, { peak: 0.03, duration: 0.012 });
  },
};
