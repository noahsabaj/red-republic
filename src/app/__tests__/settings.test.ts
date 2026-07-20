import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  defaultSettings, getSettings, reloadSettingsFromStorage, resetSettings,
  subscribeSettings, updateSettings,
} from '../settings';

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

describe('settings store', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', fakeStorage());
    reloadSettingsFromStorage();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    reloadSettingsFromStorage();
  });

  it('starts at defaults when storage is empty', () => {
    expect(getSettings()).toEqual(defaultSettings());
  });

  it('updates merge, clamp, persist, and notify exactly once per write', () => {
    let notified = 0;
    const unsub = subscribeSettings(() => notified++);
    const before = getSettings();
    updateSettings({ panSpeed: 1.5, uiScale: 99 }); // uiScale clamps to 1.3
    expect(notified).toBe(1);
    expect(getSettings().panSpeed).toBe(1.5);
    expect(getSettings().uiScale).toBe(1.3);
    expect(getSettings()).not.toBe(before); // fresh identity per write
    const stored = JSON.parse(localStorage.getItem('rr.settings.v1')!) as { panSpeed: number };
    expect(stored.panSpeed).toBe(1.5);
    unsub();
    updateSettings({ muted: true });
    expect(notified).toBe(1); // unsubscribed
  });

  it('drops unknown keys and repairs bad values on load', () => {
    vi.stubGlobal('localStorage', fakeStorage({
      'rr.settings.v1': JSON.stringify({
        panSpeed: 'fast', dprCap: 3, autosaveIntervalDays: 17, legacyKnob: true, muted: true,
      }),
    }));
    reloadSettingsFromStorage();
    const s = getSettings();
    expect(s.panSpeed).toBe(1);           // bad type → default
    expect(s.dprCap).toBe(2);             // not in the allowed set → default
    expect(s.autosaveIntervalDays).toBe(30);
    expect(s.muted).toBe(true);           // valid value survives
    expect('legacyKnob' in s).toBe(false);
  });

  it('clamps interfaceVolume and repairs hoverSounds on load', () => {
    updateSettings({ interfaceVolume: 5 });
    expect(getSettings().interfaceVolume).toBe(1);   // clamps high
    updateSettings({ interfaceVolume: -2 });
    expect(getSettings().interfaceVolume).toBe(0);   // clamps low
    expect(defaultSettings().hoverSounds).toBe(true);
    vi.stubGlobal('localStorage', fakeStorage({
      'rr.settings.v1': JSON.stringify({ interfaceVolume: 'loud', hoverSounds: 'yes' }),
    }));
    reloadSettingsFromStorage();
    expect(getSettings().interfaceVolume).toBe(defaultSettings().interfaceVolume); // bad type → default
    expect(getSettings().hoverSounds).toBe(true);
  });

  it('survives corrupt JSON and missing storage', () => {
    vi.stubGlobal('localStorage', fakeStorage({ 'rr.settings.v1': '{oops' }));
    reloadSettingsFromStorage();
    expect(getSettings()).toEqual(defaultSettings());

    vi.stubGlobal('localStorage', undefined);
    reloadSettingsFromStorage();
    expect(getSettings()).toEqual(defaultSettings());
    updateSettings({ muted: true }); // persist is a no-op, value still applies
    expect(getSettings().muted).toBe(true);
  });

  it('reset restores defaults and notifies', () => {
    updateSettings({ muted: true, showGrid: true });
    let notified = 0;
    subscribeSettings(() => notified++);
    resetSettings();
    expect(getSettings()).toEqual(defaultSettings());
    expect(notified).toBe(1);
  });

  it('defaults and sanitizes the music player preferences', () => {
    const d = defaultSettings();
    expect(d.musicShuffle).toBe(false);
    expect(d.musicRepeat).toBe('all');
    expect(d.musicTrackId).toBe('');

    vi.stubGlobal('localStorage', fakeStorage({
      'rr.settings.v1': JSON.stringify({ musicShuffle: 'yes', musicRepeat: 'sometimes', musicTrackId: 42 }),
    }));
    reloadSettingsFromStorage();
    const s = getSettings();
    expect(s.musicShuffle).toBe(false);   // bad type → default
    expect(s.musicRepeat).toBe('all');    // not in the allowed set → default
    expect(s.musicTrackId).toBe('');      // non-string → default

    updateSettings({ musicShuffle: true, musicRepeat: 'one', musicTrackId: 'march' });
    expect(getSettings().musicShuffle).toBe(true);
    expect(getSettings().musicRepeat).toBe('one');
    expect(getSettings().musicTrackId).toBe('march');
  });
});
