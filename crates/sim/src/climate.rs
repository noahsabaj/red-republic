//! Weather, to the extent the simulation needs it: how cold it is today.
//!
//! # Why the republic needs a temperature at all
//!
//! Because heating must not be driven by the calendar. That was an explicit
//! rule in the archived build — *"never derive weather from `season()`"* — and
//! it is the difference between a January that always costs the same and a
//! January that can catch a republic short. A cold snap in a mild month is the
//! event worth simulating, and a month index cannot produce one.
//!
//! # A table of monthly means, not a sinusoid
//!
//! The archived build modelled the year as `mean + amplitude * cos(...)`. This
//! carries the same climates as **twelve authored monthly means**, derived from
//! that sinusoid so the balance is unchanged, for two reasons.
//!
//! The first is arithmetic. `cos` is a libm function permitted to differ in its
//! last bit between platforms; a one-ULP difference in today's temperature
//! changes heat demand, which changes coal burnt, which changes what the power
//! plant has left. The running simulation only has to reproduce for the same
//! binary, so that would be *legal* — but it is a needless hazard for something
//! that costs nothing to avoid, and interpolating a table is `+ - * /` only.
//!
//! The second is that a table is honest data. It can express a late spring or a
//! long autumn, which a symmetric curve cannot, and it puts the climate where
//! balance belongs — next to the fields it relates to, editable without
//! touching a system.
//!
//! # The daily draw is its own stream
//!
//! Day-to-day variation comes from a substream keyed by the day index, never
//! from the simulation's main generator. The archived build learned this with
//! contract offers: drawing from the economy stream meant that merely *looking*
//! at the weather would shift every later economic roll. Being a pure function
//! of `(seed, day)` also means the forecast can be computed for any future day
//! without advancing anything.

use crate::time::{DAYS_PER_MONTH, DAYS_PER_YEAR, MONTHS_PER_YEAR};
use serde::{Deserialize, Serialize};

/// Outdoor temperature below which buildings need heat. Ported.
pub const HEAT_THRESHOLD_C: f64 = 8.0;

/// The temperature at which heat demand reaches 100%. Colder over-drives it,
/// which is what makes a deep cold snap eat a stockpile rather than merely
/// match it.
pub const HEAT_DESIGN_C: f64 = -15.0;

/// The most heat demand a cold snap can drive, as a share of nominal.
pub const HEAT_DEMAND_CEILING: f64 = 1.25;

/// Which climate a republic was posted to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClimateId {
    Plains,
    Taiga,
    Steppe,
    Maritime,
}

impl ClimateId {
    pub const ALL: [ClimateId; 4] = [
        ClimateId::Plains,
        ClimateId::Taiga,
        ClimateId::Steppe,
        ClimateId::Maritime,
    ];

    pub fn def(self) -> &'static Climate {
        CLIMATES
            .iter()
            .find(|c| c.id == self)
            .expect("every climate is in the table — guarded by a test")
    }
}

/// One climate's authored year.
#[derive(Debug, Clone, PartialEq)]
pub struct Climate {
    pub id: ClimateId,
    pub name: &'static str,
    /// Mean temperature in each month, January first, in degrees Celsius.
    pub monthly_mean_c: [f64; 12],
    /// How far a single day may stray from its month's mean, either way.
    pub daily_swing_c: f64,
    /// Mean precipitation in each month, January first, in millimetres per day.
    ///
    /// Authored beside the temperature it shares a year with, because the two
    /// together are what make a climate somewhere: the taiga is cold and dry,
    /// the maritime posting is mild and wet, and those are different problems.
    pub monthly_rain_mm: [f64; 12],
}

