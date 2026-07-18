// Map generation: terrain, forests, deposits, river
import type { DepositType } from './config';

export interface Tile {
  terrain: 'grass' | 'forest' | 'water' | 'rock';
  deposit?: DepositType;
  road?: boolean;
  buildingId?: number; // building occupying (footprint tiles all point to id)
  variant: number;     // visual variation seed 0..1
}

export const MAP_W = 48;
export const MAP_H = 48;

// deterministic rng (shared with the engine for economy drift)
export function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export interface MapData {
  tiles: Tile[][];
  startX: number; // suggested starting area (near center, clear)
  startY: number;
}

function carveDisk(tiles: Tile[][], cx: number, cy: number, r: number) {
  for (let y = Math.floor(cy - r); y <= Math.ceil(cy + r); y++) {
    for (let x = Math.floor(cx - r); x <= Math.ceil(cx + r); x++) {
      const t = tiles[y]?.[x];
      if (t && Math.hypot(x + 0.5 - cx, y + 0.5 - cy) <= r) t.terrain = 'water';
    }
  }
}

/**
 * A river with a genuinely random course: it enters at one random map edge,
 * exits at a different one, and meanders between them on a momentum walk
 * (repelled from the starting base). Once per river it may briefly fork
 * into a second channel, leaving an island between the two arms.
 */
function carveRiver(tiles: Tile[][], rnd: () => number, sx: number, sy: number) {
  const edgePoint = (edge: number) =>
    edge === 0 ? { x: 3 + rnd() * (MAP_W - 6), y: -1 }
    : edge === 1 ? { x: 3 + rnd() * (MAP_W - 6), y: MAP_H }
    : edge === 2 ? { x: -1, y: 3 + rnd() * (MAP_H - 6) }
    : { x: MAP_W, y: 3 + rnd() * (MAP_H - 6) };
  const e1 = Math.floor(rnd() * 4);
  let e2 = Math.floor(rnd() * 4);
  while (e2 === e1) e2 = Math.floor(rnd() * 4);
  const a = edgePoint(e1), b = edgePoint(e2);

  let px = a.x, py = a.y, vx = 0, vy = 0;
  let width = 1.5 + rnd() * 0.6;
  let forkUsed = false, forkRemaining = 0, forkTotal = 0;

  for (let step = 0; step < 400; step++) {
    const dx = b.x - px, dy = b.y - py;
    const dist = Math.hypot(dx, dy);
    if (dist < 1) break;
    // steer: pull toward the exit + wander + keep clear of the starting base
    let ax = (dx / dist) * 0.4 + (rnd() - 0.5) * 0.55;
    let ay = (dy / dist) * 0.4 + (rnd() - 0.5) * 0.55;
    const cdx = px - sx, cdy = py - sy;
    const cd = Math.hypot(cdx, cdy) || 1;
    if (cd < 12) { ax += (cdx / cd) * (12 - cd) * 0.12; ay += (cdy / cd) * (12 - cd) * 0.12; }
    vx = (vx + ax) * 0.7;
    vy = (vy + ay) * 0.7;
    const vlen = Math.hypot(vx, vy) || 1;
    px += (vx / vlen) * 0.8;
    py += (vy / vlen) * 0.8;
    width = Math.max(1.3, Math.min(2.4, width + (rnd() - 0.5) * 0.12));
    carveDisk(tiles, px, py, width);

    if (!forkUsed && rnd() < 0.025 && dist > 12) {
      forkUsed = true;
      forkTotal = 12 + Math.floor(rnd() * 6);
      forkRemaining = forkTotal;
    }
    if (forkRemaining > 0) {
      // side channel that rejoins: gap follows a half-sine so the arms
      // merge at both ends and enclose an island at the widest point
      const t = 1 - forkRemaining / forkTotal;
      const gap = Math.sin(Math.PI * t) * (width + 2.6);
      const nx = -vy / vlen, ny = vx / vlen;
      carveDisk(tiles, px + nx * gap, py + ny * gap, Math.max(1.1, width * 0.8));
      forkRemaining--;
    }
  }
}

