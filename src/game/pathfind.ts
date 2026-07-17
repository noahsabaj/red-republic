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
// ============================================================
import { MAP_W, MAP_H } from './mapgen';

const N = MAP_W * MAP_H;
const dist = new Int32Array(N);
const parent = new Int32Array(N);
const stamp = new Int32Array(N); // stamp[k] === gen ⇔ visited in current flood
const queue = new Int32Array(N);
let gen = 0;

export interface RoadPos { x: number; y: number }

export class DistanceField {
  private readonly d: Int32Array; // -1 = unreachable
  constructor(d: Int32Array) { this.d = d; }
  distanceAt(x: number, y: number): number { return this.d[y * MAP_W + x]; }
  reachable(x: number, y: number): boolean { return this.d[y * MAP_W + x] >= 0; }
}

export class FloodResult {
  private readonly myGen: number;
  constructor(myGen: number) { this.myGen = myGen; }

  private check() {
    if (this.myGen !== gen) throw new Error('Stale FloodResult: floodRoads() ran again — snapshot() results you need to keep');
  }

  /** Road-tile distance from the nearest source; -1 if unreachable. */
  distanceAt(x: number, y: number): number {
    this.check();
    const k = y * MAP_W + x;
    return stamp[k] === gen ? dist[k] : -1;
  }

  /** Road tiles from (x,y) to the nearest source, both inclusive; null if unreachable. */
  pathFrom(x: number, y: number): RoadPos[] | null {
    this.check();
    let k = y * MAP_W + x;
    if (stamp[k] !== gen) return null;
    const path: RoadPos[] = [];
    while (k >= 0) {
      path.push({ x: k % MAP_W, y: Math.floor(k / MAP_W) });
      k = parent[k];
    }
    return path;
  }

  /** Copy distances into owned storage, safe to cache across floods. */
  snapshot(): DistanceField {
    this.check();
    const d = new Int32Array(N).fill(-1);
    for (let k = 0; k < N; k++) if (stamp[k] === gen) d[k] = dist[k];
    return new DistanceField(d);
  }
}

/** Multi-source BFS over road tiles. Non-road sources are ignored. */
export function floodRoads(isRoad: (x: number, y: number) => boolean, sources: RoadPos[]): FloodResult {
  gen++;
  let head = 0, tail = 0;
  for (const s of sources) {
    if (s.x < 0 || s.y < 0 || s.x >= MAP_W || s.y >= MAP_H || !isRoad(s.x, s.y)) continue;
    const k = s.y * MAP_W + s.x;
    if (stamp[k] === gen) continue;
    stamp[k] = gen; dist[k] = 0; parent[k] = -1;
    queue[tail++] = k;
  }
  while (head < tail) {
    const cur = queue[head++];
    const cx = cur % MAP_W, cy = Math.floor(cur / MAP_W);
    for (let i = 0; i < 4; i++) {
      const nx = cx + (i === 0 ? 1 : i === 1 ? -1 : 0);
      const ny = cy + (i === 2 ? 1 : i === 3 ? -1 : 0);
      if (nx < 0 || ny < 0 || nx >= MAP_W || ny >= MAP_H) continue;
      const nk = ny * MAP_W + nx;
      if (stamp[nk] === gen || !isRoad(nx, ny)) continue;
      stamp[nk] = gen; dist[nk] = dist[cur] + 1; parent[nk] = cur;
      queue[tail++] = nk;
    }
  }
  return new FloodResult(gen);
}
