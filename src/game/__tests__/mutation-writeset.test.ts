// The enforcement that makes the systems refactor stick rather than decay.
//
// Every system declares the mutation kinds it may emit. This runs the pinned
// campaign with the mutation journal on and checks that nothing any system
// applies falls outside its own declaration. A new mechanic that quietly
// reaches for another subsystem's state now fails the build instead of landing
// silently — which is the whole point of naming the mutations in the first
// place.
import { describe, expect, it } from 'vitest';
import { setMutationJournal } from '../mutation';
import type { MutationKind } from '../mutation';
import { runCampaign } from './campaign';
import { makeEngine, placeBuilt } from './helpers';

import * as boats from '../systems/boats';
import * as citizens from '../systems/citizens';
import * as connectivity from '../systems/connectivity';
import * as construction from '../systems/construction';
import * as contracts from '../systems/contracts';
import * as fleet from '../systems/fleet';
import * as foreignTrade from '../systems/foreign-trade';
import * as loans from '../systems/loans';
import * as logistics from '../systems/logistics';
import * as objectives from '../systems/objectives';
import * as powerHeat from '../systems/power-heat';
import * as production from '../systems/production';
import * as totals from '../systems/totals';
import * as weather from '../systems/weather';
import * as workers from '../systems/workers';

/** Keyed by the function name the day loop runs it under. */
const DECLARED: Record<string, MutationKind[]> = {
  weather: weather.WRITES,
  connectivity: connectivity.WRITES,
  workers: workers.WRITES,
  powerHeat: powerHeat.WRITES,
  production: production.WRITES,
  foreignTrade: foreignTrade.WRITES,
  contracts: contracts.WRITES,
  loans: loans.WRITES,
  syncFleet: fleet.WRITES,
  refuelVehicles: fleet.WRITES,
  logistics: logistics.WRITES,
  boats: boats.WRITES,
  construction: construction.WRITES,
  citizens: citizens.WRITES,
  totals: totals.WRITES,
  objectives: objectives.WRITES,
};

/**
 * The pinned campaign never borrows, so it never exercises the loan sweep.
 * Rather than exempt `loans` from the guard, drive a scenario that does: take a
 * loan, sweep the surplus into repaying it, and let the closed loan age out.
 */
function runLoanScenario(): void {
  const e = makeEngine();
  placeBuilt(e, 'depot', 4, 4);
  e.rubles = 200_000;
  e.takeLoan('east', 0);
  e.setLoanAutoRepay(true);
  e.setLoanAutoRepayThreshold('east', 0);
  for (let d = 0; d < 200; d++) e.advance(e.TICK_MS);
}

describe('mutation write-sets', () => {
  it('no system applies a mutation kind it did not declare', () => {
    // Journalled over the pinned campaign so the check sees the states that
    // only arise late — a repaid loan, a frozen river, a worn machine, a
    // customs house at its storage cap.
    const seen = new Map<string, Set<MutationKind>>();
    setMutationJournal((m, system) => {
      let set = seen.get(system);
      if (!set) seen.set(system, set = new Set());
      set.add(m.k);
    });
    try {
      runCampaign(1080);
      runLoanScenario();
    } finally {
      setMutationJournal(null);
    }

    const offenders: string[] = [];
    for (const [system, kinds] of seen) {
      const declared = DECLARED[system];
      if (!declared) {
        // Player actions (place, sell, bulldoze) apply outside any system and
        // are attributed to '(engine)'. Anything else is a system the day loop
        // runs that this test does not know about — declare it.
        if (system !== '(engine)') offenders.push(`undeclared system '${system}'`);
        continue;
      }
      for (const k of kinds) {
        if (!declared.includes(k)) offenders.push(`${system} applied undeclared '${k}'`);
      }
    }
    expect(offenders, offenders.join('\n')).toEqual([]);
  });

  it('every system in the day loop is actually observed', () => {
    const seen = new Set<string>();
    setMutationJournal((_m, system) => { seen.add(system); });
    try {
      runCampaign(1080);
      runLoanScenario();
    } finally {
      setMutationJournal(null);
    }
    // A system that never applies anything over three campaign years is either
    // dead or silently broken; either way the guard above proves nothing about
    // it, so fail loudly rather than pass vacuously.
    const silent = Object.keys(DECLARED).filter(s => !seen.has(s));
    expect(silent, `never applied anything: ${silent.join(', ')}`).toEqual([]);
  });
});
