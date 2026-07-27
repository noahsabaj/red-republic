// Barge dispatch. Water is the only network lorries cannot use, so a port pair
// on a shared water component is the one route that needs its own vehicle.
// Orders queue until there is stock portside, a free barge and fair weather —
// ice or a gale keeps them tied up rather than dropping the order.
//
// Staged: orders are consumed as they are served, and the whole loop walks the
// queue backwards removing entries, so each decision must see the queue the
// previous one left behind.
import { BALANCE, WEATHER } from '../config';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import { shareAnyComponent } from '../topology';
import { rankedGoals } from '../world';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['boatOrdersClear', 'boatOrderDrop', 'boatOrderTake', 'boatDispatch', 'routingRejection'];

export function boats(w: World): Mutation[] {
  const s = new Staged(w);
  const ports = [...w.buildings.values()].filter(p => w.def(p).isPort && p.constructed);
  if (!ports.length) { s.emit({ k: 'boatOrdersClear' }); return s.muts; }
  // ice or grounding weather keeps barges in port — orders wait for fair skies
  if (w.weather.riverFrozen || WEATHER[w.weather.condition].boatMult === 0) return s.muts;
  let activeBoats = w.boats.filter(b => b.phase === 'go').length;
  // Backwards, and orders are dropped by INDEX: Staged applies each removal
  // immediately, so every index below `i` is still the one this loop read.
  for (let i = w.boatOrders.length - 1; i >= 0; i--) {
    if (activeBoats >= ports.length) break;
    const order = w.boatOrders[i];
    const src = w.buildings.get(order.srcId);
    const dest = w.buildings.get(order.destId);
    if (!src?.constructed || !dest?.constructed) { s.emit({ k: 'boatOrderDrop', index: i }); continue; }
    const avail = w.stockOf(src, order.r);
    if (avail < 1) continue; // trucks are still bringing it portside

    const destAccess = w.waterAccess(dest);
    const srcAccess = w.waterAccess(src);
    if (!shareAnyComponent(destAccess.components, srcAccess.components)) {
      s.emit({ k: 'routingRejection' }, { k: 'boatOrderDrop', index: i });
      continue;
    }
    const nearest = w.nearestPath('water', destAccess.tiles, rankedGoals(srcAccess.tiles, 0, null));
    if (!nearest) { s.emit({ k: 'boatOrderDrop', index: i }); continue; }

    const amount = Math.min(order.amt, avail, BALANCE.boatCapacity,
      w.capOf(dest, order.r) - w.stockOf(dest, order.r) - w.incomingOf(dest, order.r));
    if (amount < 1) { s.emit({ k: 'boatOrderDrop', index: i }); continue; }
    s.emit({ k: 'boatDispatch', srcId: src.id, destId: dest.id, r: order.r, amount, path: nearest.path });
    activeBoats++;
    s.emit({ k: 'boatOrderTake', index: i, amount });
    if (order.amt < 1) s.emit({ k: 'boatOrderDrop', index: i });
  }
  return s.muts;
}
