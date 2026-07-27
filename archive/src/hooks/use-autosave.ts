import { useEffect } from 'react';
import { getSettings } from '@/app/settings';
import { writeAutosave } from '@/game/save-slots';
import type { GameSession } from '@/app/session';

/**
 * Day-driven autosave: subscribes to the engine and rotates a save into
 * the autosave slots every `autosaveIntervalDays` in-game days (0 = off).
 * The just-started (or just-loaded) state is never immediately re-saved.
 */
export function useAutosave(
  session: GameSession | null,
  onSaved: (dayIndex: number) => void,
  notify: (text: string, kind?: 'good' | 'bad' | 'info', icon?: string) => void,
) {
  useEffect(() => {
    if (!session) return;
    const engine = session.engine;
    let lastAutoDay = engine.dayIndex();
    return engine.subscribe(() => {
      const interval = getSettings().autosaveIntervalDays;
      if (interval === 0) return;
      const idx = engine.dayIndex();
      if (idx - lastAutoDay < interval) return;
      lastAutoDay = idx;
      const res = writeAutosave(engine.serialize());
      if (res.ok) {
        onSaved(idx);
        notify('Autosaved', 'info', 'save');
      } else {
        notify(`Autosave failed: ${res.message}`, 'bad');
      }
    });
  }, [session, onSaved, notify]);
}
