// Demo town seeder — opens a developed republic via ?demo in the URL
import type { GameEngine } from './engine';
import { MAP_W, MAP_H } from './mapgen';

export function seedDemoTown(e: GameEngine) {
  const depot = [...e.buildings.values()].find(b => b.defId === 'depot')!;
  const sx = depot.x, sy = depot.y;
  e.dollars = 999999;
  e.rubles = 999999;

  const road = (x: number, y: number) => {
    const t = e.tiles[y]?.[x];
    if (t && !t.road && !t.buildingId && t.terrain !== 'water') t.road = true;
  };
  // road grid around the base
  for (let x = sx - 12; x <= sx + 12; x++) { road(x, sy - 1); road(x, sy + 3); road(x, sy + 8); }
  for (let y = sy - 1; y <= sy + 8; y++) { road(sx - 12, y); road(sx + 12, y); road(sx - 6, y); road(sx + 5, y); }

  const put = (defId: string, x: number, y: number) => e.tryPlace(defId, x, y, true);

  // housing & services
  put('house', sx - 11, sy);
  put('house', sx - 10, sy);
  put('house', sx - 9, sy);
  put('apartment', sx - 11, sy + 4);
  put('apartment', sx - 8, sy + 4);
  put('store', sx - 5, sy + 4);
  put('pub', sx - 4, sy);
  put('clinic', sx - 5, sy);
  put('heatingPlant', sx + 3, sy + 4);
  put('powerPlant', sx + 6, sy + 4);
  // industry row
  put('farm', sx - 11, sy + 9);
  put('foodFactory', sx - 8, sy + 9);
  put('sawmill', sx - 5, sy + 9);
  put('warehouse', sx - 3, sy + 9);
  put('brickworks', sx + 1, sy + 9);
  put('textileMill', sx + 3, sy + 9);
  // mines on real deposits nearby
  const findDeposit = (kind: string) => {
    for (let rad = 4; rad < 24; rad++)
      for (let dy = -rad; dy <= rad; dy++) for (let dx = -rad; dx <= rad; dx++) {
        if (e.tiles[sy + dy]?.[sx + dx]?.deposit === kind) return { x: sx + dx, y: sy + dy };
      }
    return null;
  };
  const connectMine = (spot: { x: number; y: number }, defId: string) => {
    const b = put(defId, spot.x, spot.y);
    const inst = [...e.buildings.values()].reduce((m, bb) => (bb.id > m.id ? bb : m));
    const tryRoute = (route: 'col' | 'row') => {
      if (route === 'col') for (let y = Math.min(spot.y, sy + 8); y <= Math.max(spot.y, sy + 8); y++) road(spot.x + 1, y);
      else for (let x = Math.min(spot.x, sx + 12); x <= Math.max(spot.x, sx + 12); x++) road(x, spot.y + 1);
    };
    tryRoute('col');
    if (!e.findPath(e.adjacentRoads(inst), e.adjacentRoads(depot))) tryRoute('row');
    return b;
  };
  const coal = findDeposit('coal');
  if (coal) connectMine(coal, 'coalMine');
  const oil = findDeposit('oil');
  if (oil) connectMine(oil, 'oilPump');

  // stock the depot so the economy hums immediately
  depot.stock = { planks: 100, bricks: 100, steel: 40, food: 100, coal: 200, clothes: 20, crops: 40 };

  // simulate ~100 days to settle workers, trucks and citizens
  for (let i = 0; i < 100; i++) e.advance(500);
  e.dollars = 5000;
  e.rubles = 60000;
  // camera hint: center already on base
  void MAP_W; void MAP_H;
}
