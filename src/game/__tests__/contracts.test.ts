import { describe, expect, it } from 'vitest';
import { CONTRACTS } from '../config';
import type { GameEngine } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * A customs town stable enough for multi-month contract runs: stocked store
 * keeps happiness above the exodus line, full beds stop migration drift.
 * `stats.produced.steel` pins the offer stream to steel (the blocs ask for
 * what the republic demonstrably produces).
 */
function contractTown() {
  const e = makeEngine();
  layRoad(e, 4, 9, 30, 9);
  placeBuilt(e, 'customs', 10, 10);
  placeBuilt(e, 'apartment', 20, 10);
  const store = placeBuilt(e, 'store', 18, 10);
  store.stock.food = 40;
  store.stock.clothes = 20;
  e.pop = 40;
  e.rubles = 50000;
  e.stats.produced.steel = 10;
  return e;
}

/** The game opens March 1 (month index 2); the first even-index rollover is month 5, day 60. */
const FIRST_OFFER_DAYS = 60;

describe('contract offers', () => {
  it('arrive on the every-other-month cadence, deterministically per seed', () => {
    const a = contractTown();
    const b = contractTown();
    runDays(a, FIRST_OFFER_DAYS - 1);
    expect(a.contracts).toHaveLength(0); // month-4 rollover is an odd index — no offer
    runDays(a, 1);
    runDays(b, FIRST_OFFER_DAYS);
    expect(a.contracts).toHaveLength(1);
    const ca = a.contracts[0];
    expect(ca.state).toBe('offer');
    expect(ca.r).toBe('steel'); // drawn from what the town produces
    expect(ca.amount).toBeGreaterThanOrEqual(CONTRACTS.minUnits);
    expect(ca.amount).toBeLessThanOrEqual(CONTRACTS.maxUnits);
    // value-banded: the order's market value sits inside the bloc's band
    const band = ca.bloc === 'east' ? CONTRACTS.valueBandEast : CONTRACTS.valueBandWest;
    const value = ca.amount * a.priceOf('steel', ca.bloc);
    expect(value).toBeGreaterThanOrEqual(band[0] * 0.8); // rounding slack
    expect(value).toBeLessThanOrEqual(band[1] * 1.2);
    expect(ca.pricePerUnit).toBeGreaterThan(a.priceOf('steel', ca.bloc)); // premium over market
    expect({ ...ca }).toEqual({ ...b.contracts[0] }); // stateless per-month stream
  });

  it('never arrive without a customs house', () => {
    const e = makeEngine();
    runDays(e, FIRST_OFFER_DAYS);
    expect(e.contracts).toHaveLength(0);
  });

  it('declined offers vanish; ignored offers are withdrawn after 30 days', () => {
    const e = contractTown();
    runDays(e, FIRST_OFFER_DAYS);
    e.declineContract(e.contracts[0].id);
    expect(e.contracts).toHaveLength(0);

    const e2 = contractTown();
    runDays(e2, FIRST_OFFER_DAYS);
    runDays(e2, CONTRACTS.offerDays + 2);
    expect(e2.contracts).toHaveLength(0);
  });
});

describe('contract delivery', () => {
  function withActiveContract() {
    const e = contractTown();
    runDays(e, FIRST_OFFER_DAYS);
    const c = e.contracts[0];
    e.acceptContract(c.id);
    expect(c.state).toBe('active');
    return { e, c };
  }
  const funds = (e: GameEngine, bloc: 'east' | 'west') => (bloc === 'east' ? e.rubles : e.dollars);

  it('manual sales credit the contract and pay the locked premium price', () => {
    const { e, c } = withActiveContract();
    const wh = placeBuilt(e, 'depot', 15, 10);
    wh.stock.steel = c.amount + 30;
    const before = funds(e, c.bloc);
    e.sell('steel', 10, c.bloc);
    expect(c.delivered).toBe(10);
    expect(funds(e, c.bloc) - before).toBeCloseTo(10 * c.pricePerUnit, 9);
    // finishing the contract: owed units at the locked price, the extra 5 at market
    const before2 = funds(e, c.bloc);
    e.sell('steel', c.amount - 10 + 5, c.bloc);
    expect(c.state).toBe('done');
    expect(funds(e, c.bloc) - before2)
      .toBeCloseTo((c.amount - 10) * c.pricePerUnit + 5 * e.priceOf('steel', c.bloc), 9);
  });

  it('auto-export deliveries count toward the contract too', () => {
    const { e, c } = withActiveContract();
    placeBuilt(e, 'constructionOffice', 25, 10);
    const wh = placeBuilt(e, 'depot', 15, 10);
    wh.stock.steel = c.amount;
    e.setAutoTradeEnabled(true);
    e.setAutoTradeRule('steel', { mode: 'export', level: 0, currency: c.bloc });
    runDays(e, 25);
    expect(c.delivered).toBeCloseTo(c.amount, 9);
    expect(c.state).toBe('done');
  });

  it('warns while the deadline closes in on an unfinished contract', () => {
    const { e, c } = withActiveContract();
    while (e.contractDaysLeft(c) > 14) runDays(e, 1);
    expect(e.alerts.some(a => a.id === 'contract')).toBe(true);
  });
});

describe('contract failure', () => {
  it('fines the treasury and sours relations, which then decay back to par', () => {
    const e = contractTown();
    runDays(e, FIRST_OFFER_DAYS);
    const c = e.contracts[0];
    e.acceptContract(c.id);
    while (e.contractDaysLeft(c) > 0) runDays(e, 1);
    const rublesBefore = e.rubles;
    const dollarsBefore = e.dollars;
    runDays(e, 1); // the deadline passes
    expect(c.state).toBe('failed');
    const fine = CONTRACTS.finePct * c.amount * c.pricePerUnit;
    if (c.bloc === 'west') {
      expect(dollarsBefore - e.dollars).toBeCloseTo(fine, 6);
    } else {
      expect(rublesBefore - e.rubles).toBeCloseTo(fine, 6); // the fine is the only ruble outflow
    }
    // relations penalty is live and hits both price directions
    const p = e.relationsPenalty[c.bloc];
    expect(p).toBeCloseTo(CONTRACTS.relationsHit - CONTRACTS.relationsDecayPerDay, 9);
    expect(e.importPriceOf('steel', c.bloc) / e.priceOf('steel', c.bloc))
      .toBeCloseTo(1.6 * (1 + p) / (1 - p), 9);
    // and it heals: gone after hit/decay days
    runDays(e, Math.ceil(CONTRACTS.relationsHit / CONTRACTS.relationsDecayPerDay));
    expect(e.relationsPenalty[c.bloc]).toBe(0);
    expect(e.importPriceOf('steel', c.bloc) / e.priceOf('steel', c.bloc)).toBeCloseTo(1.6, 9);
  });
});
