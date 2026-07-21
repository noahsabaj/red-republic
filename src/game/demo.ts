// Demo town seeder — opens a developed republic via ?demo in the URL
import type { GameEngine, TilePatch } from './engine';

export function seedDemoTown(e: GameEngine) {
  const depot = [...e.buildings.values()].find(b => b.defId === 'depot')!;
  const sx = depot.x, sy = depot.y;
  e.dollars = 999999;
  e.rubles = 999999;

  // The base hugs the national border, so the demo grid anchors further
  // inland (its bounding box clears the base row, the customs crossing and
  // both foreign strips) and a connector road ties it back to the depot.
  const shift = e.borderEdge === 'W' ? [15, 0] : e.borderEdge === 'E' ? [-15, 0]
    : e.borderEdge === 'N' ? [0, 8] : e.borderEdge === 'S' ? [0, -13] : [0, 0];
  const gx = Math.max(14, Math.min(e.mapW - 15, sx + shift[0]));
  const gy = Math.max(4, Math.min(e.mapH - 13, sy + shift[1]));

  const pendingRoads: TilePatch[] = [];
  const road = (x: number, y: number) => {
    const t = e.tiles[y]?.[x];
    if (t && !t.road && !t.buildingId && t.terrain !== 'water') pendingRoads.push({ x, y, road: true });
  };
  const flushRoads = () => {
    e.applyTilePatches(pendingRoads);
    pendingRoads.length = 0;
  };
  // road grid around the town center
  for (let x = gx - 12; x <= gx + 12; x++) { road(x, gy - 1); road(x, gy + 3); road(x, gy + 8); }
  for (let y = gy - 1; y <= gy + 8; y++) { road(gx - 12, y); road(gx + 12, y); road(gx - 6, y); road(gx + 5, y); }
  // connector: base road row to the grid (an L along the shift direction)
  for (let x = Math.min(sx, gx); x <= Math.max(sx, gx); x++) road(x, sy - 1);
  for (let y = Math.min(sy - 1, gy - 1); y <= Math.max(sy - 1, gy - 1); y++) road(gx, y);
  flushRoads();

  const put = (defId: string, x: number, y: number) => e.tryPlace(defId, x, y, { instant: true });

  // housing & services
  put('house', gx - 11, gy);
  put('house', gx - 10, gy);
  put('house', gx - 9, gy);
  put('apartment', gx - 11, gy + 4);
  put('apartment', gx - 8, gy + 4);
  put('store', gx - 5, gy + 4);
  put('pub', gx - 4, gy);
  put('clinic', gx - 5, gy);
  put('heatingPlant', gx + 3, gy + 4);
  put('powerPlant', gx + 6, gy + 4);
  // industry row
  put('farm', gx - 11, gy + 9);
  put('foodFactory', gx - 8, gy + 9);
  put('sawmill', gx - 5, gy + 9);
  put('warehouse', gx - 3, gy + 9);
  put('brickworks', gx + 1, gy + 9);
  put('textileMill', gx + 3, gy + 9);
  // mines on real deposits nearby
  const findDeposit = (kind: string) => {
    for (let rad = 4; rad < 24; rad++)
      for (let dy = -rad; dy <= rad; dy++) for (let dx = -rad; dx <= rad; dx++) {
        if (e.tiles[gy + dy]?.[gx + dx]?.deposit === kind) return { x: gx + dx, y: gy + dy };
      }
    return null;
  };
  const connectMine = (spot: { x: number; y: number }, defId: string) => {
    const res = put(defId, spot.x, spot.y);
    if (!res.ok) return res; // placement failed — don't build roads to nothing
    const inst = e.buildingAt(spot.x, spot.y)!;
    const tryRoute = (route: 'col' | 'row') => {
      if (route === 'col') for (let y = Math.min(spot.y, gy + 8); y <= Math.max(spot.y, gy + 8); y++) road(spot.x + 1, y);
      else for (let x = Math.min(spot.x, gx + 12); x <= Math.max(spot.x, gx + 12); x++) road(x, spot.y + 1);
      flushRoads();
    };
    tryRoute('col');
    if (!e.findPath(e.adjacentRoads(inst), e.adjacentRoads(depot))) tryRoute('row');
    return res;
  };
  const coal = findDeposit('coal');
  if (coal) connectMine(coal, 'coalMine');
  const oil = findDeposit('oil');
  if (oil) connectMine(oil, 'oilPump');

  // stock the depot so the economy hums immediately (within its storage caps)
  depot.stock = { planks: 100, bricks: 100, steel: 40, food: 100, coal: 120, clothes: 20, crops: 40 };

  // simulate ~100 days to settle workers, trucks and citizens
  for (let i = 0; i < 100; i++) e.advance(500);
  e.dollars = 5000;
  e.rubles = 60000;
}
