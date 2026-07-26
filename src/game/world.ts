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
import {
  ALL_RESOURCES, BALANCE, BUILDINGS, CLIMATES, DIFFICULTIES, FARM_SEASON, IMPORT_MARKUP,
  LOANS, POWER_SECTORS, RESOURCES, WEATHER,
} from './config';
import type { Category, ClimateId, DepositType, DifficultyId, ResourceId } from './config';
import { mulberry32 } from '@/lib/rng';
import type { BorderEdge, Tile } from './mapgen';
import type { SeededRng } from '@/lib/rng';
import { FloodResult, floodCost, shortestPathToAny } from './pathfind';
import type { NearestPath, RankedGoal } from './pathfind';
import { TopologyIndex } from './topology';
import { shareAnyComponent, unionComponents } from './topology';
import type { RoutingTile, TopologyAccess, TopologyDomain, TopologyPos } from './topology';
import { WeatherTimeline } from './weather';
import type { DayWeather } from './weather';
// Type-only, so World and Mutation never form a runtime cycle: the systems
// import both at runtime, neither imports the other's code.
import type { Mutation } from './mutation';

export type DemandKind =
  | 'plantFuel' | 'heatFuel' | 'fleetFuel'   // lifeline
  | 'shopGoods'                              // consumer
  | 'factoryInput' | 'wear'                  // industry
  | 'construction'
  | 'housekeeping';                          // overflow / export staging — prevents no downtime

export const DEMAND_CATEGORY: Record<DemandKind, LogisticsCategory> = {
  plantFuel: 'lifeline', heatFuel: 'lifeline', fleetFuel: 'lifeline',
  shopGoods: 'consumer',
  factoryInput: 'industry', wear: 'industry',
  construction: 'construction',
  housekeeping: 'industry',
};

export interface LogisticsDemand {
  b: BuildingInst;
  r: ResourceId;
  amt: number;
  kind: DemandKind;
  /** Cached score for the dispatch loop; recomputed when the destination's incoming changes. */
  score?: number;
  /**
   * A cross-water relay leg is addressed to a PORT, which consumes nothing and
   * would therefore score zero on its own. It carries the urgency of the real
   * destination it is serving instead, so island deliveries keep the priority
   * of whatever is actually running dry.
   */
  relayScore?: number;
  /** Days of operation the destination has left for this resource (Infinity = never drains). */
  cover?: number;
  from?: number;
  noCustomsSrc?: boolean;
  bonded?: boolean;
  repairImport?: 'east' | 'west';
}

/** One dispatch pass's straight-line ETA scratch. Local to the pass, never engine state. */
export interface EtaPass {
  cache: Map<number, number>;
  storages: { x: number; y: number }[];
}

export interface IndexedFacility {
  b: BuildingInst;
  buildingRank: number;
  isCustoms: boolean;
  road: TopologyAccess;
  land: TopologyAccess;
}

export interface ComponentAvailability {
  all: Map<number, number>;
  nonCustoms: Map<number, number>;
}

export interface SupplierCandidate {
  facility: IndexedFacility;
  active: boolean;
}

export interface ResourceSupplyState {
  candidates: SupplierCandidate[];
  road: ComponentAvailability;
  land: ComponentAvailability;
}

export interface LogisticsRoutingContext {
  facilities: Map<number, IndexedFacility>;
  orderedFacilities: IndexedFacility[];
  /** Lazily built per resource on first demand (see ensureSupply). */
  supply: Map<ResourceId, ResourceSupplyState>;
  /** Resources whose supply has been built — memoizes even the no-supplier case. */
  builtResources: Set<ResourceId>;
  /** Topology revisions captured at build; the context's cached component IDs are
   *  only comparable within this generation, so a mid-pass rebuild must throw. */
  roadRevision: number;
  landRevision: number;
}

export interface SupplyPick {
  supplier: BuildingInst;
  candidate: SupplierCandidate | null;
  path: { x: number; y: number }[];
  cost: number;
}

/** Controlled setup/debug mutation. Footprint ownership is deliberately absent:
 * buildingId is owned exclusively by GameEngine's placement lifecycle. */

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

/** Player-facing grouping of demand kinds — the four dials on the Delivery panel. */
export type LogisticsCategory = 'lifeline' | 'consumer' | 'industry' | 'construction';

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

/**
 * A `RevisionMemo` whose computation is supplied by the caller instead of the
 * constructor. That is what lets an expensive system cache its mutation list on
 * the World it reads — the state lives with the state, the code lives with the
 * system, and World never imports a system module.
 */
export class RevisionCache<T> {
  private key: readonly number[] | null = null;
  private value!: T;

