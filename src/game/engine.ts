// ============================================================
// Red Republic — game engine & simulation
// ============================================================
import {
  BUILDINGS, RESOURCES, ALL_RESOURCES, BALANCE, FARM_SEASON, WEATHER,
  DOLLAR_BUILD_RATE, IMPORT_MARKUP, OBJECTIVES,
} from './config';
import type { DepositType, ResourceId } from './config';
import { generateMap, mulberry32, MAP_W, MAP_H } from './mapgen';
import type { BorderEdge, MapData, Tile } from './mapgen';
import { floodRoads, FloodResult } from './pathfind';
import type { DistanceField } from './pathfind';
import { WeatherTimeline } from './weather';
import type { DayWeather } from './weather';

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
  connected: boolean;
  coalFactor: number;
  farmFields: number;
  priorityHigh?: boolean;
}

export interface Truck {
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

/** A barge sailing between ports — same lifecycle as a truck, on water. */
export type Boat = Truck;

/** Standing freight order: sail `amt` of `r` from one port to another once the goods arrive portside. */
interface BoatOrder { srcId: number; destId: number; r: ResourceId; amt: number }

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
}

const emptyLedger = (): TradeDayLedger =>
  ({ imports: {}, exports: {}, rubles: 0, dollars: 0, used: 0, capacity: 0, blocked: [] });

interface LogisticsDemand { b: BuildingInst; r: ResourceId; amt: number; prio: number; from?: number; noCustomsSrc?: boolean }

const JOB_PRIORITY = [
  'powerPlant', 'heatingPlant', 'store', 'foodFactory',
  'clinic', 'pub', 'customs', 'farm', 'textileMill', 'sawmill', 'brickworks',
  'woodcutter', 'gravelQuarry', 'coalMine', 'ironMine', 'steelMill',
  'oilPump', 'refinery', 'port', 'depot', 'warehouse', 'constructionOffice',
];

export class GameEngine {
  tiles: Tile[][];
  buildings = new Map<number, BuildingInst>();
  trucks: Truck[] = [];
  boats: Boat[] = [];
  day = 1; month = 3; year = 1960;
  rubles = BALANCE.startRubles;
  dollars = BALANCE.startDollars;
  pop = 0;
  speed: 0 | 1 | 2 | 4 = 1;

  // computed stats
  capacity = 0;
  workers = 0;
  employed = 0;
  jobs = 0;
  happiness = 70;
  sat = { food: 1, clothes: 1, power: 1, heat: 1, culture: 0, health: 0, employment: 1, pollution: 1 };
  powerProduced = 0; powerDemand = 0;
  heatProduced = 0; heatDemand = 0;
  priceFactorEast = 1;
  priceFactorWest = 1;
  totals: Record<ResourceId, number> = Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>;
  stats = {
    produced: Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>,
    exportedValue: 0,
    roadsBuilt: 0, // cumulative player-built road tiles (objective metric; never decremented)
  };
  objectivesDone: string[] = [];
  alerts: Alert[] = [];
  wagesUnpaid = false;
  /** National auto-trade policy — mutate only via the setAutoTrade* methods. */
  autoTrade = {
    enabled: false,
    reserveRubles: BALANCE.autoReserveRubles,
    reserveDollars: BALANCE.autoReserveDollars,
    rules: {} as Partial<Record<ResourceId, AutoTradeRule>>,
  };
  tradeLedger = { today: emptyLedger(), yesterday: emptyLedger() };

  private nextBuildingId = 1;
  private nextTruckId = 1;
  private nextBoatId = 1;
  private nextEventId = 1;
  private boatOrders: BoatOrder[] = [];
  private acc = 0;
  private events: GameEvent[] = [];
  private listeners = new Set<() => void>();
  private version = 0;

  readonly TICK_MS = 500; // one game day at 1x speed

  readonly seed: number;
  /** Which map edge is the national border; null on bare test maps (no border rules). */
  readonly borderEdge: BorderEdge | null;
  private rng: () => number;
  private timeline: WeatherTimeline;
  /** Test/debug seam: overlays the deterministic timeline (helpers force calm weather). */
  weatherScript?: (dayIndex: number) => Partial<DayWeather>;
  weather: DayWeather;
  private hasWater = false;
  private dryStreak = 0;   // hot rainless days in a row (drought)
  private gloomStreak = 0; // miserable-weather days in a row (morale)
  private sunStreak = 0;
  private wasFrost = false;

  constructor(opts: { seed?: number; map?: MapData; skipStartingBase?: boolean; weatherScript?: (dayIndex: number) => Partial<DayWeather> } = {}) {
    this.seed = opts.seed ?? Math.floor(Math.random() * 2 ** 31);
    this.rng = mulberry32(this.seed ^ 0x9e3779b9); // decorrelate from map generation
    this.timeline = new WeatherTimeline(this.seed);
    this.weatherScript = opts.weatherScript;
    this.weather = this.weatherAt(this.dayIndex());
    const map = opts.map ?? generateMap(this.seed);
    this.tiles = map.tiles;
    this.borderEdge = map.border ?? null;
    this.hasWater = this.tiles.some(row => row.some(t => t.terrain === 'water'));
    if (!opts.skipStartingBase) this.setupStartingBase(map);
  }

  // ---------------- setup ----------------

  private setupStartingBase(map: MapData) {
    const sx = map.startX, sy = map.startY;
    // road line north of buildings
    for (let x = sx - 2; x <= sx + 1; x++) {
      this.tiles[sy - 1][x].road = true;
    }
    this.placeFree('depot', sx, sy);
    this.placeFree('constructionOffice', sx - 2, sy);
    if (map.border && map.crossX !== undefined && map.crossY !== undefined) {
      // the customs house is the border crossing itself
      this.placeFree('customs', map.crossX, map.crossY);
      this.layCrossingRoads(map.border, map.crossX, map.crossY, sx, sy);
    } else {
      // borderless (test) maps keep the legacy row layout
      this.placeFree('customs', sx + 3, sy);
      for (let x = sx + 2; x <= sx + 4; x++) this.tiles[sy - 1][x].road = true;
    }
    const depot = [...this.buildings.values()].find(b => b.defId === 'depot')!;
    depot.stock = { planks: 120, bricks: 120, steel: 50, food: 100 };
    this.pushEvent('The Politburo has granted you this land. Build a thriving socialist republic!', 'info', 'star');
  }

  /** Border crossing: a lane through the foreign strip to the map edge, plus a domestic link to the base. */
  private layCrossingRoads(edge: BorderEdge, cx: number, cy: number, sx: number, sy: number) {
    const lay = (x: number, y: number) => {
      const t = this.tiles[y]?.[x];
      if (t && !t.buildingId) t.road = true; // over water this is a bridge
    };
    // through the strip to the edge (foreign soil — engine-laid; players cannot build here)
    if (edge === 'W') for (let x = 0; x < cx; x++) lay(x, cy);
    if (edge === 'E') for (let x = cx + 2; x < MAP_W; x++) lay(x, cy);
    if (edge === 'N') for (let y = 0; y < cy; y++) lay(cx, y);
    if (edge === 'S') for (let y = cy + 2; y < MAP_H; y++) lay(cx, y);
    // domestic link: the front-door tile, then an L to the base road row
    const front = edge === 'W' ? { x: cx + 2, y: cy }
      : edge === 'E' ? { x: cx - 1, y: cy }
      : edge === 'N' ? { x: cx, y: cy + 2 }
      : { x: cx, y: cy - 1 };
    for (let y = Math.min(front.y, sy - 1); y <= Math.max(front.y, sy - 1); y++) lay(front.x, y);
    for (let x = Math.min(front.x, sx - 2); x <= Math.max(front.x, sx + 1); x++) lay(x, sy - 1);
  }

  private placeFree(defId: string, x: number, y: number) {
    const def = BUILDINGS[defId];
    const b: BuildingInst = {
      id: this.nextBuildingId++, defId, x, y, w: def.size[0], h: def.size[1],
      constructed: true, progress: def.labor, stock: {}, incoming: {},
      staff: 0, eff: 0, powered: false, heated: false, connected: false,
      coalFactor: 1, farmFields: 0,
    };
    this.buildings.set(b.id, b);
    for (let dy = 0; dy < b.h; dy++)
      for (let dx = 0; dx < b.w; dx++)
        this.tiles[y + dy][x + dx].buildingId = b.id;
  }

  // ---------------- helpers ----------------

