import { describe, expect, it } from 'vitest';
import { fmtQty } from '../format';
import { layRoad, makeEngine, placeBuilt, runDays } from './helpers';

/**
 * Regression: a contract-failure toast once read
 *   "Contract failed: 42.1423453564675456 coal undelivered"
 * The economy tracks stock as a continuous float (by design), but nothing that
 * crosses the border — or reaches the player — may show a fractional tail. The
 * fix floors manual sales to whole units (like buy()/auto-trade already do) so
 * `contract.delivered` stays integer, and funnels quantity displays through
 * fmtQty(). These tests lock both halves.
 */

/** The game opens in March (month index 2); the first even-index rollover is month 5, day 60. */
const FIRST_OFFER_DAYS = 60;

/** A customs town wired for a multi-month contract run (mirrors contracts.test.ts). */
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
  e.stats.produced.steel = 10; // pins the offer stream to steel
  return e;
}

describe('fmtQty', () => {
  it('renders whole usable units, never a fractional tail', () => {
    expect(fmtQty(42.6)).toBe('42'); // floors — does not round up
    expect(fmtQty(300 / 7)).toBe('42'); // 42.857…, an unclean float renders clean
    expect(fmtQty(200)).toBe('200');
    expect(fmtQty(0)).toBe('0');
    expect(fmtQty(-1e-9)).toBe('0'); // a hair below zero must not render "-1"
  });
});

describe('the border trades in whole units', () => {
  it('sell() ships only whole units and leaves the sub-unit remainder home', () => {
    const e = makeEngine();
    layRoad(e, 4, 9, 30, 9);
    placeBuilt(e, 'customs', 10, 10);
    const wh = placeBuilt(e, 'depot', 15, 10);
    const ugly = 300 / 7; // 42.857142857…, the kind of unclean float a real production tick leaves
    wh.stock.steel = ugly;

    const res = e.sell('steel', 200, 'west');
    expect(res.ok).toBe(true);
    expect(res.msg).toMatch(/^Sold 42 /); // whole units, no decimal
    // 42 units cross the border; the 0.857… remainder stays in inventory (not drained, not invented)
    expect(wh.stock.steel).toBeCloseTo(ugly - 42, 9);
  });
});

describe('contract delivery stays integer', () => {
  it('a short manual sale keeps delivered integer and the failure toast whole-numbered', () => {
    const e = contractTown();
    runDays(e, FIRST_OFFER_DAYS);
    const c = e.contracts[0];
    e.acceptContract(c.id);
    expect(c.state).toBe('active');

    // Deliver a fractional amount short of the order, then miss the deadline.
    const wh = placeBuilt(e, 'depot', 15, 10);
    wh.stock.steel = c.amount - 1 + 1 / 3; // ugly float, one-plus units short of the order
    e.sell('steel', c.amount, c.bloc);

    expect(Number.isInteger(c.delivered)).toBe(true);
    expect(c.delivered).toBe(c.amount - 1); // floor(amount − 0.666…)
    expect(wh.stock.steel).toBeGreaterThan(0); // sub-unit remainder conserved, not shipped
    expect(wh.stock.steel).toBeLessThan(1);

    while (e.contractDaysLeft(c) > 0) runDays(e, 1);
    runDays(e, 1); // the deadline passes
    expect(c.state).toBe('failed');

    const failure = e.drainEvents().find(ev => ev.text.startsWith('Contract failed:'));
    expect(failure).toBeDefined();
    // Pre-fix this read "…0.6666666666666667 steel undelivered"; the quantity must be a whole number.
    expect(failure!.text).toMatch(/^Contract failed: \d+ .+ undelivered/);
  });
});
