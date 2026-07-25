// Delivery. Ranked by downtime prevented, never by a priority table.
//
// `dispatchScore()` derives rank from the sim — days of cover against delivery
// ETA, valued by the downtime a load averts and divided by the round trip. A
// bin that was never going to run dry averts ~0 and scores ~0 however empty it
// looks, and that non-linearity is what lets round-trip cost divide the score
// unconditionally, with no lifeline carve-out.
//
// Staged, and it is the system the whole idea was needed for: serving a
// destination raises its `incoming`, which lowers what its remaining demands
// are worth, so the next lorry can fall to a rival building entirely. That
// read-your-own-writes loop IS the load-spreading — there are no reserved
// slices and no fair-share quotas anywhere in here.
import { BALANCE } from '../config';
import { Staged } from '../mutation';
import type { Mutation, MutationKind } from '../mutation';
import { shareAnyComponent } from '../topology';
import type {
  EtaPass, IndexedFacility, LogisticsDemand, LogisticsRoutingContext, World,
} from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['routingCounter', 'repairImport', 'stock', 'incoming', 'vehicleJob', 'boatOrderAdd', 'emergencyFuel'];

export function logistics(w: World): Mutation[] {
  const s = new Staged(w);
  // The safety net runs FIRST, before the fleet is even counted. It exists to
  // break the state where there is no fuel and therefore nothing can haul
  // fuel — so gating it on having a working fleet would disarm it in exactly
  // the case it was written for.
  if (w.emergencyFuelAutoBuy) checkEmergencyFuelAutoBuy(w, s);

  let budget = w.trucks.reduce((n, v) => n + (w.vehicleAvailable(v) ? 1 : 0), 0);
  if (budget <= 0) return s.muts;

  // One ordered supplier index per pass. It snapshots Map insertion order,
  // while live active flags/counts are decremented after each dispatch.
  const routing = w.buildLogisticsRoutingContext();
  const demands = w.collectLogisticsDemands(routing);
  const eta = w.beginEtaPass();

  // ---- Pass 1: prevent downtime, best value per truck-day first ----
  //
  // Marginal greedy, NOT a frozen sorted walk. Serving a destination raises
  // its `incoming`, which lowers what its remaining demands are worth, so the
  // next truck can fall to a rival building or class. That is what makes
  // load spread out on its own — no reserved slices, no per-class quotas,
  // no fair-share pass. Model the diminishing returns and sharing is free.
  const preventive = demands.filter(d => d.kind !== 'housekeeping');
  for (const d of preventive) d.score = w.dispatchScore(d, w.estimateEtaDays(eta, d.b));

  // Rank EVERY live demand — the cap below bounds routing work, not what the
  // republic is willing to look at. Slicing the candidate list instead let a
  // handful of high-scoring but unservable demands (an aged site waiting on
  // steel nobody has) crowd every real need out of the window entirely.
  const pool = preventive
    .filter(d => (d.score ?? 0) > 0)
    .sort((a, b) => (b.score ?? 0) - (a.score ?? 0));

  // Routing is the expensive step, so bound ATTEMPTS: a tick never performs
  // more than a few route searches per free truck, however long the queue is.
  // ONE counter for the whole tick — pass 2 walks the entire demand list, so
  // an unbounded second pass would hand back everything the first pass's cap
  // was protecting (500 unservable demands × a full pathfinding flood each).
  let attempts = 0;
  const maxAttempts = Math.max(8, budget * BALANCE.logisticsCandidateFactor);

  const done = new Set<LogisticsDemand>();
  while (budget > 0 && attempts < maxAttempts) {
    let best: LogisticsDemand | null = null;
    let bestScore = 0;
    for (const d of pool) {
      if (done.has(d)) continue;
      const s = d.score ?? 0;
      if (s > bestScore) { bestScore = s; best = d; }
    }
    if (!best) break;
    done.add(best);
    attempts++;
    const servedId = best.b.id;
    const before = demands.length;
    const dispatched = tryDispatch(w, s, best, routing, demands, eta);
    // A failed route may have registered cross-water relay legs; they stand in
    // for this same need, so they join the pool and compete this pass — the
    // old sorted walk saw them for the same reason (for-of over a growing array).
    for (let i = before; i < demands.length; i++) {
      const leg = demands[i];
      if (leg.kind === 'housekeeping') continue;
      leg.score = w.dispatchScore(leg, w.estimateEtaDays(eta, leg.b));
      if ((leg.score ?? 0) > 0) pool.push(leg);
    }
    if (dispatched) {
      budget--;
      // Only the served building's outlook changed — re-score just those.
      for (const d of pool) {
        if (!done.has(d) && d.b.id === servedId) d.score = w.dispatchScore(d, w.estimateEtaDays(eta, d.b));
      }
    }
  }

  // ---- Pass 2: opportunistic, only on trucks nothing preventable needs ----
  //
  // Everything that prevents no downtime lands here: overflow hauls, export
  // staging, comfortable bins topping up, and stocking a shop that has no
  // citizens drawing on it yet. None of it can outbid a real need — that is
  // structural, not a priority band kept low by hand — but when the fleet has
  // spare capacity there is no reason to leave shelves empty.
  //
  // This is also why scarcity is the only regime where ranking bites: with
  // trucks to spare the republic does everything, in collection order.
  //
  // Bounded like pass 1, but off ITS OWN budget — the lorries pass 1 did not
  // use. Sharing one counter looked tidier and was wrong: a busy day of real
  // needs would spend the whole allowance and leave nothing to stage exports
  // with, so the border went quiet exactly when the republic was productive.
  // Total routing work stays O(fleet), which is the invariant that matters.
  //
  // Index loop, not for-of: `demands` grows while we walk it (relayViaPorts
  // appends legs), and the cap has to bound that too.
  let spare = 0;
  const maxSpare = Math.max(8, budget * BALANCE.logisticsCandidateFactor);
  for (let i = 0; i < demands.length && budget > 0 && spare < maxSpare; i++) {
    const d = demands[i];
    if (done.has(d)) continue;
    spare++;
    if (tryDispatch(w, s, d, routing, demands, eta)) budget--;
  }
  return s.muts;
}