  get(deps: readonly number[], compute: () => T): T {
    if (this.key === null || !sameRevisions(this.key, deps)) {
      this.value = compute();
      this.key = deps;
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

/**
 * The simulated world: the map, what stands on it, and the primitive operations
 * every system needs over both.
 *
 * Split out of GameEngine so a system can be a module that takes a World rather
 * than a method on the class that runs the day loop. What lives here is
 * deliberately narrow — state, plus the accessors that read and mutate it
 * safely. Deciding anything (who keeps power, what a lorry hauls, whether a
 * site may be built) belongs to a system, not to the world it acts on.
 *
 * Tile mutation is funnelled: `applyInternalTilePatches` is the only writer, so
 * the routing topology can never be invalidated silently, and footprints are
 * owned exclusively by add/removeBuilding.
 */
export class World {
  private _tiles: Tile[][];
  /** Read-only world view. All first-party mutations go through the patch API so
   * the routing topology is never invalidated silently. The `Readonly<Tile>`
   * element type rejects `world.tiles[y][x].road = …` at compile time. NOTE the
   * residual gaps: TypeScript widens `Readonly<Tile>` back to a mutable `Tile` on
   * assignment (a known soundness hole), and the dev console can mutate at
   * runtime. mutation-guards.test.ts covers the realistic regression vector. */
  get tiles(): readonly (readonly Readonly<Tile>[])[] { return this._tiles; }

  readonly mapW: number;
  readonly mapH: number;
  /** Which map edge is the national border; null on bare test maps. */
  readonly borderEdge: BorderEdge | null;
  hasWater = false;

  readonly topology: TopologyIndex;
  /** Bumped whenever the set of buildings changes, so derived indexes rebuild. */
  facilityRevision = 0;
  /** Today's routing work — the diagnostics panel and the performance tripwire. */
  routingDay: Omit<RoutingDiagnostics, 'topologyRebuilds'> = {
    dayIndex: 0,
    demandsConsidered: 0,
    successfulDispatches: 0,
    componentRejections: 0,
    roadSearches: 0,
    landSearches: 0,
    waterSearches: 0,
    supplierCandidatesChecked: 0,
    settledTiles: 0,
    pathsMaterialized: 0,
  };

  buildings = new Map<number, BuildingInst>();
  nextBuildingId = 1;

  // ---------------- calendar & weather ----------------

  day = 1; month = 3; year = 1960;
  readonly seed: number;
  /** Start conditions only — except the import price multiplier, which the
   *  border charges every day, so the sim does read it. */
  readonly difficulty: DifficultyId;
  /** The economy's stream. Contracts and the weather timeline draw from their
   *  own decorrelated streams, so this one has exactly one call site. */
  rng: SeededRng;
  private timeline: WeatherTimeline;
  /** Test/debug seam: overlays the deterministic timeline (helpers force calm weather). */
  weatherScript?: (dayIndex: number) => Partial<DayWeather>;
  weather: DayWeather;
  dryStreak = 0;   // hot rainless days in a row (drought)
  gloomStreak = 0; // miserable-weather days in a row (morale)
  sunStreak = 0;
  wasFrost = false;

  // ---------------- the republic's condition ----------------

  pop = 0;
  capacity = 0;
  workers = 0;
  employed = 0;
  jobs = 0;
  happiness = 70;
  sat = { food: 1, clothes: 1, power: 1, heat: 1, culture: 0, health: 0, employment: 1, pollution: 1 };
  powerProduced = 0; powerDemand = 0;
  heatProduced = 0; heatDemand = 0;
  totals: Record<ResourceId, number> = Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>;
  stats = {
    produced: Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>,
    /** Cumulative customs imports per resource (objective metric). */
    imported: {} as Partial<Record<ResourceId, number>>,
    exportedValue: 0,
    roadsBuilt: 0, // cumulative COMPLETED road tiles (objective metric; never decremented)
  };
  objectivesDone: string[] = [];
  alerts: Alert[] = [];

  /** The notice board. Systems never push directly — they emit an `event`
   *  mutation, and `applyMutations` calls this — so ids stay in emission order.
   *  `GameEngine.drainEvents()` is the single (destructive) consumer. */
  events: GameEvent[] = [];
  private nextEventId = 1;
  pushEvent(text: string, kind: GameEvent['kind'], icon?: string): void {
    this.events.push({ id: this.nextEventId++, text, kind, icon });
  }

  // ---------------- the fleet ----------------

  /** The road fleet. Persistent machines owned by garages — see `Vehicle`. */
  trucks: Vehicle[] = [];
  boats: Boat[] = [];
  /** Cosmetic border traffic: foreign lorries visiting the customs on trade days. */
  foreignTrucks: Mover[] = [];
  boatOrders: BoatOrder[] = [];
  nextTruckId = 1;
  nextBoatId = 1;

  // ---------------- the ledger and standing policy ----------------

  // Foreign currency only — nothing domestic ever charges the treasury.
  // The real starting grants come from DIFFICULTIES when the engine is built.
  rubles = 0;
  dollars = 0;
  priceFactorEast = 1;
  priceFactorWest = 1;
  /** National auto-trade policy — mutate only via the setAutoTrade* methods. */
  autoTrade = {
    enabled: false,
    reserveRubles: BALANCE.autoReserveRubles,
    reserveDollars: BALANCE.autoReserveDollars,
    rules: {} as Partial<Record<ResourceId, AutoTradeRule>>,
  };
  tradeLedger = { today: emptyLedger(), yesterday: emptyLedger() };
  /** Global construction master switch. Off = all construction and material dispatches paused. */
  globalConstructionEnabled = true;
  /** Hire imported construction crews with ₽ for builders beyond your citizens.
   *  Off = domestic builders only (construction stalls without staffed offices). */
  foreignLaborEnabled = true;
  foreignLaborCurrency: 'east' | 'west' = 'east';
  /** Import machinery from the border (paid ₽/$) to repair a worn building when no
   *  domestic machinery can reach it — a town with no Machine Works is then never
   *  permanently stuck at half output. Off = domestic supply only. */
  repairImportsEnabled = true;
  repairImportCurrency: 'east' | 'west' = 'east';
  /** Offers, active deals and recent history — mutate only via accept/declineContract. */
  contracts: Contract[] = [];
  nextContractId = 1;
  /** 0..cap price malus per bloc from failed contracts; decays daily. */
  relationsPenalty = { east: 0, west: 0 };
  /** Active, repaid and recently-defaulted loans. */
  loans: Loan[] = [];
  nextLoanId = 1;
  /** Auto-repay: when treasury exceeds threshold, chip away at active loans. */
  loanAutoRepay = {
    enabled: false,
    thresholdRubles: LOANS.autoRepayThresholdRubles,
    thresholdDollars: LOANS.autoRepayThresholdDollars,
  };
  /** Per-bloc cooldown: absolute dayIndex when borrowing is allowed again. */
  loanCooldown = { east: 0, west: 0 };

  /** Global category construction priorities (Low -1 / Normal 0 / High 1).
   *  Applies to all sites in that category unless overridden on the individual site. */
  globalCategoryPriorities: Record<Category, -1 | 0 | 1> = {
    infra: 0,
    housing: 0,
    industry: 0,
    services: 0,
    trade: 0,
  };

  /**
   * What the republic values, per demand category. Dispatch rank is otherwise
   * derived entirely from the sim (days of cover vs. delivery time), so these
   * dials are the player's whole control surface: they scale consequence, they
   * never override urgency. 1 = neutral.
   */
  logisticsCategoryWeights: Record<LogisticsCategory, number> = {
    lifeline: 1, consumer: 1, industry: 1, construction: 1,
  };
  /** Enable automatic emergency fuel imports at Customs House when city fuel is dry. */
  emergencyFuelAutoBuy = true;

  /**
   * Who the grid keeps lit when generation falls short, worst-served last.
   *
   * Deliberately the player's, not the engine's: a brownout is a choice about
   * whether the plan or the people come first, and that is the one question a
   * planned-economy game should never answer on the player's behalf. The engine
   * only decides the order WITHIN a sector (see `allocationPriority`).
   */
  powerSectorOrder: Category[] = [...POWER_SECTORS];

  /** The constructed customs house nearest (Manhattan) a tile — where an
   *  auto-bought site's bonded materials ship from. Deterministic. */
  nearestConstructedCustoms(x: number, y: number): BuildingInst | undefined {
    let best: BuildingInst | undefined;
    let bestD = Infinity;
    for (const b of this.buildings.values()) {
      if (!this.def(b).isCustoms || !b.constructed) continue;
      const d = Math.abs(b.x + b.w / 2 - x) + Math.abs(b.y + b.h / 2 - y);
      if (d < bestD) { bestD = d; best = b; }
    }
    return best;
  }

  readonly customsComponentsMemo = new RevisionMemo<readonly number[]>(
    () => [this.topology.revision('land'), this.facilityRevision],
    () => {
      const lists: (readonly number[])[] = [];
      for (const b of this.buildings.values()) {
        if (b.constructed && this.def(b).isCustoms) lists.push(this.landAccess(b).components);
      }
      return Object.freeze(unionComponents(...lists));
    },
  );
  customsComponents(): readonly number[] { return this.customsComponentsMemo.get(); }

  /** Customs-connected buildings and how much each is willing to sell (supplyOf-protected). */
  sellableSources(r: ResourceId): { b: BuildingInst; amt: number }[] {
    const customs = this.customsComponents();
    if (!customs.length) return [];
    const out: { b: BuildingInst; amt: number }[] = [];
    for (const b of this.buildings.values()) {
      if (!b.constructed) continue;
      const amt = this.supplyOf(b, r);
      if (amt < 0.01) continue;
      if (shareAnyComponent(this.landAccess(b).components, customs)) out.push({ b, amt });
    }
    return out;
  }

  /**
   * What `b` WANTS to consume of `r` per day — its demand, not its flow.
   *
   * Deliberately NOT `productionRates()`: that reports what a building actually
   * manages to consume, which throttles to zero the moment a bin runs dry
   * (`inputFactor` for factories, `coalFactor` for plants). Driving delivery
   * urgency off actual flow would mean a starved building reports needing
   * nothing and is never resupplied again — a genuine deadlock. Cover must be
   * measured against intent.
   */
  nominalInputRate(b: BuildingInst, r: ResourceId): number {
    const def = this.def(b);
    if (!b.constructed) return 0;
    const wear = def.wear?.[r] ?? 0;
    const input = def.inputs?.[r] ?? 0;
    // Plants: b.eff carries staffing; coalFactor (fuel on hand) is excluded.
    if (def.powerOutput || def.heatOutput) {
      const isFuel = def.inputs ? (Object.keys(def.inputs)[0] as ResourceId) === r : false;
      return ((isFuel ? input : 0) + wear) * b.eff;
    }
    if (!def.outputs) return 0;
    // nominalEff, not baseEff: a building stalled by an outage still wants its
    // inputs staged for the moment the lights come back on.
    return (input + wear) * this.outputMultiplier(b, this.nominalEff(b));
  }

  /** Daily citizen demand for a resource (what stores would sell at full coverage). */
  citizenDemandOf(r: ResourceId): number {
    if (r === 'food') return this.pop * BALANCE.foodPerCitizen;
    if (r === 'clothes') return this.pop * BALANCE.clothesPerCitizen;
    return 0;
  }

  // ---------------- the fleet, counted ----------------

  officeTrucks(): number {
    let n = 0;
    for (const b of this.buildings.values()) if (this.def(b).isConstructionOffice) n += this.trucksFrom(b);
    return n;
  }

  /** Vehicles the Motor Depots crew — one per staffed driver. */
  driverTrucks(): number {
    let n = 0;
    for (const b of this.buildings.values()) if (this.def(b).isMotorDepot) n += this.trucksFrom(b);
    return n;
  }

  /** Fuel standing in pumps the fleet can actually reach (Gas Stations, Motor
   *  Depots, Construction Offices). Customs fuel is the emergency reserve and
   *  is reported separately — it is a border terminal, not a filling station. */
  pumpFuel(): number {
    let f = 0;
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if ((def.isGasStation || def.isMotorDepot || def.isConstructionOffice) && b.constructed && b.connected) f += this.stockOf(b, 'fuel');
    }
    return f;
  }

  /** Emergency fuel on hand at connected Customs Houses. */
  customsFuel(): number {
    let f = 0;
    for (const b of this.buildings.values()) {
      if (b.constructed && b.connected && this.def(b).isCustoms) f += this.stockOf(b, 'fuel');
    }
    return f;
  }

  /** Fuel in the fleet's own tanks, right now. */
  tankFuel(): number {
    let f = 0;
    for (const v of this.trucks) f += v.fuel;
    return f;
  }

  /**
   * Public fleet snapshot for the HUD gauge, Logistics panel and advisories.
   *
   * Every figure is counted off the real fleet: `max` is how many vehicles
   * exist, `active` how many are driving, `grounded` how many are parked
   * because their tanks are too low to accept work. There is no derived
   * "fuelled capacity" any more — a vehicle either has fuel or it does not.
   */
  // ---------------- logistics: what is wanted, and who can supply it ----------------

  supplyOf(b: BuildingInst, r: ResourceId): number {
    const def = this.def(b);
    if (!b.constructed) return 0;
    const isStorage = !!(def.isDepot || def.isCustoms || def.isPort || b.defId === 'warehouse');
    const isProducer = (def.outputs?.[r] ?? 0) > 0;
    if (!isStorage && !isProducer) return 0;

    // keep 3 days of production inputs plus a month of wear spares — trucks
    // must never rob one factory's machinery bin to feed another's
    const keep = (def.inputs?.[r] ?? 0) * 3 + (def.wear?.[r] ?? 0) * BALANCE.wearReserveDays;
    // a port's stock already promised to an outstanding barge is earmarked for that
    // leg — trucks can't poach a relayed import mid-hop (dispatchBoats loads via
    // stockOf, so the barge still gets its reservation).
    const reserved = def.isPort
      ? this.boatOrders.reduce((s, o) => (o.srcId === b.id && o.r === r ? s + o.amt : s), 0)
      : 0;
    return Math.max(0, this.stockOf(b, r) - keep - reserved);
  }

  buildLogisticsRoutingContext(): LogisticsRoutingContext {
    const facilities = new Map<number, IndexedFacility>();
    const orderedFacilities: IndexedFacility[] = [];
    let buildingRank = 0;
    for (const b of this.buildings.values()) {
      const facility: IndexedFacility = {
        b,
        buildingRank: buildingRank++,
        isCustoms: !!this.def(b).isCustoms,
        road: this.roadAccess(b),
        land: this.landAccess(b),
      };
      facilities.set(b.id, facility);
      orderedFacilities.push(facility);
    }
    // Supply candidates are built lazily per resource (ensureSupply), so a pass only
    // pays for the resources actually demanded — not ALL_RESOURCES × every building.
    return {
      facilities,
      orderedFacilities,
      supply: new Map(),
      builtResources: new Set(),
      roadRevision: this.topology.revision('road'),
      landRevision: this.topology.revision('land'),
    };
  }

  /** Fails loudly if the topology was invalidated after the context was built: its
   *  cached component IDs would then reference a stale labelling (cf. FloodResult).
   *  Nothing invalidates mid-pass today (construction runs after logistics) — this
   *  turns a would-be silent misroute into a throw if that ever changes. */
  assertRoutingFresh(ctx: LogisticsRoutingContext): void {
    if (ctx.roadRevision !== this.topology.revision('road') ||
        ctx.landRevision !== this.topology.revision('land')) {
      throw new Error('Stale routing context: topology changed mid-logistics-pass');
    }
  }

  /** Apply ±delta to a facility's component ref-counts in a resource's availability
   *  maps (one traversal shared by the build (+1) and deactivate (−1) paths). */
  applyAvailability(state: ResourceSupplyState, facility: IndexedFacility, delta: number): void {
    const apply = (map: Map<number, number>, component: number) => {
      const next = (map.get(component) ?? 0) + delta;
      if (next > 0) map.set(component, next); else map.delete(component);
    };
    for (const component of facility.road.components) {
      apply(state.road.all, component);
      if (!facility.isCustoms) apply(state.road.nonCustoms, component);
    }
    for (const component of facility.land.components) {
      apply(state.land.all, component);
      if (!facility.isCustoms) apply(state.land.nonCustoms, component);
    }
  }

  /** Build (once, memoized) the supplier candidates + component availability for a
   *  resource. Returns undefined when no building supplies it — the no-supplier case
   *  is memoized too, so repeated undelivered demands don't re-scan. Candidate order
   *  is buildings.values() order, byte-identical to the former eager build. */
  ensureSupply(ctx: LogisticsRoutingContext, r: ResourceId): ResourceSupplyState | undefined {
    if (ctx.builtResources.has(r)) return ctx.supply.get(r);
    ctx.builtResources.add(r);
    let state: ResourceSupplyState | undefined;
    for (const facility of ctx.orderedFacilities) {
      if (this.supplyOf(facility.b, r) < 1) continue;
      if (!state) {
        state = {
          candidates: [],
          road: { all: new Map(), nonCustoms: new Map() },
          land: { all: new Map(), nonCustoms: new Map() },
        };
        ctx.supply.set(r, state);
      }
      const candidate: SupplierCandidate = { facility, active: true };
      state.candidates.push(candidate);
      this.applyAvailability(state, facility, 1);
    }
    return state;
  }

  deactivateSupplyCandidate(
    ctx: LogisticsRoutingContext,
    candidate: SupplierCandidate,
    r: ResourceId,
  ): void {
    if (!candidate.active || this.supplyOf(candidate.facility.b, r) >= 1) return;
    candidate.active = false;
    const state = ctx.supply.get(r);
    if (!state) return;
    this.applyAvailability(state, candidate.facility, -1);
  }

  routeToSupply(
    ctx: LogisticsRoutingContext,
    d: LogisticsDemand,
    domain: 'road' | 'land',
  ): SupplyPick | null {
    this.assertRoutingFresh(ctx);
    const destination = ctx.facilities.get(d.b.id);
    const destAccess = destination?.[domain];
    if (!destAccess?.tiles.length) {
      this.routingDay.componentRejections++;
      return null;
    }

    // Non-bonded demand routes through supply candidates (built lazily for d.r);
    // bonded demand draws from customs directly and never touches `state`.
    const state = d.bonded ? undefined : this.ensureSupply(ctx, d.r);
    if (!d.bonded) {
      if (!state) {
        this.routingDay.componentRejections++;
        return null;
      }
      const availability = d.noCustomsSrc ? state[domain].nonCustoms : state[domain].all;
      if (!destAccess.components.some(component => (availability.get(component) ?? 0) > 0)) {
        this.routingDay.componentRejections++;
        return null;
      }
    }

    type GoalValue = { facility: IndexedFacility; candidate: SupplierCandidate | null };
    const goals: RankedGoal<GoalValue>[] = [];
    const addFacility = (facility: IndexedFacility, candidate: SupplierCandidate | null) => {
      this.routingDay.supplierCandidatesChecked++;
      if (facility.b.id === d.b.id) return;
      if (d.from !== undefined && facility.b.id !== d.from) return;
      if (d.noCustomsSrc && facility.isCustoms) return;
      if (candidate) {
        this.deactivateSupplyCandidate(ctx, candidate, d.r);
        if (!candidate.active) return;
      } else if (!d.bonded && this.supplyOf(facility.b, d.r) < 1) {
        return;
      }
      const access = facility[domain];
      if (!shareAnyComponent(access.components, destAccess.components)) return;
      goals.push(...rankedGoals(access.tiles, facility.buildingRank, { facility, candidate }));
    };

    if (d.bonded) {
      if (d.from !== undefined) {
        const facility = ctx.facilities.get(d.from);
        if (facility) addFacility(facility, null);
      } else {
        for (const facility of ctx.orderedFacilities) addFacility(facility, null);
      }
    } else if (state) {
      if (d.from !== undefined) {
        const candidate = state.candidates.find(c => c.facility.b.id === d.from);
        if (candidate) addFacility(candidate.facility, candidate);
      } else {
        for (const candidate of state.candidates) addFacility(candidate.facility, candidate);
      }
    }

    if (!goals.length) {
      this.routingDay.componentRejections++;
      return null;
    }
    const result = this.nearestPath(domain, destAccess.tiles, goals);
    if (!result) return null; // defensive: shared components make this unreachable in a valid topology
    return {
      supplier: result.goal.value.facility.b,
      candidate: result.goal.value.candidate,
      path: result.path,
      cost: result.cost,
    };
  }

  roadSupplierReaches(
    ctx: LogisticsRoutingContext,
    r: ResourceId,
    destination: IndexedFacility,
  ): boolean {
    this.assertRoutingFresh(ctx);
    const state = this.ensureSupply(ctx, r);
    if (!state || !destination.road.components.some(c => state.road.all.has(c))) return false;
    for (const candidate of state.candidates) {
      this.routingDay.supplierCandidatesChecked++;
      this.deactivateSupplyCandidate(ctx, candidate, r);
      if (!candidate.active || candidate.facility.b.id === destination.b.id) continue;
      if (shareAnyComponent(candidate.facility.road.components, destination.road.components)) return true;
    }
    return false;
  }

  /**
   * Per-day consumption of `r` at `b` under today's conditions — the drain that
   * turns a stock level into "days of operation left".
   *
   * Reuses `productionRates()` (the documented single source of truth that
   * `production()` applies verbatim), so dispatch urgency can never disagree
   * with what the building actually burns.
   */
  drainRateOf(b: BuildingInst, r: ResourceId, kind: DemandKind): number {
    switch (kind) {
      case 'plantFuel':
      case 'heatFuel':
      case 'factoryInput':
      case 'wear':
        return this.nominalInputRate(b, r);
      case 'shopGoods': {
        // Citizens draw from all stores in aggregate; steady-state per-shop
        // drain is the town's daily appetite spread over the shops that serve it.
        const shops = this.shopCount();
        return shops > 0 ? this.citizenDemandOf(r) / shops : 0;
      }
      case 'fleetFuel':
        // Fleet fuel is POOLED — burnFleetFuel() drains town-wide, fullest
        // first — so a single station's stock is not what keeps trucks rolling.
        // Cover is computed against the pool in coverDaysOf().
        return this.fleetFuelBurnPerDay();
      default:
        return 0; // construction + housekeeping never "run dry"
    }
  }

  /** Number of constructed shops serving citizens (denominator for per-shop drain). */
  shopCount(): number {
    let n = 0;
    for (const b of this.buildings.values()) if (b.constructed && this.def(b).serviceType === 'shop') n++;
    return n;
  }

  /**
   * Town-wide daily fleet fuel requirement, measured at FULL UTILISATION —
   * every vehicle the republic owns, driving all day.
   *
   * Deliberately not live burn: a grounded fleet burns nothing, so keying off
   * observed consumption would report "no fuel needed" precisely when every
   * lorry is parked dry, and the restock would deadlock. Same trap, same
   * answer, as `nominalInputRate` for factories — measure intent, not flow.
   */
  fleetFuelBurnPerDay(): number {
    const tilesPerDay = 1 / BALANCE.truckDaysPerTile;
    return this.trucks.length * tilesPerDay * BALANCE.vehicleFuelPerTile;
  }

  /** Days of operation `b` has left on `r` before it stalls. Infinity = never drains. */
  coverDaysOf(b: BuildingInst, r: ResourceId, kind: DemandKind): number {
    const drain = this.drainRateOf(b, r, kind);
    if (drain <= 1e-9) return Infinity;
    // Fleet fuel is not this bin's problem alone: a vehicle fills at whichever
    // pump it can reach, and what is already in the tanks is fuel the republic
    // does not have to deliver again. Cover is measured against both.
    const have = kind === 'fleetFuel'
      ? this.pumpFuel() + this.tankFuel() + this.incomingOf(b, r)
      : this.stockOf(b, r) + this.incomingOf(b, r);
    return have / drain;
  }

  /**
   * A multi-input factory runs only as long as its SCARCEST input lasts, so
   * hauling the abundant input to a bottlenecked mill prevents no downtime.
   *
   * Scoped deliberately to `factoryInput`: nothing else in the sim couples this
   * way. Machinery wear halves efficiency rather than stopping a building
   * (`wornEffMult`), a store out of clothes still sells food, plants burn a
   * single fuel, and fleet fuel is pooled — all independent.
   *
   * `binding` is when the building actually stalls; `headroom` is how far
   * topping up this resource can push that out before the next input binds.
   */
  inputCoupling(b: BuildingInst, r: ResourceId, kind: DemandKind): { binding: number; headroom: number } {
    const own = this.coverDaysOf(b, r, kind);
    const def = this.def(b);
    if (kind !== 'factoryInput' || !def.inputs || Object.keys(def.inputs).length < 2) {
      return { binding: own, headroom: Infinity };
    }
    let worst = Infinity, second = Infinity;
    for (const i of Object.keys(def.inputs) as ResourceId[]) {
      const c = this.coverDaysOf(b, i, kind);
      if (c < worst) { second = worst; worst = c; } else if (c < second) { second = c; }
    }
    // Not the binding constraint → topping it up buys no uptime at all.
    if (own > worst + 1e-6) return { binding: worst, headroom: 0 };
    return { binding: worst, headroom: Math.max(0, second - worst) };
  }

  /**
   * Days of downtime this load prevents, over the planning horizon.
   *
   * This is the heart of the model. It is ~0 for any destination that was not
   * going to run dry within the horizon (however empty its bin looks), and
   * large for one about to stall — so round-trip cost can divide the score
   * unconditionally without a healthy-but-near demand ever outranking a
   * dying-but-far one. No carve-outs, no bands.
   */
  avertedDaysOf(cover: number, eta: number, loadDays: number): number {
    const H = BALANCE.logisticsHorizonDays;
    if (!Number.isFinite(cover) || cover >= H) return 0;
    const without = H - cover;
    const gap = Math.max(0, eta - cover);            // downtime no delivery can prevent
    const resumeAt = Math.max(eta, cover);
    const withLoad = gap + Math.max(0, H - resumeAt - loadDays);
    return Math.max(0, without - withLoad);
  }

  /** How badly the republic suffers per day this building is stalled. */
  consequenceWeightOf(b: BuildingInst, kind: DemandKind): number {
    const def = this.def(b);
    let base: number;
    switch (kind) {
      case 'plantFuel':
        // Blast radius: a dark plant takes its share of the grid down with it.
        base = BALANCE.consequencePlantFuel * (1 + this.poweredDependents(b));
        break;
      case 'heatFuel':
        // Self-silencing: heatDemandFactor() is 0 above the heating threshold,
        // so heat fuel simply stops competing in summer — no mode, no toggle.
        base = BALANCE.consequenceHeatFuel * this.heatDemandFactor() * (1 + this.def(b).heatOutput! * 0.05);
        break;
      case 'fleetFuel':
        // The fleet hauls everything else, so its collapse is systemic.
        base = BALANCE.consequenceFleetFuel * (1 + Math.min(4, this.driverTrucks() * 0.25));
        break;
      case 'shopGoods':
        base = BALANCE.consequenceShopGoods * (1 + this.pop * 0.01);
        break;
      case 'wear':
        base = BALANCE.consequenceWear * (buildingWorn(b) ? 2 : 1);
        break;
      case 'factoryInput':
        base = BALANCE.consequenceFactoryInput;
        break;
      case 'construction':
        base = BALANCE.consequenceConstruction * (1 + Math.sign(b.buildPriority ?? 0) * 0.5);
        break;
      default:
        return 0; // housekeeping — handled by the opportunistic pass, never ranked here
    }
    void def;
    return base * this.categoryDialOf(kind);
  }

  /**
   * Buildings that would lose power if this plant stalled — its share of the
   * grid, measured against power DEMAND rather than current output. Reading
   * live `powerProduced` would return zero dependents for a plant that has
   * already gone dark, i.e. exactly when restoring it matters most.
   */
  poweredDependents(b: BuildingInst): number {
    const out = this.def(b).powerOutput ?? 0;
    if (out <= 0) return 0;
    let consumers = 0, demand = 0;
    for (const x of this.buildings.values()) {
      if (!x.constructed) continue;
      const p = this.def(x).power;
      if (p > 0) { consumers++; demand += p; }
    }
    if (consumers === 0) return 0;
    return consumers * Math.min(1, out / Math.max(demand, 1e-6));
  }

  categoryDialOf(kind: DemandKind): number {
    const w = this.logisticsCategoryWeights[DEMAND_CATEGORY[kind]];
    return Number.isFinite(w) && w > 0 ? w : 1;
  }

  /**
   * Dispatch score — downtime prevented per truck-day. HIGHER is served first
   * (the old band table was lower-first; this is the opposite convention).
   * `eta` is one-way delivery days; pass a cheap estimate for pre-ranking and
   * the routed value once a path is known.
   */
  dispatchScore(d: LogisticsDemand, eta: number): number {
    if (d.relayScore !== undefined) return d.relayScore;
    if (d.kind === 'housekeeping') return 0;
    const roundTrip = Math.max(0.6, eta * 2);

    if (d.kind === 'construction') {
      // A site is not losing anything while it waits — it is failing to GAIN,
      // and it gains NOTHING until it is finished. So the value of a load is
      // flat (no "how empty" term to reshuffle the build order) and, unlike
      // every other kind, round-trip cost is deliberately left out: dividing by
      // distance spreads materials across whichever sites happen to sit nearest
      // the depot, so a dozen sites crawl in parallel and none gets a roof.
      // Equal scores fall back to commissioning order, which finishes sites one
      // at a time — and an early completion compounds, because it staffs up and
      // pays for the next one. Tier still separates High from Low.
      //
      // Nothing escalates inside this category, and that is load-bearing. Both
      // obvious escalations — by nearness to completion and by days blocked —
      // measurably starve NEW sites: whoever is already ahead (or has waited
      // longest, including on a material nobody produces yet) permanently
      // outranks a large site placed later, and the campaign's steel mill was
      // never built under either. Construction-vs-industry is balanced by the
      // category weight, which scales every site equally.
      return this.consequenceWeightOf(d.b, d.kind);
    }

    const drain = this.drainRateOf(d.b, d.r, d.kind);

    if (d.kind === 'wear' && buildingWorn(d.b)) {
      // An already-worn building is not at RISK of stalling — it is losing
      // output right now, every day, at `wornEffMult`. There is no cover to
      // compute and no drain rate to wait on: the damage is present tense, so
      // restoring it is worth the whole horizon. (A healthy spare bin falls
      // through to the normal cover logic below and usually scores ~0.)
      const restored = drain > 1e-9
        ? Math.min(Math.min(d.amt, BALANCE.truckCapacity) / drain, BALANCE.logisticsHorizonDays)
        : BALANCE.logisticsHorizonDays;
      return this.consequenceWeightOf(d.b, d.kind) * restored / roundTrip;
    }

    if (drain <= 1e-9) return 0;
    const { binding, headroom } = this.inputCoupling(d.b, d.r, d.kind);
    if (headroom <= 0) return 0; // abundant input to a bottlenecked factory
    const load = Math.min(d.amt, BALANCE.truckCapacity);
    const loadDays = Math.min(load / drain, headroom);
    const averted = this.avertedDaysOf(binding, eta, loadDays);
    if (averted <= 0) return 0;
    return this.consequenceWeightOf(d.b, d.kind) * averted / roundTrip;
  }

  /**
   * Open a per-pass ETA cache. Scoring needs a delivery time for every demand,
   * but routing every one of them would cost far more than the old sort did —
   * so pre-ranking uses a cheap straight-line estimate and only the surviving
   * candidates are actually routed.
   *
   * The pass is a LOCAL object, not engine state. `logisticsPriorityPreview()`
   * is called from a React render, and a read-only view must not be able to
   * scribble on the dispatcher's scratch — even harmlessly.
   */
  beginEtaPass(): EtaPass {
    const storages: { x: number; y: number }[] = [];
    for (const s of this.buildings.values()) {
      if (!s.constructed) continue;
      const def = this.def(s);
      if (def.isDepot || def.isCustoms || s.defId === 'warehouse') storages.push({ x: s.x, y: s.y });
    }
    return { cache: new Map(), storages };
  }

  /** Straight-line day estimate used to pre-rank before any routing is done. */
  estimateEtaDays(pass: EtaPass, b: BuildingInst): number {
    const hit = pass.cache.get(b.id);
    if (hit !== undefined) return hit;
    let best = Infinity;
    for (const s of pass.storages) best = Math.min(best, Math.max(Math.abs(s.x - b.x), Math.abs(s.y - b.y)));
    if (!Number.isFinite(best)) best = Math.max(this.mapW, this.mapH) * 0.5;
    const days = Math.max(0.6, best * BALANCE.truckDaysPerTile);
    pass.cache.set(b.id, days);
    return days;
  }

  /**
   * Collect every logistics demand for this day (no routing / dispatch).
   * Overflow hauls need a routing context to pick the nearest storage; pass one
   * when calling from logistics(). Preview skips overflow (housekeeping only).
   */
  collectLogisticsDemands(routing?: LogisticsRoutingContext): LogisticsDemand[] {
    const demands: LogisticsDemand[] = [];

    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (b.paused || (!b.constructed && !this.globalConstructionEnabled)) continue;
      if (!b.constructed) {
        // construction site materials. Threshold is ~0, not 1: a supply-starved
        // truck can deliver a fraction (e.g. 1.4/2 gravel), and the remainder
        // must still be requestable or the site starves forever.
        // An auto-bought site draws BONDED imports from its customs (paid at
        // placement) — pinned so no other site or export can take them.
        let from: number | undefined;
        let bonded = false;
        if (b.autoBought) {
          const customs = this.buildings.get(b.bondedCustomsId ?? -1) ?? this.nearestConstructedCustoms(b.x, b.y);
          if (customs?.constructed && this.def(customs).isCustoms) { from = customs.id; bonded = true; }
        }
        for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
          const missing = amt - this.stockOf(b, r) - this.incomingOf(b, r);
          if (missing > 0.001) {
            // A site never "runs dry" — nothing is consuming here — so it earns
            // no urgency from emptiness. Its rank comes from consequence alone
            // (× build priority), and ordering inside a tier stays placement
            // order. Biasing by how much is missing reshuffles the build order
            // and leaves the town full of 90%-complete sites with no roofs.
            demands.push({ b, r, amt: missing, kind: 'construction', from, bonded });
          }
        }
        continue;
      }
      // power & heating plant fuel
      if ((def.powerOutput || def.heatOutput) && def.inputs) {
        for (const r of Object.keys(def.inputs) as ResourceId[]) {
          if (r === 'machinery') continue;
          const free = this.capOf(b, r) - this.stockOf(b, r) - this.incomingOf(b, r);
          if (free >= 2) demands.push({ b, r, amt: free, kind: def.powerOutput ? 'plantFuel' : 'heatFuel' });
        }
      }
      // store goods
      if (def.serviceType === 'shop') {
        const fFree = this.capOf(b, 'food') - this.stockOf(b, 'food') - this.incomingOf(b, 'food');
        if (fFree >= 6) demands.push({ b, r: 'food', amt: fFree, kind: 'shopGoods' });
        const cFree = this.capOf(b, 'clothes') - this.stockOf(b, 'clothes') - this.incomingOf(b, 'clothes');
        if (cFree >= 4) demands.push({ b, r: 'clothes', amt: cFree, kind: 'shopGoods' });
      }
      // factory inputs
      if (def.inputs && !def.powerOutput && !def.heatOutput) {
        for (const [r] of Object.entries(def.inputs) as [ResourceId, number][]) {
          const bufferTarget = this.capOf(b, r) * 0.6;
          const missing = bufferTarget - this.stockOf(b, r) - this.incomingOf(b, r);
          if (missing >= 6) demands.push({ b, r, amt: missing, kind: 'factoryInput' });
        }
      }
      // gas station, construction office & motor depot: keep the fleet rolling.
      // Urgency comes from pooled fuel vs. actual burn — a scarce pool escalates
      // on its own, so there is no "emergency" band to trip.
      if (def.isGasStation || def.isConstructionOffice || def.isMotorDepot) {
        const free = this.capOf(b, 'fuel') - this.stockOf(b, 'fuel') - this.incomingOf(b, 'fuel');
        if (free >= 6) demands.push({ b, r: 'fuel', amt: free, kind: 'fleetFuel' });
      }
      // wear spares (machinery). Urgency is now the bin's own days-of-cover
      // against its actual wear rate, so a worn bin outranks a healthy top-up
      // without a second band — and a healthy bin scores ~0 on its own. Both
      // plants and factories qualify (the fuel branch above skips machinery).
      if (def.wear) {
        for (const r of Object.keys(def.wear) as ResourceId[]) {
          const cap = this.capOf(b, r);
          const have = this.stockOf(b, r) + this.incomingOf(b, r);
          const free = cap - have;
          if (free < 1) continue;
          const worn = buildingWorn(b);
          demands.push({ b, r, amt: free, kind: 'wear' });
          // Paid border-import fallback: only for an actually-worn bin, only if a
          // customs house can clear it. Queued just below the domestic urgent
          // demand, so the sort tries domestic first and this fires only for the
          // shortfall — in full only when no domestic machinery exists anywhere.
          // Bonded (an infinite paid customs source, like a construction auto-buy),
          // charged at dispatch, capped to a modest top-up that clears 'worn'.
          if (worn && this.repairImportsEnabled) {
            const customs = this.nearestConstructedCustoms(b.x, b.y);
            if (customs?.constructed && this.def(customs).isCustoms) {
              const topUp = Math.min(free, cap * BALANCE.repairImportTopUpFrac);
              if (topUp >= 1) {
                demands.push({
                  b, r, amt: topUp, kind: 'wear',
                  from: customs.id, bonded: true, repairImport: this.repairImportCurrency,
                });
              }
            }
          }
        }
      }
    }

    // auto-export staging: haul surplus above the keep-level to a customs
    // house, one truckload per demand — the border sells only what reaches it
    if (this.autoTrade.enabled) {
      const customsHouses = [...this.buildings.values()].filter(b => this.def(b).isCustoms && b.constructed);
      for (const r of ALL_RESOURCES) {
        const rule = this.autoTrade.rules[r];
        if (rule?.mode !== 'export' || !customsHouses.length) continue;
        let inland = 0;
        for (const s of this.sellableSources(r)) if (!this.def(s.b).isCustoms) inland += s.amt;
        let surplus = inland - rule.level;
        for (const c of customsHouses) {
          if (surplus < 1) break;
          const free = this.capOf(c, r) - this.stockOf(c, r) - this.incomingOf(c, r);
          if (free < 1) continue;
          let left = Math.min(surplus, free);
          surplus -= left;
          while (left >= 1) {
            const chunk = Math.min(left, BALANCE.truckCapacity);
            demands.push({ b: c, r, amt: chunk, kind: 'housekeeping', noCustomsSrc: true });
            left -= chunk;
          }
        }
      }
    }

    // overflow hauling needs routing to pin nearest storage — only when dispatching
    if (routing) {
      const storages = [...this.buildings.values()].filter(b =>
        (this.def(b).isDepot || this.def(b).isCustoms || b.defId === 'warehouse') && b.constructed);
      for (const b of this.buildings.values()) {
        this.assertRoutingFresh(routing);
        const def = this.def(b);
        if (!b.constructed || !def.outputs || def.serviceType) continue;
        const source = routing.facilities.get(b.id);
        if (!source?.road.tiles.length) continue;
        for (const [r] of Object.entries(def.outputs) as [ResourceId, number][]) {
          const cap = this.capOf(b, r);
          if (cap <= 0 || this.stockOf(b, r) <= cap * 0.8) continue;
          const goals: RankedGoal<BuildingInst>[] = [];
          for (const s of storages) {
            if (s.id === b.id) continue;
            const free = this.capOf(s, r) - this.stockOf(s, r) - this.incomingOf(s, r);
            if (free < 4) continue;
            const facility = routing.facilities.get(s.id)!;
            if (!shareAnyComponent(source.road.components, facility.road.components)) continue;
            goals.push(...rankedGoals(facility.road.tiles, facility.buildingRank, s));
          }
          if (!goals.length) {
            this.routingDay.componentRejections++;
            continue;
          }
          const nearest = this.nearestPath('road', source.road.tiles, goals);
          const best = nearest?.goal.value ?? null;
          if (best) {
            const free = this.capOf(best, r) - this.stockOf(best, r) - this.incomingOf(best, r);
            demands.push({ b: best, r, amt: Math.min(free, this.stockOf(b, r) - cap * 0.3), kind: 'housekeeping', from: b.id });
          }
        }
      }
    }

    return demands;
  }


