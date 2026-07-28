//! Physical units, as types.
//!
//! The simulation measures in **metres and seconds**, and these newtypes make
//! that structural rather than conventional. The word "tile" does not appear
//! anywhere in this crate, and there is no scalar `f64` position for someone
//! to accidentally add a duration to.
//!
//! This exists because the archived build documented its own dishonesty about
//! exactly this. Its vehicle speed field carried a note that it was
//! "deliberately not km/h: a day covers ~5 tiles, so any real-world unit here
//! would be a fiction the sim cannot back." That was an honest comment about a
//! dishonest model — the numbers were tuned to feel right per tick, and no
//! real-world quantity could be recovered from them. Fixing that is one of the
//! reasons this build exists, so the type system is where it gets fixed:
//! [`Speed`] can only be constructed by dividing a [`Metres`] by a [`Seconds`],
//! which means every speed in the game is a real quantity by construction.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A distance or a coordinate, in metres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct Metres(pub f64);

/// A duration, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct Seconds(pub f64);

/// A mass, in tonnes.
///
/// The economy is bulk and continuous — a fractional tonne of coal is a real
/// quantity, not a rounding artefact. Wholeness is a property of the *edges*
/// (what crosses the border, what a contract owes, what the player is shown),
/// never of the simulation, and that was true in the archived build for the
/// same reason: production, wear and consumption are all rates.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct Tonnes(pub f64);

impl Tonnes {
    pub const ZERO: Self = Self(0.0);

    pub fn is_positive(self) -> bool {
        self.0 > 0.0
    }

    /// Clamped at zero: a negative remainder is a float artefact, never a
    /// meaningful quantity of ore.
    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self((self.0 - rhs.0).max(0.0))
    }

    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }
}

impl Add for Tonnes {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Tonnes {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Tonnes {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl AddAssign for Tonnes {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Tonnes {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl std::iter::Sum for Tonnes {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|t| t.0).sum())
    }
}

/// A speed, in metres per second.
///
/// Deliberately has no public tuple constructor: the only ways to make one are
/// [`Speed::from_kph`] and dividing a distance by a duration. A number cannot
/// become a speed just by being called one.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct Speed(f64);

impl Metres {
    pub const ZERO: Self = Self(0.0);

    pub fn from_km(km: f64) -> Self {
        Self(km * 1000.0)
    }

