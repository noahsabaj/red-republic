import { describe, expect, it } from 'vitest';
import { soundForEvent } from '../event-sounds';
import { SFX_DEFS } from '../sfx';

/**
 * The simulation's real event vocabulary, read out of its source. Events used
 * to be pushed from engine.ts alone; v1.9.1 moved most of them into systems/,
 * so anything that scans a single file now sees only part of the vocabulary.
 */
const simSources = import.meta.glob<string>('../../game/**/*.ts', { query: '?raw', import: 'default', eager: true });

interface Emitted { kind: string; icon: string; where: string }

function emittedEvents(): Emitted[] {
  const out: Emitted[] = [];
  for (const [path, src] of Object.entries(simSources)) {
    if (path.includes('/__tests__/')) continue;
    // `kind: 'good', icon: 'star'` — the mutation-object form the systems use.
    for (const m of src.matchAll(/kind: '(good|bad|info)',\s*icon: '([A-Za-z-]+)'/g)) {
      out.push({ kind: m[1], icon: m[2], where: path });
    }
    // `pushEvent(text, 'good', 'star')` — the positional helper on the engine.
    for (const m of src.matchAll(/pushEvent\([^)]*?'(good|bad|info)',\s*'([A-Za-z-]+)'\)/g)) {
      out.push({ kind: m[1], icon: m[2], where: path });
    }
  }
  return out;
}

/**
 * Events the player deliberately hears nothing for. Being explicit is the
 * point: a new event must either get a sound or be listed here, and cannot
 * slip through by being silently unmapped — which is the exact failure the
 * string-keyed map makes invisible.
 *
 * `coins` is the interesting one: a `coin` recipe exists, but it fires from
 * the trade panel as the OUTCOME of a player action (SidePanel). Sounding the
 * engine's money toast too would double-fire the same moment, which the
 * one-owner-per-moment rule forbids.
 */
const DELIBERATELY_SILENT = new Set([
  'good:coins', 'bad:coins',   // already sounded by the trade panel that caused them
  'bad:bulldoze',              // the bulldoze SFX already played on the action
  'info:star',                 // new year / the opening Politburo grant — text, not a fanfare
  'info:spring',               // seasonal note; only the harsh turns get a cue
]);

describe('event → sound mapping', () => {
  it('maps the flagship events', () => {
    expect(soundForEvent('good', 'star')).toBe('objective');
    expect(soundForEvent('good', 'check')).toBe('complete');
    expect(soundForEvent('good', 'contract')).toBe('contractDone');
    expect(soundForEvent('bad', 'contract')).toBe('alertBad');
    expect(soundForEvent('info', 'contract')).toBe('contractOffer');
  });

  it('unmapped events stay silent', () => {
    expect(soundForEvent('info', 'spring')).toBeNull();
    expect(soundForEvent('info', 'star')).toBeNull();
    expect(soundForEvent('good', undefined)).toBeNull();
    expect(soundForEvent('weird', 'nonsense')).toBeNull();
  });

  it('every mapped sound has a synth recipe', () => {
    for (const { kind, icon } of emittedEvents()) {
      const s = soundForEvent(kind, icon);
      if (s !== null) expect(SFX_DEFS[s], `${kind}:${icon}`).toBeTypeOf('function');
    }
  });

  // The drift guard. event-sounds.ts keys its map on the STRING `kind:icon`,
  // so renaming an event's icon mutes it silently — no error, no test failure,
  // just a sound that stops happening. Deriving the vocabulary from the
  // simulation's own source turns that into a build break.
  it('every event the sim can emit is either mapped or explicitly silent', () => {
    const events = emittedEvents();
    expect(events.length, 'the source scan found nothing — the emission shape changed').toBeGreaterThan(15);

    const unaccounted = [...new Set(
      events
        .filter(e => soundForEvent(e.kind, e.icon) === null)
        .filter(e => !DELIBERATELY_SILENT.has(`${e.kind}:${e.icon}`))
        .map(e => `${e.kind}:${e.icon} (${e.where})`),
    )];
    expect(unaccounted, 'give these a sound in event-sounds.ts, or list them in DELIBERATELY_SILENT').toEqual([]);
  });

  it('nothing is listed as silent that the sim can no longer emit', () => {
    const real = new Set(emittedEvents().map(e => `${e.kind}:${e.icon}`));
    const stale = [...DELIBERATELY_SILENT].filter(k => !real.has(k));
    expect(stale, 'these events no longer exist — drop them from DELIBERATELY_SILENT').toEqual([]);
  });
});
