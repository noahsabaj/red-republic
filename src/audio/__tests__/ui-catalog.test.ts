import { describe, expect, it } from 'vitest';
import { classify, FAMILY_BUS, UI_FAMILIES } from '../ui-catalog';
import type { ElementDesc } from '../ui-catalog';
import { SFX_BUS, SFX_DEFS, UI_VOICES } from '../sfx';
import type { SfxName } from '../sfx';

/** An ElementDesc with everything neutral, overridable per-test. */
const desc = (over: Partial<ElementDesc> = {}): ElementDesc => ({
  tag: 'button', type: null, disabled: false, role: null,
  ariaPressed: null, ariaChecked: null, dataSfx: null, dataArmed: null,
  ...over,
});

describe('classify — silence rules', () => {
  it('never sounds a disabled control (even when tagged)', () => {
    expect(classify(desc({ disabled: true }))).toBeNull();
    expect(classify(desc({ disabled: true, dataSfx: 'confirm' }))).toBeNull();
  });
  it('ignores sliders and file inputs', () => {
    expect(classify(desc({ tag: 'input', type: 'range' }))).toBeNull();
    expect(classify(desc({ tag: 'input', type: 'file' }))).toBeNull();
  });
  it('data-sfx="none" is explicitly silent', () => {
    expect(classify(desc({ dataSfx: 'none' }))).toBe('none');
  });
  it('a non-control element with nothing to go on is null', () => {
    expect(classify(desc({ tag: 'div' }))).toBeNull();
  });
});

describe('classify — explicit tags (data-sfx wins)', () => {
  it('destructive splits on arm state (capture reads pre-click)', () => {
    expect(classify(desc({ dataSfx: 'destructive', dataArmed: 'false' }))).toBe('arm');
    expect(classify(desc({ dataSfx: 'destructive' }))).toBe('arm');            // not yet armed
    expect(classify(desc({ dataSfx: 'destructive', dataArmed: 'true' }))).toBe('commit');
  });
  it('panel splits on aria-pressed: closed→open, open→back', () => {
    expect(classify(desc({ dataSfx: 'panel', ariaPressed: 'false' }))).toBe('open');
    expect(classify(desc({ dataSfx: 'panel', ariaPressed: 'true' }))).toBe('back');
  });
  it('toggle splits on aria-checked (switch) or aria-pressed (pressed toggle button)', () => {
    expect(classify(desc({ dataSfx: 'toggle', ariaChecked: 'false' }))).toBe('toggleOn');
    expect(classify(desc({ dataSfx: 'toggle', ariaChecked: 'true' }))).toBe('toggleOff');
    // a pressed ToggleButton carries aria-pressed, not aria-checked
    expect(classify(desc({ dataSfx: 'toggle', ariaPressed: 'false' }))).toBe('toggleOn');
    expect(classify(desc({ dataSfx: 'toggle', ariaPressed: 'true' }))).toBe('toggleOff');
  });
  it('a direct family passes through; an unknown tag degrades to neutral', () => {
    expect(classify(desc({ dataSfx: 'confirm' }))).toBe('confirm');
    expect(classify(desc({ dataSfx: 'speed' }))).toBe('speed');
    expect(classify(desc({ dataSfx: 'bogus' }))).toBe('neutral');
  });
});

describe('classify — inference fallback for untagged controls', () => {
  it('a switch / native checkbox reads its pre-click checked state', () => {
    expect(classify(desc({ role: 'switch', ariaChecked: 'false' }))).toBe('toggleOn');
    expect(classify(desc({ role: 'switch', ariaChecked: 'true' }))).toBe('toggleOff');
    expect(classify(desc({ tag: 'input', type: 'checkbox', ariaChecked: 'false' }))).toBe('toggleOn');
  });
  it('an aria-pressed control is a tab/segment', () => {
    expect(classify(desc({ ariaPressed: 'false' }))).toBe('tab');
    expect(classify(desc({ ariaPressed: 'true' }))).toBe('tab');
  });
  it('a plain button or link is neutral', () => {
    expect(classify(desc())).toBe('neutral');
    expect(classify(desc({ tag: 'a' }))).toBe('neutral');
    expect(classify(desc({ tag: 'span', role: 'button' }))).toBe('neutral');
  });
});

describe('catalog completeness — one voice + one bus per name', () => {
  it('every UI family has a voice recipe and a bus', () => {
    for (const f of UI_FAMILIES) {
      expect(UI_VOICES[f], `voice for ${f}`).toBeTypeOf('function');
      expect(FAMILY_BUS[f], `bus for ${f}`).toMatch(/^(interface|effects)$/);
    }
  });
  it('world cues ride the Effects bus, everything else Interface', () => {
    expect(FAMILY_BUS.select).toBe('effects');
    expect(FAMILY_BUS.toolArm).toBe('effects');
    expect(FAMILY_BUS.toolCancel).toBe('effects');
    expect(FAMILY_BUS.neutral).toBe('interface');
    expect(FAMILY_BUS.hover).toBe('interface');
  });
  it('every outcome effect has a recipe and a bus', () => {
    for (const name of Object.keys(SFX_DEFS) as SfxName[]) {
      expect(SFX_DEFS[name], `recipe for ${name}`).toBeTypeOf('function');
      expect(SFX_BUS[name], `bus for ${name}`).toMatch(/^(interface|effects)$/);
    }
    expect(SFX_BUS.quicksave).toBe('interface'); // the one interface outcome
  });
});
