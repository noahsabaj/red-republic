// Standing orders of the Foreign Trade Directorate. Runs before logistics
// (imports land in customs stock in time for today's lorries) and before
// citizens (the reserve floor keeps consumption safe from automation). Each
// customs house clears a limited daily tonnage scaled by its staffing — exports
// sell from its own stock, which the lorries stage there; imports arrive into
// it. Manual panel trades stay instant and bypass this entirely.
//
// Staged: the budget, the treasury and the ledger are all read back within the
// sweep, and a customs house that spends its rubles on coal has fewer for steel.
import { ALL_RESOURCES, BALANCE } from '../config';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['ledgerRoll', 'ledgerCapacity', 'ledgerBlocked', 'exportSale', 'importPurchase'];

export function foreignTrade(w: World): Mutation[] {
  const s = new Staged(w);
  s.emit({ k: 'ledgerRoll' });
  const customsHouses = [...w.buildings.values()]
    .filter(b => w.def(b).isCustoms && b.constructed)
    .sort((a, b) => a.id - b.id);
  for (const c of customsHouses) {
    s.emit({ k: 'ledgerCapacity', amount: Math.floor(BALANCE.customsThroughputPerDay * c.eff) });
  }
  if (!w.autoTrade.enabled || !customsHouses.length) return s.muts;
  if (!ALL_RESOURCES.some(r => w.autoTrade.rules[r])) return s.muts;
  if (w.tradeLedger.today.capacity <= 0) {
    s.emit({ k: 'ledgerBlocked', why: 'customs house unstaffed' });
    return s.muts;
  }

  for (const c of customsHouses) {
    let budget = Math.floor(BALANCE.customsThroughputPerDay * c.eff);
    if (budget <= 0) continue;

    // exports first — earn before spending, straight from this customs' stock
    for (const r of ALL_RESOURCES) {
      if (budget <= 0) break;
      const rule = w.autoTrade.rules[r];
      if (rule?.mode !== 'export') continue;
      const amt = Math.min(budget, Math.floor(w.stockOf(c, r)));
      if (amt < 1) continue;
      s.emit({ k: 'exportSale', id: c.id, r, bloc: rule.currency, amount: amt });
      budget -= amt;
    }

    // imports — fill the town to each rule's level, throughput- and reserve-limited
    for (const r of ALL_RESOURCES) {
      if (budget <= 0) break;
      const rule = w.autoTrade.rules[r];
      if (rule?.mode !== 'import') continue;
      const deficit = Math.floor(rule.level - w.liveTownTotal(r));
      if (deficit < 1) continue;
      const free = Math.floor(w.capOf(c, r) - w.stockOf(c, r) - w.incomingOf(c, r));
      if (free < 1) { s.emit({ k: 'ledgerBlocked', why: 'customs storage full' }); continue; }
      const spendable = rule.currency === 'east'
        ? w.rubles - w.autoTrade.reserveRubles
        : w.dollars - w.autoTrade.reserveDollars;
      const affordable = Math.floor(spendable / w.importPriceOf(r, rule.currency));
      if (affordable < 1) { s.emit({ k: 'ledgerBlocked', why: 'treasury at reserve floor' }); continue; }
      const amt = Math.min(deficit, budget, free, affordable);
      s.emit({ k: 'importPurchase', id: c.id, r, bloc: rule.currency, amount: amt });
      budget -= amt;
    }
  }
  return s.muts;
}
