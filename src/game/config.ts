// ============================================================
// Red Republic — Planned Economy Builder
// Game configuration: resources, buildings, prices, objectives
// ============================================================
import type { CondDist, WeatherCondition } from './weather';

export type ResourceId =
  | 'coal' | 'ironOre' | 'steel'
  | 'oil' | 'fuel'
  | 'wood' | 'planks'
  | 'gravel' | 'bricks'
  | 'crops' | 'food' | 'clothes'
  | 'machinery';

export const ALL_RESOURCES: ResourceId[] = [
  'coal', 'ironOre', 'steel', 'oil', 'fuel',
  'wood', 'planks', 'gravel', 'bricks',
  'crops', 'food', 'clothes', 'machinery',
];

export interface ResourceDef {
  id: ResourceId;
  name: string;
  icon: string;
  color: string;
  priceEast: number; // sell price in rubles
  priceWest: number; // sell price in dollars
}

export const RESOURCES: Record<ResourceId, ResourceDef> = {
  coal:    { id: 'coal',    name: 'Coal',      icon: 'coal', color: '#3a3a3a', priceEast: 2.5,  priceWest: 1 },
  ironOre: { id: 'ironOre', name: 'Iron Ore',  icon: 'ironOre', color: '#8a4b2f', priceEast: 3,    priceWest: 1.5 },
  steel:   { id: 'steel',   name: 'Steel',     icon: 'steel', color: '#9aa5b1', priceEast: 14,   priceWest: 8 },
  oil:     { id: 'oil',     name: 'Oil',       icon: 'oil', color: '#22252a', priceEast: 5,    priceWest: 3 },
  fuel:    { id: 'fuel',    name: 'Fuel',      icon: 'fuel', color: '#c9a227', priceEast: 10,   priceWest: 6 },
  wood:    { id: 'wood',    name: 'Wood',      icon: 'wood', color: '#7a5230', priceEast: 2,    priceWest: 1 },
  planks:  { id: 'planks',  name: 'Planks',    icon: 'planks', color: '#a4713b', priceEast: 5,    priceWest: 2.5 },
  gravel:  { id: 'gravel',  name: 'Gravel',    icon: 'gravel', color: '#8d8d8d', priceEast: 1.5,  priceWest: 0.7 },
  bricks:  { id: 'bricks',  name: 'Bricks',    icon: 'bricks', color: '#b0472f', priceEast: 6,    priceWest: 3 },
  crops:   { id: 'crops',   name: 'Crops',     icon: 'crops', color: '#d4b545', priceEast: 2,    priceWest: 1 },
  food:    { id: 'food',    name: 'Food',      icon: 'food', color: '#d98e4a', priceEast: 4.5,  priceWest: 2 },
  clothes: { id: 'clothes', name: 'Clothes',   icon: 'clothes', color: '#5b7fb5', priceEast: 9,    priceWest: 5 },
  // The industrialization tax: East sells machine-tools cheaper for rubles
  // (the bloc's specialty); Western machines carry an embargo premium.
  machinery: { id: 'machinery', name: 'Machinery', icon: 'machinery', color: '#67805e', priceEast: 80, priceWest: 50 },
};

export const IMPORT_MARKUP = 1.6; // buy price = sell price * markup

// ------------------------------------------------------------
// Buildings
// ------------------------------------------------------------

export type Category = 'infra' | 'housing' | 'industry' | 'services' | 'trade';

export const CATEGORY_NAMES: Record<Category, string> = {
  infra: 'Infrastructure',
  housing: 'Housing',
  industry: 'Industry & Resources',
  services: 'Services',
  trade: 'Trade & Storage',
};

export type DepositType = 'coal' | 'ironOre' | 'oil' | 'gravel';

export interface BuildingDef {
  id: string;
  name: string;
  icon: string;
  category: Category;
  size: [number, number]; // w x h in tiles
  // construction — the ONLY cost: nothing domestic costs money in a planned economy
  materials: Partial<Record<ResourceId, number>>;
  labor: number; // total worker-days to construct
  /** Completed site becomes a road tile instead of a standing building. */
  becomesRoad?: boolean;
  // operation
  workers: number; // jobs at full staffing
  /** Daily operational consumption at full activity (machinery wear). An empty
   *  bin never stalls the building — it runs 'worn' at BALANCE.wornEffMult. */
  wear?: Partial<Record<ResourceId, number>>;
  power: number;   // MW consumed (+) or produced handled via powerOutput
  powerOutput?: number; // MW produced (power plants)
  heatOutput?: number;  // heat units produced
  heat: number;    // heat consumed (housing)
  inputs?: Partial<Record<ResourceId, number>>;   // per day at full efficiency
  outputs?: Partial<Record<ResourceId, number>>;  // per day at full efficiency
  storage: Partial<Record<ResourceId, number>>;   // local storage caps
  housingCapacity?: number;
  serviceRadius?: number;
  serviceType?: 'shop' | 'health' | 'culture';
  requiresDeposit?: DepositType;
  requiresForest?: boolean;
  isFarm?: boolean;
  isConstructionOffice?: boolean;
  isCustoms?: boolean;
  isDepot?: boolean;
  isPort?: boolean; // dockside freight hub — must touch water; barges link ports
  isMotorDepot?: boolean; // garage — staffed drivers add trucks to the haulage fleet
  isGasStation?: boolean; // fuels the fleet — holds the fuel depot-trucks burn
  pollution?: number; // 0..5 intensity
  boxHeight: number; // render height
  color: string;     // roof color
  wallColor: string;
  description: string;
}

