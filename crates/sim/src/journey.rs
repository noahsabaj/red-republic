//! Getting somewhere, as a plan rather than a walk.
//!
//! # A journey is a plan, not a stepped simulation
//!
//! This is the decision the whole moving half of the simulation rests on, and it
//! was arrived at by measurement. The obvious model — step every vehicle a
//! little way each tick — dies twice over. A one-second tick would put the
//! fastest game speed at 28,800 ticks per real second against 0.069 ms of work
//! each, which is 1,982 ms of compute per real second: not slow, *impossible*.
//! And stepping thousands of walkers per tick is worse still.
//!
//! So a moving thing carries a [`Journey`]: a fixed list of waypoints, and a
//! window of absolute time for the leg it is currently on. Three things follow.
//!
//! - **Position is a pure function of `(plan, time)`.** [`Journey::position_at`]
//!   takes a *fractional* tick, so a renderer running at 60 fps interpolates
//!   smoothly between simulation steps a minute apart without the simulation
//!   knowing anything about frames. It is the same answer at every game speed,
//!   because the plan does not depend on how often anybody looks at it.
//! - **Work happens at leg boundaries, not per tick.** Each tick the fleet
//!   compares `leg_end` against the clock — one float comparison per vehicle —
//!   and only the vehicles whose leg has actually finished do real work.
//! - **Determinism holds** because the simulation still advances only in whole
//!   ticks; the fractional time is display-only, and never feeds back. `leg_end`
//!   is absolute rather than a countdown, so no drift accumulates over a long
//!   journey.
//!
//! # Why a leg takes at least one tick
//!
//! [`MIN_LEG_TICKS`] is what keeps that last claim true in the awkward cases. A
//! zero-length leg — a supplier whose loading bay is where the lorry already
//! stands — would otherwise have a zero-width time window, and dividing by it
//! produces an infinity that propagates straight into arrival times. Flooring
//! every leg at one tick removes the case entirely, and it buys a second
//! property worth having: **a vehicle can finish at most one leg per tick**, so
//! the per-tick scan never has to loop and its cost is exactly O(vehicles).
//!
//! At a one-minute tick the floor costs a minute per waypoint on journeys short
//! enough to notice, which is below the resolution anything in this simulation
//! is measured at.

use crate::citizen::ROAD_ACCESS;
use crate::road::RoadNetwork;
use crate::time::TICK;
use crate::units::{Metres, Point, Speed};
use serde::{Deserialize, Serialize};

/// The shortest a leg may take, in ticks. See the module docs.
pub const MIN_LEG_TICKS: f64 = 1.0;

/// Two waypoints closer together than this are the same place.
///
/// Without it, a building that happens to sit exactly on a junction produces a
/// zero-length leg, which costs a whole tick to drive nowhere.
const SAME_PLACE: Metres = Metres(1.0);

/// How long a leg takes, in ticks, floored at [`MIN_LEG_TICKS`].
pub fn leg_ticks(distance: Metres, speed: Speed) -> f64 {
    (speed.time_to_cover(distance).0 / TICK.0).max(MIN_LEG_TICKS)
}

/// A planned route, and where along it the traveller currently is.
///
/// Only the *current* leg carries a time window. The rest are re-timed as they
/// are reached, which is deliberate: it is what lets ground conditions change
/// under a vehicle already on its way, rather than fixing the whole schedule at
/// dispatch and pretending the weather cannot turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Journey {
    /// Waypoints. Leg `i` runs from `path[i]` to `path[i + 1]`.
    pub path: Vec<Point>,
    /// Whether each leg rides the road network. Always one shorter than `path`.
    pub on_road: Vec<bool>,
    /// Which leg is under way.
    pub leg: u32,
    /// When this leg began, as an absolute fractional tick.
    pub leg_start: f64,
    /// When this leg is due to end. Absolute, so nothing drifts.
    pub leg_end: f64,
}

impl Journey {
    /// Start a journey along `path`, with its first leg already timed.
    ///
    /// # Panics
    /// If the path has fewer than two waypoints, or if `on_road` does not
    /// describe exactly one flag per leg. Both are contract violations by the
    /// caller rather than states the simulation can reach.
    pub fn begin(path: Vec<Point>, on_road: Vec<bool>, now: f64, first_leg: f64) -> Self {
        assert!(path.len() >= 2, "a journey needs somewhere to go");
        assert_eq!(
            on_road.len() + 1,
            path.len(),
            "every leg needs to say whether it is on road"
        );
        Self {
            path,
            on_road,
            leg: 0,
            leg_start: now,
            leg_end: now + first_leg,
        }
    }

    /// How many legs there are in total.
    pub fn legs(&self) -> u32 {
        (self.path.len() - 1) as u32
    }

    /// Where the current leg started from.
    pub fn leg_from(&self) -> Point {
        self.path[self.leg as usize]
    }

