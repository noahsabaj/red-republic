// ============================================================
// Pure playlist transport — ordering, shuffle, wrapping skip, and
// auto-advance selection. No WebAudio, no globals: the rng is injected
// (Math.random at runtime, a seeded generator in tests), so every branch
// is deterministic and node-testable. AudioSystem owns the actual timer
// and delegates the "what plays next" decision here.
// ============================================================

export type RepeatMode = 'off' | 'all' | 'one';

/** Identity order 0..n-1. */
export function naturalOrder(n: number): number[] {
  return Array.from({ length: n }, (_, i) => i);
}

/** Fisher–Yates permutation of 0..n-1 using the injected rng. */
export function shuffledOrder(n: number, rng: () => number): number[] {
  const a = naturalOrder(n);
  for (let i = n - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

/** Position within `order` holding a given playlist index (0 if absent). */
export function orderPos(order: number[], playlistIndex: number): number {
  const p = order.indexOf(playlistIndex);
  return p < 0 ? 0 : p;
}

/** Step a cursor position by ±1 with wrap-around. */
export function stepPos(pos: number, delta: 1 | -1, len: number): number {
  if (len <= 0) return 0;
  return ((pos + delta) % len + len) % len;
}

export interface AdvanceResult {
  pos: number;         // next cursor position within the order
  reshuffle: boolean;  // caller should rebuild a shuffled order (wrapped in 'all')
  stop: boolean;       // playlist ended ('off' at the last track)
  sameTrack: boolean;  // keep the current endless stream going ('one')
}

/**
 * Decide the next cursor after a track's play window elapses.
 * - 'one'  → hold on the current track (endless; caller just re-arms the timer)
 * - 'all'  → advance with wrap; ask for a reshuffle when it wraps to the top
 * - 'off'  → advance; stop at the end of the order
 */
export function autoAdvance(pos: number, len: number, repeat: RepeatMode): AdvanceResult {
  if (len <= 0) return { pos: 0, reshuffle: false, stop: true, sameTrack: false };
  if (repeat === 'one') return { pos, reshuffle: false, stop: false, sameTrack: true };
  const atEnd = pos + 1 >= len;
  if (repeat === 'off' && atEnd) return { pos, reshuffle: false, stop: true, sameTrack: false };
  const next = atEnd ? 0 : pos + 1;
  return { pos: next, reshuffle: atEnd, stop: false, sameTrack: false };
}