  def(b: BuildingInst) { return BUILDINGS[b.defId]; }

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
  private dayIndex(): number {
    return (this.year - 1960) * 360 + (this.month - 1) * 30 + (this.day - 1);
  }

  private weatherAt(index: number): DayWeather {
    const w = this.timeline.at(index);
    const o = this.weatherScript?.(index);
    return { ...w, ...o }; // copy: memoized timeline entries stay pristine
  }

  /** Exact upcoming weather — the timeline is deterministic, so the State Hydrometeorological Service never misses. */
  forecast(days = 5): DayWeather[] {
    const idx = this.dayIndex();
    return Array.from({ length: days }, (_, i) => this.weatherAt(idx + 1 + i));
  }

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

  adjacentRoads(b: BuildingInst): { x: number; y: number }[] {
    const out: { x: number; y: number }[] = [];
    for (let dy = -1; dy <= b.h; dy++) {
      for (let dx = -1; dx <= b.w; dx++) {
        const onEdge = dx === -1 || dx === b.w || dy === -1 || dy === b.h;
        const onCorner = (dx === -1 || dx === b.w) && (dy === -1 || dy === b.h);
        if (!onEdge || onCorner) continue; // corners touch only diagonally — no road access
        const tx = b.x + dx, ty = b.y + dy;
        if (this.tiles[ty]?.[tx]?.road) out.push({ x: tx, y: ty });
      }
    }
    return out;
  }

  private floodFrom(sources: { x: number; y: number }[]): FloodResult {
    return floodRoads((x, y) => !!this.tiles[y][x].road, sources);
  }

  /** Water tiles orthogonally touching a building's footprint (its docks). */
  adjacentWater(b: BuildingInst): { x: number; y: number }[] {
    const out: { x: number; y: number }[] = [];
    for (let dy = -1; dy <= b.h; dy++) {
      for (let dx = -1; dx <= b.w; dx++) {
        const onEdge = dx === -1 || dx === b.w || dy === -1 || dy === b.h;
        const onCorner = (dx === -1 || dx === b.w) && (dy === -1 || dy === b.h);
        if (!onEdge || onCorner) continue;
        const tx = b.x + dx, ty = b.y + dy;
        if (this.tiles[ty]?.[tx]?.terrain === 'water') out.push({ x: tx, y: ty });
      }
    }
    return out;
  }

  private waterFlood(sources: { x: number; y: number }[]): FloodResult {
    return floodRoads((x, y) => this.tiles[y][x].terrain === 'water', sources);
  }

  findPath(from: { x: number; y: number }[], to: { x: number; y: number }[]): { x: number; y: number }[] | null {
    if (!from.length || !to.length) return null;
    const flood = this.floodFrom(to);
    let best: { x: number; y: number } | null = null;
    let bestD = Infinity;
    for (const f of from) {
      const d = flood.distanceAt(f.x, f.y);
      if (d >= 0 && d < bestD) { bestD = d; best = f; }
    }
    return best ? flood.pathFrom(best.x, best.y) : null;
  }

  centerOf(b: BuildingInst) { return { x: b.x + b.w / 2, y: b.y + b.h / 2 }; }

