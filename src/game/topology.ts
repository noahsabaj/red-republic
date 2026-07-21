import type { Tile } from './mapgen';
import { MAX_SUPPORTED_STEP } from './pathfind';

/** The tile fields that affect vehicle routing. */
export type RoutingTile = Pick<Tile, 'terrain' | 'road' | 'buildingId' | 'foreign'>;

export type TopologyDomain = 'road' | 'land' | 'water';
const DOMAINS: readonly TopologyDomain[] = ['road', 'land', 'water'];

export interface TopologyPos {
  readonly x: number;
  readonly y: number;
}

export interface TopologyFootprint {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

export interface TopologyAccess {
  /** Passable perimeter tiles in the engine's historical scan order. */
  readonly tiles: readonly TopologyPos[];
  /** Distinct component IDs, ordered by their first access tile. */
  readonly components: readonly number[];
}

/** Entry cost for one routing domain. A non-positive value is impassable. */
export type TopologyCost = (tile: Readonly<RoutingTile>, x: number, y: number) => number;

export interface TopologyCosts {
  readonly road?: TopologyCost;
  readonly land?: TopologyCost;
  readonly water?: TopologyCost;
}

export interface TopologyIndexOptions {
  readonly width: number;
  readonly height: number;
  /**
   * Fetched at rebuild time, rather than captured by value, so replacing the
   * engine's live tile grid does not leave this index pointed at an old map.
   */
  readonly tiles: () => readonly (readonly Readonly<RoutingTile>[])[];
  readonly offRoadCost: number;
  /** Optional domain overrides; omitted costs use the GameEngine predicates. */
  readonly costs?: TopologyCosts;
}

export interface TopologyDiagnostics {
  readonly revisions: Readonly<Record<TopologyDomain, number>>;
  readonly builtRevisions: Readonly<Record<TopologyDomain, number>>;
  readonly rebuilds: Readonly<Record<TopologyDomain, number>>;
  readonly componentCounts: Readonly<Record<TopologyDomain, number>>;
}

interface DomainState {
  revision: number;
  builtRevision: number;
  rebuilds: number;
  mask: Uint8Array;
  components: Int32Array;
  componentCount: number;
  /** Largest passable entry cost in the current mask (≥ 1). This is exactly the
   *  bucket-ring bound a Dial flood over this mask needs, so pathfinders stay in
   *  sync with the costs the topology actually produced. */
  maxStep: number;
  accessCache: Map<string, TopologyAccess>;
}

function makeState(): DomainState {
  return {
    revision: 0,
    builtRevision: -1,
    rebuilds: 0,
    mask: new Uint8Array(0),
    components: new Int32Array(0),
    componentCount: 0,
    maxStep: 1,
    accessCache: new Map(),
  };
}

/** True when two ordered component sets have at least one non-zero ID in common. */
export function shareAnyComponent(a: readonly number[], b: readonly number[]): boolean {
  if (a.length === 0 || b.length === 0) return false;
  const [scan, lookup] = a.length <= b.length ? [a, b] : [b, a];
  const ids = new Set(lookup);
  for (const id of scan) if (id > 0 && ids.has(id)) return true;
  return false;
}

/** Union of several already-deduped component lists, keeping first-seen order.
 *  The engine folds many buildings' `access(...).components` into one anchor set. */
export function unionComponents(...lists: readonly (readonly number[])[]): number[] {
  const out: number[] = [];
  const seen = new Set<number>();
  for (const list of lists) {
    for (const id of list) if (id > 0 && !seen.has(id)) { seen.add(id); out.push(id); }
  }
  return out;
}

/**
 * Visit the tiles orthogonally surrounding a footprint — the on/off-ramp ring — in
 * the engine's historical scan order: top edge, then each middle row's left & right,
 * then the bottom edge. Diagonal corners (touching only at a point) are excluded
 * unless `opts.corners` is set. Return a truthy value from `fn` to stop early.
 * The single source of the perimeter geometry for both routing access and the
 * engine's placement/advisory checks.
 */
export function forEachPerimeterTile(
  x: number,
  y: number,
  w: number,
  h: number,
  opts: { corners?: boolean },
  fn: (px: number, py: number) => boolean | void,
): void {
  const includeCorners = opts.corners ?? false;
  for (let dy = -1; dy <= h; dy++) {
    for (let dx = -1; dx <= w; dx++) {
      const onEdge = dx === -1 || dx === w || dy === -1 || dy === h;
      if (!onEdge) continue;
      const onCorner = (dx === -1 || dx === w) && (dy === -1 || dy === h);
      if (onCorner && !includeCorners) continue;
      if (fn(x + dx, y + dy)) return;
    }
  }
}

/**
 * Lazily materialized routing costs and connected components for one engine.
 *
 * Returned typed-array views are valid until that domain is invalidated. The
 * next query rebuilds that domain from the current grid supplied by `tiles`.
 */
export class TopologyIndex {
  readonly width: number;
  readonly height: number;

