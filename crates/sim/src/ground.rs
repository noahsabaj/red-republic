//! The state of the ground: how wet it is, how frozen, and what that costs to
//! drive across.
//!
//! # Why this is state and not a function of the calendar
//!
//! [`crate::climate`] makes today's temperature and today's rain pure functions
//! of `(seed, day)`, so a forecast never perturbs anything. Soil is not like
//! that. Water that fell last week is still in the ground this week; snow that
//! fell in December is still lying in February and comes off all at once in
//! March. **Moisture and snow are accumulated state**, and modelling them as a
//! function of the date would throw away the only interesting thing about them.
//!
//! This is a deliberate departure from the plan, which proposed computing
//! softness purely from a window of recent days. Two things decided it. A
//! window long enough to hold a winter's snow is sixty days of substream draws
//! per query, and phase four asks this question once per vehicle per leg; and a
//! window cannot express the *carry* that makes a thaw — the pack has to
//! survive from one query to the next to be able to melt.
//!
//! What is kept from the plan is that nothing here is seasonal. Frost follows
//! the temperature, melt follows the temperature, and a warm February does the
//! same thing to the ground that a warm March does.
//!
//! # The spring thaw falls out; it is not written down
//!
//! Three rules, none of which mentions spring:
//!
//! - snow lies while it is below freezing and melts above it,
//! - meltwater goes into the topsoil like rain does,
//! - frost lags the air temperature, because soil has thermal mass.
//!
//! Run them through a winter and the worst going of the year lands a week or so
//! after the first warm spell: a season's snow arriving in the topsoil at once
//! while the frost that was holding the ground up is still on its way out. That
//! is the *rasputitsa*, and it is the seasonal event this whole model exists to
//! produce.

use crate::terrain::Surface;
use serde::{Deserialize, Serialize};

/// The temperature at which water freezes. Named rather than written as `0.0`
/// so the comparisons read as decisions rather than as sign checks.
pub const FREEZE_C: f64 = 0.0;

/// How far below freezing the air has to sit for the ground to be fully frozen.
pub const FROST_RANGE_C: f64 = 8.0;

/// How much of the gap to today's conditions the frost closes in a day.
///
/// Soil has thermal mass: one mild afternoon does not thaw a frozen field and
/// one cold night does not freeze a wet one. This lag is what makes the thaw a
/// period rather than a moment.
pub const FROST_LAG: f64 = 0.12;

/// Millimetres of water the topsoil holds when it is saturated.
pub const SATURATION_MM: f64 = 40.0;

/// Millimetres of snow that melt per degree above freezing, per day.
pub const MELT_PER_DEGREE_MM: f64 = 2.5;

/// Share of its water the topsoil gives up on a warm, snow-free day.
pub const DRYING_PER_DAY: f64 = 0.10;

/// How warm it has to be for drying to run at full rate, above freezing.
pub const DRYING_FULL_AT_C: f64 = 15.0;

/// How wet and how frozen the open ground is.
///
/// One figure for the whole republic. Weather is regional at this scale — a map
/// is ten kilometres across and it does not rain on half of it — so the
/// variation that matters is the *surface*, which is static and lives on the
/// terrain. See [`going`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ground {
    /// Water in the topsoil, `0.0` bone dry to `1.0` saturated.
    pub moisture: f64,
    /// Snow lying, in millimetres of water equivalent.
    pub snow: f64,
    /// How frozen the ground is, `0.0` soft to `1.0` set hard.
    pub frost: f64,
}

impl Default for Ground {
    /// A republic is founded on 1 March, at the end of a winter it did not
    /// simulate. Starting bone dry and unfrozen would hand every founding a
    /// spring that never happened; starting damp and part-frozen is the honest
    /// guess, and a week of real weather washes it out either way.
    fn default() -> Self {
        Self {
            moisture: 0.5,
            snow: 0.0,
            frost: 0.3,
        }
    }
}

impl Ground {
    /// Take one day of weather.
    pub fn advance(&mut self, temperature_c: f64, precipitation_mm: f64) {
        let target = ((FREEZE_C - temperature_c) / FROST_RANGE_C).clamp(0.0, 1.0);
        self.frost += (target - self.frost) * FROST_LAG;

        let freezing = temperature_c < FREEZE_C;
        let melt = if freezing {
            0.0
        } else {
            ((temperature_c - FREEZE_C) * MELT_PER_DEGREE_MM).min(self.snow)
        };
        // Below freezing it falls as snow and lies; above it, it runs straight
        // into the ground along with whatever the pack is giving up.
        let fell_as_snow = if freezing { precipitation_mm } else { 0.0 };
        self.snow = (self.snow + fell_as_snow - melt).max(0.0);

        let water = if freezing { 0.0 } else { precipitation_mm } + melt;
        self.moisture = (self.moisture + water / SATURATION_MM).min(1.0);

        // It dries out only when it is warm and there is nothing lying on top.
        if !freezing && self.snow <= 0.0 {
            let warmth = ((temperature_c - FREEZE_C) / DRYING_FULL_AT_C).clamp(0.0, 1.5);
            self.moisture = (self.moisture - DRYING_PER_DAY * warmth).max(0.0);
        }
    }

