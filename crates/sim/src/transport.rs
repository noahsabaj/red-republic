//! Getting to work: the journey, and what extends how far one can reach.
//!
//! # Why this is a module and not a number
//!
//! [`crate::citizen`] establishes the constraint the whole labour model rests
//! on — a job you cannot reach is not a job — and until now the only way to
//! reach one was to walk. That is a complete model of a village and a useless
//! model of a city: it means a republic can never be larger than the distance a
//! person will walk, and that every remote deposit needs its own town whether
//! or not the geography wants one there.
//!
//! Transport is what lifts that ceiling, and it lifts it **physically**. A bus
//! rides the road network, so it reaches exactly what the republic has built
//! road to and nothing else. It carries a finite number of people, so extending
//! reach is a capacity you fund rather than a rule that changes. And it burns
//! fuel, so a commute is an ongoing cost against the same refinery output
//! everything else wants.
//!
//! The consequence worth stating plainly: a mining town that would have died
//! when its seam ran out can now survive — but only if somebody laid road to it
//! and can keep the depot fuelled. Which is the decision the mechanic exists to
//! create.
//!
//! # Commercial speed, not top speed
//!
//! [`BUS_SPEED`] is 20 km/h, which looks slow beside a 50 km/h road. It is not
//! the vehicle's top speed — it is *commercial* speed, the figure that already
//! includes stopping, boarding and waiting, and it is what a transit authority
//! quotes because it is what a passenger experiences. Modelling a bus at road
//! speed and then subtracting stop dwell time separately would arrive at the
//! same place with more machinery.

use crate::building::{BuildingKind, Buildings};
use crate::citizen::{MAX_WALK, ROAD_ACCESS, walking_speed};
use crate::resource::Resource;
use crate::road::RoadNetwork;
use crate::units::{Metres, Point, Seconds, Speed, Tonnes};
use bevy_ecs::prelude::Component;
use serde::{Deserialize, Serialize};

/// The longest journey anyone will make to work, each way.
///
/// A bound on *time*, not distance, which is the point of having transport at
/// all: the same 45 minutes is a 3.7 km walk or a 12 km ride, and that
/// difference is the whole mechanic.
pub const MAX_COMMUTE: Seconds = Seconds(45.0 * 60.0);

/// A city bus's commercial speed — stops, boarding and waiting included.
pub fn bus_speed() -> Speed {
    Speed::from_kph(20.0)
}

/// How far someone will walk to a stop at each end.
///
/// Further than [`crate::citizen::ROAD_ACCESS`], which is about whether a
/// *lorry* can serve a building: people will walk further to a bus than freight
/// will divert to a loading bay.
pub const STOP_WALK: Metres = Metres(600.0);

/// Fuel a depot burns per seat filled, per day.
///
/// A tonne of fuel moving 500 people to work and back. Placeholder in the same
/// sense as the rest of the balance table: plausible, and meant to be felt out
/// against the trajectory runner rather than reasoned to.
pub const FUEL_PER_SEAT_DAY: f64 = 0.002;

/// How a citizen gets to work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// No job, so no journey.
    None,
    Foot,
    Bus,
}

/// A citizen's journey to work — the ECS component the labour pass writes.
///
/// Kept as state rather than recomputed on demand because it is what a renderer
/// draws, what a panel reports, and what makes "why is this mine unstaffed?"
/// answerable without re-deriving the routing that produced the answer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Commute {
    pub mode: Mode,
    pub distance: Metres,
    pub time: Seconds,
}

impl Commute {
    /// Someone who is not going anywhere.
    pub const NONE: Self = Self {
        mode: Mode::None,
        distance: Metres::ZERO,
        time: Seconds::ZERO,
    };

    pub fn on_foot(distance: Metres) -> Self {
        Self {
            mode: Mode::Foot,
            distance,
            time: walking_speed().time_to_cover(distance),
        }
    }

    pub fn by_bus(distance: Metres, time: Seconds) -> Self {
        Self {
            mode: Mode::Bus,
            distance,
            time,
        }
    }

    pub fn is_travelling(&self) -> bool {
        self.mode != Mode::None
    }
}

impl Default for Commute {
    fn default() -> Self {
        Self::NONE
    }
}

/// How a citizen could get from `home` to `work`, if at all.
///
/// Walking is tried first and always wins when it is possible, because a seat
/// spent on someone who could have walked is a seat denied to someone who could
/// not. That ordering is the entire allocation policy and it is deliberate: any
/// cleverer scheme would be the simulation making a decision about who deserves
/// a bus, which is not its call.
pub fn reach(home: Point, work: Point, roads: &RoadNetwork) -> Option<Commute> {
    let straight = crate::citizen::commute_distance(home, work, roads);
    if straight.0 <= MAX_WALK.0 {
        return Some(Commute::on_foot(straight));
    }
    ride(home, work, roads)
}

