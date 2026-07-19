// ============================================================
// Road-network BFS on reusable scratch buffers.
//
// One floodRoads() call answers distance and path queries for EVERY
// tile on the network, so callers evaluate all candidates with a single
// BFS instead of one BFS per candidate.
//
// NOT re-entrant: the module owns one set of scratch buffers, stamped
// with a generation counter. A FloodResult is a view over them and is
// valid only until the next floodRoads() call (stale use throws).
// Cache a DistanceField snapshot instead when persistence is needed.
//
// Buffers are lazily sized to the requested map. Resizing bumps the
// generation (fresh buffers zero the stamps, so an un-bumped stale view
// would silently read garbage instead of throwing).
// ============================================================

let W = 0, H = 0, N = 0;
let dist = new Int32Array(0);
let parent = new Int32Array(0);
let stamp = new Int32Array(0); // stamp[k] === gen ⇔ visited in current flood
let queue = new Int32Array(0);
let gen = 0; // monotonic across resizes — never reset

function ensureSize(w: number, h: number) {
  if (w === W && h === H) return;
  W = w; H = h; N = w * h;
  dist = new Int32Array(N);
  parent = new Int32Array(N);
  stamp = new Int32Array(N);
  queue = new Int32Array(N);
  gen++; // invalidate every outstanding FloodResult view
}

export interface RoadPos { x: number; y: number }

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
    if (this.myGen !== gen) throw new Error('Stale FloodResult: floodRoads() ran again — snapshot() results you need to keep');
  }

  /** Road-tile distance from the nearest source; -1 if unreachable. */
  distanceAt(x: number, y: number): number {
    this.check();
    const k = y * this.myW + x;
    return stamp[k] === gen ? dist[k] : -1;
  }

  /** Road tiles from (x,y) to the nearest source, both inclusive; null if unreachable. */
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

/** Multi-source BFS over road tiles of a w×h map. Non-road sources are ignored. */
export function floodRoads(w: number, h: number, isRoad: (x: number, y: number) => boolean, sources: RoadPos[]): FloodResult {
  ensureSize(w, h);
  gen++;
  let head = 0, tail = 0;
  for (const s of sources) {
    if (s.x < 0 || s.y < 0 || s.x >= W || s.y >= H || !isRoad(s.x, s.y)) continue;
    const k = s.y * W + s.x;
    if (stamp[k] === gen) continue;
    stamp[k] = gen; dist[k] = 0; parent[k] = -1;
    queue[tail++] = k;
  }
  while (head < tail) {
    const cur = queue[head++];
    const cx = cur % W, cy = Math.floor(cur / W);
    for (let i = 0; i < 4; i++) {
      const nx = cx + (i === 0 ? 1 : i === 1 ? -1 : 0);
      const ny = cy + (i === 2 ? 1 : i === 3 ? -1 : 0);
      if (nx < 0 || ny < 0 || nx >= W || ny >= H) continue;
      const nk = ny * W + nx;
      if (stamp[nk] === gen || !isRoad(nx, ny)) continue;
      stamp[nk] = gen; dist[nk] = dist[cur] + 1; parent[nk] = cur;
      queue[tail++] = nk;
    }
  }
  return new FloodResult(gen, W);
}
