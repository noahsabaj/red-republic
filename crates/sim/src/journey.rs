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
use crate::ground::Crossing;
use crate::network::Network;
use crate::time::TICK;
use crate::units::{Metres, Point, Speed};
use serde::{Deserialize, Serialize};

/// The shortest a leg may take, in ticks. See the module docs.
pub const MIN_LEG_TICKS: f64 = 1.0;

/// How far a confined vehicle's terminal may stand from the way it rides.
///
/// A station forecourt, a wharf, an apron: the short shunt between where the
/// vehicle stops and where the goods are. Tighter than
/// [`crate::citizen::ROAD_ACCESS`] on purpose — a lorry that finds itself
/// 300 m from a road drives across the field, and a train that finds itself
/// 300 m from the rails is a train in a field.
pub const TERMINAL_REACH: Metres = Metres(120.0);

/// The pace in a yard, apron or wharf — the short hop between a terminal's
/// door and the way it stands beside.
///
/// One figure for every confined medium, because the thing being modelled is
/// the same in all three: manoeuvring in a confined space at the end of a
/// journey. Slow enough that a terminal sited carelessly costs real minutes.
pub const SHUNTING: Speed = Speed::from_kph(15.0);

/// The way a vehicle gets about, and the whole of what separates a lorry from
/// a train, a barge and an aeroplane.
///
/// # Confinement is the mechanic
///
/// A lorry **prefers** roads: [`plan`] costs the network route against the
/// open-ground line and takes whichever is quicker, which is what makes a road
/// something a vehicle chooses rather than a rail it is stuck on. Everything
/// else here is stuck on one. A train that cannot find rails at both ends of a
/// job does not take the job — and because that refusal comes back as `None`
/// from the planner, the dispatcher needed **no** new rule to keep trains off
/// road work. It already skips a vehicle that cannot make the trip.
///
/// # Why air has a network at all
///
/// It has no ground to follow, so the obvious model is a straight line from
/// anywhere to anywhere. That is wrong for the same reason a train in a field
/// is wrong: an aeroplane lands at an aerodrome. Giving air a network whose
/// nodes *are* the aerodromes puts it under exactly the rule the other two
/// already obey, and the alternative was a special case in the dispatcher
/// saying so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Medium {
    /// Roads where they help, open country where they do not.
    Road,
    Rail,
    /// Street track. Its own network rather than a cheap grade of railway,
    /// and that is a balance rule rather than a modelling flourish: sharing
    /// one network would let a republic lay tramway at a third the price and
    /// run hundred-and-twenty-tonne freight trains down it.
    Tram,
    /// Underground. Enormously dear, very fast, and it goes under a river
    /// rather than over one — the only way that crosses water without a bridge.
    Metro,
    /// Rivers and lakes — the one network nobody builds.
    Water,
    Air,
}

impl Medium {
    /// Whether a vehicle of this medium may leave its network and cross open
    /// ground. Only a road vehicle can, which is the entire difference.
    pub fn free_ranging(self) -> bool {
        matches!(self, Medium::Road)
    }

    pub fn name(self) -> &'static str {
        match self {
            Medium::Road => "Road",
            Medium::Rail => "Rail",
            Medium::Tram => "Tram",
            Medium::Metro => "Metro",
            Medium::Water => "Water",
            Medium::Air => "Air",
        }
    }

    /// What a passenger service on this way makes, stops and boarding
    /// included — see [`crate::transport`] on commercial speed.
    ///
    /// A property of the way rather than of the depot, because it is the way
    /// that decides: a tram stops every four hundred metres in traffic and a
    /// metro does not stop at traffic at all, whoever runs them.
    pub fn commercial_speed(self) -> Speed {
        match self {
            Medium::Road => Speed::from_kph(20.0),
            Medium::Tram => Speed::from_kph(17.0),
            Medium::Metro => Speed::from_kph(38.0),
            Medium::Rail => Speed::from_kph(55.0),
            Medium::Water => Speed::from_kph(12.0),
            Medium::Air => Speed::from_kph(300.0),
        }
    }

    pub const ALL: [Medium; 6] = [
        Medium::Road,
        Medium::Rail,
        Medium::Tram,
        Medium::Metro,
        Medium::Water,
        Medium::Air,
    ];
}

