// The national stockpile: every bin in the republic, recounted from scratch
// once a day. Cheap, and a full recount is what keeps the headline figure from
// drifting away from the bins it claims to summarise.
import { ALL_RESOURCES } from '../config';
import type { ResourceId } from '../config';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['totals'];

export function totals(w: World): Mutation[] {
  const sum = Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>;
  for (const b of w.buildings.values()) {
    for (const r of ALL_RESOURCES) sum[r] += w.stockOf(b, r);
  }
  return [{ k: 'totals', totals: sum }];
}