/// The four postings, carried over from the archived build.
///
/// The means were derived from its `tempMean + tempAmp * cos()` curve sampled
/// at each month's midpoint, so a republic feels the same year it always did.
pub const CLIMATES: &[Climate] = &[
    Climate {
        id: ClimateId::Plains,
        name: "Central Plains",
        monthly_mean_c: [
            -12.0, -9.4, -2.7, 6.3, 15.3, 21.7, 24.0, 21.4, 14.7, 5.7, -3.3, -9.4,
        ],
        daily_swing_c: 6.0,
        monthly_rain_mm: [1.0, 0.9, 1.0, 1.3, 1.6, 2.2, 2.4, 2.0, 1.6, 1.4, 1.3, 1.1],
    },
    Climate {
        id: ClimateId::Taiga,
        name: "Northern Taiga",
        monthly_mean_c: [
            -24.0, -20.9, -12.7, -1.6, 9.3, 17.2, 20.0, 16.9, 8.7, -2.4, -13.3, -20.9,
        ],
        daily_swing_c: 7.0,
        monthly_rain_mm: [0.8, 0.7, 0.7, 0.9, 1.3, 2.0, 2.4, 2.1, 1.7, 1.4, 1.1, 0.9],
    },
    Climate {
        id: ClimateId::Steppe,
        name: "Southern Steppe",
        monthly_mean_c: [
            -6.0, -3.7, 2.2, 10.3, 18.2, 24.0, 26.0, 23.7, 17.8, 9.7, 1.8, -3.7,
        ],
        daily_swing_c: 6.0,
        monthly_rain_mm: [0.7, 0.7, 0.9, 1.2, 1.5, 1.4, 1.2, 1.0, 1.0, 1.0, 0.9, 0.8],
    },
    Climate {
        id: ClimateId::Maritime,
        name: "Western Maritime",
        monthly_mean_c: [
            2.0, 3.1, 6.1, 10.1, 14.1, 17.0, 18.0, 16.9, 13.9, 9.9, 5.9, 3.1,
        ],
        daily_swing_c: 4.0,
        monthly_rain_mm: [2.6, 2.2, 2.0, 1.8, 1.7, 1.7, 1.8, 2.0, 2.3, 2.7, 2.9, 2.8],
    },
];

/// Read a twelve-month table on a given day of the year, interpolating between
/// the two months either side of it.
///
/// Each authored figure sits at the middle of its month, so the curve passes
/// through the table rather than stepping between plateaus — a 1 January that
/// is a whole degree different from 31 December would be a calendar artefact,
/// and those are what this module exists to avoid.
fn read_monthly(table: &[f64; 12], day_of_year: u32) -> f64 {
    let day = f64::from(day_of_year % DAYS_PER_YEAR);
    let per_month = f64::from(DAYS_PER_MONTH);
    // Position along the year in months, with month m's figure at m + 0.5.
    let position = day / per_month - 0.5;
    let months = f64::from(MONTHS_PER_YEAR);
    // `rem_euclid` keeps late December interpolating into January rather than
    // falling off the end of the table.
    let wrapped = position.rem_euclid(months);
    let index = wrapped.floor();
    let frac = wrapped - index;
    let a = table[(index as usize) % 12];
    let b = table[((index as usize) + 1) % 12];
    a + (b - a) * frac
}

impl Climate {
    /// The seasonal mean on a given day of the year, interpolated between the
    /// two months either side of it.
    ///
    /// Each authored mean sits at the middle of its month, so the curve passes
    /// through the table rather than stepping between plateaus — a 1 January
    /// that is a whole degree different from 31 December would be a calendar
    /// artefact, and those are what this module exists to avoid.
    pub fn mean_on(&self, day_of_year: u32) -> f64 {
        read_monthly(&self.monthly_mean_c, day_of_year)
    }

    /// The seasonal mean rainfall on a given day, in millimetres, read from the
    /// table the same way the temperature is.
    pub fn rain_on(&self, day_of_year: u32) -> f64 {
        read_monthly(&self.monthly_rain_mm, day_of_year)
    }

    /// The coldest month's mean — what a founding briefing quotes, because it
    /// is the number that decides how much coal a winter costs.
    pub fn coldest_mean_c(&self) -> f64 {
        self.monthly_mean_c
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
    }

