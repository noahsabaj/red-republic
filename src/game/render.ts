// ============================================================
// Isometric canvas renderer
// ============================================================
import { BUILDINGS, RESOURCES } from './config';
import { MAP_W, MAP_H } from './mapgen';
import type { GameEngine, BuildingInst, Season, Truck } from './engine';

export const TILE_W = 64;
export const TILE_H = 32;

export interface Camera { x: number; y: number; z: number; }

export interface UIState {
  hoverTile: { x: number; y: number } | null;
  tool: { kind: 'select' } | { kind: 'build'; defId: string } | { kind: 'bulldoze' };
  selectedId: number | null;
  time: number; // ms, for animation
}

const GRASS: Record<Season, [string, string]> = {
  spring: ['#6ea851', '#79b25c'],
  summer: ['#5f9c49', '#69a553'],
  autumn: ['#93a04b', '#9faa55'],
  winter: ['#d8e2e9', '#e3ebf1'],
};
const WATER: Record<Season, string> = {
  spring: '#3f7fb8', summer: '#3f7fb8', autumn: '#3b74a8', winter: '#a8c8de',
};
const TREE: Record<Season, string> = {
  spring: '#3e7d3a', summer: '#2f6b31', autumn: '#c26a2a', winter: '#b9cdc9',
};

export function toScreen(cx: number, cy: number, cam: Camera) {
  return {
    x: (cx - cy) * (TILE_W / 2) * cam.z + cam.x,
    y: (cx + cy) * (TILE_H / 2) * cam.z + cam.y,
  };
}

export function screenToTile(sx: number, sy: number, cam: Camera) {
  const A = (sx - cam.x) / ((TILE_W / 2) * cam.z);
  const B = (sy - cam.y) / ((TILE_H / 2) * cam.z);
  return { x: Math.floor((A + B) / 2), y: Math.floor((B - A) / 2) };
}

function shade(hex: string, f: number): string {
  const n = parseInt(hex.slice(1), 16);
  const r = Math.min(255, Math.max(0, Math.round(((n >> 16) & 255) * f)));
  const g = Math.min(255, Math.max(0, Math.round(((n >> 8) & 255) * f)));
  const b = Math.min(255, Math.max(0, Math.round((n & 255) * f)));
  return `rgb(${r},${g},${b})`;
}

function poly(ctx: CanvasRenderingContext2D, pts: { x: number; y: number }[], fill: string, stroke?: string) {
  ctx.beginPath();
  ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
  ctx.closePath();
  ctx.fillStyle = fill;
  ctx.fill();
  if (stroke) { ctx.strokeStyle = stroke; ctx.lineWidth = 1; ctx.stroke(); }
}

export function buildingPolys(b: BuildingInst, cam: Camera) {
  const def = BUILDINGS[b.defId];
  const hPx = def.boxHeight * cam.z;
  const p00 = toScreen(b.x, b.y, cam);
  const p10 = toScreen(b.x + b.w, b.y, cam);
  const p01 = toScreen(b.x, b.y + b.h, cam);
  const p11 = toScreen(b.x + b.w, b.y + b.h, cam);
  const up = (p: { x: number; y: number }) => ({ x: p.x, y: p.y - hPx });
  return {
    ground: [p00, p10, p11, p01],
    top: [up(p00), up(p10), up(p11), up(p01)],
    left: [up(p01), up(p11), p11, p01],
    right: [up(p10), up(p11), p11, p10],
    hPx,
  };
}

/**
 * True isometric occlusion order for axis-aligned footprints (rects and
 * points; points have w = h = 0). Footprints never overlap, so any two are
 * separated on the x or the y axis: being east or south of something means
 * being in front of it. Returns < 0 when `a` draws first (is behind).
 * The final tie-break only fires for genuinely overlapping footprints
 * (e.g. a wandering citizen inside another building's footprint).
 */
