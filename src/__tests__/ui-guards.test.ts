// ============================================================
// Guards for silent UI-breakage classes:
// 1. every icon NAME referenced by config/engine must exist in the registry
//    (config/engine stay UI-free, so this is the type-safety substitute)
// 2. no emoji in first-party sources — the game uses the icon system
// 3. Tailwind color-opacity modifiers must be real opacity steps —
//    an invalid step (e.g. bg-red-950/97) silently generates NO css
// 4. full-viewport stacking layers must be modals or pointer-transparent —
//    a transparent inset-0 z-layer silently eats clicks for overlays below
// ============================================================
import { describe, expect, it } from 'vitest';
import { BUILDINGS, RESOURCES } from '@/game/config';
import { isGameIcon } from '@/ui/icons';

const allSources = import.meta.glob<string>('../**/*.{ts,tsx,css}', { query: '?raw', import: 'default', eager: true });
const firstParty = Object.entries(allSources).filter(([path]) =>
  !path.includes('/components/ui/') && !path.includes('/__tests__/') && !path.endsWith('use-mobile.ts'));

describe('icon registry coverage', () => {
  it('covers every building and resource icon in config', () => {
    for (const def of Object.values(BUILDINGS)) {
      expect(isGameIcon(def.icon), `building ${def.id} icon '${def.icon}'`).toBe(true);
    }
    for (const res of Object.values(RESOURCES)) {
      expect(isGameIcon(res.icon), `resource ${res.id} icon '${res.icon}'`).toBe(true);
    }
  });

  it("covers every icon name the engine's alerts and events use", () => {
    const src = allSources['../game/engine.ts'];
    const names = new Set<string>();
    for (const m of src.matchAll(/icon: '([A-Za-z-]+)'/g)) names.add(m[1]);
    for (const m of src.matchAll(/pushEvent\([^)]*, '(?:good|bad|info)', '([A-Za-z-]+)'\)/g)) names.add(m[1]);
    expect(names.size).toBeGreaterThanOrEqual(9);
    for (const n of names) expect(isGameIcon(n), `engine icon '${n}'`).toBe(true);
  });
});

describe('no emoji in first-party sources', () => {
  it('finds zero emoji (the icon system replaced them)', () => {
    const emoji = /[\u{1F000}-\u{1FAFF}\u{2190}-\u{21FF}\u{2600}-\u{27BF}\u{2B00}-\u{2BFF}]/u;
    // typographic characters welcome in a Soviet-themed UI; not stickers
    const typographic = /[★→⇔·×]/gu;
    const offenders: string[] = [];
    for (const [path, src] of firstParty) {
      src.split('\n').forEach((line, i) => {
        if (emoji.test(line.replace(typographic, ''))) offenders.push(`${path}:${i + 1} ${line.trim().slice(0, 80)}`);
      });
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});

describe('full-viewport stacking layers', () => {
  it('every inset-0 z-layer is a real modal or pointer-events-none', () => {
    // The class this guards: MainMenu's transparent z-40 layout layer used to
    // swallow every click aimed at the z-30 UpdateBanner beneath it — layout
    // containers must pass pointers through; only actual modals may block.
    const offenders: string[] = [];
    for (const [path, src] of firstParty) {
      if (!path.endsWith('.tsx')) continue;
      for (const m of src.matchAll(/(["'`])[^"'`]*?\binset-0\b[^"'`]*?\1/g)) {
        const cls = m[0];
        if (!/\bz-\d/.test(cls)) continue;                  // no stacking level -> follows document order, fine
        if (cls.includes('pointer-events-none')) continue;  // pointer-transparent layout layer
        const tagBefore = src.slice(Math.max(0, m.index - 300), m.index);
        if (/aria-modal|role="dialog"/.test(tagBefore)) continue; // real modal, blocking is the point
        const line = src.slice(0, m.index).split('\n').length;
        offenders.push(`${path}:${line} ${cls.slice(0, 70)}`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});

describe('tailwind color-opacity steps', () => {
  it('uses only real opacity-scale steps (invalid ones silently emit no CSS)', () => {
    const allowed = new Set(Array.from({ length: 21 }, (_, i) => String(i * 5)));
    const offenders: string[] = [];
    for (const [path, src] of firstParty) {
      if (!path.endsWith('.tsx')) continue;
      for (const m of src.matchAll(/(?:bg|text|border|from|to|via|ring|fill|stroke|divide|outline|decoration|accent|shadow)-[a-z]+-\d{2,3}\/(\d{1,3})/g)) {
        if (!allowed.has(m[1])) offenders.push(`${path}: ${m[0]}`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});