/** Route and dispatch one demand. Returns true if a truck actually left. */
function tryDispatch(
  w: World, s: Staged,
  d: LogisticsDemand, routing: LogisticsRoutingContext, demands: LogisticsDemand[], eta: EtaPass,
): boolean {
  s.emit({ k: 'routingCounter', field: 'demandsConsidered' });
  const destFree = d.b.constructed
    ? w.capOf(d.b, d.r) - w.stockOf(d.b, d.r) - w.incomingOf(d.b, d.r)
    : (w.def(d.b).materials[d.r] ?? 0) - w.stockOf(d.b, d.r) - w.incomingOf(d.b, d.r);
  // sites accept fractional remainders (a dribble-fed site missing 0.8
  // bricks must not starve forever, holding its other materials hostage);
  // constructed buildings keep the ≥1 gate against truck churn
  const minLoad = d.b.constructed ? BALANCE.logisticsMinLoad : 0.001;
  if (destFree < minLoad) return false;

  // A repair import that cannot buy even the minimum load at the current
  // reserve floor is rejected before any routing work.
  if (d.repairImport) {
    const cur = d.repairImport;
    const price = w.importPriceOf(d.r, cur);
    const reserve = cur === 'east' ? w.autoTrade.reserveRubles : w.autoTrade.reserveDollars;
    const funds = cur === 'east' ? w.rubles : w.dollars;
    if (Math.floor(Math.max(0, funds - reserve) / price) < minLoad) return false;
  }

  // ROAD-FIRST: a bounded destination-origin search sees only eligible
  // supplier access goals in a shared component, preserving the old tie
  // and path rules without filling the rest of the map.
  let pick = w.routeToSupply(routing, d, 'road');
  let offRoad = false;

  if (!pick) {
    // OFF-ROAD FALLBACK: weighted land, only after the road attempt fails.
    pick = w.routeToSupply(routing, d, 'land');
    offRoad = true;
    if (!pick) {
      // Domestic demands relay any goods across water; an auto-buy construction
      // site (bonded, pinned to its customs) relays its paid IMPORTS across too.
      if (d.from === undefined || (d.bonded && !d.b.constructed)) relayViaPorts(w, s, d, routing, demands, eta);
      return false;
    }
  }

  // bonded goods are a paid virtual import — the customs is an infinite
  // source and its real stock is never touched (bypasses the storage cap)
  // Revalidate immediately before charging or mutating sequential stock.
  const supplyCap = d.bonded ? Infinity : w.supplyOf(pick.supplier, d.r);
  let amount = Math.min(d.amt, destFree, supplyCap, BALANCE.truckCapacity);
  if (amount < minLoad) {
    if (pick.candidate) w.deactivateSupplyCandidate(routing, pick.candidate, d.r);
    return false;
  }

  // roads: legacy per-tile timing; off-road: accumulated weighted cost (slower)
  const travel = offRoad ? pick.cost : pick.path.length;

  // A lorry has to be free, near enough, and carrying enough fuel to finish
  // the whole run. Claimed BEFORE any stock or treasury is touched, so a
  // fleet-limited day never half-commits a trade.
  const assign = w.pickVehicleFor(pick.supplier, travel);
  if (!assign) return false;

  // a repair import is a paid border purchase (unlike a construction auto-buy,
  // paid upfront): cap it to what the treasury can spend above its auto-reserve,
  // then charge on dispatch and book it on the ledger + import stats.
  if (d.repairImport) {
    const cur = d.repairImport;
    const price = w.importPriceOf(d.r, cur);
    const reserve = cur === 'east' ? w.autoTrade.reserveRubles : w.autoTrade.reserveDollars;
    const funds = cur === 'east' ? w.rubles : w.dollars;
    amount = Math.min(amount, Math.floor(Math.max(0, funds - reserve) / price));
    if (amount < minLoad) return false; // treasury at the reserve floor — retry another day
    s.emit({ k: 'repairImport', r: d.r, bloc: cur, amount, cost: amount * price });
  }

  if (!d.bonded) {
    s.emit({ k: 'stock', id: pick.supplier.id, r: d.r, delta: -amount });
    // The routing context is PASS-LOCAL scratch, not world state — it is not a
    // mutation, and staging it would be staging a local variable.
    if (pick.candidate) w.deactivateSupplyCandidate(routing, pick.candidate, d.r);
  }
  s.emit({ k: 'incoming', id: d.b.id, r: d.r, amount });

  s.emit({
    k: 'vehicleJob',
    vehicleId: assign.v.id,
    supplierId: pick.supplier.id,
    destId: d.b.id,
    r: d.r,
    amount,
    approach: assign.path,
    approachTiles: assign.tiles,
    loadedPath: pick.path,
    loadedTiles: travel,
  });
  s.emit({ k: 'routingCounter', field: 'successfulDispatches' });
  return true;
}

