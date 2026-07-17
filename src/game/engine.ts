// ============================================================
// Red Republic — game engine & simulation
// ============================================================
import {
  BUILDINGS, RESOURCES, ALL_RESOURCES, BALANCE, FARM_SEASON,
  DOLLAR_BUILD_RATE, IMPORT_MARKUP, OBJECTIVES,
} from './config';
import type { ResourceId } from './config';
import { generateMap, mulberry32, MAP_W, MAP_H } from './mapgen';
import type { MapData, Tile } from './mapgen';
import { floodRoads, FloodResult } from './pathfind';
import type { DistanceField } from './pathfind';

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
}

export type Season = 'winter' | 'spring' | 'summer' | 'autumn';

const JOB_PRIORITY = [
  'powerPlant', 'heatingPlant', 'store', 'foodFactory',
  'clinic', 'pub', 'customs', 'farm', 'textileMill', 'sawmill', 'brickworks',
  'woodcutter', 'gravelQuarry', 'coalMine', 'ironMine', 'steelMill',
  'oilPump', 'refinery', 'depot', 'warehouse', 'constructionOffice',
];

export class GameEngine {
  tiles: Tile[][];
  buildings = new Map<number, BuildingInst>();
  trucks: Truck[] = [];
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

  private nextBuildingId = 1;
  private nextTruckId = 1;
  private nextEventId = 1;
  private acc = 0;
  private events: GameEvent[] = [];
  private listeners = new Set<() => void>();
  private version = 0;

  readonly TICK_MS = 500; // one game day at 1x speed

  readonly seed: number;
  private rng: () => number;

  constructor(opts: { seed?: number; map?: MapData; skipStartingBase?: boolean } = {}) {
    this.seed = opts.seed ?? Math.floor(Math.random() * 2 ** 31);
    this.rng = mulberry32(this.seed ^ 0x9e3779b9); // decorrelate from map generation
    const map = opts.map ?? generateMap(this.seed);
    this.tiles = map.tiles;
    if (!opts.skipStartingBase) this.setupStartingBase(map.startX, map.startY);
  }

  // ---------------- setup ----------------

