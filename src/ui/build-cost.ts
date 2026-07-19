// Single source for "what does this building cost right now" — every build
// affordance (build menu, deposit panels, multi-select) displays the SAME
// terms the placement code will actually apply. Domestic construction costs
// no money: the bill is materials + labor; instant mode imports a Western
// prefab for dollars.
import { BUILDINGS, RESOURCES } from '@/game/config';
import type { ResourceId } from '@/game/config';
import type { GameEngine } from '@/game/engine';

function materialsText(defId: string): string {
  const entries = Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][];
  if (!entries.length) return 'labor only';
  return entries.map(([r, amt]) => `${amt} ${RESOURCES[r].name}`).join(', ');
}

export function buildCostText(engine: GameEngine, defId: string, instant: boolean): string {
  return instant
    ? `$${engine.instantCost(defId).toLocaleString()}`
    : materialsText(defId);
}

export function buildCostTotalText(engine: GameEngine, defId: string, count: number, instant: boolean): string {
  if (instant) return `$${(engine.instantCost(defId) * count).toLocaleString()}`;
  const entries = Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][];
  if (!entries.length) return 'labor only';
  return entries.map(([r, amt]) => `${amt * count} ${RESOURCES[r].name}`).join(', ');
}

/** Placement is only ever money-gated in instant mode — sites wait for materials. */
export function canAffordBuild(engine: GameEngine, defId: string, instant: boolean): boolean {
  return !instant || engine.dollars >= engine.instantCost(defId);
}

/** Advisory: materials the republic's TOTAL stockpile cannot currently cover.
 *  Placement still succeeds — the site waits for production to catch up. */
export function materialsShort(engine: GameEngine, defId: string): ResourceId[] {
  return (Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][])
    .filter(([r, amt]) => engine.totals[r] < amt)
    .map(([r]) => r);
}
