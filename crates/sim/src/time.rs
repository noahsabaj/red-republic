//! Simulated time: a fixed timestep, and the calendar derived from it.
//!
//! # The tick
//!
//! The simulation advances in whole ticks of [`TICK`] and never in variable
//! real-time deltas. A fixed timestep is not a style preference here — it is
//! what makes a run reproducible, because a variable step makes the result a
//! function of how busy the machine was.
//!
//! **How many ticks pass per real second is not this crate's business.** That
//! is a game-speed control and it belongs to the shell. The simulation only
//! knows how to take one step.
//!
//! # Why a minute
//!
//! [`TICK`] is one simulated minute, which puts a 2 km walk at roughly
//! twenty-four steps and a lorry at 54 km/h at 900 m per step — fine enough
//! for commutes to be felt and coarse enough that a day is 1,440 steps rather
//! than tens of thousands. It is a **starting value chosen to be changed**:
//! the plan calls for resolution to be settled by measurement once there are
//! agents to measure, and everything here is written against the constant so
//! that changing it is a one-line experiment.
//!
//! # The calendar
//!
//! Twelve thirty-day months, a 360-day year, founding on 1 March 1960 — taken
//! unchanged from the archived build. It is a simplification, deliberately:
//! equal months make seasonal balance legible, and every balance figure ported
//! from `archive/` was tuned against this calendar. Changing it would silently
//! invalidate all of them.

use crate::units::Seconds;

/// The fixed simulation timestep. See the module docs before changing it.
pub const TICK: Seconds = Seconds(60.0);

/// Ticks in one simulated day.
pub const TICKS_PER_DAY: u64 = 1_440;

pub const DAYS_PER_MONTH: u32 = 30;
pub const MONTHS_PER_YEAR: u32 = 12;
pub const DAYS_PER_YEAR: u32 = DAYS_PER_MONTH * MONTHS_PER_YEAR;

/// The year day indices count from. Day index 0 is 1 January 1960.
pub const EPOCH_YEAR: u32 = 1960;

/// Founding is 1 March 1960 — day index 60, not 0.
pub const FOUNDING_DAY_INDEX: u64 = 60;

/// A calendar date in the republic's 360-day year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: u32,
    /// 1..=12
    pub month: u32,
    /// 1..=30
    pub day: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    Autumn,
}

impl Date {
    /// Days since 1 January 1960.
    pub fn day_index(self) -> u64 {
        u64::from(self.year - EPOCH_YEAR) * u64::from(DAYS_PER_YEAR)
            + u64::from(self.month - 1) * u64::from(DAYS_PER_MONTH)
            + u64::from(self.day - 1)
    }

    /// The inverse of [`Date::day_index`].
    pub fn from_day_index(index: u64) -> Self {
        let year = EPOCH_YEAR + (index / u64::from(DAYS_PER_YEAR)) as u32;
        let within_year = (index % u64::from(DAYS_PER_YEAR)) as u32;
        Self {
            year,
            month: within_year / DAYS_PER_MONTH + 1,
            day: within_year % DAYS_PER_MONTH + 1,
        }
    }

    pub fn season(self) -> Season {
        match self.month {
            12 | 1 | 2 => Season::Winter,
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            _ => Season::Autumn,
        }
    }
}

/// The simulation's clock. Monotonic, tick-counted, and the single source of
/// "when it is" — nothing else may keep its own notion of elapsed time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SimClock {
    /// Ticks elapsed since founding.
    ticks: u64,
}

impl SimClock {
    /// A clock at the founding moment: midnight, 1 March 1960.
    pub fn new() -> Self {
        Self { ticks: 0 }
    }

    /// Take one step. The only way time moves.
    pub fn advance(&mut self) {
        self.ticks += 1;
    }

    /// Take several steps at once. Identical in effect to calling
    /// [`SimClock::advance`] that many times — it must stay that way, or
    /// fast-forwarding would diverge from playing.
    pub fn advance_by(&mut self, ticks: u64) {
        self.ticks += ticks;
    }

    pub fn ticks(self) -> u64 {
        self.ticks
    }

    /// Simulated time since founding.
    pub fn elapsed(self) -> Seconds {
        TICK * self.ticks as f64
    }

    /// Whole days since founding.
    pub fn days_elapsed(self) -> u64 {
        self.ticks / TICKS_PER_DAY
    }

    /// Days since 1 January 1960 — the index balance tables and deadlines use.
    pub fn day_index(self) -> u64 {
        FOUNDING_DAY_INDEX + self.days_elapsed()
    }

    pub fn date(self) -> Date {
        Date::from_day_index(self.day_index())
    }

