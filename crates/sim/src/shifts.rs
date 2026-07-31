//! How many hours a day a building is manned, and what asking for more costs.
//!
//! # Night is a labour problem, not a lighting effect
//!
//! A factory does not run itself through the night. It runs because somebody is
//! standing in it, and that somebody is a second crew the republic has to have,
//! house, feed and get to work in the dark. That is the whole mechanic: an hour
//! nobody is rostered for is an hour that produces nothing, and **no part of the
//! simulation needs a flag saying "it is night"** for that to be true. The clock
//! is already there; the roster is what was missing.
//!
//! # Two levers, and they are not the same lever
//!
//! **Shifts** — how many crews a building runs. Three shifts is three times the
//! output and three times the people, which is a straight trade the player makes
//! against every other thing those people could be doing. Nothing is free here
//! and nothing is unfair.
//!
//! **Hours** — how long a shift is. This one is nearly free, and that is exactly
//! why it has to cost something else: a twelve-hour shift covers half again as
//! much of the day with the same crew. What it costs is the crew. Health and
//! loyalty fall in proportion to the hours worked past [`STANDARD_HOURS`], and
//! loyalty is already wired to emigration — so a republic that runs its people
//! into the ground loses them, without a single new mechanism.
//!
//! # Three levels, because that is how the question is actually asked
//!
//! A national standard, a rule for a category of workplace, and an exception for
//! one building. *"Doctors work twelve, but at this hospital fourteen."* The
//! narrower answer always wins, and clearing an override falls back rather than
//! freezing whatever the value happened to be.
//!
//! # Why the resolved answer lives on the building
//!
//! [`crate::building::Building::hours`] is the resolved number, cached where
//! everything that reads it already is. It is **not** a second source of truth
//! that could disagree with the policy, because the policy lives inside
//! [`crate::building::Buildings`] and every path that could change either one
//! goes through it: placing a building resolves its hours from the policy in the
//! same call, and changing the policy walks the buildings in the same call.
//! There is no window in which the two are apart and therefore no guard needed
//! to notice one — the state is unrepresentable rather than merely tested for.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::building::{BuildingId, BuildingKind};

/// The shift length every authored production rate in the building table
/// assumes.
///
/// **Load-bearing for balance, not merely a default.** A building's authored
/// output is what one crew of its authored size makes in one of these, so a
/// republic that changes nothing produces exactly what it produced before this
/// module existed. Every other roster is measured against it: twelve hours is
/// 1.5 days' work out of one crew, three eight-hour shifts is three.
pub const STANDARD_HOURS: f64 = 8.0;

/// The shortest shift the republic will roster. Below this a "shift" is a
/// commute with a cup of tea at the end of it.
pub const MIN_HOURS: f64 = 4.0;

/// The longest. Not a health limit — the health cost is real and applies right
/// up to it — but a limit on what can be called a shift at all: past sixteen
/// hours a second crew is the only honest way to cover the day.
pub const MAX_HOURS: f64 = 16.0;

/// The longest working period that still fits inside daylight.
///
/// Not a sunrise table — see [`crate::building::Building::works_after_dark`] for
/// why one is not needed. Twelve hours is generous for a winter posting and mean
/// for a summer one, and the answer it gates is binary, so a figure in the
/// middle of the year is the honest one.
pub const DAYLIGHT_HOURS: f64 = 12.0;

/// How many crews one building may run. Three is the whole day at the standard
/// length, which is the ceiling that matters.
pub const MAX_SHIFTS: u8 = 3;

/// Health lost per day, per hour worked past [`STANDARD_HOURS`].
///
/// At twelve hours that is a target twelve points lower, which a republic with
/// a clinic can absorb and one without cannot — the same shape as the alcohol
/// cost it sits beside, and deliberately the same order of magnitude.
pub const OVERWORK_HEALTH: f64 = 0.030;

/// Loyalty lost per day, per hour past [`STANDARD_HOURS`].
///
/// Larger than the health cost on purpose. People tolerate being made ill more
/// quietly than they tolerate being made to work late, and loyalty is what
/// reaches [`crate::wellbeing::LOYALTY_LEAVES`] and takes them out of the
/// republic altogether.
pub const OVERWORK_LOYALTY: f64 = 0.045;

/// What the republic has decided about working hours, nationally and by
/// exception.
///
/// Lives inside [`crate::building::Buildings`] rather than beside it — see the
/// module note. Nothing outside this crate can hold one; the player edits it
/// through [`crate::command::Command::SetShiftHours`] and reads it back through
/// [`crate::world::World::shift_policy`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShiftPolicy {
    national: f64,
    /// *"Doctors work twelve."* Keyed by kind, so a hospital built tomorrow is
    /// covered by a rule made today — which is the difference between a policy
    /// and a batch edit.
    by_kind: BTreeMap<BuildingKind, f64>,
    /// *"...but at this hospital, fourteen."*
    by_building: BTreeMap<BuildingId, f64>,
}

impl Default for ShiftPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl ShiftPolicy {
    pub fn new() -> Self {
        Self {
            national: STANDARD_HOURS,
            by_kind: BTreeMap::new(),
            by_building: BTreeMap::new(),
        }
    }