/**
 * A demand no road-connected supplier can serve may still be servable across
 * water. Register a twin demand at a far-shore port (trucks bring the goods
 * portside) plus a standing barge order to the near-shore port; the original
 * demand is served from that port on a later day. Two kinds of source:
 *   • domestic goods  (d.from === undefined) — the far-shore port is fed by a
 *     road-connected supplier, and the site pulls the ferried stock itself.
 *   • auto-buy IMPORTS (d.bonded, pinned to a customs) — the far-shore leg is a
 *     bonded customs import; because the site's own demand is pinned to the customs
 *     (and can't draw a port), a non-bonded FINAL leg drains the island port into
 *     the site. Only the finite far-shore leg is bonded, so the infinite customs
 *     source can never leak past what is still owed.
 */
function relayViaPorts(
  w: World,
  s: Staged,
  d: LogisticsDemand,
  routing: LogisticsRoutingContext,
  demands: LogisticsDemand[],
  eta: EtaPass,
) {
  const ports = [...w.buildings.values()].filter(p => w.def(p).isPort && p.constructed);
  if (ports.length < 2) return;
  const destination = routing.facilities.get(d.b.id);
  if (!destination) return;
  // Every leg created below stands in for THIS demand, so it inherits this
  // demand's value — a port consumes nothing and would otherwise score zero.
  const relayed = d.relayScore ?? d.score ?? w.dispatchScore(d, w.estimateEtaDays(eta, d.b));
  const pDest = ports.find(p => {
    const port = routing.facilities.get(p.id)!;
    return p.id !== d.b.id && shareAnyComponent(port.land.components, destination.land.components);
  });
  if (!pDest) return;

  // Bonded FINAL leg: a bonded site can't pull a port itself, so drain whatever
  // imports have already landed on the island into the site by NON-bonded truck
  // (which actually decrements the port). A land haul on the island — it runs even
  // while the river is frozen.
  if (d.bonded) {
    const landed = w.supplyOf(pDest, d.r);
    if (landed >= 1) demands.push({ b: d.b, r: d.r, amt: Math.min(d.amt, landed), kind: d.kind, relayScore: relayed, from: pDest.id });
  }

  if (w.weather.riverFrozen) return; // no new water chains onto an ice-locked river

  const pending = w.boatOrders.find(o => o.destId === pDest.id && o.r === d.r);
  if (pending) {
    // order already exists — keep the far-shore leg alive until the source port
    // actually holds the goods (its truck may have lost the dispatch budget earlier)
    const src = w.buildings.get(pending.srcId);
    if (src) {
      const short = pending.amt - w.stockOf(src, d.r) - w.incomingOf(src, d.r);
      if (short >= 1) demands.push(d.bonded
        ? { b: src, r: d.r, amt: short, kind: d.kind, relayScore: relayed, from: d.from, bonded: true }
        : { b: src, r: d.r, amt: short, kind: d.kind, relayScore: relayed });
    }
    return;
  }

  // Size a new chain to the shortfall not already staged/in-transit at the island
  // port. Without this, a bonded demand would re-materialize from the infinite
  // customs on every pass while a barge is still en route (incomingOf(pDest)).
  const need = d.bonded ? d.amt - (w.stockOf(pDest, d.r) + w.incomingOf(pDest, d.r)) : d.amt;
  if (need < 1) return;

  const pDestWater = w.waterAccess(pDest);
  const overWater = ports.filter(p => p.id !== pDest.id &&
    shareAnyComponent(w.waterAccess(p).components, pDestWater.components));
  for (const pSrc of overWater) {
    // domestic: pSrc's road reaches a willing supplier; bonded: it reaches the customs
    const source = routing.facilities.get(pSrc.id)!;
    const qualifies = d.bonded
      ? portRoadReachesCustoms(w, routing, d.from!, source)
      : w.roadSupplierReaches(routing, d.r, source);
    if (!qualifies) continue;
    const amt = Math.min(
      need,
      BALANCE.boatCapacity,
      w.capOf(pSrc, d.r) - w.stockOf(pSrc, d.r) - w.incomingOf(pSrc, d.r),
      w.capOf(pDest, d.r) - w.stockOf(pDest, d.r) - w.incomingOf(pDest, d.r),
    );
    if (amt < 1) return;
    demands.push(d.bonded
      ? { b: pSrc, r: d.r, amt, kind: d.kind, relayScore: relayed, from: d.from, bonded: true } // bonded import leg
      : { b: pSrc, r: d.r, amt, kind: d.kind, relayScore: relayed });                            // domestic leg
    s.emit({ k: 'boatOrderAdd', srcId: pSrc.id, destId: pDest.id, r: d.r, amt });
    return;
  }
}

