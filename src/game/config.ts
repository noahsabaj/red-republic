// ============================================================
// Red Republic — Planned Economy Builder
// Game configuration: resources, buildings, prices, objectives
// ============================================================

export type ResourceId =
  | 'coal' | 'ironOre' | 'steel'
  | 'oil' | 'fuel'
  | 'wood' | 'planks'
  | 'gravel' | 'bricks'
  | 'crops' | 'food' | 'clothes';

export const ALL_RESOURCES: ResourceId[] = [
  'coal', 'ironOre', 'steel', 'oil', 'fuel',
  'wood', 'planks', 'gravel', 'bricks',
  'crops', 'food', 'clothes',
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
  coal:    { id: 'coal',    name: 'Coal',      icon: '⚫', color: '#3a3a3a', priceEast: 2.5,  priceWest: 1 },
  ironOre: { id: 'ironOre', name: 'Iron Ore',  icon: '🟤', color: '#8a4b2f', priceEast: 3,    priceWest: 1.5 },
  steel:   { id: 'steel',   name: 'Steel',     icon: '🔩', color: '#9aa5b1', priceEast: 14,   priceWest: 8 },
  oil:     { id: 'oil',     name: 'Oil',       icon: '🛢️', color: '#22252a', priceEast: 5,    priceWest: 3 },
  fuel:    { id: 'fuel',    name: 'Fuel',      icon: '⛽', color: '#c9a227', priceEast: 10,   priceWest: 6 },
  wood:    { id: 'wood',    name: 'Wood',      icon: '🪵', color: '#7a5230', priceEast: 2,    priceWest: 1 },
  planks:  { id: 'planks',  name: 'Planks',    icon: '🟫', color: '#a4713b', priceEast: 5,    priceWest: 2.5 },
  gravel:  { id: 'gravel',  name: 'Gravel',    icon: '🪨', color: '#8d8d8d', priceEast: 1.5,  priceWest: 0.7 },
  bricks:  { id: 'bricks',  name: 'Bricks',    icon: '🧱', color: '#b0472f', priceEast: 6,    priceWest: 3 },
  crops:   { id: 'crops',   name: 'Crops',     icon: '🌾', color: '#d4b545', priceEast: 2,    priceWest: 1 },
  food:    { id: 'food',    name: 'Food',      icon: '🍞', color: '#d98e4a', priceEast: 4.5,  priceWest: 2 },
  clothes: { id: 'clothes', name: 'Clothes',   icon: '🧥', color: '#5b7fb5', priceEast: 9,    priceWest: 5 },
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
  costRubles: number;
  // construction
  materials: Partial<Record<ResourceId, number>>;
  labor: number; // total worker-days to construct
  // operation
  workers: number; // jobs at full staffing
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
    id: 'road', name: 'Road', icon: '🛣️', category: 'infra', size: [1, 1],
    costRubles: 6, materials: {}, labor: 2, workers: 0, power: 0, heat: 0,
    storage: {}, boxHeight: 0, color: '#5c5c5c', wallColor: '#5c5c5c',
    description: 'Gravel road. Buildings must touch a road to receive deliveries. Trucks haul goods along roads.',
  }),

  // ---------- Housing ----------
  house: B({
    id: 'house', name: 'Small House', icon: '🏠', category: 'housing', size: [1, 1],
    costRubles: 400, materials: { planks: 6, bricks: 4 }, labor: 60,
    workers: 0, power: 0.3, heat: 0.5, storage: {},
    housingCapacity: 8, boxHeight: 12, color: '#b0483a', wallColor: '#e2d3b3',
    description: 'A modest family house for 8 citizens. Needs power, heat in winter, and a shop nearby.',
  }),
  apartment: B({
    id: 'apartment', name: 'Apartment Block', icon: '🏢', category: 'housing', size: [2, 2],
    costRubles: 2400, materials: { planks: 10, bricks: 30, steel: 6 }, labor: 300,
    workers: 0, power: 1.2, heat: 2, storage: {},
    housingCapacity: 40, boxHeight: 30, color: '#8f3d31', wallColor: '#c9b18a',
    description: 'A proud socialist prefab block housing 40 citizens.',
  }),

  // ---------- Industry ----------
  woodcutter: B({
    id: 'woodcutter', name: 'Woodcutter Post', icon: '🪓', category: 'industry', size: [1, 1],
    costRubles: 500, materials: { planks: 4 }, labor: 50,
    workers: 6, power: 0, heat: 0, storage: { wood: 30 },
    outputs: { wood: 4 }, requiresForest: true, pollution: 1,
    boxHeight: 10, color: '#4a6b3a', wallColor: '#8a6b45',
    description: 'Lumberjacks fell trees nearby. Place close to forest. Produces wood.',
  }),
  sawmill: B({
    id: 'sawmill', name: 'Sawmill', icon: '🪚', category: 'industry', size: [1, 1],
    costRubles: 900, materials: { bricks: 10, planks: 6, steel: 2 }, labor: 120,
    workers: 6, power: 1, heat: 0, storage: { wood: 20, planks: 30 },
    inputs: { wood: 2 }, outputs: { planks: 3 }, pollution: 1,
    boxHeight: 14, color: '#7a5230', wallColor: '#b08b5e',
    description: 'Saws 2 wood into 3 planks per day. Planks are a core construction material.',
  }),
  gravelQuarry: B({
    id: 'gravelQuarry', name: 'Gravel Quarry', icon: '⛏️', category: 'industry', size: [1, 1],
    costRubles: 600, materials: { planks: 6, bricks: 4 }, labor: 80,
    workers: 8, power: 0.5, heat: 0, storage: { gravel: 40 },
    outputs: { gravel: 5 }, requiresDeposit: 'gravel', pollution: 2,
    boxHeight: 8, color: '#6d6d6d', wallColor: '#9a9a9a',
    description: 'Must be built on a gravel deposit. Extracts gravel.',
  }),
  brickworks: B({
    id: 'brickworks', name: 'Brickworks', icon: '🧱', category: 'industry', size: [1, 1],
    costRubles: 1100, materials: { bricks: 12, steel: 4, planks: 4 }, labor: 130,
    workers: 10, power: 1.5, heat: 0, storage: { gravel: 25, bricks: 35 },
    inputs: { gravel: 3 }, outputs: { bricks: 4 }, pollution: 2,
    boxHeight: 16, color: '#8a3226', wallColor: '#b0604a',
    description: 'Fires 3 gravel into 4 bricks per day. Bricks are needed for almost everything.',
  }),
  coalMine: B({
    id: 'coalMine', name: 'Coal Mine', icon: '⚒️', category: 'industry', size: [1, 1],
    costRubles: 1500, materials: { bricks: 15, steel: 6, planks: 4 }, labor: 200,
    workers: 14, power: 2, heat: 0, storage: { coal: 60 },
    outputs: { coal: 6 }, requiresDeposit: 'coal', pollution: 3,
    boxHeight: 12, color: '#2e2e2e', wallColor: '#4f4f4f',
    description: 'Must be built on a coal deposit. Coal feeds power and heating plants and the steel mill.',
  }),
  ironMine: B({
    id: 'ironMine', name: 'Iron Ore Mine', icon: '⛏️', category: 'industry', size: [1, 1],
    costRubles: 1700, materials: { bricks: 15, steel: 6, planks: 4 }, labor: 200,
    workers: 14, power: 2, heat: 0, storage: { ironOre: 60 },
    outputs: { ironOre: 5 }, requiresDeposit: 'ironOre', pollution: 3,
    boxHeight: 12, color: '#6e3a24', wallColor: '#8a5a40',
    description: 'Must be built on an iron ore deposit.',
  }),
  steelMill: B({
    id: 'steelMill', name: 'Steel Mill', icon: '🏭', category: 'industry', size: [2, 2],
    costRubles: 4000, materials: { bricks: 30, steel: 15, planks: 8 }, labor: 400,
    workers: 30, power: 6, heat: 0, storage: { ironOre: 40, coal: 40, steel: 40 },
    inputs: { ironOre: 2, coal: 1 }, outputs: { steel: 1.5 }, pollution: 4,
    boxHeight: 24, color: '#5a5f66', wallColor: '#7d838c',
    description: 'Smelts 2 iron ore + 1 coal into 1.5 steel daily. Steel sells well abroad.',
  }),
  oilPump: B({
    id: 'oilPump', name: 'Oil Pump', icon: '🛢️', category: 'industry', size: [1, 1],
    costRubles: 2500, materials: { bricks: 12, steel: 10 }, labor: 220,
    workers: 10, power: 2, heat: 0, storage: { oil: 50 },
    outputs: { oil: 4 }, requiresDeposit: 'oil', pollution: 2,
    boxHeight: 18, color: '#1e2126', wallColor: '#3a3f46',
    description: 'Must be built on an oil deposit. Pumps crude oil.',
  }),
  refinery: B({
    id: 'refinery', name: 'Oil Refinery', icon: '⚗️', category: 'industry', size: [2, 2],
    costRubles: 4500, materials: { bricks: 30, steel: 18, planks: 6 }, labor: 420,
    workers: 25, power: 5, heat: 0, storage: { oil: 40, fuel: 40 },
    inputs: { oil: 3 }, outputs: { fuel: 2 }, pollution: 3,
    boxHeight: 22, color: '#8c7a2a', wallColor: '#a89a4a',
    description: 'Refines 3 oil into 2 fuel per day. Fuel earns hard currency in the West.',
  }),
  powerPlant: B({
    id: 'powerPlant', name: 'Coal Power Plant', icon: '⚡', category: 'industry', size: [2, 2],
    costRubles: 3500, materials: { bricks: 25, steel: 12, planks: 6 }, labor: 350,
    workers: 15, power: 0, powerOutput: 12, heat: 0,
    storage: { coal: 50 }, inputs: { coal: 2 }, pollution: 3,
    boxHeight: 26, color: '#4e5661', wallColor: '#6b7480',
    description: 'Burns 2 coal daily to generate 12 MW. Without power, industry stalls and homes go dark.',
  }),
  heatingPlant: B({
    id: 'heatingPlant', name: 'Heating Plant', icon: '🔥', category: 'industry', size: [1, 1],
    costRubles: 1800, materials: { bricks: 18, steel: 8, planks: 4 }, labor: 180,
    workers: 8, power: 1, heatOutput: 8, heat: 0,
    storage: { coal: 40 }, inputs: { coal: 1 }, pollution: 2,
    boxHeight: 16, color: '#7a3b2a', wallColor: '#9c5a44',
    description: 'Burns 1 coal daily for 8 heat. Citizens freeze without it from October to March.',
  }),
  farm: B({
    id: 'farm', name: 'Collective Farm', icon: '🌾', category: 'industry', size: [2, 2],
    costRubles: 1200, materials: { planks: 12, bricks: 8 }, labor: 150,
    workers: 10, power: 0.5, heat: 0, storage: { crops: 80 },
    outputs: { crops: 6 }, isFarm: true, pollution: 0,
    boxHeight: 8, color: '#8a6b3a', wallColor: '#c9a86b',
    description: 'Yields crops from open ground around it. Sowing in spring, harvest late summer–autumn. Nothing grows in winter!',
  }),
  foodFactory: B({
    id: 'foodFactory', name: 'Food Factory', icon: '🍞', category: 'industry', size: [1, 1],
    costRubles: 2000, materials: { bricks: 18, steel: 6, planks: 6 }, labor: 200,
    workers: 12, power: 2, heat: 0, storage: { crops: 40, food: 40 },
    inputs: { crops: 2.5 }, outputs: { food: 2.5 }, pollution: 1,
    boxHeight: 16, color: '#b06a2a', wallColor: '#d09a5a',
    description: 'Bakes 2.5 crops into 2.5 food daily. Citizens without food will leave — or worse.',
  }),
  textileMill: B({
    id: 'textileMill', name: 'Textile Mill', icon: '🧵', category: 'industry', size: [1, 1],
    costRubles: 1800, materials: { bricks: 16, steel: 5, planks: 6 }, labor: 180,
    workers: 12, power: 2, heat: 0, storage: { crops: 30, clothes: 30 },
    inputs: { crops: 2 }, outputs: { clothes: 1.2 }, pollution: 1,
    boxHeight: 16, color: '#3a5a8a', wallColor: '#6b8ab5',
    description: 'Weaves 2 crops into 1.2 clothes daily. Dressed citizens are happy citizens.',
  }),

  // ---------- Services ----------
  store: B({
    id: 'store', name: 'State Store', icon: '🏪', category: 'services', size: [1, 1],
    costRubles: 700, materials: { planks: 6, bricks: 8 }, labor: 80,
    workers: 3, power: 0.5, heat: 0, storage: { food: 40, clothes: 20 },
    serviceRadius: 8, serviceType: 'shop',
    boxHeight: 12, color: '#3a6b4f', wallColor: '#d8cdb0',
    description: 'Sells food and clothes to citizens within 8 tiles. Keep it stocked by road!',
  }),
  clinic: B({
    id: 'clinic', name: 'Polyclinic', icon: '🏥', category: 'services', size: [1, 1],
    costRubles: 1500, materials: { bricks: 14, steel: 4, planks: 6 }, labor: 150,
    workers: 6, power: 1, heat: 0, storage: {},
    serviceRadius: 8, serviceType: 'health',
    boxHeight: 14, color: '#b5b5b5', wallColor: '#e8e8e8',
    description: 'Free healthcare for citizens within 8 tiles. Healthy workers work harder.',
  }),
  pub: B({
    id: 'pub', name: 'Culture Club', icon: '🍺', category: 'services', size: [1, 1],
    costRubles: 900, materials: { planks: 8, bricks: 10 }, labor: 100,
    workers: 4, power: 0.5, heat: 0, storage: {},
    serviceRadius: 8, serviceType: 'culture',
    boxHeight: 12, color: '#6b4a8a', wallColor: '#9a7ab5',
    description: 'Beer, chess and patriotic cinema within 8 tiles. Raises happiness.',
  }),

  // ---------- Trade & Storage ----------
  warehouse: B({
    id: 'warehouse', name: 'Warehouse', icon: '📦', category: 'trade', size: [1, 1],
    costRubles: 800, materials: { planks: 8, bricks: 10 }, labor: 90,
    workers: 2, power: 0.2, heat: 0,
    storage: { coal: 40, ironOre: 40, steel: 40, oil: 40, fuel: 40, wood: 40, planks: 40, gravel: 40, bricks: 40, crops: 40, food: 40, clothes: 40 },
    boxHeight: 14, color: '#7a6a4a', wallColor: '#a89878',
    description: 'Open storage for 40 units of every good. Trucks haul surplus here.',
  }),
  depot: B({
    id: 'depot', name: 'Council Depot', icon: '🏛️', category: 'trade', size: [2, 2],
    costRubles: 1000, materials: { bricks: 15, planks: 10 }, labor: 120,
    workers: 4, power: 0.5, heat: 0, isDepot: true,
    storage: { coal: 120, ironOre: 120, steel: 120, oil: 120, fuel: 120, wood: 120, planks: 120, gravel: 120, bricks: 120, crops: 120, food: 120, clothes: 120 },
    boxHeight: 18, color: '#8a2a2a', wallColor: '#c9b890',
    description: 'Central storage of the republic. Holds 120 of every good.',
  }),
  constructionOffice: B({
    id: 'constructionOffice', name: 'Construction Office', icon: '🏗️', category: 'infra', size: [1, 1],
    costRubles: 1200, materials: { bricks: 10, planks: 8 }, labor: 110,
    workers: 10, power: 0.5, heat: 0, storage: {},
    isConstructionOffice: true,
    boxHeight: 12, color: '#b0802a', wallColor: '#d0aa5a',
    description: 'Employs builders and operates trucks. No office, no construction, no haulage.',
  }),
  customs: B({
    id: 'customs', name: 'Customs House', icon: '🛃', category: 'trade', size: [2, 2],
    costRubles: 2000, materials: { bricks: 20, steel: 6, planks: 8 }, labor: 200,
    workers: 8, power: 1, heat: 0, isCustoms: true,
    storage: { coal: 80, ironOre: 80, steel: 80, oil: 80, fuel: 80, wood: 80, planks: 80, gravel: 80, bricks: 80, crops: 80, food: 80, clothes: 80 },
    boxHeight: 16, color: '#2a4a7a', wallColor: '#5a7aaa',
    description: 'Foreign trade terminal. Imports arrive here; exports leave from here. Must connect by road.',
  }),
};