  // ---------------- vehicles ----------------

  /**
   * Lorries a single building owns (0 if it isn't a garage or is unbuilt /
   * off-grid). Offices come with a pool; Motor Depots crew one per driver.
   * The single source of the per-building formula (UI reads it, never recomputes).
   */
  trucksFrom(b: BuildingInst): number {
    const def = this.def(b);
    if (!b.constructed || !b.connected) return 0;
    if (def.isConstructionOffice) {
      return BALANCE.officeTruckBase + Math.floor(BALANCE.maxActiveTrucksPerOffice * (b.staff / def.workers));
    }
    if (def.isMotorDepot) return Math.floor(BALANCE.trucksPerDriver * b.staff);
    return 0;
  }

  startLeg(v: Vehicle, from: BuildingInst, to: BuildingInst, state: VehicleState): boolean {
    const leg = this.routeBetween(from, to);
    if (!leg) return false;
    v.state = state;
    v.phase = 'go';
    v.daysDone = 0;
    v.legTiles = leg.tiles;
    v.daysTotal = Math.max(0.6, leg.tiles * BALANCE.truckDaysPerTile);
    v.points = [this.centerOf(from), ...leg.path, this.centerOf(to)];
    v.legTo = to.id;
    if (state !== 'toPickup') v.destId = to.id;
    v.atId = 0;
    return true;
  }