/// The bus half of [`reach`], separated so a caller that has already spent a
/// seat budget can ask for it specifically.
///
/// `None` when either end is out of walking range of the network, when the two
/// are not connected by road, or when the whole journey would take longer than
/// anyone will travel.
pub fn ride(home: Point, work: Point, roads: &RoadNetwork) -> Option<Commute> {
    let start = roads.nearest_node(home, STOP_WALK)?;
    let finish = roads.nearest_node(work, STOP_WALK)?;
    let route = roads.route(start, finish)?;

    let walk = walking_speed();
    let to_stop = roads.position_of(start)?.distance_to(home);
    let from_stop = roads.position_of(finish)?.distance_to(work);
    let time = walk.time_to_cover(to_stop)
        + bus_speed().time_to_cover(route.distance)
        + walk.time_to_cover(from_stop);

    if time.0 > MAX_COMMUTE.0 {
        return None;
    }
    Some(Commute::by_bus(to_stop + route.distance + from_stop, time))
}

/// How many people the republic's bus depots can carry today.
///
/// Reads **yesterday's** staffing, because a depot staffed yesterday is what
/// runs buses this morning. Deriving it from the staffing this same pass is
/// about to compute would be circular — a depot could staff itself by carrying
/// its own drivers to work.
pub fn seats(buildings: &Buildings) -> u32 {
    buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.kind == BuildingKind::BusDepot)
        .map(|b| {
            let running = b.staffing().min(fuel_fraction(b));
            (f64::from(b.def().seats) * running) as u32
        })
        .sum()
}

/// What share of a depot's buses can actually roll, from what is in its tank.
///
/// A depot with no fuel runs no buses at all, and one with a little runs a few —
/// the same shape as every other input limiter in the simulation, and for the
/// same reason: a shortage should degrade a service rather than switch it off
/// at a threshold nobody can see.
fn fuel_fraction(building: &crate::building::Building) -> f64 {
    let seats = f64::from(building.def().seats);
    if seats <= 0.0 {
        return 0.0;
    }
    let wanted = seats * FUEL_PER_SEAT_DAY;
    if wanted <= 0.0 {
        return 1.0;
    }
    (building.stock.get(Resource::Fuel).0 / wanted).clamp(0.0, 1.0)
}

/// Book `used` seats against the depots that provided them, cheapest to run
/// first, and report the fuel each one burns.
///
/// Depots are drawn down in id order so the answer is reproducible, and each is
/// filled to its own running capacity before the next is touched — a half-used
/// fleet is one depot working and another idle, not two depots half-idling.
pub fn fuel_burn(
    buildings: &Buildings,
    mut used: u32,
) -> Vec<(crate::building::BuildingId, Tonnes)> {
    let mut depots: Vec<_> = buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.kind == BuildingKind::BusDepot)
        .collect();
    depots.sort_by_key(|b| b.id);

    let mut out = Vec::new();
    for depot in depots {
        if used == 0 {
            break;
        }
        let running = depot.staffing().min(fuel_fraction(depot));
        let capacity = (f64::from(depot.def().seats) * running) as u32;
        let taken = used.min(capacity);
        if taken == 0 {
            continue;
        }
        used -= taken;
        out.push((depot.id, Tonnes(f64::from(taken) * FUEL_PER_SEAT_DAY)));
    }
    out
}

