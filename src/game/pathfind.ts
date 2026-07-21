// ============================================================
// Weighted multi-source shortest-path on reusable scratch buffers.
//
// One floodCost() call answers distance and path queries for EVERY reachable
// tile, so callers evaluate all candidates with a single flood instead of one
// search per candidate. shortestPathToAny() is the bounded counterpart that
// stops at the nearest ranked goal.
//
// The weighted core is Dial's algorithm (bucket-queue Dijkstra): entry
// costs are tiny integers (1 = road, K = off-road), so a circular array of
// C = maxStep+1 FIFO buckets covers every live distance with no heap and no
// comparisons — O(V + E + D_max) and bit-deterministic (FIFO within a
// bucket, fixed +x,−x,+y,−y neighbour scan, parent set only on strict
// improvement). maxStep=1 (all weights 1) degenerates to plain FIFO BFS.
//
// NOT re-entrant: the module owns one set of scratch buffers, stamped with
// a generation counter. A FloodResult is a view valid only until the next
// flood (stale use throws) — consume it before flooding again.
//
// Buffers are lazily sized to the requested map. Resizing bumps the
// generation (fresh buffers zero the stamps, so an un-bumped stale view
// would silently read garbage instead of throwing).
// ============================================================

let W = 0, H = 0, N = 0;
let dist = new Int32Array(0);
let parent = new Int32Array(0);
let stamp = new Int32Array(0);   // stamp[k]===gen ⇔ node has a finite (tentative→final) distance
let settled = new Int32Array(0); // settled[k]===gen ⇔ popped with its final distance
let next = new Int32Array(0);    // intrusive FIFO chain within a bucket (-1 = end of chain)
let gen = 0; // monotonic across resizes — never reset

export const MAX_BUCKETS = 16; // supports maxStep up to 15 (off-road cost is 8)
/** Largest maxStep the Dial bucket ring supports (C = maxStep + 1 must be ≤ MAX_BUCKETS).
 *  Cost sources (e.g. TopologyIndex) validate their entry costs against this. */
export const MAX_SUPPORTED_STEP = MAX_BUCKETS - 1;
const bhead = new Int32Array(MAX_BUCKETS);
const btail = new Int32Array(MAX_BUCKETS);

function ensureSize(w: number, h: number) {
  if (w === W && h === H) return;
  W = w; H = h; N = w * h;
  dist = new Int32Array(N);
  parent = new Int32Array(N);
  stamp = new Int32Array(N);
  settled = new Int32Array(N);
  next = new Int32Array(N);
  gen++; // invalidate every outstanding FloodResult view
}

export interface RoadPos { x: number; y: number }

/** Entry cost of stepping onto (x,y); ≤ 0 means impassable. */
export type CostFn = (x: number, y: number) => number;

/**
 * A destination for a bounded shortest-path search. Ranks are explicit so a
 * caller can preserve its domain ordering even when several goals settle at
 * the same minimum distance (for logistics: building insertion order, then
 * that building's access-tile order).
 */
export interface RankedGoal<T> extends RoadPos {
  value: T;
  buildingRank: number;
  accessRank: number;
}

/** Owned result from shortestPathToAny(); safe across subsequent searches. */
export interface NearestPath<T> {
  goal: RankedGoal<T>;
  /** Tiles from the selected goal to the winning source, both inclusive. */
  path: RoadPos[];
  /** Accumulated entry cost from the winning source to the selected goal. */
  cost: number;
  /** Number of nodes popped with final distances, including all winning-distance ties. */
  settledNodes: number;
}

export class FloodResult {
  private readonly myGen: number;
  private readonly myW: number;
  constructor(myGen: number, myW: number) { this.myGen = myGen; this.myW = myW; }

  private check() {
    if (this.myGen !== gen) throw new Error('Stale FloodResult: another flood ran — consume it before the next flood');
  }

  /** Accumulated travel COST from the nearest source; -1 if unreachable. */
  distanceAt(x: number, y: number): number {
    this.check();
    const k = y * this.myW + x;
    return stamp[k] === gen ? dist[k] : -1;
  }

