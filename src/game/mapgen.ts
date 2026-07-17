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

  // --- Meandering river along the west side ---
  // The center follows a damped random walk (momentum, not per-row jitter)
  // and the width breathes between 2 and ~3.5 tiles. Consecutive rows are
  // forced to overlap orthogonally, so bends widen into smooth curves
  // instead of stepping diagonally.
  let center = 3 + rnd() * 2;
  let vel = 0;
  let width = 2 + rnd();
  let prevL = -1, prevR = -1;
  for (let y = 0; y < MAP_H; y++) {
    vel = Math.max(-0.8, Math.min(0.8, (vel + (rnd() - 0.5) * 0.5) * 0.85));
    center = Math.max(2, Math.min(6.5, center + vel));
    width = Math.max(2, Math.min(3.5, width + (rnd() - 0.5) * 0.3));
    let L = Math.round(center - width / 2);
    let R = Math.round(center + width / 2);
    if (prevL >= 0) {
      if (L > prevR) L = prevR; // keep the channel 4-connected row to row
      if (R < prevL) R = prevL;
    }
    for (let x = Math.max(0, L); x <= Math.min(MAP_W - 1, R); x++) tiles[y][x].terrain = 'water';
    prevL = L; prevR = R;
  }

  // --- Forest patches (blobby) ---
  const forestSpots = 16;
  for (let i = 0; i < forestSpots; i++) {
    const cx = 6 + Math.floor(rnd() * (MAP_W - 8));
    const cy = Math.floor(rnd() * MAP_H);
    const r = 2 + Math.floor(rnd() * 3);
    for (let y = cy - r; y <= cy + r; y++) {
      for (let x = cx - r; x <= cx + r; x++) {
        if (x < 5 || x >= MAP_W || y < 0 || y >= MAP_H) continue;
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
  const startX = Math.floor(MAP_W / 2);
  const startY = Math.floor(MAP_H / 2);
  for (let y = startY - 6; y <= startY + 6; y++) {
    for (let x = startX - 6; x <= startX + 6; x++) {
      const t = tiles[y]?.[x];
      if (t && t.terrain !== 'water') {
        if (rnd() > 0.15) t.terrain = 'grass';
        t.deposit = undefined;
      }
    }
  }

  return { tiles, startX, startY };
}
