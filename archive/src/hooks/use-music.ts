import { useSyncExternalStore } from 'react';
import { audio } from '@/audio';
import type { MusicState } from '@/audio';

/** Live now-playing state for the music player UI. Re-renders on track /
 *  play / shuffle / repeat changes (including auto-advance). */
export function useMusicState(): MusicState {
  return useSyncExternalStore(audio.subscribeMusic, audio.musicState);
}
