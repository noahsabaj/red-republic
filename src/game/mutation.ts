// ============================================================
// Red Republic — the mutation vocabulary
// ============================================================
// A system is a pure function of the world that returns what it wants to
// change. `applyMutations` is the ONLY thing that writes. That inversion is the
// point of the whole refactor: a system's effects become declared, auditable
// and impossible to exceed, so a new mechanic can no longer widen the blast
// radius of an old one by quietly reaching for a field.
//
// Every kind is NAMED after the effect it has. There is deliberately no generic
// `{ field, value }` escape hatch — that would be "write anything" with extra
// steps, and the guard test in `mutation-writeset.test.ts` would have nothing
// to check.
//
// Ordering is load-bearing. Systems emit in source order and the day loop
// applies in emission order, so float accumulation and event ids stay
// bit-identical to the hand-written code these replace.
import { BALANCE } from './config';
import type { ResourceId } from './config';
import { emptyLedger } from './world';
import type { GameEvent, World } from './world';
import type { DayWeather } from './weather';

export type Mutation =
  // ---- weather ----
  /** Today's weather, read from the deterministic timeline. */
  | { k: 'weather'; weather: DayWeather }
  /** The four running streak counters, derived together and set together. */
  | { k: 'weatherStreaks'; dry: number; gloom: number; sun: number; frost: boolean }

  // ---- connectivity ----
  /** One building's freight-network reachability. */
  | { k: 'connectivity'; id: number; connected: boolean; roadConnected: boolean }

  // ---- labour ----
  /** The labour pool: of-age citizens, posts on offer, posts filled. */
  | { k: 'labour'; workers: number; jobs: number; employed: number }
  /** One building's crew. */
  | { k: 'staff'; id: number; staff: number }

  // ---- the grid and the boilers ----
  /** The day's power and heat balance. */
  | { k: 'grid'; powerProduced: number; powerDemand: number; heatProduced: number; heatDemand: number }
  /** One building's efficiency for the day. */
  | { k: 'eff'; id: number; eff: number }
  /** One plant's input-availability throttle for the day. */
  | { k: 'coalFactor'; id: number; coalFactor: number }
  /** Whether the grid could feed this building today. */
  | { k: 'powered'; id: number; powered: boolean }
  /** Whether the boilers could reach this building today. */
  | { k: 'heated'; id: number; heated: boolean }

  // ---- work done ----
  /** A bin changes by `delta`, already clamped by the emitting system to what
   *  the bin can actually take. Applying it re-clamps, so it can never overfill. */
  | { k: 'stock'; id: number; r: ResourceId; delta: number }
  /** Cumulative national output, the objective metric. */
  | { k: 'produced'; r: ResourceId; amount: number }
  /** A farm's worked field count (display). */
  | { k: 'farmFields'; id: number; fields: number }

  // ---- the people ----
  /** Beds in the republic. */
  | { k: 'housingCapacity'; capacity: number }
  /** The satisfaction table, lerped as a set so every figure reads the same
   *  previous day. */
  | { k: 'satisfaction'; sat: World['sat'] }
  | { k: 'happiness'; happiness: number }
  | { k: 'population'; pop: number }

  // ---- national accounts ----
  /** The national stockpile table, recounted from every bin. */
  | { k: 'totals'; totals: Record<ResourceId, number> }

  // ---- the ledger ----
  /** Money crosses the border. `bloc` picks the currency: east = rubles, west
   *  = dollars. Emitters clamp so the treasury can never go negative. */
  | { k: 'treasury'; bloc: 'east' | 'west'; delta: number }
  /** Standing price malus with one bloc (0..cap). */
  | { k: 'relations'; bloc: 'east' | 'west'; penalty: number }

  // ---- barges ----
  /** No port stands: the whole queue is void. */
  | { k: 'boatOrdersClear' }
  /** Remove one queued order. By index because the dispatcher walks the queue
   *  backwards and Staged applies immediately, so the index it read is the
   *  index that is removed. */
  | { k: 'boatOrderDrop'; index: number }
  /** Draw `amount` off a queued order that has just been loaded. */
  | { k: 'boatOrderTake'; index: number; amount: number }
  /** Load a barge and send it: stock out of the source, booked as incoming at
   *  the destination so nothing double-orders against it. */
  | {
      k: 'boatDispatch'; srcId: number; destId: number; r: ResourceId;
      amount: number; path: { x: number; y: number }[];
    }
  /** One route refused because the endpoints share no component. */
  | { k: 'routingRejection' }

  // ---- construction ----
  /** Builder-days applied to a site. */
  | { k: 'siteProgress'; id: number; days: number }
  /** The site reached its labour bill: materials consumed, spares installed —
   *  or, for a road/bridge, the site dissolves into the tile it paved. */
  | { k: 'siteComplete'; id: number }
  /** Today's bill for hired foreign crews. */
  | { k: 'foreignLaborBill'; bloc: 'east' | 'west'; cost: number }

  // ---- the daily trade ledger ----
  /** Close the day's book and open a fresh one. */
  | { k: 'ledgerRoll' }
  /** Add one customs house's cleared tonnage to today's throughput. */
  | { k: 'ledgerCapacity'; amount: number }
  /** Note why auto-trade could not do more (deduplicated by the applier). */
  | { k: 'ledgerBlocked'; why: string }
  /** One export across the border, whole units. Atomic on purpose: stock out,
   *  contract credited, treasury paid, ledger booked, lorry waved through are a
   *  single transaction and are never emitted apart. */
  | { k: 'exportSale'; id: number; r: ResourceId; bloc: 'east' | 'west'; amount: number }
  /** The mirror: treasury out, goods into that customs house's stock. */
  | { k: 'importPurchase'; id: number; r: ResourceId; bloc: 'east' | 'west'; amount: number }

  // ---- contracts ----
  | { k: 'contractState'; id: number; state: 'active' | 'done' | 'failed'; closedIdx?: number }
  /** Withdraw an expired offer or retire old history. */
  | { k: 'contractDrop'; id: number }

  // ---- loans ----
  | { k: 'loanState'; id: number; state: 'active' | 'repaid' | 'defaulted' }
  | { k: 'loanRepaid'; id: number; amount: number }
  | { k: 'loanCooldown'; bloc: 'east' | 'west'; untilDayIdx: number }
  | { k: 'loanDrop'; id: number }

  // ---- the five-year plan ----
  | { k: 'objectiveDone'; id: string }

  // ---- the notice board ----
  /** A message for the player. Queued, not state — but it is an effect, so it
   *  is declared like one; that is what keeps a system from needing the engine. */
  | { k: 'event'; text: string; kind: GameEvent['kind']; icon?: string };

