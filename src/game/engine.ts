// ============================================================
// Red Republic — game engine & simulation
// ============================================================
import {
  BUILDINGS, RESOURCES, ALL_RESOURCES, BALANCE, CONTRACTS, LOANS, WEATHER,
  INSTANT_BUILD, IMPORT_MARKUP,
  DEFAULT_CLIMATE, DIFFICULTIES, DEFAULT_DIFFICULTY, POWER_SECTORS,
} from './config';
import type { Category, ClimateId, DepositType, DifficultyId, ResourceId } from './config';
import { generateMap } from './mapgen';
import { mulberry32 } from '@/lib/rng';
import type { BorderEdge, MapData, Tile } from './mapgen';
import { SAVE_FORMAT_VERSION, packTiles, unpackTiles, validateSave } from './save-format';
import type { SaveGameV1 } from './save-format';
import { FloodResult } from './pathfind';
import { forEachPerimeterTile, shareAnyComponent } from './topology';
import type { TopologyDomain, TopologyPos } from './topology';
import type { DayWeather } from './weather';
import { fmtQty, fmtOwed, fmtMoney } from './format';
import { applyMutations, underSystem } from './mutation';
import type { Mutation } from './mutation';
import { boats } from './systems/boats';
import { citizens } from './systems/citizens';
import { logistics } from './systems/logistics';
import { contracts } from './systems/contracts';
import { connectivity } from './systems/connectivity';
import { construction } from './systems/construction';
import { foreignTrade } from './systems/foreign-trade';
import { refuelVehicles, syncFleet } from './systems/fleet';
import { loans } from './systems/loans';
import { objectives } from './systems/objectives';
import { powerHeat } from './systems/power-heat';
import { production } from './systems/production';
import { totals } from './systems/totals';
import { weather } from './systems/weather';
import { workers } from './systems/workers';
import {
  World,
} from './world';
import { DEMAND_CATEGORY } from './world';
import type { DemandKind, LogisticsCategory } from './world';
import type {
  Alert, AutoTradeRule, BoatOrder, Boat, BuildingInst, Contract, GameEvent,
  HappinessBreakdown, HappinessFactor, InternalTilePatch, Loan, Mover, PlacePolicy,
  RoutingDiagnostics, Season, TilePatch, TradeDayLedger, Truck, Vehicle, VehicleState,
} from './world';

// The domain model lives in world.ts so systems can name it without importing
// the engine. Re-exported here because the UI, renderer and tests have always
// imported these from './engine' — that entry point stays valid.
export {
  DEFAULT_ALLOCATION_PRIORITY, DEFAULT_UNPOWERED_EFF, buildingWorn, truckWorldPos,
} from './world';
export type {
  Alert, AutoTradeRule, BoatOrder, Boat, BuildingInst, Contract, GameEvent,
  DemandKind, HappinessBreakdown, HappinessFactor, Loan, LogisticsCategory, Mover, PlacePolicy,
  RoutingDiagnostics, Season, TilePatch, TradeDayLedger, Truck, Vehicle, VehicleState,
} from './world';

/**
 * What a delivery is FOR. Drives the consequence weight and which drain model
 * applies. There is no priority number: urgency is computed from how many days
 * of operation the destination has left, not declared here.
 */
export class GameEngine {
  /**
   * The simulated world — map, buildings and the primitives over them.
   *
   * Systems being extracted take a `World` rather than the engine, so this is
   * the seam the whole refactor is built on. The forwarding members below keep
   * the engine's public surface identical for the UI, renderer and tests; they
   * thin out as each system moves to `world.ts`'s side of the line.
   */
  readonly w: World;

  get tiles(): readonly (readonly Readonly<Tile>[])[] { return this.w.tiles; }
  get buildings() { return this.w.buildings; }
  get mapW() { return this.w.mapW; }
  get mapH() { return this.w.mapH; }
  get borderEdge(): BorderEdge | null { return this.w.borderEdge; }
  private get topology() { return this.w.topology; }
  private get routingDay() { return this.w.routingDay; }
  private set routingDay(v: Omit<RoutingDiagnostics, 'topologyRebuilds'>) { this.w.routingDay = v; }
  private get nextBuildingId() { return this.w.nextBuildingId; }
  private set nextBuildingId(v: number) { this.w.nextBuildingId = v; }

  speed: 0 | 1 | 2 | 4 = 1;

  // The fleet, the treasury and every standing policy are World state too — the
  // transactions read and write them, so they live where the systems can reach
  // them. These forward for the UI, the renderer, saves and tests.
  get trucks() { return this.w.trucks; }
  set trucks(v: Vehicle[]) { this.w.trucks = v; }
  get boats() { return this.w.boats; }
  set boats(v: Boat[]) { this.w.boats = v; }
  get foreignTrucks() { return this.w.foreignTrucks; }
  set foreignTrucks(v: Mover[]) { this.w.foreignTrucks = v; }
  private get boatOrders() { return this.w.boatOrders; }
  private set boatOrders(v: BoatOrder[]) { this.w.boatOrders = v; }
  private get nextTruckId() { return this.w.nextTruckId; }
  private set nextTruckId(v: number) { this.w.nextTruckId = v; }
  private get nextBoatId() { return this.w.nextBoatId; }
  private set nextBoatId(v: number) { this.w.nextBoatId = v; }

  get rubles() { return this.w.rubles; }
  set rubles(v: number) { this.w.rubles = v; }
  get dollars() { return this.w.dollars; }
  set dollars(v: number) { this.w.dollars = v; }
  get priceFactorEast() { return this.w.priceFactorEast; }
  set priceFactorEast(v: number) { this.w.priceFactorEast = v; }
  get priceFactorWest() { return this.w.priceFactorWest; }
  set priceFactorWest(v: number) { this.w.priceFactorWest = v; }
  get autoTrade() { return this.w.autoTrade; }
  set autoTrade(v: World['autoTrade']) { this.w.autoTrade = v; }
  get tradeLedger() { return this.w.tradeLedger; }
  set tradeLedger(v: World['tradeLedger']) { this.w.tradeLedger = v; }
  get globalConstructionEnabled() { return this.w.globalConstructionEnabled; }
  set globalConstructionEnabled(v: boolean) { this.w.globalConstructionEnabled = v; }
  get foreignLaborEnabled() { return this.w.foreignLaborEnabled; }
  set foreignLaborEnabled(v: boolean) { this.w.foreignLaborEnabled = v; }
  get foreignLaborCurrency() { return this.w.foreignLaborCurrency; }
  set foreignLaborCurrency(v: 'east' | 'west') { this.w.foreignLaborCurrency = v; }
  get repairImportsEnabled() { return this.w.repairImportsEnabled; }
  set repairImportsEnabled(v: boolean) { this.w.repairImportsEnabled = v; }
  get repairImportCurrency() { return this.w.repairImportCurrency; }
  set repairImportCurrency(v: 'east' | 'west') { this.w.repairImportCurrency = v; }
  get contracts() { return this.w.contracts; }
  set contracts(v: Contract[]) { this.w.contracts = v; }
  private get nextContractId() { return this.w.nextContractId; }
  private set nextContractId(v: number) { this.w.nextContractId = v; }
  get relationsPenalty() { return this.w.relationsPenalty; }
  set relationsPenalty(v: World['relationsPenalty']) { this.w.relationsPenalty = v; }
  get loans() { return this.w.loans; }
  set loans(v: Loan[]) { this.w.loans = v; }
  private get nextLoanId() { return this.w.nextLoanId; }
  private set nextLoanId(v: number) { this.w.nextLoanId = v; }
  get loanAutoRepay() { return this.w.loanAutoRepay; }
  set loanAutoRepay(v: World['loanAutoRepay']) { this.w.loanAutoRepay = v; }
  get loanCooldown() { return this.w.loanCooldown; }
  set loanCooldown(v: World['loanCooldown']) { this.w.loanCooldown = v; }
  get globalCategoryPriorities() { return this.w.globalCategoryPriorities; }
  set globalCategoryPriorities(v: World['globalCategoryPriorities']) { this.w.globalCategoryPriorities = v; }
  get logisticsCategoryWeights() { return this.w.logisticsCategoryWeights; }
  set logisticsCategoryWeights(v: World['logisticsCategoryWeights']) { this.w.logisticsCategoryWeights = v; }
  get emergencyFuelAutoBuy() { return this.w.emergencyFuelAutoBuy; }
  set emergencyFuelAutoBuy(v: boolean) { this.w.emergencyFuelAutoBuy = v; }

  // The calendar, the weather and the republic's measured condition are World
  // state — every derivation writes them, so they belong where the systems can
  // reach them. These forward for the UI, the renderer and the tests.
  get day() { return this.w.day; }
  set day(v: number) { this.w.day = v; }
  get month() { return this.w.month; }
  set month(v: number) { this.w.month = v; }
  get year() { return this.w.year; }
  set year(v: number) { this.w.year = v; }
  get weather() { return this.w.weather; }
  set weather(v: DayWeather) { this.w.weather = v; }
  get weatherScript() { return this.w.weatherScript; }
  set weatherScript(v: ((dayIndex: number) => Partial<DayWeather>) | undefined) { this.w.weatherScript = v; }
  private get dryStreak() { return this.w.dryStreak; }
  private set dryStreak(v: number) { this.w.dryStreak = v; }
  private get gloomStreak() { return this.w.gloomStreak; }
  private set gloomStreak(v: number) { this.w.gloomStreak = v; }
  private get sunStreak() { return this.w.sunStreak; }
  private set sunStreak(v: number) { this.w.sunStreak = v; }
  private get wasFrost() { return this.w.wasFrost; }
  private set wasFrost(v: boolean) { this.w.wasFrost = v; }

  get pop() { return this.w.pop; }
  set pop(v: number) { this.w.pop = v; }
  get capacity() { return this.w.capacity; }
  set capacity(v: number) { this.w.capacity = v; }
  get workers() { return this.w.workers; }
  set workers(v: number) { this.w.workers = v; }
  get employed() { return this.w.employed; }
  set employed(v: number) { this.w.employed = v; }
  get jobs() { return this.w.jobs; }
  set jobs(v: number) { this.w.jobs = v; }
  get happiness() { return this.w.happiness; }
  set happiness(v: number) { this.w.happiness = v; }
  get sat() { return this.w.sat; }
  get powerProduced() { return this.w.powerProduced; }
  set powerProduced(v: number) { this.w.powerProduced = v; }
  get powerDemand() { return this.w.powerDemand; }
  set powerDemand(v: number) { this.w.powerDemand = v; }
  get heatProduced() { return this.w.heatProduced; }
  set heatProduced(v: number) { this.w.heatProduced = v; }
  get heatDemand() { return this.w.heatDemand; }
  set heatDemand(v: number) { this.w.heatDemand = v; }
  get totals() { return this.w.totals; }
  get stats() { return this.w.stats; }
  get objectivesDone() { return this.w.objectivesDone; }
  set objectivesDone(v: string[]) { this.w.objectivesDone = v; }
  get alerts() { return this.w.alerts; }
  set alerts(v: Alert[]) { this.w.alerts = v; }
  get powerSectorOrder() { return this.w.powerSectorOrder; }
  set powerSectorOrder(v: Category[]) { this.w.powerSectorOrder = v; }


  private acc = 0;
  private listeners = new Set<() => void>();
  private version = 0;

  readonly TICK_MS = 500; // one game day at 1x speed

  /** Climate region driving the weather timeline. Fixed for the whole run. */
  readonly climate: ClimateId;
  /** Difficulty preset (start conditions, plus the border's import markup). */
  get difficulty() { return this.w.difficulty; }
  /** The republic's name (player-chosen at founding; shown in HUD and saves). */
  name: string;
  get seed() { return this.w.seed; }
  private get rng() { return this.w.rng; }

