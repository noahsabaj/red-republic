// ============================================================
// Player settings: one typed, localStorage-persisted store with the
// same subscribe/notify shape as the engine, so React components read
// it through useSyncExternalStore (src/hooks/use-settings.ts) and
// non-React consumers (audio, canvas, input) read getSettings() live.
//
// The stored object is replaced (never mutated) on every update and
// frozen, so identity comparison is a correct change signal.
//
// Persistence goes through the platform storage facade (localStorage
// in the browser, real files on desktop) — never raw localStorage.
// ============================================================
import { storage } from '@/platform/storage';

export interface Settings {
  // gameplay & UI
  autosaveIntervalDays: 0 | 10 | 30 | 90; // 0 = off
  showBriefing: boolean;      // commissar's briefing on new game
  panSpeed: number;           // 0.5..2 multiplier for WASD/edge panning
  edgePan: boolean;           // pointer near viewport edge pans (mouse only)
  invertZoom: boolean;        // flips wheel-zoom direction
  toastSeconds: number;       // 2..10 toast lifetime
  // display
  uiScale: number;            // 0.85..1.3 root font-size multiplier
  dprCap: 1 | 1.5 | 2;        // canvas devicePixelRatio ceiling
  showGrid: boolean;          // tile-grid overlay on terrain
  // audio
  musicVolume: number;        // 0..1
  sfxVolume: number;          // 0..1
  muted: boolean;             // master mute
  muteWhenHidden: boolean;    // suspend audio while the tab is hidden
  // accessibility
  colorblind: boolean;        // colorblind-safe status palette on the canvas
  reducedMotion: boolean;     // no decorative animation (weather particles, shimmer)
}

const STORAGE_KEY = 'rr.settings.v1';

function prefersReducedMotion(): boolean {
  try {
    return globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
  } catch {
    return false;
  }
}

export function defaultSettings(): Settings {
  return {
    autosaveIntervalDays: 30,
    showBriefing: true,
    panSpeed: 1,
    edgePan: false,
    invertZoom: false,
    toastSeconds: 4.2,
    uiScale: 1,
    dprCap: 2,
    showGrid: false,
    musicVolume: 0.6,
    sfxVolume: 0.8,
    muted: false,
    muteWhenHidden: true,
    colorblind: false,
    reducedMotion: prefersReducedMotion(),
  };
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

const bool = (v: unknown, dv: boolean) => (typeof v === 'boolean' ? v : dv);
const numIn = (v: unknown, lo: number, hi: number, dv: number) =>
  (typeof v === 'number' && Number.isFinite(v) ? clamp(v, lo, hi) : dv);
const oneOf = <T,>(v: unknown, allowed: readonly T[], dv: T) =>
  (allowed.includes(v as T) ? (v as T) : dv);

/** Per-key sanitizers: unknown keys are dropped, bad values fall back to defaults. */
function sanitize(raw: unknown): Settings {
  const d = defaultSettings();
  if (typeof raw !== 'object' || raw === null) return d;
  const r = raw as Record<string, unknown>;
  return {
    autosaveIntervalDays: oneOf(r.autosaveIntervalDays, [0, 10, 30, 90] as const, d.autosaveIntervalDays),
    showBriefing: bool(r.showBriefing, d.showBriefing),
    panSpeed: numIn(r.panSpeed, 0.5, 2, d.panSpeed),
    edgePan: bool(r.edgePan, d.edgePan),
    invertZoom: bool(r.invertZoom, d.invertZoom),
    toastSeconds: numIn(r.toastSeconds, 2, 10, d.toastSeconds),
    uiScale: numIn(r.uiScale, 0.85, 1.3, d.uiScale),
    dprCap: oneOf(r.dprCap, [1, 1.5, 2] as const, d.dprCap),
    showGrid: bool(r.showGrid, d.showGrid),
    musicVolume: numIn(r.musicVolume, 0, 1, d.musicVolume),
    sfxVolume: numIn(r.sfxVolume, 0, 1, d.sfxVolume),
    muted: bool(r.muted, d.muted),
    muteWhenHidden: bool(r.muteWhenHidden, d.muteWhenHidden),
    colorblind: bool(r.colorblind, d.colorblind),
    reducedMotion: bool(r.reducedMotion, d.reducedMotion),
  };
}

function load(): Settings {
  try {
    const raw = storage()?.getItem(STORAGE_KEY);
    if (!raw) return defaultSettings();
    return sanitize(JSON.parse(raw));
  } catch {
    return defaultSettings();
  }
}

let current: Settings = Object.freeze(load());
const listeners = new Set<() => void>();

function persist() {
  try {
    storage()?.setItem(STORAGE_KEY, JSON.stringify(current));
  } catch {
    // storage unavailable/full — settings still apply for this session
  }
}

/** Stable object identity between writes — safe for useSyncExternalStore. */
export function getSettings(): Settings {
  return current;
}

export function updateSettings(patch: Partial<Settings>): void {
  current = Object.freeze(sanitize({ ...current, ...patch }));
  persist();
  listeners.forEach(fn => fn());
}

export function resetSettings(): void {
  current = Object.freeze(defaultSettings());
  persist();
  listeners.forEach(fn => fn());
}

export function subscribeSettings(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Test seam: reload from storage (after stubbing localStorage). */
export function reloadSettingsFromStorage(): void {
  current = Object.freeze(load());
  listeners.forEach(fn => fn());
}
