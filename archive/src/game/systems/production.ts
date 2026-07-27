// A day's work in every building: consume the inputs, wear the machinery, make
// the outputs. The rates themselves come from `productionRates()` — the same
// call the inspector displays — so what the panel promises is literally what
// gets applied.
//
// Staged, because it reads its own writes twice over: `clampedAdd` has to see
// what an earlier emission already put in (or took out of) the same bin, and
// `productionRates` for a building has to see the world the buildings before it
// left behind. Batching instead would only be correct while no two buildings
// ever share a bin — an assumption nothing enforces and nobody would think to
// check when adding a mechanic that breaks it.
import type { ResourceId } from '../config';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import type { BuildingInst, World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['eff', 'farmFields', 'stock', 'produced'];

export function production(w: World): Mutation[] {
  const s = new Staged(w);
  // What the bin can actually take right now — the world already reflects every
  // mutation emitted before this one, so there is no shadow ledger to keep.
  const move = (b: BuildingInst, r: ResourceId, amt: number): number => {
    const delta = w.clampedAdd(b, r, amt);
    s.emit({ k: 'stock', id: b.id, r, delta });
    return delta;
  };

  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (!b.constructed) continue;
    // Plants keep the eff the power/heat system fixed for them; everyone else
    // gets theirs here. Both are display-only — the rates below recompute.
    if (!def.powerOutput && !def.heatOutput) {
      s.emit({ k: 'eff', id: b.id, eff: w.baseEff(b) });
      if (def.isFarm) s.emit({ k: 'farmFields', id: b.id, fields: Math.min(12, w.countFarmFields(b.x, b.y, b.w, b.h)) });
    }
    const rates = w.productionRates(b);
    for (const [r, amt] of Object.entries(rates.inputs) as [ResourceId, number][]) {
      move(b, r, -amt);
    }
    for (const [r, amt] of Object.entries(rates.outputs) as [ResourceId, number][]) {
      s.emit({ k: 'produced', r, amount: move(b, r, amt) });
    }
  }
  return s.muts;
}
