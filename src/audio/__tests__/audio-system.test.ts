import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AudioSystem } from '../audio-system';
import { PLAYLIST } from '../tracks';
import { getSettings, reloadSettingsFromStorage, updateSettings } from '@/app/settings';
import { FakeAudioContext, runSongToEnd } from './fake-audio-context';

/**
 * The transport state machine — playlist cursor, shuffle, repeat,
 * auto-advance, play/pause and the persisted preferences.
 *
 * playlist.ts (the DECISION layer) was already well covered; this is the layer
 * that APPLIES those decisions, and it is where both of the audit's real bugs
 * lived. It could not be tested before because unlock() reached
 * `new AudioContext()` directly; it now takes a factory.
 */
function fakeStorage(seed: Record<string, string> = {}): Storage {
  const map = new Map(Object.entries(seed));
  return {
    get length() { return map.size; },
    key: (i: number) => [...map.keys()][i] ?? null,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => { map.set(k, v); },
    removeItem: (k: string) => { map.delete(k); },
    clear: () => map.clear(),
  };
}

/** An unlocked system on a fake clock, started on a known track with shuffle
 *  off so the order is the playlist order. */
function systemOn(trackId: string, repeat: 'off' | 'all' | 'one' = 'all') {
  updateSettings({ musicTrackId: trackId, musicShuffle: false, musicRepeat: repeat });
  const ctx = new FakeAudioContext();
  const audio = new AudioSystem(() => ctx as unknown as AudioContext);
  audio.unlock();
  return { ctx, audio };
}

const idAt = (i: number) => PLAYLIST[i].id;

describe('AudioSystem transport', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', fakeStorage());
    reloadSettingsFromStorage();
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('starts on the persisted track and reports it', () => {
    const { audio } = systemOn(idAt(2));
    expect(audio.musicState().trackId).toBe(idAt(2));
    expect(audio.musicState().playing).toBe(true);
    expect(audio.musicState().durationS).toBeGreaterThan(0);
  });

  it('next and prev wrap around the programme', () => {
    const { audio } = systemOn(idAt(0));
    audio.prevTrack();
    expect(audio.musicState().trackId).toBe(idAt(PLAYLIST.length - 1));
    audio.nextTrack();
    expect(audio.musicState().trackId).toBe(idAt(0));
  });

  it('selecting a track persists it for the next session', () => {
    const { audio } = systemOn(idAt(0));
    audio.selectTrack(idAt(3));
    expect(audio.musicState().trackId).toBe(idAt(3));
    expect(getSettings().musicTrackId).toBe(idAt(3));
  });

  it('repeat all advances to the next track when a song ends', () => {
    const { ctx, audio } = systemOn(idAt(0), 'all');
    runSongToEnd(ctx, ms => vi.advanceTimersByTime(ms));
    expect(audio.musicState().trackId).toBe(idAt(1));
    expect(audio.musicState().playing).toBe(true);
  });

  it('repeat one replays the same track', () => {
    const { ctx, audio } = systemOn(idAt(2), 'one');
    runSongToEnd(ctx, ms => vi.advanceTimersByTime(ms));
    expect(audio.musicState().trackId).toBe(idAt(2));
    expect(audio.musicState().playing).toBe(true);
  });

  it('repeat off stops at the end of the programme', () => {
    const { ctx, audio } = systemOn(idAt(PLAYLIST.length - 1), 'off');
    runSongToEnd(ctx, ms => vi.advanceTimersByTime(ms));
    expect(audio.musicState().playing).toBe(false);
  });

  // The F2 regression, at the layer the player actually touches: the whole
  // path from "the programme finished" to "I pressed Play again".
  it('pressing play after the programme finished restarts the song (F2)', () => {
    const { ctx, audio } = systemOn(idAt(PLAYLIST.length - 1), 'off');
    const duration = audio.musicState().durationS;
    runSongToEnd(ctx, ms => vi.advanceTimersByTime(ms));
    expect(audio.musicState().playing).toBe(false);
    expect(audio.musicProgress().elapsedS).toBeCloseTo(duration, 0);

    audio.setMusicPlaying(true);
    expect(audio.musicState().playing).toBe(true);
    // Used to resume into the final block — a few seconds from the end.
    expect(audio.musicProgress().elapsedS).toBeLessThan(1);
  });

  it('browsing the programme while paused leaves it paused (O7)', () => {
    const { audio } = systemOn(idAt(0));
    audio.setMusicPlaying(false);
    expect(audio.musicState().playing).toBe(false);

    audio.nextTrack();
    expect(audio.musicState().trackId).toBe(idAt(1));
    expect(audio.musicState().playing).toBe(false); // used to resume itself

    audio.selectTrack(idAt(4));
    expect(audio.musicState().playing).toBe(false);

    // ...and resuming then starts the newly chosen track from its top.
    audio.setMusicPlaying(true);
    expect(audio.musicState().playing).toBe(true);
    expect(audio.musicState().trackId).toBe(idAt(4));
    expect(audio.musicProgress().elapsedS).toBeLessThan(1);
  });

  it('toggling shuffle keeps the current track under the cursor', () => {
    const { audio } = systemOn(idAt(3));
    audio.setShuffle(true);
    expect(audio.musicState().trackId).toBe(idAt(3));
    expect(getSettings().musicShuffle).toBe(true);
    audio.setShuffle(false);
    expect(audio.musicState().trackId).toBe(idAt(3));
  });

  it('musicState keeps a stable identity between changes', () => {
    // useSyncExternalStore re-renders on every getSnapshot inequality, so the
    // snapshot must be the same object until something actually changes.
    const { audio } = systemOn(idAt(0));
    const a = audio.musicState();
    expect(audio.musicState()).toBe(a);
    audio.nextTrack();
    expect(audio.musicState()).not.toBe(a);
  });

  it('notifies subscribers on transport changes and stops after unsubscribe', () => {
    const { audio } = systemOn(idAt(0));
    let calls = 0;
    const off = audio.subscribeMusic(() => { calls++; });
    audio.nextTrack();
    expect(calls).toBeGreaterThan(0);
    const seen = calls;
    off();
    audio.nextTrack();
    expect(calls).toBe(seen);
  });

  it('is inert when WebAudio is unavailable', () => {
    const audio = new AudioSystem(() => null);
    expect(() => { audio.unlock(); audio.nextTrack(); audio.setMusicPlaying(true); }).not.toThrow();
    expect(audio.musicProgress().elapsedS).toBe(0);
  });
});