export function isoCompare(
  a: { x: number; y: number; w: number; h: number },
  b: { x: number; y: number; w: number; h: number },
): number {
  if (a.x >= b.x + b.w) return 1;   // a entirely east of b → in front
  if (b.x >= a.x + a.w) return -1;
  if (a.y >= b.y + b.h) return 1;   // a entirely south of b → in front
  if (b.y >= a.y + a.h) return -1;
  return (a.x + a.y + a.w + a.h) - (b.x + b.y + b.w + b.h);
}

export function pickBuilding(engine: GameEngine, sx: number, sy: number, cam: Camera): BuildingInst | null {
  const list = [...engine.buildings.values()]
    .filter(b => BUILDINGS[b.defId].boxHeight > 0)
    .sort(isoCompare); // draw order — walk it back-to-front reversed
  for (let i = list.length - 1; i >= 0; i--) {
    const b = list[i];
    const ps = buildingPolys(b, cam);
    if (pointInPoly(sx, sy, ps.top) || pointInPoly(sx, sy, ps.left) || pointInPoly(sx, sy, ps.right)) return b;
  }
  return null;
}

function pointInPoly(x: number, y: number, pts: { x: number; y: number }[]) {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const xi = pts[i].x, yi = pts[i].y, xj = pts[j].x, yj = pts[j].y;
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

// ------------------------------------------------------------

interface DepthItem {
  x: number; y: number; w: number; h: number; // footprint (points: w = h = 0)
  kind: 'building' | 'trees' | 'truck' | 'citizen' | 'ghost';
  b?: BuildingInst;
  tr?: Truck;
  wx?: number; wy?: number;
  variant?: number;
}

// farm-field membership only changes when the world changes — cache per engine version
let fieldCache: { version: number; tiles: Set<number> } | null = null;

function fieldTilesOf(engine: GameEngine): Set<number> {
  if (fieldCache && fieldCache.version === engine.getVersion()) return fieldCache.tiles;
  const tiles = new Set<number>();
  for (const b of engine.buildings.values()) {
    const def = BUILDINGS[b.defId];
    if (!def.isFarm || !b.constructed) continue;
    for (let dy = -3; dy <= 3; dy++) for (let dx = -3; dx <= 3; dx++) {
      const tx = b.x + dx, ty = b.y + dy;
      const t = engine.tiles[ty]?.[tx];
      if (t && t.terrain === 'grass' && !t.buildingId && !t.road && !t.deposit) tiles.add(ty * MAP_W + tx);
    }
  }
  fieldCache = { version: engine.getVersion(), tiles };
  return tiles;
}

export function truckWorldPos(tr: Truck): { wx: number; wy: number } {
  const pts = tr.phase === 'go' ? tr.points : [...tr.points].reverse();
  const frac = Math.min(1, tr.daysDone / tr.daysTotal);
  const segs = pts.length - 1;
  if (segs <= 0) return { wx: pts[0]?.x ?? 0, wy: pts[0]?.y ?? 0 };
  const f = frac * segs;
  const i = Math.min(segs - 1, Math.floor(f));
  const t = f - i;
  return {
    wx: pts[i].x + (pts[i + 1].x - pts[i].x) * t,
    wy: pts[i].y + (pts[i + 1].y - pts[i].y) * t,
  };
}

export function render(ctx: CanvasRenderingContext2D, engine: GameEngine, cam: Camera, ui: UIState, vw: number, vh: number) {
  const season = engine.season();
  ctx.fillStyle = '#1a2028';
  ctx.fillRect(0, 0, vw, vh);

  const fieldTiles = fieldTilesOf(engine);

  const [g1, g2] = GRASS[season];
  // per-frame caches: shaded colors and font strings are invariant per frame
  const forest1 = shade(g1, 0.92), forest2 = shade(g2, 0.92);
  const frame: FrameStyle = {
    time: ui.time,
    fontIcon: `${Math.round(15 * cam.z)}px sans-serif`,
    fontStatus: `${Math.round(10 * cam.z)}px sans-serif`,
    fontSite: `bold ${Math.max(9, Math.round(10 * cam.z))}px sans-serif`,
    fontDeposit: `${Math.max(6, Math.round(9 * cam.z))}px sans-serif`,
    treeShade0: '', treeShade1: '',
  };
  const treeCol = TREE[season];
  frame.treeShade0 = shade(treeCol, 0.85);
  frame.treeShade1 = shade(treeCol, 1.0);

  // Everything raised above the ground plane draws in one depth-sorted pass
  // ordered by isoCompare — scan-row order is NOT the occlusion relation
  // (a 1x1 building east of a 2x2 sits in front of it despite an earlier row).
  const items: DepthItem[] = [];
  for (const tr of engine.trucks) {
    const p = truckWorldPos(tr);
    items.push({ x: p.wx, y: p.wy, w: 0, h: 0, kind: 'truck', tr, wx: p.wx, wy: p.wy });
  }
  if (engine.pop > 0 && cam.z >= 0.5) {
    for (const b of engine.buildings.values()) {
      const def = BUILDINGS[b.defId];
      if (!def.housingCapacity || !b.constructed) continue;
      const n = Math.min(3, Math.ceil(def.housingCapacity / 12));
      for (let i = 0; i < n; i++) {
        const ph = ui.time / 1400 + b.id * 1.7 + i * 2.1;
        const wx = b.x + b.w / 2 + Math.sin(ph) * (b.w / 2 + 0.6);
        const wy = b.y + b.h / 2 + Math.cos(ph * 0.8) * (b.h / 2 + 0.6);
        items.push({ x: wx, y: wy, w: 0, h: 0, kind: 'citizen', wx, wy });
      }
    }
  }
  if (ui.hoverTile && (ui.tool.kind === 'build' || ui.tool.kind === 'bulldoze')) {
    const [gw, gh] = ui.tool.kind === 'build' ? BUILDINGS[ui.tool.defId].size : [1, 1];
    items.push({ x: ui.hoverTile.x, y: ui.hoverTile.y, w: gw, h: gh, kind: 'ghost' });
  }

  const hwz = (TILE_W / 2) * cam.z;
  const hhz = (TILE_H / 2) * cam.z;
  for (let y = 0; y < MAP_H; y++) {
    for (let x = 0; x < MAP_W; x++) {
      // allocation-free viewport cull (corner extremes in closed form)
      const minX = (x - y - 1) * hwz + cam.x;
      if (minX > vw + 80 || minX + 2 * hwz < -80) continue;
      const minY = (x + y) * hhz + cam.y;
      if (minY > vh + 80 || minY + 2 * hhz < -120) continue;

      const t = engine.tiles[y][x];
      const c0 = toScreen(x, y, cam), c1 = toScreen(x + 1, y, cam);
      const c2 = toScreen(x + 1, y + 1, cam), c3 = toScreen(x, y + 1, cam);
      const pts = [c0, c1, c2, c3];

      let fill = (x + y) % 2 === 0 ? g1 : g2;
      if (t.terrain === 'water') fill = WATER[season];
      else if (t.terrain === 'rock') fill = (x + y) % 2 === 0 ? '#8b8b8b' : '#949494';
      else if (t.terrain === 'forest') fill = (x + y) % 2 === 0 ? forest1 : forest2;
      poly(ctx, pts, fill);

      // water shimmer
      if (t.terrain === 'water') {
        const wave = Math.sin(ui.time / 700 + x * 0.8 + y * 0.5) * 0.5 + 0.5;
        ctx.globalAlpha = 0.15 + wave * 0.1;
        poly(ctx, [lerpP(c0, c3, 0.3), lerpP(c1, c2, 0.3), lerpP(c1, c2, 0.45), lerpP(c0, c3, 0.45)], '#ffffff');
        ctx.globalAlpha = 1;
      }

      // farm fields
      if (fieldTiles.has(y * MAP_W + x)) {
        const fc = season === 'winter' ? '#cfd8de' : season === 'spring' ? '#8fbf5f' : season === 'summer' ? '#c9b545' : '#d9a83a';
        ctx.globalAlpha = 0.85;
        poly(ctx, [lerpP(c0, c2, 0.12), lerpP(c1, c3, 0.12), lerpP(c2, c0, 0.12), lerpP(c3, c1, 0.12)], fc);
        ctx.globalAlpha = 1;
        if (season === 'summer' || season === 'autumn') {
          ctx.strokeStyle = 'rgba(90,60,10,0.35)';
          ctx.lineWidth = 1;
          for (let i = 0.25; i < 1; i += 0.25) {
            ctx.beginPath();
            ctx.moveTo(lerpP(c0, c2, 0.12 * i + 0.1).x, lerpP(c0, c2, 0.12 * i + 0.1).y);
            ctx.lineTo(lerpP(c1, c3, 0.12 * i + 0.1).x, lerpP(c1, c3, 0.12 * i + 0.1).y);
            ctx.stroke();
          }
        }
      }

      // deposits
      if (t.deposit && t.terrain !== 'water') drawDeposit(ctx, t.deposit, c0, c1, c2, c3, t.variant, frame);

      // raised things at this tile join the depth pass (visible tiles only)
      if (t.terrain === 'forest') items.push({ x, y, w: 1, h: 1, kind: 'trees', variant: t.variant });
      if (t.buildingId) {
        const b = engine.buildings.get(t.buildingId);
        if (b && x === b.x + b.w - 1 && y === b.y + b.h - 1) {
          items.push({ x: b.x, y: b.y, w: b.w, h: b.h, kind: 'building', b });
        }
      }

      // road
      if (t.road) drawRoad(ctx, engine, x, y, c0, c1, c2, c3, cam);
    }
  }

  // depth pass: back to front
  items.sort(isoCompare);
  for (const it of items) {
    switch (it.kind) {
      case 'building':
        drawBuilding(ctx, it.b!, cam, frame);
        break;
      case 'trees':
        drawTrees(ctx, it.x, it.y, it.variant!, cam, frame);
        break;
      case 'truck':
        drawTruck(ctx, it.tr!, it.wx!, it.wy!, cam);
        break;
      case 'citizen': {
        const p = toScreen(it.wx!, it.wy!, cam);
        ctx.fillStyle = '#22303c';
        ctx.beginPath();
        ctx.arc(p.x, p.y - 2 * cam.z, 1.6 * cam.z, 0, Math.PI * 2);
        ctx.fill();
        break;
      }
      case 'ghost':
        drawGhost(ctx, engine, cam, ui);
        break;
    }
  }

  // selection highlight
  if (ui.selectedId) {
    const b = engine.buildings.get(ui.selectedId);
    if (b) {
      const c0 = toScreen(b.x, b.y, cam), c1 = toScreen(b.x + b.w, b.y, cam);
      const c2 = toScreen(b.x + b.w, b.y + b.h, cam), c3 = toScreen(b.x, b.y + b.h, cam);
      ctx.setLineDash([6, 4]);
      ctx.lineDashOffset = -ui.time / 60;
      ctx.strokeStyle = '#ffd94d';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(c0.x, c0.y); ctx.lineTo(c1.x, c1.y); ctx.lineTo(c2.x, c2.y); ctx.lineTo(c3.x, c3.y);
      ctx.closePath(); ctx.stroke();
      ctx.setLineDash([]);
    }
  }

}

// build/bulldoze placement ghost — drawn from within the row scan at its
// depth position (see ghostRow in render())
function drawGhost(ctx: CanvasRenderingContext2D, engine: GameEngine, cam: Camera, ui: UIState) {
  if (!ui.hoverTile) return;
  if (ui.tool.kind === 'build') {
    const defId = ui.tool.defId;
    const def = BUILDINGS[defId];
    const [w, h] = def.size;
    const chk = engine.canPlace(defId, ui.hoverTile.x, ui.hoverTile.y);
    const col = chk.ok ? 'rgba(80,255,120,0.5)' : 'rgba(255,70,70,0.5)';
    const c0 = toScreen(ui.hoverTile.x, ui.hoverTile.y, cam);
    const c1 = toScreen(ui.hoverTile.x + w, ui.hoverTile.y, cam);
    const c2 = toScreen(ui.hoverTile.x + w, ui.hoverTile.y + h, cam);
    const c3 = toScreen(ui.hoverTile.x, ui.hoverTile.y + h, cam);
    poly(ctx, [c0, c1, c2, c3], col, chk.ok ? '#4dff7a' : '#ff5050');
    if (chk.ok && defId !== 'road') {
      ctx.font = `${Math.round(20 * cam.z)}px sans-serif`;
      ctx.textAlign = 'center';
      ctx.globalAlpha = 0.9;
      ctx.fillText(def.icon, c2.x, c2.y - 18 * cam.z);
      ctx.globalAlpha = 1;
    }
  } else if (ui.tool.kind === 'bulldoze') {
    const c0 = toScreen(ui.hoverTile.x, ui.hoverTile.y, cam);
    const c1 = toScreen(ui.hoverTile.x + 1, ui.hoverTile.y, cam);
    const c2 = toScreen(ui.hoverTile.x + 1, ui.hoverTile.y + 1, cam);
    const c3 = toScreen(ui.hoverTile.x, ui.hoverTile.y + 1, cam);
    poly(ctx, [c0, c1, c2, c3], 'rgba(255,60,60,0.4)', '#ff4040');
  }
}

function lerpP(a: { x: number; y: number }, b: { x: number; y: number }, t: number) {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

// per-frame invariants (fonts scale with zoom; shades depend on season)
interface FrameStyle {
  time: number;
  fontIcon: string;
  fontStatus: string;
  fontSite: string;
  fontDeposit: string;
  treeShade0: string;
  treeShade1: string;
}

function drawDeposit(ctx: CanvasRenderingContext2D, kind: string, c0: Pt, c1: Pt, c2: Pt, c3: Pt, v: number, frame: FrameStyle) {
  const colors: Record<string, string> = { coal: '#26262a', ironOre: '#8a4b2f', oil: '#15181c', gravel: '#cfcfcf' };
  ctx.fillStyle = colors[kind] ?? '#000';
  const zoom = (c1.x - c0.x) / TILE_W * 2; // cam.z recovered from the corners
  const dotR = Math.max(1.2, 2.2 * zoom);
  for (let i = 0; i < 6; i++) {
    const fx = ((v * 97 + i * 0.37) % 0.7) + 0.15;
    const fy = ((v * 57 + i * 0.23) % 0.7) + 0.15;
    const p = lerpP(lerpP(c0, c1, fx), lerpP(c3, c2, fx), fy);
    ctx.beginPath();
    ctx.arc(p.x, p.y, dotR, 0, Math.PI * 2);
    ctx.fill();
  }
  // marker icon
  const mid = lerpP(c0, c2, 0.5);
  ctx.font = frame.fontDeposit;
  ctx.textAlign = 'center';
  ctx.globalAlpha = 0.85;
  ctx.fillStyle = '#fff';
  const label: Record<string, string> = { coal: 'COAL', ironOre: 'IRON', oil: 'OIL', gravel: 'GRAVEL' };
  ctx.fillText(label[kind] ?? '', mid.x, mid.y + 3);
  ctx.globalAlpha = 1;
}

type Pt = { x: number; y: number };

function drawTrees(ctx: CanvasRenderingContext2D, x: number, y: number, v: number, cam: Camera, frame: FrameStyle) {
  const n = 2 + Math.floor(v * 2);
  for (let i = 0; i < n; i++) {
    const fx = 0.2 + ((v * 31 + i * 0.4) % 0.6);
    const fy = 0.2 + ((v * 17 + i * 0.29) % 0.6);
    const p = toScreen(x + fx, y + fy, cam);
    const s = cam.z * (5 + ((v * 13 + i * 3) % 3));
    // trunk
    ctx.fillStyle = '#5b4028';
    ctx.fillRect(p.x - s * 0.12, p.y - s * 0.5, s * 0.24, s * 0.6);
    // canopy (two triangles)
    ctx.fillStyle = i % 2 === 0 ? frame.treeShade0 : frame.treeShade1;
    ctx.beginPath();
    ctx.moveTo(p.x, p.y - s * 2.2);
    ctx.lineTo(p.x - s * 0.9, p.y - s * 0.9);
    ctx.lineTo(p.x + s * 0.9, p.y - s * 0.9);
    ctx.closePath(); ctx.fill();
    ctx.beginPath();
    ctx.moveTo(p.x, p.y - s * 2.8);
    ctx.lineTo(p.x - s * 0.65, p.y - s * 1.7);
    ctx.lineTo(p.x + s * 0.65, p.y - s * 1.7);
    ctx.closePath(); ctx.fill();
  }
}

function drawRoad(ctx: CanvasRenderingContext2D, engine: GameEngine, x: number, y: number, c0: Pt, c1: Pt, c2: Pt, c3: Pt, cam: Camera) {
  poly(ctx, [c0, c1, c2, c3], '#585858');
  poly(ctx, [lerpP(c0, c2, 0.08), lerpP(c1, c3, 0.08), lerpP(c2, c0, 0.08), lerpP(c3, c1, 0.08)], '#6e6e6e');
  // center line toward connected neighbors
  const mid = lerpP(c0, c2, 0.5);
  ctx.strokeStyle = '#c9c25a';
  ctx.lineWidth = Math.max(1, 1.5 * cam.z);
  ctx.setLineDash([4 * cam.z, 4 * cam.z]);
  const dirs: [number, number][] = [[1, 0], [0, 1], [-1, 0], [0, -1]];
  for (const [dx, dy] of dirs) {
    if (engine.tiles[y + dy]?.[x + dx]?.road) {
      const edge = lerpP(dx === 1 ? c1 : dx === -1 ? c3 : dy === 1 ? c2 : c0, mid, 0.5);
      ctx.beginPath();
      ctx.moveTo(mid.x, mid.y);
      ctx.lineTo(edge.x, edge.y);
      ctx.stroke();
    }
  }
  ctx.setLineDash([]);
}

const CHIMNEY_DEFS = new Set(['powerPlant', 'steelMill', 'refinery', 'heatingPlant', 'brickworks']);

function drawBuilding(ctx: CanvasRenderingContext2D, b: BuildingInst, cam: Camera, frame: FrameStyle) {
  const def = BUILDINGS[b.defId];
  const ps = buildingPolys(b, cam);

  if (!b.constructed) {
    // construction site: low wooden frame + hatch + progress
    const pct = Math.round((b.progress / def.labor) * 100);
    poly(ctx, ps.ground, 'rgba(120,90,50,0.5)');
    const hPx = Math.max(4, ps.hPx * (b.progress / def.labor));
    const lift = ps.hPx - hPx;
    const shift = (p: Pt) => ({ x: p.x, y: p.y - lift });
    poly(ctx, [shift(ps.left[0]), shift(ps.left[1]), ps.left[2], ps.left[3]], '#a5804e');
    poly(ctx, [shift(ps.right[0]), shift(ps.right[1]), ps.right[2], ps.right[3]], '#8a6a3e');
    poly(ctx, [shift(ps.top[0]), shift(ps.top[1]), shift(ps.top[2]), shift(ps.top[3])], '#c19a63', '#7a5a30');
    ctx.font = frame.fontSite;
    ctx.textAlign = 'center';
    ctx.fillStyle = '#fff';
    ctx.strokeStyle = 'rgba(0,0,0,0.7)';
    ctx.lineWidth = 3;
    const mid = lerpP(ps.top[0], ps.top[2], 0.5);
    ctx.strokeText(`${def.icon} ${pct}%`, mid.x, mid.y - 6 * cam.z);
    ctx.fillText(`${def.icon} ${pct}%`, mid.x, mid.y - 6 * cam.z);
    return;
  }

  // walls & roof
  const unpowered = def.power > 0 && !b.powered;
  const wallBase = def.wallColor;
  poly(ctx, ps.left, shade(wallBase, 0.8));
  poly(ctx, ps.right, shade(wallBase, 0.6));
  poly(ctx, ps.top, def.color, shade(def.color, 0.6));

  // windows on left face
  if (ps.hPx > 14 * cam.z && cam.z > 0.55) {
    const rows = Math.min(3, Math.floor(ps.hPx / (12 * cam.z)));
    ctx.fillStyle = 'rgba(255,255,220,0.75)';
    for (let r = 0; r < rows; r++) {
      for (let i = 0.25; i < 1; i += 0.3) {
        const base = lerpP(ps.left[0], ps.left[1], i);
        const p = { x: base.x, y: base.y + (r + 0.6) * (ps.hPx / (rows + 0.5)) };
        ctx.fillRect(p.x - 1.5 * cam.z, p.y, 3 * cam.z, 3.5 * cam.z);
      }
    }
  }

  // chimneys + smoke
  if (CHIMNEY_DEFS.has(b.defId)) {
    const t1 = lerpP(ps.top[0], ps.top[2], 0.3);
    const t2 = lerpP(ps.top[0], ps.top[2], 0.6);
    for (const [i, tp] of [t1, t2].entries()) {
      const cw = 4 * cam.z, ch = 14 * cam.z;
      ctx.fillStyle = '#7a3b2a';
      ctx.fillRect(tp.x - cw / 2, tp.y - ch, cw, ch);
      ctx.fillStyle = '#5a2b1e';
      ctx.fillRect(tp.x - cw / 2 - 1, tp.y - ch - 2, cw + 2, 3);
      // smoke while the plant is actually running
      if (b.eff > 0.05) {
        for (let s = 0; s < 3; s++) {
          const ph = ((frame.time / 1800 + s * 0.33 + i * 0.5 + b.id * 0.17) % 1);
          ctx.globalAlpha = 0.28 * (1 - ph);
          ctx.fillStyle = '#cccccc';
          ctx.beginPath();
          ctx.arc(tp.x + Math.sin(ph * 5 + b.id) * 4 * cam.z, tp.y - ch - ph * 26 * cam.z, (2.5 + ph * 4) * cam.z, 0, Math.PI * 2);
          ctx.fill();
          ctx.globalAlpha = 1;
        }
      }
    }
  }

  // icon
  const mid = lerpP(ps.top[0], ps.top[2], 0.5);
  ctx.font = frame.fontIcon;
  ctx.textAlign = 'center';
  ctx.fillText(def.icon, mid.x, mid.y + 5 * cam.z);

  // status icons
  let sx = mid.x;
  const sy = mid.y - 14 * cam.z;
  ctx.font = frame.fontStatus;
  if (unpowered) { ctx.fillText('⚡', sx, sy); sx += 12 * cam.z; }
  if (!b.connected) { ctx.fillText('🚫', sx, sy); sx += 12 * cam.z; }
  if (def.heat > 0 && !b.heated) { ctx.fillText('🥶', sx, sy); }
}

function drawTruck(ctx: CanvasRenderingContext2D, tr: Truck, wx: number, wy: number, cam: Camera) {
  const p = toScreen(wx, wy, cam);
  const s = cam.z;
  // body
  ctx.fillStyle = '#2f3844';
  ctx.fillRect(p.x - 5 * s, p.y - 9 * s, 10 * s, 7 * s);
  // cargo dot
  ctx.fillStyle = RESOURCES[tr.cargo].color;
  ctx.fillRect(p.x - 3 * s, p.y - 12 * s, 6 * s, 4 * s);
  // cab light
  ctx.fillStyle = '#e8e04a';
  ctx.fillRect(p.x + 3 * s, p.y - 8 * s, 2 * s, 2 * s);
}
