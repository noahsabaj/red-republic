import { useSyncExternalStore } from 'react';
import { getSettings, subscribeSettings } from '@/app/settings';
import type { Settings } from '@/app/settings';

/** Live settings for React components; re-renders on any settings change. */
export function useSettings(): Settings {
  return useSyncExternalStore(subscribeSettings, getSettings);
}