    /// Days since 1 January of the current year — what the climate curve is a
    /// function of.
    pub fn day_of_year(self) -> u32 {
        (self.day_index() % u64::from(DAYS_PER_YEAR)) as u32
    }

    pub fn season(self) -> Season {
        self.date().season()
    }

    /// Time since midnight. Sub-day resolution is the reason the tick is a
    /// minute rather than a day: a commute that happens at a time of day is
    /// the whole point of simulating citizens individually.
    pub fn time_of_day(self) -> Seconds {
        TICK * (self.ticks % TICKS_PER_DAY) as f64
    }

    /// True on the tick that starts a new day — where daily bookkeeping hangs.
    pub fn is_day_boundary(self) -> bool {
        self.ticks.is_multiple_of(TICKS_PER_DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founding_is_the_first_of_march_nineteen_sixty() {
        let clock = SimClock::new();
        assert_eq!(
            clock.date(),
            Date {
                year: 1960,
                month: 3,
                day: 1
            }
        );
        // The archived build's dayIndex() put founding at 60, and contract
        // deadlines were absolute indices against it. Ported balance figures
        // assume this origin.
        assert_eq!(clock.day_index(), FOUNDING_DAY_INDEX);
        assert_eq!(clock.time_of_day(), Seconds::ZERO);
    }

    #[test]
    fn a_day_is_exactly_one_thousand_four_hundred_and_forty_ticks() {
        let mut clock = SimClock::new();
        for _ in 0..TICKS_PER_DAY - 1 {
            clock.advance();
        }
        assert_eq!(clock.days_elapsed(), 0, "not a new day until the boundary");
        clock.advance();
        assert_eq!(clock.days_elapsed(), 1);
        assert_eq!(clock.date().day, 2);
        assert!((clock.elapsed().as_days() - 1.0).abs() < 1e-9);
    }

    /// Fast-forwarding must be indistinguishable from playing, or a save that
    /// catches up would land somewhere a live run never would.
    #[test]
    fn advancing_in_bulk_matches_advancing_one_at_a_time() {
        let mut stepwise = SimClock::new();
        for _ in 0..5_000 {
            stepwise.advance();
        }
        let mut bulk = SimClock::new();
        bulk.advance_by(5_000);
        assert_eq!(stepwise, bulk);
    }

    #[test]
    fn dates_round_trip_through_their_day_index() {
        for index in [0u64, 60, 359, 360, 1_000, 12_345] {
            let date = Date::from_day_index(index);
            assert_eq!(date.day_index(), index, "round trip failed at {index}");
            assert!((1..=MONTHS_PER_YEAR).contains(&date.month));
            assert!((1..=DAYS_PER_MONTH).contains(&date.day));
        }
    }

    #[test]
    fn a_year_is_three_hundred_and_sixty_days() {
        let mut clock = SimClock::new();
        clock.advance_by(TICKS_PER_DAY * u64::from(DAYS_PER_YEAR));
        let date = clock.date();
        assert_eq!(date.year, 1961);
        assert_eq!((date.month, date.day), (3, 1), "same date, one year on");
    }

    /// Pinned against the archived build's season(): month 12, 1 and 2 are
    /// winter; through 5 spring; through 8 summer; the rest autumn. Farm
    /// output and heating demand are tuned to these boundaries.
    #[test]
    fn seasons_fall_where_the_archived_build_put_them() {
        let season_of = |month| {
            Date {
                year: 1960,
                month,
                day: 1,
            }
            .season()
        };
        for month in [12, 1, 2] {
            assert_eq!(season_of(month), Season::Winter, "month {month}");
        }
        for month in 3..=5 {
            assert_eq!(season_of(month), Season::Spring, "month {month}");
        }
        for month in 6..=8 {
            assert_eq!(season_of(month), Season::Summer, "month {month}");
        }
        for month in 9..=11 {
            assert_eq!(season_of(month), Season::Autumn, "month {month}");
        }
    }

    #[test]
    fn time_of_day_wraps_at_midnight_and_reaches_noon() {
        let mut clock = SimClock::new();
        clock.advance_by(TICKS_PER_DAY / 2);
        assert!((clock.time_of_day().as_hours() - 12.0).abs() < 1e-9);
        assert!(!clock.is_day_boundary());

        clock.advance_by(TICKS_PER_DAY / 2);
        assert_eq!(clock.time_of_day(), Seconds::ZERO);
        assert!(clock.is_day_boundary());
    }

    #[test]
    fn the_tick_is_a_minute() {
        assert!((TICK.as_minutes() - 1.0).abs() < 1e-9);
        assert_eq!(TICKS_PER_DAY, (Seconds::from_days(1.0).0 / TICK.0) as u64);
    }
}