  /** Road-first, off-road-second building-to-building route. `tiles` is the
   *  weighted cost: off-road tiles count `offRoadStepCost` each, so travel time
   *  and fuel burn both scale with the real difficulty of the route. */
  routeBetween(from: BuildingInst, to: BuildingInst):
    { path: { x: number; y: number }[]; tiles: number } | null {
    if (from.id === to.id) return { path: [], tiles: 0 };
    const road = this.findPath(this.accessTiles(from), this.accessTiles(to));
    if (road) return { path: road, tiles: road.length };
    const land = this.nearestPath('land', this.accessTiles(to), rankedGoals(this.accessTiles(from), 0, to));
    if (!land) return null;
    return { path: land.path, tiles: Math.max(land.path.length, land.cost) };
  }

  /** Every building that can pump fuel into a tank, nearest first.
   *  Customs is last-resort — it is the border, not a filling station. */
  fuelSourcesFor(v: Vehicle): BuildingInst[] {
    const here = this.buildings.get(v.atId) ?? this.buildings.get(v.homeId);
    const ox = here ? here.x : 0, oy = here ? here.y : 0;
    const rank = (b: BuildingInst) => {
      const def = this.def(b);
      const tier = def.isGasStation || def.isMotorDepot || def.isConstructionOffice ? 0 : 1;
      return tier * 1e6 + Math.hypot(b.x - ox, b.y - oy);
    };
    return [...this.buildings.values()]
      .filter(b => {
        if (!b.constructed || !b.connected || this.stockOf(b, 'fuel') <= 0.001) return false;
        const def = this.def(b);
        return def.isGasStation || def.isMotorDepot || def.isConstructionOffice || def.isCustoms;
      })
      .sort((a, b) => rank(a) - rank(b) || a.id - b.id);
  }

