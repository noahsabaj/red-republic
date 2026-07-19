// ============================================================
// The pure brain of the interaction-sound layer: it maps a plain
// structural DESCRIPTOR of an activated control to a semantic sound
// family. No DOM, no WebAudio — the descriptor is built by the browser
// layer (ui-sounds.ts), so this stays unit-testable in the node harness.
//
// One family per semantic meaning; the DOM layer plays UI_VOICES[family]
// (in sfx.ts) on the bus given by FAMILY_BUS.
// ============================================================

/** Every interface-sound family. `hover` is played directly by the DOM
 *  layer (never returned by classify — hover comes from pointerover). */
export const UI_FAMILIES = [
  'neutral', 'confirm', 'back', 'open',
  'toggleOn', 'toggleOff', 'tab',
  'arm', 'commit', 'speed',
  'select', 'toolArm', 'toolCancel',
  'hover',
] as const;

export type UiFamily = typeof UI_FAMILIES[number];

const FAMILY_SET = new Set<string>(UI_FAMILIES);

/** Menu/interface families ride the Interface bus; world cues ride Effects. */
export const FAMILY_BUS: Record<UiFamily, 'interface' | 'effects'> = {
  neutral: 'interface', confirm: 'interface', back: 'interface', open: 'interface',
  toggleOn: 'interface', toggleOff: 'interface', tab: 'interface',
  arm: 'interface', commit: 'interface', speed: 'interface', hover: 'interface',
  select: 'effects', toolArm: 'effects', toolCancel: 'effects',
};

/** DOM-free snapshot of an activated control (built by ui-sounds.describe). */
export interface ElementDesc {
  tag: string;                 // lowercased tagName
  type: string | null;         // input type attribute
  disabled: boolean;           // disabled || aria-disabled==='true'
  role: string | null;
  ariaPressed: string | null;
  ariaChecked: string | null;  // from aria-checked, or normalized input.checked
  dataSfx: string | null;      // explicit family tag
  dataArmed: string | null;    // TwoStepButton arm state
}

/**
 * Classify an activated control into a sound family.
 * - `null`   → play nothing (disabled, sliders, file inputs, non-controls)
 * - `'none'` → explicitly silenced (its result sound is the feedback)
 *
 * Read in the CAPTURE phase, so aria-pressed/aria-checked still hold the
 * PRE-click value: a switch at aria-checked="false" is about to turn on,
 * hence false → toggleOn. Explicit data-sfx wins over inference.
 */
export function classify(d: ElementDesc): UiFamily | 'none' | null {
  if (d.disabled) return null;
  if (d.type === 'file' || d.type === 'range') return null;

  const t = d.dataSfx;
  if (t === 'none') return 'none';
  if (t === 'destructive') return d.dataArmed === 'true' ? 'commit' : 'arm';
  if (t === 'panel') return d.ariaPressed === 'true' ? 'back' : 'open';
  if (t === 'toggle') return d.ariaChecked === 'true' ? 'toggleOff' : 'toggleOn';
  if (t) return FAMILY_SET.has(t) ? (t as UiFamily) : 'neutral';

  // ---- inference fallback for untagged controls ----
  if (d.role === 'switch' || d.type === 'checkbox')
    return d.ariaChecked === 'true' ? 'toggleOff' : 'toggleOn';
  if (d.ariaPressed != null) return 'tab';
  if (d.tag === 'button' || d.role === 'button' || d.tag === 'a') return 'neutral';
  return null;
}
