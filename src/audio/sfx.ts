// ============================================================
// Procedural sound effects — every sound is synthesized on demand from
// oscillators and filtered noise, so the repo ships zero audio assets.
// Each recipe schedules against absolute context time and cleans itself
// up via osc.stop()/source.stop().
// ============================================================

export type SfxName =
  | 'click' | 'panelOpen' | 'panelClose'
  | 'buildPlace' | 'roadPaint' | 'bulldoze' | 'error'
  | 'coin' | 'complete' | 'objective'
  | 'contractOffer' | 'contractDone' | 'alertBad'
  | 'speedChange' | 'quicksave';

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

export const SFX_DEFS: Record<SfxName, (ctx: BaseAudioContext, dest: AudioNode, t0: number) => void> = {
  click: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'bandpass', 2500, { peak: 0.25, duration: 0.03 });
  },
  panelOpen: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'bandpass', 400, { peak: 0.16, duration: 0.09 }, 1200);
  },
  panelClose: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'bandpass', 1200, { peak: 0.14, duration: 0.07 }, 400);
  },
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
  coin: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'sine', 1320, { peak: 0.2, duration: 0.09 });
    tone(ctx, dest, t0 + 0.06, 'sine', 1760, { peak: 0.2, duration: 0.12 });
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
  speedChange: (ctx, dest, t0) => {
    tone(ctx, dest, t0, 'sine', 700, { peak: 0.12, duration: 0.03 });
  },
  quicksave: (ctx, dest, t0) => {
    noise(ctx, dest, t0, 'highpass', 2000, { peak: 0.15, duration: 0.02 });
    tone(ctx, dest, t0, 'sine', 1200, { peak: 0.15, duration: 0.08 }, 500);
  },
};
