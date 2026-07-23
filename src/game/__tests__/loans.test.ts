import { describe, expect, it } from 'vitest';
import { LOANS } from '../config';
import type { SaveGameV1 } from '../save-format';
import { GameEngine } from '../engine';
import { makeEngine, runDays } from './helpers';

describe('loan system', () => {
  it('allows taking a loan, crediting treasury and recording active loan', () => {
    const e = makeEngine();
    const initialRubles = e.rubles;
    const res = e.takeLoan('east', 0); // Small tier (500 ₽)
    expect(res.ok).toBe(true);
    expect(e.rubles).toBe(initialRubles + LOANS.tiersEast[0]);

    const active = e.activeLoan('east');
    expect(active).toBeDefined();
    expect(active?.principal).toBe(LOANS.tiersEast[0]);
    expect(active?.totalOwed).toBe(Math.round(LOANS.tiersEast[0] * (1 + LOANS.interestEast)));
    expect(active?.repaid).toBe(0);
    expect(active?.state).toBe('active');
  });

  it('enforces limit of max 1 active loan per bloc', () => {
    const e = makeEngine();
    e.takeLoan('east', 0);
    const second = e.canTakeLoan('east', 1);
    expect(second.ok).toBe(false);
    expect(second.reason).toContain('Already have an active East loan');

    // Western loan from other bloc is still allowed
    const westRes = e.canTakeLoan('west', 0);
    expect(westRes.ok).toBe(true);
  });

  it('supports partial and full repayment', () => {
    const e = makeEngine();
    e.takeLoan('east', 0);
    const loan = e.activeLoan('east')!;
    const totalOwed = loan.totalOwed;

    // Partial repayment
    const partRes = e.repayLoan('east', 100);
    expect(partRes.ok).toBe(true);
    expect(loan.repaid).toBe(100);
    expect(loan.state).toBe('active');

    // Full repayment
    const remaining = totalOwed - 100;
    const fullRes = e.repayLoan('east', remaining);
    expect(fullRes.ok).toBe(true);
    expect(loan.repaid).toBe(totalOwed);
    expect(loan.state).toBe('repaid');
    expect(e.activeLoan('east')).toBeUndefined();
  });

  it('completes the debtFree objective when a loan is fully repaid', () => {
    const e = makeEngine();
    expect(e.objectivesDone).not.toContain('debtFree');

    e.takeLoan('east', 0);
    const loan = e.activeLoan('east')!;
    e.repayLoan('east', loan.totalOwed);

    expect(e.objectivesDone).toContain('debtFree');
  });

  it('marks loan as defaulted when deadline passes and applies penalty/cooldown', () => {
    const e = makeEngine();
    e.takeLoan('east', 0); // 90 days deadline
    const initialPenalty = e.relationsPenalty.east;

    runDays(e, 91);

    const defaultedLoan = e.loans.find(l => l.bloc === 'east');
    expect(defaultedLoan?.state).toBe('defaulted');
    expect(e.relationsPenalty.east).toBeGreaterThan(initialPenalty);

    // Borrowing from East is blocked by cooldown
    const newBorrow = e.canTakeLoan('east', 0);
    expect(newBorrow.ok).toBe(false);
    expect(newBorrow.reason).toContain('credit frozen');
  });

  it('processes auto-repayment when enabled and treasury exceeds threshold', () => {
    const e = makeEngine();
    e.takeLoan('east', 0);
    const loan = e.activeLoan('east')!;
    e.rubles = 3000; // Above default threshold of 2000

    e.setLoanAutoRepay(true);
    runDays(e, 1);

    expect(loan.repaid).toBe(loan.totalOwed);
    expect(loan.state).toBe('repaid');
  });

  it('persists and hydrates loan state in save games', () => {
    const e = makeEngine();
    e.takeLoan('east', 1); // Medium tier
    e.repayLoan('east', 300);
    e.setLoanAutoRepay(true);

    const saveBlob: SaveGameV1 = e.serialize();
    const loaded = GameEngine.fromSave(saveBlob);

    expect(loaded.loans).toHaveLength(1);
    const loadedLoan = loaded.loans[0];
    expect(loadedLoan.bloc).toBe('east');
    expect(loadedLoan.tierIndex).toBe(1);
    expect(loadedLoan.repaid).toBe(300);
    expect(loaded.loanAutoRepay.enabled).toBe(true);
  });
});