const B = (def: BuildingDef) => def;

export const BUILDINGS: Record<string, BuildingDef> = {
  // ---------- Infrastructure ----------
  road: B({
    id: 'road', name: 'Road', icon: 'road', category: 'infra', size: [1, 1],
    materials: { gravel: 2 }, labor: 3, becomesRoad: true,
    workers: 0, power: 0, heat: 0,
    storage: {}, boxHeight: 0, color: '#5c5c5c', wallColor: '#5c5c5c',
    description: 'Gravel road, built tile by tile: trucks deliver the gravel, crews lay the surface. Buildings must touch a finished road to receive deliveries. Over water it becomes a bridge needing planks and steel.',
  }),
  // Hidden def: tryPlace substitutes it when a road is painted on water.
  // Not in BUILD_LIST — the player only ever sees the Road tool.
  bridge: B({
    id: 'bridge', name: 'Bridge', icon: 'road', category: 'infra', size: [1, 1],
    materials: { planks: 5, steel: 3 }, labor: 40, becomesRoad: true,
    workers: 0, power: 0, heat: 0,
    storage: {}, boxHeight: 0, color: '#8a6f4d', wallColor: '#8a6f4d',
    description: 'A timber-and-steel crossing. Piling by piling, a megaproject worth its steel.',
  }),

  // ---------- Housing ----------
  house: B({
    id: 'house', name: 'Small House', icon: 'house', category: 'housing', size: [1, 1],
    materials: { planks: 6, bricks: 4 }, labor: 60,
    workers: 0, power: 0.3, heat: 0.5, storage: {},
    housingCapacity: 8, boxHeight: 12, color: '#b0483a', wallColor: '#e2d3b3',
    description: 'A modest family house for 8 citizens. Needs power, heat in winter, and a shop nearby.',
  }),
  apartment: B({
    id: 'apartment', name: 'Apartment Block', icon: 'apartment', category: 'housing', size: [2, 2],
    materials: { planks: 10, bricks: 30, steel: 6, gravel: 8 }, labor: 300,
    workers: 0, power: 1.2, heat: 2, storage: {},
    housingCapacity: 40, boxHeight: 30, color: '#8f3d31', wallColor: '#c9b18a',
    description: 'A proud socialist prefab block housing 40 citizens.',
  }),

  // ---------- Industry ----------
  woodcutter: B({
    id: 'woodcutter', name: 'Woodcutter Post', icon: 'woodcutter', category: 'industry', size: [1, 1],
    materials: { planks: 4 }, labor: 50,
    workers: 6, power: 0, heat: 0, storage: { wood: 30 },
    outputs: { wood: 4 }, requiresForest: true, pollution: 1,
    boxHeight: 10, color: '#4a6b3a', wallColor: '#8a6b45',
    description: 'Lumberjacks fell trees nearby. Place close to forest. Produces wood.',
  }),
  sawmill: B({
    id: 'sawmill', name: 'Sawmill', icon: 'sawmill', category: 'industry', size: [1, 1],
    materials: { bricks: 10, planks: 6, steel: 2 }, labor: 120,
    workers: 6, power: 1, heat: 0, storage: { wood: 20, planks: 30 },
    inputs: { wood: 2 }, outputs: { planks: 3 }, pollution: 1,
    boxHeight: 14, color: '#7a5230', wallColor: '#b08b5e',
    description: 'Saws 2 wood into 3 planks per day. Planks are a core construction material.',
  }),
  gravelQuarry: B({
    id: 'gravelQuarry', name: 'Gravel Quarry', icon: 'gravelQuarry', category: 'industry', size: [1, 1],
    materials: { planks: 6, bricks: 4 }, labor: 80,
    workers: 8, power: 0.5, heat: 0, storage: { gravel: 40 },
    outputs: { gravel: 5 }, requiresDeposit: 'gravel', pollution: 2,
    boxHeight: 8, color: '#6d6d6d', wallColor: '#9a9a9a',
    description: 'Must be built on a gravel deposit. Extracts gravel.',
  }),
  brickworks: B({
    id: 'brickworks', name: 'Brickworks', icon: 'brickworks', category: 'industry', size: [1, 1],
    materials: { bricks: 12, steel: 4, planks: 4 }, labor: 130,
    workers: 10, power: 1.5, heat: 0, storage: { gravel: 25, bricks: 35 },
    inputs: { gravel: 3 }, outputs: { bricks: 4 }, pollution: 2,
    boxHeight: 16, color: '#8a3226', wallColor: '#b0604a',
    description: 'Fires 3 gravel into 4 bricks per day. Bricks are needed for almost everything.',
  }),
  coalMine: B({
    id: 'coalMine', name: 'Coal Mine', icon: 'coalMine', category: 'industry', size: [1, 1],
    materials: { bricks: 15, steel: 6, planks: 4, machinery: 2 }, labor: 200,
    workers: 14, power: 2, heat: 0, storage: { coal: 60, machinery: 6 },
    wear: { machinery: 0.015 },
    outputs: { coal: 6 }, requiresDeposit: 'coal', pollution: 3,
    boxHeight: 12, color: '#2e2e2e', wallColor: '#4f4f4f',
    description: 'Must be built on a coal deposit. Coal feeds power and heating plants and the steel mill.',
  }),
  ironMine: B({
    id: 'ironMine', name: 'Iron Ore Mine', icon: 'ironMine', category: 'industry', size: [1, 1],
    materials: { bricks: 15, steel: 6, planks: 4, machinery: 2 }, labor: 200,
    workers: 14, power: 2, heat: 0, storage: { ironOre: 60, machinery: 6 },
    wear: { machinery: 0.015 },
    outputs: { ironOre: 5 }, requiresDeposit: 'ironOre', pollution: 3,
    boxHeight: 12, color: '#6e3a24', wallColor: '#8a5a40',
    description: 'Must be built on an iron ore deposit.',
  }),
  steelMill: B({
    id: 'steelMill', name: 'Steel Mill', icon: 'steelMill', category: 'industry', size: [2, 2],
    materials: { bricks: 30, steel: 15, planks: 8, gravel: 16, machinery: 8 }, labor: 400,
    workers: 30, power: 6, heat: 0, storage: { ironOre: 40, coal: 40, steel: 40, machinery: 6 },
    wear: { machinery: 0.03 },
    inputs: { ironOre: 2, coal: 1 }, outputs: { steel: 1.5 }, pollution: 4,
    boxHeight: 24, color: '#5a5f66', wallColor: '#7d838c',
    description: 'Smelts 2 iron ore + 1 coal into 1.5 steel daily. Steel sells well abroad.',
  }),
  oilPump: B({
    id: 'oilPump', name: 'Oil Pump', icon: 'oilPump', category: 'industry', size: [1, 1],
    materials: { bricks: 12, steel: 10, machinery: 3 }, labor: 220,
    workers: 10, power: 2, heat: 0, storage: { oil: 50, machinery: 6 },
    wear: { machinery: 0.02 },
    outputs: { oil: 4 }, requiresDeposit: 'oil', pollution: 2,
    boxHeight: 18, color: '#1e2126', wallColor: '#3a3f46',
    description: 'Must be built on an oil deposit. Pumps crude oil.',
  }),
  refinery: B({
    id: 'refinery', name: 'Oil Refinery', icon: 'refinery', category: 'industry', size: [2, 2],
    materials: { bricks: 30, steel: 18, planks: 6, gravel: 16, machinery: 6 }, labor: 420,
    workers: 25, power: 5, heat: 0, storage: { oil: 40, fuel: 40, machinery: 6 },
    wear: { machinery: 0.025 },
    inputs: { oil: 3 }, outputs: { fuel: 2 }, pollution: 3,
    boxHeight: 22, color: '#8c7a2a', wallColor: '#a89a4a',
    description: 'Refines 3 oil into 2 fuel per day. Fuel earns hard currency in the West.',
  }),
  powerPlant: B({
    id: 'powerPlant', name: 'Coal Power Plant', icon: 'powerPlant', category: 'industry', size: [2, 2],
    materials: { bricks: 25, steel: 12, planks: 6, gravel: 12, machinery: 5 }, labor: 350,
    workers: 15, power: 0, powerOutput: 12, heat: 0,
    storage: { coal: 50, machinery: 6 }, inputs: { coal: 2 }, pollution: 3,
    wear: { machinery: 0.02 },
    boxHeight: 26, color: '#4e5661', wallColor: '#6b7480',
    description: 'Burns 2 coal daily to generate 12 MW. Without power, industry stalls and homes go dark.',
  }),
  heatingPlant: B({
    id: 'heatingPlant', name: 'Heating Plant', icon: 'heatingPlant', category: 'industry', size: [1, 1],
    materials: { bricks: 18, steel: 8, planks: 4, machinery: 1 }, labor: 180,
    workers: 8, power: 1, heatOutput: 8, heat: 0,
    storage: { coal: 40, machinery: 6 }, inputs: { coal: 1 }, pollution: 2,
    wear: { machinery: 0.01 },
    boxHeight: 16, color: '#7a3b2a', wallColor: '#9c5a44',
    description: 'Burns coal for heat, throttling to demand — the colder the day, the more it burns. Citizens freeze without it.',
  }),
  farm: B({
    id: 'farm', name: 'Collective Farm', icon: 'farm', category: 'industry', size: [2, 2],
    materials: { planks: 12, bricks: 8 }, labor: 150,
    workers: 10, power: 0.5, heat: 0, storage: { crops: 80 },
    outputs: { crops: 6 }, isFarm: true, pollution: 0,
    boxHeight: 8, color: '#8a6b3a', wallColor: '#c9a86b',
    description: 'Yields crops from open ground around it. Sowing in spring, harvest late summer–autumn. Nothing grows in winter!',
  }),
  foodFactory: B({
    id: 'foodFactory', name: 'Food Factory', icon: 'foodFactory', category: 'industry', size: [1, 1],
    materials: { bricks: 18, steel: 6, planks: 6, machinery: 1 }, labor: 200,
    workers: 12, power: 2, heat: 0, storage: { crops: 40, food: 40, machinery: 6 },
    wear: { machinery: 0.01 },
    inputs: { crops: 2.5 }, outputs: { food: 2.5 }, pollution: 1,
    boxHeight: 16, color: '#b06a2a', wallColor: '#d09a5a',
    description: 'Bakes 2.5 crops into 2.5 food daily. Citizens without food will leave — or worse.',
  }),
  textileMill: B({
    id: 'textileMill', name: 'Textile Mill', icon: 'textileMill', category: 'industry', size: [1, 1],
    materials: { bricks: 16, steel: 5, planks: 6, machinery: 1 }, labor: 180,
    workers: 12, power: 2, heat: 0, storage: { crops: 30, clothes: 30, machinery: 6 },
    wear: { machinery: 0.01 },
    inputs: { crops: 2 }, outputs: { clothes: 1.2 }, pollution: 1,
    boxHeight: 16, color: '#3a5a8a', wallColor: '#6b8ab5',
    description: 'Weaves 2 crops into 1.2 clothes daily. Dressed citizens are happy citizens.',
  }),
  machineWorks: B({
    id: 'machineWorks', name: 'Machine Works', icon: 'machineWorks', category: 'industry', size: [2, 2],
    materials: { bricks: 35, steel: 20, planks: 10, gravel: 16, machinery: 6 }, labor: 450,
    workers: 22, power: 4, heat: 0, storage: { steel: 30, machinery: 20 },
    wear: { machinery: 0.02 },
    inputs: { steel: 3 }, outputs: { machinery: 1 }, pollution: 2,
    boxHeight: 24, color: '#4a5e42', wallColor: '#77906b',
    description: 'Turns 3 steel into 1 machinery per day — the means of production, produced. Ends your dependence on imported machines.',
  }),

  // ---------- Services ----------
  store: B({
    id: 'store', name: 'State Store', icon: 'store', category: 'services', size: [1, 1],
    materials: { planks: 6, bricks: 8 }, labor: 80,
    workers: 3, power: 0.5, heat: 0, storage: { food: 40, clothes: 20 },
    serviceRadius: 8, serviceType: 'shop',
    boxHeight: 12, color: '#3a6b4f', wallColor: '#d8cdb0',
    description: 'Sells food and clothes to citizens within 8 tiles. Keep it stocked by road!',
  }),
  clinic: B({
    id: 'clinic', name: 'Polyclinic', icon: 'clinic', category: 'services', size: [1, 1],
    materials: { bricks: 14, steel: 4, planks: 6 }, labor: 150,
    workers: 6, power: 1, heat: 0, storage: {},
    serviceRadius: 8, serviceType: 'health',
    boxHeight: 14, color: '#b5b5b5', wallColor: '#e8e8e8',
    description: 'Free healthcare for citizens within 8 tiles. Healthy workers work harder.',
  }),
  pub: B({
    id: 'pub', name: 'Culture Club', icon: 'pub', category: 'services', size: [1, 1],
    materials: { planks: 8, bricks: 10 }, labor: 100,
    workers: 4, power: 0.5, heat: 0, storage: {},
    serviceRadius: 8, serviceType: 'culture',
    boxHeight: 12, color: '#6b4a8a', wallColor: '#9a7ab5',
    description: 'Beer, chess and patriotic cinema within 8 tiles. Raises happiness.',
  }),

  // ---------- Trade & Storage ----------
  warehouse: B({
    id: 'warehouse', name: 'Warehouse', icon: 'warehouse', category: 'trade', size: [1, 1],
    materials: { planks: 8, bricks: 10 }, labor: 90,
    workers: 2, power: 0.2, heat: 0,
    storage: { coal: 40, ironOre: 40, steel: 40, oil: 40, fuel: 40, wood: 40, planks: 40, gravel: 40, bricks: 40, crops: 40, food: 40, clothes: 40, machinery: 20 },
    boxHeight: 14, color: '#7a6a4a', wallColor: '#a89878',
    description: 'Open storage for 40 units of every good. Trucks haul surplus here.',
  }),
  depot: B({
    id: 'depot', name: 'Council Depot', icon: 'depot', category: 'trade', size: [2, 2],
    materials: { bricks: 15, planks: 10 }, labor: 120,
    workers: 4, power: 0.5, heat: 0, isDepot: true,
    storage: { coal: 120, ironOre: 120, steel: 120, oil: 120, fuel: 120, wood: 120, planks: 120, gravel: 120, bricks: 120, crops: 120, food: 120, clothes: 120, machinery: 60 },
    boxHeight: 18, color: '#8a2a2a', wallColor: '#c9b890',
    description: 'Central storage of the republic. Holds 120 of every good.',
  }),
  constructionOffice: B({
    id: 'constructionOffice', name: 'Construction Office', icon: 'constructionOffice', category: 'infra', size: [1, 1],
    materials: { bricks: 10, planks: 8 }, labor: 110,
    workers: 10, power: 0.5, heat: 0, storage: {},
    isConstructionOffice: true,
    boxHeight: 12, color: '#b0802a', wallColor: '#d0aa5a',
    description: 'Employs builders and operates trucks. No office, no construction, no haulage.',
  }),
  port: B({
    id: 'port', name: 'River Port', icon: 'port', category: 'trade', size: [2, 2],
    materials: { planks: 14, bricks: 10, steel: 4, gravel: 8 }, labor: 160,
    workers: 6, power: 0.5, heat: 0, isPort: true,
    storage: { coal: 50, ironOre: 50, steel: 50, oil: 50, fuel: 50, wood: 50, planks: 50, gravel: 50, bricks: 50, crops: 50, food: 50, clothes: 50, machinery: 30 },
    boxHeight: 14, color: '#3a6b8a', wallColor: '#7a99ad',
    description: 'Dockside freight hub — must be built on the shore. Barges ferry goods between ports across water, far cheaper than long bridges.',
  }),
  motorDepot: B({
    id: 'motorDepot', name: 'Motor Depot', icon: 'truck', category: 'trade', size: [2, 2],
    materials: { bricks: 18, planks: 12, steel: 6, gravel: 8 }, labor: 150,
    workers: 16, power: 0.5, heat: 0, isMotorDepot: true, storage: {},
    boxHeight: 14, color: '#54584e', wallColor: '#8a8f80',
    description: 'Garage for the haulage fleet. Every staffed driver puts another truck on the road, on top of your Construction Offices — but those trucks burn fuel from a Gas Station.',
  }),
  gasStation: B({
    id: 'gasStation', name: 'Gas Station', icon: 'fuel', category: 'trade', size: [1, 1],
    materials: { bricks: 8, steel: 6, planks: 4 }, labor: 90,
    workers: 4, power: 0.5, heat: 0, isGasStation: true, storage: { fuel: 60 },
    boxHeight: 12, color: '#a83a2a', wallColor: '#cf6a4a',
    description: 'Fuels the truck fleet. Depot trucks burn fuel as they haul — keep it stocked (refinery fuel or imports) or the fleet grinds down. Refills by truck like any store.',
  }),
  customs: B({
    id: 'customs', name: 'Customs House', icon: 'customs', category: 'trade', size: [2, 2],
    materials: { bricks: 20, steel: 6, planks: 8, gravel: 10 }, labor: 200,
    workers: 8, power: 1, heat: 0, isCustoms: true,
    storage: { coal: 80, ironOre: 80, steel: 80, oil: 80, fuel: 80, wood: 80, planks: 80, gravel: 80, bricks: 80, crops: 80, food: 80, clothes: 80, machinery: 40 },
    boxHeight: 16, color: '#2a4a7a', wallColor: '#5a7aaa',
    description: 'Foreign trade terminal. Imports arrive here; exports leave from here. Must connect by road.',
  }),
};

