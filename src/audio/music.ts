// ============================================================
// Score engine: plays a Track as a FIXED, deterministic song. The chord
// skeleton comes from arrange.ts (buildSongPlan — a seeded, cadence-resolved
// block list with an exact duration); this module voices that plan on the
// WebAudio clock with the standard lookahead pattern (the JS interval only
// tops up the schedule). Reads a mood probe (season/weather) but never
// mutates anything — audio listens to the simulation, it never participates.
//
// Determinism: NO Math.random anywhere. Structure is fixed by the plan;
// per-block voicing (arp gate, lead phrase, perc jitter) draws from an
// independent per-block stream mix(seed, blockIndex), so playback is
// bit-identical every play and seeking to any block needs no dry-run.
//
// Layers: pad/sub/lead reproduce the old ambient voice; bass/arp/perc add a
// rhythm section on the track's beat grid. Track switching crossfades via a
// per-track gain node UNDER the caller's dest (musicGain).
// ============================================================
import { midiToHz, phrase } from './music-theory';
import { PLAYLIST } from './tracks';
import type { ArpLayer, BassLayer, Envelope, LeadLayer, PadLayer, PercLayer, PercVoice, SubLayer, Track } from './tracks';
import { blockIndexAtTime, buildSongPlan, mix, seedOf } from './arrange';
import type { PlanBlock, SongPlan } from './arrange';
import { mulberry32 } from '../game/mapgen';
import type { SeededRng } from '../game/mapgen';

export interface EngineMood {
  season: 'winter' | 'spring' | 'summer' | 'autumn';
  tempC: number;
  condition: string;
}

