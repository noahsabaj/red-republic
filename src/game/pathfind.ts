// ============================================================
// Weighted multi-source shortest-path on reusable scratch buffers.
//
// One floodCost()/floodRoads() call answers distance and path queries for
// EVERY reachable tile, so callers evaluate all candidates with a single
// flood instead of one search per candidate.
//
// The weighted core is Dial's algorithm (bucket-queue Dijkstra): entry
// costs are tiny integers (1 = road, K = off-road), so a circular array of
// C = maxStep+1 FIFO buckets covers every live distance with no heap and no
// comparisons — O(V + E + D_max) and bit-deterministic (FIFO within a
// bucket, fixed +x,−x,+y,−y neighbour scan, parent set only on strict
// improvement). floodRoads() is the all-weights-1 specialization: it
// degenerates to plain FIFO BFS, byte-identical to the old road flood.
//
// NOT re-entrant: the module owns one set of scratch buffers, stamped with
// a generation counter. A FloodResult is a view valid only until the next
// flood (stale use throws). Cache a DistanceField snapshot to persist.
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

const MAX_BUCKETS = 16; // supports maxStep up to 15 (off-road cost is 8)
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

export class DistanceField {
  private readonly d: Int32Array; // -1 = unreachable
  private readonly w: number;
  constructor(d: Int32Array, w: number) { this.d = d; this.w = w; }
  distanceAt(x: number, y: number): number { return this.d[y * this.w + x]; }
  reachable(x: number, y: number): boolean { return this.d[y * this.w + x] >= 0; }
}

export class FloodResult {
  private readonly myGen: number;
  private readonly myW: number;
  constructor(myGen: number, myW: number) { this.myGen = myGen; this.myW = myW; }

  private check() {
    if (this.myGen !== gen) throw new Error('Stale FloodResult: another flood ran — snapshot() results you need to keep');
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

  /** Copy distances into owned storage, safe to cache across floods. */
  snapshot(): DistanceField {
    this.check();
    const d = new Int32Array(N).fill(-1);
    for (let k = 0; k < N; k++) if (stamp[k] === gen) d[k] = dist[k];
    return new DistanceField(d, this.myW);
  }
}

/**
 * Weighted multi-source shortest-path (Dial's). `cost(x,y)` is the entry
 * cost of a tile (≤0 = impassable); `maxStep` is the largest finite cost
 * (bounds the bucket ring). Sources on impassable tiles are ignored.
 */
export function floodCost(w: number, h: number, cost: CostFn, sources: RoadPos[], maxStep: number): FloodResult {
  ensureSize(w, h);
  gen++;
  const C = maxStep + 1;
  if (C > MAX_BUCKETS) throw new Error(`floodCost: maxStep ${maxStep} exceeds MAX_BUCKETS ${MAX_BUCKETS}`);
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
 * Multi-source BFS over road tiles (all weights 1). Non-road sources are
 * ignored. Byte-identical to the historical road flood — the weighted core
 * with maxStep=1 is plain FIFO BFS.
 */
export function floodRoads(w: number, h: number, isRoad: (x: number, y: number) => boolean, sources: RoadPos[]): FloodResult {
  return floodCost(w, h, (x, y) => (isRoad(x, y) ? 1 : 0), sources, 1);
}