/** A blobby lake (wobbled radius); big ones sometimes hold an island. */
function carveLake(tiles: Tile[][], rnd: () => number, sx: number, sy: number) {
  for (let tries = 0; tries < 20; tries++) {
    const cx = 5 + rnd() * (MAP_W - 10);
    const cy = 5 + rnd() * (MAP_H - 10);
    if (Math.hypot(cx - sx, cy - sy) < 13) continue;
    const R = 2.4 + rnd() * 2.6;
    const phase = rnd() * Math.PI * 2;
    const lobes = 2 + Math.floor(rnd() * 3);
    const island = R > 3.4 && rnd() < 0.5;
    const iR = island ? 0.9 + rnd() * (R - 2.6) * 0.5 : 0;
    for (let y = Math.floor(cy - R - 1); y <= Math.ceil(cy + R + 1); y++) {
      for (let x = Math.floor(cx - R - 1); x <= Math.ceil(cx + R + 1); x++) {
        const t = tiles[y]?.[x];
        if (!t) continue;
        const dx = x + 0.5 - cx, dy = y + 0.5 - cy;
        const d = Math.hypot(dx, dy);
        const rr = R * (0.78 + 0.22 * Math.sin(Math.atan2(dy, dx) * lobes + phase));
        if (d <= rr && d >= iR) t.terrain = 'water';
      }
    }
    return;
  }
}

export function generateMap(seed = 1961): MapData {
  const rnd = mulberry32(seed);
  const tiles: Tile[][] = [];
  for (let y = 0; y < MAP_H; y++) {
    const row: Tile[] = [];
    for (let x = 0; x < MAP_W; x++) {
      row.push({ terrain: 'grass', variant: rnd() });
    }
    tiles.push(row);
  }

  // --- Water: a river with a random course, plus 0-2 lakes ---
  const startX = Math.floor(MAP_W / 2);
  const startY = Math.floor(MAP_H / 2);
  carveRiver(tiles, rnd, startX, startY);
  const lakeCount = Math.floor(rnd() * 3);
  for (let i = 0; i < lakeCount; i++) carveLake(tiles, rnd, startX, startY);

  // --- Forest patches (blobby) ---
  const forestSpots = 16;
  for (let i = 0; i < forestSpots; i++) {
    const cx = 6 + Math.floor(rnd() * (MAP_W - 8));
    const cy = Math.floor(rnd() * MAP_H);
    const r = 2 + Math.floor(rnd() * 3);
    for (let y = cy - r; y <= cy + r; y++) {
      for (let x = cx - r; x <= cx + r; x++) {
        if (x < 0 || x >= MAP_W || y < 0 || y >= MAP_H) continue;
        const d = Math.hypot(x - cx, y - cy);
        if (d <= r && rnd() > 0.25 && tiles[y][x].terrain === 'grass') {
          tiles[y][x].terrain = 'forest';
        }
      }
    }
  }

  // --- Deposits ---
  const placeDeposits = (type: DepositType, count: number, cluster: number) => {
    let placed = 0, guard = 0;
    while (placed < count && guard++ < 500) {
      const cx = 8 + Math.floor(rnd() * (MAP_W - 10));
      const cy = 2 + Math.floor(rnd() * (MAP_H - 4));
      // keep distance from start center
      if (Math.hypot(cx - MAP_W / 2, cy - MAP_H / 2) < 6) continue;
      let ok = true;
      for (let y = cy - 3; y <= cy + 3 && ok; y++)
        for (let x = cx - 3; x <= cx + 3 && ok; x++)
          if (tiles[y]?.[x]?.deposit) ok = false;
      if (!ok) continue;
      for (let i = 0; i < cluster; i++) {
        const x = cx + Math.floor(rnd() * 3) - 1;
        const y = cy + Math.floor(rnd() * 3) - 1;
        const t = tiles[y]?.[x];
        if (t && t.terrain !== 'water') {
          t.deposit = type;
          if (type === 'gravel') t.terrain = 'rock';
          if (t.terrain === 'forest' && rnd() > 0.5) t.terrain = 'grass';
        }
      }
      placed++;
    }
  };
  placeDeposits('coal', 4, 4);
  placeDeposits('ironOre', 3, 4);
  placeDeposits('oil', 3, 3);
  placeDeposits('gravel', 4, 4);

  // --- Clear a starting area near center ---
  // The carvers are repelled from the base, so the inner hard-guarantee
  // (water -> grass within ±4) is a safety net that almost never fires.
  for (let y = startY - 6; y <= startY + 6; y++) {
    for (let x = startX - 6; x <= startX + 6; x++) {
      const t = tiles[y]?.[x];
      if (!t) continue;
      const inner = Math.abs(x - startX) <= 4 && Math.abs(y - startY) <= 4;
      if (t.terrain === 'water') {
        if (inner) t.terrain = 'grass';
        continue;
      }
      if (rnd() > 0.15) t.terrain = 'grass';
      t.deposit = undefined;
    }
  }

  return { tiles, startX, startY };
}