    pub fn warmest_mean_c(&self) -> f64 {
        self.monthly_mean_c
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
    }
}

/// Whether it is cold enough out that buildings need heating.
pub fn heating_required(temperature_c: f64) -> bool {
    temperature_c < HEAT_THRESHOLD_C
}

/// Share of nominal heat demand today, `0.0..=1.25`.
///
/// Mild days sip fuel and deep cold over-drives, which is the whole reason
/// heating is temperature-driven rather than seasonal: the demand curve has to
/// be able to spike.
pub fn heat_demand_factor(temperature_c: f64) -> f64 {
    if !heating_required(temperature_c) {
        return 0.0;
    }
    let span = HEAT_THRESHOLD_C - HEAT_DESIGN_C;
    ((HEAT_THRESHOLD_C - temperature_c) / span).min(HEAT_DEMAND_CEILING)
}

/// Today's temperature: the seasonal mean, plus a day's own weather.
///
/// `deviation` is a draw in `0.0..1.0` from the caller's weather substream —
/// passed in rather than drawn here so this stays a pure function and the
/// stream discipline stays visible at the call site.
pub fn temperature_on(climate: &Climate, day_of_year: u32, deviation: f64) -> f64 {
    let swing = (deviation * 2.0 - 1.0) * climate.daily_swing_c;
    climate.mean_on(day_of_year) + swing
}

/// Share of days that carry any rain at all.
///
/// Rain is bursty and that matters: a month's water smeared evenly over thirty
/// days never saturates anything, while the same water in nine falls turns the
/// ground to mud twice and dries out in between. The wet week is the event the
/// mechanic exists to produce, and an average cannot produce one.
pub const WET_DAY_SHARE: f64 = 0.3;