export const BUILD_LIST: string[] = [
  'road', 'constructionOffice',
  'house', 'apartment',
  'woodcutter', 'sawmill', 'gravelQuarry', 'brickworks',
  'coalMine', 'ironMine', 'steelMill', 'oilPump', 'refinery',
  'powerPlant', 'heatingPlant', 'farm', 'foodFactory', 'textileMill', 'machineWorks',
  'store', 'clinic', 'pub',
  'warehouse', 'depot', 'motorDepot', 'gasStation', 'port', 'customs',
];

/** One drill-down group inside a category: a labelled cluster of buildings. */
export interface SubCategory { id: string; name: string; ids: string[] }

/** The bottom bar's 3-tier taxonomy: category → sub-category → buildings. Every
 *  BUILD_LIST id appears in exactly one sub-category (a guard test enforces it,
 *  so a newly-added building can never silently fall out of the menu). */
export const SUBCATEGORIES: Record<Category, SubCategory[]> = {
  infra: [
    { id: 'roads', name: 'Roads', ids: ['road'] },
    { id: 'construction', name: 'Construction', ids: ['constructionOffice'] },
  ],
  housing: [
    { id: 'homes', name: 'Homes', ids: ['house', 'apartment'] },
  ],
  industry: [
    { id: 'timber', name: 'Timber', ids: ['woodcutter', 'sawmill'] },
    { id: 'mining', name: 'Mining', ids: ['gravelQuarry', 'coalMine', 'ironMine'] },
    { id: 'energy', name: 'Energy & Fuel', ids: ['powerPlant', 'heatingPlant', 'oilPump', 'refinery'] },
    { id: 'materials', name: 'Materials', ids: ['brickworks', 'steelMill', 'machineWorks'] },
    { id: 'consumer', name: 'Food & Textile', ids: ['farm', 'foodFactory', 'textileMill'] },
  ],
  services: [
    { id: 'shops', name: 'Shops', ids: ['store'] },
    { id: 'health', name: 'Health', ids: ['clinic'] },
    { id: 'culture', name: 'Culture', ids: ['pub'] },
  ],
  trade: [
    { id: 'storage', name: 'Storage', ids: ['warehouse', 'depot'] },
    { id: 'fleet', name: 'Fleet', ids: ['motorDepot', 'gasStation'] },
    { id: 'border', name: 'Border', ids: ['port', 'customs'] },
  ],
};

