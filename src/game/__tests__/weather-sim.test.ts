import { describe, expect, it } from 'vitest';
import { BALANCE, WEATHER } from '../config';
import { GameEngine } from '../engine';
import { WeatherTimeline } from '../weather';
import type { DayWeather, WeatherCondition } from '../weather';
import { flatMap, layRoad, makeEngine, placeBuilt, runDays } from './helpers';

const still = (condition: WeatherCondition, tempC = 15): Partial<DayWeather> =>
  ({ tempC, condition, snowDepth: 0, riverFrozen: false });

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
      // three full years, day 0 = January 1
      const byMonth = new Map<number, number[]>();
      for (let i = 0; i < 3 * 360; i++) {
        const month = (Math.floor(i / 30) % 12) + 1;
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
        const month = (Math.floor(i / 30) % 12) + 1;
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

describe('weather gameplay effects', () => {
  it('storms slow every vehicle already on the road', () => {
    const progressUnder = (condition: WeatherCondition) => {
      const e = makeEngine({ weather: () => still(condition) });
      e.trucks.push({
        id: 1, points: [{ x: 0, y: 0 }, { x: 30, y: 0 }], cargo: 'food', amount: 1,
        daysTotal: 30, daysDone: 0, phase: 'go', destId: 999, srcId: 999,
      });
      e.setSpeed(1);
      e.advance(e.TICK_MS); // exactly one day
      return e.trucks[0].daysDone;
    };
    expect(progressUnder('clear')).toBeCloseTo(1, 9);
    expect(progressUnder('storm')).toBeCloseTo(WEATHER.storm.truckMult, 9);
    expect(progressUnder('blizzard')).toBeCloseTo(WEATHER.blizzard.truckMult, 9);
  });

  it('a frozen river blocks new barge traffic; the thaw releases it; a barge caught out finishes its run', () => {
    let frozen = true;
    const e = new GameEngine({
      seed: 1, map: flatMap(), skipStartingBase: true,
      weatherScript: () => ({ tempC: 15, condition: 'clear', snowDepth: 0, riverFrozen: frozen }),
    });
    for (let y = 0; y < 48; y++) for (let x = 20; x <= 22; x++) e.tiles[y][x].terrain = 'water';
    const depot = placeBuilt(e, 'depot', 5, 10);
    placeBuilt(e, 'constructionOffice', 10, 10);
    placeBuilt(e, 'port', 18, 10);
    layRoad(e, 4, 9, 19, 9);
    depot.stock.planks = 60;
    depot.stock.bricks = 60;
    placeBuilt(e, 'port', 23, 10);
    layRoad(e, 23, 9, 32, 9);
    expect(e.tryPlace('house', 30, 10, false).ok).toBe(true);
    const site = e.buildingAt(30, 10)!;

    runDays(e, 20); // ice-locked: no barge may sail
    expect(e.boats.length).toBe(0);
    expect(site.constructed).toBe(false);

    frozen = false; // spring
    let sailed = false;
    for (let i = 0; i < 40 && !site.constructed; i++) {
      runDays(e, 1);
      if (e.boats.length > 0 && !sailed) {
        sailed = true;
        frozen = true; // freeze snaps shut mid-voyage — the barge must still finish
      }
    }
    expect(sailed).toBe(true);
    expect(site.constructed).toBe(true);
  });

  it('rain feeds the crops, frost stops them, drought withers them', () => {
    const script = { current: still('clear', 20) };
    const e = makeEngine({ weather: () => script.current });
    e.month = 7;
    layRoad(e, 4, 9, 20, 9);
    placeBuilt(e, 'depot', 5, 10);
    const farm = placeBuilt(e, 'farm', 10, 10);
    placeBuilt(e, 'apartment', 17, 10);
    e.pop = 40;
    runDays(e, 2); // staff the farm
    const cropsUnder = () => e.productionRates(farm).outputs.crops ?? 0;

    const clear = cropsUnder();
    expect(clear).toBeGreaterThan(0);

    script.current = still('rain', 20);
    runDays(e, 1);
    expect(cropsUnder()).toBeCloseTo(clear * WEATHER.rain.farmMult, 6);

    script.current = still('clear', -2); // frost
    runDays(e, 1);
    expect(cropsUnder()).toBe(0);

    script.current = still('clear', 25); // hot and dry
    runDays(e, BALANCE.droughtAfterDays + 8);
    const withered = cropsUnder();
    expect(withered).toBeLessThan(clear);
    expect(withered).toBeGreaterThanOrEqual(clear * 0.6 - 1e-9);

    script.current = still('rain', 20); // the drought breaks
    runDays(e, 1);
    expect(cropsUnder()).toBeCloseTo(clear * WEATHER.rain.farmMult, 6);
  });

  it('blizzards slow construction crews', () => {
    const progressAfterOneDay = (condition: WeatherCondition) => {
      const e = makeEngine({ weather: () => still(condition, condition === 'blizzard' ? -10 : 15) });
      layRoad(e, 4, 9, 14, 9);
      placeBuilt(e, 'depot', 5, 10);
      placeBuilt(e, 'constructionOffice', 10, 10); // contract crew of 10
      expect(e.tryPlace('house', 13, 10, false).ok).toBe(true);
      const site = e.buildingAt(13, 10)!;
      site.stock.planks = 6; // materials pre-delivered
      site.stock.bricks = 4;
      runDays(e, 1);
      return site.progress;
    };
    expect(progressAfterOneDay('clear')).toBeCloseTo(10, 9);
    expect(progressAfterOneDay('blizzard')).toBeCloseTo(10 * WEATHER.blizzard.buildMult, 9);
  });

  it('a long gray spell weighs on happiness; sunshine lifts it', () => {
    const moodAfter = (condition: WeatherCondition) => {
      const e = makeEngine({ weather: () => still(condition, 15) });
      runDays(e, 30);
      return e.happiness;
    };
    const sunny = moodAfter('clear');
    const grim = moodAfter('rain');
    expect(sunny).toBeGreaterThan(grim);
  });

  it('warns the day before a storm front arrives — the forecast does gameplay work', () => {
    // the game opens March 1 (day index 60); the storm hits index 62
    const e = makeEngine({
      weather: (idx) => (idx === 62 ? still('storm', 15) : still('clear', 15)),
    });
    runDays(e, 1); // now index 61; tomorrow is the storm
    expect(e.alerts.some(a => a.id === 'stormfront')).toBe(true);
    runDays(e, 2); // storm has passed
    expect(e.alerts.some(a => a.id === 'stormfront')).toBe(false);
  });
});