/// Every way through the republic, handed to a caller together.
///
/// One parameter rather than six, because a caller that has to name each
/// network separately is a caller that can be given the wrong one — and a
/// planner asked for a rail route with the road network in its hand would
/// answer confidently and be wrong.
///
/// **The open ground is deliberately not in here.** It was, briefly, and it was
/// wrong twice over: the ground is not a way anybody built, and carrying it
/// meant every caller that only wanted to know which networks exist — the
/// labour pass, a panel — had to produce a `Crossing` it had no use for.
#[derive(Debug, Clone, Copy)]
pub struct Ways<'a> {
    pub roads: &'a Network,
    pub rails: &'a Network,
    pub tramway: &'a Network,
    pub metro: &'a Network,
    pub water: &'a Network,
    /// Aerodromes, joined to each other. See [`Medium`].
    pub air: &'a Network,
}

/// A republic that has built roads and nothing else.
///
/// What most of this crate's tests are about, and what a republic is on its
/// first day.
static NO_WAY: std::sync::LazyLock<Network> = std::sync::LazyLock::new(Network::new);

impl<'a> Ways<'a> {
    pub fn on_roads(roads: &'a Network) -> Self {
        Self {
            roads,
            rails: &NO_WAY,
            tramway: &NO_WAY,
            metro: &NO_WAY,
            water: &NO_WAY,
            air: &NO_WAY,
        }
    }
}

