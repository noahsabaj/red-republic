// ============================================================
// Game sessions: the engine plus the founding configuration, with a
// monotonically increasing id used as the React key that remounts
// GameCanvas (fresh camera, fresh input, fresh debug hook) whenever a
// new game starts or a save is loaded.
// ============================================================
import { GameEngine } from '@/game/engine';
import { seedDemoTown } from '@/game/demo';
import { MAP_SIZES, DEFAULT_CLIMATE, DEFAULT_DIFFICULTY, CLIMATES } from '@/game/config';
import type { ClimateId, DifficultyId, MapSizeId } from '@/game/config';
import type { SaveGameV1 } from '@/game/save-format';

export interface NewGameConfig {
  name: string;
  seed: number; // uint32
  mapSize: MapSizeId;
  climate: ClimateId;
  difficulty: DifficultyId;
}

export interface GameSession {
  engine: GameEngine;
  id: number;      // React key for GameCanvas remounts
  config: NewGameConfig;
  isNew: boolean;  // true => show the commissar's briefing once
}

export function createSession(cfg: NewGameConfig, id: number): GameSession {
  const size = MAP_SIZES[cfg.mapSize].tiles;
  const engine = new GameEngine({
    seed: cfg.seed,
    mapW: size,
    mapH: size,
    climate: cfg.climate,
    difficulty: cfg.difficulty,
    name: cfg.name,
  });
  return { engine, id, config: cfg, isNew: true };
}

export function sessionFromSave(save: SaveGameV1, id: number): GameSession {
  const engine = GameEngine.fromSave(save);
  const sizeEntry = (Object.entries(MAP_SIZES) as [MapSizeId, { tiles: number }][])
    .find(([, v]) => v.tiles === engine.mapW)?.[0] ?? 'medium';
  return {
    engine,
    id,
    config: {
      name: engine.name,
      seed: engine.seed,
      mapSize: sizeEntry,
      climate: engine.climate,
      difficulty: engine.difficulty,
    },
    isNew: false,
  };
}

/**
 * Dev workflow: ?demo and ?seed=N bypass the menu straight into gameplay
 * (?climate=<id> optionally picks a region). Returns null for the normal
 * boot-to-menu path. Side-effect-free w.r.t. globals — StrictMode runs
 * state initializers twice.
 */
export function bootFromUrl(): GameSession | null {
  const params = new URLSearchParams(window.location.search);
  const demo = params.has('demo');
  const seedParam = params.get('seed');
  if (!demo && seedParam === null) return null;
  const climateParam = params.get('climate');
  const climate: ClimateId = climateParam !== null && climateParam in CLIMATES ? climateParam as ClimateId : DEFAULT_CLIMATE;
  const cfg: NewGameConfig = {
    name: demo ? 'Demo Republic' : 'Red Republic',
    seed: demo ? 1961 : Number(seedParam) >>> 0,
    mapSize: 'medium',
    climate,
    difficulty: DEFAULT_DIFFICULTY,
  };
  const session = createSession(cfg, 1);
  if (demo) seedDemoTown(session.engine);
  session.isNew = false; // URL boots skip the briefing
  return session;
}

// UI-side randomness (names, seed rerolls) is outside the simulation — plain
// Math.random is fine here; determinism only binds sim code to the seeded rng.
const NAME_PREFIX = [
  'Krasno', 'Novo', 'Staro', 'Zhelezno', 'Verkhne', 'Nizhne',
  'Belo', 'Cherno', 'Severo', 'Yugo', 'Ugle', 'Pervo',
];
const NAME_ROOT = [
  'gorsk', 'grad', 'volsk', 'zavodsk', 'kamensk', 'ozersk',
  'retsk', 'polsk', 'morsk', 'stal', 'shakhtinsk', 'borsk',
];

export function randomRepublicName(): string {
  const pick = (a: string[]) => a[Math.floor(Math.random() * a.length)];
  return pick(NAME_PREFIX) + pick(NAME_ROOT);
}

export function randomSeed(): number {
  return Math.floor(Math.random() * 2 ** 31);
}
