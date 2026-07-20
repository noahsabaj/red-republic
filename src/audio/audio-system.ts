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
//
// It also owns the music player: the playlist cursor, shuffle/repeat, and
// the auto-advance timer. User preferences persist in the settings store;
// the live "now playing" is a small observable (subscribeMusic/musicState)
// the UI wraps in useSyncExternalStore. The settings subscription stays
// volume-only, so a track change never re-enters transport.
// ============================================================
import { getSettings, subscribeSettings, updateSettings } from '@/app/settings';
import { SFX_DEFS, SFX_BUS, UI_VOICES } from './sfx';
import type { SfxName, VoiceCtx } from './sfx';
import { FAMILY_BUS } from './ui-catalog';
import type { UiFamily } from './ui-catalog';
import { CHORD_TONES, ROOT_MIDI } from './music-theory';
import { soundForEvent } from './event-sounds';
import { MusicEngine } from './music';
import type { EngineMood } from './music';
import { DEFAULT_TRACK_ID, PLAYLIST, trackById } from './tracks';
import type { Track } from './tracks';
import { autoAdvance, naturalOrder, orderPos, shuffledOrder, stepPos } from './playlist';
import type { RepeatMode } from './playlist';

const SFX_MIN_INTERVAL_MS = 50; // same-sound rate limit (event bursts at 4x speed)
// Crossfades are short so switching songs doesn't audibly stack them: a manual
// pick/skip cuts over near-instantly; only a natural track end blends gently.
const MANUAL_CROSSFADE_S = 0.35;
const AUTO_CROSSFADE_S = 1.2;

/** Live "now playing" snapshot for the player UI. */
export interface MusicState {
  trackId: string;
  trackName: string;
  index: number;   // 0-based position in PLAYLIST
  total: number;
  playing: boolean;
  shuffle: boolean;
  repeat: RepeatMode;
}

/** Pre-unlock snapshot from persisted prefs, so the panel reads correctly
 *  even before the first gesture starts the audio. */
function initialMusicState(): MusicState {
  const s = getSettings();
  const track = trackById(s.musicTrackId || DEFAULT_TRACK_ID);
  return Object.freeze({
    trackId: track.id, trackName: track.name,
    index: PLAYLIST.indexOf(track), total: PLAYLIST.length,
    playing: false, shuffle: s.musicShuffle, repeat: s.musicRepeat,
  });
}

export class AudioSystem {
  private ctx: AudioContext | null = null;
  private masterGain: GainNode | null = null;
  private musicGain: GainNode | null = null;
  private sfxGain: GainNode | null = null;
  private interfaceGain: GainNode | null = null;
  private music: MusicEngine | null = null;
  private lastPlayed = new Map<SfxName, number>();
  private lastUi = new Map<UiFamily, number>();
  private scene: 'menu' | 'game' = 'menu';
  private probe: (() => EngineMood) | null = null;
  private autoSuspended = false;

