// Reads today's weather off the deterministic timeline and keeps the running
// streaks that gameplay hangs off: drought (dry days), morale (gloom/sun) and
// the frost warning. Pure — the multipliers themselves are read straight from
// `w.weather` by whoever needs them (see `farmWeatherMult`), never recomputed.
import { BALANCE, FARM_SEASON, WEATHER } from '../config';
import type { Mutation, MutationKind } from '../mutation';
import type { World } from '../world';

/** Every mutation kind this system is allowed to emit. `mutation-writeset.test.ts`
 *  fails the build if it emits anything else — the enforcement that keeps a new
 *  mechanic from quietly widening this one's blast radius. */
export const WRITES: MutationKind[] = ['weather', 'weatherStreaks', 'event'];

export function weather(w: World): Mutation[] {
  const out: Mutation[] = [];
  const prev = w.weather;
  const today = w.weatherAt(w.dayIndex());
  out.push({ k: 'weather', weather: today });

  const hasFarms = [...w.buildings.values()].some(b => w.def(b).isFarm && b.constructed);

  // drought bookkeeping: hot rainless days accumulate, any precipitation resets
  let dry = w.dryStreak;
  const wet = today.condition === 'rain' || today.condition === 'storm'
    || today.condition === 'snow' || today.condition === 'blizzard';
  if (wet) {
    if (dry > BALANCE.droughtAfterDays && hasFarms)
      out.push({ k: 'event', text: 'Rain breaks the drought — the fields recover.', kind: 'good', icon: 'rain' });
    dry = 0;
  } else if (today.tempC >= 18) {
    dry++;
    if (dry === BALANCE.droughtAfterDays + 1 && hasFarms)
      out.push({ k: 'event', text: 'Drought — the fields are withering.', kind: 'bad', icon: 'summer' });
  }

  // frost: one warning per cold spell while crops are growing
  const frost = today.tempC < 0 && (FARM_SEASON[w.month] ?? 0) > 0;
  if (frost && !w.wasFrost && hasFarms)
    out.push({ k: 'event', text: 'Frost grips the fields — crops stop growing.', kind: 'bad', icon: 'freeze' });

  // morale streaks: long gray spells wear people down, sunny runs lift them
  let gloom = w.gloomStreak, sun = w.sunStreak;
  const mood = WEATHER[today.condition].morale;
  if (mood < 0) { gloom++; sun = 0; }
  else if (mood > 0) { sun++; gloom = 0; }
  else { gloom = Math.max(0, gloom - 1); sun = Math.max(0, sun - 1); }
  out.push({ k: 'weatherStreaks', dry, gloom, sun, frost });

  // river freeze-over / break-up
  if (w.hasWater && today.riverFrozen !== prev.riverFrozen) {
    if (today.riverFrozen)
      out.push({ k: 'event', text: 'The river has frozen over — barges are ice-locked until the thaw.', kind: 'bad', icon: 'freeze' });
    else
      out.push({ k: 'event', text: 'The ice breaks up — barges can sail again.', kind: 'good', icon: 'port' });
  }
  return out;
}