/** Ordered top-level categories for the bottom build bar: label, icon, and a
 *  Soviet-muted accent tint that groups each cluster visually over the red base. */
export const CATEGORIES: { id: Category; name: string; icon: string; accent: string }[] = [
  { id: 'infra',    name: CATEGORY_NAMES.infra,    icon: 'cat-infra',    accent: '#8ca0b3' },
  { id: 'housing',  name: CATEGORY_NAMES.housing,  icon: 'cat-housing',  accent: '#e0a83e' },
  { id: 'industry', name: CATEGORY_NAMES.industry, icon: 'cat-industry', accent: '#d97a34' },
  { id: 'services', name: CATEGORY_NAMES.services, icon: 'cat-services', accent: '#6fb86a' },
  { id: 'trade',    name: CATEGORY_NAMES.trade,    icon: 'cat-trade',    accent: '#5fa6c9' },
];

// Instant build = importing a Western prefab: priced from the materials bill
// at Western import prices plus a labor surcharge, with a convenience premium.
export const INSTANT_BUILD = {
  laborDollars: 0.15, // $ per labor-day of the prefab crew
  premium: 1.25,      // convenience markup over the raw import value
};

// ------------------------------------------------------------
// Balance constants
// ------------------------------------------------------------

export const BALANCE = {
  workerShare: 0.7,       // share of population that can work
  foodPerCitizen: 0.015,  // per day
  clothesPerCitizen: 0.004,
  truckCapacity: 6,
  truckDaysPerTile: 0.18, // travel days per road tile
  offRoadStepCost: 8,     // off-road land tile costs 8× a road tile (roads always preferred; off-road is a slow fallback)
  foreignLaborPerDay: 0.5, // ₽/builder-day for imported (non-citizen) construction crews (×importPriceMult)
  foreignLaborPerDayEast: 0.5, // ₽/builder-day (East)
  foreignLaborPerDayWest: 0.1, // $/builder-day (West)
  maxActiveTrucksPerOffice: 6,
  trucksPerDriver: 1,      // Motor Depot: trucks added per staffed driver (on top of office trucks)
  truckFuelPerDay: 0.1,    // fuel a working depot-truck burns per day, drawn from Gas Stations
  boatCapacity: 24,       // one barge hauls four truckloads
  boatDaysPerTile: 0.22,  // barges are slower per tile but shortcut the water
  buildersPerSite: 10,    // max builders on one site per day
  serviceRadius: 8,       // fallback when a building def has no serviceRadius
  pollutionRadius: 6,
  winterMonths: [10, 11, 12, 1, 2, 3], // flavor calendar (events); heat need is temperature-driven
  months: ['January','February','March','April','May','June','July','August','September','October','November','December'],
  heatThresholdC: 8,      // outdoor °C below which buildings need heat
  heatDesignTempC: -15,   // heat demand reaches 100% at this temp; colder over-drives it
  droughtAfterDays: 10,   // consecutive rainless warm days before crops start to wither
  borderDepth: 2,         // tiles of foreign soil along the national-border map edge
  customsThroughputPerDay: 30, // units a fully staffed customs house clears daily (auto-trade)
  autoReserveRubles: 500,      // default treasury floor auto-imports never spend below
  autoReserveDollars: 50,
  wornEffMult: 0.5,       // efficiency of a building whose machinery bin ran dry
  wearReserveDays: 30,    // days of wear stock supplyOf protects from being hauled away
  // Machinery-repair dispatch priorities (lower = served first). A worn or
  // critically-low bin is urgent — a half-dead building loses more output than a
  // fed one gains from one more input load — so it outranks factory inputs (20),
  // staying below the construction band (15-17). A healthy bin tops up lazily.
  wearRepairPrio: 18,     // worn/critical machinery bin — urgent domestic repair
  wearImportPrio: 19,     // worn bin, no domestic machinery — paid border import fallback
  wearTopUpPrio: 24,      // healthy bin — routine top-up when trucks are free
  wearCriticalFrac: 0.25, // a bin below this fraction of cap is 'critical' (repaired before it runs dry)
  repairImportTopUpFrac: 0.5, // a repair import buys at most this fraction of the bin — clears 'worn', domestic fills the rest
};

