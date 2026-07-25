// A day's work in every building: consume the inputs, wear the machinery, make
// the outputs. The rates themselves come from `productionRates()` — the same
// call the inspector displays — so what the panel promises is literally what
// gets applied.
import type { ResourceId } from '../config';
import type { Mutation } from '../mutation';
import type { BuildingInst, World } from '../world';

export function production(w: World): Mutation[] {
  const out: Mutation[] = [];
  // Deltas already emitted this batch, so a bin's clamp accounts for what an
  // earlier mutation in the same list will have put in (or taken out of) it.
  const applied = new Map<string, number>();
  const add = (b: BuildingInst, r: ResourceId, amt: number) => {
    const key = `${b.id}:${r}`;
    const prior = applied.get(key) ?? 0;
    const delta = w.clampedAdd(b, r, amt, w.stockOf(b, r) + prior);
    applied.set(key, prior + delta);
    return delta;
  };

  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (!b.constructed) continue;
    // Plants keep the eff the power/heat system fixed for them; everyone else
    // gets theirs here. Both are display-only — the rates below recompute.
    if (!def.powerOutput && !def.heatOutput) {
      out.push({ k: 'eff', id: b.id, eff: w.baseEff(b) });
      if (def.isFarm) out.push({ k: 'farmFields', id: b.id, fields: Math.min(12, w.countFarmFields(b.x, b.y, b.w, b.h)) });
    }
    const rates = w.productionRates(b);
    for (const [r, amt] of Object.entries(rates.inputs) as [ResourceId, number][]) {
      out.push({ k: 'stock', id: b.id, r, delta: add(b, r, -amt) });
    }
    for (const [r, amt] of Object.entries(rates.outputs) as [ResourceId, number][]) {
      const delta = add(b, r, amt);
      out.push({ k: 'stock', id: b.id, r, delta });
      out.push({ k: 'produced', r, amount: delta });
    }
  }
  return out;
}
