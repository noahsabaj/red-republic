// ============================================================
// Red Republic — player-facing number formatting
// ============================================================
// One place that names every KIND of number the player sees, so choosing a
// format is never a per-call-site guess (and never a forgotten `Math.floor`).
//
// The economy is BULK/continuous by design — stock is a float, and that is
// correct. "Wholeness" is not a property of the simulation; it's a property of
// the *edges* (what the player sees and what crosses the border). These helpers
// ARE that edge. Leaf module, zero deps: engine event strings and the React UI
// both import it.
//
// Scope: the economic numbers — resource quantities/levels/flows, shortfalls,
// money, ratios. Physical units (MW, °C) and prices (₽/unit) keep their own
// unit-appropriate formatting and are intentionally not funnelled here.

/**
 * A whole usable AMOUNT of a resource — what the player ships, spends, or holds.
 * Floors (you can't act on a partial tonne) and clamps ≥0 (a −1e-9 float must
 * never render "-1"). The default for stock, cargo, deliveries, sellable.
 */
export function fmtQty(n: number): string {
  return String(Math.max(0, Math.floor(n)));
}

/**
 * A precise stock LEVEL for a diagnostic readout (a building's bin gauge), where
 * the fraction is the point — you can watch a slow wear-drain tick down. One
 * decimal, clamped ≥0. Deliberately distinct from fmtQty: the microscope shows
 * the tenths the summary floors away. (This is what the StorageBar reading *is* —
 * a named kind, not an "exception" to whole-number display.)
 */
export function fmtLevel(n: number): string {
  return Math.max(0, n).toFixed(1);
}

/**
 * A per-day FLOW rate (net production, consumption). Keeps its natural sign; one
 * decimal while small, whole once |n| ≥ 10 (a big flow doesn't need tenths).
 * Callers prepend an explicit '+' where they want to stress a gain.
 */
export function fmtRate(n: number): string {
  return Math.abs(n) < 10 ? n.toFixed(1) : String(Math.round(n));
}

/**
 * A shortfall still OWED (contract undelivered, materials missing). Ceils — the
 * mirror of fmtQty's floor: what you *have* rounds down, what you *owe* rounds
 * up (a 0.2-unit shortfall is still a whole unit you must deliver). Clamps ≥0.
 */
export function fmtOwed(n: number): string {
  return String(Math.max(0, Math.ceil(n)));
}

/**
 * A MONEY magnitude (rubles/dollars) — border currency, always shown whole with
 * thousands grouping. Floors, clamps ≥0; the caller prepends the ₽/$ symbol and
 * any sign.
 */
export function fmtMoney(n: number): string {
  return Math.floor(Math.max(0, n)).toLocaleString('en-US');
}

/**
 * A RATIO in [0,1] rendered as a whole percent (efficiency, soured relations).
 * Rounds; the caller adds the '%'. For a value already on a 0–100 scale, this
 * is not the tool — pass a true ratio.
 */
export function fmtPct(ratio: number): string {
  return String(Math.round(ratio * 100));
}