// ------------------------------------------------------------
// Trade contracts (deadline bulk orders from the blocs)
// ------------------------------------------------------------

export const CONTRACTS = {
  offerEveryMonths: 2,    // a new offer lands every other month (if a customs stands)
  // Orders are value-banded, not unit-banded: a machinery tender is a few
  // machines, a coal tender is a trainload, but both are worth comparable money.
  valueBandEast: [250, 1250] as const, // ₽ value of an East tender
  valueBandWest: [125, 625] as const,  // $ value of a West tender
  minUnits: 2, maxUnits: 200,          // amount = clamp(round(value / price))
  premiumMin: 0.15, premiumMax: 0.25,  // over market price, locked at offer time
  deadlineMinDays: 60, deadlineMaxDays: 90,
  offerDays: 30,          // unaccepted offers are withdrawn
  finePct: 0.25,          // of the undelivered value, on failure
  relationsHit: 0.12,     // price penalty added per failed contract
  relationsCap: 0.25,
  relationsDecayPerDay: 0.002, // a failure haunts prices for ~2 months
};

// Farm seasonal factor by month (1-12): sowing, growth, harvest
export const FARM_SEASON: Record<number, number> = {
  1: 0, 2: 0, 3: 0.2, 4: 0.3, 5: 0.35, 6: 0.5,
  7: 0.7, 8: 1.0, 9: 1.2, 10: 1.0, 11: 0.15, 12: 0,
};

