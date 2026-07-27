import { useRef, useSyncExternalStore } from 'react';
import type { GameEngine } from '@/game/engine';

function sameSig(a: readonly unknown[], b: readonly unknown[]) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (!Object.is(a[i], b[i])) return false;
  return true;
}

/**
 * Subscribe to the engine but re-render only when the computed signature
 * (an array of primitives covering what the component displays) changes.
 * The component then reads the engine directly during render — the values
 * are fresh by that point.
 */
export function useEngineSignature(engine: GameEngine, compute: (e: GameEngine) => readonly unknown[]): void {
  const cache = useRef<{ version: number; sig: readonly unknown[] } | null>(null);
  useSyncExternalStore(
    (cb) => engine.subscribe(cb),
    () => {
      const version = engine.getVersion();
      if (!cache.current || cache.current.version !== version) {
        const sig = compute(engine);
        cache.current = {
          version,
          // keep the old array identity when nothing changed so React skips the re-render
          sig: cache.current && sameSig(cache.current.sig, sig) ? cache.current.sig : sig,
        };
      }
      return cache.current.sig;
    },
  );
}

/** Re-render on every engine change — for detail panels that mirror live state. */
export function useEngineVersion(engine: GameEngine): number {
  return useSyncExternalStore((cb) => engine.subscribe(cb), () => engine.getVersion());
}
