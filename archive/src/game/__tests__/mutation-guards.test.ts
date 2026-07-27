import { describe, expect, it } from 'vitest';

// Source-scanning guard (like ui-guards.test.ts). The routing caches key on
// structural dimension revisions bumped by exactly one mutator each; this test
// enforces that those mutators are the ONLY writers, so a future edit can't
// silently bypass a revision bump and leave a derived cache stale.
const engineSource = import.meta.glob<string>('../engine.ts', {
  query: '?raw', import: 'default', eager: true,
})['../engine.ts'];

const KEYWORDS = new Set(['if', 'for', 'while', 'switch', 'catch', 'do', 'else', 'return', 'function']);
const METHOD_DECL = /^ {2}(?:private |protected |public |static |readonly |async |get |set )*([A-Za-z_]\w*)\s*[(<]/;

/** Blank out comment bodies (keeping newlines, so line numbers survive) — the guard
 *  scans code, not prose that happens to quote a `.field =` pattern in a doc block. */
function stripComments(src: string): string {
  return src
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
    .replace(/\/\/[^\n]*/g, '');
}

/** Report every code line matching `pattern` whose enclosing class member is not allowed. */
function offendersOutside(source: string, pattern: RegExp, allowed: Set<string>): string[] {
  const offenders: string[] = [];
  let member = '<file scope>';
  const original = source.split('\n');
  stripComments(source).split('\n').forEach((line, i) => {
    const m = METHOD_DECL.exec(line);
    if (m && !KEYWORDS.has(m[1])) member = m[1];
    if (pattern.test(line) && !allowed.has(member)) {
      offenders.push(`engine.ts:${i + 1} [${member}] ${original[i].trim().slice(0, 80)}`);
    }
  });
  return offenders;
}

describe('engine mutation discipline', () => {
  it('has a readable source to scan', () => {
    expect(typeof engineSource).toBe('string');
    expect(engineSource.length).toBeGreaterThan(1000);
  });

  it('writes tile routing/visual fields only in applyInternalTilePatches', () => {
    // Tile fields feed the topology cost functions; every write must flow through
    // the one patch method that computes the affected-domain invalidation.
    const pattern = /\.(road|terrain|foreign|buildingId|deposit|variant)\s*=(?!=)/;
    const offenders = offendersOutside(engineSource, pattern, new Set(['applyInternalTilePatches']));
    expect(offenders, offenders.join('\n')).toEqual([]);
  });

  it('flips b.constructed only in markConstructed (which bumps facilityRevision)', () => {
    const pattern = /\.constructed\s*=(?!=)/;
    const offenders = offendersOutside(engineSource, pattern, new Set(['markConstructed']));
    expect(offenders, offenders.join('\n')).toEqual([]);
  });

  it('mutates the buildings map only in addBuilding / removeBuilding', () => {
    const pattern = /\bthis\.buildings\.(set|delete)\s*\(/;
    const offenders = offendersOutside(engineSource, pattern, new Set(['addBuilding', 'removeBuilding']));
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});