  private setupStartingBase(sx: number, sy: number) {
    // road line north of buildings
    for (let x = sx - 2; x <= sx + 4; x++) {
      this.tiles[sy - 1][x].road = true;
    }
    this.placeFree('depot', sx, sy);
    this.placeFree('constructionOffice', sx - 2, sy);
    this.placeFree('customs', sx + 3, sy);
    const depot = [...this.buildings.values()].find(b => b.defId === 'depot')!;
    depot.stock = { planks: 120, bricks: 120, steel: 50, food: 100 };
    this.pushEvent('The Politburo has granted you this land. Build a thriving socialist republic!', 'info');
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

  isHeatingSeason() { return BALANCE.winterMonths.includes(this.month); }

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

  canPlace(defId: string, x: number, y: number): { ok: boolean; reason?: string } {
    const def = BUILDINGS[defId];
    if (defId === 'road') {
      const t = this.tiles[y]?.[x];
      if (!t) return { ok: false, reason: 'Out of bounds' };
      if (t.terrain === 'water') return { ok: false, reason: 'Cannot build on water' };
      if (t.road) return { ok: false, reason: 'Road already here' };
      if (t.buildingId) return { ok: false, reason: 'Occupied by a building' };
      return { ok: true };
    }
    const [w, h] = def.size;
    if (x < 0 || y < 0 || x + w > MAP_W || y + h > MAP_H) return { ok: false, reason: 'Out of bounds' };
    let depositOk = !def.requiresDeposit;
    for (let dy = 0; dy < h; dy++) {
      for (let dx = 0; dx < w; dx++) {
        const t = this.tiles[y + dy][x + dx];
        if (t.terrain === 'water') return { ok: false, reason: 'Cannot build on water' };
        if (t.buildingId) return { ok: false, reason: 'Tile occupied' };
        if (t.road) return { ok: false, reason: 'Tile has a road' };
        if (def.requiresDeposit && t.deposit === def.requiresDeposit) depositOk = true;
      }
    }
    if (!depositOk) return { ok: false, reason: `Requires a ${def.requiresDeposit === 'ironOre' ? 'iron ore' : def.requiresDeposit} deposit` };
    if (def.requiresForest && this.countForestTiles(x, y, w, h) < 3) {
      return { ok: false, reason: 'Needs at least 3 forest tiles nearby' };
    }
    if (def.isFarm && this.countFarmFields(x, y, w, h) < 6) {
      return { ok: false, reason: 'Needs at least 6 open grass tiles around (fields)' };
    }
    return { ok: true };
  }

  tryPlace(defId: string, x: number, y: number, instant: boolean): { ok: boolean; reason?: string } {
    const chk = this.canPlace(defId, x, y);
    if (!chk.ok) return chk;
    const def = BUILDINGS[defId];
    if (instant) {
      const cost = this.instantCost(defId);
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
    if (this.rubles < def.costRubles) return { ok: false, reason: `Not enough rubles (₽${def.costRubles})` };
    this.rubles -= def.costRubles;
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
    if (t.buildingId) {
      const b = this.buildings.get(t.buildingId);
      if (!b) return false;
      for (let dy = 0; dy < b.h; dy++)
        for (let dx = 0; dx < b.w; dx++)
          this.tiles[b.y + dy][b.x + dx].buildingId = undefined;
      // trucks en route turn around and return their cargo to the source
      for (const tr of this.trucks) {
        if (tr.destId === b.id && tr.phase === 'go') {
          tr.phase = 'back';
          tr.daysDone = Math.max(0, tr.daysTotal - tr.daysDone);
        }
      }
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

  setSpeed(s: 0 | 1 | 2 | 4) { this.speed = s; this.bump(); }

  advance(dtMs: number) {
    if (this.speed === 0) return;
    const daysDelta = (dtMs / this.TICK_MS) * this.speed;
    // trucks move continuously
    this.moveTrucks(daysDelta);
    this.acc += dtMs * this.speed;
    let days = 0;
    while (this.acc >= this.TICK_MS && days < 20) {
      this.acc -= this.TICK_MS;
      this.simulateDay();
      days++;
    }
  }

  private moveTrucks(daysDelta: number) {
    const arrived: Truck[] = [];
    for (const t of this.trucks) {
      t.daysDone += daysDelta;
      if (t.daysDone >= t.daysTotal) {
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
          arrived.push(t);
        }
      }
    }
    if (arrived.length) {
      const ids = new Set(arrived.map(t => t.id));
      this.trucks = this.trucks.filter(t => !ids.has(t.id));
    }
  }

  private simulateDay() {
    // advance date
    this.day++;
    if (this.day > 30) {
      this.day = 1; this.month++;
      if (this.month > 12) { this.month = 1; this.year++; this.pushEvent(`Happy New Year ${this.year}, comrade!`, 'info'); }
      this.monthlyEconomy();
      if (this.month === 10) this.pushEvent('❄️ Winter approaches — make sure your Heating Plant works!', 'bad');
      if (this.month === 4) this.pushEvent('🌱 Spring sowing season begins.', 'info');
    }

    this.updateConnectivity();
    this.assignWorkers();
    this.updatePowerHeat();
    this.production();
    this.logistics();
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
    // Plants: fix eff & coalFactor for the whole day (powerFactor uses the
    // previous day's allocation). production() burns coal via productionRates()
    // with these same stored factors, so output and fuel always agree.
    this.powerProduced = 0;
    this.heatProduced = 0;
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed || (!def.powerOutput && !def.heatOutput)) continue;
      const eff = this.baseEff(b);
      const need = (def.inputs?.coal ?? 0) * eff;
      const have = this.stockOf(b, 'coal');
      b.coalFactor = need <= 0 ? 1 : Math.min(1, have / need);
      b.eff = eff;
      if (def.powerOutput) this.powerProduced += def.powerOutput * eff * b.coalFactor;
      if (def.heatOutput) this.heatProduced += def.heatOutput * eff * b.coalFactor;
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

    // heat
    this.heatDemand = 0;
    const heating = this.isHeatingSeason();
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (b.constructed && def.heat > 0) {
        this.heatDemand += def.heat;
        b.heated = !heating; // outside season everyone is fine
      }
    }
    if (heating) {
      let hb = this.heatProduced;
      for (const b of this.buildings.values()) {
        const def = this.def(b);
        if (!b.constructed || def.heat === 0) continue;
        if (hb >= def.heat) { b.heated = true; hb -= def.heat; }
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
      outMul = eff * (fields / 12) * (FARM_SEASON[this.month] ?? 0) * 2.2;
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
    const keep = def.inputs?.[r] ? def.inputs[r]! * 3 : 0;
    return Math.max(0, this.stockOf(b, r) - keep);
  }

  private logistics() {
    const maxT = this.maxTrucks();
    let budget = maxT - this.trucks.filter(t => t.phase === 'go').length;
    if (budget <= 0) return;

    interface Demand { b: BuildingInst; r: ResourceId; amt: number; prio: number; from?: number }
    const demands: Demand[] = [];

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
        if (this.supplyOf(s, d.r) < 1) continue;
        for (const t of adjOf(s)) {
          const dd = flood.distanceAt(t.x, t.y);
          if (dd >= 0 && dd < bestD) { bestD = dd; bestSupplier = s; bestTile = t; }
        }
      }
      if (!bestSupplier || !bestTile) continue;
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
      b.progress += crew;
      pool -= crew;
      if (b.progress >= def.labor) {
        b.constructed = true;
        b.progress = def.labor;
        // consume materials
        for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
          this.addStock(b, r, -amt);
        }
        this.pushEvent(`✅ ${def.name} completed!`, 'good');
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
    this.happiness = this.lerp(this.happiness, Math.max(0, Math.min(100, target)), 0.2);

    // migration — settlers only (re)found the republic while its reputation holds
    const freeBeds = this.capacity - this.pop;
    if (this.pop === 0 && freeBeds > 0 && this.happiness >= 48) {
      this.pop = Math.min(freeBeds, 6);
      this.pushEvent('👷 First settlers arrived to your republic!', 'good');
    } else if (this.happiness >= 48 && freeBeds > 0) {
      const arrivals = Math.min(freeBeds, 1 + Math.floor(this.happiness / 35));
      this.pop += arrivals;
      if (arrivals > 1) this.pushEvent(`👥 ${arrivals} migrants joined your republic`, 'good');
    } else if (this.happiness < 30 && this.pop > 0) {
      const departures = Math.min(this.pop, Math.max(1, Math.min(Math.ceil(this.pop * 0.1), Math.ceil((30 - this.happiness) / 8))));
      this.pop -= departures;
      this.pushEvent(`🚶 ${departures} citizens left the republic (unhappy)`, 'bad');
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
        this.pushEvent(`⭐ Objective complete: ${o.title}! ${rw}`, 'good');
      }
    }
  }

  private updateAlerts() {
    const a: Alert[] = [];
    if (this.wagesUnpaid) a.push({ id: 'wages', icon: '💸', text: 'Treasury empty — wages unpaid!', level: 'bad' });
    if (this.pop > 5 && this.sat.food < 0.5) a.push({ id: 'food', icon: '🍞', text: 'Food shortage — citizens are hungry', level: 'bad' });
    const hasPlant = [...this.buildings.values()].some(b => this.def(b).powerOutput && b.constructed);
    if (this.powerDemand > this.powerProduced + 0.01 && (hasPlant || this.pop > 0)) a.push({ id: 'power', icon: '⚡', text: `Power deficit (${this.powerDemand.toFixed(1)} MW needed, ${this.powerProduced.toFixed(1)} MW generated)`, level: 'warn' });
    if (this.isHeatingSeason() && this.capacity > 0 && this.sat.heat < 0.8) a.push({ id: 'heat', icon: '🥶', text: 'Heating shortage — citizens are freezing', level: 'bad' });
    const unconnected = [...this.buildings.values()].filter(b => b.constructed && !b.connected).length;
    if (unconnected > 0) a.push({ id: 'roads', icon: '🛣️', text: `${unconnected} building${unconnected > 1 ? 's' : ''} not connected to a road`, level: 'warn' });
    const sites = [...this.buildings.values()].filter(b => !b.constructed);
    if (sites.length > 0 && this.builderPool() === 0) a.push({ id: 'builders', icon: '🏗️', text: 'No builders available — construction halted', level: 'warn' });
    if (this.maxTrucks() === 0) a.push({ id: 'trucks', icon: '🚚', text: 'No trucks — staff a Construction Office to haul goods', level: 'warn' });
    if (this.jobs > this.workers && this.workers > 0) a.push({ id: 'labor', icon: '👷', text: 'Labor shortage — not enough workers for all jobs', level: 'warn' });
    const customs = [...this.buildings.values()].some(b => this.def(b).isCustoms && b.constructed);
    if (!customs) a.push({ id: 'customs', icon: '🛃', text: 'No Customs House — foreign trade impossible', level: 'warn' });
    this.alerts = a;
  }

  /** Flip a building's staffing priority (UI action — keeps mutation + notification in the engine). */
  toggleStaffPriority(id: number) {
    const b = this.buildings.get(id);
    if (!b) return;
    b.priorityHigh = !b.priorityHigh;
    this.bump();
  }

  // ---------------- events / subscription ----------------

  private pushEvent(text: string, kind: GameEvent['kind']) {
    this.events.push({ id: this.nextEventId++, text, kind });
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