/// Millimetres of rain today.
///
/// `roll` is a draw in `0.0..1.0` from the caller's weather substream, passed
/// in for the same reason [`temperature_on`] takes one: this stays a pure
/// function and the stream discipline stays visible at the call site.
///
/// The distribution preserves the authored monthly mean exactly — most wet days
/// are small, a few are not, and the average over a month is what the table
/// says. Below freezing this is snow, which is [`crate::ground`]'s business.
pub fn precipitation_on(climate: &Climate, day_of_year: u32, roll: f64) -> f64 {
    if roll >= WET_DAY_SHARE {
        return 0.0;
    }
    // Where in the wet share the draw landed, 1.0 at the wettest.
    let intensity = 1.0 - roll / WET_DAY_SHARE;
    // The 0.4 + 1.2x shape averages to 1.0 over a uniform draw, so scaling the
    // month's mean by 1/WET_DAY_SHARE conserves it.
    climate.rain_on(day_of_year) / WET_DAY_SHARE * (0.4 + 1.2 * intensity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_climate_appears_exactly_once_in_the_table() {
        let mut ids: Vec<_> = CLIMATES.iter().map(|c| c.id).collect();
        let before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), before, "a climate is in the table twice");
        assert_eq!(ids.len(), ClimateId::ALL.len(), "a climate is unauthored");
        for c in CLIMATES {
            assert_eq!(c.id.def().id, c.id);
        }
    }

    /// The year has to be a year: coldest in winter, warmest in summer, and the
    /// four postings ordered the way their names promise.
    #[test]
    fn each_climate_has_a_winter_and_a_summer() {
        for c in CLIMATES {
            assert!(
                c.warmest_mean_c() > c.coldest_mean_c() + 5.0,
                "{} has no seasons",
                c.name
            );
        }
        assert!(
            ClimateId::Taiga.def().coldest_mean_c() < ClimateId::Plains.def().coldest_mean_c(),
            "the taiga must be the harder posting"
        );
        assert!(
            ClimateId::Steppe.def().warmest_mean_c() > ClimateId::Plains.def().warmest_mean_c(),
            "the steppe must be the hotter one"
        );
        // The maritime posting is the mild one: the smallest annual range.
        let range = |id: ClimateId| id.def().warmest_mean_c() - id.def().coldest_mean_c();
        assert!(
            ClimateId::ALL
                .iter()
                .all(|&id| id == ClimateId::Maritime || range(id) > range(ClimateId::Maritime))
        );
    }

    /// The interpolation must not put a step at the year boundary — a
    /// temperature that jumps a degree between 30 December and 1 January is a
    /// calendar artefact, which is the class of bug this module avoids.
    #[test]
    fn the_year_closes_smoothly_around_new_year() {
        let c = ClimateId::Plains.def();
        let last = c.mean_on(DAYS_PER_YEAR - 1);
        let first = c.mean_on(0);
        assert!(
            (last - first).abs() < 0.5,
            "31 December was {last:.2} and 1 January {first:.2}"
        );
    }

    /// Each authored mean should be very nearly what the curve reads at the
    /// middle of its own month, or the table is not the thing being played.
    #[test]
    fn mid_month_reads_back_the_authored_mean() {
        for c in CLIMATES {
            for (month, &authored) in c.monthly_mean_c.iter().enumerate() {
                let mid = month as u32 * DAYS_PER_MONTH + DAYS_PER_MONTH / 2;
                let read = c.mean_on(mid);
                assert!(
                    (read - authored).abs() < 0.2,
                    "{} month {}: authored {authored}, curve reads {read:.2}",
                    c.name,
                    month + 1
                );
            }
        }
    }

    #[test]
    fn heat_is_wanted_only_when_it_is_cold() {
        assert!(!heating_required(20.0));
        assert!(!heating_required(HEAT_THRESHOLD_C));
        assert!(heating_required(HEAT_THRESHOLD_C - 0.1));
        assert_eq!(heat_demand_factor(20.0), 0.0);
    }

    /// The shape that makes a cold snap cost something: demand reaches full at
    /// the design temperature and over-drives below it, capped so it cannot run
    /// away.
    #[test]
    fn demand_rises_with_the_cold_and_is_capped() {
        assert!(heat_demand_factor(7.0) < 0.1, "a mild day sips");
        assert!((heat_demand_factor(HEAT_DESIGN_C) - 1.0).abs() < 1e-12);
        assert!(heat_demand_factor(-25.0) > 1.0, "a cold snap over-drives");
        assert_eq!(heat_demand_factor(-100.0), HEAT_DEMAND_CEILING);
        // Monotone: colder is never cheaper.
        let mut previous = 0.0;
        for t in (-40..=20).rev() {
            let factor = heat_demand_factor(f64::from(t));
            assert!(factor >= previous - 1e-12, "demand fell at {t} C");
            previous = factor;
        }
    }

    #[test]
    fn a_days_weather_strays_from_the_mean_but_not_beyond_its_swing() {
        let c = ClimateId::Plains.def();
        let mean = c.mean_on(15);
        assert_eq!(temperature_on(c, 15, 0.5), mean);
        assert!((temperature_on(c, 15, 1.0) - (mean + c.daily_swing_c)).abs() < 1e-12);
        assert!((temperature_on(c, 15, 0.0) - (mean - c.daily_swing_c)).abs() < 1e-12);
    }

    /// The taiga must actually be a coal problem: nearly the whole winter
    /// should sit at or beyond full heat demand.
    #[test]
    fn a_taiga_winter_drives_heat_demand_to_the_ceiling() {
        let c = ClimateId::Taiga.def();
        let january: Vec<f64> = (0..30).map(|d| heat_demand_factor(c.mean_on(d))).collect();
        assert!(
            january.iter().all(|&f| f >= 1.0),
            "a taiga January should never be cheap: {january:?}"
        );
        // And the maritime posting must not be, or the choice means nothing.
        let mild = ClimateId::Maritime.def();
        assert!(
            (0..30)
                .map(|d| heat_demand_factor(mild.mean_on(d)))
                .all(|f| f < 0.5)
        );
    }
}
