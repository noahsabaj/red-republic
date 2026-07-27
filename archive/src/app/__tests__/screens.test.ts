import { describe, expect, it } from 'vitest';
import { MENU_ROOT, PLAYING, screenReducer } from '../screens';
import type { AppScreen, ScreenAction } from '../screens';

const step = (s: AppScreen, ...actions: ScreenAction[]) => actions.reduce(screenReducer, s);

describe('screenReducer', () => {
  it('navigates the main menu and BACK returns to root', () => {
    let s = step(MENU_ROOT, { type: 'MENU_GOTO', sub: 'new-game' });
    expect(s).toEqual({ phase: 'menu', sub: 'new-game' });
    s = step(s, { type: 'BACK' });
    expect(s).toEqual(MENU_ROOT);
    expect(step(MENU_ROOT, { type: 'BACK' })).toEqual(MENU_ROOT); // root BACK is a no-op
  });

  it('START_GAME always lands in playing with no overlay', () => {
    expect(step(MENU_ROOT, { type: 'START_GAME' })).toEqual(PLAYING);
    expect(step({ phase: 'playing', overlay: 'load' }, { type: 'START_GAME' })).toEqual(PLAYING);
  });

  it('pause opens only from unobstructed play and navigates sub-screens', () => {
    let s = step(PLAYING, { type: 'OPEN_PAUSE' });
    expect(s).toEqual({ phase: 'playing', overlay: 'root' });
    expect(step(s, { type: 'OPEN_PAUSE' })).toEqual(s); // idempotent while open
    s = step(s, { type: 'PAUSE_GOTO', sub: 'save' });
    expect(s).toEqual({ phase: 'playing', overlay: 'save' });
    // BACK from a sub-screen goes to the pause root, not to the game
    s = step(s, { type: 'BACK' });
    expect(s).toEqual({ phase: 'playing', overlay: 'root' });
    // BACK from the pause root resumes
    expect(step(s, { type: 'BACK' })).toEqual(PLAYING);
  });

  it('PAUSE_GOTO is inert without an open overlay; MENU_GOTO is inert in play', () => {
    expect(step(PLAYING, { type: 'PAUSE_GOTO', sub: 'save' })).toEqual(PLAYING);
    expect(step(PLAYING, { type: 'MENU_GOTO', sub: 'options' })).toEqual(PLAYING);
    expect(step(MENU_ROOT, { type: 'OPEN_PAUSE' })).toEqual(MENU_ROOT);
  });

  it('RESUME closes any pause overlay; EXIT_TO_MENU lands on the menu root', () => {
    expect(step({ phase: 'playing', overlay: 'confirm-exit' }, { type: 'RESUME' })).toEqual(PLAYING);
    expect(step({ phase: 'playing', overlay: 'confirm-exit' }, { type: 'EXIT_TO_MENU' })).toEqual(MENU_ROOT);
    expect(step({ phase: 'menu', sub: 'load' }, { type: 'RESUME' })).toEqual({ phase: 'menu', sub: 'load' });
  });

  it('confirm dialogs are pause sub-screens with standard BACK', () => {
    const s = step(PLAYING, { type: 'OPEN_PAUSE' }, { type: 'PAUSE_GOTO', sub: 'confirm-exit' });
    expect(s).toEqual({ phase: 'playing', overlay: 'confirm-exit' });
    expect(step(s, { type: 'BACK' })).toEqual({ phase: 'playing', overlay: 'root' });
  });

  it('confirm-quit (desktop close) is a pause sub-screen with standard BACK', () => {
    const s = step(PLAYING, { type: 'OPEN_PAUSE' }, { type: 'PAUSE_GOTO', sub: 'confirm-quit' });
    expect(s).toEqual({ phase: 'playing', overlay: 'confirm-quit' });
    expect(step(s, { type: 'BACK' })).toEqual({ phase: 'playing', overlay: 'root' });
  });

  it('the exit chooser is a pause sub-screen with standard BACK', () => {
    const s = step(PLAYING, { type: 'OPEN_PAUSE' }, { type: 'PAUSE_GOTO', sub: 'exit' });
    expect(s).toEqual({ phase: 'playing', overlay: 'exit' });
    expect(step(s, { type: 'BACK' })).toEqual({ phase: 'playing', overlay: 'root' });
  });
});
