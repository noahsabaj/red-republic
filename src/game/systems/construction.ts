// Building the republic. Two-phase, domestic-first labour spread by MAX-MIN
// FAIR-SHARE across every ready site, segmented by construction priority:
// builders fill the highest priority tier first (sharing evenly within a tier),
// spilling to the next tier only once the top is fully crewed. Phase 1 spends
// citizens' FREE labour; phase 2 tops up with PAID foreign builders — only
// sites whose per-site policy permits them, and only as far as the treasury can
// afford.
//
// Total builder-days applied per day is CONSERVED (same as the old greedy
// pool); only the distribution across sites changed. A single ready site
// collapses to min(cap, pool).
import { BALANCE, DIFFICULTIES, WEATHER } from '../config';
import { Staged } from '../mutation';
import type { Mutation } from '../mutation';
import type { BuildingInst, World } from '../world';

export function construction(w: World): Mutation[] {
  const s = new Staged(w);
  if (!w.globalConstructionEnabled) return s.muts;

  const domesticPool = w.domesticBuilderPool();
  const foreignPool = Math.max(0, w.builderPool() - domesticPool);
  const isEast = (w.foreignLaborCurrency ?? 'east') === 'east';
  const rateBase = isEast ? BALANCE.foreignLaborPerDayEast : BALANCE.foreignLaborPerDayWest;
  const perDay = rateBase * DIFFICULTIES[w.difficulty].importPriceMult;
  const treasury = isEast ? w.rubles : w.dollars;
  const affordableForeign = perDay > 0 ? Math.floor(treasury / perDay) : foreignPool;
  const domestic = domesticPool;
  const foreign = w.foreignLaborEnabled ? Math.min(foreignPool, affordableForeign) : 0;
  if (domestic + foreign <= 0) return s.muts;
  const buildMult = WEATHER[w.weather.condition].buildMult;

  // Snapshot ready sites into an ARRAY (id order) — completing a road site
  // deletes it from the live Map mid-apply, so we must not iterate that.
  const ready = [...w.buildings.values()].filter(
    b => !b.constructed && !b.paused && w.siteReady(b));
  if (!ready.length) return s.muts;
  // exact fractional remaining builder-days, capped at the per-site slot —
  // a near-done or 3-labor road site takes only its true need and releases
  // the surplus back to the pool (no ceil() rounding to hoard a full slot).
  const cap = ready.map(b => Math.min(BALANCE.buildersPerSite, (w.def(b).labor - b.progress) / Math.max(1e-4, buildMult)));
  const tierOf = (b: BuildingInst) => b.buildPriority ?? 0;
  const tiers = [...new Set(ready.map(tierOf))].sort((x, y) => y - x); // high → low

  const domCrew = new Array<number>(ready.length).fill(0);
  const forCrew = new Array<number>(ready.length).fill(0);

  // Phase 1 — free domestic labor, tier by tier (strict: top tier first).
  let domLeft = domestic;
  for (const tier of tiers) {
    if (domLeft <= 1e-9) break;
    const idx = ready.map((_, i) => i).filter(i => tierOf(ready[i]) === tier);
    const alloc = waterFill(idx.map(i => cap[i]), domLeft);
    for (let k = 0; k < idx.length; k++) { domCrew[idx[k]] = alloc[k]; domLeft -= alloc[k]; }
  }

  // Phase 2 — paid foreign residual, same tier order, only where policy allows.
  let forLeft = foreign;
  for (const tier of tiers) {
    if (forLeft <= 1e-9) break;
    const idx = ready.map((_, i) => i).filter(i => tierOf(ready[i]) === tier && ready[i].foreignLabor !== false);
    const alloc = waterFill(idx.map(i => Math.max(0, cap[i] - domCrew[i])), forLeft);
    for (let k = 0; k < idx.length; k++) { forCrew[idx[k]] = alloc[k]; forLeft -= alloc[k]; }
  }

  // Apply each site's total crew once, then pay for the foreign builder-days.
  let foreignUsed = 0;
  for (let i = 0; i < ready.length; i++) {
    foreignUsed += forCrew[i];
    const crew = domCrew[i] + forCrew[i];
    if (crew <= 0) continue;
    s.emit({ k: 'siteProgress', id: ready[i].id, days: crew * buildMult }); // storms slow the site
    if (ready[i].progress >= w.def(ready[i]).labor) s.emit({ k: 'siteComplete', id: ready[i].id });
  }

  // foreignUsed <= foreign <= affordableForeign (= floor(treasury/perDay)), so
  // cost <= treasury — the treasury never goes negative. The min() clamp defends
  // against a summed-fractional overshoot of at most one ULP.
  if (foreignUsed > 0) {
    foreignUsed = Math.min(foreignUsed, affordableForeign);
    s.emit({ k: 'foreignLaborBill', bloc: isEast ? 'east' : 'west', cost: foreignUsed * perDay });
  }
  return s.muts;
}

/** Max-min fair-share (water-filling): split `budget` across sites with the
 *  given per-site caps, filling the smallest caps first so any surplus from a
 *  nearly-done site redistributes evenly to the rest. Returns alloc[] with
 *  Sum(alloc) = min(budget, Sum(cap)). Pure + deterministic (stable index order). */
function waterFill(caps: number[], budget: number): number[] {
  const alloc = new Array<number>(caps.length).fill(0);
  const order = caps.map((_, i) => i).sort((x, y) => caps[x] - caps[y] || x - y);
  let rem = budget;
  let k = caps.length;
  for (let p = 0; p < order.length; p++) {
    if (k <= 0 || rem <= 1e-9) break;
    const i = order[p];
    const share = rem / k;
    if (caps[i] <= share) { alloc[i] = caps[i]; rem -= caps[i]; k--; } // saturates below the line
    else { for (let q = p; q < order.length; q++) alloc[order[q]] = rem / k; rem = 0; break; } // equal split among the rest
  }
  return alloc;
}
