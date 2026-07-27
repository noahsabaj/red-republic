// Daily contract sweep: withdraw stale offers, fail passed deadlines, heal
// relations. Accepting and delivering happen elsewhere (player action and the
// export path); this is only the clock running out on them.
import { CONTRACTS, RESOURCES } from '../config';
import { fmtMoney, fmtOwed } from '../format';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['contractDrop', 'contractState', 'treasury', 'relations', 'event'];

export function contracts(w: World): Mutation[] {
  const s = new Staged(w);
  const idx = w.dayIndex();
  for (let i = w.contracts.length - 1; i >= 0; i--) {
    const c = w.contracts[i];
    if (c.state === 'offer' && idx > c.offerExpiresIdx) {
      s.emit(
        { k: 'contractDrop', id: c.id },
        { k: 'event', text: `The ${c.bloc === 'east' ? 'East' : 'West'} withdrew its ${RESOURCES[c.r].name} offer.`, kind: 'info', icon: 'contract' },
      );
      continue;
    }
    if (c.state === 'active' && idx > c.deadlineIdx) {
      const fine = CONTRACTS.finePct * (c.amount - c.delivered) * c.pricePerUnit;
      const cur = c.bloc === 'east' ? '₽' : '$';
      s.emit(
        { k: 'contractState', id: c.id, state: 'failed', closedIdx: idx },
        // the fine can never overdraw: an empty treasury simply pays nothing
        { k: 'treasury', bloc: c.bloc, delta: -Math.min(fine, c.bloc === 'east' ? w.rubles : w.dollars) },
        { k: 'relations', bloc: c.bloc, penalty: Math.min(CONTRACTS.relationsCap, w.relationsPenalty[c.bloc] + CONTRACTS.relationsHit) },
        {
          k: 'event',
          text: `Contract failed: ${fmtOwed(c.amount - c.delivered)} ${RESOURCES[c.r].name} undelivered. Fined ${cur}${fmtMoney(fine)}; the ${c.bloc === 'east' ? 'East' : 'West'} sours on us.`,
          kind: 'bad', icon: 'contract',
        },
      );
      continue;
    }
    // prune old history so the panel stays readable
    if ((c.state === 'done' || c.state === 'failed') && c.closedIdx !== undefined && idx - c.closedIdx > 60) {
      s.emit({ k: 'contractDrop', id: c.id });
    }
  }
  s.emit(
    { k: 'relations', bloc: 'east', penalty: Math.max(0, w.relationsPenalty.east - CONTRACTS.relationsDecayPerDay) },
    { k: 'relations', bloc: 'west', penalty: Math.max(0, w.relationsPenalty.west - CONTRACTS.relationsDecayPerDay) },
  );
  return s.muts;
}
