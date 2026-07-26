// ============================================================
// The seeded random generator, shared by every layer that needs a
// reproducible stream: map generation, the economy, the weather timeline —
// and the score, which must produce the same performance every play.
//
// It lives here rather than in game/mapgen.ts because the audio layer needs
// it too, and audio is not allowed to reach into the simulation. Sharing the
// FACTORY is not sharing a stream: each caller seeds its own, and the
// decorrelation constants at the call sites are what keep them independent.
// ============================================================

/** A deterministic 0..1 generator. State is exposed so save/load can restore
 *  a stream's exact position mid-sequence. */
export type SeededRng = (() => number) & { getState(): number; setState(s: number): void };

export function mulberry32(seed: number): SeededRng {
  let a = seed >>> 0;
  const next = () => {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return Object.assign(next, {
    getState: () => a >>> 0,
    setState: (s: number) => { a = s >>> 0; },
  });
}
