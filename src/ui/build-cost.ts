// Single source for "what does this building cost right now" — every build
// affordance (build menu, deposit panels, multi-select) displays the SAME
// purchasing mode the placement code will actually charge.
import { BUILDINGS } from '@/game/config';
import type { GameEngine } from '@/game/engine';

export function buildCostText(engine: GameEngine, defId: string, instant: boolean): string {
  return instant
    ? `$${engine.instantCost(defId).toLocaleString()}`
    : `₽${BUILDINGS[defId].costRubles.toLocaleString()}`;
}

export function buildCostTotalText(engine: GameEngine, defId: string, count: number, instant: boolean): string {
  return instant
    ? `$${(engine.instantCost(defId) * count).toLocaleString()}`
    : `₽${(BUILDINGS[defId].costRubles * count).toLocaleString()}`;
}

export function canAffordBuild(engine: GameEngine, defId: string, instant: boolean): boolean {
  return instant
    ? engine.dollars >= engine.instantCost(defId)
    : engine.rubles >= BUILDINGS[defId].costRubles;
}