export const BUILD_LIST: string[] = [
  'road', 'constructionOffice',
  'house', 'apartment',
  'woodcutter', 'sawmill', 'gravelQuarry', 'brickworks',
  'coalMine', 'ironMine', 'steelMill', 'oilPump', 'refinery',
  'powerPlant', 'heatingPlant', 'farm', 'foodFactory', 'textileMill',
  'store', 'clinic', 'pub',
  'warehouse', 'depot', 'customs',
];

// Instant-build with western dollars: costs this multiplier vs rubles
export const DOLLAR_BUILD_RATE = 0.09; // dollars per ruble of building cost

// ------------------------------------------------------------
// Balance constants
// ------------------------------------------------------------

export const BALANCE = {
  startRubles: 40000,
  startDollars: 3000,
  workerShare: 0.7,       // share of population that can work
  wagePerWorker: 0.4,     // rubles per worker per day
  foodPerCitizen: 0.015,  // per day
  clothesPerCitizen: 0.004,
  truckCapacity: 6,
  truckDaysPerTile: 0.18, // travel days per road tile
  maxActiveTrucksPerOffice: 6,
  buildersPerSite: 10,    // max builders on one site per day
  shopRadius: 8,
  serviceRadius: 8,
  pollutionRadius: 6,
  winterMonths: [10, 11, 12, 1, 2, 3], // heat required
  months: ['January','February','March','April','May','June','July','August','September','October','November','December'],
};

