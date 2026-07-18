// ============================================================
// Isometric canvas renderer
// ============================================================
import { BALANCE, BUILDINGS, RESOURCES } from './config';
import { MAP_W, MAP_H } from './mapgen';
import { drawIcon } from '@/ui/icons';
import type { GameEngine, BuildingInst, Season, Truck } from './engine';
import type { WeatherCondition } from './weather';

export const TILE_W = 64;
export const TILE_H = 32;

import type { SelectionItem } from './selection';

export interface Camera { x: number; y: number; z: number; }

export interface UIState {
  hoverTile: { x: number; y: number } | null;
  tool: { kind: 'select' } | { kind: 'build'; defId: string } | { kind: 'bulldoze' };
  selection: SelectionItem[];
  time: number; // ms, for animation
}

// Bare-season bases: winter is dormant, not white — snow cover (a sim value,
// engine.weather.snowDepth) whitens the world gradually via tint().
const GRASS: Record<Season, [string, string]> = {
  spring: ['#6ea851', '#79b25c'],
  summer: ['#5f9c49', '#69a553'],
  autumn: ['#93a04b', '#9faa55'],
  winter: ['#8d9377', '#979d80'],
};
const WATER: Record<Season, string> = {
  spring: '#3f7fb8', summer: '#3f7fb8', autumn: '#3b74a8', winter: '#36648e',
};
const ICE = '#a8c8de'; // shown when engine.weather.riverFrozen — the state barges obey
const TREE: Record<Season, string> = {
  spring: '#3e7d3a', summer: '#2f6b31', autumn: '#c26a2a', winter: '#40634c',
};
const FIELD: Record<Season, string> = {
  spring: '#8fbf5f', summer: '#c9b545', autumn: '#d9a83a', winter: '#a89a72',
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

// accepts '#rrggbb' or 'rgb(r,g,b)' so shade/tint chain (e.g. snow over shade)
function chan(c: string): [number, number, number] {
  if (c.startsWith('#')) {
    const n = parseInt(c.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const m = c.match(/\d+/g);
  return m ? [+m[0], +m[1], +m[2]] : [0, 0, 0];
}

function shade(col: string, f: number): string {
  const [r, g, b] = chan(col);
  return `rgb(${Math.min(255, Math.round(r * f))},${Math.min(255, Math.round(g * f))},${Math.min(255, Math.round(b * f))})`;
}

function tint(col: string, f: number): string {
  const [r, g, b] = chan(col);
  return `rgb(${Math.round(r + (255 - r) * f)},${Math.round(g + (255 - g) * f)},${Math.round(b + (255 - b) * f)})`;
}

/** Desaturate + dim — foreign soil beyond the border reads as not-yours. */
function dull(col: string, f: number): string {
  const [r, g, b] = chan(col);
  const l = 0.3 * r + 0.59 * g + 0.11 * b;
  const mix = (c: number) => Math.round((c + (l - c) * f) * 0.9);
  return `rgb(${mix(r)},${mix(g)},${mix(b)})`;
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
  kind: 'building' | 'trees' | 'truck' | 'boat' | 'citizen' | 'ghost' | 'borderpost';
  b?: BuildingInst;
  tr?: Truck;
  wx?: number; wy?: number;
  variant?: number;
  foreign?: boolean;
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

  // per-frame caches: shaded colors and font strings are invariant per frame.
  // Snow cover and river ice come from the SIMULATION (engine.weather), so
  // what you see is literally the state the farms and barges obey.
  const snowT = Math.min(1, engine.weather.snowDepth / 6) * 0.85;
  const frozen = engine.weather.riverFrozen;
  const [g1, g2] = GRASS[season].map(c => tint(c, snowT));
  const forest1 = shade(g1, 0.92), forest2 = shade(g2, 0.92);
  const rock1 = tint('#8b8b8b', snowT), rock2 = tint('#949494', snowT);
  const wBase = frozen ? tint(ICE, snowT * 0.4) : WATER[season];
  const frame: FrameStyle = {
    time: ui.time,
    season,
    snowT,
    fontSite: `bold ${Math.max(9, Math.round(10 * cam.z))}px sans-serif`,
    fontDeposit: `${Math.max(6, Math.round(9 * cam.z))}px sans-serif`,
    treeShade0: '', treeShade1: '',
    foreign: { g1: '', g2: '', f1: '', f2: '', r1: '', r2: '', tree0: '', tree1: '' },
    fieldCol: tint(FIELD[season], snowT),
    waterA: wBase,
    waterB: shade(wBase, 0.94),
    waterDeepA: shade(wBase, frozen ? 0.96 : 0.86),
    waterDeepB: shade(wBase, frozen ? 0.93 : 0.81),
    waterEdge: tint(wBase, 0.5),
  };
  const treeCol = tint(TREE[season], snowT * 0.9);
  frame.treeShade0 = shade(treeCol, 0.85);
  frame.treeShade1 = shade(treeCol, 1.0);
  frame.foreign = {
    g1: dull(g1, 0.5), g2: dull(g2, 0.5),
    f1: dull(forest1, 0.5), f2: dull(forest2, 0.5),
    r1: dull(rock1, 0.5), r2: dull(rock2, 0.5),
    tree0: dull(frame.treeShade0, 0.5), tree1: dull(frame.treeShade1, 0.5),
  };

  // Everything raised above the ground plane draws in one depth-sorted pass
  // ordered by isoCompare — scan-row order is NOT the occlusion relation
  // (a 1x1 building east of a 2x2 sits in front of it despite an earlier row).
  const items: DepthItem[] = [];
  for (const tr of engine.trucks) {
    const p = truckWorldPos(tr);
    items.push({ x: p.wx, y: p.wy, w: 0, h: 0, kind: 'truck', tr, wx: p.wx, wy: p.wy });
  }
  for (const tr of engine.foreignTrucks) {
    const p = truckWorldPos(tr);
    items.push({ x: p.wx, y: p.wy, w: 0, h: 0, kind: 'truck', tr, wx: p.wx, wy: p.wy, foreign: true });
  }
  for (const bt of engine.boats) {
    const p = truckWorldPos(bt);
    items.push({ x: p.wx, y: p.wy, w: 0, h: 0, kind: 'boat', tr: bt, wx: p.wx, wy: p.wy });
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
  for (const bp of borderPosts(engine)) {
    items.push({ x: bp.x, y: bp.y, w: 0, h: 0, kind: 'borderpost', wx: bp.x, wy: bp.y });
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

      if (t.terrain === 'water') {
        drawWater(ctx, engine, x, y, c0, c1, c2, c3, cam, frame);
      } else {
        const even = (x + y) % 2 === 0;
        let fill: string;
        if (t.foreign) {
          const fs = frame.foreign;
          fill = t.terrain === 'rock' ? (even ? fs.r1 : fs.r2)
            : t.terrain === 'forest' ? (even ? fs.f1 : fs.f2)
            : (even ? fs.g1 : fs.g2);
        } else {
          fill = even ? g1 : g2;
          if (t.terrain === 'rock') fill = even ? rock1 : rock2;
          else if (t.terrain === 'forest') fill = even ? forest1 : forest2;
        }
        poly(ctx, [c0, c1, c2, c3], fill);
      }

      // farm fields
      if (fieldTiles.has(y * MAP_W + x)) {
        ctx.globalAlpha = 0.85;
        poly(ctx, [lerpP(c0, c2, 0.12), lerpP(c1, c3, 0.12), lerpP(c2, c0, 0.12), lerpP(c3, c1, 0.12)], frame.fieldCol);
        ctx.globalAlpha = 1;
        if ((season === 'summer' || season === 'autumn') && frame.snowT < 0.3) {
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
      if (t.terrain === 'forest') items.push({ x, y, w: 1, h: 1, kind: 'trees', variant: t.variant, foreign: t.foreign });
      if (t.buildingId) {
        const b = engine.buildings.get(t.buildingId);
        if (b && x === b.x + b.w - 1 && y === b.y + b.h - 1) {
          items.push({ x: b.x, y: b.y, w: b.w, h: b.h, kind: 'building', b });
        }
      }

      // road
      if (t.road) drawRoad(ctx, engine, x, y, c0, c1, c2, c3, cam);

      // the national border: a striped line along the homeland side of the strip
      // (drawn over roads too — at the crossing it reads as the checkpoint bar)
      if (!t.foreign) {
        const segs: [Pt, Pt][] = [];
        if (engine.tiles[y]?.[x - 1]?.foreign) segs.push([c0, c3]);
        if (engine.tiles[y]?.[x + 1]?.foreign) segs.push([c1, c2]);
        if (engine.tiles[y - 1]?.[x]?.foreign) segs.push([c0, c1]);
        if (engine.tiles[y + 1]?.[x]?.foreign) segs.push([c3, c2]);
        if (segs.length) {
          ctx.lineWidth = Math.max(1.5, 2.2 * cam.z);
          ctx.strokeStyle = 'rgba(140,34,26,0.85)';
          ctx.beginPath();
          for (const [a, b] of segs) { ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); }
          ctx.stroke();
          ctx.lineWidth = Math.max(1, 1.1 * cam.z);
          ctx.strokeStyle = 'rgba(232,226,212,0.7)';
          ctx.setLineDash([3 * cam.z, 5 * cam.z]);
          ctx.beginPath();
          for (const [a, b] of segs) { ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); }
          ctx.stroke();
          ctx.setLineDash([]);
        }
      }
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
        drawTrees(ctx, it.x, it.y, it.variant!, cam, frame, it.foreign);
        break;
      case 'borderpost':
        drawBorderPost(ctx, it.wx!, it.wy!, cam);
        break;
      case 'truck':
        drawTruck(ctx, it.tr!, it.wx!, it.wy!, cam, it.foreign);
        break;
      case 'boat':
        drawBoat(ctx, it.tr!, it.wx!, it.wy!, cam, ui.time);
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

  // weather atmosphere — lighting scrim, precipitation, lightning, fog —
  // over the world, under the selection UI
  drawWeather(ctx, engine.weather.condition, ui.time, vw, vh);

  // selection highlights (deliberate overlay — dashed outlines read as UI)
  if (ui.selection.length) {
    ctx.setLineDash([6, 4]);
    ctx.lineDashOffset = -ui.time / 60;
    ctx.strokeStyle = '#ffd94d';
    ctx.lineWidth = 2;
    for (const sel of ui.selection) {
      let rect: { x: number; y: number; w: number; h: number } | null = null;
      if (sel.kind === 'building') {
        const b = engine.buildings.get(sel.id);
        if (b) rect = b;
      } else {
        rect = { x: sel.x, y: sel.y, w: 1, h: 1 };
      }
      if (!rect) continue;
      const c0 = toScreen(rect.x, rect.y, cam), c1 = toScreen(rect.x + rect.w, rect.y, cam);
      const c2 = toScreen(rect.x + rect.w, rect.y + rect.h, cam), c3 = toScreen(rect.x, rect.y + rect.h, cam);
      ctx.beginPath();
      ctx.moveTo(c0.x, c0.y); ctx.lineTo(c1.x, c1.y); ctx.lineTo(c2.x, c2.y); ctx.lineTo(c3.x, c3.y);
      ctx.closePath(); ctx.stroke();
    }
    ctx.setLineDash([]);
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
      ctx.globalAlpha = 0.9;
      drawIcon(ctx, def.icon, c2.x, c2.y - 18 * cam.z, 18 * cam.z);
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

function drawWater(ctx: CanvasRenderingContext2D, engine: GameEngine, x: number, y: number, c0: Pt, c1: Pt, c2: Pt, c3: Pt, cam: Camera, frame: FrameStyle) {
  const isWater = (tx: number, ty: number) => engine.tiles[ty]?.[tx]?.terrain === 'water';
  const nN = isWater(x, y - 1), nE = isWater(x + 1, y), nS = isWater(x, y + 1), nW = isWater(x - 1, y);
  const even = (x + y) % 2 === 0;
  const deep = nN && nE && nS && nW; // fully surrounded reads as depth
  poly(ctx, [c0, c1, c2, c3], deep ? (even ? frame.waterDeepA : frame.waterDeepB) : (even ? frame.waterA : frame.waterB));

  // shallow bank + foam line along every edge that touches land — this is
  // what visually rounds the tile staircase into a shoreline
  const bank = (a: Pt, b: Pt, aIn: Pt, bIn: Pt) => {
    ctx.globalAlpha = 0.3;
    poly(ctx, [a, b, lerpP(b, bIn, 0.26), lerpP(a, aIn, 0.26)], frame.waterEdge);
    ctx.globalAlpha = 0.55;
    ctx.strokeStyle = frame.waterEdge;
    ctx.lineWidth = Math.max(1, 1.2 * cam.z);
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
    ctx.globalAlpha = 1;
  };
  if (!nN) bank(c0, c1, c3, c2);
  if (!nE) bank(c1, c2, c0, c3);
  if (!nS) bank(c3, c2, c0, c1);
  if (!nW) bank(c0, c3, c1, c2);

  // calm glints that breathe in and out (a frozen river lies still)
  if (!engine.weather.riverFrozen) {
    const v = engine.tiles[y][x].variant;
    for (let i = 0; i < 2; i++) {
      const fx = 0.18 + ((v * 37 + i * 0.43) % 0.5);
      const fy = 0.18 + ((v * 53 + i * 0.31) % 0.5);
      const ph = (frame.time / 1700 + v * 9 + i * 0.5) % 1;
      ctx.globalAlpha = 0.04 + 0.08 * (0.5 + 0.5 * Math.sin(ph * Math.PI * 2));
      const top = lerpP(c0, c1, fx), bot = lerpP(c3, c2, fx);
      const p = lerpP(top, bot, fy);
      ctx.strokeStyle = '#eaf6ff';
      ctx.lineWidth = Math.max(1, 1.2 * cam.z);
      ctx.beginPath();
      ctx.moveTo(p.x, p.y);
      ctx.lineTo(p.x + (c1.x - c0.x) * 0.16, p.y + (c1.y - c0.y) * 0.16);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }
}

// ------------------------------------------------------------
// Weather atmosphere: everything is a pure function of (time, index) like
// the chimney smoke — no particle state to carry between frames.
// ------------------------------------------------------------

const fract = (n: number) => n - Math.floor(n);
/** Deterministic 0..1 hash — stable per particle index across frames. */
export const hash01 = (i: number) => fract(Math.sin(i * 127.1 + 311.7) * 43758.5453);

/** Screen position of precipitation particle `i` at time `time` (ms). Pure. */
export function precipParticle(
  i: number, time: number, vw: number, vh: number,
  kind: 'rain' | 'snow', slant: number, speed: number,
): { x: number; y: number } {
  const span = vh + 40;
  const y = fract((time / 1000) * speed / span + hash01(i + 7919)) * span - 20;
  const sway = kind === 'snow' ? Math.sin(time / 900 + i * 1.7) * 6 : 0;
  const raw = hash01(i) * (vw + 200) + slant * y + sway;
  return { x: fract(raw / (vw + 200)) * (vw + 200) - 100, y };
}

// full-viewport light scrim per condition (uniform: buildings, roads, water)
const SCRIM: Partial<Record<WeatherCondition, string>> = {
  overcast: 'rgba(62,68,82,0.15)',
  rain: 'rgba(36,46,62,0.18)',
  storm: 'rgba(28,36,54,0.30)',
  snow: 'rgba(200,208,222,0.12)',
  blizzard: 'rgba(198,206,220,0.26)',
  fog: 'rgba(158,170,184,0.34)',
};

function drawWeather(ctx: CanvasRenderingContext2D, cond: WeatherCondition, time: number, vw: number, vh: number) {
  if (cond === 'clear') return;
  const scrim = SCRIM[cond];
  if (scrim) { ctx.fillStyle = scrim; ctx.fillRect(0, 0, vw, vh); }

  if (cond === 'fog') {
    // soft banks of fog drifting slowly down the view
    for (let i = 0; i < 3; i++) {
      const bandH = vh * 0.16;
      const y = fract(time / (26000 + i * 9000) + i * 0.37) * (vh + bandH) - bandH / 2;
      const gr = ctx.createLinearGradient(0, y, 0, y + bandH);
      gr.addColorStop(0, 'rgba(190,200,212,0)');
      gr.addColorStop(0.5, 'rgba(190,200,212,0.22)');
      gr.addColorStop(1, 'rgba(190,200,212,0)');
      ctx.fillStyle = gr;
      ctx.fillRect(0, y, vw, bandH);
    }
    return;
  }

  const snowfall = cond === 'snow' || cond === 'blizzard';
  if (snowfall || cond === 'rain' || cond === 'storm') {
    const n = cond === 'blizzard' ? 220 : cond === 'storm' ? 170 : cond === 'rain' ? 130 : 110;
    const speed = cond === 'blizzard' ? 300 : snowfall ? 70 : cond === 'storm' ? 950 : 640;
    const slant = cond === 'blizzard' ? 0.75 : cond === 'storm' ? 0.5 : snowfall ? 0.08 : 0.14;
    if (snowfall) {
      ctx.fillStyle = 'rgba(240,246,252,0.85)';
      for (let i = 0; i < n; i++) {
        const p = precipParticle(i, time, vw, vh, 'snow', slant, speed);
        ctx.globalAlpha = 0.35 + hash01(i + 57) * 0.45;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 1 + hash01(i + 31) * 1.4, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalAlpha = 1;
    } else {
      ctx.strokeStyle = 'rgba(180,205,230,0.5)';
      ctx.lineWidth = 1;
      const len = cond === 'storm' ? 14 : 10;
      ctx.beginPath();
      for (let i = 0; i < n; i++) {
        const p = precipParticle(i, time, vw, vh, 'rain', slant, speed);
        ctx.moveTo(p.x, p.y);
        ctx.lineTo(p.x - slant * len, p.y - len);
      }
      ctx.stroke();
    }
  }

  // lightning: rare deterministic double-strike lighting the whole scene
  if (cond === 'storm') {
    const cell = Math.floor(time / 700);
    if (hash01(cell) < 0.07) {
      const ph = (time % 700) / 700;
      if (ph < 0.24) {
        const flicker = ph < 0.08 ? 1 : ph < 0.12 ? 0.25 : ph < 0.2 ? 0.7 : 0.3;
        ctx.fillStyle = `rgba(235,241,255,${(0.5 * (1 - ph / 0.24) * flicker).toFixed(3)})`;
        ctx.fillRect(0, 0, vw, vh);
      }
    }
  }
}

// per-frame invariants (fonts scale with zoom; shades depend on season & weather)
interface FrameStyle {
  time: number;
  season: Season;
  snowT: number; // 0..0.85 whitening from simulated snow depth
  fontSite: string;
  fontDeposit: string;
  treeShade0: string;
  treeShade1: string;
  fieldCol: string;
  waterA: string;
  waterB: string;
  waterDeepA: string;
  waterDeepB: string;
  waterEdge: string;
  foreign: { g1: string; g2: string; f1: string; f2: string; r1: string; r2: string; tree0: string; tree1: string };
}

/**
 * Fence posts along the national border, every other tile corner. Gaps open
 * where the crossing lane or the river passes. World-corner coordinates.
 */
function borderPosts(engine: GameEngine): Pt[] {
  const edge = engine.borderEdge;
  if (!edge) return [];
  const D = BALANCE.borderDepth;
  const posts: Pt[] = [];
  const alongMax = edge === 'N' || edge === 'S' ? MAP_W : MAP_H;
  for (let u = 0; u <= alongMax; u += 2) {
    // the two foreign-side tiles flanking this corner
    const f1 = edge === 'W' ? { x: D - 1, y: u } : edge === 'E' ? { x: MAP_W - D, y: u }
      : edge === 'N' ? { x: u, y: D - 1 } : { x: u, y: MAP_H - D };
    const f2 = edge === 'N' || edge === 'S' ? { x: f1.x - 1, y: f1.y } : { x: f1.x, y: f1.y - 1 };
    const t1 = engine.tiles[f1.y]?.[f1.x], t2 = engine.tiles[f2.y]?.[f2.x];
    if (t1 && (t1.road || t1.terrain === 'water')) continue;
    if (t2 && (t2.road || t2.terrain === 'water')) continue;
    posts.push(edge === 'W' ? { x: D, y: u } : edge === 'E' ? { x: MAP_W - D, y: u }
      : edge === 'N' ? { x: u, y: D } : { x: u, y: MAP_H - D });
  }
  return posts;
}

function drawBorderPost(ctx: CanvasRenderingContext2D, wx: number, wy: number, cam: Camera) {
  const p = toScreen(wx, wy, cam);
  const s = cam.z;
  const h = 13 * s;
  const w = Math.max(1.5, 2 * s);
  const stripes = ['#c8382e', '#e8e2d4', '#c8382e', '#e8e2d4'];
  const seg = h / stripes.length;
  for (let i = 0; i < stripes.length; i++) {
    ctx.fillStyle = stripes[i];
    ctx.fillRect(p.x - w / 2, p.y - h + i * seg, w, seg + 0.5);
  }
  ctx.fillStyle = '#8a1f18';
  ctx.fillRect(p.x - w * 0.9, p.y - h - 1.8 * s, w * 1.8, 1.8 * s);
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

function drawTrees(ctx: CanvasRenderingContext2D, x: number, y: number, v: number, cam: Camera, frame: FrameStyle, foreign?: boolean) {
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
    ctx.fillStyle = foreign
      ? (i % 2 === 0 ? frame.foreign.tree0 : frame.foreign.tree1)
      : (i % 2 === 0 ? frame.treeShade0 : frame.treeShade1);
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
  const road = (tx: number, ty: number) => !!engine.tiles[ty]?.[tx]?.road;
  const n = road(x, y - 1), e = road(x + 1, y), s = road(x, y + 1), w = road(x - 1, y);
  const bridge = engine.tiles[y][x].terrain === 'water';

  const edgeBand = (a: Pt, b: Pt, aIn: Pt, bIn: Pt, fill: string, line: string, inset: number) => {
    poly(ctx, [a, b, lerpP(b, bIn, inset), lerpP(a, aIn, inset)], fill);
    ctx.strokeStyle = line;
    ctx.lineWidth = Math.max(1, 1.2 * cam.z);
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
  };

  if (bridge) {
    // timber deck over the water, pilings under the front edge
    poly(ctx, [c0, c1, c2, c3], '#8a6f4d');
    ctx.fillStyle = '#4a3821';
    for (const f of [0.25, 0.75]) {
      const p = lerpP(c3, c2, f);
      ctx.fillRect(p.x - 1.5 * cam.z, p.y, 3 * cam.z, 4.5 * cam.z);
    }
    // rails along the sides that don't continue the road
    if (!n) edgeBand(c0, c1, c3, c2, 'rgba(74,56,33,0.35)', '#4a3821', 0.1);
    if (!e) edgeBand(c1, c2, c0, c3, 'rgba(74,56,33,0.35)', '#4a3821', 0.1);
    if (!s) edgeBand(c3, c2, c0, c1, 'rgba(74,56,33,0.35)', '#4a3821', 0.1);
    if (!w) edgeBand(c0, c3, c1, c2, 'rgba(74,56,33,0.35)', '#4a3821', 0.1);
  } else {
    // seamless asphalt: full-tile surface, shoulders ONLY where the road ends
    poly(ctx, [c0, c1, c2, c3], '#6e6e6e');
    if (!n) edgeBand(c0, c1, c3, c2, '#585858', '#4c4c4c', 0.14);
    if (!e) edgeBand(c1, c2, c0, c3, '#585858', '#4c4c4c', 0.14);
    if (!s) edgeBand(c3, c2, c0, c1, '#585858', '#4c4c4c', 0.14);
    if (!w) edgeBand(c0, c3, c1, c2, '#585858', '#4c4c4c', 0.14);
  }

  // center markings run along the travel direction: through shared-edge
  // MIDPOINTS (anchoring at corners pointed them screen-vertical/horizontal)
  const mid = lerpP(c0, c2, 0.5);
  const em = {
    n: lerpP(c0, c1, 0.5), // edge shared with (x, y-1)
    e: lerpP(c1, c2, 0.5),
    s: lerpP(c3, c2, 0.5),
    w: lerpP(c0, c3, 0.5),
  };
  ctx.strokeStyle = '#c9c25a';
  ctx.lineWidth = Math.max(1, 1.5 * cam.z);
  ctx.setLineDash([4 * cam.z, 4 * cam.z]);
  ctx.beginPath();
  const seg = (a: Pt, b: Pt) => { ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); };
  if (n && s && !e && !w) seg(em.n, em.s);        // straight through
  else if (e && w && !n && !s) seg(em.e, em.w);
  else {
    if (n) seg(mid, em.n);
    if (e) seg(mid, em.e);
    if (s) seg(mid, em.s);
    if (w) seg(mid, em.w);
  }
  ctx.stroke();
  ctx.setLineDash([]);
}

function drawBoat(ctx: CanvasRenderingContext2D, boat: Truck, wx: number, wy: number, cam: Camera, time: number) {
  const p = toScreen(wx, wy, cam);
  const s = cam.z;
  const y = p.y + Math.sin(time / 600 + boat.id * 1.3) * 0.8 * s; // gentle bob
  // wake
  ctx.strokeStyle = 'rgba(255,255,255,0.25)';
  ctx.lineWidth = Math.max(1, 1.2 * s);
  ctx.beginPath();
  ctx.moveTo(p.x - 11 * s, y + 1.5 * s);
  ctx.lineTo(p.x + 11 * s, y + 1.5 * s);
  ctx.stroke();
  // hull
  ctx.fillStyle = '#54432f';
  ctx.beginPath();
  ctx.moveTo(p.x - 9 * s, y - 4 * s);
  ctx.lineTo(p.x + 9 * s, y - 4 * s);
  ctx.lineTo(p.x + 6 * s, y);
  ctx.lineTo(p.x - 6 * s, y);
  ctx.closePath();
  ctx.fill();
  // cargo
  ctx.fillStyle = RESOURCES[boat.cargo].color;
  ctx.fillRect(p.x - 5 * s, y - 8.5 * s, 10 * s, 4.5 * s);
  // wheelhouse
  ctx.fillStyle = '#d8d2c2';
  ctx.fillRect(p.x + 5.5 * s, y - 7.5 * s, 2.5 * s, 3.5 * s);
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
    const mid = lerpP(ps.top[0], ps.top[2], 0.5);
    const ty = mid.y - 6 * cam.z;
    drawIcon(ctx, def.icon, mid.x - 12 * cam.z, ty - 3.5 * cam.z, 11 * cam.z);
    ctx.font = frame.fontSite;
    ctx.textAlign = 'left';
    ctx.fillStyle = '#fff';
    ctx.strokeStyle = 'rgba(0,0,0,0.7)';
    ctx.lineWidth = 3;
    ctx.strokeText(`${pct}%`, mid.x - 4 * cam.z, ty);
    ctx.fillText(`${pct}%`, mid.x - 4 * cam.z, ty);
    ctx.textAlign = 'center';
    return;
  }

  // walls & roof
  const unpowered = def.power > 0 && !b.powered;
  const wallBase = def.wallColor;
  poly(ctx, ps.left, shade(wallBase, 0.8));
  poly(ctx, ps.right, shade(wallBase, 0.6));
  // roofs carry the simulated snow cover
  const roof = frame.snowT > 0.05 ? tint(def.color, frame.snowT * 0.7) : def.color;
  poly(ctx, ps.top, roof, shade(def.color, 0.6));

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
  drawIcon(ctx, def.icon, mid.x, mid.y, 14 * cam.z);

  // status badges above the roof
  const badges: { icon: string; color: string }[] = [];
  if (unpowered) badges.push({ icon: 'power', color: '#ffb02e' });
  if (!b.connected) badges.push({ icon: 'ban', color: '#ff6b5e' });
  if (def.heat > 0 && !b.heated) badges.push({ icon: 'freeze', color: '#bfe3ff' });
  let sx = mid.x - ((badges.length - 1) * 12 * cam.z) / 2;
  const sy = mid.y - 16 * cam.z;
  for (const badge of badges) {
    drawIcon(ctx, badge.icon, sx, sy, 10 * cam.z, badge.color);
    sx += 12 * cam.z;
  }
}

function drawTruck(ctx: CanvasRenderingContext2D, tr: Truck, wx: number, wy: number, cam: Camera, foreign?: boolean) {
  const p = toScreen(wx, wy, cam);
  const s = cam.z;
  // body — foreign lorries wear a pale international livery with a red band
  ctx.fillStyle = foreign ? '#8a94a0' : '#2f3844';
  ctx.fillRect(p.x - 5 * s, p.y - 9 * s, 10 * s, 7 * s);
  if (foreign) {
    ctx.fillStyle = '#b03030';
    ctx.fillRect(p.x - 5 * s, p.y - 6 * s, 10 * s, 1.4 * s);
  }
  // cargo dot
  ctx.fillStyle = RESOURCES[tr.cargo].color;
  ctx.fillRect(p.x - 3 * s, p.y - 12 * s, 6 * s, 4 * s);
  // cab light
  ctx.fillStyle = '#e8e04a';
  ctx.fillRect(p.x + 3 * s, p.y - 8 * s, 2 * s, 2 * s);
}