    /// The standard everything falls back to.
    pub fn national(&self) -> f64 {
        self.national
    }

    /// The rule for a category of workplace, if there is one.
    pub fn of_kind(&self, kind: BuildingKind) -> Option<f64> {
        self.by_kind.get(&kind).copied()
    }

    /// The exception for one building, if there is one.
    pub fn of_building(&self, id: BuildingId) -> Option<f64> {
        self.by_building.get(&id).copied()
    }

    /// How long a shift is at this building. Narrowest rule wins.
    pub fn hours_for(&self, kind: BuildingKind, id: BuildingId) -> f64 {
        self.of_building(id)
            .or_else(|| self.of_kind(kind))
            .unwrap_or(self.national)
    }

    /// Every category with a rule of its own, in a stable order.
    pub fn kind_rules(&self) -> impl Iterator<Item = (BuildingKind, f64)> + '_ {
        self.by_kind.iter().map(|(&k, &h)| (k, h))
    }

    /// Every building with an exception of its own, in a stable order.
    pub fn building_rules(&self) -> impl Iterator<Item = (BuildingId, f64)> + '_ {
        self.by_building.iter().map(|(&i, &h)| (i, h))
    }

    pub(crate) fn set_national(&mut self, hours: f64) {
        self.national = clamp_hours(hours);
    }

    pub(crate) fn set_kind(&mut self, kind: BuildingKind, hours: Option<f64>) {
        match hours {
            Some(h) => self.by_kind.insert(kind, clamp_hours(h)),
            None => self.by_kind.remove(&kind),
        };
    }

    pub(crate) fn set_building(&mut self, id: BuildingId, hours: Option<f64>) {
        match hours {
            Some(h) => self.by_building.insert(id, clamp_hours(h)),
            None => self.by_building.remove(&id),
        };
    }

    /// Forget a demolished building's exception, so a later building can never
    /// inherit a rule made about something that no longer stands.
    pub(crate) fn forget(&mut self, id: BuildingId) {
        self.by_building.remove(&id);
    }
}

/// Hours are clamped rather than refused, because the command that carries them
/// already refuses out-of-range values with a sentence. This is the second
/// gate — the one that holds for anything the crate does to itself.
fn clamp_hours(hours: f64) -> f64 {
    if hours.is_nan() {
        STANDARD_HOURS
    } else {
        hours.clamp(MIN_HOURS, MAX_HOURS)
    }
}

/// What working `hours` a day costs a person, per day, as `(health, loyalty)`.
///
/// Zero at or below the standard: a republic that rosters normally pays nothing
/// for a mechanic it is not using.
pub fn overwork_cost(hours: f64) -> (f64, f64) {
    let over = (hours - STANDARD_HOURS).max(0.0);
    (over * OVERWORK_HEALTH, over * OVERWORK_LOYALTY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_narrowest_rule_wins() {
        let mut p = ShiftPolicy::new();
        let hospital = BuildingId(7);
        assert_eq!(p.hours_for(BuildingKind::Hospital, hospital), 8.0);

        p.set_kind(BuildingKind::Hospital, Some(12.0));
        assert_eq!(p.hours_for(BuildingKind::Hospital, hospital), 12.0);
        assert_eq!(p.hours_for(BuildingKind::Sawmill, BuildingId(8)), 8.0);

        p.set_building(hospital, Some(14.0));
        assert_eq!(p.hours_for(BuildingKind::Hospital, hospital), 14.0);
        assert_eq!(p.hours_for(BuildingKind::Hospital, BuildingId(9)), 12.0);
    }

    /// Clearing an override falls back to the next rule up. It must not freeze
    /// whatever number the building happened to be running, or "clear" would
    /// quietly mean "pin".
    #[test]
    fn clearing_an_override_falls_back_rather_than_freezing() {
        let mut p = ShiftPolicy::new();
        p.set_national(10.0);
        p.set_kind(BuildingKind::Hospital, Some(12.0));
        p.set_building(BuildingId(7), Some(14.0));

        p.set_building(BuildingId(7), None);
        assert_eq!(p.hours_for(BuildingKind::Hospital, BuildingId(7)), 12.0);
        p.set_kind(BuildingKind::Hospital, None);
        assert_eq!(p.hours_for(BuildingKind::Hospital, BuildingId(7)), 10.0);
    }

    #[test]
    fn a_standard_day_costs_nothing_at_all() {
        assert_eq!(overwork_cost(STANDARD_HOURS), (0.0, 0.0));
        assert_eq!(overwork_cost(MIN_HOURS), (0.0, 0.0));
        let (health, loyalty) = overwork_cost(12.0);
        assert!(health > 0.0 && loyalty > health, "{health} {loyalty}");
    }

    /// A building's exception must not outlive the building. Ids are drawn from
    /// one rising sequence so no later building can take this one's id — but a
    /// rule about something that has been pulled down is still a rule sitting in
    /// a save with nothing to apply to, and the panel would list it.
    #[test]
    fn a_demolished_buildings_exception_is_forgotten() {
        let mut p = ShiftPolicy::new();
        p.set_building(BuildingId(3), Some(12.0));
        p.forget(BuildingId(3));
        assert_eq!(p.building_rules().count(), 0);
    }
}
