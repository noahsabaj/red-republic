import { describe, expect, it } from 'vitest';
import { BALANCE } from '../config';
import type { GameEngine } from '../engine';
import { layRoad, makeEngine, placeBuilt, runDays, totalOf } from './helpers';

/**
 * A staffable customs town: road row, customs house, beds for a stable
 * workforce. Customs is unpowered (no plant), so eff = staffRatio * 0.5.
 */
function autoTown(pop = 40) {
  const e = makeEngine();
  layRoad(e, 4, 9, 30, 9);
  const customs = placeBuilt(e, 'customs', 10, 10);
  placeBuilt(e, 'apartment', 20, 10); // 40 beds; pop == capacity → no migration drift
  e.pop = pop;
  e.rubles = 50000;
  e.setAutoTradeEnabled(true);
  return { e, customs };
}

const coalImportPrice = 2.5 * 1.6; // priceEast * IMPORT_MARKUP, factor 1 in the first month

describe('auto-import', () => {
  it('fills the town to the rule level, throughput-limited, then stops', () => {
    const { e, customs } = autoTown();
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    const perDay: number[] = [];
    for (let i = 0; i < 6; i++) {
      runDays(e, 1);
      perDay.push(e.tradeLedger.today.imports.coal ?? 0);
    }
    // eff 0.5 → 15/day: 15, 15, 10, then satisfied
    expect(customs.eff).toBeCloseTo(0.5, 9);
    expect(perDay).toEqual([15, 15, 10, 0, 0, 0]);
    expect(totalOf(e, 'coal')).toBe(40);
  });

  it('spends exactly to the reserve floor, never past it', () => {
    const { e } = autoTown();
    runDays(e, 1); // settle day-one objective rewards before pinning the treasury
    e.rubles = e.autoTrade.reserveRubles + coalImportPrice; // exactly one unit of headroom
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    runDays(e, 1);
    expect(e.tradeLedger.today.imports.coal).toBe(1);
    runDays(e, 1); // wages already pushed the treasury below the floor
    expect(e.tradeLedger.today.imports.coal).toBeUndefined();
    expect(e.tradeLedger.today.blocked).toContain('treasury at reserve floor');
    expect(e.alerts.some(a => a.id === 'autotrade')).toBe(true);
  });

  it('a treasury below one unit of headroom buys nothing', () => {
    const { e } = autoTown();
    runDays(e, 1); // settle day-one objective rewards before pinning the treasury
    e.rubles = e.autoTrade.reserveRubles + coalImportPrice - 0.01;
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    runDays(e, 1);
    expect(e.tradeLedger.today.imports.coal).toBeUndefined();
    expect(e.tradeLedger.today.blocked).toContain('treasury at reserve floor');
  });

  it('is clamped by customs storage space and reports the blockage', () => {
    const { e, customs } = autoTown();
    customs.stock.coal = 78; // free space 2 of the 80 cap
    e.setAutoTradeRule('coal', { mode: 'import', level: 200, currency: 'east' });
    runDays(e, 1);
    expect(e.tradeLedger.today.imports.coal).toBe(2);
    runDays(e, 1);
    expect(e.tradeLedger.today.imports.coal).toBeUndefined();
    expect(e.tradeLedger.today.blocked).toContain('customs storage full');
  });

  it('daily tonnage scales with customs staffing', () => {
    const full = autoTown(40); // staff 8/8
    const half = autoTown(6);  // workers 4 → staff 4/8
    for (const { e } of [full, half]) {
      e.setAutoTradeRule('coal', { mode: 'import', level: 500, currency: 'east' });
      runDays(e, 1);
    }
    const got = (t: { e: GameEngine }) => t.e.tradeLedger.today.imports.coal ?? 0;
    expect(got(full)).toBe(Math.floor(BALANCE.customsThroughputPerDay * full.customs.eff));
    expect(got(half)).toBe(Math.floor(BALANCE.customsThroughputPerDay * half.customs.eff));
    expect(got(half)).toBeLessThan(got(full));
    expect(got(half)).toBeGreaterThan(0);
  });

  it('an unstaffed customs house stalls automation with an alert', () => {
    const { e } = autoTown(0);
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    runDays(e, 1);
    expect(e.tradeLedger.today.capacity).toBe(0);
    expect(e.tradeLedger.today.imports.coal).toBeUndefined();
    expect(e.tradeLedger.today.blocked).toContain('customs house unstaffed');
    expect(e.alerts.some(a => a.id === 'autotrade')).toBe(true);
  });

  it('does nothing while the master switch is off', () => {
    const { e } = autoTown();
    e.setAutoTradeEnabled(false);
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    runDays(e, 2);
    expect(totalOf(e, 'coal')).toBe(0);
    expect(e.tradeLedger.today.capacity).toBeGreaterThan(0); // capacity still on the page
  });
});