export type MutationKind = Mutation['k'];

/** Set by the write-set guard test to record what each system actually emits.
 *  Null in normal play, so this costs one null check per mutation. */
let journal: ((m: Mutation) => void) | null = null;

export function setMutationJournal(fn: ((m: Mutation) => void) | null): void {
  journal = fn;
}

/**
 * Records what a system does, and does it, in emission order.
 *
 * The derivations need nothing like this: each reads the world as it was and
 * its effects don't feed back into its own reads. The transactions do — a loan
 * repayment checks objectives, which reads the treasury the repayment just
 * changed — and so does logistics, whose marginal-greedy pass re-scores a
 * building after every dispatch it makes.
 *
 * A copy-on-write overlay (nothing written until the day loop applies) would
 * buy nothing over this and would cost a shadow of the whole object graph.
 * Either way every effect passes through `applyMutations` and lands in the
 * returned list, which is the property that makes a write-set auditable — the
 * point was never to delay the write, it was to declare it.
 */
export class Staged {
  readonly muts: Mutation[] = [];
  readonly w: World;
  constructor(w: World) { this.w = w; }

  emit(...ms: Mutation[]): void {
    for (const m of ms) {
      this.muts.push(m);
      applyMutations(this.w, [m]);
    }
  }

  /** Fold in a nested staged system's list. It has already applied them — this
   *  only keeps the caller's record of the day complete. */
  record(ms: readonly Mutation[]): void {
    this.muts.push(...ms);
  }
}

