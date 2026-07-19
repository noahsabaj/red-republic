// ============================================================
// The audio system: lazy AudioContext (created on the first user
// gesture — autoplay policy), a gain graph
//
//   sfx voices ─► sfxGain ──┐
//                           ├─► masterGain ─► compressor ─► speakers
//   music pads ─► musicGain ┘
//
// with settings-driven volumes, tab-visibility suspension, and a
// menu/game scene switch. The constructor is side-effect-free so the
// module is safe to import anywhere (including tests without WebAudio).
// ============================================================
import { getSettings, subscribeSettings } from '@/app/settings';
import { SFX_DEFS } from './sfx';
import type { SfxName } from './sfx';
import { soundForEvent } from './event-sounds';
import { MusicEngine } from './music';
import type { EngineMood } from './music';

const SFX_MIN_INTERVAL_MS = 50; // same-sound rate limit (event bursts at 4x speed)

export class AudioSystem {
  private ctx: AudioContext | null = null;
  private masterGain: GainNode | null = null;
  private musicGain: GainNode | null = null;
  private sfxGain: GainNode | null = null;
  private music: MusicEngine | null = null;
  private lastPlayed = new Map<SfxName, number>();
  private scene: 'menu' | 'game' = 'menu';
  private probe: (() => EngineMood) | null = null;
  private autoSuspended = false;

  /**
   * Create (or resume) the AudioContext. Must be reachable from a user
   * gesture; safe to call redundantly — every click goes through sfx(),
   * which calls this first.
   */
  unlock() {
    if (!('AudioContext' in globalThis)) return;
    if (!this.ctx) {
      this.ctx = new AudioContext();
      const compressor = this.ctx.createDynamicsCompressor();
      compressor.threshold.value = -18;
      compressor.ratio.value = 4;
      compressor.connect(this.ctx.destination);
      this.masterGain = this.ctx.createGain();
      this.masterGain.connect(compressor);
      this.musicGain = this.ctx.createGain();
      this.musicGain.connect(this.masterGain);
      this.sfxGain = this.ctx.createGain();
      this.sfxGain.connect(this.masterGain);
      this.applyVolumes(true);
      subscribeSettings(() => this.applyVolumes());
      document.addEventListener('visibilitychange', () => this.onVisibility());
      this.music = new MusicEngine(this.ctx, this.musicGain);
      this.music.setIntensity(this.scene === 'menu' ? 0.35 : 0.55);
      this.music.setMood(this.scene === 'game' ? this.probe : null);
      this.music.start();
    }
    if (this.ctx.state === 'suspended' && !document.hidden) {
      void this.ctx.resume();
      this.autoSuspended = false;
    }
  }

  /** Play one effect. No-op before the first unlock or while muted. */
  sfx(name: SfxName) {
    this.unlock();
    if (!this.ctx || !this.sfxGain) return;
    if (getSettings().muted) return;
    const now = performance.now();
    const last = this.lastPlayed.get(name) ?? -Infinity;
    if (now - last < SFX_MIN_INTERVAL_MS) return;
    this.lastPlayed.set(name, now);
    SFX_DEFS[name](this.ctx, this.sfxGain, this.ctx.currentTime);
  }

  /** Fan-out hook for the App event drain — maps engine events to sounds. */
  onGameEvent(e: { kind: string; icon?: string }) {
    const name = soundForEvent(e.kind, e.icon);
    if (name) this.sfx(name);
  }

  /** Menu music sits lower and ignores the weather; the game reacts to it. */
  setScene(scene: 'menu' | 'game') {
    this.scene = scene;
    this.music?.setIntensity(scene === 'menu' ? 0.35 : 0.55);
    this.music?.setMood(scene === 'game' ? this.probe : null);
  }

  /** Read-only engine peek (season/temperature/condition) for the score. */
  setEngineProbe(probe: (() => EngineMood) | null) {
    this.probe = probe;
    if (this.scene === 'game') this.music?.setMood(probe);
  }

  private applyVolumes(immediate = false) {
    if (!this.ctx || !this.masterGain || !this.musicGain || !this.sfxGain) return;
    const s = getSettings();
    const t = this.ctx.currentTime;
    const set = (g: GainNode, v: number) => {
      if (immediate) g.gain.setValueAtTime(v, t);
      else g.gain.setTargetAtTime(v, t, 0.03); // no zipper noise
    };
    set(this.masterGain, s.muted ? 0 : 1);
    set(this.musicGain, s.musicVolume ** 2); // perceptual curve
    set(this.sfxGain, s.sfxVolume ** 2);
  }

  private onVisibility() {
    if (!this.ctx) return;
    if (document.hidden) {
      if (getSettings().muteWhenHidden && this.ctx.state === 'running') {
        this.autoSuspended = true;
        void this.ctx.suspend();
      }
    } else if (this.autoSuspended) {
      // only undo our own suspension — never fight an external pause
      this.autoSuspended = false;
      void this.ctx.resume();
    }
  }
}

/** The one app-wide instance. Import-safe: nothing happens until unlock(). */
export const audio = new AudioSystem();