describe('auto-export', () => {
  function exportTown() {
    const t = autoTown();
    placeBuilt(t.e, 'constructionOffice', 25, 10); // trucks for staging
    const wh = placeBuilt(t.e, 'depot', 15, 10);   // 120-cap storage holds the 60 within limits
    wh.stock.steel = 60;
    t.e.setAutoTradeRule('steel', { mode: 'export', level: 20, currency: 'east' });
    return { ...t, wh };
  }

  it('stages surplus to the border by truck, sells from customs stock, keeps the level inland', () => {
    const { e, customs, wh } = exportTown();
    let sold = 0;
    let earned = 0;
    for (let i = 0; i < 8; i++) {
      runDays(e, 1);
      sold += e.tradeLedger.today.exports.steel ?? 0;
      earned += e.tradeLedger.today.rubles;
      // the keep-level is never violated inland
      expect(wh.stock.steel).toBeGreaterThanOrEqual(20 - 1e-9);
    }
    expect(sold).toBe(40); // 60 minus the keep-level of 20
    expect(earned).toBeCloseTo(40 * 14, 9); // steel priceEast, factor 1
    expect(wh.stock.steel).toBe(20);
    expect(e.stockOf(customs, 'steel')).toBe(0); // everything staged was shipped
    expect(e.stats.exportedValue).toBeCloseTo(40 * 14, 9);
  });

  it('sells only what physically reached the border (nothing on day one)', () => {
    const { e } = exportTown();
    runDays(e, 1); // trucks dispatched, still rolling
    expect(e.tradeLedger.today.exports.steel ?? 0).toBe(0);
  });

  it('staging never drains the customs house back inland', () => {
    const { e, customs } = exportTown();
    customs.stock.steel = 30; // already border-side
    runDays(e, 1);
    // surplus is measured inland only: warehouse 60 - keep 20 = 40 staged;
    // the 30 at customs stay for sale, no truck re-hauls them
    for (const tr of e.trucks) {
      expect(tr.srcId).not.toBe(customs.id);
    }
  });
});

describe('auto-trade policy', () => {
  it('one rule per resource: import and export replace each other', () => {
    const { e } = autoTown();
    e.setAutoTradeRule('coal', { mode: 'import', level: 40, currency: 'east' });
    e.setAutoTradeRule('coal', { mode: 'export', level: 10, currency: 'west' });
    expect(e.autoTrade.rules.coal).toEqual({ mode: 'export', level: 10, currency: 'west' });
    e.setAutoTradeRule('coal', null);
    expect(e.autoTrade.rules.coal).toBeUndefined();
  });

  it('is deterministic: same seed + same policy → identical ledgers and treasury', () => {
    const run = () => {
      const { e } = autoTown();
      placeBuilt(e, 'constructionOffice', 25, 10);
      const wh = placeBuilt(e, 'depot', 15, 10);
      wh.stock.steel = 60;
      e.setAutoTradeRule('steel', { mode: 'export', level: 20, currency: 'east' });
      e.setAutoTradeRule('coal', { mode: 'import', level: 30, currency: 'east' });
      runDays(e, 10);
      return e;
    };
    const a = run();
    const b = run();
    expect(a.rubles).toBe(b.rubles);
    expect(JSON.stringify(a.tradeLedger)).toBe(JSON.stringify(b.tradeLedger));
    expect(totalOf(a, 'coal')).toBe(totalOf(b, 'coal'));
  });

  it('ledger pages roll daily: yesterday is yesterday', () => {
    const { e } = autoTown();
    e.setAutoTradeRule('coal', { mode: 'import', level: 15, currency: 'east' });
    runDays(e, 1);
    const day1 = e.tradeLedger.today;
    runDays(e, 1);
    expect(e.tradeLedger.yesterday).toBe(day1);
  });
});