  // ---- music player state ----
  private order: number[] = [];       // permutation of playlist indices
  private pos = 0;                     // cursor within `order`
  private shuffle = false;
  private repeat: RepeatMode = 'all';
  private musicSnap: MusicState = initialMusicState();
  private musicListeners = new Set<() => void>();
  private advanceTimer: ReturnType<typeof setTimeout> | null = null;
  private advanceEpoch = 0;           // performance.now() when the timer armed
  private advanceWindowMs = 0;        // duration the current timer was armed for
  private advanceRemainingMs: number | null = null; // leftover window across a pause

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
      this.interfaceGain = this.ctx.createGain();
      this.interfaceGain.connect(this.masterGain);
      this.applyVolumes(true);
      subscribeSettings(() => this.applyVolumes());
      document.addEventListener('visibilitychange', () => this.onVisibility());
      this.startMusic();
    }
    if (this.ctx.state === 'suspended' && !document.hidden) {
      void this.ctx.resume();
      this.autoSuspended = false;
    }
  }

  /** Build the MusicEngine and begin the soundtrack from persisted prefs. */
  private startMusic() {
    if (!this.ctx || !this.musicGain) return;
    const s = getSettings();
    this.shuffle = s.musicShuffle;
    this.repeat = s.musicRepeat;
    this.order = this.shuffle ? shuffledOrder(PLAYLIST.length, Math.random) : naturalOrder(PLAYLIST.length);
    const startId = trackById(s.musicTrackId || DEFAULT_TRACK_ID).id;
    const startIdx = Math.max(0, PLAYLIST.findIndex(t => t.id === startId));
    this.pos = orderPos(this.order, startIdx);
    this.music = new MusicEngine(this.ctx, this.musicGain);
    this.music.setIntensity(this.scene === 'menu' ? 0.35 : 0.55);
    this.music.setMood(this.scene === 'game' ? this.probe : null);
    this.music.playTrack(this.currentTrack(), { crossfadeS: 0 });
    this.armAdvance();
    this.notifyMusic(this.snapshot());
  }

  /** Play one outcome effect. No-op before the first unlock or while muted. */
  sfx(name: SfxName) {
    this.unlock();
    if (!this.ctx || !this.sfxGain || !this.interfaceGain) return;
    if (getSettings().muted) return;
    const now = performance.now();
    const last = this.lastPlayed.get(name) ?? -Infinity;
    if (now - last < SFX_MIN_INTERVAL_MS) return;
    this.lastPlayed.set(name, now);
    const dest = SFX_BUS[name] === 'interface' ? this.interfaceGain : this.sfxGain;
    SFX_DEFS[name](this.ctx, dest, this.ctx.currentTime, this.voiceCtx());
  }

  /**
   * Play a press/interaction voice — the single owner of press-driven UI
   * sound. Menu families route to the Interface bus, world cues (select /
   * tool*) to Effects. Per-family rate-limit mirrors sfx().
   */
  ui(family: UiFamily) {
    this.unlock();
    if (!this.ctx || !this.sfxGain || !this.interfaceGain) return;
    if (getSettings().muted) return;
    const now = performance.now();
    const last = this.lastUi.get(family) ?? -Infinity;
    if (now - last < SFX_MIN_INTERVAL_MS) return;
    this.lastUi.set(family, now);
    const dest = FAMILY_BUS[family] === 'effects' ? this.sfxGain : this.interfaceGain;
    UI_VOICES[family](this.ctx, dest, this.ctx.currentTime, this.voiceCtx());
  }

  /** A whisper hover tick — gated by the hoverSounds setting; its stricter
   *  throttle (rate + same-element + touch skip) is owned by ui-sounds.ts. */
  uiHover() {
    this.unlock();
    if (!this.ctx || !this.interfaceGain) return;
    if (getSettings().muted || !getSettings().hoverSounds) return;
    UI_VOICES.hover(this.ctx, this.interfaceGain, this.ctx.currentTime, this.voiceCtx());
  }

  /** Live pitch + humanization for a play: the score's current chord and key
   *  root (so clicks stay consonant) plus a fresh jitter. */
  private voiceCtx(): VoiceCtx {
    return {
      chord: this.music?.currentChord() ?? CHORD_TONES.i,
      jitter: Math.random(),
      root: this.music?.currentRoot() ?? ROOT_MIDI,
    };
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

  // ---------------- music player transport ----------------

  /** Live now-playing snapshot for React (stable identity between changes). */
  musicState = (): MusicState => this.musicSnap;

  /** Subscribe to now-playing changes; arrow field keeps a stable ref for the hook. */
  subscribeMusic = (cb: () => void): (() => void) => {
    this.musicListeners.add(cb);
    return () => this.musicListeners.delete(cb);
  };

  /** Jump to a specific track by id (the playlist "pick a song" control). */
  selectTrack(id: string) {
    const idx = PLAYLIST.findIndex(t => t.id === id);
    if (idx < 0) return;
    this.pos = orderPos(this.order, idx);
    this.startCurrent(MANUAL_CROSSFADE_S);
    updateSettings({ musicTrackId: id });
  }

  nextTrack() {
    this.pos = stepPos(this.pos, 1, this.order.length);
    this.startCurrent(MANUAL_CROSSFADE_S);
    updateSettings({ musicTrackId: this.currentTrack().id });
  }

  prevTrack() {
    this.pos = stepPos(this.pos, -1, this.order.length);
    this.startCurrent(MANUAL_CROSSFADE_S);
    updateSettings({ musicTrackId: this.currentTrack().id });
  }

  /** Toggle shuffle without changing the current track (it stays under the cursor). */
  setShuffle(on: boolean) {
    if (on === this.shuffle) return;
    this.shuffle = on;
    const currentIdx = this.order[this.pos] ?? 0;
    this.order = on ? shuffledOrder(PLAYLIST.length, Math.random) : naturalOrder(PLAYLIST.length);
    this.pos = orderPos(this.order, currentIdx);
    updateSettings({ musicShuffle: on });
    this.armAdvance();
    this.notifyMusic(this.snapshot());
  }

  setRepeat(mode: RepeatMode) {
    this.repeat = mode;
    updateSettings({ musicRepeat: mode });
    this.notifyMusic(this.snapshot());
  }

  /** Play/pause the music without touching master mute or the sim. */
  setMusicPlaying(on: boolean) {
    this.music?.setPlaying(on);
    if (on) this.armAdvance(); else this.pauseAdvance();
    this.notifyMusic(this.snapshot());
  }

  private currentTrack(): Track {
    return PLAYLIST[this.order[this.pos]] ?? PLAYLIST[0];
  }

  private startCurrent(crossfadeS: number) {
    if (!this.music) return;
    this.advanceRemainingMs = null;
    this.music.playTrack(this.currentTrack(), { crossfadeS });
    this.armAdvance();
    this.notifyMusic(this.snapshot());
  }

  private armAdvance() {
    this.clearAdvance();
    if (!this.music?.isPlaying()) return;
    const ms = this.advanceRemainingMs ?? this.currentTrack().playMs;
    this.advanceRemainingMs = null;
    this.advanceWindowMs = ms;
    this.advanceEpoch = performance.now();
    this.advanceTimer = setTimeout(() => this.onAdvance(), ms);
  }

  private pauseAdvance() {
    if (this.advanceTimer === null) return;
    const elapsed = performance.now() - this.advanceEpoch;
    this.advanceRemainingMs = Math.max(0, this.advanceWindowMs - elapsed);
    this.clearAdvance();
  }

  private clearAdvance() {
    if (this.advanceTimer !== null) { clearTimeout(this.advanceTimer); this.advanceTimer = null; }
  }

  private onAdvance() {
    this.advanceTimer = null;
    const r = autoAdvance(this.pos, this.order.length, this.repeat);
    if (r.stop) { this.setMusicPlaying(false); return; }
    if (r.sameTrack) { this.advanceRemainingMs = null; this.armAdvance(); return; }
    if (r.reshuffle && this.shuffle) this.order = shuffledOrder(PLAYLIST.length, Math.random);
    this.pos = r.pos;
    this.advanceRemainingMs = null;
    this.music?.playTrack(this.currentTrack(), { crossfadeS: AUTO_CROSSFADE_S });
    this.armAdvance();
    updateSettings({ musicTrackId: this.currentTrack().id });
    this.notifyMusic(this.snapshot());
  }

  private snapshot(): MusicState {
    const track = this.currentTrack();
    return {
      trackId: track.id, trackName: track.name,
      index: PLAYLIST.indexOf(track), total: PLAYLIST.length,
      playing: this.music?.isPlaying() ?? false,
      shuffle: this.shuffle, repeat: this.repeat,
    };
  }

  private notifyMusic(next: MusicState) {
    this.musicSnap = Object.freeze(next);
    this.musicListeners.forEach(f => f());
  }

  private applyVolumes(immediate = false) {
    if (!this.ctx || !this.masterGain || !this.musicGain || !this.sfxGain || !this.interfaceGain) return;
    const s = getSettings();
    const t = this.ctx.currentTime;
    const set = (g: GainNode, v: number) => {
      if (immediate) g.gain.setValueAtTime(v, t);
      else g.gain.setTargetAtTime(v, t, 0.03); // no zipper noise
    };
    set(this.masterGain, s.muted ? 0 : 1);
    set(this.musicGain, s.musicVolume ** 2); // perceptual curve
    set(this.sfxGain, s.sfxVolume ** 2);
    set(this.interfaceGain, s.interfaceVolume ** 2);
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
