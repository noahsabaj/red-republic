// ============================================================
// The game's single icon source: Lucide vector data, rendered as
// inline SVG by <GameIcon> (see GameIcon.tsx) and stroked onto the
// canvas by drawIcon(). config/engine refer to icons by NAME (plain
// strings) so the simulation stays UI-free; a test asserts every
// referenced name exists here.
// ============================================================
import type { IconNode } from 'lucide';
import {
  Anchor, Anvil, Axe, Ban, Beer, BrickWall, Building2, CircleCheck, CircleDot, CircleHelp,
  Cloud, CloudFog, CloudLightning, CloudRain, CloudSnow,
  Container, CookingPot, Coins, Droplet, Eraser, Factory, Flag, FlaskConical, Flame, Fuel,
  Gauge, Grip, HardHat, HeartPulse, Home, Keyboard, Landmark, Layers, Leaf, Magnet,
  Map as MapIcon, Package, Pause, Pickaxe, Route, Ruler, ScrollText, ShoppingBasket, Shirt, Shovel,
  Slice, Smile, Snowflake, Sprout, Square, Star, Sun, Target, Tractor, TreePine, Truck,
  Users, Warehouse, Wheat, Wind, X, Zap, BedDouble,
} from 'lucide';

export const GAME_ICONS = {
  // resources
  coal: CircleDot,
  ironOre: Magnet,
  steel: Anvil,
  oil: Droplet,
  fuel: Fuel,
  wood: TreePine,
  planks: Layers,
  gravel: Grip,
  bricks: BrickWall,
  crops: Wheat,
  food: CookingPot,
  clothes: Shirt,
  // buildings
  road: Route,
  house: Home,
  apartment: Building2,
  woodcutter: Axe,
  sawmill: Slice,
  gravelQuarry: Shovel,
  brickworks: BrickWall,
  coalMine: Pickaxe,
  ironMine: Magnet,
  steelMill: Anvil,
  oilPump: Droplet,
  refinery: FlaskConical,
  powerPlant: Zap,
  heatingPlant: Flame,
  farm: Tractor,
  foodFactory: CookingPot,
  textileMill: Shirt,
  store: ShoppingBasket,
  clinic: HeartPulse,
  pub: Beer,
  warehouse: Warehouse,
  depot: Landmark,
  constructionOffice: HardHat,
  customs: Container,
  port: Anchor,
  // build-menu categories
  'cat-infra': Route,
  'cat-housing': Home,
  'cat-industry': Factory,
  'cat-services': HeartPulse,
  'cat-trade': Package,
  // HUD / panels / status
  stockpiles: Package,
  plan: Target,
  trade: Container,
  contract: ScrollText,
  flag: Flag,
  help: CircleHelp,
  users: Users,
  happy: Smile,
  power: Zap,
  heat: Flame,
  freeze: Snowflake,
  ban: Ban,
  truck: Truck,
  builders: HardHat,
  coins: Coins,
  star: Star,
  check: CircleCheck,
  square: Square,
  pause: Pause,
  close: X,
  bulldoze: Eraser,
  map: MapIcon,
  pick: Pickaxe,
  keyboard: Keyboard,
  factory: Factory,
  staff: Users,
  eff: Gauge,
  beds: BedDouble,
  coverage: Ruler,
  fields: Sprout,
  // seasons
  winter: Snowflake,
  spring: Sprout,
  summer: Sun,
  autumn: Leaf,
  // weather conditions
  clear: Sun,
  overcast: Cloud,
  rain: CloudRain,
  snow: CloudSnow,
  storm: CloudLightning,
  blizzard: Wind,
  fog: CloudFog,
} satisfies Record<string, IconNode>;

export type GameIconName = keyof typeof GAME_ICONS;

export function isGameIcon(name: string): name is GameIconName {
  return name in GAME_ICONS;
}

// ---------------- canvas ----------------

const pathCache = new Map<string, Path2D>();

function compiledPath(name: GameIconName): Path2D {
  let p = pathCache.get(name);
  if (p) return p;
  p = new Path2D();
  for (const [tag, attrs] of GAME_ICONS[name]) {
    const a = attrs as Record<string, string | number>;
    switch (tag) {
      case 'path':
        p.addPath(new Path2D(String(a.d)));
        break;
      case 'circle':
        p.moveTo(Number(a.cx) + Number(a.r), Number(a.cy));
        p.arc(Number(a.cx), Number(a.cy), Number(a.r), 0, Math.PI * 2);
        break;
      case 'rect': {
        const rx = Number(a.rx ?? 0);
        if (rx > 0) p.roundRect(Number(a.x), Number(a.y), Number(a.width), Number(a.height), rx);
        else p.rect(Number(a.x), Number(a.y), Number(a.width), Number(a.height));
        break;
      }
      case 'line':
        p.moveTo(Number(a.x1), Number(a.y1));
        p.lineTo(Number(a.x2), Number(a.y2));
        break;
      case 'polyline':
      case 'polygon': {
        const pts = String(a.points).trim().split(/[\s,]+/).map(Number);
        p.moveTo(pts[0], pts[1]);
        for (let i = 2; i < pts.length; i += 2) p.lineTo(pts[i], pts[i + 1]);
        if (tag === 'polygon') p.closePath();
        break;
      }
    }
  }
  pathCache.set(name, p);
  return p;
}

/** Stroke an icon centered at (cx, cy). A dark halo keeps it readable on any roof. */
export function drawIcon(
  ctx: CanvasRenderingContext2D,
  name: string,
  cx: number,
  cy: number,
  size: number,
  color = '#ffffff',
  halo: string | null = 'rgba(20,16,12,0.55)',
) {
  if (!isGameIcon(name) || size < 5) return;
  const p = compiledPath(name);
  ctx.save();
  ctx.translate(cx - size / 2, cy - size / 2);
  ctx.scale(size / 24, size / 24);
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  if (halo) {
    ctx.strokeStyle = halo;
    ctx.lineWidth = 4.5;
    ctx.stroke(p);
  }
  ctx.strokeStyle = color;
  ctx.lineWidth = 2.2;
  ctx.stroke(p);
  ctx.restore();
}
