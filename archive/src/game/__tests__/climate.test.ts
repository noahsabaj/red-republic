import { describe, expect, it } from 'vitest';
import { WeatherTimeline } from '../weather';
import type { ClimateId } from '../config';
import { BALANCE, CLIMATES } from '../config';
import { GameEngine } from '../engine';
import { isGameIcon } from '../../ui/icons';
import { flatMap } from './helpers';

// ============================================================
// Determinism tripwire: the default climate's weather stream is pinned. The
// climate-preset refactor must reproduce today's sequences exactly for the
// default (plains) preset — same rnd() draw order, same constants. If this
// fails, existing seeds' weather changed: fix the change, don't repin.
// ============================================================

const PINNED_FIRST_10 = [
  { tempC: -8.7, condition: 'clear', snowDepth: 0, riverFrozen: false },
  { tempC: -7.5, condition: 'clear', snowDepth: 0, riverFrozen: false },
  { tempC: -7.9, condition: 'clear', snowDepth: 0, riverFrozen: false },
  { tempC: -10.4, condition: 'snow', snowDepth: 1.2, riverFrozen: true },
  { tempC: -11.6, condition: 'snow', snowDepth: 2.4, riverFrozen: true },
  { tempC: -9.7, condition: 'snow', snowDepth: 3.6, riverFrozen: true },
  { tempC: -11.3, condition: 'blizzard', snowDepth: 6.1, riverFrozen: true },
  { tempC: -8.5, condition: 'overcast', snowDepth: 6.1, riverFrozen: true },
  { tempC: -9.5, condition: 'overcast', snowDepth: 6.1, riverFrozen: true },
  { tempC: -10.1, condition: 'overcast', snowDepth: 6.1, riverFrozen: true },
];

const ALL_CLIMATES = Object.keys(CLIMATES) as ClimateId[];

function run(seed: number, climate: ClimateId, days: number) {
  const tl = new WeatherTimeline(seed, CLIMATES[climate]);
  return Array.from({ length: days }, (_, i) => tl.at(i));
}

describe('climate determinism', () => {
  it('default timeline reproduces the pinned seed-7 stream', () => {
    const tl = new WeatherTimeline(7);
    const days = Array.from({ length: 10 }, (_, i) => tl.at(i));
    expect(days).toEqual(PINNED_FIRST_10);
  });

  it('the default constructor is exactly the plains preset', () => {
    const bare = new WeatherTimeline(7);
    const plains = new WeatherTimeline(7, CLIMATES.plains);
    for (let i = 0; i < 240; i++) expect(bare.at(i)).toEqual(plains.at(i));
  });

  it('same seed + same climate is reproducible; different climates differ', () => {
    for (const c of ALL_CLIMATES) {
      expect(run(42, c, 240)).toEqual(run(42, c, 240));
    }
    const streams = ALL_CLIMATES.map(c => JSON.stringify(run(42, c, 240)));
    expect(new Set(streams).size).toBe(ALL_CLIMATES.length);
  });
});

describe('climate config sanity', () => {
  it('every condDist row sums to 1 and every icon exists', () => {
    for (const c of ALL_CLIMATES) {
      const def = CLIMATES[c];
      for (const row of Object.values(def.condDist)) {
        expect(row.reduce((a, b) => a + b, 0)).toBeCloseTo(1, 9);
      }
      expect(isGameIcon(def.icon)).toBe(true);
      expect(def.id).toBe(c);
      expect(def.thawAt).toBeLessThan(def.freezeAt);
    }
  });
});

describe('climate characterization (statistical, 3 years x 3 seeds)', () => {
  const YEARS = 3;
  const stats = (climate: ClimateId, seed: number) => {
    const days = run(seed, climate, 360 * YEARS);
    const jan = days.filter((_, i) => i % 360 < 30);
    const janMean = jan.reduce((a, d) => a + d.tempC, 0) / jan.length;
    const midwinter = days.filter((_, i) => i % 360 < 60); // Jan + Feb
    const frozenShare = midwinter.filter(d => d.riverFrozen).length / midwinter.length;
    const snowDays = days.filter(d => d.condition === 'snow' || d.condition === 'blizzard').length;
    return { days, janMean, frozenShare, snowDays };
  };

  for (const seed of [1, 42, 1961]) {
    it(`seed ${seed}: taiga is coldest, steppe mild, maritime nearly ice-free`, () => {
      const taiga = stats('taiga', seed);
      const plains = stats('plains', seed);
      const steppe = stats('steppe', seed);
      const maritime = stats('maritime', seed);

      expect(taiga.janMean).toBeLessThan(plains.janMean);
      expect(plains.janMean).toBeLessThan(steppe.janMean);
      expect(maritime.frozenShare).toBeLessThan(0.05);
      expect(taiga.frozenShare).toBeGreaterThan(plains.frozenShare);
      expect(taiga.snowDays).toBeGreaterThan(plains.snowDays);

      // steppe: at least one summer dry spell long enough to trigger drought
      let dry = 0, worstDry = 0;
      steppe.days.forEach((d, i) => {
        const doy = i % 360;
        const summer = doy >= 150 && doy < 240; // Jun-Aug
        if (summer && d.condition !== 'rain' && d.condition !== 'storm') dry++;
        else dry = 0;
        worstDry = Math.max(worstDry, dry);
      });
      expect(worstDry).toBeGreaterThanOrEqual(BALANCE.droughtAfterDays);
    });
  }
});

describe('engine climate passthrough', () => {
  it('engine weather matches its climate timeline and defaults to plains', () => {
    const e = new GameEngine({ seed: 5, climate: 'taiga', map: flatMap(), skipStartingBase: true });
    expect(e.climate).toBe('taiga');
    const tl = new WeatherTimeline(5, CLIMATES.taiga);
    expect(e.weather).toEqual(tl.at(e.dayIndex()));
    expect(e.forecast(3)).toEqual([1, 2, 3].map(i => tl.at(e.dayIndex() + i)));

    const d = new GameEngine({ seed: 5, map: flatMap(), skipStartingBase: true });
    expect(d.climate).toBe('plains');
  });
});
