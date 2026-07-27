// Moscow's five-year plan: the standing goals, checked against the sim every
// day and paid out at the border when met.
//
// Staged rather than batched, because the reward changes the treasury a later
// objective in the same sweep may be measured against — and because loans call
// this mid-repayment.
import { OBJECTIVES } from '../config';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['objectiveDone', 'treasury', 'event'];

export function objectives(w: World): Mutation[] {
  const s = new Staged(w);
  for (const o of OBJECTIVES) {
    if (w.objectivesDone.includes(o.id)) continue;
    let done = false;
    switch (o.id) {
      case 'roads': done = w.stats.roadsBuilt >= 10; break;
      case 'housing': done = w.pop >= 20; break;
      case 'shop': done = [...w.buildings.values()].some(b => w.def(b).serviceType === 'shop' && b.constructed && w.stockOf(b, 'food') >= 5); break;
      case 'sow': done = [...w.buildings.values()].some(b => w.def(b).isFarm && b.constructed); break;
      case 'builders': done = w.stats.produced.planks >= 20 && w.stats.produced.bricks >= 20; break;
      case 'firstMachines': done = (w.stats.imported.machinery ?? 0) >= 5; break;
      case 'meansOfProduction': done = [...w.buildings.values()].some(b => b.defId === 'machineWorks' && b.constructed); break;
      case 'autarky': done = w.stats.produced.machinery >= 50; break;
      case 'coal': done = w.stats.produced.coal >= 30; break;
      // must match the threshold OBJECTIVES advertises — display and
      // simulation disagreeing is the one thing the UI rule forbids
      case 'power': done = w.powerProduced >= 50; break;
      case 'heat': done = [...w.buildings.values()].some(b => w.def(b).heatOutput && b.constructed && b.staff > 0); break;
      case 'steel': done = w.stats.produced.steel >= 15; break;
      case 'foodchain': done = w.stats.produced.food >= 25; break;
      case 'export': done = w.stats.exportedValue >= 5000; break;
      case 'debtFree': done = w.loans.some(l => l.state === 'repaid'); break;
      case 'pop150': done = w.pop >= 150; break;
      case 'flourish': done = w.pop >= 300 && w.happiness >= 65; break;
    }
    if (!done) continue;
    s.emit({ k: 'objectiveDone', id: o.id });
    if (o.rewardRubles) s.emit({ k: 'treasury', bloc: 'east', delta: o.rewardRubles });
    if (o.rewardDollars) s.emit({ k: 'treasury', bloc: 'west', delta: o.rewardDollars });
    const rw = [o.rewardRubles ? `+₽${o.rewardRubles.toLocaleString()}` : '', o.rewardDollars ? `+$${o.rewardDollars.toLocaleString()}` : ''].filter(Boolean).join(' ');
    s.emit({ k: 'event', text: `Objective complete: ${o.title}! ${rw}`, kind: 'good', icon: 'star' });
  }
  return s.muts;
}