    /// Where the current leg is heading.
    pub fn leg_to(&self) -> Point {
        self.path[self.leg as usize + 1]
    }

    /// Whether the current leg rides the road network.
    pub fn leg_on_road(&self) -> bool {
        self.on_road[self.leg as usize]
    }

    pub fn leg_distance(&self) -> Metres {
        self.leg_from().distance_to(self.leg_to())
    }

    /// Where the whole journey ends.
    pub fn destination(&self) -> Point {
        *self.path.last().expect("a journey has at least two points")
    }

    /// Total distance over every leg — what fuel is charged against.
    pub fn distance(&self) -> Metres {
        self.path
            .windows(2)
            .fold(Metres::ZERO, |acc, pair| acc + pair[0].distance_to(pair[1]))
    }

    /// Whether the leg under way is the last one, so finishing it finishes the
    /// journey.
    pub fn on_last_leg(&self) -> bool {
        self.leg + 1 >= self.legs()
    }

    /// Whether the current leg is due to have finished by `now`.
    pub fn leg_done_by(&self, now: f64) -> bool {
        now >= self.leg_end
    }

    /// The next leg, timed at `speed`, as `(leg, leg_start, leg_end)`.
    ///
    /// `leg_start` is the previous leg's *scheduled* end rather than the tick it
    /// was noticed on, which is the whole reason arrival times do not drift on a
    /// long haul.
    ///
    /// # Panics
    /// If there is no next leg. Callers check [`Journey::on_last_leg`] first.
    pub fn next_leg(&self, speed: Speed) -> (u32, f64, f64) {
        assert!(!self.on_last_leg(), "there is no leg after the last one");
        let leg = self.leg + 1;
        let start = self.leg_end;
        let distance = self.path[leg as usize].distance_to(self.path[leg as usize + 1]);
        (leg, start, start + leg_ticks(distance, speed))
    }

