// ============================================================
// Generative score engine: reads a Track descriptor (key, chord scheme,
// beat grid, layer stack — see tracks.ts) and voices it forever, scheduled
// with the standard lookahead pattern (all timing on the WebAudio clock;
// the JS interval only tops up the schedule). Reads a mood probe
// (season/weather) but never mutates anything — audio listens to the
// simulation, it never participates.
//
// Layers: pad/sub/lead reproduce the old ambient voice; bass/arp/perc add
// a rhythm section on the track's beat grid. Track switching crossfades via
// a per-track gain node UNDER the caller's dest (musicGain), so the old
// track's scheduled voices ring out while its gain ramps down.
// ============================================================
import { midiToHz, nextChord, phrase } from './music-theory';
import { PLAYLIST } from './tracks';
import type { ArpLayer, BassLayer, Envelope, LeadLayer, PadLayer, PercLayer, PercVoice, SubLayer, Track } from './tracks';

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
  private track: Track = PLAYLIST[0];
  private degree: string = PLAYLIST[0].chords.start;
  private trackGain: GainNode | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private tailTimers = new Set<ReturnType<typeof setTimeout>>();
  private noiseCache: AudioBuffer | null = null;
  private playing = false;
  private nextChordTime = 0;
  private intensity = 0.5;      // menu 0.35, game 0.55
  private mood: (() => EngineMood) | null = null;

  constructor(ctx: BaseAudioContext, dest: AudioNode) {
    this.ctx = ctx;
    this.dest = dest;
  }

  setIntensity(v: number) {
    this.intensity = v;
  }

  setMood(probe: (() => EngineMood) | null) {
    this.mood = probe;
  }

  /** The chord currently voiced — UI clicks pitch to these tones (offsets
   *  from currentRoot) so interface sounds stay consonant with the score. */
  currentChord(): readonly [number, number, number] {
    return this.track.chords.tones[this.degree];
  }

  /** The active track's key root in MIDI — threaded into clickHz so clicks
   *  land in the current song's key, not a fixed A. */
  currentRoot(): number {
    return this.track.root;
  }

  isPlaying(): boolean {
    return this.playing;
  }

  /** Crossfade to a track. The old track's gain ramps down while its already
   *  scheduled voices ring out; a tail timer disconnects the retired node. */
  playTrack(track: Track, opts?: { crossfadeS?: number }) {
    const xf = opts?.crossfadeS ?? 1.2;
    const now = this.ctx.currentTime;
    const old = this.trackGain;
    if (old) {
      // Fade the outgoing track to silence over `xf`, then drop the node. The
      // fade is short so switching songs doesn't stack them — once the gain
      // hits ~0 every already-scheduled voice on this node is silent, however
      // long its own envelope still had to run.
      old.gain.cancelScheduledValues(now);
      old.gain.setValueAtTime(old.gain.value, now);
      old.gain.linearRampToValueAtTime(0.0001, now + xf);
      const handle = setTimeout(() => {
        try { old.disconnect(); } catch { /* already disconnected */ }
        this.tailTimers.delete(handle);
      }, (xf + 0.5) * 1000);
      this.tailTimers.add(handle);
    }
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(old ? 0.0001 : 1, now);
    if (old) g.gain.linearRampToValueAtTime(1, now + xf);
    g.connect(this.dest);
    this.trackGain = g;
    this.track = track;
    this.degree = track.chords.start;
    this.nextChordTime = now + 0.1;
    this.playing = true;
    if (!this.timer) this.timer = setInterval(() => this.tick(), TICK_MS);
  }

  /** Silence the music without tearing down the engine (clicks stay in-key). */
  setPlaying(on: boolean) {
    this.playing = on;
    const g = this.trackGain;
    if (!g) return;
    const now = this.ctx.currentTime;
    g.gain.cancelScheduledValues(now);
    g.gain.setValueAtTime(g.gain.value, now);
    if (on) {
      g.gain.linearRampToValueAtTime(1, now + 0.4);
      this.nextChordTime = now + 0.05; // resume scheduling from now, no catch-up burst
    } else {
      g.gain.linearRampToValueAtTime(0.0001, now + 0.25);
    }
  }

  /** Stop scheduling and release nodes/timers (new-game / teardown). */
  dispose() {
    if (this.timer) { clearInterval(this.timer); this.timer = null; }
    for (const h of this.tailTimers) clearTimeout(h);
    this.tailTimers.clear();
    const g = this.trackGain;
    if (g) {
      const now = this.ctx.currentTime;
      g.gain.cancelScheduledValues(now);
      g.gain.setValueAtTime(g.gain.value, now);
      g.gain.linearRampToValueAtTime(0.0001, now + 0.3);
      setTimeout(() => { try { g.disconnect(); } catch { /* already disconnected */ } }, 500);
    }
    this.trackGain = null;
    this.playing = false;
  }

  private tick() {
    if (!this.playing) return;
    const dest = this.trackGain;
    if (!dest) return;
    const [lo, hi] = this.track.chordBars;
    while (this.nextChordTime < this.ctx.currentTime + LOOKAHEAD_S) {
      const bars = lo + Math.floor(Math.random() * (hi - lo + 1));
      this.nextChordTime += this.scheduleBlock(dest, this.nextChordTime, bars);
    }
  }

  /** Schedule one chord block across every layer; return its duration. */
  private scheduleBlock(dest: AudioNode, t0: number, bars: number): number {
    const { bpm, beatsPerBar, chords } = this.track;
    const beat = 60 / bpm;
    const dur = bars * beatsPerBar * beat;
    const degree = this.degree;
    const next = nextChord(degree, Math.random, chords.markov);
    const tones = chords.tones[degree];
    const nextTones = chords.tones[next];
    const mood = this.mood?.();
    const cold = mood ? mood.season === 'winter' || mood.tempC < 0 : false;
    const stormy = mood ? mood.condition === 'storm' || mood.condition === 'blizzard' : false;

    for (const layer of this.track.layers) {
      switch (layer.kind) {
        case 'pad':  this.schedulePad(dest, layer, t0, dur, tones, cold, stormy); break;
        case 'sub':  this.scheduleSub(dest, layer, t0, dur, tones, stormy); break;
        case 'bass': this.scheduleBass(dest, layer, t0, bars, beat, beatsPerBar, tones, nextTones); break;
        case 'arp':  this.scheduleArp(dest, layer, t0, bars, beat, beatsPerBar, tones); break;
        case 'perc': this.schedulePerc(dest, layer, t0, bars, beat, beatsPerBar, stormy); break;
        case 'lead': this.scheduleLead(dest, layer, t0, dur, degree, cold); break;
      }
    }
    this.degree = next;
    return dur;
  }

  // ---------------- layer voices ----------------

  private schedulePad(dest: AudioNode, l: PadLayer, t0: number, dur: number, tones: readonly number[], cold: boolean, stormy: boolean) {
    const root = this.track.root;
    const voiced = cold && l.dropThirdWhenCold ? [tones[0], tones[2]] : [tones[0], tones[1], tones[2]];
    const cutoff = (l.cutoffBase + l.cutoffSpan * this.intensity) * (cold ? 0.7 : 1);
    const level = l.level * this.intensity * (stormy ? 0.5 : 1);
    for (const [vi, offset] of voiced.entries()) {
      const freq = midiToHz(root + offset + (vi === 2 ? 12 : 0));
      const lp = this.ctx.createBiquadFilter();
      lp.type = 'lowpass';
      lp.frequency.setValueAtTime(cutoff * (1 + vi * 0.25), t0);
      const g = this.ctx.createGain();
      g.gain.setValueAtTime(0.0001, t0);
      g.gain.linearRampToValueAtTime(level / voiced.length, t0 + l.env.attack);
      g.gain.setValueAtTime(level / voiced.length, t0 + dur);
      g.gain.linearRampToValueAtTime(0.0001, t0 + dur + l.env.release);
      lp.connect(g);
      g.connect(dest);
      for (const detune of l.detune) {
        const osc = this.ctx.createOscillator();
        osc.type = l.osc;
        osc.frequency.value = freq;
        osc.detune.value = detune;
        osc.connect(lp);
        osc.start(t0);
        osc.stop(t0 + dur + l.env.release + 0.1);
      }
    }
  }

  private scheduleSub(dest: AudioNode, l: SubLayer, t0: number, dur: number, tones: readonly number[], stormy: boolean) {
    const level = l.level * this.intensity * (stormy ? 0.5 : 1);
    const osc = this.ctx.createOscillator();
    osc.type = l.osc;
    osc.frequency.value = midiToHz(this.track.root + tones[0] + l.octave);
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(0.0001, t0);
    g.gain.linearRampToValueAtTime(level, t0 + l.env.attack);
    g.gain.setValueAtTime(level, t0 + dur);
    g.gain.linearRampToValueAtTime(0.0001, t0 + dur + l.env.release);
    osc.connect(g);
    g.connect(dest);
    osc.start(t0);
    osc.stop(t0 + dur + l.env.release + 0.1);
  }

  private scheduleBass(dest: AudioNode, l: BassLayer, t0: number, bars: number, beat: number, beatsPerBar: number, tones: readonly number[], nextTones: readonly number[]) {
    const barDur = beat * beatsPerBar;
    const root = this.track.root;
    const level = l.level * this.intensity;
    const n = l.pattern.length;
    for (let bar = 0; bar < bars; bar++) {
      for (let i = 0; i < n; i++) {
        const t = t0 + bar * barDur + l.pattern[i] * beat;
        const toneIdx = (l.toneIndex?.[i] ?? 0) % tones.length;
        const freq = midiToHz(root + tones[toneIdx] + l.octave);
        const last = bar === bars - 1 && i === n - 1;
        const glideTo = last && l.glideToNext ? midiToHz(root + nextTones[0] + l.octave) : undefined;
        this.pluck(dest, l.osc, freq, t, l.env, level, glideTo);
      }
    }
  }

  private scheduleArp(dest: AudioNode, l: ArpLayer, t0: number, bars: number, beat: number, beatsPerBar: number, tones: readonly number[]) {
    const root = this.track.root;
    const level = l.level * this.intensity;
    const step = beat / l.subdivision;
    const steps = bars * beatsPerBar * l.subdivision;
    const prob = l.density * Math.min(1, this.intensity * 1.8);
    const noteLen = step * l.gate;
    for (let s = 0; s < steps; s++) {
      if (Math.random() >= prob) continue;
      const toneIdx = l.pattern[s % l.pattern.length] % tones.length;
      const freq = midiToHz(root + tones[toneIdx] + l.octave);
      this.pluck(dest, l.osc, freq, t0 + s * step, { attack: l.env.attack, release: noteLen }, level);
    }
  }

  private schedulePerc(dest: AudioNode, l: PercLayer, t0: number, bars: number, beat: number, beatsPerBar: number, stormy: boolean) {
    const barDur = beat * beatsPerBar;
    const level = l.level * this.intensity * (stormy ? 0.6 : 1);
    for (let bar = 0; bar < bars; bar++) {
      for (const voice of l.steps) {
        for (const b of voice.on) this.noiseHit(dest, voice, t0 + bar * barDur + b * beat, level);
      }
    }
  }

  private scheduleLead(dest: AudioNode, l: LeadLayer, t0: number, dur: number, degree: string, cold: boolean) {
    const chance = (cold ? l.chance * 0.5 : l.chance) * this.intensity * 2;
    if (Math.random() >= chance) return;
    const notes = phrase(degree, Math.random, this.track.chords.tones, l.scale);
    const octave = l.octaves[Math.floor(Math.random() * l.octaves.length)];
    let t = t0 + 2 + Math.random() * (dur / 2);
    for (const n of notes) {
      const osc = this.ctx.createOscillator();
      osc.type = l.osc;
      osc.frequency.value = midiToHz(this.track.root + octave + n);
      const g = this.ctx.createGain();
      g.gain.setValueAtTime(0.0001, t);
      g.gain.linearRampToValueAtTime(l.level * this.intensity, t + l.env.attack);
      g.gain.exponentialRampToValueAtTime(0.0001, t + l.env.release);
      osc.connect(g);
      g.connect(dest);
      osc.start(t);
      osc.stop(t + l.env.release + 0.1);
      t += 1.0 + Math.random() * 0.5;
    }
  }

  // ---------------- shared helpers ----------------

  /** A plucked note: linear attack, exponential decay, optional pitch glide. */
  private pluck(dest: AudioNode, type: OscillatorType, freq: number, t: number, env: Envelope, peak: number, glideTo?: number) {
    const osc = this.ctx.createOscillator();
    osc.type = type;
    osc.frequency.setValueAtTime(freq, t);
    if (glideTo !== undefined) osc.frequency.exponentialRampToValueAtTime(glideTo, t + env.attack + env.release);
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(0.0001, t);
    g.gain.linearRampToValueAtTime(peak, t + env.attack);
    g.gain.exponentialRampToValueAtTime(0.0001, t + env.attack + env.release);
    osc.connect(g);
    g.connect(dest);
    osc.start(t);
    osc.stop(t + env.attack + env.release + 0.05);
  }

  private noiseHit(dest: AudioNode, voice: PercVoice, t: number, level: number) {
    const src = this.ctx.createBufferSource();
    src.buffer = this.noiseBuffer();
    src.loop = true;
    const filter = this.ctx.createBiquadFilter();
    filter.type = voice.filter;
    filter.frequency.value = voice.freq;
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(0.0001, t);
    g.gain.linearRampToValueAtTime(voice.peak * level, t + 0.004);
    g.gain.exponentialRampToValueAtTime(0.0001, t + voice.decay);
    src.connect(filter);
    filter.connect(g);
    g.connect(dest);
    src.start(t, Math.random());
    src.stop(t + voice.decay + 0.05);
  }

  private noiseBuffer(): AudioBuffer {
    if (this.noiseCache) return this.noiseCache;
    const buf = this.ctx.createBuffer(1, this.ctx.sampleRate, this.ctx.sampleRate);
    const data = buf.getChannelData(0);
    for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
    this.noiseCache = buf;
    return buf;
  }
}
