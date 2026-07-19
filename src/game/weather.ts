// ============================================================
// Weather: a deterministic climate timeline per map seed.
//
// The engine owns one WeatherTimeline and reads DayWeather from it;
// the renderer and HUD only ever read engine.weather / engine.forecast().
// Generation is sequential and memoized, so "today" and the forecast
// come from the same source — the forecast is exact by construction.
//
// The numeric climate (temperature curve, condition odds, freeze
// thresholds, snowfall rates) comes from a ClimateDef — the CLIMATES
// table in config.ts. The default preset reproduces the historical
// continental values exactly (climate.test.ts pins the stream).
// ============================================================
import { mulberry32 } from './mapgen';
import { CLIMATES } from './config';
import type { ClimateDef } from './config';

export type WeatherCondition = 'clear' | 'overcast' | 'rain' | 'snow' | 'storm' | 'blizzard' | 'fog';

export interface DayWeather {
  tempC: number;
  condition: WeatherCondition;
  snowDepth: number;   // 0..10 abstract cover; drives ground whitening
  riverFrozen: boolean;
}

// The Markov chain runs over abstract states; presentation derives from
// temperature (precipitation at ≤0 °C IS snow, a freezing storm IS a
// blizzard) so cold isn't a separate roll — it's the same front, colder.
const ABSTRACT = ['clear', 'overcast', 'precip', 'storm', 'fog'] as const;
type Abstract = (typeof ABSTRACT)[number];

export type SeasonKey = 'winter' | 'spring' | 'summer' | 'autumn';

/** Per-season probability rows over [clear, overcast, precip, storm, fog]; each sums to 1. */
export type CondDist = Record<SeasonKey, [number, number, number, number, number]>;

function seasonOfMonth(month: number): SeasonKey {
  if (month === 12 || month <= 2) return 'winter';
  if (month <= 5) return 'spring';
  if (month <= 8) return 'summer';
  return 'autumn';
}

export class WeatherTimeline {
  private rnd: () => number;
  private readonly c: ClimateDef;
  private days: DayWeather[] = [];
  // generator state, evolved one day at a time
  private noise = 0;            // fast AR(1) day-to-day wobble
  private front = 0;            // slow AR(1) multi-day cold snaps / warm spells
  private state: Abstract = 'clear';
  private freezeDD = 0;         // freeze-degree-day accumulator
  private frozen = false;
  private snow = 0;

  constructor(seed: number, climate: ClimateDef = CLIMATES.plains) {
    this.rnd = mulberry32((seed ^ 0x51c6a2b7) >>> 0); // decorrelated from map & economy streams
    this.c = climate;
  }

  /** Weather for an absolute day index (0 = January 1, 1960; 30-day months). */
  at(index: number): DayWeather {
    while (this.days.length <= index) this.generateNext();
    return this.days[index];
  }

  private generateNext() {
    const idx = this.days.length;
    const month = (Math.floor(idx / 30) % 12) + 1; // calendar month
    const doy = idx % 360;

    // temperature: seasonal sinusoid + fast wobble + slow fronts
    this.noise = this.noise * 0.7 + (this.rnd() * 2 - 1) * 3.4;
    this.front = this.front * 0.93 + (this.rnd() * 2 - 1) * 1.5;
    const base = this.c.tempMean + this.c.tempAmp * Math.cos(((doy - this.c.peakDoy) / 360) * 2 * Math.PI);
    const tempC = Math.round((base + this.noise + this.front) * 10) / 10;

    // condition: persist or re-roll from the season's distribution
    const persist = this.state === 'storm' ? 0.25 : 0.5; // storms blow through
    if (this.rnd() >= persist) {
      const dist = this.c.condDist[seasonOfMonth(month)];
      let roll = this.rnd();
      let next: Abstract = 'clear';
      for (let i = 0; i < ABSTRACT.length; i++) {
        roll -= dist[i];
        if (roll < 0) { next = ABSTRACT[i]; break; }
      }
      this.state = next;
    }
    const freezing = tempC <= 0;
    const condition: WeatherCondition =
      this.state === 'precip' ? (freezing ? 'snow' : 'rain')
      : this.state === 'storm' ? (freezing ? 'blizzard' : 'storm')
      : this.state;

    // snow cover accumulates in snowfall, melts with warmth
    if (condition === 'snow') this.snow += this.c.snowfallPerDay;
    else if (condition === 'blizzard') this.snow += this.c.blizzardPerDay;
    if (tempC > 0) this.snow -= tempC * this.c.meltRate;
    this.snow = Math.min(10, Math.max(0, Math.round(this.snow * 100) / 100));

    // river freeze: sustained cold locks it, sustained warmth breaks it up
    if (tempC < 0) this.freezeDD += -tempC;
    else this.freezeDD -= tempC * 2;
    this.freezeDD = Math.min(150, Math.max(0, this.freezeDD));
    if (!this.frozen && this.freezeDD >= this.c.freezeAt) this.frozen = true;
    else if (this.frozen && this.freezeDD <= this.c.thawAt) this.frozen = false;

    this.days.push({ tempC, condition, snowDepth: this.snow, riverFrozen: this.frozen });
  }
}