  private readonly tilesProvider: TopologyIndexOptions['tiles'];
  private readonly costs: Record<TopologyDomain, TopologyCost>;
  private readonly states: Record<TopologyDomain, DomainState> = {
    road: makeState(),
    land: makeState(),
    water: makeState(),
  };
  private readonly queue: Int32Array;

  constructor(options: TopologyIndexOptions) {
    if (!Number.isInteger(options.width) || options.width <= 0) {
      throw new Error(`TopologyIndex: invalid width ${options.width}`);
    }
    if (!Number.isInteger(options.height) || options.height <= 0) {
      throw new Error(`TopologyIndex: invalid height ${options.height}`);
    }
    if (!Number.isInteger(options.offRoadCost) || options.offRoadCost <= 0 || options.offRoadCost > MAX_SUPPORTED_STEP) {
      // Bounded by the pathfinder's Dial bucket ring, not the Uint8Array mask:
      // an entry cost the flood can't represent must fail here, not mid-day.
      throw new Error(`TopologyIndex: offRoadCost must be an integer in [1, ${MAX_SUPPORTED_STEP}], got ${options.offRoadCost}`);
    }

    this.width = options.width;
    this.height = options.height;
    this.tilesProvider = options.tiles;
    this.queue = new Int32Array(this.width * this.height);
    this.costs = {
      // Roads deliberately ignore terrain, foreign soil, and footprints.
      road: options.costs?.road ?? ((tile) => tile.road ? 1 : 0),
      // Roads on water/foreign soil are not land-drivable; an ordinary road
      // remains drivable even if a footprint happens to reference its tile.
      land: options.costs?.land ?? ((tile) =>
        tile.foreign || tile.terrain === 'water' || (!tile.road && tile.buildingId)
          ? 0
          : tile.road ? 1 : options.offRoadCost),
      // Bridges do not remove the water beneath them from the water network.
      water: options.costs?.water ?? ((tile) => tile.terrain === 'water' ? 1 : 0),
    };
  }

  invalidateRoad(): void { this.invalidate('road'); }
  invalidateLand(): void { this.invalidate('land'); }
  invalidateWater(): void { this.invalidate('water'); }

  /** Increment each requested domain at most once, even if it is repeated. */
  invalidate(...domains: readonly TopologyDomain[]): void {
    const dirty = new Set(domains);
    for (const domain of dirty) {
      const state = this.states[domain];
      state.revision++;
      state.accessCache.clear();
    }
  }

  /**
   * Domains whose entry cost differs between two tile states at (x,y) — the exact
   * set that must be invalidated when a tile changes. The cost functions are the
   * single source of truth for both cost AND invalidation, so a caller never has
   * to hand-mirror which fields each domain reads. (A road laid on water/foreign,
   * or a footprint stamped on a road tile, leaves land cost unchanged → no land
   * rebuild, unlike a field→domain map that over-invalidates.)
   */
  affectedDomains(
    before: Readonly<RoutingTile>,
    after: Readonly<RoutingTile>,
    x: number,
    y: number,
  ): TopologyDomain[] {
    const out: TopologyDomain[] = [];
    for (const domain of DOMAINS) {
      const cost = this.costs[domain];
      if (cost(before, x, y) !== cost(after, x, y)) out.push(domain);
    }
    return out;
  }

  revision(domain: TopologyDomain): number { return this.states[domain].revision; }
  rebuildCount(domain: TopologyDomain): number { return this.states[domain].rebuilds; }

  /** Cost mask indexed by `y * width + x`; zero means impassable. */
  mask(domain: TopologyDomain): Uint8Array {
    return this.ensure(domain).mask;
  }

  /** Component IDs indexed by `y * width + x`; zero means impassable. */
  components(domain: TopologyDomain): Int32Array {
    return this.ensure(domain).components;
  }

  componentCount(domain: TopologyDomain): number {
    return this.ensure(domain).componentCount;
  }

  /** Largest passable entry cost in this domain's current mask (≥ 1) — exactly the
   *  `maxStep` a Dial flood over `mask(domain)` must use, so the bound can never
   *  disagree with the costs the topology produced (road/water → 1, land → offRoadCost). */
  maxStep(domain: TopologyDomain): number {
    return this.ensure(domain).maxStep;
  }

  componentAt(domain: TopologyDomain, x: number, y: number): number {
    if (!this.inBounds(x, y)) return 0;
    return this.ensure(domain).components[y * this.width + x];
  }

