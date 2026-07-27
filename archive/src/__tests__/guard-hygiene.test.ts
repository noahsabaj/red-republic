// ============================================================
// A guard on the guards.
//
// Twice a source-scanning guard has kept passing while the code it watches
// moved out from under it:
//   - the icon scan, bound to engine.ts, went blind to four icons when v1.9.1
//     moved event emission into systems/ (it had a floor, set to 9 against a
//     real population of 20+, so the floor could never bite);
//   - the format-vocabulary scan, bound to engine.ts, went blind to the exact
//     line it was written for — contracts.ts's `c.amount - c.delivered` slot —
//     and had no floor at all, so looking in the wrong file was
//     indistinguishable from a clean pass.
//
// Both had one shape: a scan addressing ONE file by name. That is what this
// forbids.
//
// It lives in its own file deliberately. import.meta.glob always excludes the
// module that calls it, so a meta-test inside ui-guards.test.ts could not see
// ui-guards.test.ts — the file it most needs to check. The same exclusion
// applies here: this file cannot check itself, which is why it contains no
// scan of its own beyond the two globs below.
//
// Two globs are needed because they cover disjoint sets: the brace-expansion
// pattern used elsewhere does not match files directly in src/__tests__/,
// while './*.ts' reaches exactly those siblings.
// ============================================================
import { describe, expect, it } from 'vitest';

const nested = import.meta.glob<string>('../**/*.ts', { query: '?raw', import: 'default', eager: true });
const siblings = import.meta.glob<string>('./*.ts', { query: '?raw', import: 'default', eager: true });

const guardSources = [...Object.entries(nested), ...Object.entries(siblings)]
  .filter(([path]) => path.includes('test'));

describe('the guards cannot narrow silently', () => {
  it('sees the guard files it is supposed to police', () => {
    // Without this the whole file passes vacuously — the same defect it exists
    // to catch. ui-guards.test.ts is named explicitly because it is the file
    // both historical narrowings happened in.
    expect(guardSources.length).toBeGreaterThan(20);
    expect(guardSources.some(([p]) => p.endsWith('ui-guards.test.ts')), 'ui-guards.test.ts is not in view').toBe(true);
  });

  it('no guard binds a scan to a hardcoded source path', () => {
    const offenders: string[] = [];
    for (const [path, src] of guardSources) {
      // A literal index into a glob result: `allSources['../game/engine.ts']`.
      for (const m of src.matchAll(/\b([A-Za-z_$][\w$]*)\[\s*(['"])(\.\.?\/[^'"]+)\2\s*\]/g)) {
        offenders.push(`${path}: ${m[1]}['${m[3]}'] — scan the glob and filter; don't name one file`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });
});