  /** Tiles from (x,y) to the nearest source, both inclusive; null if unreachable. */
  pathFrom(x: number, y: number): RoadPos[] | null {
    this.check();
    let k = y * this.myW + x;
    if (stamp[k] !== gen) return null;
    const path: RoadPos[] = [];
    while (k >= 0) {
      path.push({ x: k % this.myW, y: Math.floor(k / this.myW) });
      k = parent[k];
    }
    return path;
  }
}

/**
 * Weighted multi-source shortest-path (Dial's). `cost(x,y)` is the entry
 * cost of a tile (≤0 = impassable); `maxStep` is the largest finite cost
 * (bounds the bucket ring). Sources on impassable tiles are ignored.
 */
export function floodCost(w: number, h: number, cost: CostFn, sources: RoadPos[], maxStep: number): FloodResult {
  if (!Number.isInteger(maxStep) || maxStep < 1) {
    throw new Error(`floodCost: maxStep must be a positive integer (received ${maxStep})`);
  }
  const C = maxStep + 1;
  if (C > MAX_BUCKETS) throw new Error(`floodCost: maxStep ${maxStep} exceeds MAX_BUCKETS ${MAX_BUCKETS}`);
  ensureSize(w, h); // validate before any side effect: a bad call must not bump gen or resize
  gen++;
  for (let b = 0; b < C; b++) { bhead[b] = -1; btail[b] = -1; }
  let active = 0;

  const push = (b: number, k: number) => {
    next[k] = -1;
    if (bhead[b] === -1) bhead[b] = k; else next[btail[b]] = k;
    btail[b] = k; active++;
  };

  for (const s of sources) {
    if (s.x < 0 || s.y < 0 || s.x >= W || s.y >= H) continue;
    if (cost(s.x, s.y) <= 0) continue;
    const k = s.y * W + s.x;
    if (stamp[k] === gen) continue; // dedup repeated sources
    stamp[k] = gen; dist[k] = 0; parent[k] = -1;
    push(0, k);
  }

  let curD = 0;
  while (active > 0) {
    const b = curD % C;
    const k = bhead[b];
    if (k === -1) { curD++; continue; } // no live entry at this distance yet — advance
    bhead[b] = next[k];
    if (bhead[b] === -1) btail[b] = -1;
    active--;
    if (settled[k] === gen) continue; // stale re-insertion (already popped at its min distance)
    if (dist[k] !== curD) continue;   // stale (this entry's distance was improved after it was queued)
    settled[k] = gen;
    const cx = k % W, cy = Math.floor(k / W);
    for (let i = 0; i < 4; i++) {
      const nx = cx + (i === 0 ? 1 : i === 1 ? -1 : 0);
      const ny = cy + (i === 2 ? 1 : i === 3 ? -1 : 0);
      if (nx < 0 || ny < 0 || nx >= W || ny >= H) continue;
      const cv = cost(nx, ny);
      if (cv <= 0) continue;
      const nk = ny * W + nx;
      if (settled[nk] === gen) continue;
      const nd = curD + cv;
      if (stamp[nk] !== gen || nd < dist[nk]) { // first sight, or a strictly shorter path
        stamp[nk] = gen; dist[nk] = nd; parent[nk] = k;
        push(nd % C, nk);
      }
    }
  }
  return new FloodResult(gen, W);
}

/**
 * Weighted multi-source shortest path to the nearest eligible goal.
 *
 * This is the bounded counterpart to floodCost(): it uses the same Dial FIFO
 * buckets, source ordering, +x,−x,+y,−y neighbour order, and strict-only
 * parent replacement. Once the first goal distance is known, every node at
 * that distance is still settled so goal ties are resolved by building rank,
 * access rank, then input order rather than queue encounter order.
 *
 * The returned path owns its points. Like a flood, invoking this search
 * invalidates outstanding FloodResult views because it reuses the same module
 * scratch buffers.
 */