const LOOKAHEAD_S = 2;
const TICK_MS = 100;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export class MusicEngine {
  private ctx: BaseAudioContext;
  private dest: AudioNode;
  private track: Track = PLAYLIST[0];
  private plan: SongPlan = buildSongPlan(PLAYLIST[0]);
  private degree: string = PLAYLIST[0].chords.start;
  private trackGain: GainNode | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private tailTimers = new Set<ReturnType<typeof setTimeout>>();
  private noiseCache: AudioBuffer | null = null;
  private playing = false;
  private blockIndex = 0;
  private songStartTime = 0;      // ctx.currentTime of song bar 0 (shifts across pause)
  private songEndTime = 0;        // songStartTime + plan.durationS
  private ended = false;
  private paused = false;
  private elapsedAtPause = 0;
  private snapFirst = false;    // the first block after a seek attacks fast, so a jump is audible
  private intensity = 0.5;      // menu 0.35, game 0.55
  private mood: (() => EngineMood) | null = null;

  /** Fired (once) when the song reaches its end — the transport advances/repeats/stops. */
  onEnded: (() => void) | null = null;

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

  /** Seconds into the current song (frozen while paused, clamped to the song). */
  elapsedS(): number {
    if (this.paused) return this.elapsedAtPause;
    return clamp(this.ctx.currentTime - this.songStartTime, 0, this.plan.durationS);
  }

  /** Exact length of the current song in seconds. */
  durationS(): number {
    return this.plan.durationS;
  }

  /**
   * Crossfade to a track and play it from `startElapsedS` (default 0). The old
   * track's gain ramps down while its already-scheduled voices ring out; a tail
   * timer disconnects the retired node. `fromBlock` (with a matching
   * `startElapsedS`) is how seeking and pause-resume re-enter mid-song — because
   * each block's voicing rng is indexed, no replay of earlier blocks is needed.
   */
  playTrack(track: Track, opts?: { crossfadeS?: number; fromBlock?: number; startElapsedS?: number }) {
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
    this.plan = buildSongPlan(track);
    this.blockIndex = clamp(opts?.fromBlock ?? 0, 0, this.plan.blocks.length - 1);
    this.degree = this.plan.blocks[this.blockIndex]?.degree ?? track.chords.start;
    this.snapFirst = this.blockIndex > 0; // entered mid-song (seek/resume) → snap the first chord in
    const startElapsed = opts?.startElapsedS ?? 0;
    this.songStartTime = now - startElapsed;
    this.songEndTime = this.songStartTime + this.plan.durationS;
    this.ended = false;
    this.paused = false;
    this.playing = true;
    if (!this.timer) this.timer = setInterval(() => this.pump(this.ctx.currentTime), TICK_MS);
  }

  /** Seek to `t` seconds, snapped to the NEAREST chord-block boundary (click-free;
   *  bar-accurate scrubbing arrives with the authored songs). */
  seek(t: number) {
    const plan = this.plan;
    const spb = plan.secondsPerBar;
    let k = blockIndexAtTime(plan, clamp(t, 0, plan.durationS));
    const next = plan.blocks[k + 1]; // round up to the next boundary if it's closer
    if (next && Math.abs(t - next.startBar * spb) < Math.abs(t - plan.blocks[k].startBar * spb)) k += 1;
    this.playTrack(this.track, { crossfadeS: 0.06, fromBlock: k, startElapsedS: plan.blocks[k].startBar * spb });
  }

  /** Play/pause without tearing down the engine (clicks stay in-key). Resume
   *  re-enters at the paused block so elapsed and audio stay in lockstep. */
  setPlaying(on: boolean) {
    if (on === this.playing) return;
    if (on) {
      const k = blockIndexAtTime(this.plan, this.elapsedAtPause);
      this.playTrack(this.track, {
        crossfadeS: 0.3,
        fromBlock: k,
        startElapsedS: this.plan.blocks[k].startBar * this.plan.secondsPerBar,
      });
      return;
    }
    this.elapsedAtPause = this.elapsedS();
    this.paused = true;
    this.playing = false;
    const g = this.trackGain;
    if (!g) return;
    const now = this.ctx.currentTime;
    g.gain.cancelScheduledValues(now);
    g.gain.setValueAtTime(g.gain.value, now);
    g.gain.linearRampToValueAtTime(0.0001, now + 0.25);
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

  /** Advance the schedule to audio-clock time `nowS`. The production interval
   *  calls this with ctx.currentTime; tests drive it against a fake clock. */
  pump(nowS: number) {
    const dest = this.trackGain;
    if (!this.playing || !dest) return;
    if (!this.ended && nowS >= this.songEndTime) { this.ended = true; this.onEnded?.(); return; }
    const spb = this.plan.secondsPerBar;
    while (this.blockIndex < this.plan.blocks.length) {
      const block = this.plan.blocks[this.blockIndex];
      const t0 = this.songStartTime + block.startBar * spb;
      if (t0 >= nowS + LOOKAHEAD_S) break;
      this.degree = block.degree;
      const snap = this.snapFirst; this.snapFirst = false; // only the first block after a seek
      this.scheduleBlock(dest, t0, block, this.blockIndex, snap);
      this.blockIndex++;
    }
  }

  /** Schedule one chord block across every layer. `snap` makes the slow pad/sub
   *  voices attack fast so a seek lands audibly instead of swelling in over seconds. */
  private scheduleBlock(dest: GainNode, t0: number, block: PlanBlock, blockIndex: number, snap = false) {
    const { bpm, beatsPerBar, chords } = this.track;
    const beat = 60 / bpm;
    const bars = block.bars;
    const dur = bars * beatsPerBar * beat;
    const degree = block.degree;
    const vrng = mix(seedOf(this.track), blockIndex);
    const nextDegree = this.plan.blocks[blockIndex + 1]?.degree ?? chords.start;
    const tones = chords.tones[degree];
    const nextTones = chords.tones[nextDegree];
    const mood = this.mood?.();
    const cold = mood ? mood.season === 'winter' || mood.tempC < 0 : false;
    const stormy = mood ? mood.condition === 'storm' || mood.condition === 'blizzard' : false;

    for (const layer of this.track.layers) {
      switch (layer.kind) {
        case 'pad':  this.schedulePad(dest, layer, t0, dur, tones, cold, stormy, snap); break;
        case 'sub':  this.scheduleSub(dest, layer, t0, dur, tones, stormy, snap); break;
        case 'bass': this.scheduleBass(dest, layer, t0, bars, beat, beatsPerBar, tones, nextTones); break;
        case 'arp':  this.scheduleArp(dest, layer, t0, bars, beat, beatsPerBar, tones, vrng); break;
        case 'perc': this.schedulePerc(dest, layer, t0, bars, beat, beatsPerBar, stormy, vrng); break;
        case 'lead': this.scheduleLead(dest, layer, t0, dur, degree, cold, vrng); break;
      }
    }

    // The last block resolves on the tonic, then fades to silence so the song
    // ends rather than being cut off. songEndTime === t0 + dur.
    if (blockIndex === this.plan.blocks.length - 1) {
      const fadeS = Math.min(3.5, dur);
      dest.gain.setValueAtTime(dest.gain.value, this.songEndTime - fadeS);
      dest.gain.linearRampToValueAtTime(0.0001, this.songEndTime);
    }
  }

  // ---------------- layer voices ----------------

  private schedulePad(dest: AudioNode, l: PadLayer, t0: number, dur: number, tones: readonly number[], cold: boolean, stormy: boolean, snap = false) {
    const root = this.track.root;
    const voiced = cold && l.dropThirdWhenCold ? [tones[0], tones[2]] : [tones[0], tones[1], tones[2]];
    const cutoff = (l.cutoffBase + l.cutoffSpan * this.intensity) * (cold ? 0.7 : 1);
    const level = l.level * this.intensity * (stormy ? 0.5 : 1);
    const atk = snap ? Math.min(l.env.attack, 0.12) : l.env.attack;
    for (const [vi, offset] of voiced.entries()) {
      const freq = midiToHz(root + offset + (vi === 2 ? 12 : 0));
      const lp = this.ctx.createBiquadFilter();
      lp.type = 'lowpass';
      lp.frequency.setValueAtTime(cutoff * (1 + vi * 0.25), t0);
      const g = this.ctx.createGain();
      g.gain.setValueAtTime(0.0001, t0);
      g.gain.linearRampToValueAtTime(level / voiced.length, t0 + atk);
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

  private scheduleSub(dest: AudioNode, l: SubLayer, t0: number, dur: number, tones: readonly number[], stormy: boolean, snap = false) {
    const level = l.level * this.intensity * (stormy ? 0.5 : 1);
    const atk = snap ? Math.min(l.env.attack, 0.12) : l.env.attack;
    const osc = this.ctx.createOscillator();
    osc.type = l.osc;
    osc.frequency.value = midiToHz(this.track.root + tones[0] + l.octave);
    const g = this.ctx.createGain();
    g.gain.setValueAtTime(0.0001, t0);
    g.gain.linearRampToValueAtTime(level, t0 + atk);
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

  private scheduleArp(dest: AudioNode, l: ArpLayer, t0: number, bars: number, beat: number, beatsPerBar: number, tones: readonly number[], vrng: SeededRng) {
    const root = this.track.root;
    const level = l.level * this.intensity;
    const step = beat / l.subdivision;
    const steps = bars * beatsPerBar * l.subdivision;
    const prob = l.density * Math.min(1, this.intensity * 1.8);
    const noteLen = step * l.gate;
    for (let s = 0; s < steps; s++) {
      if (vrng() >= prob) continue;
      const toneIdx = l.pattern[s % l.pattern.length] % tones.length;
      const freq = midiToHz(root + tones[toneIdx] + l.octave);
      this.pluck(dest, l.osc, freq, t0 + s * step, { attack: l.env.attack, release: noteLen }, level);
    }
  }

  private schedulePerc(dest: AudioNode, l: PercLayer, t0: number, bars: number, beat: number, beatsPerBar: number, stormy: boolean, vrng: SeededRng) {
    const barDur = beat * beatsPerBar;
    const level = l.level * this.intensity * (stormy ? 0.6 : 1);
    for (let bar = 0; bar < bars; bar++) {
      for (const voice of l.steps) {
        for (const b of voice.on) this.noiseHit(dest, voice, t0 + bar * barDur + b * beat, level, vrng());
      }
    }
  }

  private scheduleLead(dest: AudioNode, l: LeadLayer, t0: number, dur: number, degree: string, cold: boolean, vrng: SeededRng) {
    const chance = (cold ? l.chance * 0.5 : l.chance) * this.intensity * 2;
    if (vrng() >= chance) return;
    const notes = phrase(degree, vrng, this.track.chords.tones, l.scale);
    const octave = l.octaves[Math.floor(vrng() * l.octaves.length)];
    let t = t0 + 2 + vrng() * (dur / 2);
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
      t += 1.0 + vrng() * 0.5;
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

  private noiseHit(dest: AudioNode, voice: PercVoice, t: number, level: number, offset: number) {
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
    src.start(t, offset);
    src.stop(t + voice.decay + 0.05);
  }

  private noiseBuffer(): AudioBuffer {
    if (this.noiseCache) return this.noiseCache;
    const buf = this.ctx.createBuffer(1, this.ctx.sampleRate, this.ctx.sampleRate);
    const data = buf.getChannelData(0);
    const rng = mulberry32(0x51ed); // fixed so the noise bed is deterministic too
    for (let i = 0; i < data.length; i++) data[i] = rng() * 2 - 1;
    this.noiseCache = buf;
    return buf;
  }
}