  /**
   * Footprint-adjacent passable tiles. The scan exactly matches the previous
   * engine helpers: top edge, alternating left/right sides, then bottom edge;
   * diagonal corners are excluded.
   */
  access(domain: TopologyDomain, footprint: TopologyFootprint): TopologyAccess {
    this.assertFootprint(footprint);
    const state = this.ensure(domain);
    const key = `${footprint.x},${footprint.y},${footprint.w},${footprint.h}`;
    const cached = state.accessCache.get(key);
    if (cached) return cached;

    const tiles: TopologyPos[] = [];
    const components: number[] = [];
    const seenComponents = new Set<number>();

    forEachPerimeterTile(footprint.x, footprint.y, footprint.w, footprint.h, {}, (x, y) => {
      if (!this.inBounds(x, y)) return;
      const index = y * this.width + x;
      if (state.mask[index] === 0) return;

      tiles.push(Object.freeze({ x, y }));
      const component = state.components[index];
      if (component > 0 && !seenComponents.has(component)) {
        seenComponents.add(component);
        components.push(component);
      }
    });

    const result = Object.freeze({
      tiles: Object.freeze(tiles),
      components: Object.freeze(components),
    });
    state.accessCache.set(key, result);
    return result;
  }

  getDiagnostics(): TopologyDiagnostics {
    const record = <K extends keyof DomainState>(key: K): Record<TopologyDomain, DomainState[K]> => ({
      road: this.states.road[key],
      land: this.states.land[key],
      water: this.states.water[key],
    });
    return {
      revisions: record('revision'),
      builtRevisions: record('builtRevision'),
      rebuilds: record('rebuilds'),
      componentCounts: record('componentCount'),
    };
  }

  private ensure(domain: TopologyDomain): DomainState {
    const state = this.states[domain];
    if (state.builtRevision === state.revision) return state;

    const grid = this.tilesProvider();
    this.assertGrid(grid);
    const size = this.width * this.height;
    const mask = new Uint8Array(size);
    const components = new Int32Array(size);
    const cost = this.costs[domain];
    let maxStep = 1; // floor: a Dial ring needs C = maxStep + 1 ≥ 2 even for an empty domain

    for (let y = 0; y < this.height; y++) {
      const row = grid[y];
      for (let x = 0; x < this.width; x++) {
        const value = cost(row[x], x, y);
        if (!Number.isFinite(value) || !Number.isInteger(value) || value > MAX_SUPPORTED_STEP) {
          throw new Error(`TopologyIndex: ${domain} cost at (${x},${y}) must be an integer in [0, ${MAX_SUPPORTED_STEP}], got ${value}`);
        }
        if (value > 0) {
          mask[y * this.width + x] = value;
          if (value > maxStep) maxStep = value;
        }
      }
    }

    let componentCount = 0;
    for (let start = 0; start < size; start++) {
      if (mask[start] === 0 || components[start] !== 0) continue;
      componentCount++;
      components[start] = componentCount;
      let head = 0;
      let tail = 0;
      this.queue[tail++] = start;

      while (head < tail) {
        const current = this.queue[head++];
        const x = current % this.width;
        const y = Math.floor(current / this.width);
        for (let i = 0; i < 4; i++) {
          const nx = x + (i === 0 ? 1 : i === 1 ? -1 : 0);
          const ny = y + (i === 2 ? 1 : i === 3 ? -1 : 0);
          if (!this.inBounds(nx, ny)) continue;
          const next = ny * this.width + nx;
          if (mask[next] === 0 || components[next] !== 0) continue;
          components[next] = componentCount;
          this.queue[tail++] = next;
        }
      }
    }

    state.mask = mask;
    state.components = components;
    state.componentCount = componentCount;
    state.maxStep = maxStep;
    state.builtRevision = state.revision;
    state.rebuilds++;
    state.accessCache.clear();
    return state;
  }

  private inBounds(x: number, y: number): boolean {
    return Number.isInteger(x) && Number.isInteger(y) &&
      x >= 0 && y >= 0 && x < this.width && y < this.height;
  }

  private assertGrid(grid: readonly (readonly Readonly<RoutingTile>[])[]): void {
    if (grid.length !== this.height) {
      throw new Error(`TopologyIndex: tile grid height ${grid.length} does not match ${this.height}`);
    }
    for (let y = 0; y < this.height; y++) {
      if (grid[y].length !== this.width) {
        throw new Error(`TopologyIndex: tile row ${y} width ${grid[y].length} does not match ${this.width}`);
      }
    }
  }

  private assertFootprint(footprint: TopologyFootprint): void {
    if (![footprint.x, footprint.y, footprint.w, footprint.h].every(Number.isInteger) ||
      footprint.w <= 0 || footprint.h <= 0) {
      throw new Error('TopologyIndex: footprint coordinates and positive dimensions must be integers');
    }
  }
}