  canPumpFuel(b: BuildingInst): boolean {
    const def = this.def(b);
    return !!(def.isGasStation || def.isMotorDepot || def.isConstructionOffice || def.isCustoms);
  }

  /** Fuel a vehicle keeps in hand so it is never stranded away from a pump. */
  get vehicleReserveFuel(): number {
    return BALANCE.vehicleReserveTiles * BALANCE.vehicleFuelPerTile;
  }

  /** Parked and fuelled enough to be given work at all. */
  vehicleAvailable(v: Vehicle): boolean {
    return v.state === 'idle' && v.fuel > this.vehicleReserveFuel;
  }

  /**
   * The vehicle that should take a job collecting from `supplier`, together
   * with the route it must drive to get there (empty when it is already
   * parked at the supplier).
   *
   * A vehicle only accepts work it can finish: the tank has to cover the run
   * to the supplier, the loaded run onward, and the reserve that keeps it able
   * to reach a pump afterwards. This is why nothing in the sim ever strands a
   * lorry in open country — running dry is a dispatch-time refusal, not a
   * mid-route accident.
   */
  pickVehicleFor(supplier: BuildingInst, deliveryTiles: number):
    { v: Vehicle; path: { x: number; y: number }[]; tiles: number } | null {
    const near = this.centerOf(supplier);
    const ranked = this.trucks
      .filter(v => this.vehicleAvailable(v))
      .map(v => {
        const at = this.buildings.get(v.atId);
        const d = at ? Math.max(Math.abs(at.x - near.x), Math.abs(at.y - near.y)) : Infinity;
        return { v, at, d };
      })
      .filter(c => !!c.at)
      .sort((a, b) => a.d - b.d || a.v.id - b.v.id);

    // Bounded: routing is the expensive step, so only the few nearest lorries
    // are actually pathed. A jam of unroutable candidates cannot eat the pass.
    let tried = 0;
    for (const c of ranked) {
      if (tried >= 3) break;
      const budget = (c.v.fuel - this.vehicleReserveFuel) / BALANCE.vehicleFuelPerTile;
      if (c.d + deliveryTiles > budget) continue; // cannot make it even in a straight line
      tried++;
      if (c.at!.id === supplier.id) return { v: c.v, path: [], tiles: 0 };
      const leg = this.routeBetween(c.at!, supplier);
      if (!leg) continue;
      if (leg.tiles + deliveryTiles > budget) continue;
      return { v: c.v, path: leg.path, tiles: leg.tiles };
    }
    return null;
  }

