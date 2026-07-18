// ============================================================
// Weather: a deterministic climate timeline per map seed.
//
// The engine owns one WeatherTimeline and reads DayWeather from it;
// the renderer and HUD only ever read engine.weather / engine.forecast().
// Generation is sequential and memoized, so "today" and the forecast
// come from the same source — the forecast is exact by construction.
// ============================================================
import { mulberry32 } from './mapgen';

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

type SeasonKey = 'winter' | 'spring' | 'summer' | 'autumn';

// Per-season target distribution over ABSTRACT (must sum to 1)
const COND_DIST: Record<SeasonKey, [number, number, number, number, number]> = {
  winter: [0.30, 0.30, 0.28, 0.05, 0.07],
  spring: [0.38, 0.26, 0.24, 0.05, 0.07],
  summer: [0.50, 0.20, 0.18, 0.08, 0.04],
  autumn: [0.28, 0.28, 0.24, 0.05, 0.15],
};

// Continental climate: mean +6 °C, ±18 swing → July ≈ +24, January ≈ −12.
const TEMP_MEAN = 6;
const TEMP_AMP = 18;
const PEAK_DOY = 134; // mid-July in day-of-year space (day 0 = March 1)

const FREEZE_AT = 30; // freeze-degree-days to lock the river…
const THAW_AT = 8;    // …and hysteresis so a one-day thaw can't flicker it open

function seasonOfMonth(month: number): SeasonKey {
  if (month === 12 || month <= 2) return 'winter';
  if (month <= 5) return 'spring';
  if (month <= 8) return 'summer';
  return 'autumn';
}

export class WeatherTimeline {
  private rnd: () => number;
  private days: DayWeather[] = [];
  // generator state, evolved one day at a time
  private noise = 0;            // fast AR(1) day-to-day wobble
  private front = 0;            // slow AR(1) multi-day cold snaps / warm spells
  private state: Abstract = 'clear';
  private freezeDD = 0;         // freeze-degree-day accumulator
  private frozen = false;
  private snow = 0;

  constructor(seed: number) {
    this.rnd = mulberry32((seed ^ 0x51c6a2b7) >>> 0); // decorrelated from map & economy streams
  }

  /** Weather for an absolute day index (0 = March 1, 1960; 30-day months). */
  at(index: number): DayWeather {
    while (this.days.length <= index) this.generateNext();
    return this.days[index];
  }

  private generateNext() {
    const idx = this.days.length;
    const month = ((Math.floor(idx / 30) + 2) % 12) + 1; // calendar month; index 0 is March
    const doy = idx % 360;

    // temperature: seasonal sinusoid + fast wobble + slow fronts
    this.noise = this.noise * 0.7 + (this.rnd() * 2 - 1) * 3.4;
    this.front = this.front * 0.93 + (this.rnd() * 2 - 1) * 1.5;
    const base = TEMP_MEAN + TEMP_AMP * Math.cos(((doy - PEAK_DOY) / 360) * 2 * Math.PI);
    const tempC = Math.round((base + this.noise + this.front) * 10) / 10;

    // condition: persist or re-roll from the season's distribution
    const persist = this.state === 'storm' ? 0.25 : 0.5; // storms blow through
    if (this.rnd() >= persist) {
      const dist = COND_DIST[seasonOfMonth(month)];
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
    if (condition === 'snow') this.snow += 1.2;
    else if (condition === 'blizzard') this.snow += 2.5;
    if (tempC > 0) this.snow -= tempC * 0.35;
    this.snow = Math.min(10, Math.max(0, Math.round(this.snow * 100) / 100));

    // river freeze: sustained cold locks it, sustained warmth breaks it up
    if (tempC < 0) this.freezeDD += -tempC;
    else this.freezeDD -= tempC * 2;
    this.freezeDD = Math.min(150, Math.max(0, this.freezeDD));
    if (!this.frozen && this.freezeDD >= FREEZE_AT) this.frozen = true;
    else if (this.frozen && this.freezeDD <= THAW_AT) this.frozen = false;

    this.days.push({ tempC, condition, snowDepth: this.snow, riverFrozen: this.frozen });
  }
}
