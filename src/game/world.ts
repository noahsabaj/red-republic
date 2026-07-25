// ============================================================
// Red Republic — the simulated world
// ============================================================
// The domain model every system speaks in. Split out of engine.ts so that a
// system module can name a Vehicle or a Contract without importing the engine
// that runs it — otherwise every extraction is a circular import.
//
// This file holds TYPES and pure helpers only. The mutable `World` state and
// its primitive accessors land here next; the systems that act on them become
// modules alongside it.
import { BUILDINGS } from './config';
import type { DepositType, ResourceId } from './config';
import type { Tile } from './mapgen';
import type { RankedGoal } from './pathfind';
import type { TopologyPos } from './topology';

/** Brownout efficiency for a powered building the grid cannot feed. Buildings
 *  that stop dead instead author `unpoweredEff: 0` in config — there is no list
 *  of ids here, because which buildings stall is game data, not engine logic. */
export const DEFAULT_UNPOWERED_EFF = 0.5;

export interface BuildingInst {
  id: number;
  defId: string;
  x: number; y: number;
  w: number; h: number;
  constructed: boolean;
  progress: number; // labor-days done
  stock: Partial<Record<ResourceId, number>>;
  incoming: Partial<Record<ResourceId, number>>;
  staff: number;
  eff: number;
  powered: boolean;
  heated: boolean;
  connected: boolean;      // reachable (road OR off-road) from the depot network
  roadConnected: boolean;  // reachable specifically by ROAD (drives the "off-road, slow" advisory)
  coalFactor: number;
  farmFields: number;
  priorityHigh?: boolean;
  buildPriority?: -1 | 0 | 1; // construction crew priority: Low | Normal | High (default 0/undefined = Normal); only meaningful while !constructed
  autoBought?: boolean;     // materials imported (bonded) not domestic — paid at placement, or at commence for a planned site
  bondedCustomsId?: number; // the customs house those bonded goods ship from
  importCurrency?: 'east' | 'west'; // which bloc auto-buy pays (default east)
  foreignLabor?: boolean;   // per-site: may hire paid foreign builders beyond citizens (default = menu default)
  paused?: boolean;         // planning mode: placed but not commenced — draws no materials/builders
}

export interface HappinessFactor {
  id: string;
  label: string;
  icon: string;
  satPct: number;       // 0..100%
  weightPct: number;    // e.g. 30 for food
  effectivePct: number; // satPct * (weightPct / 100)
}

export interface HappinessBreakdown {
  overall: number;             // Current smoothed happiness (0..100)
  target: number;              // Raw un-lerped target happiness
  factors: HappinessFactor[];
  modifiers: {
    pollutionPenaltyPct: number; // e.g. 5 if pollution penalty applies
    weatherMoralePct: number;    // e.g. +2 or -6
  };
}

/** Placement policy stamped onto a new site — defaults come from the build-menu toggles. */
export interface PlacePolicy {
  instant?: boolean;          // $ Western prefab, completes immediately (no site)
  autoBuy?: boolean;          // import the material bill rather than use domestic stock
  currency?: 'east' | 'west'; // auto-buy pays ₽ (east) or $ (west)
  foreignLabor?: boolean;     // may hire paid foreign builders (default = engine.foreignLaborEnabled)
  plan?: boolean;             // place paused (planning mode) — commence later
}

/**
 * Anything that moves along a tile-space polyline: barges and foreign lorries.
 * These really are shipments — created for one delivery, retired on arrival —
 * and that is the right model for them. Road vehicles are NOT movers; see
 * `Vehicle`.
 */
export interface Mover {
  id: number;
  points: { x: number; y: number }[]; // tile-space polyline incl. building centers
  cargo: ResourceId;
  amount: number;
  daysTotal: number;
  daysDone: number;
  phase: 'go' | 'back';
  destId: number;
  srcId: number; // undelivered cargo returns here
}

/** What a road vehicle is doing right now. `idle` means parked at `atId`. */
export type VehicleState = 'idle' | 'toPickup' | 'toDeliver' | 'returning' | 'toRefuel';

/**
 * A road vehicle: a persistent machine owned by a garage, not a per-delivery
 * ticket. It exists from the day its garage is staffed until the garage is
 * demolished, carries its own fuel between jobs, parks where it finished, and
 * drives to a pump when it runs low.
 *
 * This is why there is no pooled fleet tank and no per-day fleet fuel levy:
 * fuel leaves a building's bin exactly once, when a vehicle pumps it, and the
 * gauge on the inspector is that vehicle's actual tank. A shipment-shaped
 * truck (created at dispatch, spliced on return) could never make a fuel
 * gauge mean anything, which is what the old per-truck fuel fields were.
 *
 * A vehicle is always either parked at a building (`atId`) or driving a single
 * leg that ends at one (`destId`). It never re-routes mid-leg, so a failed
 * route can never strand it on a stale path.
 */