impl<'a> Ways<'a> {
    pub fn of(&self, medium: Medium) -> &'a Network {
        match medium {
            Medium::Road => self.roads,
            Medium::Rail => self.rails,
            Medium::Tram => self.tramway,
            Medium::Metro => self.metro,
            Medium::Water => self.water,
            Medium::Air => self.air,
        }
    }
}

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
    /// What the road allows on each leg, or `None` where the leg crosses open
    /// ground. Always one shorter than `path`.
    ///
    /// A limit rather than a flag because a road's whole point is that a better
    /// surface carries faster: a lorry on a dirt track makes 25 km/h whatever it
    /// is capable of, and the same lorry on tarmac makes its own fifty.
    pub limit: Vec<Option<Speed>>,
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
    /// If the path has fewer than two waypoints, or if `limit` does not
    /// describe exactly one leg per hop. Both are contract violations by the
    /// caller rather than states the simulation can reach.
    pub fn begin(path: Vec<Point>, limit: Vec<Option<Speed>>, now: f64, first_leg: f64) -> Self {
        assert!(path.len() >= 2, "a journey needs somewhere to go");
        assert_eq!(
            limit.len() + 1,
            path.len(),
            "every leg needs to say what the road allows on it"
        );
        Self {
            path,
            limit,
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
        self.limit[self.leg as usize].is_some()
    }

    /// What a vehicle makes on leg `leg`: its own pace on the road, capped by
    /// what that stretch of road allows, or its cross-country pace off it.
    ///
    /// The cap is the whole reason a grade is a decision. Without it a road is
    /// worth exactly what any other road is worth, and there is nothing to
    /// choose between a dirt track and tarmac.
    /// `drag` is what today's conditions cost this leg — one on firm dry
    /// ground, more in mud off road and more under snow on it. It is passed in
    /// rather than looked up because it is a property of *today*, not of the
    /// plan: a leg planned in August is timed in April at April's drag, which is
    /// what lets the weather turn under a lorry that is already out.
    ///
    /// **It applies to a road leg too, and that is what makes snow clearance a
    /// mechanic.** A road used to be immune to the weather entirely — the leg
    /// took the grade's speed limit and nothing else was consulted — so a
    /// republic under half a metre of snow drove at exactly the pace it drove in
    /// June, and a plough would have had nothing to buy back. What the caller
    /// must pass is the drag for *this kind of leg*, which is why
    /// [`crate::ground::Crossing::drag_for`] exists rather than two functions a
    /// call site can pick the wrong one of.
    pub fn speed_on(&self, leg: u32, on_road: Speed, cross_country: Speed, drag: f64) -> Speed {
        match self.limit[leg as usize] {
            Some(limit) => on_road.min(limit) / drag.max(1.0),
            None => cross_country / drag.max(1.0),
        }
    }

    /// [`Journey::speed_on`] for the leg under way.
    pub fn leg_speed(&self, on_road: Speed, cross_country: Speed, drag: f64) -> Speed {
        self.speed_on(self.leg, on_road, cross_country, drag)
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

    /// The next leg, timed for a vehicle of these two speeds, as
    /// `(leg, leg_start, leg_end)`.
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

    /// The two ends of a leg, whichever leg is asked for.
    pub fn leg_ends(&self, leg: u32) -> (Point, Point) {
        (self.path[leg as usize], self.path[leg as usize + 1])
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

/// How far off a straight line two hops may be and still count as one leg.
///
/// A sine rather than an angle, because that is what the cross product gives
/// directly and no trigonometry means nothing platform-dependent. About one
/// degree.
const STRAIGHT_ENOUGH: f64 = 0.02;

/// Whether `a -> b -> c` carries straight on rather than turning.
fn is_straight_on(a: Point, b: Point, c: Point) -> bool {
    let (ux, uy) = ((b.x - a.x).0, (b.y - a.y).0);
    let (vx, vy) = ((c.x - b.x).0, (c.y - b.y).0);
    let (lu, lv) = ((ux * ux + uy * uy).sqrt(), (vx * vx + vy * vy).sqrt());
    if lu <= 0.0 || lv <= 0.0 {
        return true;
    }
    // Doubling back is never straight on, however small the cross product.
    if ux * vx + uy * vy <= 0.0 {
        return false;
    }
    (ux * vy - uy * vx).abs() / (lu * lv) < STRAIGHT_ENOUGH
}

/// Builds a waypoint list, dropping steps that go nowhere and merging steps
/// that carry straight on at the same speed.
struct PathBuilder {
    path: Vec<Point>,
    limit: Vec<Option<Speed>>,
}

impl PathBuilder {
    fn from(start: Point) -> Self {
        Self {
            path: vec![start],
            limit: Vec::new(),
        }
    }

    fn step(&mut self, to: Point, limit: Option<Speed>) {
        let last = *self.path.last().expect("a builder always has its start");
        if last.distance_to(to).0 < SAME_PLACE.0 {
            return;
        }
        // A straight run at one speed is **one leg**, however many junctions it
        // passes through.
        //
        // This is not tidiness, it is correctness, and it was found by
        // measurement. A road is laid down with a junction every 200 m so that
        // buildings along it can reach it — but [`MIN_LEG_TICKS`] floors every
        // leg at a minute, and 200 m in a minute is 12 km/h. A road subdivided
        // for access was therefore *slower* than the open ground beside it, and
        // the planner correctly refused to use any of them. Waypoints exist to
        // mark where something changes; a junction on a straight road changes
        // nothing.
        if self.limit.last() == Some(&limit)
            && let Some(&previous) = self.path.get(self.path.len().wrapping_sub(2))
            && is_straight_on(previous, last, to)
        {
            *self.path.last_mut().expect("just read it") = to;
            return;
        }
        self.path.push(to);
        self.limit.push(limit);
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
    roads: &Network,
    crossing: &Crossing,
    on_road: Speed,
    cross_country: Speed,
    now: f64,
) -> Journey {
    // Across country, the way the lattice says is quickest — round the bog
    // rather than through it, and (once there is wear on the ground) along
    // whatever line other lorries have already packed down.
    let direct = {
        let mut b = PathBuilder::from(from);
        match crossing.route(from, to) {
            Some(way) => {
                for step in way.into_iter().skip(1) {
                    b.step(step, None);
                }
            }
            // No way across at all. Return something rather than nothing —
            // the caller decides whether to send anybody, and a straight line
            // priced through impassable ground is expensive enough that the
            // road candidate wins whenever there is one.
            None => b.step(to, None),
        }
        b
    };

    let candidate = by_road(from, to, roads).unwrap_or_else(|| PathBuilder {
        path: Vec::new(),
        limit: Vec::new(),
    });

    let cost = |b: &PathBuilder| -> f64 {
        if b.path.len() < 2 {
            return f64::INFINITY;
        }
        b.path
            .windows(2)
            .zip(&b.limit)
            .map(|(pair, &limit)| {
                // Costed with snow on the road as well as mud off it, so the
                // planner takes a buried road against open ground on the merits
                // rather than because roads used to be free of the weather.
                let drag = crossing.drag_for(limit.is_some(), pair[0], pair[1]);
                let speed = match limit {
                    Some(limit) => on_road.min(limit) / drag.max(1.0),
                    None => cross_country / drag,
                };
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
    let (path, limit) = if chosen.path.len() < 2 {
        (vec![from, to], vec![None])
    } else {
        (chosen.path, chosen.limit)
    };

    let first_drag = crossing.drag_for(limit[0].is_some(), path[0], path[1]);
    let first = leg_ticks(
        path[0].distance_to(path[1]),
        match limit[0] {
            Some(limit) => on_road.min(limit) / first_drag.max(1.0),
            None => cross_country / first_drag,
        },
    );
    Journey::begin(path, limit, now, first)
}

/// The road-network candidate: cross-country to the nearest junction, along the
/// network, then cross-country to the door.
fn by_road(from: Point, to: Point, roads: &Network) -> Option<PathBuilder> {
    let start = roads.nearest_node(from, ROAD_ACCESS)?;
    let finish = roads.nearest_node(to, ROAD_ACCESS)?;
    let route = roads.route(start, finish)?;

    let mut b = PathBuilder::from(from);
    let mut nodes = route.nodes.into_iter();
    let mut previous = nodes.next()?;
    b.step(roads.position_of(previous)?, None);
    for node in nodes {
        // Each hop carries the limit of the road actually being ridden, so a
        // dirt track in the middle of a paved route slows only that stretch.
        let limit = roads.speed_between(previous, node);
        b.step(roads.position_of(node)?, limit);
        previous = node;
    }
    b.step(to, None);
    Some(b)
}

/// Plan a journey for a vehicle of any medium.
///
/// **`None` is a real answer and the whole mechanic.** A lorry can always get
/// somewhere, however slowly, so [`Medium::Road`] always plans. A train, a
/// barge and an aeroplane cannot leave their network, so if either end of the
/// job is out of reach of it there is no journey — and the dispatcher, which
/// already moves on to the next vehicle when one cannot make a trip, keeps
/// them off work they have no business taking without knowing they exist.
#[allow(clippy::too_many_arguments)]
pub fn plan_for(
    medium: Medium,
    from: Point,
    to: Point,
    ways: Ways<'_>,
    crossing: &Crossing<'_>,
    on_road: Speed,
    cross_country: Speed,
    now: f64,
) -> Option<Journey> {
    if medium.free_ranging() {
        return Some(plan(
            from,
            to,
            ways.roads,
            crossing,
            on_road,
            cross_country,
            now,
        ));
    }
    let net = ways.of(medium);
    let start = net.nearest_node(from, TERMINAL_REACH)?;
    let finish = net.nearest_node(to, TERMINAL_REACH)?;
    let route = net.route(start, finish)?;

    // Every hop carries a limit, including the shunt in and out of the yard —
    // a confined vehicle is never off its network, so there is no leg anywhere
    // on this path that the ground could slow down. That is also why nothing
    // here consults the crossing: a barge does not care that it rained.
    let mut b = PathBuilder::from(from);
    let mut nodes = route.nodes.into_iter();
    let mut previous = nodes.next()?;
    b.step(net.position_of(previous)?, Some(SHUNTING));
    for node in nodes {
        let limit = net.speed_between(previous, node);
        b.step(net.position_of(node)?, limit);
        previous = node;
    }
    b.step(to, Some(SHUNTING));

    if b.path.len() < 2 {
        // Loading bay and unloading bay are the same place. Still a journey,
        // or the vehicle never arrives — see [`MIN_LEG_TICKS`].
        b.path = vec![from, to];
        b.limit = vec![Some(SHUNTING)];
    }
    let first = leg_ticks(
        b.path[0].distance_to(b.path[1]),
        match b.limit[0] {
            Some(limit) => on_road.min(limit),
            None => cross_country,
        },
    );
    Some(Journey::begin(b.path, b.limit, now, first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ground::Lattice;
    use crate::network::default_road_speed;
    use crate::terrain::Terrain;

    /// Flat, dry, open ground with nothing in the way: the crossing these tests
    /// are not about.
    fn firm() -> Lattice {
        Lattice::from_terrain(&Terrain::flat(Metres(24_000.0)))
    }

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    fn lorry() -> Speed {
        Speed::from_kph(15.0)
    }

    /// The same lorry's speed once it is on tarmac.
    fn fast_lorry() -> Speed {
        Speed::from_kph(50.0)
    }

    /// Junctions in a line 500 m apart, starting at the origin.
    fn highway(length: f64) -> Network {
        let mut roads = Network::new();
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
            vec![None],
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
            vec![None],
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
        let j = Journey::begin(
            vec![at(0.0, 0.0), at(900.0, 300.0)],
            vec![Some(default_road_speed())],
            7.0,
            3.0,
        );
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
            vec![None; 20],
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
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(0.0, 0.0),
            at(3_000.0, 0.0),
            &Network::new(),
            &crossing,
            default_road_speed(),
            lorry(),
            0.0,
        );
        assert_eq!(j.path, vec![at(0.0, 0.0), at(3_000.0, 0.0)]);
        assert_eq!(j.limit, vec![None]);
        // 3 km at 15 km/h is twelve minutes, and a tick is a minute.
        assert!((j.leg_end - 12.0).abs() < 1e-9, "{}", j.leg_end);
    }

    /// The consequence the whole mechanic exists for: the same haul is quicker
    /// once there is road under it.
    #[test]
    fn a_road_beats_open_ground_over_the_same_ground() {
        let from = at(0.0, 100.0);
        let to = at(5_000.0, 100.0);
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let cross_country = plan(
            from,
            to,
            &Network::new(),
            &crossing,
            default_road_speed(),
            lorry(),
            0.0,
        );
        let roads = highway(6_000.0);
        let driven = plan(
            from,
            to,
            &roads,
            &crossing,
            default_road_speed(),
            lorry(),
            0.0,
        );

        let total = |j: &Journey| -> f64 {
            (0..j.legs())
                .map(|leg| {
                    let hop = j.path[leg as usize].distance_to(j.path[leg as usize + 1]);
                    leg_ticks(hop, j.speed_on(leg, default_road_speed(), lorry(), 1.0))
                })
                .sum()
        };
        assert!(
            total(&driven) < total(&cross_country),
            "the road was not taken: {:.1} vs {:.1} minutes",
            total(&driven),
            total(&cross_country)
        );
        assert!(
            driven.limit.iter().any(|l| l.is_some()),
            "no leg rode the network"
        );
        // The hops onto and off the network are never on tarmac.
        assert!(driven.limit[0].is_none());
        assert!(driven.limit[driven.limit.len() - 1].is_none());
    }

    /// The interaction that would otherwise have made every laid road useless.
    ///
    /// A road is subdivided every 200 m so buildings along it can reach it, and
    /// a leg is floored at one tick — so a road taken junction by junction has
    /// a ceiling of 12 km/h, slower than driving across the field beside it.
    /// Merging a straight run into one leg is what makes the two rules able to
    /// coexist.
    #[test]
    fn a_straight_road_is_one_leg_however_many_junctions_it_has() {
        let mut sparse = Network::new();
        let mut dense = Network::new();
        for (roads, spacing) in [(&mut sparse, 1_000.0), (&mut dense, 200.0)] {
            let steps = (6_000.0 / spacing) as u32;
            let mut previous = roads.add_node(at(0.0, 0.0));
            for i in 1..=steps {
                let next = roads.add_node(at(f64::from(i) * spacing, 0.0));
                roads.connect(previous, next, default_road_speed());
                previous = next;
            }
        }

        let ends = (at(0.0, 100.0), at(5_000.0, 100.0));
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let a = plan(
            ends.0,
            ends.1,
            &sparse,
            &crossing,
            fast_lorry(),
            lorry(),
            0.0,
        );
        let b = plan(
            ends.0,
            ends.1,
            &dense,
            &crossing,
            fast_lorry(),
            lorry(),
            0.0,
        );
        assert_eq!(
            a.legs(),
            b.legs(),
            "junction spacing changed the shape of the journey"
        );
        assert!(b.legs() <= 3, "{} legs down a straight road", b.legs());

        // And the dense road is still worth taking, which is the point.
        let across = plan(
            ends.0,
            ends.1,
            &Network::new(),
            &crossing,
            fast_lorry(),
            lorry(),
            0.0,
        );
        assert!(
            b.leg_end < across.leg_end || b.legs() > 1,
            "the subdivided road was refused"
        );
        let by_road: f64 = (0..b.legs())
            .map(|leg| {
                let hop = b.path[leg as usize].distance_to(b.path[leg as usize + 1]);
                leg_ticks(hop, b.speed_on(leg, fast_lorry(), lorry(), 1.0))
            })
            .sum();
        assert!(
            by_road < leg_ticks(ends.0.distance_to(ends.1), lorry()),
            "the road took {by_road:.1} minutes against open ground"
        );
    }

    /// Merging must not cut corners: a road that turns keeps its turn, or the
    /// shell would draw lorries driving through the countryside.
    #[test]
    fn a_road_that_turns_keeps_its_corner() {
        let mut roads = Network::new();
        let a = roads.add_node(at(0.0, 0.0));
        let corner = roads.add_node(at(2_000.0, 0.0));
        let b = roads.add_node(at(2_000.0, 2_000.0));
        roads.connect(a, corner, default_road_speed());
        roads.connect(corner, b, default_road_speed());

        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(0.0, 0.0),
            at(2_000.0, 2_000.0),
            &roads,
            &crossing,
            fast_lorry(),
            lorry(),
            0.0,
        );
        assert!(
            j.path.iter().any(|p| p.x.0 > 1_900.0 && p.y.0 < 100.0),
            "the corner was cut: {:?}",
            j.path
        );
    }

    /// A road is only worth what its surface allows.
    ///
    /// The limit is what makes a grade a decision: a lorry capable of 50 km/h
    /// makes 25 on a dirt track and its own fifty on tarmac, so ordering the
    /// better road buys something measurable rather than a nicer word.
    #[test]
    fn a_leg_is_capped_by_the_road_it_is_on() {
        let fast = Speed::from_kph(50.0);
        let track = Speed::from_kph(25.0);
        let j = Journey::begin(
            vec![at(0.0, 0.0), at(1_000.0, 0.0), at(2_000.0, 0.0)],
            vec![Some(track), Some(fast)],
            0.0,
            1.0,
        );
        assert_eq!(
            j.speed_on(0, fast, lorry(), 1.0),
            track,
            "the track should bind"
        );
        assert_eq!(j.speed_on(1, fast, lorry(), 1.0), fast, "tarmac should not");
        // And a lorry slower than the road is still the slower of the two.
        assert_eq!(j.speed_on(1, lorry(), lorry(), 1.0), lorry());
    }

    /// Two roads, same length, different surface: the better one is chosen and
    /// the journey is timed at what it allows.
    #[test]
    fn a_better_surface_wins_over_a_longer_detour_on_a_worse_one() {
        let mut roads = Network::new();
        let a = roads.add_node(at(0.0, 0.0));
        let b = roads.add_node(at(4_000.0, 0.0));
        let track = roads.add_node(at(2_000.0, 100.0));
        // A direct dirt track, and a paved way round through one junction.
        roads.connect(a, b, Speed::from_kph(25.0));
        roads.connect(a, track, Speed::from_kph(60.0));
        roads.connect(track, b, Speed::from_kph(60.0));

        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(0.0, 0.0),
            at(4_000.0, 0.0),
            &roads,
            &crossing,
            fast_lorry(),
            lorry(),
            0.0,
        );
        assert!(
            j.path.iter().any(|p| p.y.0 > 50.0),
            "the lorry stayed on the dirt track: {:?}",
            j.path
        );
    }

    /// A detour that is longer in minutes loses, however much road it has. The
    /// network is an option, not an obligation.
    #[test]
    fn a_road_going_the_wrong_way_is_ignored() {
        let mut roads = Network::new();
        let a = roads.add_node(at(0.0, 0.0));
        let b = roads.add_node(at(0.0, 20_000.0));
        let c = roads.add_node(at(200.0, 20_000.0));
        let d = roads.add_node(at(200.0, 0.0));
        roads.connect(a, b, default_road_speed());
        roads.connect(b, c, default_road_speed());
        roads.connect(c, d, default_road_speed());

        // Two hundred metres apart, with 40 km of road joining them the long way.
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(0.0, 0.0),
            at(200.0, 0.0),
            &roads,
            &crossing,
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
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(0.0, 0.0),
            at(5_000.0, 0.0),
            &roads,
            &crossing,
            default_road_speed(),
            lorry(),
            0.0,
        );
        assert_eq!(j.path[0], at(0.0, 0.0));
        assert_ne!(j.path[1], at(0.0, 0.0), "a leg that goes nowhere survived");
        assert_eq!(j.limit.len() + 1, j.path.len());
    }

    /// Somewhere you already are is still a journey, or a vehicle sent to fetch
    /// from the yard it is parked in never arrives.
    #[test]
    fn a_journey_to_where_you_already_stand_still_finishes() {
        let lattice = firm();
        let crossing = Crossing {
            lattice: &lattice,
            softness: 0.0,
            snow: 0.0,
        };
        let j = plan(
            at(500.0, 500.0),
            at(500.0, 500.0),
            &Network::new(),
            &crossing,
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
            vec![None, Some(default_road_speed())],
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
    #[should_panic(expected = "what the road allows")]
    fn a_journey_whose_flags_do_not_match_its_legs_is_refused() {
        Journey::begin(vec![at(0.0, 0.0), at(1.0, 1.0)], vec![], 0.0, 1.0);
    }
}