/// Whether a building is close enough to a road to be served by freight.
pub fn is_road_served(at: Point, roads: &RoadNetwork) -> bool {
    roads.nearest_node(at, ROAD_ACCESS).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geology::Geology;
    use crate::road::default_road_speed;
    use crate::terrain::Terrain;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    /// A road from one end to the other, with junctions every 500 m.
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
    fn a_short_hop_is_walked_and_never_needs_a_seat() {
        let roads = highway(10_000.0);
        let commute =
            reach(at(0.0, 50.0), at(1_000.0, 50.0), &roads).expect("within walking range");
        assert_eq!(commute.mode, Mode::Foot);
        // A kilometre at 5 km/h is twelve minutes.
        assert!((commute.time.as_minutes() - 12.0).abs() < 0.1);
    }

    /// The mechanic, stated as a test: a job out of walking range is reachable
    /// by bus when there is road to it, and not otherwise.
    #[test]
    fn a_bus_reaches_what_walking_cannot_but_only_along_road() {
        let home = at(0.0, 50.0);
        let far_work = at(6_000.0, 50.0);

        // No roads at all: out of range, exactly as before transport existed.
        assert!(reach(home, far_work, &RoadNetwork::new()).is_none());

        let roads = highway(10_000.0);
        let commute = reach(home, far_work, &roads).expect("the bus should reach it");
        assert_eq!(commute.mode, Mode::Bus);
        assert!(commute.distance.0 >= 6_000.0);
        // 6 km at 20 km/h is eighteen minutes, plus a short walk at each end.
        assert!(
            (18.0..25.0).contains(&commute.time.as_minutes()),
            "{} minutes",
            commute.time.as_minutes()
        );
    }

    /// Road that goes past the door but not to the work is no help.
    #[test]
    fn a_road_that_does_not_connect_the_two_ends_carries_nobody() {
        let mut roads = highway(3_000.0);
        // A second, separate road out by the distant workplace.
        let a = roads.add_node(at(8_000.0, 0.0));
        let b = roads.add_node(at(8_500.0, 0.0));
        roads.connect(a, b, default_road_speed());

        assert!(reach(at(0.0, 50.0), at(8_200.0, 50.0), &roads).is_none());
    }

    /// Nobody commutes for two hours. The bound is on time, which is what makes
    /// a faster road genuinely extend reach rather than just shorten a trip.
    #[test]
    fn no_one_travels_longer_than_they_will_stand() {
        let roads = highway(40_000.0);
        // 12 km of riding is inside the limit; 30 km is not.
        assert!(reach(at(0.0, 50.0), at(12_000.0, 50.0), &roads).is_some());
        let far = reach(at(0.0, 50.0), at(30_000.0, 50.0), &roads);
        assert!(far.is_none(), "{far:?} is a two-hour commute each way");
    }

    /// A stop has to be within walking distance of both ends, or the bus is
    /// something you can see and not something you can catch.
    #[test]
    fn a_stop_out_of_walking_range_is_not_a_stop() {
        let roads = highway(10_000.0);
        // Home is a kilometre off the road — too far to walk to any stop.
        let stranded = at(0.0, 1_500.0);
        assert!(ride(stranded, at(6_000.0, 50.0), &roads).is_none());
        // Just inside the stop walk, and it works.
        let near = at(0.0, STOP_WALK.0 - 50.0);
        assert!(ride(near, at(6_000.0, 50.0), &roads).is_some());
    }

    fn depot_world() -> (Buildings, Terrain, Geology) {
        (
            Buildings::new(),
            Terrain::flat(Metres(4_000.0)),
            Geology::new(),
        )
    }

    #[test]
    fn an_unstaffed_or_dry_depot_carries_nobody() {
        let (mut b, t, g) = depot_world();
        let id = b
            .place_built(BuildingKind::BusDepot, at(500.0, 500.0), &t, &g)
            .expect("open ground");
        assert_eq!(seats(&b), 0, "an unstaffed depot runs no buses");

        b.get_mut(id).unwrap().staff = BuildingKind::BusDepot.def().workers;
        assert_eq!(seats(&b), 0, "a dry depot runs no buses either");

        b.get_mut(id)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(10.0));
        assert_eq!(seats(&b), BuildingKind::BusDepot.def().seats);
    }

    /// A shortage degrades the service rather than switching it off — the same
    /// shape as every other input limiter in the simulation.
    #[test]
    fn a_half_fuelled_depot_runs_half_a_service() {
        let (mut b, t, g) = depot_world();
        let id = b
            .place_built(BuildingKind::BusDepot, at(500.0, 500.0), &t, &g)
            .unwrap();
        let def = BuildingKind::BusDepot.def();
        b.get_mut(id).unwrap().staff = def.workers;

        let full_day = f64::from(def.seats) * FUEL_PER_SEAT_DAY;
        b.get_mut(id)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(full_day / 2.0));
        let running = seats(&b);
        assert!(
            running > 0 && running <= def.seats / 2 + 1,
            "{running} seats on half a tank"
        );
    }

    #[test]
    fn seats_are_drawn_from_one_depot_before_the_next() {
        let (mut b, t, g) = depot_world();
        let def = BuildingKind::BusDepot.def();
        let first = b
            .place_built(BuildingKind::BusDepot, at(500.0, 500.0), &t, &g)
            .unwrap();
        let second = b
            .place_built(BuildingKind::BusDepot, at(1_500.0, 500.0), &t, &g)
            .unwrap();
        for id in [first, second] {
            let depot = b.get_mut(id).unwrap();
            depot.staff = def.workers;
            depot.stock.add(Resource::Fuel, Tonnes(50.0));
        }

        // Fewer riders than one depot holds: only the first one works.
        let burn = fuel_burn(&b, def.seats / 2);
        assert_eq!(burn.len(), 1);
        assert_eq!(burn[0].0, first);

        // More than one holds: the second picks up the remainder.
        let burn = fuel_burn(&b, def.seats + 10);
        assert_eq!(burn.len(), 2);
        assert_eq!(burn[1].0, second);
        let total: f64 = burn.iter().map(|(_, t)| t.0).sum();
        assert!(
            (total - f64::from(def.seats + 10) * FUEL_PER_SEAT_DAY).abs() < 1e-9,
            "fuel burnt must match seats filled"
        );
    }

    #[test]
    fn nobody_riding_burns_nothing() {
        let (mut b, t, g) = depot_world();
        let id = b
            .place_built(BuildingKind::BusDepot, at(500.0, 500.0), &t, &g)
            .unwrap();
        b.get_mut(id).unwrap().staff = BuildingKind::BusDepot.def().workers;
        b.get_mut(id)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(50.0));
        assert!(fuel_burn(&b, 0).is_empty());
    }
}