export interface Vehicle extends Mover {
  homeId: number;   // the garage that owns it
  atId: number;     // building it is parked at (meaningful while `state === 'idle'`)
  /**
   * The building the CURRENT leg ends at. Differs from `destId` only while
   * running empty out to a supplier: `destId` stays the job's real destination
   * for the whole task, so anything asking "where is this load going?" gets a
   * stable answer whichever leg the lorry is on.
   */
  legTo: number;
  state: VehicleState;
  fuel: number;
  fuelCap: number;
  odometer: number; // lifetime road-tile-equivalents driven
  /** Live road-tile-equivalents per day on the current leg — 0 when parked.
   *  Derived, never authored: an off-road or storm-slowed leg reads slower
   *  because it IS slower. (Deliberately not km/h: a day covers ~5 tiles, so
   *  any real-world unit here would be a fiction the sim cannot back.) */
  speed: number;
  /** Road-tile-equivalents in the current leg — off-road legs cost more of
   *  these per map tile, so fuel and travel time scale together. */
  legTiles: number;
  /**
   * The loaded supplier→destination leg, routed at DISPATCH and driven once
   * the lorry has collected. Held rather than re-routed on arrival so the
   * supplier's ranked access-tile tie-breaking decides the path actually
   * driven — and so a delivery costs one pathfinding search, not two.
   */
  pendingPath?: { x: number; y: number }[];
  pendingTiles?: number;
  /** True once the tank ran dry mid-leg: it crawls to the end at limpSpeedMult. */
  limping?: boolean;
}

/** Legacy alias: saves, the renderer and the boat/foreign-lorry paths all
 *  speak `Truck`. Road vehicles are the richer `Vehicle`. */
export type Truck = Mover;

export function truckWorldPos(tr: Mover): { wx: number; wy: number } {
  const pts = tr.points;
  const segs = pts.length - 1;
  if (segs <= 0) return { wx: pts[0]?.x ?? 0, wy: pts[0]?.y ?? 0 };
  const frac = Math.min(1, tr.daysDone / Math.max(0.1, tr.daysTotal));
  const f = frac * segs;
  if (tr.phase === 'go') {
    const i = Math.min(segs - 1, Math.floor(f));
    const t = f - i;
    const p1 = pts[i], p2 = pts[i + 1];
    if (!p1 || !p2) return { wx: pts[0]?.x ?? 0, wy: pts[0]?.y ?? 0 };
    return { wx: p1.x + (p2.x - p1.x) * t, wy: p1.y + (p2.y - p1.y) * t };
  } else {
    const revI = Math.min(segs - 1, Math.floor(f));
    const t = f - revI;
    const i = segs - revI;
    const p1 = pts[i], p2 = pts[i - 1];
    if (!p1 || !p2) return { wx: pts[0]?.x ?? 0, wy: pts[0]?.y ?? 0 };
    return { wx: p1.x + (p2.x - p1.x) * t, wy: p1.y + (p2.y - p1.y) * t };
  }
}

/** A barge sailing between ports — same lifecycle as a truck, on water. */
export type Boat = Truck;

/** Standing freight order: sail `amt` of `r` from one port to another once the goods arrive portside. */
export interface BoatOrder { srcId: number; destId: number; r: ResourceId; amt: number }

export interface Alert {
  id: string;
  icon: string;
  text: string;
  level: 'warn' | 'bad';
}

export interface GameEvent {
  id: number;
  text: string;
  kind: 'good' | 'bad' | 'info';
  icon?: string; // game icon name (see src/ui/icons)
}

export type Season = 'winter' | 'spring' | 'summer' | 'autumn';

/** A standing order of the Foreign Trade Directorate for one resource. */
export interface AutoTradeRule {
  mode: 'import' | 'export';
  level: number; // import: keep town stock at >= level; export: sell surplus above level
  currency: 'east' | 'west';
}

