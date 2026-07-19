import { describe, expect, it } from 'vitest';
import { soundForEvent } from '../event-sounds';
import { SFX_DEFS } from '../sfx';

describe('event → sound mapping', () => {
  it('maps the flagship events', () => {
    expect(soundForEvent('good', 'star')).toBe('objective');
    expect(soundForEvent('good', 'check')).toBe('complete');
    expect(soundForEvent('good', 'contract')).toBe('contractDone');
    expect(soundForEvent('bad', 'contract')).toBe('alertBad');
    expect(soundForEvent('info', 'contract')).toBe('contractOffer');
  });

  it('unmapped events stay silent', () => {
    expect(soundForEvent('info', 'spring')).toBeNull();
    expect(soundForEvent('info', 'star')).toBeNull();
    expect(soundForEvent('good', undefined)).toBeNull();
    expect(soundForEvent('weird', 'nonsense')).toBeNull();
  });

  it('every mapped sound has a synth recipe', () => {
    const kinds = ['good', 'bad', 'info'];
    const icons = ['star', 'check', 'contract', 'winter', 'freeze', 'summer', 'users', 'coins', 'rain', 'port', 'spring'];
    for (const k of kinds) {
      for (const i of icons) {
        const s = soundForEvent(k, i);
        if (s !== null) expect(SFX_DEFS[s]).toBeTypeOf('function');
      }
    }
  });
});