// ------------------------------------------------------------
// Weather gameplay effects (the timeline itself lives in weather.ts)
// ------------------------------------------------------------

export interface WeatherFx {
  label: string;
  icon: string;       // GameIcon name
  truckMult: number;  // road speed multiplier
  boatMult: number;   // barge speed multiplier; 0 also grounds new sailings
  buildMult: number;  // construction crew effectiveness
  farmMult: number;   // crop growth multiplier
  morale: -1 | 0 | 1; // daily mood contribution (streaks nudge happiness slightly)
}

export const WEATHER: Record<WeatherCondition, WeatherFx> = {
  clear:    { label: 'Clear',    icon: 'clear',    truckMult: 1,    boatMult: 1,   buildMult: 1,    farmMult: 1,    morale: 1 },
  overcast: { label: 'Overcast', icon: 'overcast', truckMult: 1,    boatMult: 1,   buildMult: 1,    farmMult: 1,    morale: 0 },
  rain:     { label: 'Rain',     icon: 'rain',     truckMult: 0.85, boatMult: 0.9, buildMult: 0.85, farmMult: 1.15, morale: -1 },
  snow:     { label: 'Snowfall', icon: 'snow',     truckMult: 0.7,  boatMult: 0.8, buildMult: 0.8,  farmMult: 1,    morale: 0 },
  storm:    { label: 'Storm',    icon: 'storm',    truckMult: 0.6,  boatMult: 0,   buildMult: 0.5,  farmMult: 1,    morale: -1 },
  blizzard: { label: 'Blizzard', icon: 'blizzard', truckMult: 0.45, boatMult: 0,   buildMult: 0.3,  farmMult: 1,    morale: -1 },
  fog:      { label: 'Fog',      icon: 'fog',      truckMult: 0.75, boatMult: 0,   buildMult: 1,    farmMult: 1,    morale: 0 },
};

