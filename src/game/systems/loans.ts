// Foreign credit: enforce deadlines, sweep surplus into repayments, retire old
// history. Borrowing itself is a player action.
import { CONTRACTS, LOANS } from '../config';
import { fmtMoney } from '../format';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';
import { objectives } from './objectives';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['loanState', 'loanCooldown', 'loanRepaid', 'loanDrop', 'treasury', 'relations', 'objectiveDone', 'event'];

export function loans(w: World): Mutation[] {
  const s = new Staged(w);
  const idx = w.dayIndex();
  for (const loan of w.loans) {
    if (loan.state !== 'active') continue;
    if (idx > loan.deadlineDayIdx) {
      const cur = loan.bloc === 'east' ? '₽' : '$';
      const remaining = loan.totalOwed - loan.repaid;
      s.emit(
        { k: 'loanState', id: loan.id, state: 'defaulted' },
        { k: 'loanCooldown', bloc: loan.bloc, untilDayIdx: idx + LOANS.defaultCooldownDays },
        {
          k: 'relations', bloc: loan.bloc,
          penalty: Math.min(CONTRACTS.relationsCap + LOANS.defaultRelationsHit,
            w.relationsPenalty[loan.bloc] + LOANS.defaultRelationsHit),
        },
        {
          k: 'event',
          text: `Defaulted on ${loan.bloc === 'east' ? 'East' : 'West'} loan — ${cur}${fmtMoney(remaining)} unpaid. Relations damaged; credit frozen for ${LOANS.defaultCooldownDays} days.`,
          kind: 'bad', icon: 'coins',
        },
      );
    }
  }

  // Auto-repay: anything above the player's floor goes to the creditor.
  if (w.loanAutoRepay.enabled) {
    for (const bloc of ['east', 'west'] as const) {
      const loan = w.loans.find(l => l.bloc === bloc && l.state === 'active');
      if (!loan) continue;
      const funds = bloc === 'east' ? w.rubles : w.dollars;
      const threshold = bloc === 'east' ? w.loanAutoRepay.thresholdRubles : w.loanAutoRepay.thresholdDollars;
      const surplus = funds - threshold;
      if (surplus <= 0) continue;
      const payment = Math.min(surplus, loan.totalOwed - loan.repaid);
      if (payment <= 0) continue;
      s.emit(
        { k: 'treasury', bloc, delta: -payment },
        { k: 'loanRepaid', id: loan.id, amount: payment },
      );
      if (loan.repaid >= loan.totalOwed) {
        s.emit({ k: 'loanState', id: loan.id, state: 'repaid' });
        // Checked here, mid-sweep, because clearing a debt is itself an
        // objective and the reward is part of what the WEST bloc's pass then
        // sees in the treasury.
        s.record(objectives(w));
        s.emit({ k: 'event', text: `${bloc === 'east' ? 'East' : 'West'} loan auto-repaid in full!`, kind: 'good', icon: 'coins' });
      }
    }
  }

  // Prune old closed loans (keep for 90 days for UI history)
  for (let i = w.loans.length - 1; i >= 0; i--) {
    const l = w.loans[i];
    if ((l.state === 'repaid' || l.state === 'defaulted') && idx - l.deadlineDayIdx > 90) {
      s.emit({ k: 'loanDrop', id: l.id });
    }
  }
  return s.muts;
}