  /** Grab the nearest available vehicle outright (demolition salvage runs). */
  takeIdleVehicle(near: { x: number; y: number }): Vehicle | null {
    let best: Vehicle | null = null, bestD = Infinity;
    for (const v of this.trucks) {
      if (!this.vehicleAvailable(v)) continue;
      const at = this.buildings.get(v.atId);
      if (!at) continue;
      const d = Math.max(Math.abs(at.x - near.x), Math.abs(at.y - near.y));
      if (d < bestD) { bestD = d; best = v; }
    }
    return best;
  }


  // ---------------- construction ----------------

  markConstructed(b: BuildingInst): void {
    if (b.constructed) return;
    b.constructed = true;
    this.facilityRevision++;
  }

  /**
   * A finished building is commissioned with a FULL spare bin, so nothing is
   * born worn and a new town never starts life half-broken. Callers subtract the
   * construction bill first (completeSite / instant-build) or place on empty
   * stock (placeFree), so this sets the bin outright. The spare set is treated as
   * part of the building — a modest amount beyond the bill's machinery is granted
   * here as the installed spares (see machinery.test.ts for the born-full invariant).
   */
  seedWearBins(b: BuildingInst) {
    const def = BUILDINGS[b.defId];
    for (const r of Object.keys(def.wear ?? {}) as ResourceId[]) {
      const cap = def.storage[r] ?? 0;
      if (cap > 0) b.stock[r] = cap; // born with a full spare set
    }
  }

  /** All of a site's construction materials delivered? */
  siteReady(b: BuildingInst): boolean {
    const def = this.def(b);
    return (Object.entries(def.materials) as [ResourceId, number][])
      .every(([r, amt]) => this.stockOf(b, r) >= amt - 0.001);
  }