  constructor(opts: {
    seed?: number; map?: MapData; mapW?: number; mapH?: number;
    climate?: ClimateId; difficulty?: DifficultyId; name?: string;
    skipStartingBase?: boolean; weatherScript?: (dayIndex: number) => Partial<DayWeather>;
  } = {}) {
    const seed = opts.seed ?? Math.floor(Math.random() * 2 ** 31);
    const difficulty = opts.difficulty ?? DEFAULT_DIFFICULTY;
    this.climate = opts.climate ?? DEFAULT_CLIMATE;
    this.name = opts.name ?? 'Red Republic';
    const map = opts.map ?? generateMap(seed, opts.mapW, opts.mapH);
    // World owns the map, the buildings on it, the topology over both, the
    // calendar/weather, the republic's condition, its fleet and its ledger. Its
    // RNG and weather timeline are decorrelated from map generation, so
    // constructing it after generateMap cannot perturb either stream.
    this.w = new World(map.tiles, map.border ?? null, seed, this.climate, difficulty, opts.weatherScript);
    this.rubles = DIFFICULTIES[this.difficulty].startRubles;
    this.dollars = DIFFICULTIES[this.difficulty].startDollars;
    if (!opts.skipStartingBase) this.setupStartingBase(map);
  }

  // ---------------- setup ----------------

  /** Apply controlled non-gameplay tile setup changes as one observable update.
   * Building ownership is intentionally unavailable here; footprints are only
   * mutated by add/removeBuilding below. */
  applyTilePatches(patches: readonly TilePatch[]): void {
    if (this.applyInternalTilePatches(patches)) this.bump();
  }

  // Tile, footprint and routing primitives now live on World. These forward so
  // the engine's public surface is unchanged for the UI, renderer and tests;
  // they thin out as each system moves to taking a World directly.
  private applyInternalTilePatches(patches: readonly InternalTilePatch[]): boolean {
    return this.w.applyInternalTilePatches(patches);
  }
  private setRoadTile(x: number, y: number, road: boolean): void { this.w.setRoadTile(x, y, road); }
  private clearFootprint(b: BuildingInst): void { this.w.clearFootprint(b); }
  private addBuilding(b: BuildingInst): void { this.w.addBuilding(b); }
  private removeBuilding(b: BuildingInst): void { this.w.removeBuilding(b); }

  private markConstructed(b: BuildingInst): void { this.w.markConstructed(b); }

  private setupStartingBase(map: MapData) {
    const sx = map.startX, sy = map.startY;
    // road line north of buildings
    this.applyInternalTilePatches(Array.from({ length: 4 }, (_, i) =>
      ({ x: sx - 2 + i, y: sy - 1, road: true })));
    this.placeFree('depot', sx, sy);
    this.placeFree('constructionOffice', sx - 2, sy);
    if (map.border && map.crossX !== undefined && map.crossY !== undefined) {
      // the customs house is the border crossing itself
      this.placeFree('customs', map.crossX, map.crossY);
      this.layCrossingRoads(map.border, map.crossX, map.crossY, sx, sy);
    } else {
      // borderless (test) maps keep the legacy row layout
      this.placeFree('customs', sx + 3, sy);
      this.applyInternalTilePatches(Array.from({ length: 3 }, (_, i) =>
        ({ x: sx + 2 + i, y: sy - 1, road: true })));
    }
    const depot = [...this.buildings.values()].find(b => b.defId === 'depot')!;
    const mult = DIFFICULTIES[this.difficulty].depotStockMult;
    depot.stock = {
      planks: Math.round(120 * mult), bricks: Math.round(120 * mult),
      steel: Math.round(50 * mult), food: Math.round(100 * mult),
      gravel: Math.round(80 * mult), machinery: Math.round(2 * mult),
      // Every lorry burns fuel, so the grant includes a fuel ration. Without
      // it day one has no haulage at all and nothing can fetch more.
      fuel: Math.round(40 * mult),
    };
    const office = [...this.buildings.values()].find(b => this.def(b).isConstructionOffice);
    if (office) this.addStock(office, 'fuel', Math.round(30 * mult));
    this.runStaged(syncFleet); // the granted lorries exist from day one
    this.pushEvent('The Politburo has granted you this land. Build a thriving socialist republic!', 'info', 'star');
  }

  /** Lane through the foreign strip to the map edge — every customs house is a crossing. */
  private layCrossingLane(edge: BorderEdge, cx: number, cy: number) {
    const patches: InternalTilePatch[] = [];
    const lay = (x: number, y: number) => {
      const t = this.tiles[y]?.[x];
      if (t && !t.buildingId) patches.push({ x, y, road: true }); // over water this is a bridge
    };
    if (edge === 'W') for (let x = 0; x < cx; x++) lay(x, cy);
    if (edge === 'E') for (let x = cx + 2; x < this.mapW; x++) lay(x, cy);
    if (edge === 'N') for (let y = 0; y < cy; y++) lay(cx, y);
    if (edge === 'S') for (let y = cy + 2; y < this.mapH; y++) lay(cx, y);
    this.applyInternalTilePatches(patches);
  }

  /** Border crossing: the strip lane plus a domestic link to the base. */
  private layCrossingRoads(edge: BorderEdge, cx: number, cy: number, sx: number, sy: number) {
    this.layCrossingLane(edge, cx, cy);
    const patches: InternalTilePatch[] = [];
    const lay = (x: number, y: number) => {
      const t = this.tiles[y]?.[x];
      if (t && !t.buildingId) patches.push({ x, y, road: true });
    };
    // domestic link: the front-door tile, then an L to the base road row
    const front = edge === 'W' ? { x: cx + 2, y: cy }
      : edge === 'E' ? { x: cx - 1, y: cy }
      : edge === 'N' ? { x: cx, y: cy + 2 }
      : { x: cx, y: cy - 1 };
    for (let y = Math.min(front.y, sy - 1); y <= Math.max(front.y, sy - 1); y++) lay(front.x, y);
    for (let x = Math.min(front.x, sx - 2); x <= Math.max(front.x, sx + 1); x++) lay(x, sy - 1);
    this.applyInternalTilePatches(patches);
  }

  private placeFree(defId: string, x: number, y: number) {
    const def = BUILDINGS[defId];
    const b: BuildingInst = {
      id: this.nextBuildingId++, defId, x, y, w: def.size[0], h: def.size[1],
      constructed: true, progress: def.labor, stock: {}, incoming: {},
      staff: 0, eff: 0, powered: false, heated: false, connected: false, roadConnected: false,
      coalFactor: 1, farmFields: 0,
    };
    this.seedWearBins(b);
    this.addBuilding(b);
  }

  private seedWearBins(b: BuildingInst) { this.w.seedWearBins(b); }

  // ---------------- helpers ----------------

  def(b: BuildingInst) { return this.w.def(b); }

  season(): Season { return this.w.season(); }
  heatingRequired() { return this.w.heatingRequired(); }
  heatDemandFactor(): number { return this.w.heatDemandFactor(); }
  farmWeatherMult(): number { return this.w.farmWeatherMult(); }
  dayIndex(): number { return this.w.dayIndex(); }
  private weatherAt(index: number): DayWeather { return this.w.weatherAt(index); }
  forecast(days = 5): DayWeather[] { return this.w.forecast(days); }

  buildingAt(x: number, y: number) { return this.w.buildingAt(x, y); }
  stockOf(b: BuildingInst, r: ResourceId) { return this.w.stockOf(b, r); }
  incomingOf(b: BuildingInst, r: ResourceId) { return this.w.incomingOf(b, r); }
  capOf(b: BuildingInst, r: ResourceId) { return this.w.capOf(b, r); }
  addStock(b: BuildingInst, r: ResourceId, amt: number) { return this.w.addStock(b, r, amt); }
  adjacentRoads(b: BuildingInst) { return this.w.adjacentRoads(b); }
  private waterAccess(b: BuildingInst) { return this.w.waterAccess(b); }
  private floodTerrain(sources: readonly TopologyPos[]): FloodResult { return this.w.floodTerrain(sources); }
  accessTiles(b: BuildingInst) { return this.w.accessTiles(b); }
  adjacentWater(b: BuildingInst) { return this.w.adjacentWater(b); }
  findPath(from: readonly TopologyPos[], to: readonly TopologyPos[]) { return this.w.findPath(from, to); }
  centerOf(b: BuildingInst) { return this.w.centerOf(b); }
  topologyRevision(domain: TopologyDomain): number { return this.w.topologyRevision(domain); }
  countFarmFields(x: number, y: number, w: number, h: number) { return this.w.countFarmFields(x, y, w, h); }
  countForestTiles(x: number, y: number, w: number, h: number) { return this.w.countForestTiles(x, y, w, h); }