    /// Where the traveller is at `now`, an absolute fractional tick.
    ///
    /// Pure, and defined at every real number: before the leg began it is at the
    /// leg's start, after it should have ended it is at the leg's end. That
    /// clamping is what lets a renderer sample freely without having to know
    /// when the simulation last ran.
    pub fn position_at(&self, now: f64) -> Point {
        let from = self.leg_from();
        let to = self.leg_to();
        let span = self.leg_end - self.leg_start;
        let t = if span > 0.0 {
            ((now - self.leg_start) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        Point::new(
            Metres(from.x.0 + (to.x.0 - from.x.0) * t),
            Metres(from.y.0 + (to.y.0 - from.y.0) * t),
        )
    }
}

/// Builds a waypoint list, dropping steps that go nowhere.
struct PathBuilder {
    path: Vec<Point>,
    on_road: Vec<bool>,
}

impl PathBuilder {
    fn from(start: Point) -> Self {
        Self {
            path: vec![start],
            on_road: Vec::new(),
        }
    }

    fn step(&mut self, to: Point, on_road: bool) {
        let last = *self.path.last().expect("a builder always has its start");
        if last.distance_to(to).0 < SAME_PLACE.0 {
            return;
        }
        self.path.push(to);
        self.on_road.push(on_road);
    }
}

/// Plan a drive from `from` to `to`.
///
/// **Roads are preferred, never required.** The road network is tried and the
/// open-ground straight line is tried, and whichever is quicker wins — which is
/// what makes a road something a vehicle *chooses* because it is faster, rather
/// than a rail it is confined to. A republic with no roads at all still moves
/// freight; it moves it slowly, which is the pressure that makes roads worth
/// building.
///
/// The access hop at each end is always cross-country: a lorry leaves the road
/// to reach a loading bay, and that last few hundred metres is not tarmac.
pub fn plan(
    from: Point,
    to: Point,
    roads: &RoadNetwork,
    on_road: Speed,
    cross_country: Speed,
    now: f64,
) -> Journey {
    let direct = {
        let mut b = PathBuilder::from(from);
        b.step(to, false);
        b
    };

    let candidate = by_road(from, to, roads).unwrap_or_else(|| PathBuilder {
        path: Vec::new(),
        on_road: Vec::new(),
    });

    let cost = |b: &PathBuilder| -> f64 {
        if b.path.len() < 2 {
            return f64::INFINITY;
        }
        b.path
            .windows(2)
            .zip(&b.on_road)
            .map(|(pair, &road)| {
                let speed = if road { on_road } else { cross_country };
                leg_ticks(pair[0].distance_to(pair[1]), speed)
            })
            .sum()
    };

    // Ties go to the direct line: fewer waypoints, and the answer does not
    // depend on which was built first.
    let chosen = if cost(&candidate) < cost(&direct) {
        candidate
    } else {
        direct
    };

    // A destination within a metre of the start still needs a leg, or there is
    // no journey to finish and the vehicle never arrives.
    let (path, flags) = if chosen.path.len() < 2 {
        (vec![from, to], vec![false])
    } else {
        (chosen.path, chosen.on_road)
    };

    let first = leg_ticks(
        path[0].distance_to(path[1]),
        if flags[0] { on_road } else { cross_country },
    );
    Journey::begin(path, flags, now, first)
}

/// The road-network candidate: cross-country to the nearest junction, along the
/// network, then cross-country to the door.
fn by_road(from: Point, to: Point, roads: &RoadNetwork) -> Option<PathBuilder> {
    let start = roads.nearest_node(from, ROAD_ACCESS)?;
    let finish = roads.nearest_node(to, ROAD_ACCESS)?;
    let route = roads.route(start, finish)?;

    let mut b = PathBuilder::from(from);
    let mut nodes = route.nodes.into_iter();
    let first = nodes.next()?;
    b.step(roads.position_of(first)?, false);
    for node in nodes {
        b.step(roads.position_of(node)?, true);
    }
    b.step(to, false);
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::road::default_road_speed;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    fn lorry() -> Speed {
        Speed::from_kph(15.0)
    }

    /// Junctions in a line 500 m apart, starting at the origin.
    fn highway(length: f64) -> RoadNetwork {
        let mut roads = RoadNetwork::new();
        let steps = (length / 500.0) as u32;
        let mut previous = roads.add_node(at(0.0, 0.0));
        for i in 1..=steps {
            let next = roads.add_node(at(f64::from(i) * 500.0, 0.0));
            roads.connect(previous, next, default_road_speed());
            previous = next;
        }
        roads
    }

    #[test]
    fn a_position_is_linear_along_the_leg_it_is_on() {
        let j = Journey::begin(
            vec![at(0.0, 0.0), at(1_000.0, 0.0)],
            vec![false],
            100.0,
            10.0,
        );
        assert_eq!(j.position_at(100.0), at(0.0, 0.0));
        assert_eq!(j.position_at(105.0), at(500.0, 0.0));
        assert_eq!(j.position_at(110.0), at(1_000.0, 0.0));
    }

    /// Sampling outside the window is defined rather than extrapolated. A
    /// renderer that asks a moment late must not be shown a lorry that has
    /// driven past its own destination.
    #[test]
    fn sampling_outside_the_leg_clamps_to_its_ends() {
        let j = Journey::begin(
            vec![at(0.0, 0.0), at(1_000.0, 0.0)],
            vec![false],
            100.0,
            10.0,
        );
        assert_eq!(j.position_at(0.0), at(0.0, 0.0));
        assert_eq!(j.position_at(1e9), at(1_000.0, 0.0));
    }

    /// Asking is free and asking twice gives the same answer — the property the
    /// whole plan-based model is built on.
    #[test]
    fn position_is_a_pure_function_of_plan_and_time() {
        let j = Journey::begin(vec![at(0.0, 0.0), at(900.0, 300.0)], vec![true], 7.0, 3.0);
        for i in 0..1_000 {
            let t = 7.0 + f64::from(i) * 0.003;
            assert_eq!(j.position_at(t), j.position_at(t));
        }
    }

    /// The floor that removes the divide-by-zero and bounds the per-tick scan.
    #[test]
    fn a_leg_that_goes_nowhere_still_takes_a_tick() {
        assert_eq!(leg_ticks(Metres::ZERO, lorry()), MIN_LEG_TICKS);
        // 15 km/h covers 250 m in a minute, so 250 m is exactly one tick and
        // anything shorter is floored.
        assert!((leg_ticks(Metres(250.0), lorry()) - 1.0).abs() < 1e-9);
        assert!(leg_ticks(Metres(2_500.0), lorry()) > 9.0);
    }

    #[test]
    fn arrival_times_do_not_drift_over_many_legs() {
        let path: Vec<Point> = (0..=20).map(|i| at(f64::from(i) * 1_000.0, 0.0)).collect();
        let mut j = Journey::begin(
            path,
            vec![false; 20],
            0.0,
            leg_ticks(Metres(1_000.0), lorry()),
        );
        let one = leg_ticks(Metres(1_000.0), lorry());
        while !j.on_last_leg() {
            let (leg, start, end) = j.next_leg(lorry());
            j.leg = leg;
            j.leg_start = start;
            j.leg_end = end;
        }
        // Twenty identical legs, so the last one ends at exactly twenty times
        // the first. An accumulating remainder would show up here.
        assert!((j.leg_end - one * 20.0).abs() < 1e-9, "{}", j.leg_end);
    }

    /// Roads are preferred, not required. With no network at all the lorry still
    /// gets there — slowly, in a straight line.
    #[test]
    fn open_ground_is_crossed_when_there_is_no_road() {
        let j = plan(
            at(0.0, 0.0),
            at(3_000.0, 0.0),
            &RoadNetwork::new(),
            default_road_speed(),
            lorry(),
            0.0,
        );
        assert_eq!(j.path, vec![at(0.0, 0.0), at(3_000.0, 0.0)]);
        assert_eq!(j.on_road, vec![false]);
        // 3 km at 15 km/h is twelve minutes, and a tick is a minute.
        assert!((j.leg_end - 12.0).abs() < 1e-9, "{}", j.leg_end);
    }

    /// The consequence the whole mechanic exists for: the same haul is quicker
    /// once there is road under it.
    #[test]
    fn a_road_beats_open_ground_over_the_same_ground() {
        let from = at(0.0, 100.0);
        let to = at(5_000.0, 100.0);
        let cross_country = plan(
            from,
            to,
            &RoadNetwork::new(),
            default_road_speed(),
            lorry(),
            0.0,
        );
        let roads = highway(6_000.0);
        let driven = plan(from, to, &roads, default_road_speed(), lorry(), 0.0);

        let total = |j: &Journey| -> f64 {
            j.path
                .windows(2)
                .zip(&j.on_road)
                .map(|(pair, &road)| {
                    leg_ticks(
                        pair[0].distance_to(pair[1]),
                        if road { default_road_speed() } else { lorry() },
                    )
                })
                .sum()
        };
        assert!(
            total(&driven) < total(&cross_country),
            "the road was not taken: {:.1} vs {:.1} minutes",
            total(&driven),
            total(&cross_country)
        );
        assert!(driven.on_road.iter().any(|&r| r), "no leg rode the network");
        // The hops onto and off the network are never on tarmac.
        assert!(!driven.on_road[0]);
        assert!(!driven.on_road[driven.on_road.len() - 1]);
    }

    /// A detour that is longer in minutes loses, however much road it has. The
    /// network is an option, not an obligation.
    #[test]
    fn a_road_going_the_wrong_way_is_ignored() {
        let mut roads = RoadNetwork::new();
        let a = roads.add_node(at(0.0, 0.0));
        let b = roads.add_node(at(0.0, 20_000.0));
        let c = roads.add_node(at(200.0, 20_000.0));
        let d = roads.add_node(at(200.0, 0.0));
        roads.connect(a, b, default_road_speed());
        roads.connect(b, c, default_road_speed());
        roads.connect(c, d, default_road_speed());

        // Two hundred metres apart, with 40 km of road joining them the long way.
        let j = plan(
            at(0.0, 0.0),
            at(200.0, 0.0),
            &roads,
            default_road_speed(),
            lorry(),
            0.0,
        );
        assert_eq!(j.path.len(), 2, "took the scenic route: {:?}", j.path);
    }

    #[test]
    fn a_waypoint_the_lorry_is_already_standing_on_is_dropped() {
        // Home sits exactly on the first junction.
        let roads = highway(6_000.0);
        let j = plan(
            at(0.0, 0.0),
            at(5_000.0, 0.0),
            &roads,
            default_road_speed(),
            lorry(),
            0.0,
        );
        assert_eq!(j.path[0], at(0.0, 0.0));
        assert_ne!(j.path[1], at(0.0, 0.0), "a leg that goes nowhere survived");
        assert_eq!(j.on_road.len() + 1, j.path.len());
    }

    /// Somewhere you already are is still a journey, or a vehicle sent to fetch
    /// from the yard it is parked in never arrives.
    #[test]
    fn a_journey_to_where_you_already_stand_still_finishes() {
        let j = plan(
            at(500.0, 500.0),
            at(500.0, 500.0),
            &RoadNetwork::new(),
            default_road_speed(),
            lorry(),
            42.0,
        );
        assert_eq!(j.legs(), 1);
        assert!(j.on_last_leg());
        assert!((j.leg_end - 43.0).abs() < 1e-9, "one tick, not zero");
    }

    #[test]
    fn distance_is_the_sum_of_the_legs() {
        let j = Journey::begin(
            vec![at(0.0, 0.0), at(300.0, 400.0), at(300.0, 0.0)],
            vec![false, true],
            0.0,
            1.0,
        );
        assert_eq!(j.distance(), Metres(900.0));
        assert_eq!(j.legs(), 2);
    }

    #[test]
    #[should_panic(expected = "somewhere to go")]
    fn a_journey_with_one_waypoint_is_refused() {
        Journey::begin(vec![at(0.0, 0.0)], vec![], 0.0, 1.0);
    }

    #[test]
    #[should_panic(expected = "whether it is on road")]
    fn a_journey_whose_flags_do_not_match_its_legs_is_refused() {
        Journey::begin(vec![at(0.0, 0.0), at(1.0, 1.0)], vec![], 0.0, 1.0);
    }
}
