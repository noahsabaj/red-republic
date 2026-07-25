// The fleet reconciles with its garages, then tops up dry tanks — both before
// dispatch, so a lorry built or filled today can work today.
//
// Fuel leaves a building's bin exactly once, here, when a lorry pumps it. There
// is no pooled fleet tank and no per-day levy anywhere else in the day; that is
// what makes the gauge on the inspector mean something.
import { BALANCE } from '../config';
import { Staged } from '../mutation';
import type { Mutation } from '../mutation';
import type { Vehicle, VehicleState, World } from '../world';

/**
 * Reconcile the fleet with the garages that own it. A Construction Office or
 * Motor Depot should own `trucksFrom()` vehicles; missing ones are built (empty
 * tank — a new lorry arrives dry), surplus ones are retired.
 *
 * Only IDLE vehicles are retired, so a garage losing staff never destroys a
 * load in transit; the busy ones are collected on a later day once parked.
 * Iteration follows Map insertion order, so fleet composition is deterministic
 * for a given seed.
 */
export function syncFleet(w: World): Mutation[] {
  const s = new Staged(w);
  const owned = new Map<number, Vehicle[]>();
  for (const v of w.trucks) {
    const list = owned.get(v.homeId);
    if (list) list.push(v); else owned.set(v.homeId, [v]);
  }
  const garages = new Set<number>();
  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (!def.isConstructionOffice && !def.isMotorDepot) continue;
    garages.add(b.id);
    const want = w.trucksFrom(b);
    const have = owned.get(b.id) ?? [];
    for (let i = have.length; i < want; i++) s.emit({ k: 'vehicleCommission', homeId: b.id });
  }
  // retire surplus / orphaned vehicles, idle ones only. By index, backwards,
  // and Staged applies immediately — so the index read is the index removed.
  for (let i = w.trucks.length - 1; i >= 0; i--) {
    const v = w.trucks[i];
    if (v.state !== 'idle') continue;
    const home = w.buildings.get(v.homeId);
    const quota = home && garages.has(v.homeId) ? w.trucksFrom(home) : 0;
    const siblings = owned.get(v.homeId);
    if (!siblings) continue;
    if (siblings.indexOf(v) >= quota) s.emit({ k: 'vehicleRetire', index: i });
  }
  return s.muts;
}

/** Send low idle vehicles to a pump. Runs before dispatch so a topped-up
 *  vehicle is available for work the same day it fills. */
export function refuelVehicles(w: World): Mutation[] {
  const s = new Staged(w);
  for (const v of w.trucks) {
    if (v.state !== 'idle') continue;
    if (v.fuel > v.fuelCap * BALANCE.vehicleRefuelAt) continue;
    const here = w.buildings.get(v.atId);
    // Already standing on fuel? Pump it without moving.
    if (here && w.stockOf(here, 'fuel') > 0.001 && w.canPumpFuel(here)) {
      const take = Math.min(v.fuelCap - v.fuel, w.stockOf(here, 'fuel'));
      if (take > 0) s.emit({ k: 'vehiclePump', vehicleId: v.id, pumpId: here.id, amount: take });
      if (v.fuel > v.fuelCap * BALANCE.vehicleRefuelAt) continue;
    }
    if (!here) continue;
    for (const src of w.fuelSourcesFor(v)) {
      if (src.id === here.id) continue;
      // Emitted speculatively: routing can fail, and a leg that cannot be routed
      // must leave the lorry untouched rather than desync destId from points.
      const before: VehicleState = v.state;
      s.emit({ k: 'vehicleLeg', vehicleId: v.id, fromId: here.id, toId: src.id, state: 'toRefuel' });
      if (v.state !== before) break;
    }
  }
  return s.muts;
}
