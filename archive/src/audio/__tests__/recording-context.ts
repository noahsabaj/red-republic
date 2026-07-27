// A recording fake BaseAudioContext for determinism tests (node env, no
// WebAudio). It implements only what MusicEngine touches, and logs every
// oscillator / noise-source .start()/.stop() so a track's note stream can be
// captured and compared across plays. `currentTime` is a plain field the test
// advances by hand — the engine's pump(nowS) reads it.

export interface Ev { kind: 'osc' | 'noise'; type: string; freq: number; start: number; stop: number }

class FakeParam {
  value = 0;
  setValueAtTime(v: number) { this.value = v; return this; }
  linearRampToValueAtTime(v: number) { this.value = v; return this; }
  exponentialRampToValueAtTime(v: number) { this.value = v; return this; }
  setTargetAtTime(v: number) { this.value = v; return this; }
  cancelScheduledValues() { return this; }
}

class FakeGain {
  gain = new FakeParam();
  connect() { return this; }
  disconnect() { /* no-op */ }
}

class FakeFilter {
  type = 'lowpass';
  frequency = new FakeParam();
  connect() { return this; }
  disconnect() { /* no-op */ }
}

class FakeOsc {
  type = 'sine';
  frequency = new FakeParam();
  detune = new FakeParam();
  private ev: Ev | null = null;
  private log: Ev[];
  constructor(log: Ev[]) { this.log = log; }
  connect() { return this; }
  disconnect() { /* no-op */ }
  start(t: number) { this.ev = { kind: 'osc', type: this.type, freq: this.frequency.value, start: t, stop: t }; this.log.push(this.ev); }
  stop(t: number) { if (this.ev) this.ev.stop = t; }
}

class FakeBufferSource {
  buffer: unknown = null;
  loop = false;
  private ev: Ev | null = null;
  private log: Ev[];
  constructor(log: Ev[]) { this.log = log; }
  connect() { return this; }
  disconnect() { /* no-op */ }
  start(t: number, offset = 0) { this.ev = { kind: 'noise', type: 'noise', freq: offset, start: t, stop: t }; this.log.push(this.ev); }
  stop(t: number) { if (this.ev) this.ev.stop = t; }
}

class FakeBuffer {
  private len: number;
  constructor(len: number) { this.len = len; }
  getChannelData() { return new Float32Array(this.len); }
}

export class RecordingContext {
  currentTime = 0;
  sampleRate = 48000;
  log: Ev[] = [];
  createGain() { return new FakeGain(); }
  createBiquadFilter() { return new FakeFilter(); }
  createOscillator() { return new FakeOsc(this.log); }
  createBufferSource() { return new FakeBufferSource(this.log); }
  createBuffer(_ch: number, len: number) { return new FakeBuffer(len); }
}