    /// How badly the open ground would bog a vehicle today: `0.0` firm, `1.0`
    /// impassable.
    ///
    /// **Frozen ground is hard however wet it is.** A frozen bog is a road, and
    /// that is not a quirk of the arithmetic — it is why winter haulage across
    /// country is easier than spring haulage, and why the thaw is the event
    /// rather than the rain.
    pub fn softness(&self) -> f64 {
        (self.moisture * (1.0 - self.frost)).clamp(0.0, 1.0)
    }

    /// What the going is on a particular surface today.
    pub fn going_on(&self, surface: Surface) -> f64 {
        (self.softness() * going(surface)).clamp(0.0, 1.0)
    }

    /// The same, rolled forward `days` from here.
    ///
    /// A forecast, and the reason it can exist at all is that temperature and
    /// rain are pure: rolling the recurrence forward from today costs one
    /// substream draw per day and moves nothing.
    pub fn forecast(
        &self,
        mut weather: impl FnMut(u64) -> (f64, f64),
        from_day: u64,
        days: u64,
    ) -> Ground {
        let mut ahead = *self;
        for step in 0..days {
            let (temperature, rain) = weather(from_day + step + 1);
            ahead.advance(temperature, rain);
        }
        ahead
    }
}

/// How much worse than open grass a surface is to cross.
///
/// A multiplier on the day's softness rather than a figure of its own, because
/// what varies by place is how badly the ground *takes* water, not how much
/// fell on it. Rock is the useful case: it is hard going and it never turns to
/// mud, so a stony route is the one that stays open in the thaw.
pub fn going(surface: Surface) -> f64 {
    match surface {
        Surface::Grass => 1.0,
        // Roots, stumps and no run-up. Worse than open field when it is wet.
        Surface::Forest => 1.3,
        // Hard on a lorry and hard under it. Softness barely touches it.
        Surface::Rock => 0.35,
        // Not going anywhere.
        Surface::Water => f64::INFINITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::climate::{ClimateId, precipitation_on, temperature_on};
    use crate::rng::Rng;
    use crate::time::DAYS_PER_YEAR;

    /// A year of weather on one climate, from a fixed seed, as
    /// `(day_of_year, temperature, rain, ground)` after each day.
    fn a_year(id: ClimateId, seed: u64, years: u32) -> Vec<(u32, f64, f64, Ground)> {
        let climate = id.def();
        let mut ground = Ground::default();
        let mut out = Vec::new();
        for day in 0..u64::from(DAYS_PER_YEAR) * u64::from(years) {
            let day_of_year = (day % u64::from(DAYS_PER_YEAR)) as u32;
            let mut stream = Rng::from_seed(seed ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let temperature = temperature_on(climate, day_of_year, stream.next_f64());
            let rain = precipitation_on(climate, day_of_year, stream.next_f64());
            ground.advance(temperature, rain);
            out.push((day_of_year, temperature, rain, ground));
        }
        out
    }

    #[test]
    fn rain_is_bursty_but_averages_to_what_was_authored() {
        let climate = ClimateId::Plains.def();
        let mut rng = Rng::from_seed(1961);
        let day = 180; // midsummer
        let mut total = 0.0;
        let mut dry = 0;
        const DAYS: u32 = 20_000;
        for _ in 0..DAYS {
            let fell = precipitation_on(climate, day, rng.next_f64());
            if fell <= 0.0 {
                dry += 1;
            }
            total += fell;
        }
        let mean = total / f64::from(DAYS);
        assert!(
            (mean - climate.rain_on(day)).abs() < 0.05,
            "mean {mean:.3} against the authored {:.3}",
            climate.rain_on(day)
        );
        let dry_share = f64::from(dry) / f64::from(DAYS);
        assert!(
            (dry_share - 0.7).abs() < 0.02,
            "{:.0}% of days were dry",
            dry_share * 100.0
        );
    }

    /// Frozen ground is hard however wet it is. This is the rule the whole
    /// seasonal shape hangs off.
    #[test]
    fn a_frozen_bog_is_a_road() {
        let soaked_and_frozen = Ground {
            moisture: 1.0,
            snow: 100.0,
            frost: 1.0,
        };
        assert_eq!(soaked_and_frozen.softness(), 0.0);
        let soaked_and_thawed = Ground {
            frost: 0.0,
            ..soaked_and_frozen
        };
        assert_eq!(soaked_and_thawed.softness(), 1.0);
    }

    #[test]
    fn dry_ground_is_firm_whatever_the_season() {
        let dry = Ground {
            moisture: 0.0,
            snow: 0.0,
            frost: 0.0,
        };
        assert_eq!(dry.softness(), 0.0);
    }

    /// The seasonal event the model exists to produce, and nothing in the code
    /// mentions spring.
    ///
    /// Over a simulated year the worst going must land in the weeks after the
    /// thaw begins — a winter's snow arriving in the topsoil at once while the
    /// frost that was holding the ground up is still on its way out.
    #[test]
    fn the_worst_going_of_the_year_is_the_spring_thaw() {
        for id in [ClimateId::Plains, ClimateId::Taiga] {
            // Two years, and read the second, so the ground is not still
            // carrying the founding guess.
            let year = a_year(id, 1961, 2);
            let second: Vec<_> = year[DAYS_PER_YEAR as usize..].to_vec();
            let (worst_day, worst) = second
                .iter()
                .map(|&(day, _, _, g)| (day, g.softness()))
                .fold(
                    (0, -1.0),
                    |best, next| if next.1 > best.1 { next } else { best },
                );

            // Months are thirty days here, so March is days 60..90 and May ends
            // at 150. The thaw should be in that window on both postings.
            assert!(
                (60..150).contains(&worst_day),
                "{}: the worst going of the year was day {worst_day} at {worst:.2}",
                id.def().name
            );
            assert!(worst > 0.5, "{}: the thaw was dry", id.def().name);

            // And midwinter must be *better* going than the thaw, which is the
            // counter-intuitive half.
            let midwinter = second
                .iter()
                .filter(|&&(day, _, _, _)| (0..30).contains(&day))
                .map(|&(_, _, _, g)| g.softness())
                .fold(0.0, f64::max);
            assert!(
                midwinter < worst,
                "{}: January ({midwinter:.2}) was worse going than the thaw ({worst:.2})",
                id.def().name
            );
        }
    }

    /// A dry hot posting should not be a mud bath, or the climates are not a
    /// choice about anything.
    #[test]
    fn the_steppe_is_firmer_going_than_the_maritime_coast() {
        let worst = |id: ClimateId| {
            a_year(id, 1961, 2)[DAYS_PER_YEAR as usize..]
                .iter()
                .map(|&(_, _, _, g)| g.softness())
                .fold(0.0, f64::max)
        };
        let mean = |id: ClimateId| {
            let year = a_year(id, 1961, 2);
            let second = &year[DAYS_PER_YEAR as usize..];
            second.iter().map(|&(_, _, _, g)| g.softness()).sum::<f64>() / second.len() as f64
        };
        assert!(
            mean(ClimateId::Steppe) < mean(ClimateId::Maritime),
            "steppe {:.2} against maritime {:.2}",
            mean(ClimateId::Steppe),
            mean(ClimateId::Maritime)
        );
        assert!(worst(ClimateId::Maritime) > 0.5, "the coast never got soft");
    }

    /// Snow has to actually pile up over a winter, or there is nothing to melt.
    #[test]
    fn a_taiga_winter_lays_snow_and_the_spring_takes_it_away() {
        let year = a_year(ClimateId::Taiga, 7, 2);
        let second = &year[DAYS_PER_YEAR as usize..];
        let deepest = second
            .iter()
            .map(|&(_, _, _, g)| g.snow)
            .fold(0.0, f64::max);
        assert!(deepest > 40.0, "only {deepest:.0} mm of snow all winter");
        let midsummer = second
            .iter()
            .find(|&&(day, _, _, _)| day == 190)
            .expect("the year has a midsummer")
            .3;
        assert_eq!(midsummer.snow, 0.0, "snow lying in July");
    }

    #[test]
    fn rock_stays_firm_when_grass_turns_to_mud() {
        let wet = Ground {
            moisture: 1.0,
            snow: 0.0,
            frost: 0.0,
        };
        assert_eq!(wet.going_on(Surface::Grass), 1.0);
        assert!(wet.going_on(Surface::Rock) < 0.5);
        assert!(wet.going_on(Surface::Forest) >= wet.going_on(Surface::Grass));
        assert_eq!(wet.going_on(Surface::Water), 1.0, "water is impassable");
    }

    /// A forecast is the same recurrence rolled forward, and asking for one
    /// moves nothing.
    #[test]
    fn a_forecast_is_what_the_next_days_will_actually_do() {
        let climate = ClimateId::Plains.def();
        let weather = |day: u64| {
            let mut stream = Rng::from_seed(11 ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let day_of_year = (day % u64::from(DAYS_PER_YEAR)) as u32;
            (
                temperature_on(climate, day_of_year, stream.next_f64()),
                precipitation_on(climate, day_of_year, stream.next_f64()),
            )
        };

        let mut lived = Ground::default();
        for day in 0..10u64 {
            let (t, r) = weather(day + 1);
            lived.advance(t, r);
        }
        let before = Ground::default();
        let forecast = before.forecast(weather, 0, 10);
        assert_eq!(forecast, lived, "the forecast was not what happened");
        assert_eq!(before, Ground::default(), "forecasting changed the ground");
    }
}