// ------------------------------------------------------------
// Climate regions (numeric climate for the WeatherTimeline)
// ------------------------------------------------------------

export type ClimateId = 'plains' | 'taiga' | 'steppe' | 'maritime';

export interface ClimateDef {
  id: ClimateId;
  label: string;
  description: string; // new-game card blurb
  icon: string;        // GameIcon name
  tempMean: number;    // annual mean, °C
  tempAmp: number;     // seasonal sinusoid amplitude
  peakDoy: number;     // warmest day-of-year (194 = mid-July)
  condDist: CondDist;
  freezeAt: number;    // freeze-degree-days to lock the river…
  thawAt: number;      // …and the hysteresis floor to break the ice
  snowfallPerDay: number;
  blizzardPerDay: number;
  meltRate: number;    // melt = tempC * meltRate on warm days
}

// plains is the pre-preset continental climate, values verbatim — the default
// stream is pinned by climate.test.ts and must never drift.
export const CLIMATES: Record<ClimateId, ClimateDef> = {
  plains: {
    id: 'plains', label: 'Central Plains', icon: 'climate-plains',
    description: 'The classic continental heartland. Warm summers, honest winters, a river that freezes when it must.',
    tempMean: 6, tempAmp: 18, peakDoy: 194,
    condDist: {
      winter: [0.30, 0.30, 0.28, 0.05, 0.07],
      spring: [0.38, 0.26, 0.24, 0.05, 0.07],
      summer: [0.50, 0.20, 0.18, 0.08, 0.04],
      autumn: [0.28, 0.28, 0.24, 0.05, 0.15],
    },
    freezeAt: 30, thawAt: 8, snowfallPerDay: 1.2, blizzardPerDay: 2.5, meltRate: 0.35,
  },
  taiga: {
    id: 'taiga', label: 'Northern Taiga', icon: 'climate-taiga',
    description: 'Brutal winters near −25 °C devour coal; the river locks for half the year and the farms sleep until May.',
    tempMean: -2, tempAmp: 22, peakDoy: 194,
    condDist: {
      winter: [0.24, 0.28, 0.36, 0.06, 0.06],
      spring: [0.34, 0.28, 0.26, 0.04, 0.08],
      summer: [0.46, 0.24, 0.20, 0.06, 0.04],
      autumn: [0.24, 0.30, 0.30, 0.05, 0.11],
    },
    freezeAt: 24, thawAt: 6, snowfallPerDay: 1.5, blizzardPerDay: 3.0, meltRate: 0.35,
  },
  steppe: {
    id: 'steppe', label: 'Southern Steppe', icon: 'climate-steppe',
    description: 'Mild winters barely trouble the heating plants, but scorching rainless summers wither the harvest.',
    tempMean: 10, tempAmp: 16, peakDoy: 194,
    condDist: {
      winter: [0.40, 0.28, 0.22, 0.04, 0.06],
      spring: [0.46, 0.24, 0.20, 0.06, 0.04],
      summer: [0.62, 0.16, 0.10, 0.09, 0.03],
      autumn: [0.42, 0.26, 0.20, 0.05, 0.07],
    },
    freezeAt: 40, thawAt: 10, snowfallPerDay: 1.0, blizzardPerDay: 2.0, meltRate: 0.45,
  },
  maritime: {
    id: 'maritime', label: 'Maritime Coast', icon: 'climate-maritime',
    description: 'The river almost never freezes — barges run all year — but fog and rain dog the docks and roads.',
    tempMean: 10, tempAmp: 8, peakDoy: 194,
    condDist: {
      winter: [0.16, 0.30, 0.30, 0.06, 0.18],
      spring: [0.24, 0.28, 0.26, 0.05, 0.17],
      summer: [0.34, 0.26, 0.22, 0.06, 0.12],
      autumn: [0.18, 0.28, 0.26, 0.06, 0.22],
    },
    freezeAt: 120, thawAt: 30, snowfallPerDay: 1.0, blizzardPerDay: 2.0, meltRate: 0.5,
  },
};