// Farm seasonal factor by month (1-12): sowing, growth, harvest
export const FARM_SEASON: Record<number, number> = {
  1: 0, 2: 0, 3: 0.2, 4: 0.3, 5: 0.35, 6: 0.5,
  7: 0.7, 8: 1.0, 9: 1.2, 10: 1.0, 11: 0.15, 12: 0,
};

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
  { id: 'roads', title: 'Lay the Foundation', description: 'Build 10 road tiles', rewardRubles: 800 },
  { id: 'shop', title: 'Feed the Masses', description: 'Open a State Store stocked with at least 5 food', rewardRubles: 1000 },
  { id: 'housing', title: 'House the People', description: 'Reach 20 citizens', rewardRubles: 1200 },
  { id: 'sow', title: 'Sow the Fields', description: 'Build a Collective Farm (food security before winter!)', rewardRubles: 1500 },
  { id: 'builders', title: 'Bricks and Planks', description: 'Produce 20 planks and 20 bricks in total', rewardRubles: 1500 },
  { id: 'coal', title: 'Black Gold', description: 'Mine 30 coal in total', rewardRubles: 1500 },
  { id: 'power', title: 'Electrification', description: 'Generate at least 8 MW of power', rewardRubles: 2500 },
  { id: 'heat', title: 'Winter is Coming', description: 'Have a working Heating Plant before winter', rewardRubles: 2000 },
  { id: 'steel', title: 'Steel for the Motherland', description: 'Produce 15 steel in total', rewardDollars: 1000 },
  { id: 'foodchain', title: 'From Field to Table', description: 'Produce 25 food in total', rewardRubles: 2500 },
  { id: 'export', title: 'Foreign Currency', description: 'Export goods worth 5,000 ₽ total', rewardDollars: 1500 },
  { id: 'pop300', title: 'A Growing Republic', description: 'Reach 150 citizens', rewardRubles: 5000 },
  { id: 'flourish', title: 'The Republic Flourishes', description: 'Reach 300 citizens with happiness ≥ 65%', rewardRubles: 10000, rewardDollars: 3000 },
];