/** Does this port's road network reach the given customs house? The bonded-import
 *  mirror of roadSupplierReaches — a customs is the paid import's road-side source. */
function portRoadReachesCustoms(
  w: World,
  ctx: LogisticsRoutingContext,
  customsId: number,
  port: IndexedFacility,
): boolean {
  w.assertRoutingFresh(ctx);
  const customs = ctx.facilities.get(customsId);
  return !!customs && shareAnyComponent(port.road.components, customs.road.components);
}


function checkEmergencyFuelAutoBuy(w: World, s: Staged): void {
  if (w.trucks.length === 0) return;                       // no fleet, no need
  if (w.pumpFuel() >= BALANCE.emergencyFuelFloor) return;   // pumps still have some
  const customs = [...w.buildings.values()]
    .filter(b => b.constructed && b.connected && w.def(b).isCustoms)
    .sort((a, b) => w.stockOf(a, 'fuel') - w.stockOf(b, 'fuel') || a.id - b.id)[0];
  if (!customs) return;
  const led = w.tradeLedger.today;

  const price = w.importPriceOf('fuel', 'east');
  const throughput = Math.floor(led.capacity - led.used);
  const free = Math.floor(w.capOf(customs, 'fuel') - w.stockOf(customs, 'fuel') - w.incomingOf(customs, 'fuel'));
  const affordable = Math.floor(Math.max(0, w.rubles - w.autoTrade.reserveRubles) / price);
  const wanted = Math.floor(BALANCE.emergencyFuelTarget - w.stockOf(customs, 'fuel'));
  const amt = Math.min(wanted, BALANCE.emergencyFuelBuy, throughput, free, affordable);
  if (amt < 1) return;

  s.emit({ k: 'emergencyFuel', customsId: customs.id, amount: amt, cost: amt * price });
}