export function shortestPathToAny<T>(
  w: number,
  h: number,
  cost: CostFn,
  sources: readonly RoadPos[],
  goals: readonly RankedGoal<T>[],
  maxStep: number,
): NearestPath<T> | null {
  if (!Number.isInteger(maxStep) || maxStep < 1) {
    throw new Error(`shortestPathToAny: maxStep must be a positive integer (received ${maxStep})`);
  }
  const C = maxStep + 1;
  if (C > MAX_BUCKETS) throw new Error(`shortestPathToAny: maxStep ${maxStep} exceeds MAX_BUCKETS ${MAX_BUCKETS}`);
  ensureSize(w, h); // validate before any side effect: a bad call must not bump gen or resize
  gen++;
  for (let b = 0; b < C; b++) { bhead[b] = -1; btail[b] = -1; }

  // A tile can be an access point for several buildings (or several ordered
  // access entries). Keep every goal so domain tie-breaking remains exact.
  const goalsByTile = new Map<number, { goal: RankedGoal<T>; order: number }[]>();
  for (let order = 0; order < goals.length; order++) {
    const goal = goals[order];
    if (goal.x < 0 || goal.y < 0 || goal.x >= W || goal.y >= H) continue;
    const k = goal.y * W + goal.x;
    const atTile = goalsByTile.get(k);
    const entry = { goal, order };
    if (atTile) atTile.push(entry); else goalsByTile.set(k, [entry]);
  }
  if (goalsByTile.size === 0) return null;

  let active = 0;
  const push = (b: number, k: number) => {
    next[k] = -1;
    if (bhead[b] === -1) bhead[b] = k; else next[btail[b]] = k;
    btail[b] = k; active++;
  };

  for (const s of sources) {
    if (s.x < 0 || s.y < 0 || s.x >= W || s.y >= H) continue;
    if (cost(s.x, s.y) <= 0) continue;
    const k = s.y * W + s.x;
    if (stamp[k] === gen) continue;
    stamp[k] = gen; dist[k] = 0; parent[k] = -1;
    push(0, k);
  }

  let curD = 0;
  let settledNodes = 0;
  let winningDistance = -1;
  let winningK = -1;
  let winningGoal: RankedGoal<T> | null = null;
  let winningOrder = Infinity;

  const winsTie = (goal: RankedGoal<T>, order: number) => {
    if (!winningGoal) return true;
    if (goal.buildingRank !== winningGoal.buildingRank) return goal.buildingRank < winningGoal.buildingRank;
    if (goal.accessRank !== winningGoal.accessRank) return goal.accessRank < winningGoal.accessRank;
    return order < winningOrder;
  };

  while (active > 0) {
    if (winningDistance >= 0 && curD > winningDistance) break;
    const b = curD % C;
    const k = bhead[b];
    if (k === -1) { curD++; continue; }
    bhead[b] = next[k];
    if (bhead[b] === -1) btail[b] = -1;
    active--;
    if (settled[k] === gen) continue;
    if (dist[k] !== curD) continue;
    settled[k] = gen;
    settledNodes++;

    const tileGoals = goalsByTile.get(k);
    if (tileGoals) {
      if (winningDistance < 0) winningDistance = curD;
      for (const { goal, order } of tileGoals) {
        if (winsTie(goal, order)) {
          winningK = k;
          winningGoal = goal;
          winningOrder = order;
        }
      }
    }

    // All edge costs are positive. Once a goal settles at curD, expanding any
    // node at curD can only create nodes beyond the winning distance.
    if (winningDistance >= 0) continue;

    const cx = k % W, cy = Math.floor(k / W);
    for (let i = 0; i < 4; i++) {
      const nx = cx + (i === 0 ? 1 : i === 1 ? -1 : 0);
      const ny = cy + (i === 2 ? 1 : i === 3 ? -1 : 0);
      if (nx < 0 || ny < 0 || nx >= W || ny >= H) continue;
      const cv = cost(nx, ny);
      if (cv <= 0) continue;
      const nk = ny * W + nx;
      if (settled[nk] === gen) continue;
      const nd = curD + cv;
      if (stamp[nk] !== gen || nd < dist[nk]) {
        stamp[nk] = gen; dist[nk] = nd; parent[nk] = k;
        push(nd % C, nk);
      }
    }
  }

  if (!winningGoal || winningK < 0) return null;
  const path: RoadPos[] = [];
  let k = winningK;
  while (k >= 0) {
    path.push({ x: k % W, y: Math.floor(k / W) });
    k = parent[k];
  }
  return { goal: winningGoal, path, cost: winningDistance, settledNodes };
}
