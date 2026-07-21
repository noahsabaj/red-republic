/** Seconds → "M:SS" for the player's elapsed/total readout (clamped ≥ 0). */
export function fmtClock(sec: number): string {
  const s = Math.max(0, Math.floor(sec));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}
