// ============================================================
// The app-level screen state machine. One pure reducer owns every
// menu/pause transition, so exactly one dialog can exist at a time
// and Escape has a single BACK semantic everywhere.
// ============================================================

export type MenuScreen = 'root' | 'new-game' | 'load' | 'options';
export type PauseScreen = 'root' | 'save' | 'load' | 'options' | 'exit' | 'confirm-exit' | 'confirm-restart' | 'confirm-quit';

export type AppScreen =
  | { phase: 'menu'; sub: MenuScreen }
  | { phase: 'playing'; overlay: PauseScreen | null };

export type ScreenAction =
  | { type: 'MENU_GOTO'; sub: MenuScreen }   // main-menu navigation
  | { type: 'START_GAME' }                   // (new game / load) -> playing, no overlay
  | { type: 'OPEN_PAUSE' }                   // playing -> pause root
  | { type: 'PAUSE_GOTO'; sub: PauseScreen } // pause sub-navigation
  | { type: 'RESUME' }                       // close the pause overlay
  | { type: 'BACK' }                         // Escape: sub -> parent root; pause root -> resume
  | { type: 'EXIT_TO_MENU' };                // abandon the session -> menu root

export const MENU_ROOT: AppScreen = { phase: 'menu', sub: 'root' };
export const PLAYING: AppScreen = { phase: 'playing', overlay: null };

export function screenReducer(s: AppScreen, a: ScreenAction): AppScreen {
  switch (a.type) {
    case 'MENU_GOTO':
      return s.phase === 'menu' ? { phase: 'menu', sub: a.sub } : s;
    case 'START_GAME':
      return PLAYING;
    case 'OPEN_PAUSE':
      return s.phase === 'playing' && s.overlay === null ? { phase: 'playing', overlay: 'root' } : s;
    case 'PAUSE_GOTO':
      return s.phase === 'playing' && s.overlay !== null ? { phase: 'playing', overlay: a.sub } : s;
    case 'RESUME':
      return s.phase === 'playing' ? PLAYING : s;
    case 'BACK':
      if (s.phase === 'menu') return s.sub === 'root' ? s : MENU_ROOT;
      if (s.overlay === null) return s;
      return s.overlay === 'root' ? PLAYING : { phase: 'playing', overlay: 'root' };
    case 'EXIT_TO_MENU':
      return MENU_ROOT;
  }
}