export function applyMutations(w: World, muts: readonly Mutation[]): void {
  for (const m of muts) {
    journal?.(m);
    switch (m.k) {
      case 'weather':
        w.weather = m.weather;
        break;
      case 'weatherStreaks':
        w.dryStreak = m.dry; w.gloomStreak = m.gloom; w.sunStreak = m.sun; w.wasFrost = m.frost;
        break;
      case 'connectivity': {
        const b = w.buildings.get(m.id);
        if (b) { b.connected = m.connected; b.roadConnected = m.roadConnected; }
        break;
      }
      case 'labour':
        w.workers = m.workers; w.jobs = m.jobs; w.employed = m.employed;
        break;
      case 'staff': {
        const b = w.buildings.get(m.id);
        if (b) b.staff = m.staff;
        break;
      }
      case 'grid':
        w.powerProduced = m.powerProduced; w.powerDemand = m.powerDemand;
        w.heatProduced = m.heatProduced; w.heatDemand = m.heatDemand;
        break;
      case 'eff': {
        const b = w.buildings.get(m.id);
        if (b) b.eff = m.eff;
        break;
      }
      case 'coalFactor': {
        const b = w.buildings.get(m.id);
        if (b) b.coalFactor = m.coalFactor;
        break;
      }
      case 'powered': {
        const b = w.buildings.get(m.id);
        if (b) b.powered = m.powered;
        break;
      }
      case 'heated': {
        const b = w.buildings.get(m.id);
        if (b) b.heated = m.heated;
        break;
      }
      case 'stock': {
        const b = w.buildings.get(m.id);
        if (b) w.addStock(b, m.r, m.delta);
        break;
      }
      case 'produced':
        w.stats.produced[m.r] += m.amount;
        break;
      case 'farmFields': {
        const b = w.buildings.get(m.id);
        if (b) b.farmFields = m.fields;
        break;
      }
      case 'housingCapacity':
        w.capacity = m.capacity;
        break;
      case 'satisfaction':
        w.sat = m.sat;
        break;
      case 'happiness':
        w.happiness = m.happiness;
        break;
      case 'population':
        w.pop = m.pop;
        break;
      case 'totals':
        w.totals = m.totals;
        break;
      case 'treasury':
        if (m.bloc === 'east') w.rubles += m.delta; else w.dollars += m.delta;
        break;
      case 'relations':
        w.relationsPenalty[m.bloc] = m.penalty;
        break;
      case 'boatOrdersClear':
        w.boatOrders = [];
        break;
      case 'boatOrderDrop':
        w.boatOrders.splice(m.index, 1);
        break;
      case 'boatOrderTake':
        w.boatOrders[m.index].amt -= m.amount;
        break;
      case 'boatDispatch': {
        const src = w.buildings.get(m.srcId), dest = w.buildings.get(m.destId);
        if (!src || !dest) break;
        w.addStock(src, m.r, -m.amount);
        dest.incoming[m.r] = w.incomingOf(dest, m.r) + m.amount;
        w.boats.push({
          id: w.nextBoatId++,
          points: [w.centerOf(src), ...m.path, w.centerOf(dest)],
          cargo: m.r, amount: m.amount,
          daysTotal: Math.max(1, m.path.length * BALANCE.boatDaysPerTile),
          daysDone: 0, phase: 'go', destId: dest.id, srcId: src.id,
        });
        break;
      }
      case 'routingRejection':
        w.routingDay.componentRejections++;
        break;
      case 'siteProgress': {
        const b = w.buildings.get(m.id);
        if (b) b.progress += m.days;
        break;
      }
      case 'siteComplete': {
        const b = w.buildings.get(m.id);
        if (b) w.completeSite(b);
        break;
      }
      case 'foreignLaborBill': {
        const led = w.tradeLedger.today;
        if (m.bloc === 'east') {
          w.rubles -= m.cost;
          led.foreignLaborRubles -= m.cost;
          led.foreignLabor = led.foreignLaborRubles;
        } else {
          w.dollars -= m.cost;
          led.foreignLaborDollars -= m.cost;
        }
        break;
      }
      case 'ledgerRoll':
        w.tradeLedger.yesterday = w.tradeLedger.today;
        w.tradeLedger.today = emptyLedger();
        break;
      case 'ledgerCapacity':
        w.tradeLedger.today.capacity += m.amount;
        break;
      case 'ledgerBlocked':
        if (!w.tradeLedger.today.blocked.includes(m.why)) w.tradeLedger.today.blocked.push(m.why);
        break;
      case 'exportSale': {
        const c = w.buildings.get(m.id);
        if (c) w.sellAcrossBorder(c, m.r, m.bloc, m.amount);
        break;
      }
      case 'importPurchase': {
        const c = w.buildings.get(m.id);
        if (c) w.buyAcrossBorder(c, m.r, m.bloc, m.amount);
        break;
      }
      case 'contractState': {
        const c = w.contracts.find(x => x.id === m.id);
        if (c) { c.state = m.state; if (m.closedIdx !== undefined) c.closedIdx = m.closedIdx; }
        break;
      }
      case 'contractDrop': {
        const i = w.contracts.findIndex(x => x.id === m.id);
        if (i >= 0) w.contracts.splice(i, 1);
        break;
      }
      case 'loanState': {
        const l = w.loans.find(x => x.id === m.id);
        if (l) l.state = m.state;
        break;
      }
      case 'loanRepaid': {
        const l = w.loans.find(x => x.id === m.id);
        if (l) l.repaid += m.amount;
        break;
      }
      case 'loanCooldown':
        w.loanCooldown[m.bloc] = m.untilDayIdx;
        break;
      case 'loanDrop': {
        const i = w.loans.findIndex(x => x.id === m.id);
        if (i >= 0) w.loans.splice(i, 1);
        break;
      }
      case 'objectiveDone':
        w.objectivesDone.push(m.id);
        break;
      case 'event':
        w.pushEvent(m.text, m.kind, m.icon);
        break;
    }
  }
}
