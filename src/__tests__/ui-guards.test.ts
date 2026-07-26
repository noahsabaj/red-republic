// ============================================================
// Guards for silent UI-breakage classes:
// 1. every icon NAME referenced by config/engine must exist in the registry
//    (config/engine stay UI-free, so this is the type-safety substitute)
// 2. no emoji in first-party sources — the game uses the icon system
// 3. Tailwind color-opacity modifiers must be real opacity steps —
//    an invalid step (e.g. bg-red-950/97) silently generates NO css
// 4. full-viewport stacking layers must be modals or pointer-transparent —
//    a transparent inset-0 z-layer silently eats clicks for overlays below
// 5. resource quantities in engine event strings must go through the format
//    vocabulary — a raw `${amount - delivered}` beside a resource name was the
//    float-tail bug (`42.1423… coal undelivered`) this whole effort started from
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

  it("covers every icon name the sim's alerts and events use", () => {
    // Scans the WHOLE simulation, not just engine.ts. This guard was written
    // when engine.ts was the only place events came from; v1.9.1 moved most
    // emission into systems/, and port/rain/star/summer quietly fell outside
    // it. A guard that keeps passing while the code it watches moves away is
    // worse than no guard, because it still reads as coverage.
    const names = new Set<string>();
    for (const [path, src] of Object.entries(allSources)) {
      if (!path.startsWith('../game/') || path.includes('/__tests__/')) continue;
      for (const m of src.matchAll(/icon: '([A-Za-z-]+)'/g)) names.add(m[1]);
      for (const m of src.matchAll(/pushEvent\([^)]*, '(?:good|bad|info)', '([A-Za-z-]+)'\)/g)) names.add(m[1]);
    }
    expect(names.size).toBeGreaterThanOrEqual(20);
    for (const n of names) expect(isGameIcon(n), `sim icon '${n}'`).toBe(true);
  });
});

describe('allocationPriority covers everything that competes for it', () => {
  // This replaced JOB_PRIORITY, an ordered list of building ids in engine.ts.
  // A list is a thing you must remember to edit, and what you forget lands
  // silently in last place — which is how HOUSING came to be the first thing
  // switched off in a brownout: it employs nobody, so it was never in the
  // *jobs* list that also happened to decide the power queue.
  it('every building that competes for workers or power declares a rank', () => {
    for (const def of Object.values(BUILDINGS)) {
      if (def.workers <= 0 && def.power <= 0) continue;
      expect(def.allocationPriority, `${def.id} competes but declares no allocationPriority`)
        .toBeTypeOf('number');
    }
  });

  it('does not set one on a building that never queues for anything', () => {
    for (const def of Object.values(BUILDINGS)) {
      if (def.allocationPriority === undefined) continue;
      expect(def.workers > 0 || def.power > 0, `${def.id} sets allocationPriority but queues for nothing`).toBe(true);
    }
  });

  it('ranks are distinct among the buildings that draw power, so a brownout order is unambiguous', () => {
    // Ties are legal (they fall back to commissioning order) but two DIFFERENT
    // power consumers sharing a rank means the author did not really choose.
    // Housing is the deliberate exception: both blocks sit together, last.
    const consumers = Object.values(BUILDINGS).filter(d => d.power > 0 && d.category !== 'housing');
    const ranks = consumers.map(d => d.allocationPriority);
    expect(new Set(ranks).size, 'duplicate allocationPriority among power consumers').toBe(ranks.length);
  });
});

describe('unpoweredEff is authored where it means something', () => {
  // `unpoweredEff` replaced a hardcoded set of building ids in the engine. It
  // only ever applies while a building draws power and cannot get it, so
  // setting it on a generator or an unpowered building is a silent no-op —
  // exactly the kind of dead data the set it replaced had two of.
  it('is only set on buildings that actually draw power, and is a real fraction', () => {
    for (const def of Object.values(BUILDINGS)) {
      if (def.unpoweredEff === undefined) continue;
      expect(def.power, `${def.id} sets unpoweredEff but draws no power`).toBeGreaterThan(0);
      expect(def.unpoweredEff, `${def.id} unpoweredEff`).toBeGreaterThanOrEqual(0);
      expect(def.unpoweredEff, `${def.id} unpoweredEff`).toBeLessThanOrEqual(1);
    }
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

describe('resource quantities in engine events use the format vocabulary', () => {
  it('never interpolates a raw arithmetic quantity beside a resource name', () => {
    // The bug this whole effort started from: a contract-failure toast printed
    // `${c.amount - c.delivered}` raw, leaking a float tail. Any number shown
    // next to a resource name must route through the format.ts vocabulary
    // (fmtQty / fmtOwed / …). A bare field like `${c.amount}` (integer by
    // construction) is fine; raw arithmetic in the slot is not.
    const src = allSources['../game/engine.ts'];
    const offenders: string[] = [];
    for (const m of src.matchAll(/\$\{([^{}]+)\}\s+\$\{RESOURCES\[/g)) {
      const expr = m[1].trim();
      if (/[-+*/]/.test(expr) && !expr.startsWith('fmt')) {
        const line = src.slice(0, m.index).split('\n').length;
        offenders.push(`engine.ts:${line}  \${${expr}}  — wrap in a format.ts helper (fmtQty/fmtOwed/…)`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});
