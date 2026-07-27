// F11's destination rule and the settings side of window mode. The window
// calls themselves need a real Tauri window and are verified by hand on the
// desktop build; what is pinned here is everything that decides WHICH mode
// the window is asked for, because that is where the panel and the hotkey
// could silently disagree.
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fullscreenToggleTarget } from '../desktop';
import { defaultSettings, getSettings, resetSettings, updateSettings } from '@/app/settings';
import type { WindowMode } from '@/app/settings';

describe('fullscreenToggleTarget (F11)', () => {
  it('enters fullscreen from any framed mode', () => {
    expect(fullscreenToggleTarget('windowed', 'windowed')).toBe('fullscreen');
    expect(fullscreenToggleTarget('borderless', 'borderless')).toBe('fullscreen');
  });

  it('returns to the framing it came from, not to windowed', () => {
    // The bug this pins: a borderless player tapping F11 twice must land back
    // in borderless. Returning a hardcoded 'windowed' would destroy their
    // choice with a hotkey and force them into Options to undo it.
    expect(fullscreenToggleTarget('fullscreen', 'borderless')).toBe('borderless');
    expect(fullscreenToggleTarget('fullscreen', 'windowed')).toBe('windowed');
  });

  it('round-trips every framed mode', () => {
    for (const framed of ['windowed', 'borderless'] as const) {
      expect(fullscreenToggleTarget(fullscreenToggleTarget(framed, framed), framed)).toBe(framed);
    }
  });
});

describe('windowMode in the settings store', () => {
  beforeEach(() => {
    vi.stubGlobal('localStorage', {
      getItem: () => null, setItem: () => {}, removeItem: () => {},
      clear: () => {}, key: () => null, length: 0,
    });
    updateSettings({ windowMode: defaultSettings().windowMode });
  });

  it('defaults to fullscreen', () => {
    expect(defaultSettings().windowMode).toBe('fullscreen');
  });

  it('survives Reset to defaults', () => {
    // Resetting is for game preferences. Relaying out the player's monitor as
    // a side effect of fixing an HUD scale is the surprise being prevented.
    updateSettings({ windowMode: 'windowed', uiScale: 1.3 });
    resetSettings();
    expect(getSettings().windowMode).toBe('windowed');
    expect(getSettings().uiScale).toBe(defaultSettings().uiScale);
  });

  it('rejects a corrupt stored value instead of passing it to the window API', () => {
    updateSettings({ windowMode: 'maximised' as unknown as WindowMode });
    expect(getSettings().windowMode).toBe('fullscreen');
  });

  it('accepts every real mode', () => {
    for (const m of ['windowed', 'borderless', 'fullscreen'] as const) {
      updateSettings({ windowMode: m });
      expect(getSettings().windowMode).toBe(m);
    }
  });
});