/** One day's page of the customs ledger (auto-trade only; manual trades stay toasts). */
export interface TradeDayLedger {
  imports: Partial<Record<ResourceId, number>>;
  exports: Partial<Record<ResourceId, number>>;
  rubles: number;   // net treasury change from automated trade
  dollars: number;
  used: number;     // customs throughput consumed
  capacity: number; // customs throughput available (staffing-scaled)
  blocked: string[];
  foreignLabor: number; // ₽ paid to imported construction crews (backward-compatible alias = foreignLaborRubles)
  foreignLaborRubles: number; // ₽ paid to imported construction crews (negative = spent)
  foreignLaborDollars: number; // $ paid to imported construction crews (negative = spent)
  repairImports: number; // ₽/$ paid to import machinery for facility repairs (negative = spent)
}

export const emptyLedger = (): TradeDayLedger =>
  ({ imports: {}, exports: {}, rubles: 0, dollars: 0, used: 0, capacity: 0, blocked: [], foreignLabor: 0, foreignLaborRubles: 0, foreignLaborDollars: 0, repairImports: 0 });

/** A deadline bulk order from one of the blocs, at a premium price locked when offered. */
export interface Contract {
  id: number;
  r: ResourceId;
  bloc: 'east' | 'west';
  amount: number;
  delivered: number;
  pricePerUnit: number;    // market price at offer time * (1 + premium)
  deadlineIdx: number;     // absolute day index (see dayIndex())
  offerExpiresIdx: number; // unaccepted offers are withdrawn after this day
  state: 'offer' | 'active' | 'done' | 'failed';
  closedIdx?: number;      // when it reached done/failed (for pruning the history)
}

/** A foreign currency advance from one of the two blocs. */
export interface Loan {
  id: number;
  bloc: 'east' | 'west';
  principal: number;        // original amount borrowed
  totalOwed: number;        // principal × (1 + interest rate)
  repaid: number;           // cumulative amount repaid so far
  takenDayIdx: number;      // absolute day index when taken
  deadlineDayIdx: number;   // absolute day index when due
  tierIndex: number;        // 0/1/2 = Small/Medium/Large
  state: 'active' | 'repaid' | 'defaulted';
}


export interface TilePatch {
  x: number;
  y: number;
  terrain?: Tile['terrain'];
  deposit?: DepositType | null;
  road?: boolean;
  foreign?: boolean;
  variant?: number;
}

export type InternalTilePatch = TilePatch & { buildingId?: number | null };

export interface RoutingDiagnostics {
  dayIndex: number;
  demandsConsidered: number;
  successfulDispatches: number;
  componentRejections: number;
  roadSearches: number;
  landSearches: number;
  waterSearches: number;
  supplierCandidatesChecked: number;
  settledTiles: number;
  pathsMaterialized: number;
  topologyRebuilds: { road: number; land: number; water: number };
}

/** Queue position for a building that authored none — behind everything that
 *  did. Ties fall back to commissioning order, which is deterministic. */
export const DEFAULT_ALLOCATION_PRIORITY = 900;

export function sameRevisions(a: readonly number[], b: readonly number[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/**
 * Memoizes a sim-internal derived value against a tuple of *dimension revisions*,
 * recomputing only when the tuple changes. Dependencies are declared as data (a
 * revision reader), so a cache can never key on the wrong signal — the whole point
 * of the taxonomy: sim-internal state keys on structural revisions (topology
 * domains, facilityRevision), never on `version` (which is only the UI-repaint
 * signal). `compute` may return a value or run a side effect (T = void).
 */
export class RevisionMemo<T> {
  private key: readonly number[] | null = null;
  private value!: T;
  private readonly deps: () => readonly number[];
  private readonly compute: () => T;
  constructor(deps: () => readonly number[], compute: () => T) {
    this.deps = deps;
    this.compute = compute;
  }

  get(): T {
    const k = this.deps();
    if (this.key === null || !sameRevisions(this.key, k)) {
      this.value = this.compute();
      this.key = k;
    }
    return this.value;
  }
}

/** Build a bounded-search goal list from a facility's access tiles, ranked by
 *  (buildingRank, then tile order). `value` is shared across all of a facility's
 *  tiles (created once by the caller, not per tile). */
export function rankedGoals<T>(tiles: readonly TopologyPos[], buildingRank: number, value: T): RankedGoal<T>[] {
  return tiles.map((tile, accessRank) => ({ x: tile.x, y: tile.y, value, buildingRank, accessRank }));
}

/** True when any of the building's machinery (wear) bins ran dry — it then
 *  runs at BALANCE.wornEffMult until spares arrive. Pure; UI-safe. */
export function buildingWorn(b: BuildingInst): boolean {
  const wear = BUILDINGS[b.defId].wear;
  if (!wear) return false;
  return (Object.keys(wear) as ResourceId[]).some(r => (b.stock[r] ?? 0) < 1e-6);
}
