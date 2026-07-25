// ============================================================
// Red Republic — the mutation vocabulary
// ============================================================
// A system is a pure function of the world that returns what it wants to
// change. `applyMutations` is the ONLY thing that writes. That inversion is the
// point of the whole refactor: a system's effects become declared, auditable
// and impossible to exceed, so a new mechanic can no longer widen the blast
// radius of an old one by quietly reaching for a field.
//
// Every kind is NAMED after the effect it has. There is deliberately no generic
// `{ field, value }` escape hatch — that would be "write anything" with extra
// steps, and the guard test in `mutation-writeset.test.ts` would have nothing
// to check.
//
// Ordering is load-bearing. Systems emit in source order and the day loop
// applies in emission order, so float accumulation and event ids stay
// bit-identical to the hand-written code these replace.
import type { ResourceId } from './config';
import type { GameEvent, World } from './world';
import type { DayWeather } from './weather';

export type Mutation =
  // ---- weather ----
  /** Today's weather, read from the deterministic timeline. */
  | { k: 'weather'; weather: DayWeather }
  /** The four running streak counters, derived together and set together. */
  | { k: 'weatherStreaks'; dry: number; gloom: number; sun: number; frost: boolean }

  // ---- connectivity ----
  /** One building's freight-network reachability. */
  | { k: 'connectivity'; id: number; connected: boolean; roadConnected: boolean }

  // ---- national accounts ----
  /** The national stockpile table, recounted from every bin. */
  | { k: 'totals'; totals: Record<ResourceId, number> }

  // ---- the notice board ----
  /** A message for the player. Queued, not state — but it is an effect, so it
   *  is declared like one; that is what keeps a system from needing the engine. */
  | { k: 'event'; text: string; kind: GameEvent['kind']; icon?: string };

export type MutationKind = Mutation['k'];

/** Set by the write-set guard test to record what each system actually emits.
 *  Null in normal play, so this costs one null check per mutation. */
let journal: ((m: Mutation) => void) | null = null;

export function setMutationJournal(fn: ((m: Mutation) => void) | null): void {
  journal = fn;
}

export function applyMutations(w: World, muts: readonly Mutation[]): void {
  for (const m of muts) {
    journal?.(m);
    switch (m.k) {
      case 'weather':
        w.weather = m.weather;
        break;
      case 'weatherStreaks':
        w.dryStreak = m.dry; w.gloomStreak = m.gloom; w.sunStreak = m.sun; w.wasFrost = m.frost;
        break;
      case 'connectivity': {
        const b = w.buildings.get(m.id);
        if (b) { b.connected = m.connected; b.roadConnected = m.roadConnected; }
        break;
      }
      case 'totals':
        w.totals = m.totals;
        break;
      case 'event':
        w.pushEvent(m.text, m.kind, m.icon);
        break;
    }
  }
}