  // ---------------- placement ----------------

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
    if (x < 0 || y < 0 || x + w > this.mapW || y + h > this.mapH) return { ok: false, reason: 'Out of bounds' };
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
      forEachPerimeterTile(x, y, w, h, {}, (px, py) => {
        if (this.tiles[py]?.[px]?.foreign) { atBorder = true; return true; }
      });
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
      forEachPerimeterTile(x, y, w, h, {}, (px, py) => {
        if (this.tiles[py]?.[px]?.terrain === 'water') { shore = true; return true; }
      });
      if (!shore) return { ok: false, reason: 'Must be built on the shore, touching water' };
    }
    return { ok: true };
  }

  tryPlace(defId: string, x: number, y: number, policy: PlacePolicy = {}): { ok: boolean; reason?: string } {
    const chk = this.canPlace(defId, x, y);
    if (!chk.ok) return chk;
    if (policy.instant) {
      // instant build = importing a Western prefab: dollars, no site, no wait
      const cost = this.instantCost(defId, x, y);
      if (this.dollars < cost) return { ok: false, reason: `Not enough dollars ($${cost})` };
      this.dollars -= cost;
      if (defId === 'road') {
        this.setRoadTile(x, y, true);
        this.stats.roadsBuilt++;
      } else {
        this.placeFree(defId, x, y);
        if (BUILDINGS[defId].isCustoms && this.borderEdge) this.layCrossingLane(this.borderEdge, x, y);
      }
      this.bump();
      return { ok: true };
    }
    // Domestic construction costs no money — only materials and labor.
    // A road painted on water becomes a bridge site (plank+steel bill).
    const effId = defId === 'road' && this.tiles[y][x].terrain === 'water' ? 'bridge' : defId;
    const def = BUILDINGS[effId];
    const currency: 'east' | 'west' = policy.currency ?? 'east';
    const wantsImport = !!policy.autoBuy && effId !== 'road'; // roads use domestic gravel; bridges DO import

    let autoBought = false;
    let importCurrency: 'east' | 'west' | undefined;
    let bondedCustomsId: number | undefined;

    if (policy.plan) {
      // Planning mode: place the blueprint but order and charge NOTHING —
      // materials and labor wait until commenceSite() pays and unpauses it.
      // Carry the import intent so commence knows which currency to charge.
      if (wantsImport) { autoBought = true; importCurrency = currency; }
    } else if (wantsImport) {
      // Auto-buy: pay the import bill upfront (₽ East / $ West); the exact
      // materials then arrive as bonded imports earmarked to THIS site.
      const customs = this.nearestConstructedCustoms(x, y);
      if (!customs) return { ok: false, reason: 'Build a Customs House first' };
      const cost = this.autoBuyImportCost(defId, x, y, currency);
      const funds = currency === 'east' ? this.rubles : this.dollars;
      if (funds < cost) return { ok: false, reason: currency === 'east'
        ? `Not enough rubles (₽${cost.toLocaleString()})`
        : `Not enough dollars ($${cost.toLocaleString()})` };
      if (currency === 'east') this.rubles -= cost; else this.dollars -= cost;
      for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][])
        this.stats.imported[r] = (this.stats.imported[r] ?? 0) + amt;
      autoBought = true;
      importCurrency = currency;
      bondedCustomsId = customs.id;
    }

    const b: BuildingInst = {
      id: this.nextBuildingId++, defId: effId, x, y, w: def.size[0], h: def.size[1],
      constructed: false, progress: 0, stock: {}, incoming: {},
      staff: 0, eff: 0, powered: false, heated: false, connected: false, roadConnected: false,
      coalFactor: 1, farmFields: 0,
      autoBought, bondedCustomsId, importCurrency,
      foreignLabor: policy.foreignLabor ?? this.foreignLaborEnabled,
      paused: policy.plan ?? false,
    };
    this.addBuilding(b);
    if (def.isCustoms && this.borderEdge) this.layCrossingLane(this.borderEdge, x, y);
    this.bump();
    return { ok: true };
  }

  /**
   * Dollar price of the Western prefab: the materials bill at import prices
   * plus a labor surcharge, with a convenience premium. Static base prices —
   * the magic path stays decoupled from market drift and relations. Pass
   * coordinates so a road on water prices as a bridge.
   */
  instantCost(defId: string, x?: number, y?: number): number {
    const effId = defId === 'road' && x !== undefined
      && this.tiles[y!]?.[x]?.terrain === 'water' ? 'bridge' : defId;
    const def = BUILDINGS[effId];
    let mats = 0;
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      mats += amt * RESOURCES[r].priceWest;
    }
    return Math.max(1, Math.ceil(
      (mats * IMPORT_MARKUP + def.labor * INSTANT_BUILD.laborDollars) * INSTANT_BUILD.premium));
  }

  /** Money to import a building's full material bill at current import prices —
   *  what auto-buy charges upfront and the build menu displays. `currency`
   *  picks the bloc (₽ East / $ West). Bridge-aware via coords (a road on water
   *  prices its plank+steel bridge bill). */
  autoBuyImportCost(defId: string, x?: number, y?: number, currency: 'east' | 'west' = 'east'): number {
    const effId = defId === 'road' && x !== undefined
      && this.tiles[y!]?.[x]?.terrain === 'water' ? 'bridge' : defId;
    const def = BUILDINGS[effId];
    let total = 0;
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      total += amt * this.importPriceOf(r, currency);
    }
    return Math.round(total);
  }

  private nearestConstructedCustoms(x: number, y: number) { return this.w.nearestConstructedCustoms(x, y); }

  /** Money to import a SITE's still-needed materials (bill minus delivered and
   *  in-flight) at current import prices — what enabling auto-buy mid-build, or
   *  commencing a planned auto-buy site, charges. */
  autoBuyRemainingCost(id: number, currency: 'east' | 'west' = 'east'): number {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return 0;
    const def = this.def(b);
    let total = 0;
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      const missing = Math.max(0, amt - this.stockOf(b, r) - this.incomingOf(b, r));
      total += missing * this.importPriceOf(r, currency);
    }
    return Math.round(total);
  }

  /** $ to instantly finish an existing site: the Western-prefab price of the
   *  work that REMAINS (undelivered materials + unbuilt labor), same markup and
   *  premium as a fresh instant build. */
  instantFinishCost(id: number): number {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return 0;
    const def = this.def(b);
    let mats = 0;
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      mats += Math.max(0, amt - this.stockOf(b, r)) * RESOURCES[r].priceWest;
    }
    const laborLeft = Math.max(0, def.labor - b.progress);
    return Math.max(1, Math.ceil(
      (mats * IMPORT_MARKUP + laborLeft * INSTANT_BUILD.laborDollars) * INSTANT_BUILD.premium));
  }

  /** Pause or unpause construction on a single site. Unpausing an auto-bought site
   *  that has not yet been paid charges the auto-buy bill (planning defers auto-buy). */
  setSitePaused(id: number, paused: boolean): { ok: boolean; reason?: string } {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return { ok: false, reason: 'Not an active site' };
    if (b.paused === paused) return { ok: true };
    if (!paused && b.autoBought && !b.bondedCustomsId) {
      const currency = b.importCurrency ?? 'east';
      const customs = this.nearestConstructedCustoms(b.x, b.y);
      if (!customs) return { ok: false, reason: 'Build a Customs House first' };
      const cost = this.autoBuyRemainingCost(id, currency);
      const funds = currency === 'east' ? this.rubles : this.dollars;
      if (funds < cost) return { ok: false, reason: currency === 'east'
        ? `Not enough rubles (₽${cost.toLocaleString()})`
        : `Not enough dollars ($${cost.toLocaleString()})` };
      if (currency === 'east') this.rubles -= cost; else this.dollars -= cost;
      const def = this.def(b);
      for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
        const missing = Math.max(0, amt - this.stockOf(b, r) - this.incomingOf(b, r));
        if (missing > 0) this.stats.imported[r] = (this.stats.imported[r] ?? 0) + missing;
      }
      b.bondedCustomsId = customs.id;
    }
    b.paused = paused;
    this.bump();
    return { ok: true };
  }

  /** Begin construction on a planned (paused) site: charge any auto-buy bill NOW
   *  (planning defers it to commence, not placement), then unpause so materials
   *  and builders flow. Fails if the site isn't planned or the bill can't be paid. */
  commenceSite(id: number): { ok: boolean; reason?: string } {
    const b = this.buildings.get(id);
    if (!b || b.constructed || !b.paused) return { ok: false, reason: 'Not a planned site' };
    return this.setSitePaused(id, false);
  }

  /** Commence every planned site the treasury can afford, highest construction
   *  priority first (then id order), so scarce funds buy what the player marked
   *  urgent. commenceSite re-checks affordability and skips what it can't pay —
   *  never overspends. Returns the number started. */
  commenceAllPlanned(): number {
    let n = 0;
    const planned = [...this.buildings.values()]
      .filter(b => b.paused)
      .sort((a, b) => this.effectiveBuildPriority(b) - this.effectiveBuildPriority(a) || a.id - b.id);
    for (const b of planned) {
      if (this.commenceSite(b.id).ok) n++;
    }
    return n;
  }

  /** Number of planned (paused, not-yet-commenced) sites. */
  plannedCount(): number {
    let n = 0;
    for (const b of this.buildings.values()) if (b.paused) n++;
    return n;
  }

  /** Total ₽/$ that commencing every planned site would charge right now,
   *  bucketed by the bloc each site imports from. Only auto-buy sites cost
   *  anything at commence; a plain planned site is free. Live (subtracts
   *  already-delivered + in-flight materials) — recompute, don't cache. */
  plannedCommenceCost(): { rubles: number; dollars: number } {
    let rubles = 0, dollars = 0;
    for (const b of this.buildings.values()) {
      if (!b.paused || !b.autoBought) continue;
      const currency = b.importCurrency ?? 'east';
      const cost = this.autoBuyRemainingCost(b.id, currency);
      if (currency === 'east') rubles += cost; else dollars += cost;
    }
    return { rubles, dollars };
  }

  /** Toggle whether a site may hire paid foreign builders beyond its citizens. */
  setSiteForeignLabor(id: number, on: boolean): void {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return;
    b.foreignLabor = on;
    this.bump();
  }

  /** Turn a site's material import (auto-buy) on — paying the REMAINING bill now
   *  in the chosen bloc's currency — or off. Enabling requires a constructed
   *  customs and the funds; a paused site only records the intent (charged at
   *  commence); disabling stops future bonded top-ups (paid cargo is not refunded).
   *  Reapplying the current state is a no-op. An unpaid paused intent may change
   *  currency directly; a paid active site must be disabled before changing it. */
  setSiteImport(id: number, currency: 'east' | 'west' | null): { ok: boolean; reason?: string } {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return { ok: false, reason: 'Not a construction site' };
    if (currency === null) {
      if (!b.autoBought) return { ok: true };
      b.autoBought = false;
      b.bondedCustomsId = undefined;
      this.bump();
      return { ok: true };
    }
    if (b.autoBought && (b.importCurrency ?? 'east') === currency) return { ok: true };
    if (b.autoBought && !b.paused) {
      return { ok: false, reason: 'Disable auto-buy before changing import currency' };
    }
    if (b.paused) {
      // planning mode: record the intent; commenceSite() pays the bill.
      b.autoBought = true;
      b.importCurrency = currency;
      this.bump();
      return { ok: true };
    }
    const customs = this.nearestConstructedCustoms(b.x, b.y);
    if (!customs) return { ok: false, reason: 'Build a Customs House first' };
    const cost = this.autoBuyRemainingCost(id, currency);
    const funds = currency === 'east' ? this.rubles : this.dollars;
    if (funds < cost) return { ok: false, reason: currency === 'east'
      ? `Not enough rubles (₽${cost.toLocaleString()})`
      : `Not enough dollars ($${cost.toLocaleString()})` };
    if (currency === 'east') this.rubles -= cost; else this.dollars -= cost;
    const def = this.def(b);
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      const missing = Math.max(0, amt - this.stockOf(b, r) - this.incomingOf(b, r));
      if (missing > 0) this.stats.imported[r] = (this.stats.imported[r] ?? 0) + missing;
    }
    b.autoBought = true;
    b.importCurrency = currency;
    b.bondedCustomsId = customs.id;
    this.bump();
    return { ok: true };
  }

  /** Turn material import (auto-buy) on/off for multiple sites, highest construction
   *  priority first (then id order). Auto-buys as many sites as treasury funds allow.
   *  Returns total cost charged, number of sites succeeded, and number failed. */
  setSiteImportMany(ids: number[], currency: 'east' | 'west' | null): { totalCost: number; succeeded: number; failed: number; reason?: string } {
    let succeeded = 0, failed = 0, totalCost = 0;
    let lastReason: string | undefined;

    const targets = [...new Set(ids)]
      .map(id => this.buildings.get(id))
      .filter((b): b is BuildingInst => !!b && !b.constructed)
      .sort((a, b) => this.effectiveBuildPriority(b) - this.effectiveBuildPriority(a) || a.id - b.id);

    if (targets.length === 0) return { totalCost: 0, succeeded: 0, failed: 0, reason: 'No unconstructed sites selected' };

    for (const b of targets) {
      const initialFunds = currency === 'east' ? this.rubles : currency === 'west' ? this.dollars : 0;
      const res = this.setSiteImport(b.id, currency);
      if (res.ok) {
        succeeded++;
        if (currency === 'east') totalCost += (initialFunds - this.rubles);
        else if (currency === 'west') totalCost += (initialFunds - this.dollars);
      } else {
        failed++;
        lastReason = res.reason;
      }
    }

    this.bump();
    return { totalCost, succeeded, failed, reason: lastReason };
  }

  /** Pay $ to finish a site immediately — a Western prefab completes the rest.
   *  Consumes whatever materials are already on site; the prefab covers the
   *  shortfall. A road/bridge site dissolves into its finished tile. */
  finishSiteInstant(id: number): { ok: boolean; reason?: string } {
    const b = this.buildings.get(id);
    if (!b || b.constructed) return { ok: false, reason: 'Not a construction site' };
    const cost = this.instantFinishCost(id);
    if (this.dollars < cost) return { ok: false, reason: `Not enough dollars ($${cost.toLocaleString()})` };
    this.dollars -= cost;
    const def = this.def(b);
    if (def.becomesRoad) {
      this.applyInternalTilePatches([{ x: b.x, y: b.y, road: true, buildingId: null }]);
      this.removeBuilding(b);
      this.stats.roadsBuilt++;
      this.bump();
      return { ok: true };
    }
    for (const [r, amt] of Object.entries(def.materials) as [ResourceId, number][]) {
      const have = this.stockOf(b, r);
      if (have > 0) this.addStock(b, r, -Math.min(have, amt)); // prefab supplies the rest
    }
    b.paused = false;
    this.markConstructed(b);
    b.progress = def.labor;
    this.seedWearBins(b);
    this.pushEvent(`${def.name} completed!`, 'good', 'check');
    this.bump();
    return { ok: true };
  }

  /**
   * Return an unfinished site's ALREADY-DELIVERED stock to storage before the
   * site is deleted — otherwise those materials vanish. Return trucks are
   * off-road capable and haul to the nearest storage (depot/warehouse/customs)
   * with room; if none is reachable a storage with room is credited directly
   * (salvage). Disjoint from the in-flight turn-back, which conserves cargo
   * still on the road (b.incoming) — this handles the pile at the site
   * (b.stock). Together: nothing is lost.
   */
  private refundSiteStock(b: BuildingInst): void {
    const resources = (Object.keys(b.stock) as ResourceId[]).filter(r => this.stockOf(b, r) > 1e-3);
    if (!resources.length) return;
    const storages = [...this.buildings.values()].filter(s =>
      s.constructed && (this.def(s).isDepot || this.def(s).isCustoms || s.defId === 'warehouse'));
    if (!storages.length) {
      this.pushEvent('Demolition scattered its materials — no depot to salvage them into.', 'bad', 'bulldoze');
      return;
    }
    const access = this.accessTiles(b);
    const flood = access.length ? this.floodTerrain(access) : null;
    const bCenter = this.centerOf(b);
    const roomFor = (s: BuildingInst, r: ResourceId) => this.capOf(s, r) - this.stockOf(s, r) - this.incomingOf(s, r);

    for (const r of resources) {
      let amt = this.stockOf(b, r);
      while (amt > 1e-3) {
        // nearest flood-reachable storage with room (tie-break: iteration order → id, then tile order)
        let bs: BuildingInst | null = null, bt: { x: number; y: number } | null = null, bd = Infinity;
        if (flood) {
          for (const s of storages) {
            if (roomFor(s, r) < 1e-3) continue;
            for (const t of this.accessTiles(s)) {
              const dd = flood.distanceAt(t.x, t.y);
              if (dd >= 0 && dd < bd) { bd = dd; bs = s; bt = t; }
            }
          }
        }
        // A real vehicle has to come and collect it. If the fleet is fully
        // committed we fall through to the direct salvage below rather than
        // leaving the pile to evaporate — conservation outranks the fiction.
        const hauler = bs && bt ? this.takeIdleVehicle(bCenter) : null;
        if (bs && bt && hauler) {
          const load = Math.min(amt, BALANCE.truckCapacity, roomFor(bs, r));
          this.addStock(b, r, -load); amt -= load;
          bs.incoming[r] = this.incomingOf(bs, r) + load; // reserve so logistics won't overfill
          const path = flood!.pathFrom(bt.x, bt.y) ?? [];
          hauler.cargo = r; hauler.amount = load;
          hauler.srcId = bs.id; hauler.destId = bs.id; hauler.legTo = bs.id; hauler.atId = 0;
          hauler.state = 'toDeliver'; hauler.phase = 'go'; hauler.daysDone = 0;
          hauler.legTiles = Math.max(path.length, bd);
          hauler.daysTotal = Math.max(0.6, bd * BALANCE.truckDaysPerTile);
          hauler.points = [bCenter, ...path.slice().reverse(), this.centerOf(bs)];
          continue;
        }
        // nothing reachable by truck — salvage directly into any storage with room
        const salvage = storages.find(s => roomFor(s, r) >= 1e-3);
        if (!salvage) break; // everything full — the remainder is lost with the site
        const load = Math.min(amt, roomFor(salvage, r));
        this.addStock(b, r, -load); this.addStock(salvage, r, load); amt -= load;
      }
    }
  }

  bulldozeAt(x: number, y: number): boolean {
    const t = this.tiles[y]?.[x];
    if (!t) return false;
    if (t.foreign) return false; // foreign soil (incl. the crossing lane) is untouchable
    if (t.buildingId) {
      const b = this.buildings.get(t.buildingId);
      if (!b) return false;
      this.clearFootprint(b); // refund routing must see the newly opened land
      // barges en route turn around and return their cargo
      for (const tr of this.boats) {
        if (tr.destId === b.id && tr.phase === 'go') {
          tr.phase = 'back';
          tr.daysDone = Math.max(0, tr.daysTotal - tr.daysDone);
        }
      }
      // so do vehicles: a load bound for a building that no longer exists goes
      // back to the supplier it came from, never into the void
      for (const v of this.trucks) {
        if (v.state === 'idle') continue;
        const bound = v.destId === b.id || v.legTo === b.id;
        if (!bound) continue;
        v.state = 'returning';
        v.destId = v.srcId;
        v.legTo = v.srcId;
        v.phase = 'back';
        v.daysDone = Math.max(0, v.daysTotal - v.daysDone);
      }
      this.boatOrders = this.boatOrders.filter(o => o.srcId !== b.id && o.destId !== b.id);
      // an unfinished site's delivered materials go back to storage, not the void
      if (!b.constructed) this.refundSiteStock(b);
      this.removeBuilding(b);
      this.bump();
      return true;
    }
    if (t.road) { this.setRoadTile(x, y, false); this.bump(); return true; }
    return false;
  }

  // ---------------- trade ----------------

  priceOf(r: ResourceId, currency: 'east' | 'west') { return this.w.priceOf(r, currency); }
  importPriceOf(r: ResourceId, currency: 'east' | 'west') { return this.w.importPriceOf(r, currency); }

  /** Land components touched by the constructed customs network — "can this good
   * physically reach the border". Sim-internal derived state: keyed on the land
   * topology + facility revisions, so stock changes and UI bumps never rebuild it. */

  private sellableSources(r: ResourceId) { return this.w.sellableSources(r); }

  private sellableCache: { version: number; map: Map<ResourceId, number> } = { version: -1, map: new Map() };

  /**
   * Stock that sell() could actually export right now — a **UI-facing** derived
   * value (only React reads it; sell()/auto-export call sellableSources() uncached).
   * Deliberately keyed on `version`, NOT a RevisionMemo: it depends on live *stock*
   * (via supplyOf), which has no structural revision, and it's read at render cadence
   * — `version` IS that cadence. Do not "align" this with customsComponents' structural
   * key; that would go stale after any production/sale with no structural change.
   */
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

  private exportPayout(r: ResourceId, bloc: 'east' | 'west', amt: number): number {
    return this.w.exportPayout(r, bloc, amt);
  }

  sell(r: ResourceId, amount: number, currency: 'east' | 'west'): { ok: boolean; msg: string } {
    const sources = this.sellableSources(r);
    if (!sources.length) return { ok: false, msg: 'No sellable goods connected to a Customs House' };
    // The border trades in whole units only (like buy()/auto-trade, which floor):
    // cap the sale to whole available units so no fraction of stock crosses the
    // border and contract `delivered` stays integer. Sub-unit surplus stays home.
    const available = sources.reduce((s, x) => s + x.amt, 0);
    const target = Math.min(amount, Math.floor(available));
    if (target <= 0) return { ok: false, msg: 'Nothing to sell' };
    let remaining = target;
    for (const s of sources) {
      const take = Math.min(remaining, s.amt);
      this.addStock(s.b, r, -take);
      remaining -= take;
      if (remaining <= 1e-9) break;
    }
    const sold = target; // fully covered: target ≤ floor(available), so Σtake = target
    const payout = this.exportPayout(r, currency, sold);
    if (currency === 'east') this.rubles += payout;
    else this.dollars += payout;
    this.stats.exportedValue += currency === 'east' ? payout : payout * 10;
    this.bump();
    return { ok: true, msg: `Sold ${fmtQty(sold)} ${RESOURCES[r].name}` };
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
    this.stats.imported[r] = (this.stats.imported[r] ?? 0) + delivered;
    this.bump();
    return { ok: true, msg: `Imported ${fmtQty(delivered)} ${RESOURCES[r].name} to Customs` };
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
    this.moveVehicles(daysDelta * wx.truckMult);
    this.moveFleet(this.boats, daysDelta * Math.max(0.4, wx.boatMult));
    this.moveForeignTrucks(daysDelta * wx.truckMult);
    this.acc += dtMs * this.speed;
    let days = 0;
    while (this.acc >= this.TICK_MS && days < 20) {
      this.acc -= this.TICK_MS;
      this.simulateDay();
      days++;
    }
  }

  /** Barge/foreign-lorry lifecycle: deliver, return undelivered cargo, retire.
   *  Road vehicles do NOT use this — they are persistent, see `moveVehicles`. */
  private moveFleet(fleet: Mover[], daysDelta: number) {
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

  // ---------------- the road fleet ----------------

  /**
   * Drive every road vehicle. One leg at a time, always ending at a building:
   * a vehicle never re-routes mid-leg, so there is no state in which its
   * `destId` and its polyline can disagree.
   *
   * Fuel burns per road-tile-equivalent actually covered, which means an
   * off-road leg (costed at `offRoadStepCost`× per map tile) burns that much
   * more fuel as well as taking that much longer — one number, both
   * consequences. A vehicle only accepts work it can finish (`canReach`), so a
   * dry tank mid-leg is the rare case: it crawls to the end of the current leg
   * and must then find a pump before it works again.
   */
  private moveVehicles(daysDelta: number) {
    for (let i = this.trucks.length - 1; i >= 0; i--) {
      const v = this.trucks[i];
      if (v.state === 'idle') { v.speed = 0; continue; }

      // Limping is the penalty for running dry PART WAY through a leg. A lorry
      // that was already empty when it was sent to a pump is being recovered,
      // not punished — crawling there would strand the fleet for weeks.
      if (v.fuel <= 1e-9 && v.state !== 'toRefuel') v.limping = true;
      const stepDelta = daysDelta * (v.limping ? BALANCE.limpSpeedMult : 1);

      // distance covered this tick, in road-tile-equivalents
      const frac = v.daysTotal > 0 ? Math.min(stepDelta, v.daysTotal - v.daysDone) / v.daysTotal : 0;
      const tiles = Math.max(0, v.legTiles * frac);
      v.odometer += tiles;
      v.fuel = Math.max(0, v.fuel - tiles * BALANCE.vehicleFuelPerTile);
      v.speed = v.daysTotal > 0 ? (v.legTiles / v.daysTotal) * (v.limping ? BALANCE.limpSpeedMult : 1) : 0;

      v.daysDone += stepDelta;
      if (v.daysDone < v.daysTotal) continue;
      this.arriveVehicle(v);
    }
  }

  /** A vehicle reached the end of its leg. Resolve it and pick the next one. */
  private arriveVehicle(v: Vehicle) {
    const at = this.buildings.get(v.legTo);
    v.limping = false;
    switch (v.state) {
      case 'toPickup': {
        // Loaded at the supplier (the goods were reserved at dispatch, exactly
        // as they were under the old model). Drive the route that dispatch
        // already chose; only re-route if it was somehow never recorded.
        const dest = this.buildings.get(v.destId);
        if (!dest || !at) { this.parkVehicle(v, at ?? this.buildings.get(v.homeId)); return; }
        if (v.pendingPath) {
          const tiles = v.pendingTiles ?? v.pendingPath.length;
          v.state = 'toDeliver';
          v.phase = 'go';
          v.daysDone = 0;
          v.legTo = dest.id;
          v.legTiles = tiles;
          v.daysTotal = Math.max(0.6, tiles * BALANCE.truckDaysPerTile);
          v.points = [this.centerOf(at), ...v.pendingPath, this.centerOf(dest)];
          v.pendingPath = undefined;
          v.pendingTiles = undefined;
          return;
        }
        if (!this.startLeg(v, at, dest, 'toDeliver')) {
          // The destination became unreachable while we drove here. Put the
          // load back where it came from rather than losing it.
          this.releaseVehicleCargo(v, dest);
          this.parkVehicle(v, at);
        }
        return;
      }
      case 'toDeliver': {
        if (at) {
          const delivered = this.addStock(at, v.cargo, v.amount);
          at.incoming[v.cargo] = Math.max(0, this.incomingOf(at, v.cargo) - v.amount);
          v.amount -= delivered; // whatever didn't fit rides back to the source
        }
        if (v.amount > 0.001) {
          const src = this.buildings.get(v.srcId);
          if (src && at && this.startLeg(v, at, src, 'returning')) return;
          if (src) this.addStock(src, v.cargo, v.amount);
        }
        v.amount = 0;
        this.parkVehicle(v, at);
        return;
      }
      case 'returning': {
        if (at && v.amount > 0.001) this.addStock(at, v.cargo, v.amount);
        v.amount = 0;
        this.parkVehicle(v, at);
        return;
      }
      case 'toRefuel': {
        if (at) {
          const take = Math.min(v.fuelCap - v.fuel, this.stockOf(at, 'fuel'));
          if (take > 0) {
            this.addStock(at, 'fuel', -take);
            v.fuel += take;
          }
        }
        this.parkVehicle(v, at);
        return;
      }
      default:
        this.parkVehicle(v, at);
    }
  }

  /** Park a vehicle at `at` (falling back to its garage, then to where it stands). */
  private parkVehicle(v: Vehicle, at: BuildingInst | null | undefined) {
    const spot = at ?? this.buildings.get(v.homeId);
    v.state = 'idle';
    v.speed = 0;
    v.daysDone = 0;
    v.daysTotal = 0;
    v.legTiles = 0;
    v.pendingPath = undefined;
    v.pendingTiles = undefined;
    if (spot) {
      v.atId = spot.id;
      v.destId = spot.id;
      v.legTo = spot.id;
      v.points = [this.centerOf(spot)];
    }
  }

  /** Hand a load back to the destination's reservation and the supplier's bin. */
  private releaseVehicleCargo(v: Vehicle, dest: BuildingInst) {
    dest.incoming[v.cargo] = Math.max(0, this.incomingOf(dest, v.cargo) - v.amount);
    const src = this.buildings.get(v.srcId);
    if (src && v.amount > 0.001) this.addStock(src, v.cargo, v.amount);
    v.amount = 0;
  }

  /**
   * Route `v` from building `from` to building `to` and start driving.
   * Roads first, weighted land second — the same two-tier rule dispatch uses.
   * Returns false and leaves the vehicle untouched when no route exists, so a
   * failed search can never desync `destId` from `points`.
   */
  private startLeg(v: Vehicle, from: BuildingInst, to: BuildingInst, state: VehicleState): boolean {
    return this.w.startLeg(v, from, to, state);
  }

  private takeIdleVehicle(near: { x: number; y: number }): Vehicle | null {
    return this.w.takeIdleVehicle(near);
  }

  private resetRoutingDiagnostics(): void {
    this.routingDay = {
      dayIndex: this.dayIndex(),
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
  }

  /** Last simulated day's routing work plus cumulative topology rebuilds.
   * Derived diagnostics are deliberately excluded from saves and RNG state. */
  getRoutingDiagnostics(): RoutingDiagnostics {
    return {
      ...this.routingDay,
      topologyRebuilds: {
        road: this.topology.rebuildCount('road'),
        land: this.topology.rebuildCount('land'),
        water: this.topology.rebuildCount('water'),
      },
    };
  }

  /** Run one pure system and apply what it asked for. The engine's only role in
   *  a converted system is to sequence it — it neither reads nor edits the
   *  mutation list, which is what makes the write-set guard test meaningful. */
  private run(system: (w: World) => Mutation[]): void {
    underSystem(system.name, () => applyMutations(this.w, system(this.w)));
  }

  /** Run a system that applies as it goes (see `Staged`). Its mutations are
   *  already in the world — re-applying them here would double every effect. */
  private runStaged(system: (w: World) => Mutation[]): void {
    underSystem(system.name, () => system(this.w));
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

    this.resetRoutingDiagnostics();
    // Converted systems are pure: they read the World and return what they want
    // changed; applyMutations is the only writer. Applied one system at a time,
    // in this order, so a later system reads the earlier one's effects exactly
    // as it did when these were void methods.
    this.run(weather);
    this.run(connectivity);
    this.run(workers);
    this.run(powerHeat);
    this.runStaged(production);
    this.runStaged(foreignTrade);
    this.runStaged(contracts);
    this.runStaged(loans);
    // The fleet reconciles with its garages, tops up dry tanks, then works.
    // Fuel leaves a bin only inside refuelVehicles() — there is no second,
    // pooled levy anywhere in the day.
    this.runStaged(syncFleet);
    this.runStaged(refuelVehicles);
    this.runStaged(logistics);
    this.runStaged(boats);
    this.runStaged(construction);
    this.run(citizens);
    this.run(totals);
    this.checkObjectives();
    this.updateAlerts();
    this.bump();
  }

  private monthlyEconomy() {
    const drift = (v: number) => Math.min(1.15, Math.max(0.85, v + (this.rng() - 0.5) * 0.1));
    this.priceFactorEast = drift(this.priceFactorEast);
    this.priceFactorWest = drift(this.priceFactorWest);
    this.offerContract();
  }

  /**
   * A bloc tenders a bulk order every other month. The draw comes from its
   * own stateless per-month stream (like the weather timeline), so contract
   * generation never perturbs the price-drift rng sequence.
   */
  private offerContract() {
    const monthIndex = (this.year - 1960) * 12 + (this.month - 1);
    if (monthIndex % CONTRACTS.offerEveryMonths !== 0) return;
    if (![...this.buildings.values()].some(b => this.def(b).isCustoms && b.constructed)) return;
    if (this.contracts.filter(c => c.state === 'offer').length >= 2) return;
    const rnd = mulberry32((this.seed ^ 0x7c3a9e50 ^ Math.imul(monthIndex, 0x9e3779b9)) >>> 0);
    // the blocs ask for what the republic demonstrably produces
    const produced = ALL_RESOURCES.filter(r => this.stats.produced[r] > 0);
    const pool = produced.length ? produced : ALL_RESOURCES;
    const r = pool[Math.floor(rnd() * pool.length)];
    const bloc: 'east' | 'west' = rnd() < 0.5 ? 'east' : 'west';
    // value-banded orders: a machinery tender is a few machines, a coal tender
    // a trainload — but both are worth comparable money
    const [vLo, vHi] = bloc === 'east' ? CONTRACTS.valueBandEast : CONTRACTS.valueBandWest;
    const value = vLo + rnd() * (vHi - vLo);
    const amount = Math.min(CONTRACTS.maxUnits,
      Math.max(CONTRACTS.minUnits, Math.round(value / this.priceOf(r, bloc))));
    const premium = CONTRACTS.premiumMin + rnd() * (CONTRACTS.premiumMax - CONTRACTS.premiumMin);
    const days = CONTRACTS.deadlineMinDays + Math.floor(rnd() * (CONTRACTS.deadlineMaxDays - CONTRACTS.deadlineMinDays + 1));
    const c: Contract = {
      id: this.nextContractId++, r, bloc, amount, delivered: 0,
      pricePerUnit: this.priceOf(r, bloc) * (1 + premium),
      deadlineIdx: this.dayIndex() + days,
      offerExpiresIdx: this.dayIndex() + CONTRACTS.offerDays,
      state: 'offer',
    };
    this.contracts.push(c);
    const cur = bloc === 'east' ? '₽' : '$';
    this.pushEvent(
      `The ${bloc === 'east' ? 'East' : 'West'} tenders a contract: ${amount} ${RESOURCES[r].name} at ${cur}${c.pricePerUnit.toFixed(1)}/unit within ${days} days.`,
      'info', 'contract');
  }

  // ---------------- systems ----------------



  /**
   * Actual per-day resource flows for a building under current conditions.
   * production() applies exactly these deltas, and the UI displays them, so
   * the simulation and the inspector cannot diverge.
   */

  nominalInputRate(b: BuildingInst, r: ResourceId): number { return this.w.nominalInputRate(b, r); }

  productionRates(b: BuildingInst) { return this.w.productionRates(b); }

  // ---------------- foreign trade (auto) ----------------


  /** Foreign lorries only cross and return — no delivery logic. */
  private moveForeignTrucks(daysDelta: number) {
    for (let i = this.foreignTrucks.length - 1; i >= 0; i--) {
      const t = this.foreignTrucks[i];
      t.daysDone += daysDelta;
      if (t.daysDone < t.daysTotal) continue;
      if (t.phase === 'go') { t.phase = 'back'; t.daysDone = 0; }
      else this.foreignTrucks.splice(i, 1);
    }
  }

  // ---------------- logistics ----------------

  private builderPool(): number { return this.w.builderPool(); }

  /**
   * Lorries a single building owns (0 if it isn't a garage or is unbuilt /
   * off-grid). Offices come with a pool; Motor Depots crew one per driver.
   * The single source of the per-building formula (UI reads it, never recomputes).
   *
   * These are VEHICLES, not concurrent shipments. The old figure counted only
   * outbound trucks — a truck on its way home had already freed its slot — so
   * the same delivery rate now takes about twice as many machines, because a
   * real lorry drives the empty leg too. Same capacity, stated physically.
   */
  trucksFrom(b: BuildingInst): number { return this.w.trucksFrom(b); }


  /** Relay status for a River Port — drives its SidePanel row so an idle port
   *  explains itself instead of looking broken. 'relaying' = a barge or queued
   *  order touches it; 'ready' = paired across water, waiting for cargo;
   *  'unpaired' = no other port shares its water yet; 'frozen' = river ice-locked. */
  portStatus(b: BuildingInst): { state: 'relaying' | 'ready' | 'unpaired' | 'frozen'; label: string } {
    if (this.weather.riverFrozen) return { state: 'frozen', label: 'Barges ice-locked until the thaw' };
    const boat = this.boats.find(t => t.destId === b.id || t.srcId === b.id);
    if (boat) return { state: 'relaying', label: `Relaying ${RESOURCES[boat.cargo].name}` };
    const order = this.boatOrders.find(o => o.destId === b.id || o.srcId === b.id);
    if (order) return { state: 'relaying', label: `Relaying ${RESOURCES[order.r].name}` };
    const water = this.waterAccess(b);
    const paired = [...this.buildings.values()].some(p =>
      p.id !== b.id && this.def(p).isPort && p.constructed &&
      shareAnyComponent(this.waterAccess(p).components, water.components));
    return paired
      ? { state: 'ready', label: 'Ready — no cargo waiting' }
      : { state: 'unpaired', label: 'Idle — needs a paired port across the water' };
  }

  /** Vehicles the Construction Offices own. */
  fleetStatus(): {
    active: number; max: number; idle: number; grounded: number;
    officeTrucks: number; driverTrucks: number;
    tankFuel: number; pumpFuel: number; customsFuel: number;
    fuelDaysLeft: number;
  } {
    let active = 0, idle = 0, grounded = 0;
    const minTank = BALANCE.vehicleReserveTiles * BALANCE.vehicleFuelPerTile;
    for (const v of this.trucks) {
      if (v.state === 'idle') {
        idle++;
        if (v.fuel < minTank) grounded++;
      } else active++;
    }
    const tank = this.w.tankFuel();
    const pump = this.w.pumpFuel();
    // Days of hauling left = fuel everywhere the fleet can draw on, over what
    // the vehicles currently rolling are actually burning.
    let burnPerDay = 0;
    for (const v of this.trucks) if (v.state !== 'idle') burnPerDay += v.speed * BALANCE.vehicleFuelPerTile;
    const fuelDaysLeft = burnPerDay > 1e-9 ? (tank + pump + this.w.customsFuel()) / burnPerDay : Infinity;
    return {
      active, max: this.trucks.length, idle, grounded,
      officeTrucks: this.w.officeTrucks(), driverTrucks: this.w.driverTrucks(),
      tankFuel: tank, pumpFuel: pump, customsFuel: this.w.customsFuel(), fuelDaysLeft,
    };
  }

  /** Is the fleet down to drinking the border's emergency reserve? */
  fleetFuelInfo(): { usingCustomsFuel: boolean; customsFuel: number } {
    const cFuel = this.w.customsFuel();
    return { usingCustomsFuel: this.w.pumpFuel() <= 0.001 && cFuel > 0, customsFuel: cFuel };
  }

  /** stock a building is willing to give away */
  // ---------------- construction ----------------

  /** True when construction is throughput-limited: two or more ready sites want
   *  more builder-days than the pool can supply, so sites build slowly and the
   *  player should add a Construction Office. Reuses the exact demand/cap math of
   *  the construction system so the advisory can never diverge from the simulation. */
  constructionThrottled(): boolean {
    if (!this.globalConstructionEnabled) return false;
    const pool = this.builderPool();
    if (pool <= 0) return false; // the pool===0 case is the "halted" advisory
    const buildMult = WEATHER[this.weather.condition].buildMult;
    let ready = 0, demand = 0;
    for (const b of this.buildings.values()) {
      if (b.constructed || b.paused || !this.siteReady(b)) continue;
      ready++;
      demand += Math.min(BALANCE.buildersPerSite, (this.def(b).labor - b.progress) / Math.max(1e-4, buildMult));
    }
    return ready >= 2 && demand > pool;
  }

  private siteReady(b: BuildingInst): boolean { return this.w.siteReady(b); }

  // ---------------- citizens ----------------

  happinessBreakdown(): HappinessBreakdown {
    const w = this.sat;
    const factors: HappinessFactor[] = [
      { id: 'food', label: 'Food Supply', icon: 'food', satPct: Math.round(w.food * 100), weightPct: 30, effectivePct: Math.round(w.food * 30) },
      { id: 'clothes', label: 'Clothing', icon: 'clothes', satPct: Math.round(w.clothes * 100), weightPct: 14, effectivePct: Math.round(w.clothes * 14) },
      { id: 'power', label: 'Electricity', icon: 'power', satPct: Math.round(w.power * 100), weightPct: 12, effectivePct: Math.round(w.power * 12) },
      { id: 'heat', label: 'Winter Heating', icon: 'heat', satPct: Math.round(w.heat * 100), weightPct: 12, effectivePct: Math.round(w.heat * 12) },
      { id: 'employment', label: 'Employment', icon: 'staff', satPct: Math.round(w.employment * 100), weightPct: 12, effectivePct: Math.round(w.employment * 12) },
      { id: 'culture', label: 'Culture & Leisure', icon: 'pub', satPct: Math.round(w.culture * 100), weightPct: 10, effectivePct: Math.round(w.culture * 10) },
      { id: 'health', label: 'Healthcare', icon: 'clinic', satPct: Math.round(w.health * 100), weightPct: 10, effectivePct: Math.round(w.health * 10) },
    ];

    const rawSum = (0.30 * w.food + 0.14 * w.clothes + 0.12 * w.power + 0.12 * w.heat +
      0.10 * w.culture + 0.10 * w.health + 0.12 * w.employment) * 100;
    const pollutionFactor = w.pollution;
    const weatherMod = -Math.min(0.06, this.gloomStreak * 0.01) + Math.min(0.02, this.sunStreak * 0.005);
    const target = Math.max(0, Math.min(100, rawSum * pollutionFactor * (1 + weatherMod)));

    return {
      overall: Math.round(this.happiness),
      target: Math.round(target),
      factors,
      modifiers: {
        pollutionPenaltyPct: Math.round((1 - pollutionFactor) * 100),
        weatherMoralePct: Math.round(weatherMod * 1000) / 10,
      },
    };
  }

  // ---------------- totals, objectives, alerts ----------------

  // --------------------------------------------------------------------------
  // Loans
  // --------------------------------------------------------------------------

  activeLoan(bloc: 'east' | 'west'): Loan | undefined {
    return this.loans.find(l => l.bloc === bloc && l.state === 'active');
  }

  canTakeLoan(bloc: 'east' | 'west', tierIndex?: number): { ok: boolean; reason?: string } {
    if (tierIndex !== undefined && (tierIndex < 0 || tierIndex >= LOANS.tierLabels.length)) {
      return { ok: false, reason: 'Invalid loan tier' };
    }
    const label = bloc === 'east' ? 'East' : 'West';
    if (this.activeLoan(bloc)) return { ok: false, reason: `Already have an active ${label} loan` };
    if (this.dayIndex() < this.loanCooldown[bloc]) {
      const days = this.loanCooldown[bloc] - this.dayIndex();
      return { ok: false, reason: `${label} credit frozen for ${days} more days` };
    }
    return { ok: true };
  }

  takeLoan(bloc: 'east' | 'west', tierIdx: 0 | 1 | 2): { ok: boolean; msg: string } {
    const check = this.canTakeLoan(bloc, tierIdx);
    if (!check.ok) return { ok: false, msg: check.reason! };
    const principal = bloc === 'east' ? LOANS.tiersEast[tierIdx] : LOANS.tiersWest[tierIdx];
    const interest = bloc === 'east' ? LOANS.interestEast : LOANS.interestWest;
    const totalOwed = Math.round(principal * (1 + interest));
    const deadlineDayIdx = this.dayIndex() + LOANS.deadlines[tierIdx];

    const loan: Loan = {
      id: this.nextLoanId++,
      bloc,
      principal,
      totalOwed,
      repaid: 0,
      takenDayIdx: this.dayIndex(),
      deadlineDayIdx,
      tierIndex: tierIdx,
      state: 'active',
    };

    this.loans.push(loan);
    if (bloc === 'east') this.rubles += principal;
    else this.dollars += principal;
    
    const cur = bloc === 'east' ? '\u20bd' : '$';
    this.pushEvent(`Secured ${LOANS.tierLabels[tierIdx]} ${bloc === 'east' ? 'East' : 'West'} Loan: received ${cur}${fmtMoney(principal)}`, 'good', 'coins');
    this.bump();
    return { ok: true, msg: `Borrowed ${cur}${fmtMoney(principal)}` };
  }

  repayLoan(bloc: 'east' | 'west', amount: number): { ok: boolean; msg: string } {
    const loan = this.activeLoan(bloc);
    if (!loan) return { ok: false, msg: `No active ${bloc === 'east' ? 'East' : 'West'} loan` };
    if (amount <= 0) return { ok: false, msg: 'Invalid amount' };
    
    const funds = bloc === 'east' ? this.rubles : this.dollars;
    const remaining = loan.totalOwed - loan.repaid;
    const payment = Math.min(amount, remaining, funds);
    if (payment <= 0) return { ok: false, msg: bloc === 'east' ? 'Not enough rubles' : 'Not enough dollars' };

    if (bloc === 'east') this.rubles -= payment;
    else this.dollars -= payment;

    loan.repaid += payment;
    const cur = bloc === 'east' ? '\u20bd' : '$';
    if (loan.repaid >= loan.totalOwed) {
      loan.state = 'repaid';
      this.checkObjectives();
      this.pushEvent(`${bloc === 'east' ? 'East' : 'West'} loan fully repaid!`, 'good', 'coins');
    }
    this.bump();
    return { ok: true, msg: `Repaid ${cur}${fmtMoney(payment)}` };
  }

  loanDaysLeft(loan: Loan): number {
    return Math.max(0, loan.deadlineDayIdx - this.dayIndex());
  }

  /** Total outstanding debt across all active loans, per currency. */
  totalDebt(): { rubles: number; dollars: number } {
    let rubles = 0, dollars = 0;
    for (const l of this.loans) {
      if (l.state !== 'active') continue;
      const rem = l.totalOwed - l.repaid;
      if (l.bloc === 'east') rubles += rem;
      else dollars += rem;
    }
    return { rubles, dollars };
  }

  setLoanAutoRepay(enabled: boolean) {
    this.loanAutoRepay.enabled = enabled;
    this.bump();
  }

  setLoanAutoRepayThreshold(bloc: 'east' | 'west', value: number) {
    if (bloc === 'east') this.loanAutoRepay.thresholdRubles = Math.max(0, Math.round(value));
    else this.loanAutoRepay.thresholdDollars = Math.max(0, Math.round(value));
    this.bump();
  }

  private checkObjectives() { underSystem('objectives', () => objectives(this.w)); }

  private updateAlerts() {
    const a: Alert[] = [];
    // stranded construction sites: truly unreachable (no road AND no off-road
    // route) and not part of a frontier road/bridge chain (which stays quiet)
    let stranded = 0;
    for (const b of this.buildings.values()) {
      if (b.constructed) continue;
      if (b.connected) continue; // reachable by road or off-road → will be served
      // Corner-inclusive on purpose: a site touching an adjacent unfinished site
      // even only diagonally is part of the same construction cluster, not stranded.
      let nearSite = false;
      forEachPerimeterTile(b.x, b.y, b.w, b.h, { corners: true }, (px, py) => {
        const id = this.tiles[py]?.[px]?.buildingId;
        if (id && id !== b.id && !this.buildings.get(id)?.constructed) { nearSite = true; return true; }
      });
      if (!nearSite) stranded++;
    }
    if (stranded > 0) a.push({ id: 'sites', icon: 'road', text: `${stranded} construction site${stranded > 1 ? 's' : ''} unreachable — no delivery route (road or off-road)`, level: 'warn' });
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
    const isolated = [...this.buildings.values()].filter(b => b.constructed && !b.connected).length;
    if (isolated > 0) a.push({ id: 'roads', icon: 'road', text: `${isolated} building${isolated > 1 ? 's' : ''} isolated — no delivery route`, level: 'warn' });
    const offroadOnly = [...this.buildings.values()].filter(b => b.constructed && b.connected && !b.roadConnected).length;
    if (offroadOnly > 0) a.push({ id: 'offroad', icon: 'road', text: `${offroadOnly} building${offroadOnly > 1 ? 's' : ''} reachable only off-road — slow deliveries; lay a road`, level: 'warn' });
    if (this.globalConstructionEnabled) {
      const sites = [...this.buildings.values()].filter(b => !b.constructed);
      if (sites.length > 0 && this.builderPool() === 0) a.push({ id: 'builders', icon: 'builders', text: 'No builders available — construction halted', level: 'warn' });
      else if (this.constructionThrottled()) a.push({ id: 'buildersSlow', icon: 'builders', text: 'Builders spread thin — sites building slowly; add a Construction Office', level: 'warn' });
    }
    const fleet = this.fleetStatus();
    if (fleet.max === 0) a.push({ id: 'trucks', icon: 'truck', text: 'No trucks — staff a Construction Office or Motor Depot to haul goods', level: 'warn' });
    else if (fleet.grounded > 0) a.push({ id: 'fleetFuel', icon: 'fuel', text: `${fleet.grounded} vehicle${fleet.grounded > 1 ? 's' : ''} grounded with empty tanks — get fuel to a pump (refinery fuel or imports)`, level: fleet.grounded >= fleet.max ? 'bad' : 'warn' });
    else if (fleet.active >= fleet.max) a.push({ id: 'fleetFull', icon: 'truck', text: 'Logistics at capacity — build a Motor Depot to grow the fleet', level: 'warn' });
    if (this.jobs > this.workers && this.workers > 0) a.push({ id: 'labor', icon: 'users', text: 'Labor shortage — not enough workers for all jobs', level: 'warn' });
    const customs = [...this.buildings.values()].some(b => this.def(b).isCustoms && b.constructed);
    if (!customs) a.push({ id: 'customs', icon: 'trade', text: 'No Customs House — foreign trade impossible', level: 'warn' });
    if (this.tradeLedger.today.blocked.length) {
      a.push({ id: 'autotrade', icon: 'trade', text: `Auto-trade stalled — ${this.tradeLedger.today.blocked.join('; ')}`, level: 'warn' });
    }
    const risky = this.contracts.find(c =>
      c.state === 'active' && this.contractDaysLeft(c) <= 15 && c.delivered < c.amount);
    if (risky) {
      a.push({
        id: 'contract', icon: 'contract',
        text: `Contract deadline in ${Math.max(0, this.contractDaysLeft(risky))} days — ${fmtOwed(risky.amount - risky.delivered)} ${RESOURCES[risky.r].name} still owed`,
        level: 'warn',
      });
    }
    // Loan deadline warnings
    for (const loan of this.loans) {
      if (loan.state !== 'active') continue;
      const daysLeft = this.loanDaysLeft(loan);
      const cur = loan.bloc === 'east' ? '\u20bd' : '$';
      const remaining = loan.totalOwed - loan.repaid;
      if (daysLeft <= LOANS.warningDays) {
        a.push({
          id: `loan-${loan.bloc}`,
          icon: 'coins',
          text: `${loan.bloc === 'east' ? 'East' : 'West'} loan due in ${Math.max(0, daysLeft)} days \u2014 ${cur}${fmtMoney(remaining)} owed`,
          level: daysLeft <= 7 ? 'bad' : 'warn',
        });
      }
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
    const seen = new Set<number>([y * this.mapW + x]);
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
        const k = ny * this.mapW + nx;
        if (seen.has(k)) continue;
        if (this.tiles[ny]?.[nx]?.deposit === kind) { seen.add(k); stack.push({ x: nx, y: ny }); }
      }
    }
    return { kind, tiles, exploitedBy };
  }

  // ---------------- the power grid ----------------

  /**
   * Live view of the grid for the Power Grid panel: what each sector draws,
   * how much of it is actually being served, and how many of its buildings are
   * dark right now. Engine-owned — the panel displays these numbers, it never
   * recomputes them.
   */
  powerGridStatus(): {
    produced: number; deficit: number;
    sectors: { id: Category; draw: number; served: number; buildings: number; dark: number }[];
  } {
    const rows = new Map<Category, { id: Category; draw: number; served: number; buildings: number; dark: number }>();
    for (const c of this.powerSectorOrder) rows.set(c, { id: c, draw: 0, served: 0, buildings: 0, dark: 0 });
    for (const b of this.buildings.values()) {
      const def = this.def(b);
      if (!b.constructed || def.power <= 0) continue;
      const row = rows.get(def.category);
      if (!row) continue;
      row.draw += def.power;
      row.buildings++;
      if (b.powered) row.served += def.power;
      else row.dark++;
    }
    return {
      produced: this.powerProduced,
      deficit: Math.max(0, this.powerDemand - this.powerProduced),
      sectors: this.powerSectorOrder.map(c => rows.get(c)!),
    };
  }

  /** Move a sector one place up (-1) or down (+1) the grid's priority order. */
  movePowerSector(cat: Category, dir: -1 | 1) {
    const order = [...this.powerSectorOrder];
    const i = order.indexOf(cat);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    this.powerSectorOrder = order;
    this.bump();
  }

  /** Replace the whole order. Ignores anything that is not a permutation of the
   *  real sectors, so a corrupt save or a stale UI can't drop one off the grid. */
  setPowerSectorOrder(order: readonly Category[]) {
    const next = order.filter((c, i) => POWER_SECTORS.includes(c) && order.indexOf(c) === i);
    if (next.length !== POWER_SECTORS.length) return;
    this.powerSectorOrder = next;
    this.bump();
  }

  resetPowerSectorOrder() {
    this.powerSectorOrder = [...POWER_SECTORS];
    this.bump();
  }

  /** Flip a building's staffing priority (UI action — keeps mutation + notification in the engine). */
  toggleStaffPriority(id: number) {
    const b = this.buildings.get(id);
    if (!b) return;
    b.priorityHigh = !b.priorityHigh;
    this.bump();
  }

  /** Set a site's construction priority tier (Low -1 / Normal 0 / High 1): higher
   *  tiers are fully crewed — and their materials hauled — before lower ones.
   *  Pass undefined to clear the per-site override and inherit the global category default. */
  setSitePriority(id: number, tier: -1 | 0 | 1 | undefined) {
    const b = this.buildings.get(id);
    if (!b) return;
    if (tier === undefined) {
      if (b.buildPriority === undefined) return;
      delete b.buildPriority;
    } else {
      if ((b.buildPriority ?? 0) === tier) return;
      b.buildPriority = tier;
    }
    this.bump();
  }

  /** Set construction priority for several sites at once (multi-selection action). */
  setSitePriorityMany(ids: number[], tier: -1 | 0 | 1) {
    let changed = false;
    for (const id of ids) {
      const b = this.buildings.get(id);
      if (b && (b.buildPriority ?? 0) !== tier) { b.buildPriority = tier; changed = true; }
    }
    if (changed) this.bump();
  }

  /** Set a global category construction priority. Sites without an explicit per-site
   *  override inherit this. */
  setGlobalCategoryPriority(cat: Category, tier: -1 | 0 | 1) {
    if (this.globalCategoryPriorities[cat] === tier) return;
    this.globalCategoryPriorities[cat] = tier;
    this.bump();
  }

  /** Effective construction priority for a building: per-site override wins,
   *  otherwise falls back to the global category priority. */
  effectiveBuildPriority(b: BuildingInst): -1 | 0 | 1 {
    if (b.buildPriority !== undefined) return b.buildPriority;
    return this.globalCategoryPriorities[this.def(b).category] ?? 0;
  }

  /**
   * Live demand snapshot for the Delivery Priorities panel — including the
   * REASON each delivery is ranked where it is. A dispatcher the player cannot
   * interrogate is one they cannot trust, and every term here is the same one
   * the engine actually sorts by, so the explanation can never drift from the
   * behaviour. UI must not recompute any of this.
   */
  logisticsPriorityPreview(): {
    categories: { id: LogisticsCategory; weight: number; pendingLoads: number; soonestCoverDays: number }[];
    next: {
      resource: ResourceId; destId: number; destName: string;
      kind: DemandKind; category: LogisticsCategory;
      coverDays: number; etaDays: number; avertedDays: number; score: number;
      reason: string;
    }[];
  } {
    const etaPass = this.w.beginEtaPass();
    const demands = this.w.collectLogisticsDemands()
      .map(d => ({ d, eta: this.w.estimateEtaDays(etaPass, d.b), score: 0 }));
    for (const x of demands) x.score = this.w.dispatchScore(x.d, x.eta);
    demands.sort((a, b) => b.score - a.score);

    const cats = new Map<LogisticsCategory, { pendingLoads: number; soonestCoverDays: number }>();
    for (const c of ['lifeline', 'consumer', 'industry', 'construction'] as LogisticsCategory[]) {
      cats.set(c, { pendingLoads: 0, soonestCoverDays: Infinity });
    }
    for (const { d } of demands) {
      if (d.kind === 'housekeeping') continue;
      const row = cats.get(DEMAND_CATEGORY[d.kind])!;
      row.pendingLoads += Math.max(1, Math.ceil(d.amt / BALANCE.truckCapacity));
      row.soonestCoverDays = Math.min(row.soonestCoverDays, this.w.coverDaysOf(d.b, d.r, d.kind));
    }

    const categories = (['lifeline', 'consumer', 'industry', 'construction'] as LogisticsCategory[]).map(id => ({
      id,
      weight: this.logisticsCategoryWeights[id],
      pendingLoads: cats.get(id)!.pendingLoads,
      soonestCoverDays: cats.get(id)!.soonestCoverDays,
    }));

    const fmtDays = (n: number) => (Number.isFinite(n) ? `${n.toFixed(1)} days` : 'no drain');
    const next = demands.filter(x => x.score > 0).slice(0, 5).map(({ d, eta, score }) => {
      const cover = this.w.coverDaysOf(d.b, d.r, d.kind);
      const { binding, headroom } = this.w.inputCoupling(d.b, d.r, d.kind);
      const load = Math.min(d.amt, BALANCE.truckCapacity);
      const drain = this.w.drainRateOf(d.b, d.r, d.kind);
      const averted = this.w.avertedDaysOf(binding, eta, Math.min(load / Math.max(drain, 1e-9), headroom));
      const name = this.def(d.b).name;
      const why = d.kind === 'plantFuel'
        ? `${Math.round(this.w.poweredDependents(d.b))} buildings go dark`
        : d.kind === 'heatFuel' ? 'citizens freeze'
        : d.kind === 'fleetFuel' ? 'the fleet stops hauling'
        : d.kind === 'shopGoods' ? 'citizens go without'
        : d.kind === 'wear' ? 'output halves when the bin runs dry'
        : d.kind === 'construction' ? 'work is idle' : 'production stops';
      return {
        resource: d.r, destId: d.b.id, destName: name,
        kind: d.kind, category: DEMAND_CATEGORY[d.kind],
        coverDays: cover, etaDays: eta, avertedDays: averted, score,
        reason: `${fmtDays(cover)} of cover, ${eta.toFixed(1)}-day trip — ${why}`,
      };
    });

    return { categories, next };
  }

  /**
   * The bootstrap breaker. Fuel is hauled BY vehicles, so a republic whose
   * pumps all ran dry cannot haul itself out — every lorry is parked. This buys
   * a few tons across the border into the customs house, where a grounded
   * vehicle can still reach it.
   *
   * Called at the very top of `logistics()`, before the fleet is counted: the
   * one state it exists for is the one where there is nothing to count.
   *
   * It is a real import and books like one — same ledger lines, same customs
   * throughput, same foreign lorry at the gate as `foreignTrade()`. A purchase
   * that debited the treasury without appearing on the day's trade page is a
   * purchase the player cannot audit.
   */

  /**
   * Set how much the republic values a demand category. This scales
   * consequence — how badly a stall hurts — and never overrides urgency, so no
   * dial setting can make an export outrank a plant about to go dark.
   */
  setLogisticsCategoryWeight(cat: LogisticsCategory, weight: number) {
    const w = Math.max(BALANCE.categoryDialMin, Math.min(BALANCE.categoryDialMax, weight));
    if (this.logisticsCategoryWeights[cat] === w) return;
    this.logisticsCategoryWeights[cat] = w;
    this.bump();
  }

  /** Return every dial to neutral. */
  resetLogisticsCategoryWeights() {
    this.logisticsCategoryWeights = { lifeline: 1, consumer: 1, industry: 1, construction: 1 };
    this.bump();
  }

  toggleEmergencyFuelAutoBuy() {
    this.emergencyFuelAutoBuy = !this.emergencyFuelAutoBuy;
    this.bump();
  }

  citizenDemandOf(r: ResourceId): number { return this.w.citizenDemandOf(r); }

  // ---------------- contracts (UI actions) ----------------

  acceptContract(id: number) {
    const c = this.contracts.find(k => k.id === id && k.state === 'offer');
    if (!c) return;
    c.state = 'active';
    this.bump();
  }

  declineContract(id: number) {
    const i = this.contracts.findIndex(k => k.id === id && k.state === 'offer');
    if (i < 0) return;
    this.contracts.splice(i, 1);
    this.bump();
  }

  /** Days left before a contract's deadline (negative once passed). */
  contractDaysLeft(c: Contract): number {
    return c.deadlineIdx - this.dayIndex();
  }

  /** Days before an unaccepted offer is withdrawn. */
  offerDaysLeft(c: Contract): number {
    return c.offerExpiresIdx - this.dayIndex();
  }

  // ---------------- auto-trade policy (UI actions) ----------------

  setAutoTradeEnabled(on: boolean) {
    this.autoTrade.enabled = on;
    this.bump();
  }

  setGlobalConstructionEnabled(on: boolean) {
    this.globalConstructionEnabled = on;
    this.bump();
  }

  setForeignLaborEnabled(on: boolean) {
    this.foreignLaborEnabled = on;
    this.bump();
  }

  setForeignLaborCurrency(c: 'east' | 'west') {
    this.foreignLaborCurrency = c;
    this.bump();
  }

  /** Toggle paid machinery imports for facility repairs, and which bloc pays. */
  setRepairImports(on: boolean, currency?: 'east' | 'west') {
    this.repairImportsEnabled = on;
    if (currency) this.repairImportCurrency = currency;
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

  // ---------------- save / load ----------------

  /**
   * Snapshot the full simulation as a versioned, JSON-safe blob. Runs
   * between advance() calls on the UI thread, so fleets and the incoming[]
   * reservations they hold are captured atomically. Deep-copies everything —
   * mutating the blob later can never corrupt the live engine.
   */
  serialize(): SaveGameV1 {
    const cloneTruck = (t: Truck): Truck => ({ ...t, points: t.points.map(p => ({ ...p })) });
    const cloneLedger = (l: TradeDayLedger): TradeDayLedger =>
      ({
        imports: { ...l.imports },
        exports: { ...l.exports },
        rubles: l.rubles,
        dollars: l.dollars,
        used: l.used,
        capacity: l.capacity,
        blocked: [...l.blocked],
        foreignLabor: l.foreignLabor ?? l.foreignLaborRubles ?? 0,
        foreignLaborRubles: l.foreignLaborRubles ?? l.foreignLabor ?? 0,
        foreignLaborDollars: l.foreignLaborDollars ?? 0,
        repairImports: l.repairImports ?? 0,
      });
    const rules: Partial<Record<ResourceId, AutoTradeRule>> = {};
    for (const [r, rule] of Object.entries(this.autoTrade.rules) as [ResourceId, AutoTradeRule][]) rules[r] = { ...rule };
    return {
      header: {
        formatVersion: SAVE_FORMAT_VERSION,
        savedAt: Date.now(),
        name: this.name,
        seed: this.seed,
        mapW: this.mapW, mapH: this.mapH,
        climate: this.climate,
        difficulty: this.difficulty,
        day: this.day, month: this.month, year: this.year,
        pop: this.pop,
        rubles: this.rubles, dollars: this.dollars,
      },
      body: {
        borderEdge: this.borderEdge,
        ...packTiles(this.w.tiles as Tile[][]),
        buildings: [...this.buildings.values()].map(b => ({ ...b, stock: { ...b.stock }, incoming: { ...b.incoming } })),
        trucks: this.trucks.map(cloneTruck),
        boats: this.boats.map(cloneTruck),
        foreignTrucks: this.foreignTrucks.map(cloneTruck),
        boatOrders: this.boatOrders.map(o => ({ ...o })),
        acc: this.acc,
        lastRunSpeed: this.lastRunSpeed,
        rngState: this.rng.getState(),
        priceFactorEast: this.priceFactorEast,
        priceFactorWest: this.priceFactorWest,
        autoTrade: { enabled: this.autoTrade.enabled, reserveRubles: this.autoTrade.reserveRubles, reserveDollars: this.autoTrade.reserveDollars, rules },
        globalCategoryPriorities: { ...this.globalCategoryPriorities },
        globalConstructionEnabled: this.globalConstructionEnabled,
        foreignLaborEnabled: this.foreignLaborEnabled,
        foreignLaborCurrency: this.foreignLaborCurrency,
        repairImportsEnabled: this.repairImportsEnabled,
        repairImportCurrency: this.repairImportCurrency,
        logisticsCategoryWeights: { ...this.logisticsCategoryWeights },
        emergencyFuelAutoBuy: this.emergencyFuelAutoBuy,
        powerSectorOrder: [...this.powerSectorOrder],
        tradeLedger: { today: cloneLedger(this.tradeLedger.today), yesterday: cloneLedger(this.tradeLedger.yesterday) },
        contracts: this.contracts.map(c => ({ ...c })),
        loans: this.loans.map(l => ({ ...l })),
        loanAutoRepay: { ...this.loanAutoRepay },
        loanCooldown: { ...this.loanCooldown },
        relationsPenalty: { ...this.relationsPenalty },
        objectivesDone: [...this.objectivesDone],
        stats: { produced: { ...this.stats.produced }, imported: { ...this.stats.imported }, exportedValue: this.stats.exportedValue, roadsBuilt: this.stats.roadsBuilt },
        happiness: this.happiness,
        sat: { ...this.sat },
        streaks: { dry: this.dryStreak, gloom: this.gloomStreak, sun: this.sunStreak, wasFrost: this.wasFrost },
        counters: { building: this.nextBuildingId, truck: this.nextTruckId, boat: this.nextBoatId, contract: this.nextContractId, loan: this.nextLoanId },
        aggregates: {
          capacity: this.capacity, workers: this.workers, employed: this.employed, jobs: this.jobs,
          powerProduced: this.powerProduced, powerDemand: this.powerDemand,
          heatProduced: this.heatProduced, heatDemand: this.heatDemand,
        },
      },
    };
  }

  /**
   * Reconstruct an engine from a save blob. Always returns a PAUSED engine
   * (speed 0) — the caller decides when time resumes. The weather timeline is
   * rebuilt from the seed and replayed to the saved day, so snow depth and
   * river-freeze hysteresis come back exactly; the economy rng position is
   * restored bit-exact via rngState. Throws SaveError on invalid blobs.
   */
  static fromSave(save: SaveGameV1, opts: { weatherScript?: (dayIndex: number) => Partial<DayWeather> } = {}): GameEngine {
    const { header: h, body } = validateSave(save);
    const tiles = unpackTiles(body.tilesPacked, body.variantsPacked, h.mapW, h.mapH);
    // buildingId stamps are not encoded — clear-and-restamp from footprints below
    const e = new GameEngine({
      seed: h.seed, climate: h.climate, difficulty: h.difficulty, name: h.name,
      skipStartingBase: true, weatherScript: opts.weatherScript,
      map: { tiles, startX: 0, startY: 0, border: body.borderEdge ?? undefined },
    });

    e.day = h.day; e.month = h.month; e.year = h.year;
    e.rubles = h.rubles; e.dollars = h.dollars; e.pop = h.pop;

    e.happiness = body.happiness;
    e.w.sat = { ...body.sat };
    e.priceFactorEast = body.priceFactorEast;
    e.priceFactorWest = body.priceFactorWest;
    e.relationsPenalty = { ...body.relationsPenalty };
    e.objectivesDone = [...body.objectivesDone];
    // merge produced over fresh defaults: a pre-machinery save must not leave
    // produced.machinery undefined (undefined + n = NaN, forever)
    e.w.stats = {
      produced: {
        ...(Object.fromEntries(ALL_RESOURCES.map(r => [r, 0])) as Record<ResourceId, number>),
        ...body.stats.produced,
      },
      imported: { ...(body.stats.imported ?? {}) },
      exportedValue: body.stats.exportedValue,
      roadsBuilt: body.stats.roadsBuilt,
    };
    e.autoTrade = {
      enabled: body.autoTrade.enabled,
      reserveRubles: body.autoTrade.reserveRubles,
      reserveDollars: body.autoTrade.reserveDollars,
      rules: Object.fromEntries((Object.entries(body.autoTrade.rules) as [ResourceId, AutoTradeRule][]).map(([r, rule]) => [r, { ...rule }])),
    };
    if (body.globalCategoryPriorities) {
      for (const [cat, tier] of Object.entries(body.globalCategoryPriorities)) {
        if (tier !== undefined) {
          e.globalCategoryPriorities[cat as Category] = tier;
        }
      }
    }
    e.globalConstructionEnabled = body.globalConstructionEnabled ?? true;
    e.foreignLaborEnabled = body.foreignLaborEnabled ?? true;
    e.foreignLaborCurrency = body.foreignLaborCurrency ?? 'east';
    e.repairImportsEnabled = body.repairImportsEnabled ?? true;
    e.repairImportCurrency = body.repairImportCurrency ?? 'east';
    // Delivery dials. A pre-dial save carries a resource ranking and a mode
    // instead; there is no faithful mapping from a 13-item order onto four
    // consequence weights, so translate the INTENT of the old presets and let
    // everything else land neutral.
    const dials = { lifeline: 1, consumer: 1, industry: 1, construction: 1 };
    if (body.logisticsCategoryWeights) {
      for (const c of ['lifeline', 'consumer', 'industry', 'construction'] as LogisticsCategory[]) {
        const w = body.logisticsCategoryWeights[c];
        if (typeof w === 'number' && Number.isFinite(w) && w > 0) {
          dials[c] = Math.max(BALANCE.categoryDialMin, Math.min(BALANCE.categoryDialMax, w));
        }
      }
    } else if (body.logisticsPriorityMode === 'lifeline') {
      dials.lifeline = 2;
    } else if (body.logisticsPriorityMode === 'construction') {
      dials.construction = 2;
    }
    e.logisticsCategoryWeights = dials;
    e.emergencyFuelAutoBuy = body.emergencyFuelAutoBuy ?? true;
    // setPowerSectorOrder rejects anything that isn't a full permutation, so a
    // pre-grid or hand-edited save falls back to the default plan intact.
    if (body.powerSectorOrder) e.setPowerSectorOrder(body.powerSectorOrder);
    const cloneLedger = (l: TradeDayLedger): TradeDayLedger =>
      ({
        imports: { ...l.imports },
        exports: { ...l.exports },
        rubles: l.rubles,
        dollars: l.dollars,
        used: l.used,
        capacity: l.capacity,
        blocked: [...l.blocked],
        foreignLabor: l.foreignLabor ?? l.foreignLaborRubles ?? 0,
        foreignLaborRubles: l.foreignLaborRubles ?? l.foreignLabor ?? 0,
        foreignLaborDollars: l.foreignLaborDollars ?? 0,
        repairImports: l.repairImports ?? 0,
      });
    e.tradeLedger = { today: cloneLedger(body.tradeLedger.today), yesterday: cloneLedger(body.tradeLedger.yesterday) };
    e.contracts = body.contracts.map(c => ({ ...c }));
    e.loans = (body.loans ?? []).map(l => ({ ...l }));
    if (body.loanAutoRepay) e.loanAutoRepay = { ...body.loanAutoRepay };
    if (body.loanCooldown) e.loanCooldown = { ...body.loanCooldown };
    e.dryStreak = body.streaks.dry;
    e.gloomStreak = body.streaks.gloom;
    e.sunStreak = body.streaks.sun;
    e.wasFrost = body.streaks.wasFrost;
    e.acc = body.acc;
    e.lastRunSpeed = body.lastRunSpeed;
    e.nextBuildingId = body.counters.building;
    e.nextTruckId = body.counters.truck;
    e.nextBoatId = body.counters.boat;
    e.nextContractId = body.counters.contract;
    e.nextLoanId = body.counters.loan ?? 1;
    e.capacity = body.aggregates.capacity;
    e.workers = body.aggregates.workers;
    e.employed = body.aggregates.employed;
    e.jobs = body.aggregates.jobs;
    e.powerProduced = body.aggregates.powerProduced;
    e.powerDemand = body.aggregates.powerDemand;
    e.heatProduced = body.aggregates.heatProduced;
    e.heatDemand = body.aggregates.heatDemand;
    e.rng.setState(body.rngState);

    // hydrate over defaults so future BuildingInst fields load from old saves
    for (const saved of body.buildings) {
      const inst: BuildingInst = Object.assign(
        { staff: 0, eff: 0, powered: false, heated: false, connected: false, roadConnected: false, coalFactor: 0, farmFields: 0 },
        saved,
        { stock: { ...saved.stock }, incoming: { ...saved.incoming } },
      );
      // Per-site policy fields live only on sites (constructed buildings never
      // need them, keeping the blob lean and the round-trip stable). Old saves
      // predate per-site foreign labor — their in-progress sites kept building
      // under the old global default (on), so migrate a missing flag → true.
      if (!inst.constructed && inst.foreignLabor === undefined) inst.foreignLabor = true;
      e.addBuilding(inst);
    }
    const cloneTruck = (t: Mover): Mover => ({ ...t, points: t.points.map(p => ({ ...p })) });
    // Vehicles hydrate over defaults like buildings do. A pre-fleet save has
    // shipment-shaped trucks with no garage: adopt them into the nearest one
    // (syncFleet trims any surplus on the first simulated day) so the loaded
    // game is never short of lorries and no cargo in transit is dropped.
    const garages = [...e.buildings.values()].filter(b => {
      const def = BUILDINGS[b.defId];
      return def.isConstructionOffice || def.isMotorDepot;
    });
    e.trucks = body.trucks.map(t => {
      const saved = t as Partial<Vehicle> & Mover;
      const home = e.buildings.get(saved.homeId ?? -1)
        ?? garages.find(g => g.id === saved.srcId)
        ?? garages[0];
      return {
        ...cloneTruck(saved),
        homeId: saved.homeId ?? home?.id ?? 0,
        atId: saved.atId ?? 0,
        legTo: saved.legTo ?? saved.destId,
        state: saved.state ?? (saved.amount > 0 ? 'toDeliver' : 'idle'),
        fuel: saved.fuel ?? BALANCE.vehicleFuelCap,
        fuelCap: saved.fuelCap ?? BALANCE.vehicleFuelCap,
        odometer: saved.odometer ?? 0,
        legTiles: saved.legTiles ?? Math.max(1, saved.daysTotal / BALANCE.truckDaysPerTile),
        speed: saved.speed ?? 0,
        limping: saved.limping,
      } satisfies Vehicle;
    });
    e.boats = body.boats.map(cloneTruck);
    e.foreignTrucks = body.foreignTrucks.map(cloneTruck);
    e.boatOrders = body.boatOrders.map(o => ({ ...o }));

    // rebuild derived state: weather replays to the saved day; totals/alerts recompute
    e.weather = e.weatherAt(e.dayIndex());
    e.run(totals);
    e.updateAlerts();
    e.speed = 0;
    e.bump();
    return e;
  }

  // ---------------- events / subscription ----------------

  private pushEvent(text: string, kind: GameEvent['kind'], icon?: string) {
    this.w.pushEvent(text, kind, icon);
  }

  drainEvents(): GameEvent[] {
    const e = this.w.events;
    this.w.events = [];
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
