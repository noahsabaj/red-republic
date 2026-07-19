// Single source for "what does this building cost right now" — every build
// affordance (build menu, deposit panels, multi-select) displays the SAME
// terms the placement code will actually apply. Three pay modes:
//   materials — domestic construction: no money, just the materials + labor bill
//   instant   — import a Western prefab for dollars (no site, no wait)
//   autoBuy   — pay rubles now to import the exact bill, delivered from customs
import { BUILDINGS, RESOURCES } from '@/game/config';
import type { ResourceId } from '@/game/config';
import type { GameEngine } from '@/game/engine';

export type BuildPayMode = 'materials' | 'instant' | 'autoBuy';

function materialsText(defId: string): string {
  const entries = Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][];
  if (!entries.length) return 'labor only';
  return entries.map(([r, amt]) => `${amt} ${RESOURCES[r].name}`).join(', ');
}

export function buildCostText(engine: GameEngine, defId: string, mode: BuildPayMode): string {
  if (mode === 'instant') return `$${engine.instantCost(defId).toLocaleString()}`;
  if (mode === 'autoBuy') return `₽${engine.autoBuyImportCost(defId).toLocaleString()}`;
  return materialsText(defId);
}

export function buildCostTotalText(engine: GameEngine, defId: string, count: number, mode: BuildPayMode): string {
  if (mode === 'instant') return `$${(engine.instantCost(defId) * count).toLocaleString()}`;
  if (mode === 'autoBuy') return `₽${(engine.autoBuyImportCost(defId) * count).toLocaleString()}`;
  const entries = Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][];
  if (!entries.length) return 'labor only';
  return entries.map(([r, amt]) => `${amt * count} ${RESOURCES[r].name}`).join(', ');
}

/** Placement is money-gated only in instant ($) and auto-buy (₽) modes —
 *  plain material sites always "afford", they just wait for deliveries. */
export function canAffordBuild(engine: GameEngine, defId: string, mode: BuildPayMode): boolean {
  if (mode === 'instant') return engine.dollars >= engine.instantCost(defId);
  if (mode === 'autoBuy') return engine.rubles >= engine.autoBuyImportCost(defId);
  return true;
}

/** Advisory: materials the republic's TOTAL stockpile cannot currently cover.
 *  Placement still succeeds — the site waits for production to catch up. */
export function materialsShort(engine: GameEngine, defId: string): ResourceId[] {
  return (Object.entries(BUILDINGS[defId].materials) as [ResourceId, number][])
    .filter(([r, amt]) => engine.totals[r] < amt)
    .map(([r]) => r);
}
