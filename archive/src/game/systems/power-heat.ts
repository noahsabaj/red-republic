// The grid and the boilers: how much is generated, and who gets it when there
// isn't enough.
//
// Plant efficiency and fuel factor are fixed for the whole day here — the power
// factor deliberately reads YESTERDAY's allocation, because a plant's own
// output cannot depend on the allocation its output is about to decide.
// `production()` burns fuel through `productionRates()` using these same stored
// factors, so what comes out and what gets burnt always agree.
import type { ResourceId } from '../config';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['eff', 'coalFactor', 'grid', 'powered', 'heated'];

export function powerHeat(w: World): Mutation[] {
  const out: Mutation[] = [];

  // Heat demand first (temperature-scaled) so plants can throttle to it:
  // mild days sip coal, a January cold snap burns through the stockpile.
  const heatFactor = w.heatDemandFactor();
  let heatDemand = 0;
  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (b.constructed && def.heat > 0) heatDemand += def.heat * heatFactor;
  }

  let powerProduced = 0;
  let heatProduced = 0;
  let heatToServe = heatDemand;
  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (!b.constructed || (!def.powerOutput && !def.heatOutput)) continue;
    const eff = w.baseEff(b);
    out.push({ k: 'eff', id: b.id, eff });
    if (def.powerOutput) {
      const inputRes = def.inputs ? (Object.keys(def.inputs)[0] as ResourceId) : 'coal';
      const need = (def.inputs?.[inputRes] ?? 0) * eff;
      const have = w.stockOf(b, inputRes);
      const coalFactor = need <= 0 ? 1 : Math.min(1, have / need);
      out.push({ k: 'coalFactor', id: b.id, coalFactor });
      powerProduced += def.powerOutput * eff * coalFactor;
    }
    if (def.heatOutput) {
      // throttle to remaining demand; fuel burn scales with actual output
      const capacity = def.heatOutput * eff;
      const throttle = capacity > 0 ? Math.min(1, heatToServe / capacity) : 0;
      const inputRes = def.inputs ? (Object.keys(def.inputs)[0] as ResourceId) : 'coal';
      const need = (def.inputs?.[inputRes] ?? 0) * eff * throttle;
      const have = w.stockOf(b, inputRes);
      const fuel = need <= 0 ? 1 : Math.min(1, have / need);
      const coalFactor = throttle * fuel;
      out.push({ k: 'coalFactor', id: b.id, coalFactor });
      const made = capacity * coalFactor;
      heatProduced += made;
      heatToServe = Math.max(0, heatToServe - made);
    }
  }

  let powerDemand = 0;
  for (const b of w.buildings.values()) {
    if (b.constructed) powerDemand += w.def(b).power;
  }
  out.push({ k: 'grid', powerProduced, powerDemand, heatProduced, heatDemand });

  // Brownout order. Three layers, coarsest first, and only the middle one is
  // the engine's opinion:
  //   1. the player's per-building `priorityHigh` flag (same flag that jumps a
  //      building to the front of the labour queue — one override, both scarce
  //      things),
  //   2. the player's sector order — who the republic keeps lit is a political
  //      decision, not a simulation detail,
  //   3. the authored `allocationPriority` inside a sector (a boiler before a
  //      steel mill; a house before a block).
  const sector = new Map(w.powerSectorOrder.map((c, i) => [c, i]));
  const ordered = [...w.buildings.values()]
    .filter(b => b.constructed && w.def(b).power > 0)
    .sort((a, b2) => {
      const hi = Number(b2.priorityHigh ?? false) - Number(a.priorityHigh ?? false);
      if (hi !== 0) return hi;
      const sa = sector.get(w.def(a).category) ?? 99;
      const sb = sector.get(w.def(b2).category) ?? 99;
      if (sa !== sb) return sa - sb;
      return w.allocationRank(a) - w.allocationRank(b2);
    });
  let budget = powerProduced;
  for (const b of ordered) {
    const need = w.def(b).power;
    const powered = budget >= need;
    if (powered) budget -= need;
    out.push({ k: 'powered', id: b.id, powered });
  }
  // Buildings that draw nothing are always lit. An unfinished building that
  // WOULD draw power is in neither set and keeps whatever it had — the site has
  // no grid connection to lose.
  for (const b of w.buildings.values()) {
    if (w.def(b).power === 0) out.push({ k: 'powered', id: b.id, powered: true });
  }

  // heat allocation
  const required = w.heatingRequired();
  if (!required) {
    for (const b of w.buildings.values()) {
      if (b.constructed && w.def(b).heat > 0) out.push({ k: 'heated', id: b.id, heated: true });
    }
  } else {
    let hb = heatProduced;
    for (const b of w.buildings.values()) {
      const def = w.def(b);
      if (!b.constructed || def.heat === 0) continue;
      const need = def.heat * heatFactor;
      const heated = hb >= need - 1e-9;
      if (heated) hb -= need;
      out.push({ k: 'heated', id: b.id, heated });
    }
  }
  return out;
}
