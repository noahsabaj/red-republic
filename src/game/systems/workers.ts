// Who works where. A share of the population is of working age; every
// connected workplace gets a skeleton crew first so no production chain dies
// outright, and only then is the remainder spread proportionally over the open
// jobs. Order is the player's `priorityHigh` flag first, then the authored
// `allocationPriority` — the same override that jumps a building up the power
// queue, because one flag for both scarce things is one thing to learn.
import { BALANCE } from '../config';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['labour', 'staff'];

export function workers(w: World): Mutation[] {
  const total = Math.floor(w.pop * BALANCE.workerShare);
  const list = [...w.buildings.values()]
    .filter(b => b.constructed && w.def(b).workers > 0 && b.connected)
    .sort((a, b2) => {
      // the player's own per-building flag outranks the authored order
      const hi = Number(b2.priorityHigh ?? false) - Number(a.priorityHigh ?? false);
      if (hi !== 0) return hi;
      return w.allocationRank(a) - w.allocationRank(b2);
    });
  const jobs = list.reduce((s, b) => s + w.def(b).workers, 0);

  // Staffing is worked out in a local table and emitted once per building, so
  // an unstaffable workplace is set to zero as explicitly as a staffed one.
  const staff = new Map<number, number>();
  for (const b of w.buildings.values()) staff.set(b.id, 0);
  const at = (id: number) => staff.get(id) ?? 0;

  let pool = total;
  // pass 1: every workplace gets a skeleton crew so all chains keep running
  for (const b of list) {
    if (pool <= 0) break;
    staff.set(b.id, 1);
    pool--;
  }
  // pass 2: distribute the rest proportionally to remaining open jobs
  const rem = list.map(b => w.def(b).workers - at(b.id));
  const remTotal = rem.reduce((x, y) => x + y, 0);
  if (pool > 0 && remTotal > 0) {
    list.forEach((b, i) => staff.set(b.id, at(b.id) + Math.min(rem[i], Math.floor((pool * rem[i]) / remTotal))));
    const used = list.reduce((x, b) => x + at(b.id), 0);
    let left = total - used;
    for (const b of list) {
      while (left > 0 && at(b.id) < w.def(b).workers) { staff.set(b.id, at(b.id) + 1); left--; }
      if (left <= 0) break;
    }
  }

  const out: Mutation[] = [
    { k: 'labour', workers: total, jobs, employed: list.reduce((x, b) => x + at(b.id), 0) },
  ];
  for (const b of w.buildings.values()) out.push({ k: 'staff', id: b.id, staff: at(b.id) });
  return out;
}
