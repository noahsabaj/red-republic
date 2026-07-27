// ============================================================
// Browser layer for the interaction-sound system and the SINGLE owner of
// press-driven UI sound: one capture-phase click listener classifies any
// activated control and plays its family (the pure brain is ui-catalog.ts),
// plus a throttled hover tick. Keyboard handlers call uiSound() directly —
// same dispatcher, never a competing structure.
// ============================================================
import { audio } from './audio-system';
import { classify } from './ui-catalog';
import type { ElementDesc, UiFamily } from './ui-catalog';

const SELECTOR = 'button,[role="button"],[role="switch"],[role="tab"],a[href],input[type="checkbox"],[data-sfx]';
const HOVER_MIN_MS = 70;

/** Snapshot the attributes classify() needs off a live element. */
export function describe(el: Element): ElementDesc {
  const type = el.getAttribute('type');
  return {
    tag: el.tagName.toLowerCase(),
    type,
    disabled: (el as HTMLButtonElement).disabled || el.getAttribute('aria-disabled') === 'true',
    role: el.getAttribute('role'),
    ariaPressed: el.getAttribute('aria-pressed'),
    // a native checkbox reports state via .checked, not aria-checked
    ariaChecked: el.getAttribute('aria-checked') ?? (type === 'checkbox' ? String((el as HTMLInputElement).checked) : null),
    dataSfx: el.getAttribute('data-sfx'),
    dataArmed: el.getAttribute('data-armed'),
  };
}

/** Imperative entry for keyboard handlers (Escape = back, etc.). */
export function uiSound(family: UiFamily): void {
  audio.ui(family);
}

/**
 * Install the delegated listeners; returns a cleanup removing both.
 * Capture phase is mandatory: a panel that calls stopPropagation on its
 * bubble-phase onClick (HelpOverlay) must not be able to swallow the sound.
 * `click` (not pointerdown) covers keyboard Enter/Space activation for free
 * and never fires on a drag down-stroke.
 */
export function installUiSounds(): () => void {
  const onPress = (e: MouseEvent) => {
    const el = (e.target as Element | null)?.closest?.(SELECTOR);
    if (!el) return;
    // a <label> wrapping a control re-dispatches to the inner input; sound that, not the label
    if (el.tagName === 'LABEL' && el.querySelector('input,button,select,textarea')) return;
    const fam = classify(describe(el));
    if (fam && fam !== 'none') audio.ui(fam);
  };

  let lastHover = -Infinity;
  let lastHoverEl: Element | null = null;
  const onHover = (e: PointerEvent) => {
    if (e.pointerType === 'touch') return;               // touch has no hover
    const el = (e.target as Element | null)?.closest?.(SELECTOR);
    if (!el || el === lastHoverEl) return;               // same control → no retrigger
    lastHoverEl = el;
    const now = performance.now();
    if (now - lastHover < HOVER_MIN_MS) return;          // sweeping a toolbar → throttle
    lastHover = now;
    const fam = classify(describe(el));
    if (fam && fam !== 'none') audio.uiHover();          // disabled/silenced controls stay quiet
  };

  window.addEventListener('click', onPress, true);       // CAPTURE
  window.addEventListener('pointerover', onHover, true);
  return () => {
    window.removeEventListener('click', onPress, true);
    window.removeEventListener('pointerover', onHover, true);
  };
}