  /** Finish a site whose progress reached its labor bill: a road/bridge site
   *  dissolves into its tile (silent — a 30-tile paint must not fire 30 toasts);
   *  a building consumes its materials and installs wear spares. */
  completeSite(b: BuildingInst) {
    const def = this.def(b);
    if (def.becomesRoad) {
      this.applyInternalTilePatches([{ x: b.x, y: b.y, road: true, buildingId: null }]);
      this.removeBuilding(b);
      this.stats.roadsBuilt++;
      return;
    }
    this.markConstructed(b);
    b.progress = def.labor;
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      this.addStock(b, r, -amt);
    }
    this.seedWearBins(b);
    this.pushEvent(`${def.name} completed!`, 'good', 'check');
  }

  /** Builder-days a staffed, connected office can supply today. */
  builderPool(): number {
    let n = 0;
    for (const b of this.buildings.values()) {
      if (this.def(b).isConstructionOffice && b.constructed && b.connected) {
        // contract crew guarantees the office works before you have citizens
        n += Math.max(10, b.staff);
      }
    }
    return n;
  }

  /** Builders actually manned by citizens — domestic labor is free. Anything
   *  the full builderPool provides beyond this is imported (foreign) labor. */
  domesticBuilderPool(): number {
    let n = 0;
    for (const b of this.buildings.values()) {
      if (this.def(b).isConstructionOffice && b.constructed && b.connected) n += b.staff;
    }
    return n;
  }

  // ---------------- the border ----------------

  private marketPrice(r: ResourceId, currency: 'east' | 'west') {
    const base = currency === 'east' ? RESOURCES[r].priceEast : RESOURCES[r].priceWest;
    return base * (currency === 'east' ? this.priceFactorEast : this.priceFactorWest);
  }

  /** Sell price. A failed contract sours relations: the bloc pays less for a while. */
  priceOf(r: ResourceId, currency: 'east' | 'west') {
    return this.marketPrice(r, currency) * (1 - this.relationsPenalty[currency]);
  }

  /** Buy price. Soured relations cut both ways: the bloc also charges more. */
  importPriceOf(r: ResourceId, currency: 'east' | 'west') {
    return this.marketPrice(r, currency) * IMPORT_MARKUP
      * DIFFICULTIES[this.difficulty].importPriceMult
      * (1 + this.relationsPenalty[currency]);
  }

  /**
   * Pay for `amt` exported units. Units owed to the oldest active contract
   * for (r, bloc) are credited and paid at its locked price; the remainder
   * fetches the market price. Both sale paths (manual sell, auto-trade) route
   * through here, so contracts cannot miss a delivery.
   */
  exportPayout(r: ResourceId, bloc: 'east' | 'west', amt: number): number {
    const c = this.contracts.find(k => k.state === 'active' && k.r === r && k.bloc === bloc);
    if (!c) return amt * this.priceOf(r, bloc);
    // Callers pass whole units (buy/sell/auto-trade all floor at the border), so
    // `delivered` stays integer. Do NOT floor `credited` here — the caller already
    // removed `amt` from stock, so flooring would make the sub-unit remainder vanish.
    const credited = Math.min(amt, c.amount - c.delivered);
    c.delivered += credited;
    if (c.delivered >= c.amount - 1e-9) {
      c.state = 'done';
      c.closedIdx = this.dayIndex();
      this.pushEvent(`Contract fulfilled: ${c.amount} ${RESOURCES[r].name} to the ${c.bloc === 'east' ? 'East' : 'West'}!`, 'good', 'contract');
    }
    return credited * c.pricePerUnit + (amt - credited) * this.priceOf(r, bloc);
  }

  /** Live town-wide stock incl. cargo on the road — auto-imports measure against this, not yesterday's totals. */
  liveTownTotal(r: ResourceId): number {
    let total = 0;
    for (const b of this.buildings.values()) total += this.stockOf(b, r);
    for (const t of this.trucks) if (t.cargo === r) total += t.amount;
    for (const bt of this.boats) if (bt.cargo === r) total += bt.amount;
    return total;
  }

  /**
   * Flavor: a foreign lorry drives in from the map edge along the crossing
   * lane, pauses at the customs, and leaves. Purely visual — capped, and
   * spawned only by actual trades, so it stays deterministic.
   */
  spawnForeignTruck(c: BuildingInst, r: ResourceId, amt: number) {
    const edge = this.borderEdge;
    if (!edge || this.foreignTrucks.length >= 8) return;
    const pts = edge === 'W' ? [{ x: -0.8, y: c.y + 0.5 }, { x: c.x - 0.5, y: c.y + 0.5 }]
      : edge === 'E' ? [{ x: this.mapW - 0.2, y: c.y + 0.5 }, { x: c.x + c.w + 0.5, y: c.y + 0.5 }]
      : edge === 'N' ? [{ x: c.x + 0.5, y: -0.8 }, { x: c.x + 0.5, y: c.y - 0.5 }]
      : [{ x: c.x + 0.5, y: this.mapH - 0.2 }, { x: c.x + 0.5, y: c.y + c.h + 0.5 }];
    this.foreignTrucks.push({
      id: this.nextTruckId++, points: pts, cargo: r, amount: amt,
      daysTotal: 0.7, daysDone: 0, phase: 'go', destId: c.id, srcId: 0,
    });
  }

  /**
   * One export across the border: stock out, contract credited, treasury paid,
   * ledger booked, lorry waved through. Deliberately one primitive rather than
   * six mutations — it is a single real transaction, and splitting it would
   * only produce parts that are never emitted apart.
   */
  sellAcrossBorder(c: BuildingInst, r: ResourceId, bloc: 'east' | 'west', amt: number): void {
    const led = this.tradeLedger.today;
    this.addStock(c, r, -amt);
    const gain = this.exportPayout(r, bloc, amt);
    if (bloc === 'east') { this.rubles += gain; led.rubles += gain; }
    else { this.dollars += gain; led.dollars += gain; }
    this.stats.exportedValue += bloc === 'east' ? gain : gain * 10;
    led.exports[r] = (led.exports[r] ?? 0) + amt;
    led.used += amt;
    this.spawnForeignTruck(c, r, amt);
  }

  /** The mirror of `sellAcrossBorder`: treasury out, goods into customs stock. */
  buyAcrossBorder(c: BuildingInst, r: ResourceId, bloc: 'east' | 'west', amt: number): void {
    const led = this.tradeLedger.today;
    const cost = amt * this.importPriceOf(r, bloc);
    if (bloc === 'east') { this.rubles -= cost; led.rubles -= cost; }
    else { this.dollars -= cost; led.dollars -= cost; }
    this.addStock(c, r, amt);
    this.stats.imported[r] = (this.stats.imported[r] ?? 0) + amt;
    led.imports[r] = (led.imports[r] ?? 0) + amt;
    led.used += amt;
    this.spawnForeignTruck(c, r, amt);
  }

  // ---------------- how well a building runs ----------------

  /** Where `b` sits in the queue for a resource the republic hands out in a
   *  fixed order (workers, power). Authored per building in config. */
  allocationRank(b: BuildingInst): number {
    return this.def(b).allocationPriority ?? DEFAULT_ALLOCATION_PRIORITY;
  }

  baseEff(b: BuildingInst): number {
    const def = this.def(b);
    const staffRatio = def.workers > 0 ? b.staff / def.workers : 1;
    const powerFactor = def.power > 0 && !b.powered ? (def.unpoweredEff ?? DEFAULT_UNPOWERED_EFF) : 1;
    // dry machinery bins never stall a building — the machines limp on, worn
    const wornFactor = buildingWorn(b) ? BALANCE.wornEffMult : 1;
    return staffRatio * powerFactor * wornFactor;
  }

  /**
   * The efficiency a building INTENDS to run at — `baseEff()` with the power
   * factor floored at the brownout rate instead of allowed to reach zero.
   *
   * Used only to size demand. A mill authored `unpoweredEff: 0` produces
   * nothing while the grid is down, which is correct, but if its *drain* also
   * read zero it would report needing nothing, score nothing, and never be
   * delivered to again — so it would still be sitting on empty bins the day
   * power returned. Intent is what keeps the pipeline stocked through an
   * outage; `baseEff` is what actually comes out of the building.
   */
  nominalEff(b: BuildingInst): number {
    const def = this.def(b);
    const staffRatio = def.workers > 0 ? b.staff / def.workers : 1;
    const powerFactor = def.power > 0 && !b.powered
      ? Math.max(DEFAULT_UNPOWERED_EFF, def.unpoweredEff ?? DEFAULT_UNPOWERED_EFF)
      : 1;
    const wornFactor = buildingWorn(b) ? BALANCE.wornEffMult : 1;
    return staffRatio * powerFactor * wornFactor;
  }

  /** Staffing/season/terrain scaling of a producer's design rate, before any
   *  input-availability throttling. Shared by `productionRates` (actual flow)
   *  and `nominalInputRate` (what it wants) so the two cannot drift. */
  outputMultiplier(b: BuildingInst, eff = this.baseEff(b)): number {
    const def = this.def(b);
    if (def.isFarm) {
      const fields = Math.min(12, this.countFarmFields(b.x, b.y, b.w, b.h));
      return eff * (fields / 12) * (FARM_SEASON[this.month] ?? 0) * 2.2 * this.farmWeatherMult();
    }
    if (def.requiresForest) {
      return eff * Math.min(1, this.countForestTiles(b.x, b.y, b.w, b.h) / 6);
    }
    return eff;
  }

  /**
   * Actual per-day resource flows for a building under current conditions.
   * `production()` applies exactly these deltas, and the UI displays them, so
   * the simulation and the inspector cannot diverge.
   */
  productionRates(b: BuildingInst): { inputs: Partial<Record<ResourceId, number>>; outputs: Partial<Record<ResourceId, number>> } {
    const rates: { inputs: Partial<Record<ResourceId, number>>; outputs: Partial<Record<ResourceId, number>> } = { inputs: {}, outputs: {} };
    const def = this.def(b);
    if (!b.constructed) return rates;

    // fuel burners: eff & coalFactor were fixed by the power/heat system this day
    if (def.powerOutput || def.heatOutput) {
      const inputRes = def.inputs ? (Object.keys(def.inputs)[0] as ResourceId) : 'coal';
      const burn = (def.inputs?.[inputRes] ?? 0) * b.eff * b.coalFactor;
      if (burn > 0) rates.inputs[inputRes] = burn;
      // machinery wears with actual burn intensity — an idle plant wears nothing
      for (const [r, amt] of Object.entries(def.wear ?? {}) as [ResourceId, number][]) {
        const worn = amt * b.eff * b.coalFactor;
        if (worn > 0) rates.inputs[r] = (rates.inputs[r] ?? 0) + worn;
      }
      return rates;
    }
    if (!def.outputs) return rates;

    const outMul = this.outputMultiplier(b);

    // input-limited?
    let inputFactor = 1;
    if (def.inputs) {
      for (const [r, amt] of Object.entries(def.inputs) as [ResourceId, number][]) {
        const need = amt * outMul;
        if (need > 0) inputFactor = Math.min(inputFactor, this.stockOf(b, r) / need);
      }
      inputFactor = Math.min(1, inputFactor);
    }
    const finalMul = outMul * inputFactor;
    if (finalMul <= 0) return rates;
    if (def.inputs) {
      for (const [r, amt] of Object.entries(def.inputs) as [ResourceId, number][]) rates.inputs[r] = amt * finalMul;
    }
    // wear scales with actual activity and NEVER gates output (addStock clamps
    // an empty bin at 0; the worn penalty rides in baseEff instead)
    for (const [r, amt] of Object.entries(def.wear ?? {}) as [ResourceId, number][]) {
      const worn = amt * finalMul;
      if (worn > 0) rates.inputs[r] = (rates.inputs[r] ?? 0) + worn;
    }
    for (const [r, amt] of Object.entries(def.outputs) as [ResourceId, number][]) rates.outputs[r] = amt * finalMul;
    return rates;
  }

  /** What `addStock` WOULD change, without changing it — the pure half of the
   *  same clamp, so a system can know the result of its own effect. `from`
   *  lets a caller chain several deltas within one batch. */
  clampedAdd(b: BuildingInst, r: ResourceId, amt: number, from = this.stockOf(b, r)): number {
    return Math.max(0, Math.min(this.capOf(b, r), from + amt)) - from;
  }

  /** Connectivity is expensive and changes rarely, so its mutation list is
   *  cached until the topology or the facility set moves. The cache lives here
   *  (it is derived world state); the computation stays in the system module,
   *  so World never has to import a system. */
  readonly connectivityCache = new RevisionCache<Mutation[]>();
  connectivityDeps(): readonly number[] {
    return [this.topology.revision('road'), this.topology.revision('land'), this.facilityRevision];
  }

  constructor(
    tiles: Tile[][],
    borderEdge: BorderEdge | null,
    seed: number,
    climate: ClimateId,
    difficulty: DifficultyId,
    weatherScript?: (dayIndex: number) => Partial<DayWeather>,
  ) {
    this._tiles = tiles;
    this.mapH = tiles.length;
    this.mapW = tiles[0].length;
    this.topology = new TopologyIndex({
      width: this.mapW,
      height: this.mapH,
      tiles: () => this._tiles,
      offRoadCost: BALANCE.offRoadStepCost,
    });
    this.borderEdge = borderEdge;
    this.hasWater = this._tiles.some(row => row.some(t => t.terrain === 'water'));
    this.seed = seed;
    this.difficulty = difficulty;
    this.rng = mulberry32(seed ^ 0x9e3779b9); // decorrelate from map generation
    this.timeline = new WeatherTimeline(seed, CLIMATES[climate]);
    this.weatherScript = weatherScript;
    this.weather = this.weatherAt(this.dayIndex());
  }

  // ---------------- calendar & weather accessors ----------------

  season(): Season {
    if (this.month === 12 || this.month <= 2) return 'winter';
    if (this.month <= 5) return 'spring';
    if (this.month <= 8) return 'summer';
    return 'autumn';
  }

  /** Heating is needed when it is actually cold out — not by the calendar. */
  heatingRequired() { return this.weather.tempC < BALANCE.heatThresholdC; }

  /** 0..1.25 share of nominal heat demand: mild days sip coal, deep cold over-drives. */
  heatDemandFactor(): number {
    if (!this.heatingRequired()) return 0;
    return Math.min(1.25,
      (BALANCE.heatThresholdC - this.weather.tempC) / (BALANCE.heatThresholdC - BALANCE.heatDesignTempC));
  }

  /** Crop growth multiplier from today's weather: rain feeds, frost stops, drought withers. */
  farmWeatherMult(): number {
    if (this.weather.tempC < 0) return 0; // frost — nothing grows
    const drought = Math.max(0.6, 1 - Math.max(0, this.dryStreak - BALANCE.droughtAfterDays) * 0.05);
    return WEATHER[this.weather.condition].farmMult * drought;
  }

  /** Absolute day index into the weather timeline (0 = January 1, 1960). */
  dayIndex(): number {
    return (this.year - 1960) * 360 + (this.month - 1) * 30 + (this.day - 1);
  }

  weatherAt(index: number): DayWeather {
    const w = this.timeline.at(index);
    const o = this.weatherScript?.(index);
    return { ...w, ...o }; // copy: memoized timeline entries stay pristine
  }

  /** Exact upcoming weather — the timeline is deterministic, so the State
   *  Hydrometeorological Service never misses. */
  forecast(days = 5): DayWeather[] {
    const idx = this.dayIndex();
    return Array.from({ length: days }, (_, i) => this.weatherAt(idx + 1 + i));
  }

  // ---------------- tiles & footprints: the only write door ----------------

  applyInternalTilePatches(patches: readonly InternalTilePatch[]): boolean {
    let changed = false;
    const dirty = new Set<TopologyDomain>();
    const owns = (p: InternalTilePatch, key: keyof InternalTilePatch) =>
      Object.prototype.hasOwnProperty.call(p, key);

    for (const p of patches) {
      const tile = this._tiles[p.y]?.[p.x];
      if (!tile) continue;

      // Snapshot the routing-relevant fields before mutating, so the topology's
      // own cost functions decide which domains this change touches — no hand-kept
      // field→domain mirror to drift out of sync with the cost predicates.
      const before: RoutingTile = {
        terrain: tile.terrain, road: tile.road, buildingId: tile.buildingId, foreign: tile.foreign,
      };
      let routingChanged = false;

      if (owns(p, 'road') && p.road !== undefined && !!tile.road !== p.road) {
        tile.road = p.road; changed = true; routingChanged = true;
      }
      if (owns(p, 'terrain') && p.terrain !== undefined && tile.terrain !== p.terrain) {
        tile.terrain = p.terrain; changed = true; routingChanged = true;
      }
      if (owns(p, 'foreign') && p.foreign !== undefined && !!tile.foreign !== p.foreign) {
        tile.foreign = p.foreign; changed = true; routingChanged = true;
      }
      // buildingId/deposit: an explicit `undefined` is a no-op (matching road/terrain/
      // foreign/variant); `null` is the explicit clear. Only add/removeBuilding pass
      // buildingId, always as a concrete id or null.
      if (owns(p, 'buildingId') && p.buildingId !== undefined) {
        const next = p.buildingId ?? undefined;
        if (tile.buildingId !== next) { tile.buildingId = next; changed = true; routingChanged = true; }
      }
      if (owns(p, 'deposit') && p.deposit !== undefined) {
        const next = p.deposit ?? undefined;
        if (tile.deposit !== next) { tile.deposit = next; changed = true; } // routing-irrelevant
      }
      if (owns(p, 'variant') && p.variant !== undefined && tile.variant !== p.variant) {
        tile.variant = p.variant; changed = true;
      }

      if (routingChanged) {
        for (const domain of this.topology.affectedDomains(before, tile, p.x, p.y)) dirty.add(domain);
      }
    }

    if (dirty.size) this.topology.invalidate(...dirty);
    if (dirty.has('water')) this.hasWater = this._tiles.some(row => row.some(t => t.terrain === 'water'));
    return changed;
  }

  setRoadTile(x: number, y: number, road: boolean): void {
    this.applyInternalTilePatches([{ x, y, road }]);
  }

  stampFootprint(b: BuildingInst): void {
    const patches: InternalTilePatch[] = [];
    for (let dy = 0; dy < b.h; dy++) for (let dx = 0; dx < b.w; dx++) {
      patches.push({ x: b.x + dx, y: b.y + dy, buildingId: b.id });
    }
    this.applyInternalTilePatches(patches);
  }

  clearFootprint(b: BuildingInst): void {
    const patches: InternalTilePatch[] = [];
    for (let dy = 0; dy < b.h; dy++) for (let dx = 0; dx < b.w; dx++) {
      patches.push({ x: b.x + dx, y: b.y + dy, buildingId: null });
    }
    this.applyInternalTilePatches(patches);
  }

  addBuilding(b: BuildingInst): void {
    this.buildings.set(b.id, b);
    this.stampFootprint(b);
    this.facilityRevision++;
  }

  removeBuilding(b: BuildingInst): void {
    // Symmetric with addBuilding's stampFootprint: removeBuilding owns clearing the
    // footprint so a caller (or a future multi-tile becomesRoad site) can never leave
    // a phantom buildingId on the map. Callers that must clear earlier for ordering
    // (bulldoze → refund routing) still may — the second clear is an idempotent no-op.
    this.clearFootprint(b);
    this.buildings.delete(b.id);
    this.facilityRevision++;
  }

  // ---------------- buildings and their bins ----------------

  def(b: BuildingInst) { return BUILDINGS[b.defId]; }

  buildingAt(x: number, y: number): BuildingInst | undefined {
    const id = this.tiles[y]?.[x]?.buildingId;
    return id ? this.buildings.get(id) : undefined;
  }

  stockOf(b: BuildingInst, r: ResourceId) { return b.stock[r] ?? 0; }
  incomingOf(b: BuildingInst, r: ResourceId) { return b.incoming[r] ?? 0; }
  capOf(b: BuildingInst, r: ResourceId) {
    const def = this.def(b);
    if (!b.constructed) return def.materials[r] ?? 0; // construction sites store delivered materials
    return def.storage[r] ?? 0;
  }

  /** Add (or remove) stock, clamped to [0, cap]. Returns the actual change. */
  addStock(b: BuildingInst, r: ResourceId, amt: number): number {
    const cap = this.capOf(b, r);
    const before = this.stockOf(b, r);
    const after = Math.max(0, Math.min(cap, before + amt));
    b.stock[r] = after;
    return after - before;
  }

  centerOf(b: BuildingInst) { return { x: b.x + b.w / 2, y: b.y + b.h / 2 }; }

  /** Open grass tiles within the farm's work radius, excluding the (would-be) footprint and foreign soil. */
  countFarmFields(x: number, y: number, w: number, h: number): number {
    let fields = 0;
    for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
      const tx = x + dx, ty = y + dy;
      if (tx >= x && tx < x + w && ty >= y && ty < y + h) continue;
      const t = this.tiles[ty]?.[tx];
      if (t && t.terrain === 'grass' && !t.buildingId && !t.road && !t.deposit && !t.foreign) fields++;
    }
    return fields;
  }

  /** Unoccupied forest tiles within reach, excluding the (would-be) footprint and foreign soil. */
  countForestTiles(x: number, y: number, w: number, h: number): number {
    let forests = 0;
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) {
      const tx = x + dx, ty = y + dy;
      if (tx >= x && tx < x + w && ty >= y && ty < y + h) continue;
      const t = this.tiles[ty]?.[tx];
      if (t && t.terrain === 'forest' && !t.buildingId && !t.road && !t.foreign) forests++;
    }
    return forests;
  }

  // ---------------- routing primitives ----------------

  adjacentRoads(b: BuildingInst): { x: number; y: number }[] {
    return this.roadAccess(b).tiles.map(({ x, y }) => ({ x, y }));
  }

  roadAccess(b: BuildingInst) { return this.topology.access('road', b); }
  landAccess(b: BuildingInst) { return this.topology.access('land', b); }
  waterAccess(b: BuildingInst) { return this.topology.access('water', b); }

  nearestPath<T>(
    domain: TopologyDomain,
    sources: readonly TopologyPos[],
    goals: readonly RankedGoal<T>[],
    recordDiagnostics = true,
  ): NearestPath<T> | null {
    if (!sources.length || !goals.length) return null;
    const mask = this.topology.mask(domain);
    if (recordDiagnostics) {
      if (domain === 'road') this.routingDay.roadSearches++;
      else if (domain === 'land') this.routingDay.landSearches++;
      else this.routingDay.waterSearches++;
    }
    const result = shortestPathToAny(
      this.mapW,
      this.mapH,
      (x, y) => mask[y * this.mapW + x],
      sources,
      goals,
      this.topology.maxStep(domain), // bound derived from the mask itself — never disagrees
    );
    if (recordDiagnostics && result) {
      this.routingDay.settledTiles += result.settledNodes;
      this.routingDay.pathsMaterialized++;
    }
    return result;
  }

  /** Weighted reachability over land: roads cost 1, off-road land costs K,
   *  water/foreign/footprints are impassable. Roads win purely on cost. */
  floodTerrain(sources: readonly TopologyPos[]): FloodResult {
    const mask = this.topology.mask('land');
    return floodCost(
      this.mapW,
      this.mapH,
      (x, y) => mask[y * this.mapW + x],
      [...sources],
      this.topology.maxStep('land'), // bound derived from the mask itself — never disagrees
    );
  }

  /** Footprint-adjacent drivable tiles (roads AND open land) — a vehicle's
   *  on/off ramps. Superset of adjacentRoads. */
  accessTiles(b: BuildingInst): { x: number; y: number }[] {
    return this.landAccess(b).tiles.map(({ x, y }) => ({ x, y }));
  }

  /** Water tiles orthogonally touching a building's footprint (its docks). */
  adjacentWater(b: BuildingInst): { x: number; y: number }[] {
    return this.waterAccess(b).tiles.map(({ x, y }) => ({ x, y }));
  }

  findPath(from: readonly TopologyPos[], to: readonly TopologyPos[]): { x: number; y: number }[] | null {
    if (!from.length || !to.length) return null;
    return this.nearestPath('road', to, rankedGoals(from, 0, null), false)?.path ?? null;
  }

  topologyRevision(domain: TopologyDomain): number {
    return this.topology.revision(domain);
  }
}