    pub fn as_km(self) -> f64 {
        self.0 / 1000.0
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl Seconds {
    pub const ZERO: Self = Self(0.0);

    pub fn from_minutes(m: f64) -> Self {
        Self(m * 60.0)
    }

    pub fn from_hours(h: f64) -> Self {
        Self(h * 3600.0)
    }

    pub fn from_days(d: f64) -> Self {
        Self(d * 86_400.0)
    }

    pub fn as_minutes(self) -> f64 {
        self.0 / 60.0
    }

    pub fn as_hours(self) -> f64 {
        self.0 / 3600.0
    }

    pub fn as_days(self) -> f64 {
        self.0 / 86_400.0
    }
}

impl Speed {
    pub const ZERO: Self = Self(0.0);

    /// Build a speed from kilometres per hour — the unit a human quotes a
    /// vehicle in. That this conversion is meaningful at all is the point of
    /// the module.
    ///
    /// `const` so that an authored table — [`crate::fleet::VEHICLES`] — can
    /// hold real [`Speed`] values rather than bare numbers waiting to be
    /// converted by whoever remembers to.
    pub const fn from_kph(kph: f64) -> Self {
        Self(kph / 3.6)
    }

    pub fn as_kph(self) -> f64 {
        self.0 * 3.6
    }

    pub fn as_mps(self) -> f64 {
        self.0
    }

    /// The slower of two speeds — a vehicle's own pace against what the road
    /// it is on will take.
    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    /// How long this speed takes to cover a distance.
    ///
    /// # Panics
    /// If the speed is zero — a stationary thing never arrives, and returning
    /// an infinity here would propagate into arrival times and schedules as a
    /// silently poisoned number.
    pub fn time_to_cover(self, distance: Metres) -> Seconds {
        assert!(self.0 > 0.0, "a zero speed never covers any distance");
        Seconds(distance.0 / self.0)
    }
}

// ---- Metres arithmetic ----

impl Add for Metres {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Metres {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Metres {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<f64> for Metres {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl Div<f64> for Metres {
    type Output = Self;
    fn div(self, rhs: f64) -> Self {
        Self(self.0 / rhs)
    }
}

impl AddAssign for Metres {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Metres {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

/// Distance over duration is a speed. This is the only arithmetic path to one.
impl Div<Seconds> for Metres {
    type Output = Speed;
    fn div(self, rhs: Seconds) -> Speed {
        assert!(rhs.0 > 0.0, "a speed needs a positive duration");
        Speed(self.0 / rhs.0)
    }
}

// ---- Seconds arithmetic ----

impl Add for Seconds {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Seconds {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f64> for Seconds {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self(self.0 * rhs)
    }
}

impl AddAssign for Seconds {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

/// Speed over duration is a distance — how far something got.
impl Mul<Seconds> for Speed {
    type Output = Metres;
    fn mul(self, rhs: Seconds) -> Metres {
        Metres(self.0 * rhs.0)
    }
}

impl Mul<Speed> for Seconds {
    type Output = Metres;
    fn mul(self, rhs: Speed) -> Metres {
        Metres(self.0 * rhs.0)
    }
}

/// A position or offset on the ground plane, in metres.
///
/// Space is continuous. There is no cell index here and there must never be
/// one: buildings sit at real positions with real footprints, and the terrain
/// grid is a description of the ground, not the unit things are made of.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: Metres,
    pub y: Metres,
}

impl Point {
    pub const ORIGIN: Self = Self {
        x: Metres::ZERO,
        y: Metres::ZERO,
    };

    pub fn new(x: Metres, y: Metres) -> Self {
        Self { x, y }
    }

    /// Straight-line distance. Not a travel distance — routing is a graph
    /// problem and this is the geometry underneath it.
    ///
    /// Deliberately `sqrt` and not `hypot`. `hypot` is a libm compound
    /// function and is permitted to differ in its last bit between platforms
    /// and library versions; `sqrt` is exactly rounded by IEEE-754 everywhere.
    /// At map scales `dx * dx` is nowhere near overflow, so the only thing
    /// `hypot` would buy is the non-determinism.
    pub fn distance_to(self, other: Self) -> Metres {
        let dx = (self.x - other.x).0;
        let dy = (self.y - other.y).0;
        Metres((dx * dx + dy * dy).sqrt())
    }
}

impl Add for Point {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test the whole module exists for: units must compose into answers
    /// that are correct about the real world, not merely self-consistent.
    ///
    /// A citizen walking the 2 km commute limit should take about 24 minutes.
    /// If that ever comes out as 24 seconds or 24 hours, the model has drifted
    /// back into being tuned per tick rather than measured.
    #[test]
    fn a_two_kilometre_walk_takes_about_twenty_four_minutes() {
        let walk = Speed::from_kph(5.0); // an unhurried adult pace
        let elapsed = walk.time_to_cover(Metres::from_km(2.0));
        assert!(
            (23.0..25.0).contains(&elapsed.as_minutes()),
            "a 2 km walk took {} minutes",
            elapsed.as_minutes()
        );
    }

    #[test]
    fn a_lorry_at_fifty_four_kph_covers_fifteen_metres_a_second() {
        let lorry = Speed::from_kph(54.0);
        assert!((lorry.as_mps() - 15.0).abs() < 1e-9);
        let travelled = lorry * Seconds::from_minutes(1.0);
        assert!((travelled.0 - 900.0).abs() < 1e-9);
    }

    #[test]
    fn distance_over_duration_is_the_speed_you_expect() {
        let speed = Metres::from_km(1.0) / Seconds::from_minutes(1.0);
        assert!((speed.as_kph() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_round_trip_through_speed_and_back_conserves_the_distance() {
        let d = Metres::from_km(3.5);
        let t = Seconds::from_minutes(7.0);
        let back = (d / t) * t;
        assert!((back.0 - d.0).abs() < 1e-9);
    }

    #[test]
    fn distance_is_euclidean_on_the_ground_plane() {
        let a = Point::new(Metres(0.0), Metres(0.0));
        let b = Point::new(Metres(300.0), Metres(400.0));
        assert!((a.distance_to(b).0 - 500.0).abs() < 1e-9);
    }

    #[test]
    #[should_panic(expected = "never covers any distance")]
    fn a_zero_speed_refuses_to_produce_an_arrival_time() {
        Speed::ZERO.time_to_cover(Metres::from_km(1.0));
    }

    #[test]
    fn day_and_hour_conversions_agree() {
        assert!((Seconds::from_days(1.0).as_hours() - 24.0).abs() < 1e-9);
        assert!((Seconds::from_hours(1.0).as_minutes() - 60.0).abs() < 1e-9);
    }
}