export const DEFAULT_CLIMATE: ClimateId = 'plains';

// ------------------------------------------------------------
// New-game presets: map sizes and difficulty (start conditions only —
// the simulation itself is identical across difficulties)
// ------------------------------------------------------------

export type MapSizeId = 'small' | 'medium' | 'large' | 'vast';

export const MAP_SIZES: Record<MapSizeId, { label: string; tiles: number; blurb: string }> = {
  small:  { label: 'Small',  tiles: 32, blurb: 'A border hamlet — tight land, quick walks' },
  medium: { label: 'Medium', tiles: 48, blurb: 'The classic republic' },
  large:  { label: 'Large',  tiles: 64, blurb: 'Room for heavy industry' },
  vast:   { label: 'Vast',   tiles: 96, blurb: 'A five-year megaproject' },
};

export type DifficultyId = 'easy' | 'normal' | 'hard';

export interface DifficultyDef {
  id: DifficultyId;
  label: string;
  blurb: string;
  icon: string; // GameIcon name
  startRubles: number;    // Moscow's hard-currency grant — pure trade capital
  startDollars: number;
  depotStockMult: number; // scales the starting depot stock
  importPriceMult: number; // scales importPriceOf — hard mode pays more abroad
}

export const DIFFICULTIES: Record<DifficultyId, DifficultyDef> = {
  easy:   { id: 'easy',   label: 'Comfortable', icon: 'coins', blurb: "Moscow's grant is generous, bloc suppliers offer fraternal discounts", startRubles: 4000, startDollars: 500, depotStockMult: 1.5, importPriceMult: 0.9 },
  normal: { id: 'normal', label: 'Planned',     icon: 'star',  blurb: 'The Plan provides exactly what the Plan provides',                    startRubles: 2500, startDollars: 250, depotStockMult: 1,   importPriceMult: 1 },
  hard:   { id: 'hard',   label: 'Austere',     icon: 'pick',  blurb: 'A lean grant, bare depots, and import agents who smell desperation', startRubles: 1500, startDollars: 100, depotStockMult: 0.6, importPriceMult: 1.2 },
};

export const DEFAULT_DIFFICULTY: DifficultyId = 'normal';

// ------------------------------------------------------------
// Objectives (Five-Year Plans)
// ------------------------------------------------------------

export interface ObjectiveDef {
  id: string;
  title: string;
  description: string;
  rewardRubles?: number;
  rewardDollars?: number;
}

export const OBJECTIVES: ObjectiveDef[] = [
  { id: 'roads', title: 'Lay the Foundation', description: 'Build 10 road tiles', rewardRubles: 200 },
  { id: 'shop', title: 'Feed the Masses', description: 'Open a State Store stocked with at least 5 food', rewardRubles: 250 },
  { id: 'housing', title: 'House the People', description: 'Reach 20 citizens', rewardRubles: 300 },
  { id: 'sow', title: 'Sow the Fields', description: 'Build a Collective Farm (food security before winter!)', rewardRubles: 300 },
  { id: 'builders', title: 'Bricks and Planks', description: 'Produce 20 planks and 20 bricks in total', rewardRubles: 350 },
  { id: 'firstMachines', title: 'First Machines', description: 'Import 5 machinery through the customs house', rewardRubles: 400 },
  { id: 'coal', title: 'Black Gold', description: 'Mine 30 coal in total', rewardRubles: 400 },
  { id: 'power', title: 'Electrification', description: 'Generate at least 8 MW of power', rewardRubles: 500 },
  { id: 'heat', title: 'Winter is Coming', description: 'Have a working Heating Plant before winter', rewardRubles: 400 },
  { id: 'steel', title: 'Steel for the Motherland', description: 'Produce 15 steel in total', rewardDollars: 150 },
  { id: 'foodchain', title: 'From Field to Table', description: 'Produce 25 food in total', rewardRubles: 400 },
  { id: 'export', title: 'Foreign Currency', description: 'Export goods worth 5,000 ₽ total', rewardDollars: 200 },
  { id: 'meansOfProduction', title: 'Means of Production', description: 'Build the Machine Works', rewardRubles: 800, rewardDollars: 200 },
  { id: 'autarky', title: 'Autarky', description: 'Produce 50 machinery — the republic no longer needs to buy its machines', rewardDollars: 500 },
  { id: 'pop150', title: 'A Growing Republic', description: 'Reach 150 citizens', rewardRubles: 600 },
  { id: 'flourish', title: 'The Republic Flourishes', description: 'Reach 300 citizens with happiness ≥ 65%', rewardRubles: 1000, rewardDollars: 300 },
];