  /** Open grass tiles within the farm's work radius, excluding the (would-be) footprint. */
  countFarmFields(x: number, y: number, w: number, h: number): number {
    let fields = 0;
    for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
      const tx = x + dx, ty = y + dy;
      if (tx >= x && tx < x + w && ty >= y && ty < y + h) continue;
      const t = this.tiles[ty]?.[tx];
      if (t && t.terrain === 'grass' && !t.buildingId && !t.road && !t.deposit) fields++;
    }
    return fields;
  }

  /** Unoccupied forest tiles within reach, excluding the (would-be) footprint. */
  countForestTiles(x: number, y: number, w: number, h: number): number {
    let forests = 0;
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) {
      const tx = x + dx, ty = y + dy;
      if (tx >= x && tx < x + w && ty >= y && ty < y + h) continue;
      const t = this.tiles[ty]?.[tx];
      if (t && t.terrain === 'forest' && !t.buildingId && !t.road) forests++;
    }
    return forests;
  }

  // ---------------- placement ----------------

  /** Roads over water are bridges — same tile flag, steeper price. */
  roadCostAt(x: number, y: number): number {
    return this.tiles[y]?.[x]?.terrain === 'water' ? BALANCE.bridgeCostRubles : BUILDINGS.road.costRubles;
  }

  canPlace(defId: string, x: number, y: number): { ok: boolean; reason?: string } {
    const def = BUILDINGS[defId];
    if (defId === 'road') {
      const t = this.tiles[y]?.[x];
      if (!t) return { ok: false, reason: 'Out of bounds' };
      if (t.foreign) return { ok: false, reason: 'Beyond the state border' };
      if (t.road) return { ok: false, reason: 'Road already here' };
      if (t.buildingId) return { ok: false, reason: 'Occupied by a building' };
      return { ok: true }; // on water this becomes a bridge
    }
    const [w, h] = def.size;
    if (x < 0 || y < 0 || x + w > MAP_W || y + h > MAP_H) return { ok: false, reason: 'Out of bounds' };
    let depositOk = !def.requiresDeposit;
    for (let dy = 0; dy < h; dy++) {
      for (let dx = 0; dx < w; dx++) {
        const t = this.tiles[y + dy][x + dx];
        if (t.foreign) return { ok: false, reason: 'Beyond the state border' };
        if (t.terrain === 'water') return { ok: false, reason: 'Cannot build on water' };
        if (t.buildingId) return { ok: false, reason: 'Tile occupied' };
        if (t.road) return { ok: false, reason: 'Tile has a road' };
        if (def.requiresDeposit && t.deposit === def.requiresDeposit) depositOk = true;
      }
    }
    if (def.isCustoms && this.borderEdge) {
      let atBorder = false;
      for (let dy = -1; dy <= h && !atBorder; dy++) for (let dx = -1; dx <= w && !atBorder; dx++) {
        const onEdge = dx === -1 || dx === w || dy === -1 || dy === h;
        const onCorner = (dx === -1 || dx === w) && (dy === -1 || dy === h);
        if (!onEdge || onCorner) continue;
        if (this.tiles[y + dy]?.[x + dx]?.foreign) atBorder = true;
      }
      if (!atBorder) return { ok: false, reason: 'A Customs House must stand at the national border' };
    }
    if (!depositOk) return { ok: false, reason: `Requires a ${def.requiresDeposit === 'ironOre' ? 'iron ore' : def.requiresDeposit} deposit` };
    if (def.requiresForest && this.countForestTiles(x, y, w, h) < 3) {
      return { ok: false, reason: 'Needs at least 3 forest tiles nearby' };
    }
    if (def.isFarm && this.countFarmFields(x, y, w, h) < 6) {
      return { ok: false, reason: 'Needs at least 6 open grass tiles around (fields)' };
    }
    if (def.isPort) {
      let shore = false;
      for (let dy = -1; dy <= h && !shore; dy++) for (let dx = -1; dx <= w && !shore; dx++) {
        const onEdge = dx === -1 || dx === w || dy === -1 || dy === h;
        const onCorner = (dx === -1 || dx === w) && (dy === -1 || dy === h);
        if (!onEdge || onCorner) continue;
        if (this.tiles[y + dy]?.[x + dx]?.terrain === 'water') shore = true;
      }
      if (!shore) return { ok: false, reason: 'Must be built on the shore, touching water' };
    }
    return { ok: true };
  }

  tryPlace(defId: string, x: number, y: number, instant: boolean): { ok: boolean; reason?: string } {
    const chk = this.canPlace(defId, x, y);
    if (!chk.ok) return chk;
    const def = BUILDINGS[defId];
    const rubleCost = defId === 'road' ? this.roadCostAt(x, y) : def.costRubles;
    if (instant) {
      const cost = Math.max(1, Math.ceil(rubleCost * DOLLAR_BUILD_RATE));
      if (this.dollars < cost) return { ok: false, reason: `Not enough dollars ($${cost})` };
      this.dollars -= cost;
      if (defId === 'road') {
        this.tiles[y][x].road = true;
        this.stats.roadsBuilt++;
      } else {
        this.placeFree(defId, x, y);
      }
      this.bump();
      return { ok: true };
    }
    if (this.rubles < rubleCost) return { ok: false, reason: `Not enough rubles (₽${rubleCost})` };
    this.rubles -= rubleCost;
    if (defId === 'road') {
      this.tiles[y][x].road = true;
      this.stats.roadsBuilt++;
      this.bump();
      return { ok: true };
    }
    const b: BuildingInst = {
      id: this.nextBuildingId++, defId, x, y, w: def.size[0], h: def.size[1],
      constructed: false, progress: 0, stock: {}, incoming: {},
      staff: 0, eff: 0, powered: false, heated: false, connected: false,
      coalFactor: 1, farmFields: 0,
    };
    this.buildings.set(b.id, b);
    for (let dy = 0; dy < b.h; dy++)
      for (let dx = 0; dx < b.w; dx++)
        this.tiles[y + dy][x + dx].buildingId = b.id;
    this.bump();
    return { ok: true };
  }

  instantCost(defId: string) {
    return Math.max(1, Math.ceil(BUILDINGS[defId].costRubles * DOLLAR_BUILD_RATE));
  }

  bulldozeAt(x: number, y: number): boolean {
    const t = this.tiles[y]?.[x];
    if (!t) return false;
    if (t.foreign) return false; // foreign soil (incl. the crossing lane) is untouchable
    if (t.buildingId) {
      const b = this.buildings.get(t.buildingId);
      if (!b) return false;
      for (let dy = 0; dy < b.h; dy++)
        for (let dx = 0; dx < b.w; dx++)
          this.tiles[b.y + dy][b.x + dx].buildingId = undefined;
      // trucks and barges en route turn around and return their cargo
      for (const tr of [...this.trucks, ...this.boats]) {
        if (tr.destId === b.id && tr.phase === 'go') {
          tr.phase = 'back';
          tr.daysDone = Math.max(0, tr.daysTotal - tr.daysDone);
        }
      }
      this.boatOrders = this.boatOrders.filter(o => o.srcId !== b.id && o.destId !== b.id);
      this.buildings.delete(b.id);
      this.bump();
      return true;
    }
    if (t.road) { t.road = false; this.bump(); return true; }
    return false;
  }

  // ---------------- trade ----------------

  priceOf(r: ResourceId, currency: 'east' | 'west') {
    const base = currency === 'east' ? RESOURCES[r].priceEast : RESOURCES[r].priceWest;
    return base * (currency === 'east' ? this.priceFactorEast : this.priceFactorWest);
  }

  importPriceOf(r: ResourceId, currency: 'east' | 'west') {
    return this.priceOf(r, currency) * IMPORT_MARKUP;
  }

  private customsCache: { version: number; field: DistanceField | null } | null = null;

  /** Road distances from the customs network, cached per engine version. */
  private customsField(): DistanceField | null {
    if (!this.customsCache || this.customsCache.version !== this.version) {
      const customs = [...this.buildings.values()].filter(b => this.def(b).isCustoms && b.constructed);
      const roads = customs.flatMap(c => this.adjacentRoads(c));
      this.customsCache = {
        version: this.version,
        field: roads.length ? this.floodFrom(roads).snapshot() : null,
      };
    }
    return this.customsCache.field;
  }

  /** Customs-connected buildings and how much each is willing to sell (supplyOf-protected). */
  private sellableSources(r: ResourceId): { b: BuildingInst; amt: number }[] {
    const field = this.customsField();
    if (!field) return [];
    const out: { b: BuildingInst; amt: number }[] = [];
    for (const b of this.buildings.values()) {
      if (!b.constructed) continue;
      const amt = this.supplyOf(b, r);
      if (amt < 0.01) continue;
      if (this.adjacentRoads(b).some(t => field.reachable(t.x, t.y))) out.push({ b, amt });
    }
    return out;
  }

  private sellableCache: { version: number; map: Map<ResourceId, number> } = { version: -1, map: new Map() };

  /** Stock that sell() could actually export right now. Cached per version. */
  sellableStock(r: ResourceId): number {
    if (this.sellableCache.version !== this.version) {
      this.sellableCache = { version: this.version, map: new Map() };
    }
    let v = this.sellableCache.map.get(r);
    if (v === undefined) {
      v = this.sellableSources(r).reduce((s, x) => s + x.amt, 0);
      this.sellableCache.map.set(r, v);
    }
    return v;
  }

  sell(r: ResourceId, amount: number, currency: 'east' | 'west'): { ok: boolean; msg: string } {
    const sources = this.sellableSources(r);
    if (!sources.length) return { ok: false, msg: 'No sellable goods connected to a Customs House' };
    let remaining = amount;
    for (const s of sources) {
      const take = Math.min(remaining, s.amt);
      this.addStock(s.b, r, -take);
      remaining -= take;
      if (remaining <= 0.001) break;
    }
    const sold = amount - Math.max(0, remaining);
    if (sold <= 0) return { ok: false, msg: 'Nothing to sell' };
    const price = this.priceOf(r, currency);
    if (currency === 'east') this.rubles += sold * price;
    else this.dollars += sold * price;
    this.stats.exportedValue += sold * (currency === 'east' ? price : price * 10);
    this.bump();
    return { ok: true, msg: `Sold ${sold.toFixed(0)} ${RESOURCES[r].name}` };
  }

  buy(r: ResourceId, amount: number, currency: 'east' | 'west'): { ok: boolean; msg: string } {
    const customs = [...this.buildings.values()].find(b => this.def(b).isCustoms && b.constructed);
    if (!customs) return { ok: false, msg: 'Build a Customs House first' };
    const price = this.importPriceOf(r, currency);
    const free = this.capOf(customs, r) - this.stockOf(customs, r) - this.incomingOf(customs, r);
    if (free < 1) return { ok: false, msg: 'Customs storage is full' };
    const funds = currency === 'east' ? this.rubles : this.dollars;
    const affordable = Math.floor(funds / price);
    if (affordable < 1) return { ok: false, msg: currency === 'east' ? 'Not enough rubles' : 'Not enough dollars' };
    const delivered = Math.min(amount, Math.floor(free), affordable);
    if (currency === 'east') this.rubles -= delivered * price;
    else this.dollars -= delivered * price;
    this.addStock(customs, r, delivered);
    this.bump();
    return { ok: true, msg: `Imported ${delivered.toFixed(0)} ${RESOURCES[r].name} to Customs` };
  }

  // ---------------- main loop ----------------

  private lastRunSpeed: 1 | 2 | 4 = 1;

  setSpeed(s: 0 | 1 | 2 | 4) {
    if (s !== 0) this.lastRunSpeed = s;
    this.speed = s;
    this.bump();
  }

  /** Pause, or resume at the speed the game was last running at. */
  togglePause() {
    this.setSpeed(this.speed === 0 ? this.lastRunSpeed : 0);
  }

  advance(dtMs: number) {
    if (this.speed === 0) return;
    const daysDelta = (dtMs / this.TICK_MS) * this.speed;
    // trucks and barges move continuously; today's weather slows everyone
    // mid-trip. Grounding weather (boatMult 0) stops new sailings, but a
    // barge already out limps on rather than stalling forever.
    const wx = WEATHER[this.weather.condition];
    this.moveFleet(this.trucks, daysDelta * wx.truckMult);
    this.moveFleet(this.boats, daysDelta * Math.max(0.4, wx.boatMult));
    this.acc += dtMs * this.speed;
    let days = 0;
    while (this.acc >= this.TICK_MS && days < 20) {
      this.acc -= this.TICK_MS;
      this.simulateDay();
      days++;
    }
  }

  /** Shared truck/barge lifecycle: deliver, return undelivered cargo, retire. */
  private moveFleet(fleet: Truck[], daysDelta: number) {
    for (let i = fleet.length - 1; i >= 0; i--) {
      const t = fleet[i];
      t.daysDone += daysDelta;
      if (t.daysDone < t.daysTotal) continue;
      if (t.phase === 'go') {
        const dest = this.buildings.get(t.destId);
        if (dest) {
          const delivered = this.addStock(dest, t.cargo, t.amount);
          dest.incoming[t.cargo] = Math.max(0, this.incomingOf(dest, t.cargo) - t.amount);
          t.amount -= delivered; // whatever didn't fit rides back to the source
        }
        t.phase = 'back';
        t.daysDone = 0;
      } else {
        if (t.amount > 0.001) {
          const src = this.buildings.get(t.srcId);
          if (src) this.addStock(src, t.cargo, t.amount);
        }
        fleet.splice(i, 1);
      }
    }
  }

  private simulateDay() {
    // advance date
    this.day++;
    if (this.day > 30) {
      this.day = 1; this.month++;
      if (this.month > 12) { this.month = 1; this.year++; this.pushEvent(`Happy New Year ${this.year}, comrade!`, 'info', 'star'); }
      this.monthlyEconomy();
      if (this.month === 10) this.pushEvent('Winter approaches — make sure your Heating Plant works!', 'bad', 'winter');
      if (this.month === 4) this.pushEvent('Spring sowing season begins.', 'info', 'spring');
    }

    this.updateWeather();
    this.updateConnectivity();
    this.assignWorkers();
    this.updatePowerHeat();
    this.production();
    this.foreignTrade();
    this.logistics();
    this.dispatchBoats();
    this.construction();
    this.citizens();
    this.computeTotals();
    this.checkObjectives();
    this.updateAlerts();
    this.bump();
  }

  private monthlyEconomy() {
    const drift = (v: number) => Math.min(1.15, Math.max(0.85, v + (this.rng() - 0.5) * 0.1));
    this.priceFactorEast = drift(this.priceFactorEast);
    this.priceFactorWest = drift(this.priceFactorWest);
  }

  // ---------------- systems ----------------

  private updateWeather() {
    const prev = this.weather;
    this.weather = this.weatherAt(this.dayIndex());
    const w = this.weather;
    const hasFarms = [...this.buildings.values()].some(b => this.def(b).isFarm && b.constructed);

    // drought bookkeeping: hot rainless days accumulate, any precipitation resets
    const wet = w.condition === 'rain' || w.condition === 'storm' || w.condition === 'snow' || w.condition === 'blizzard';
    if (wet) {
      if (this.dryStreak > BALANCE.droughtAfterDays && hasFarms) this.pushEvent('Rain breaks the drought — the fields recover.', 'good', 'rain');
      this.dryStreak = 0;
    } else if (w.tempC >= 18) {
      this.dryStreak++;
      if (this.dryStreak === BALANCE.droughtAfterDays + 1 && hasFarms) this.pushEvent('Drought — the fields are withering.', 'bad', 'summer');
    }

    // frost: one warning per cold spell while crops are growing
    const frost = w.tempC < 0 && (FARM_SEASON[this.month] ?? 0) > 0;
    if (frost && !this.wasFrost && hasFarms) this.pushEvent('Frost grips the fields — crops stop growing.', 'bad', 'freeze');
    this.wasFrost = frost;

    // morale streaks: long gray spells wear people down, sunny runs lift them
    const mood = WEATHER[w.condition].morale;
    if (mood < 0) { this.gloomStreak++; this.sunStreak = 0; }
    else if (mood > 0) { this.sunStreak++; this.gloomStreak = 0; }
    else { this.gloomStreak = Math.max(0, this.gloomStreak - 1); this.sunStreak = Math.max(0, this.sunStreak - 1); }

    // river freeze-over / break-up
    if (this.hasWater && w.riverFrozen !== prev.riverFrozen) {
      if (w.riverFrozen) this.pushEvent('The river has frozen over — barges are ice-locked until the thaw.', 'bad', 'freeze');
      else this.pushEvent('The ice breaks up — barges can sail again.', 'good', 'port');
    }
  }

  private updateConnectivity() {
    // a building "works" only if its road reaches the council depot network
    const depots = [...this.buildings.values()].filter(b => this.def(b).isDepot && b.constructed);
    const flood = depots.length ? this.floodFrom(depots.flatMap(d => this.adjacentRoads(d))) : null;
    for (const b of this.buildings.values()) {
      const adj = this.adjacentRoads(b);
      b.connected = adj.length > 0 && (!flood || adj.some(t => flood.distanceAt(t.x, t.y) >= 0));
    }
  }

  private assignWorkers() {
    this.workers = Math.floor(this.pop * BALANCE.workerShare);
    const list = [...this.buildings.values()]
      .filter(b => b.constructed && this.def(b).workers > 0 && b.connected)
      .sort((a, b2) => {
        const hi = Number(b2.priorityHigh ?? false) - Number(a.priorityHigh ?? false);
        if (hi !== 0) return hi;
        const pa = JOB_PRIORITY.indexOf(a.defId), pb = JOB_PRIORITY.indexOf(b2.defId);
        return (pa < 0 ? 99 : pa) - (pb < 0 ? 99 : pb);
      });
    this.jobs = list.reduce((s, b) => s + this.def(b).workers, 0);
    for (const b of this.buildings.values()) b.staff = 0;
    let pool = this.workers;
    // pass 1: every workplace gets a skeleton crew so all chains keep running
    for (const b of list) {
      if (pool <= 0) break;
      b.staff = 1;
      pool--;
    }
    // pass 2: distribute the rest proportionally to remaining open jobs
    const rem = list.map(b => this.def(b).workers - b.staff);
    const remTotal = rem.reduce((x, y) => x + y, 0);
    if (pool > 0 && remTotal > 0) {
      list.forEach((b, i) => { b.staff += Math.min(rem[i], Math.floor((pool * rem[i]) / remTotal)); });
      const used = list.reduce((x, b) => x + b.staff, 0);
      let left = this.workers - used;
      for (const b of list) {
        while (left > 0 && b.staff < this.def(b).workers) { b.staff++; left--; }
        if (left <= 0) break;
      }
    }
    this.employed = list.reduce((x, b) => x + b.staff, 0);
  }

  private baseEff(b: BuildingInst): number {
    const def = this.def(b);
    const staffRatio = def.workers > 0 ? b.staff / def.workers : 1;
    const powerFactor = def.power > 0 && !b.powered ? 0.5 : 1;
    return staffRatio * powerFactor;
  }

  private updatePowerHeat() {
    // Heat demand first (temperature-scaled) so plants can throttle to it:
    // mild days sip coal, a January cold snap burns through the stockpile.
    const heatFactor = this.heatDemandFactor();
    this.heatDemand = 0;
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (b.constructed && def.heat > 0) this.heatDemand += def.heat * heatFactor;
    }

    // Plants: fix eff & coalFactor for the whole day (powerFactor uses the
    // previous day's allocation). production() burns coal via productionRates()
    // with these same stored factors, so output and fuel always agree.
    this.powerProduced = 0;
    this.heatProduced = 0;
    let heatToServe = this.heatDemand;
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed || (!def.powerOutput && !def.heatOutput)) continue;
      const eff = this.baseEff(b);
      b.eff = eff;
      if (def.powerOutput) {
        const need = (def.inputs?.coal ?? 0) * eff;
        const have = this.stockOf(b, 'coal');
        b.coalFactor = need <= 0 ? 1 : Math.min(1, have / need);
        this.powerProduced += def.powerOutput * eff * b.coalFactor;
      }
      if (def.heatOutput) {
        // throttle to remaining demand; fuel burn scales with actual output
        const capacity = def.heatOutput * eff;
        const throttle = capacity > 0 ? Math.min(1, heatToServe / capacity) : 0;
        const need = (def.inputs?.coal ?? 0) * eff * throttle;
        const have = this.stockOf(b, 'coal');
        const fuel = need <= 0 ? 1 : Math.min(1, have / need);
        b.coalFactor = throttle * fuel;
        const out = capacity * b.coalFactor;
        this.heatProduced += out;
        heatToServe = Math.max(0, heatToServe - out);
      }
    }
    // demand & allocation (priority order)
    this.powerDemand = 0;
    for (const b of this.buildings.values()) {
      if (b.constructed) this.powerDemand += this.def(b).power;
    }
    const ordered = [...this.buildings.values()]
      .filter(b => b.constructed && this.def(b).power > 0)
      .sort((a, b2) => {
        const pa = JOB_PRIORITY.indexOf(a.defId), pb = JOB_PRIORITY.indexOf(b2.defId);
        return (pa < 0 ? 99 : pa) - (pb < 0 ? 99 : pb);
      });
    let budget = this.powerProduced;
    for (const b of ordered) {
      const need = this.def(b).power;
      if (budget >= need) { b.powered = true; budget -= need; }
      else b.powered = false;
    }
    for (const b of this.buildings.values()) if (this.def(b).power === 0) b.powered = true;

    // heat allocation
    const required = this.heatingRequired();
    for (const b of this.buildings.values()) {
      if (b.constructed && this.def(b).heat > 0) b.heated = !required; // warm days everyone is fine
    }
    if (required) {
      let hb = this.heatProduced;
      for (const b of this.buildings.values()) {
        const def = this.def(b);
        if (!b.constructed || def.heat === 0) continue;
        const need = def.heat * heatFactor;
        if (hb >= need - 1e-9) { b.heated = true; hb -= need; }
        else b.heated = false;
      }
    }
  }

  /**
   * Actual per-day resource flows for a building under current conditions.
   * production() applies exactly these deltas, and the UI displays them, so
   * the simulation and the inspector cannot diverge.
   */
  productionRates(b: BuildingInst): { inputs: Partial<Record<ResourceId, number>>; outputs: Partial<Record<ResourceId, number>> } {
    const rates: { inputs: Partial<Record<ResourceId, number>>; outputs: Partial<Record<ResourceId, number>> } = { inputs: {}, outputs: {} };
    const def = this.def(b);
    if (!b.constructed) return rates;

    // fuel burners: eff & coalFactor were fixed by updatePowerHeat this day
    if (def.powerOutput || def.heatOutput) {
      const burn = (def.inputs?.coal ?? 0) * b.eff * b.coalFactor;
      if (burn > 0) rates.inputs.coal = burn;
      return rates;
    }
    if (!def.outputs) return rates;

    const eff = this.baseEff(b);
    let outMul = eff;
    if (def.isFarm) {
      const fields = Math.min(12, this.countFarmFields(b.x, b.y, b.w, b.h));
      outMul = eff * (fields / 12) * (FARM_SEASON[this.month] ?? 0) * 2.2 * this.farmWeatherMult();
    }
    if (def.requiresForest) {
      outMul = eff * Math.min(1, this.countForestTiles(b.x, b.y, b.w, b.h) / 6);
    }

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
    for (const [r, amt] of Object.entries(def.outputs) as [ResourceId, number][]) rates.outputs[r] = amt * finalMul;
    return rates;
  }

  private production() {
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed) continue;
      if (!def.powerOutput && !def.heatOutput) {
        b.eff = this.baseEff(b); // plants keep the eff set by updatePowerHeat
        if (def.isFarm) b.farmFields = Math.min(12, this.countFarmFields(b.x, b.y, b.w, b.h));
      }
      const rates = this.productionRates(b);
      for (const [r, amt] of Object.entries(rates.inputs) as [ResourceId, number][]) {
        this.addStock(b, r, -amt);
      }
      for (const [r, amt] of Object.entries(rates.outputs) as [ResourceId, number][]) {
        this.stats.produced[r] += this.addStock(b, r, amt);
      }
    }
  }

  // ---------------- foreign trade (auto) ----------------

  /** Live town-wide stock incl. cargo on the road — auto-imports measure against this, not yesterday's totals. */
  private liveTownTotal(r: ResourceId): number {
    let total = 0;
    for (const b of this.buildings.values()) total += this.stockOf(b, r);
    for (const t of this.trucks) if (t.cargo === r) total += t.amount;
    for (const bt of this.boats) if (bt.cargo === r) total += bt.amount;
    return total;
  }

  /**
   * Standing orders of the Foreign Trade Directorate. Runs before logistics
   * (imports land in customs stock in time for today's trucks) and before
   * citizens (the reserve floor keeps wages safe from automation). Each
   * customs house clears a limited daily tonnage scaled by its staffing —
   * exports sell from its own stock (trucks stage them via logistics),
   * imports arrive into it. Manual panel trades stay instant.
   */
  private foreignTrade() {
    this.tradeLedger.yesterday = this.tradeLedger.today;
    const led = this.tradeLedger.today = emptyLedger();
    const customsHouses = [...this.buildings.values()]
      .filter(b => this.def(b).isCustoms && b.constructed)
      .sort((a, b) => a.id - b.id);
    for (const c of customsHouses) led.capacity += Math.floor(BALANCE.customsThroughputPerDay * c.eff);
    if (!this.autoTrade.enabled || !customsHouses.length) return;
    if (!ALL_RESOURCES.some(r => this.autoTrade.rules[r])) return;
    const blocked = (why: string) => { if (!led.blocked.includes(why)) led.blocked.push(why); };
    if (led.capacity <= 0) { blocked('customs house unstaffed'); return; }

    for (const c of customsHouses) {
      let budget = Math.floor(BALANCE.customsThroughputPerDay * c.eff);
      if (budget <= 0) continue;

      // exports first — earn before spending, straight from this customs' stock
      for (const r of ALL_RESOURCES) {
        if (budget <= 0) break;
        const rule = this.autoTrade.rules[r];
        if (rule?.mode !== 'export') continue;
        const amt = Math.min(budget, Math.floor(this.stockOf(c, r)));
        if (amt < 1) continue;
        this.addStock(c, r, -amt);
        const price = this.priceOf(r, rule.currency);
        const gain = amt * price;
        if (rule.currency === 'east') { this.rubles += gain; led.rubles += gain; }
        else { this.dollars += gain; led.dollars += gain; }
        this.stats.exportedValue += amt * (rule.currency === 'east' ? price : price * 10);
        led.exports[r] = (led.exports[r] ?? 0) + amt;
        led.used += amt;
        budget -= amt;
      }

      // imports — fill the town to each rule's level, throughput- and reserve-limited
      for (const r of ALL_RESOURCES) {
        if (budget <= 0) break;
        const rule = this.autoTrade.rules[r];
        if (rule?.mode !== 'import') continue;
        const deficit = Math.floor(rule.level - this.liveTownTotal(r));
        if (deficit < 1) continue;
        const free = Math.floor(this.capOf(c, r) - this.stockOf(c, r) - this.incomingOf(c, r));
        if (free < 1) { blocked('customs storage full'); continue; }
        const price = this.importPriceOf(r, rule.currency);
        const spendable = rule.currency === 'east'
          ? this.rubles - this.autoTrade.reserveRubles
          : this.dollars - this.autoTrade.reserveDollars;
        const affordable = Math.floor(spendable / price);
        if (affordable < 1) { blocked('treasury at reserve floor'); continue; }
        const amt = Math.min(deficit, budget, free, affordable);
        const cost = amt * price;
        if (rule.currency === 'east') { this.rubles -= cost; led.rubles -= cost; }
        else { this.dollars -= cost; led.dollars -= cost; }
        this.addStock(c, r, amt);
        led.imports[r] = (led.imports[r] ?? 0) + amt;
        led.used += amt;
        budget -= amt;
      }
    }
  }

  // ---------------- logistics ----------------

  private builderPool(): number {
    let n = 0;
    for (const b of this.buildings.values()) {
      if (this.def(b).isConstructionOffice && b.constructed && b.connected) {
        // contract crew guarantees the office works before you have citizens
        n += Math.max(10, b.staff);
      }
    }
    return n;
  }

  private maxTrucks(): number {
    let n = 0;
    for (const b of this.buildings.values()) {
      if (this.def(b).isConstructionOffice && b.constructed && b.connected) {
        n += 6 + Math.floor(BALANCE.maxActiveTrucksPerOffice * (b.staff / this.def(b).workers));
      }
    }
    return n;
  }

  /** stock a building is willing to give away */
  private supplyOf(b: BuildingInst, r: ResourceId): number {
    const def = this.def(b);
    if (!b.constructed) return 0;
    if (def.serviceType === 'shop' && (r === 'food' || r === 'clothes')) return 0;
    const keep = def.inputs?.[r] ? def.inputs[r] * 3 : 0;
    return Math.max(0, this.stockOf(b, r) - keep);
  }

  private logistics() {
    const maxT = this.maxTrucks();
    let budget = maxT - this.trucks.filter(t => t.phase === 'go').length;
    if (budget <= 0) return;

    const demands: LogisticsDemand[] = [];

    // adjacentRoads is O(perimeter); cache per building for this pass
    const adjCache = new Map<number, { x: number; y: number }[]>();
    const adjOf = (b: BuildingInst) => {
      let a = adjCache.get(b.id);
      if (!a) { a = this.adjacentRoads(b); adjCache.set(b.id, a); }
      return a;
    };

    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed) {
        // construction site materials
        for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
          const missing = amt - this.stockOf(b, r) - this.incomingOf(b, r);
          if (missing >= 1) demands.push({ b, r, amt: missing, prio: 16 });
        }
        continue;
      }
      // power & heating coal — critical, more so for plants short on power
      if ((def.powerOutput || def.heatOutput) && def.inputs?.coal) {
        const free = this.capOf(b, 'coal') - this.stockOf(b, 'coal') - this.incomingOf(b, 'coal');
        if (free >= 2) demands.push({ b, r: 'coal', amt: free, prio: b.powered || def.power === 0 ? 10 : 6 });
      }
      // store goods
      if (def.serviceType === 'shop') {
        const fFree = this.capOf(b, 'food') - this.stockOf(b, 'food') - this.incomingOf(b, 'food');
        if (fFree >= 6) demands.push({ b, r: 'food', amt: fFree, prio: 12 });
        const cFree = this.capOf(b, 'clothes') - this.stockOf(b, 'clothes') - this.incomingOf(b, 'clothes');
        if (cFree >= 4) demands.push({ b, r: 'clothes', amt: cFree, prio: 14 });
      }
      // factory inputs
      if (def.inputs && !def.powerOutput && !def.heatOutput) {
        for (const [r] of Object.entries(def.inputs) as [ResourceId, number][]) {
          const bufferTarget = this.capOf(b, r) * 0.6;
          const missing = bufferTarget - this.stockOf(b, r) - this.incomingOf(b, r);
          if (missing >= 6) demands.push({ b, r, amt: missing, prio: 20 });
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
        // surplus measured inland: what connected buildings would sell,
        // excluding stock already staged border-side
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
            demands.push({ b: c, r, amt: chunk, prio: 44, noCustomsSrc: true });
            left -= chunk;
          }
        }
      }
    }

    // overflow hauling: pin the overflowing producer as the supplier and
    // target the nearest storage with room, so the producer actually drains
    const storages = [...this.buildings.values()].filter(b =>
      (this.def(b).isDepot || this.def(b).isCustoms || b.defId === 'warehouse') && b.constructed);
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed || !def.outputs || def.serviceType) continue;
      const srcRoads = adjOf(b);
      if (!srcRoads.length) continue;
      let flood: FloodResult | null = null;
      for (const [r] of Object.entries(def.outputs) as [ResourceId, number][]) {
        const cap = this.capOf(b, r);
        if (cap <= 0 || this.stockOf(b, r) <= cap * 0.8) continue;
        flood ??= this.floodFrom(srcRoads);
        let best: BuildingInst | null = null;
        let bestD = Infinity;
        for (const s of storages) {
          if (s.id === b.id) continue;
          const free = this.capOf(s, r) - this.stockOf(s, r) - this.incomingOf(s, r);
          if (free < 4) continue;
          for (const t of adjOf(s)) {
            const d = flood.distanceAt(t.x, t.y);
            if (d >= 0 && d < bestD) { bestD = d; best = s; }
          }
        }
        if (best) {
          const free = this.capOf(best, r) - this.stockOf(best, r) - this.incomingOf(best, r);
          demands.push({ b: best, r, amt: Math.min(free, this.stockOf(b, r) - cap * 0.3), prio: 40, from: b.id });
        }
      }
    }

    demands.sort((a, b) => a.prio - b.prio);

    for (const d of demands) {
      if (budget <= 0) break;
      const destRoads = adjOf(d.b);
      if (!destRoads.length) continue;
      const destFree = d.b.constructed
        ? this.capOf(d.b, d.r) - this.stockOf(d.b, d.r) - this.incomingOf(d.b, d.r)
        : (this.def(d.b).materials[d.r] ?? 0) - this.stockOf(d.b, d.r) - this.incomingOf(d.b, d.r);
      if (destFree < 1) continue;

      // one flood from the destination ranks every candidate supplier by road distance
      const flood = this.floodFrom(destRoads);
      let bestSupplier: BuildingInst | null = null;
      let bestTile: { x: number; y: number } | null = null;
      let bestD = Infinity;
      for (const s of this.buildings.values()) {
        if (s.id === d.b.id) continue;
        if (d.from !== undefined && s.id !== d.from) continue;
        if (d.noCustomsSrc && this.def(s).isCustoms) continue; // staging never drains the border back inland
        if (this.supplyOf(s, d.r) < 1) continue;
        for (const t of adjOf(s)) {
          const dd = flood.distanceAt(t.x, t.y);
          if (dd >= 0 && dd < bestD) { bestD = dd; bestSupplier = s; bestTile = t; }
        }
      }
      if (!bestSupplier || !bestTile) {
        // nowhere on this road network can supply it — try relaying the goods
        // across water: truck to a far port, barge over, truck onward later
        if (d.from === undefined) this.relayViaPorts(d, flood, adjOf, demands);
        continue;
      }
      const bestPath = flood.pathFrom(bestTile.x, bestTile.y)!;

      const amount = Math.min(d.amt, destFree, this.supplyOf(bestSupplier, d.r), BALANCE.truckCapacity);
      if (amount < 1) continue;

      this.addStock(bestSupplier, d.r, -amount);
      d.b.incoming[d.r] = this.incomingOf(d.b, d.r) + amount;

      const pts = [this.centerOf(bestSupplier), ...bestPath, this.centerOf(d.b)];
      const daysTotal = Math.max(0.6, bestPath.length * BALANCE.truckDaysPerTile);
      this.trucks.push({
        id: this.nextTruckId++, points: pts, cargo: d.r, amount,
        daysTotal, daysDone: 0, phase: 'go', destId: d.b.id, srcId: bestSupplier.id,
      });
      budget--;
    }
  }

  /**
   * A demand no road-connected supplier can serve may still be servable
   * across water: register a twin demand at a far-shore port (trucks bring
   * the goods portside) plus a standing barge order to the near-shore port.
   * The original demand is then served from that port on a later day.
   * Flood-buffer discipline: consume `destFlood` fully before flooding again.
   */
  private relayViaPorts(
    d: LogisticsDemand,
    destFlood: FloodResult,
    adjOf: (b: BuildingInst) => { x: number; y: number }[],
    demands: LogisticsDemand[],
  ) {
    if (this.weather.riverFrozen) return; // no new relay chains onto an ice-locked river
    const ports = [...this.buildings.values()].filter(p => this.def(p).isPort && p.constructed);
    if (ports.length < 2) return;
    const pDest = ports.find(p => p.id !== d.b.id && adjOf(p).some(t => destFlood.distanceAt(t.x, t.y) >= 0));
    if (!pDest) return;
    const pending = this.boatOrders.find(o => o.destId === pDest.id && o.r === d.r);
    if (pending) {
      // order already exists — keep the far-shore truck leg alive until the
      // source port actually holds the goods (its truck may have lost the
      // dispatch budget on earlier days)
      const src = this.buildings.get(pending.srcId);
      if (src) {
        const short = pending.amt - this.stockOf(src, d.r) - this.incomingOf(src, d.r);
        if (short >= 1) demands.push({ b: src, r: d.r, amt: short, prio: d.prio });
      }
      return;
    }

    const wf = this.waterFlood(this.adjacentWater(pDest));
    const overWater = ports.filter(p =>
      p.id !== pDest.id && this.adjacentWater(p).some(t => wf.distanceAt(t.x, t.y) >= 0));
    for (const pSrc of overWater) {
      // does pSrc's own road network reach any willing supplier?
      const sf = this.floodFrom(this.adjacentRoads(pSrc));
      const supplied = [...this.buildings.values()].some(s =>
        s.id !== pSrc.id && this.supplyOf(s, d.r) >= 1 &&
        this.adjacentRoads(s).some(t => sf.distanceAt(t.x, t.y) >= 0));
      if (!supplied) continue;
      const amt = Math.min(
        d.amt,
        BALANCE.boatCapacity,
        this.capOf(pSrc, d.r) - this.stockOf(pSrc, d.r) - this.incomingOf(pSrc, d.r),
        this.capOf(pDest, d.r) - this.stockOf(pDest, d.r) - this.incomingOf(pDest, d.r),
      );
      if (amt < 1) return;
      demands.push({ b: pSrc, r: d.r, amt, prio: d.prio }); // truck leg on the far shore
      this.boatOrders.push({ srcId: pSrc.id, destId: pDest.id, r: d.r, amt });
      return;
    }
  }

  /** Sail pending freight orders whose goods have reached the source port. */
  private dispatchBoats() {
    const ports = [...this.buildings.values()].filter(p => this.def(p).isPort && p.constructed);
    if (!ports.length) { this.boatOrders = []; return; }
    // ice or grounding weather keeps barges in port — orders wait for fair skies
    if (this.weather.riverFrozen || WEATHER[this.weather.condition].boatMult === 0) return;
    for (let i = this.boatOrders.length - 1; i >= 0; i--) {
      if (this.boats.filter(b => b.phase === 'go').length >= ports.length) break;
      const order = this.boatOrders[i];
      const src = this.buildings.get(order.srcId);
      const dest = this.buildings.get(order.destId);
      if (!src?.constructed || !dest?.constructed) { this.boatOrders.splice(i, 1); continue; }
      const avail = this.stockOf(src, order.r);
      if (avail < 1) continue; // trucks are still bringing it portside

      const wf = this.waterFlood(this.adjacentWater(dest));
      let bestTile: { x: number; y: number } | null = null;
      let bestD = Infinity;
      for (const t of this.adjacentWater(src)) {
        const dd = wf.distanceAt(t.x, t.y);
        if (dd >= 0 && dd < bestD) { bestD = dd; bestTile = t; }
      }
      if (!bestTile) { this.boatOrders.splice(i, 1); continue; } // water link gone
      const path = wf.pathFrom(bestTile.x, bestTile.y)!;

      const amount = Math.min(order.amt, avail, BALANCE.boatCapacity,
        this.capOf(dest, order.r) - this.stockOf(dest, order.r) - this.incomingOf(dest, order.r));
      if (amount < 1) { this.boatOrders.splice(i, 1); continue; }
      this.addStock(src, order.r, -amount);
      dest.incoming[order.r] = this.incomingOf(dest, order.r) + amount;
      this.boats.push({
        id: this.nextBoatId++,
        points: [this.centerOf(src), ...path, this.centerOf(dest)],
        cargo: order.r, amount,
        daysTotal: Math.max(1, path.length * BALANCE.boatDaysPerTile),
        daysDone: 0, phase: 'go', destId: dest.id, srcId: src.id,
      });
      order.amt -= amount;
      if (order.amt < 1) this.boatOrders.splice(i, 1);
    }
  }

  // ---------------- construction ----------------

  private construction() {
    let pool = this.builderPool();
    if (pool <= 0) return;
    for (const b of this.buildings.values()) {
      if (pool <= 0) break;
      if (b.constructed) continue;
      const def = this.def(b);
      // all materials delivered?
      const ready = (Object.entries(def.materials) as [ResourceId, number][])
        .every(([r, amt]) => this.stockOf(b, r) >= amt - 0.001);
      if (!ready) continue;
      const crew = Math.min(BALANCE.buildersPerSite, pool);
      b.progress += crew * WEATHER[this.weather.condition].buildMult; // storms slow the site
      pool -= crew;
      if (b.progress >= def.labor) {
        b.constructed = true;
        b.progress = def.labor;
        // consume materials
        for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
          this.addStock(b, r, -amt);
        }
        this.pushEvent(`${def.name} completed!`, 'good', 'check');
      }
    }
  }

  // ---------------- citizens ----------------

  private citizens() {
    // capacity
    this.capacity = 0;
    const housing: BuildingInst[] = [];
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (b.constructed && def.housingCapacity) {
        this.capacity += def.housingCapacity;
        housing.push(b);
      }
    }

    // services coverage
    const servicesOf = (type: 'shop' | 'health' | 'culture') =>
      [...this.buildings.values()].filter(b => {
        const def = this.def(b);
        return b.constructed && def.serviceType === type && b.staff > 0;
      });
    const coveredRatio = (type: 'shop' | 'health' | 'culture') => {
      if (this.capacity === 0) return 0;
      const svcs = servicesOf(type);
      if (!svcs.length) return 0;
      let covered = 0;
      for (const h of housing) {
        const hc = this.centerOf(h);
        const ok = svcs.some(s => {
          const sc = this.centerOf(s);
          return Math.max(Math.abs(hc.x - sc.x), Math.abs(hc.y - sc.y)) <= (this.def(s).serviceRadius ?? BALANCE.serviceRadius);
        });
        if (ok) covered += this.def(h).housingCapacity!;
      }
      return covered / this.capacity;
    };

    const shopCov = coveredRatio('shop');
    const healthCov = coveredRatio('health');
    const cultureCov = coveredRatio('culture');
    this.sat.health = this.lerp(this.sat.health, healthCov, 0.1);
    this.sat.culture = this.lerp(this.sat.culture, cultureCov, 0.1);

    // food & clothes consumption from stores
    const stores = servicesOf('shop');
    const consume = (r: ResourceId, perCapita: number, satKey: 'food' | 'clothes') => {
      const demand = this.pop * perCapita;
      if (demand <= 0) { this.sat[satKey] = this.lerp(this.sat[satKey], 1, 0.1); return; }
      const coveredDemand = demand * shopCov;
      let available = 0;
      for (const s of stores) available += this.stockOf(s, r);
      const served = Math.min(coveredDemand, available);
      // consume proportionally
      if (available > 0) {
        for (const s of stores) {
          const share = this.stockOf(s, r) / available;
          this.addStock(s, r, -served * share);
        }
      }
      const raw = served / demand;
      this.sat[satKey] = this.lerp(this.sat[satKey], Math.min(1, raw), 0.12);
    };
    consume('food', BALANCE.foodPerCitizen, 'food');
    consume('clothes', BALANCE.clothesPerCitizen, 'clothes');

    // power / heat satisfaction
    let poweredCap = 0, heatedCap = 0;
    for (const h of housing) {
      if (h.powered) poweredCap += this.def(h).housingCapacity!;
      if (h.heated) heatedCap += this.def(h).housingCapacity!;
    }
    this.sat.power = this.lerp(this.sat.power, this.capacity ? poweredCap / this.capacity : 1, 0.15);
    this.sat.heat = this.lerp(this.sat.heat, this.capacity ? heatedCap / this.capacity : 1, 0.15);

    // employment
    this.sat.employment = this.workers > 0
      ? Math.min(1, this.employed / (this.workers * 0.95))
      : 1;

    // pollution
    const polluters = [...this.buildings.values()].filter(b => {
      const def = this.def(b);
      return b.constructed && def.pollution && b.eff > 0;
    });
    if (this.capacity > 0 && polluters.length) {
      let penaltySum = 0;
      for (const h of housing) {
        const hc = this.centerOf(h);
        let pl = 0;
        for (const p of polluters) {
          const pc = this.centerOf(p);
          if (Math.max(Math.abs(hc.x - pc.x), Math.abs(hc.y - pc.y)) <= BALANCE.pollutionRadius) {
            pl += this.def(p).pollution!;
          }
        }
        penaltySum += Math.max(0.6, 1 - 0.05 * pl) * this.def(h).housingCapacity!;
      }
      this.sat.pollution = this.lerp(this.sat.pollution, penaltySum / this.capacity, 0.1);
    } else {
      this.sat.pollution = this.lerp(this.sat.pollution, 1, 0.1);
    }

    // happiness
    const w = this.sat;
    let target = 100 * (
      0.30 * w.food + 0.14 * w.clothes + 0.12 * w.power + 0.12 * w.heat +
      0.10 * w.culture + 0.10 * w.health + 0.12 * w.employment
    ) * w.pollution;
    // unpaid wages
    const wages = this.employed * BALANCE.wagePerWorker;
    if (this.rubles >= wages) {
      this.rubles -= wages;
      this.wagesUnpaid = false;
    } else {
      this.wagesUnpaid = true;
      target *= 0.85;
    }
    // weather morale: long gray spells wear on people, sunny runs lift them
    target *= 1 - Math.min(0.06, this.gloomStreak * 0.01) + Math.min(0.02, this.sunStreak * 0.005);
    this.happiness = this.lerp(this.happiness, Math.max(0, Math.min(100, target)), 0.2);

    // migration — settlers only (re)found the republic while its reputation holds
    const freeBeds = this.capacity - this.pop;
    if (this.pop === 0 && freeBeds > 0 && this.happiness >= 48) {
      this.pop = Math.min(freeBeds, 6);
      this.pushEvent('First settlers arrived to your republic!', 'good', 'users');
    } else if (this.happiness >= 48 && freeBeds > 0) {
      const arrivals = Math.min(freeBeds, 1 + Math.floor(this.happiness / 35));
      this.pop += arrivals;
      if (arrivals > 1) this.pushEvent(`${arrivals} migrants joined your republic`, 'good', 'users');
    } else if (this.happiness < 30 && this.pop > 0) {
      const departures = Math.min(this.pop, Math.max(1, Math.min(Math.ceil(this.pop * 0.1), Math.ceil((30 - this.happiness) / 8))));
      this.pop -= departures;
      this.pushEvent(`${departures} citizens left the republic (unhappy)`, 'bad', 'users');
    }
    if (this.pop > this.capacity) this.pop = this.capacity;
  }

  private lerp(a: number, b: number, t: number) { return a + (b - a) * t; }

  // ---------------- totals, objectives, alerts ----------------

  private computeTotals() {
    for (const r of ALL_RESOURCES) this.totals[r] = 0;
    for (const b of this.buildings.values()) {
      for (const r of ALL_RESOURCES) this.totals[r] += this.stockOf(b, r);
    }
  }

  private checkObjectives() {
    for (const o of OBJECTIVES) {
      if (this.objectivesDone.includes(o.id)) continue;
      let done = false;
      switch (o.id) {
        case 'roads': done = this.stats.roadsBuilt >= 10; break;
        case 'housing': done = this.pop >= 20; break;
        case 'shop': done = [...this.buildings.values()].some(b => this.def(b).serviceType === 'shop' && b.constructed && this.stockOf(b, 'food') >= 5); break;
        case 'sow': done = [...this.buildings.values()].some(b => this.def(b).isFarm && b.constructed); break;
        case 'builders': done = this.stats.produced.planks >= 20 && this.stats.produced.bricks >= 20; break;
        case 'coal': done = this.stats.produced.coal >= 30; break;
        case 'power': done = this.powerProduced >= 8; break;
        case 'heat': done = [...this.buildings.values()].some(b => this.def(b).heatOutput && b.constructed && b.staff > 0); break;
        case 'steel': done = this.stats.produced.steel >= 15; break;
        case 'foodchain': done = this.stats.produced.food >= 25; break;
        case 'export': done = this.stats.exportedValue >= 5000; break;
        case 'pop150': done = this.pop >= 150; break;
        case 'flourish': done = this.pop >= 300 && this.happiness >= 65; break;
      }
      if (done) {
        this.objectivesDone.push(o.id);
        if (o.rewardRubles) this.rubles += o.rewardRubles;
        if (o.rewardDollars) this.dollars += o.rewardDollars;
        const rw = [o.rewardRubles ? `+₽${o.rewardRubles.toLocaleString()}` : '', o.rewardDollars ? `+$${o.rewardDollars.toLocaleString()}` : ''].filter(Boolean).join(' ');
        this.pushEvent(`Objective complete: ${o.title}! ${rw}`, 'good', 'star');
      }
    }
  }

  private updateAlerts() {
    const a: Alert[] = [];
    if (this.wagesUnpaid) a.push({ id: 'wages', icon: 'coins', text: 'Treasury empty — wages unpaid!', level: 'bad' });
    if (this.pop > 5 && this.sat.food < 0.5) a.push({ id: 'food', icon: 'food', text: 'Food shortage — citizens are hungry', level: 'bad' });
    const hasPlant = [...this.buildings.values()].some(b => this.def(b).powerOutput && b.constructed);
    if (this.powerDemand > this.powerProduced + 0.01 && (hasPlant || this.pop > 0)) a.push({ id: 'power', icon: 'power', text: `Power deficit (${this.powerDemand.toFixed(1)} MW needed, ${this.powerProduced.toFixed(1)} MW generated)`, level: 'warn' });
    if (this.heatingRequired() && this.capacity > 0 && this.sat.heat < 0.8) a.push({ id: 'heat', icon: 'freeze', text: 'Heating shortage — citizens are freezing', level: 'bad' });
    if (this.weather.riverFrozen && [...this.buildings.values()].some(b => this.def(b).isPort && b.constructed)) {
      a.push({ id: 'ice', icon: 'freeze', text: 'River frozen — barges ice-locked until the thaw', level: 'warn' });
    }
    const tomorrow = this.forecast(1)[0];
    if (tomorrow.condition === 'storm' || tomorrow.condition === 'blizzard') {
      a.push({ id: 'stormfront', icon: tomorrow.condition, text: `${tomorrow.condition === 'storm' ? 'Storm' : 'Blizzard'} front approaches — expect slow roads tomorrow`, level: 'warn' });
    }
    const unconnected = [...this.buildings.values()].filter(b => b.constructed && !b.connected).length;
    if (unconnected > 0) a.push({ id: 'roads', icon: 'road', text: `${unconnected} building${unconnected > 1 ? 's' : ''} not connected to a road`, level: 'warn' });
    const sites = [...this.buildings.values()].filter(b => !b.constructed);
    if (sites.length > 0 && this.builderPool() === 0) a.push({ id: 'builders', icon: 'builders', text: 'No builders available — construction halted', level: 'warn' });
    if (this.maxTrucks() === 0) a.push({ id: 'trucks', icon: 'truck', text: 'No trucks — staff a Construction Office to haul goods', level: 'warn' });
    if (this.jobs > this.workers && this.workers > 0) a.push({ id: 'labor', icon: 'users', text: 'Labor shortage — not enough workers for all jobs', level: 'warn' });
    const customs = [...this.buildings.values()].some(b => this.def(b).isCustoms && b.constructed);
    if (!customs) a.push({ id: 'customs', icon: 'trade', text: 'No Customs House — foreign trade impossible', level: 'warn' });
    if (this.tradeLedger.today.blocked.length) {
      a.push({ id: 'autotrade', icon: 'trade', text: `Auto-trade stalled — ${this.tradeLedger.today.blocked.join('; ')}`, level: 'warn' });
    }
    this.alerts = a;
  }

  /**
   * The contiguous (8-way) deposit cluster at a tile, and the mine working
   * it if any. Null when the tile has no deposit. Inspection API for the UI.
   */
  depositClusterAt(x: number, y: number): { kind: DepositType; tiles: { x: number; y: number }[]; exploitedBy: BuildingInst | null } | null {
    const start = this.tiles[y]?.[x];
    if (!start?.deposit) return null;
    const kind = start.deposit;
    const seen = new Set<number>([y * MAP_W + x]);
    const stack = [{ x, y }];
    const tiles: { x: number; y: number }[] = [];
    let exploitedBy: BuildingInst | null = null;
    while (stack.length) {
      const cur = stack.pop()!;
      tiles.push(cur);
      const bid = this.tiles[cur.y][cur.x].buildingId;
      if (bid) {
        const b = this.buildings.get(bid);
        if (b && this.def(b).requiresDeposit === kind) exploitedBy ??= b;
      }
      for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
        if (dx === 0 && dy === 0) continue;
        const nx = cur.x + dx, ny = cur.y + dy;
        const k = ny * MAP_W + nx;
        if (seen.has(k)) continue;
        if (this.tiles[ny]?.[nx]?.deposit === kind) { seen.add(k); stack.push({ x: nx, y: ny }); }
      }
    }
    return { kind, tiles, exploitedBy };
  }

  /** Flip a building's staffing priority (UI action — keeps mutation + notification in the engine). */
  toggleStaffPriority(id: number) {
    const b = this.buildings.get(id);
    if (!b) return;
    b.priorityHigh = !b.priorityHigh;
    this.bump();
  }

  /** Daily citizen demand for a resource (what stores would sell at full coverage). */
  citizenDemandOf(r: ResourceId): number {
    if (r === 'food') return this.pop * BALANCE.foodPerCitizen;
    if (r === 'clothes') return this.pop * BALANCE.clothesPerCitizen;
    return 0;
  }

  // ---------------- auto-trade policy (UI actions) ----------------

  setAutoTradeEnabled(on: boolean) {
    this.autoTrade.enabled = on;
    this.bump();
  }

  /** One rule per resource: import OR export — setting one replaces the other (no buy-high/sell-low churn). */
  setAutoTradeRule(r: ResourceId, rule: AutoTradeRule | null) {
    if (rule) this.autoTrade.rules[r] = { ...rule, level: Math.max(0, Math.round(rule.level)) };
    else delete this.autoTrade.rules[r];
    this.bump();
  }

  setAutoTradeReserve(currency: 'east' | 'west', amt: number) {
    const v = Math.max(0, Math.round(amt));
    if (currency === 'east') this.autoTrade.reserveRubles = v;
    else this.autoTrade.reserveDollars = v;
    this.bump();
  }

  /** Set staffing priority for several buildings at once (multi-selection action). */
  setStaffPriorityMany(ids: number[], on: boolean) {
    let changed = false;
    for (const id of ids) {
      const b = this.buildings.get(id);
      if (b && (b.priorityHigh ?? false) !== on) {
        b.priorityHigh = on;
        changed = true;
      }
    }
    if (changed) this.bump();
  }

  // ---------------- events / subscription ----------------

  private pushEvent(text: string, kind: GameEvent['kind'], icon?: string) {
    this.events.push({ id: this.nextEventId++, text, kind, icon });
  }

  drainEvents(): GameEvent[] {
    const e = this.events;
    this.events = [];
    return e;
  }

  subscribe(fn: () => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  getVersion() { return this.version; }

  private bump() {
    this.version++;
    this.listeners.forEach(fn => fn());
  }
}
