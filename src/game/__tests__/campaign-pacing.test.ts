import { describe, expect, it } from 'vitest';
import { runCampaign } from './campaign';

/**
 * The pinned campaign is the economy's tripwire — the mapgen snapshot's
 * counterpart for balance. A deterministic map, weather script and three-year
 * build order that a competent player could follow (campaign.ts); if a config
 * or engine change breaks the bootstrap chain, treasury solvency, the
 * electrification push, or the machinery arc, these milestones trip.
 *
 * Thresholds carry ~30-40% slack below the observed trajectory: they catch
 * structural regressions, not small tuning. If a deliberate rebalance moves
 * the curve, rerun the scratch trajectory and re-pin — don't loosen blindly.
 * Every numeric pin below carries the figure it was derived from, so the next
 * person can see the margin they are working inside.
 *
 * Re-derived when the fleet became physical: lorries are persistent machines
 * that burn their own fuel, so the plan now has to supply them — a standing
 * fuel order early, then a pump and a refinery that end the bill. That arc is
 * asserted here alongside the machinery one.
 */
describe('campaign pacing', () => {
  it('three years: bootstrap, solvency, electrification, machinery autarky', () => {
    let peakPop = 0;
    let paidForeignLabor = false;
    let electrified = false;
    const engine = runCampaign(1080, (e, day) => {
      if (e.powerProduced > 0) electrified = true; // the grid came online (deep-winter days can dip to 0)
      // ---- every-day invariants ----
      // the only domestic money sink is foreign construction labor (capped by
      // affordability), so the treasury bends but never goes negative
      expect(e.rubles, `day ${day}: treasury went negative`).toBeGreaterThanOrEqual(0);
      if (e.tradeLedger.yesterday.foreignLabor < 0) paidForeignLabor = true;
      // winters bite (high 20s is a hard January), but never collapse
      expect(e.happiness, `day ${day}: happiness collapsed`).toBeGreaterThan(15);
      peakPop = Math.max(peakPop, e.pop);
      if (peakPop >= 40) { // settler churn in the first weeks is noise, not collapse
        expect(e.pop, `day ${day}: population collapsed from peak ${peakPop}`)
          .toBeGreaterThanOrEqual(Math.floor(peakPop * 0.5));
      }

      // ---- milestones ----
      switch (day) {
        case 120: // the wooden town runs on depot stock + first sawmill output.
          // Under fair-share construction the prioritized chain finishes first, so
          // the sawmill is milling by ~d113 — only just producing by d120 (the pool
          // is spread thinner early than the old one-site-at-a-time build).
          expect(e.stats.produced.planks, 'd120 planks').toBeGreaterThanOrEqual(1);
          expect(e.pop, 'd120 pop').toBeGreaterThanOrEqual(24);
          expect(e.objectivesDone.length, 'd120 objectives').toBeGreaterThanOrEqual(2);
          break;
        case 240: // bricks + food chains flowing; treasury bore the bootstrap labor tax and held
          expect(e.stats.produced.bricks, 'd240 bricks').toBeGreaterThanOrEqual(30);
          expect(e.stats.produced.food, 'd240 food').toBeGreaterThanOrEqual(20);
          expect(e.rubles, 'd240 treasury').toBeGreaterThanOrEqual(2000);
          break;
        case 360: // survived the first electrified winter. Power is instantaneous
          // and on the coldest days the lone plant loses its coal to heating, so we
          // assert electrification HAPPENED (grid seen live by now), not a nonzero
          // reading on this exact deep-winter day.
          expect(e.pop, 'd360 pop').toBeGreaterThanOrEqual(56);
          expect(electrified, 'd360 electrified').toBe(true);
          expect(e.rubles, 'd360 treasury').toBeGreaterThanOrEqual(1000);
          break;
        case 600: // electrified, though the machinery-wear arc keeps mid-game
          // power tight (worn plants run at half) until the Machine Works closes
          // the loop — the grid fully catches up by d1080 (asserted below)
          expect(e.powerProduced, 'd600 power').toBeGreaterThanOrEqual(60); // observed 100
          break;
        case 720: // the border earns: textiles + surplus flow out for rubles
          expect(e.stats.exportedValue, 'd720 exports').toBeGreaterThanOrEqual(3000); // observed 5311
          // the oil chain is pumping — the fleet's fuel bill is on its way out
          expect(e.stats.produced.oil, 'd720 oil').toBeGreaterThan(0); // observed 80
          break;
        case 960: // heavy industry online: domestic steel, machinery and fuel
          expect(e.stats.produced.steel, 'd960 steel').toBeGreaterThanOrEqual(140); // observed 209
          expect(e.stats.produced.machinery, 'd960 machinery').toBeGreaterThanOrEqual(30); // observed 71
          expect(e.stats.produced.fuel, 'd960 fuel').toBeGreaterThanOrEqual(70); // observed 310
          expect(e.dollars, 'd960 dollars from objectives').toBeGreaterThanOrEqual(800); // observed 1300
          break;
        case 1080: // the mature republic
          expect(e.stats.produced.steel, 'd1080 steel').toBeGreaterThanOrEqual(180); // observed 258
          expect(e.stats.produced.machinery, 'd1080 machinery').toBeGreaterThanOrEqual(55); // observed 85
          expect(e.stats.produced.fuel, 'd1080 fuel').toBeGreaterThanOrEqual(170); // observed 378
          expect(e.pop, 'd1080 pop').toBeGreaterThanOrEqual(220); // observed 296
          expect(e.rubles, 'd1080 treasury').toBeGreaterThanOrEqual(1000); // observed 4841
          expect(e.powerProduced, 'd1080 power').toBeGreaterThanOrEqual(130); // observed 200
          break;
      }
    });

    // the full arc of Moscow's plan, by name
    for (const id of ['roads', 'housing', 'firstMachines', 'coal', 'power', 'steel', 'export', 'meansOfProduction', 'pop150', 'autarky']) {
      expect(engine.objectivesDone, `objective ${id}`).toContain(id);
    }
    // the wear tax was actually paid across the border
    expect(engine.stats.imported.machinery ?? 0).toBeGreaterThanOrEqual(5);
    // fuel autarky: the refinery took over and the standing fuel order went quiet.
    // Imports level off at ~155 once it runs; anything much above that means the
    // oil chain never closed and the fleet is still drinking foreign fuel.
    expect(engine.stats.imported.fuel ?? 0, 'fuel imports tapered off').toBeLessThanOrEqual(260); // observed 127
    // the fleet is fuelled and working, not a yard of grounded lorries
    expect(engine.fleetStatus().grounded, 'lorries grounded at plan end').toBe(0);
    // and the republic paid for imported construction labor to bootstrap
    expect(paidForeignLabor, 'foreign labor was hired and paid').toBe(true);
    // and no construction site is stranded at the end of the plan
    expect([...engine.buildings.values()].filter(b => !b.constructed)).toHaveLength(0);
    // ~3.5s locally but 7s+ under CI parallel-worker contention — a generous
    // wall-clock ceiling (well past the worst case) stops the flaky false-red
    // without masking a real hang; the assertions above are the actual tripwire.
  }, 30_000);
});
