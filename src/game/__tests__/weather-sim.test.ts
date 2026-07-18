import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import { WeatherTimeline } from '../weather';
import type { DayWeather } from '../weather';
import { flatMap, makeEngine, runDays } from './helpers';

describe('weather timeline', () => {
  it('is deterministic per seed and differs between seeds', () => {
    const a = new WeatherTimeline(7);
    const b = new WeatherTimeline(7);
    const c = new WeatherTimeline(8);
    const seq = (t: WeatherTimeline) =>
      Array.from({ length: 120 }, (_, i) => t.at(i));
    expect(seq(a)).toEqual(seq(b));
    expect(seq(a)).not.toEqual(seq(c));
  });

  it('produces a plausible continental climate', () => {
    for (const seed of [1, 42, 1961]) {
      const t = new WeatherTimeline(seed);
      // three full years, day 0 = March 1
      const byMonth = new Map<number, number[]>();
      for (let i = 0; i < 3 * 360; i++) {
        const month = ((Math.floor(i / 30) + 2) % 12) + 1;
        const w = t.at(i);
        byMonth.set(month, [...(byMonth.get(month) ?? []), w.tempC]);
        // presentation follows temperature: no snow above freezing-ish temps
        if (w.condition === 'snow' || w.condition === 'blizzard') {
          expect(w.tempC, `seed ${seed} day ${i}`).toBeLessThanOrEqual(0);
        }
        if (w.condition === 'rain' || w.condition === 'storm') {
          expect(w.tempC, `seed ${seed} day ${i}`).toBeGreaterThan(0);
        }
        expect(w.snowDepth).toBeGreaterThanOrEqual(0);
        expect(w.snowDepth).toBeLessThanOrEqual(10);
      }
      const mean = (xs: number[]) => xs.reduce((a2, b2) => a2 + b2, 0) / xs.length;
      const july = mean(byMonth.get(7)!);
      const january = mean(byMonth.get(1)!);
      expect(july - january, `seed ${seed}`).toBeGreaterThan(15);
      // the river locks in deep winter and runs free in summer
      const winterDays: DayWeather[] = [];
      const julyDays: DayWeather[] = [];
      for (let i = 0; i < 3 * 360; i++) {
        const month = ((Math.floor(i / 30) + 2) % 12) + 1;
        if (month === 1 || month === 2) winterDays.push(t.at(i));
        if (month === 7) julyDays.push(t.at(i));
      }
      expect(winterDays.some(w => w.riverFrozen), `seed ${seed} frozen in Jan/Feb`).toBe(true);
      expect(julyDays.every(w => !w.riverFrozen), `seed ${seed} thawed in July`).toBe(true);
    }
  });
});

describe('engine weather', () => {
  it('follows the timeline deterministically and the forecast is exact', () => {
    const real = () => new GameEngine({ seed: 5, map: flatMap(), skipStartingBase: true });
    const a = real();
    const b = real();
    const predicted = a.forecast(5);
    const seen: DayWeather[] = [];
    for (let i = 0; i < 5; i++) {
      runDays(a, 1);
      runDays(b, 1);
      seen.push({ ...a.weather });
      expect(a.weather).toEqual(b.weather);
    }
    expect(seen).toEqual(predicted);
  });

  it('helpers pin calm weather so unrelated tests stay deterministic', () => {
    const e = makeEngine();
    runDays(e, 3);
    expect(e.weather.condition).toBe('clear');
    expect(e.weather.tempC).toBe(15);
    expect(e.weather.riverFrozen).toBe(false);
  });
});
