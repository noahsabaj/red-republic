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
import { ALL_RESOURCES, BALANCE, BUILDINGS, CLIMATES, FARM_SEASON, LOANS, POWER_SECTORS, WEATHER } from './config';
import type { Category, ClimateId, DepositType, ResourceId } from './config';
import { mulberry32 } from './mapgen';
import type { BorderEdge, SeededRng, Tile } from './mapgen';
import { FloodResult, floodCost, shortestPathToAny } from './pathfind';
import type { NearestPath, RankedGoal } from './pathfind';
import { TopologyIndex } from './topology';
import type { RoutingTile, TopologyDomain, TopologyPos } from './topology';
import { WeatherTimeline } from './weather';
import type { DayWeather } from './weather';
// Type-only, so World and Mutation never form a runtime cycle: the systems
// import both at runtime, neither imports the other's code.
import type { Mutation } from './mutation';

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
