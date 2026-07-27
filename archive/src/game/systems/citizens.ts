// How well the republic is living, and who therefore stays.
//
// Every satisfaction figure is lerped rather than set: a town that loses power
// for a day should not lose its population that day. Happiness is the weighted
// roll-up, and migration reads only happiness — there are no wages, because
// citizens are compensated in what they consume, which the republic must
// actually produce or import.
import { BALANCE } from '../config';
import type { ResourceId } from '../config';
import type { Mutation, MutationKind } from '../mutation';
import type { BuildingInst, World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['housingCapacity', 'stock', 'satisfaction', 'happiness', 'population', 'event'];

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

export function citizens(w: World): Mutation[] {
  const out: Mutation[] = [];

  // capacity
  let capacity = 0;
  const housing: BuildingInst[] = [];
  for (const b of w.buildings.values()) {
    const def = w.def(b);
    if (b.constructed && def.housingCapacity) {
      capacity += def.housingCapacity;
      housing.push(b);
    }
  }
  out.push({ k: 'housingCapacity', capacity });

  // Satisfaction is worked out on a local copy and emitted once: every figure
  // below is lerped from the previous day's, so they must all read the same
  // starting point regardless of the order they are computed in.
  const sat = { ...w.sat };

  // services coverage
  const servicesOf = (type: 'shop' | 'health' | 'culture') =>
    [...w.buildings.values()].filter(b => {
      const def = w.def(b);
      return b.constructed && def.serviceType === type && b.staff > 0;
    });
  const coveredRatio = (type: 'shop' | 'health' | 'culture') => {
    if (capacity === 0) return 0;
    const svcs = servicesOf(type);
    if (!svcs.length) return 0;
    let covered = 0;
    for (const h of housing) {
      const hc = w.centerOf(h);
      const ok = svcs.some(s => {
        const sc = w.centerOf(s);
        return Math.max(Math.abs(hc.x - sc.x), Math.abs(hc.y - sc.y)) <= (w.def(s).serviceRadius ?? BALANCE.serviceRadius);
      });
      if (ok) covered += w.def(h).housingCapacity!;
    }
    return covered / capacity;
  };

  const shopCov = coveredRatio('shop');
  sat.health = lerp(sat.health, coveredRatio('health'), 0.1);
  sat.culture = lerp(sat.culture, coveredRatio('culture'), 0.1);

  // food & clothes consumption from stores
  const stores = servicesOf('shop');
  const consume = (r: ResourceId, perCapita: number, satKey: 'food' | 'clothes') => {
    const demand = w.pop * perCapita;
    if (demand <= 0) { sat[satKey] = lerp(sat[satKey], 1, 0.1); return; }
    const coveredDemand = demand * shopCov;
    let available = 0;
    for (const s of stores) available += w.stockOf(s, r);
    const served = Math.min(coveredDemand, available);
    // consume proportionally
    if (available > 0) {
      for (const s of stores) {
        const share = w.stockOf(s, r) / available;
        out.push({ k: 'stock', id: s.id, r, delta: -served * share });
      }
    }
    sat[satKey] = lerp(sat[satKey], Math.min(1, served / demand), 0.12);
  };
  consume('food', BALANCE.foodPerCitizen, 'food');
  consume('clothes', BALANCE.clothesPerCitizen, 'clothes');

  // power / heat satisfaction
  let poweredCap = 0, heatedCap = 0;
  for (const h of housing) {
    if (h.powered) poweredCap += w.def(h).housingCapacity!;
    if (h.heated) heatedCap += w.def(h).housingCapacity!;
  }
  sat.power = lerp(sat.power, capacity ? poweredCap / capacity : 1, 0.15);
  sat.heat = lerp(sat.heat, capacity ? heatedCap / capacity : 1, 0.15);

  // employment
  sat.employment = w.workers > 0 ? Math.min(1, w.employed / (w.workers * 0.95)) : 1;

  // pollution
  const polluters = [...w.buildings.values()].filter(b => {
    const def = w.def(b);
    return b.constructed && def.pollution && b.eff > 0;
  });
  if (capacity > 0 && polluters.length) {
    let penaltySum = 0;
    for (const h of housing) {
      const hc = w.centerOf(h);
      let pl = 0;
      for (const p of polluters) {
        const pc = w.centerOf(p);
        if (Math.max(Math.abs(hc.x - pc.x), Math.abs(hc.y - pc.y)) <= BALANCE.pollutionRadius) {
          pl += w.def(p).pollution!;
        }
      }
      penaltySum += Math.max(0.6, 1 - 0.05 * pl) * w.def(h).housingCapacity!;
    }
    sat.pollution = lerp(sat.pollution, penaltySum / capacity, 0.1);
  } else {
    sat.pollution = lerp(sat.pollution, 1, 0.1);
  }
  out.push({ k: 'satisfaction', sat });

  // happiness
  let target = 100 * (
    0.30 * sat.food + 0.14 * sat.clothes + 0.12 * sat.power + 0.12 * sat.heat +
    0.10 * sat.culture + 0.10 * sat.health + 0.12 * sat.employment
  ) * sat.pollution;
  // weather morale: long gray spells wear on people, sunny runs lift them
  target *= 1 - Math.min(0.06, w.gloomStreak * 0.01) + Math.min(0.02, w.sunStreak * 0.005);
  const happiness = lerp(w.happiness, Math.max(0, Math.min(100, target)), 0.2);
  out.push({ k: 'happiness', happiness });

  // migration — settlers only (re)found the republic while its reputation holds
  let pop = w.pop;
  const freeBeds = capacity - pop;
  if (pop === 0 && freeBeds > 0 && happiness >= 48) {
    pop = Math.min(freeBeds, 6);
    out.push({ k: 'event', text: 'First settlers arrived to your republic!', kind: 'good', icon: 'users' });
  } else if (happiness >= 48 && freeBeds > 0) {
    const arrivals = Math.min(freeBeds, 1 + Math.floor(happiness / 35));
    pop += arrivals;
    if (arrivals > 1) out.push({ k: 'event', text: `${arrivals} migrants joined your republic`, kind: 'good', icon: 'users' });
  } else if (happiness < 30 && pop > 0) {
    const departures = Math.min(pop, Math.max(1, Math.min(Math.ceil(pop * 0.1), Math.ceil((30 - happiness) / 8))));
    pop -= departures;
    out.push({ k: 'event', text: `${departures} citizens left the republic (unhappy)`, kind: 'bad', icon: 'users' });
  }
  out.push({ k: 'population', pop: Math.min(pop, capacity) });
  return out;
}
