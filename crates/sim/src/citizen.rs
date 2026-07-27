//! Citizens: individual people who live somewhere and have to get to work.
//!
//! # Why individuals
//!
//! The archived build's labour model was a single scalar. `workers()` built one
//! global list of every connected workplace, sorted it by priority, and spread
//! `pop * workerShare` across it — so a citizen in the far south staffed a mine
//! in the far north for free, and nobody lived anywhere.
//!
//! That is what this replaces. A citizen has a home, and a job only if there is
//! one they can actually reach. Everything interesting follows from that single
//! constraint: towns form around work, a remote mine needs housing built beside
//! it, and when its seam runs out that housing is stranded. **Depletion plus
//! commute is the acceptance scenario for this module** — see
//! `a_mining_town_dies_when_its_work_does`.
//!
//! # ECS, and the three things it does not give for free
//!
//! Storage is `bevy_ecs` standalone — no renderer, no app, no scheduler. Using
//! the crate Bevy is built on keeps that shell option additive rather than a
//! rewrite. Three things need care:
//!
//! 1. **Query order is not identity.** It follows table order, which changes as
//!    entities are despawned and slots reused. Every citizen carries a
//!    [`CitizenId`] and [`Population::records`] sorts by it. Relying on query
//!    order would be a determinism bug that first appears after a death.
//! 2. **A `bevy_ecs::World` is not `Serialize`, `Clone` or `PartialEq`.** So
//!    [`Population`] is all three by way of its records, and rebuilds by
//!    respawning.
//! 3. **Entity handles are slot indices and are never persisted.** A save that
//!    stored one would be reading a different citizen after a reload.

use crate::building::{BuildingId, Buildings};
use crate::road::RoadNetwork;
use crate::units::{Metres, Point, Speed};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryState;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How far someone will walk to work, each way.
///
/// Soviet citizens walked a long way to work; this is a forgiving figure.
/// Beyond it a job is simply not reachable. Transport that extends this is a
/// later mechanic, and until it exists the constraint is the entire point.
pub const MAX_WALK: Metres = Metres(2_000.0);

/// How far a junction can be from a building and still serve it.
pub const ROAD_ACCESS: Metres = Metres(300.0);

/// The years during which someone holds a job.
pub const WORKING_AGE: std::ops::Range<u32> = 16..60;

/// An unhurried adult pace.
pub fn walking_speed() -> Speed {
    Speed::from_kph(5.0)
}

/// A stable identity, independent of ECS storage.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CitizenId(pub u32);

/// Where a citizen lives.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Home(pub BuildingId);

/// Where a citizen works, if anywhere.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workplace(pub Option<BuildingId>);

/// Years lived.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Age(pub u32);

/// One citizen, flattened — the save representation and the ordered view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitizenRecord {
    pub id: CitizenId,
    pub home: Home,
    pub workplace: Workplace,
    pub age: Age,
}

impl CitizenRecord {
    pub fn can_work(&self) -> bool {
        WORKING_AGE.contains(&self.age.0)
    }
}

type CitizenQuery = (
    &'static CitizenId,
    &'static Home,
    &'static Workplace,
    &'static Age,
);

/// Everyone in the republic.
pub struct Population {
    world: World,
    /// Cached so [`Population::records`] can read through a shared reference —
    /// building a query needs `&mut World`, iterating a built one does not.
    query: QueryState<CitizenQuery>,
    next_id: u32,
}

impl Population {
    pub fn new() -> Self {
        let mut world = World::new();
        let query = world.query::<CitizenQuery>();
        Self {
            world,
            query,
            next_id: 1,
        }
    }

    /// Add a citizen.
    pub fn spawn_citizen(&mut self, home: BuildingId, age: u32) -> CitizenId {
        let id = CitizenId(self.next_id);
        self.next_id += 1;
        self.world
            .spawn((id, Home(home), Workplace(None), Age(age)));
        // The cached query must learn about the archetype the first spawn
        // creates, or `iter_manual` sees nothing at all.
        self.query.update_archetypes(&self.world);
        id
    }

    /// Remove a citizen.
    pub fn remove(&mut self, id: CitizenId) -> bool {
        let mut finder = self.world.query::<(Entity, &CitizenId)>();
        let Some(entity) = finder
            .iter(&self.world)
            .find(|(_, cid)| **cid == id)
            .map(|(e, _)| e)
        else {
            return false;
        };
        self.world.despawn(entity);
        self.query.update_archetypes(&self.world);
        true
    }

    /// Everyone, **sorted by id**. Every system that walks the population walks
    /// it through here, never through raw query order.
    pub fn records(&self) -> Vec<CitizenRecord> {
        let mut out: Vec<CitizenRecord> = self
            .query
            .iter_manual(&self.world)
            .map(|(&id, &home, &workplace, &age)| CitizenRecord {
                id,
                home,
                workplace,
                age,
            })
            .collect();
        out.sort_by_key(|c| c.id);
        out
    }

    pub fn count(&self) -> usize {
        self.records().len()
    }

    pub fn employed(&self) -> usize {
        self.records()
            .iter()
            .filter(|c| c.workplace.0.is_some())
            .count()
    }

    pub fn residents_of(&self, building: BuildingId) -> Vec<CitizenRecord> {
        self.records()
            .into_iter()
            .filter(|c| c.home.0 == building)
            .collect()
    }

    pub fn staff_of(&self, building: BuildingId) -> u32 {
        self.records()
            .iter()
            .filter(|c| c.workplace.0 == Some(building))
            .count() as u32
    }

    /// Apply a set of workplace assignments, keyed by citizen id.
    fn apply_workplaces(&mut self, assignment: &[(CitizenId, Option<BuildingId>)]) {
        let mut query = self.world.query::<(&CitizenId, &mut Workplace)>();
        for (id, mut workplace) in query.iter_mut(&mut self.world) {
            if let Ok(index) = assignment.binary_search_by_key(id, |(i, _)| *i) {
                workplace.0 = assignment[index].1;
            }
        }
    }

    fn from_records(records: &[CitizenRecord], next_id: u32) -> Self {
        let mut population = Self::new();
        for record in records {
            population
                .world
                .spawn((record.id, record.home, record.workplace, record.age));
        }
        population.query.update_archetypes(&population.world);
        population.next_id = next_id;
        population
    }
}

impl Default for Population {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Population {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Population")
            .field("count", &self.count())
            .field("next_id", &self.next_id)
            .finish()
    }
}

impl Clone for Population {
    fn clone(&self) -> Self {
        Self::from_records(&self.records(), self.next_id)
    }
}

impl PartialEq for Population {
    fn eq(&self, other: &Self) -> bool {
        self.next_id == other.next_id && self.records() == other.records()
    }
}

/// The serialized shape. `next_id` travels too — without it a reloaded
/// republic would reissue ids that are already in use.
#[derive(Serialize, Deserialize)]
struct PopulationSave {
    next_id: u32,
    citizens: Vec<CitizenRecord>,
}

impl Serialize for Population {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PopulationSave {
            next_id: self.next_id,
            citizens: self.records(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Population {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let save = PopulationSave::deserialize(deserializer)?;
        Ok(Self::from_records(&save.citizens, save.next_id))
    }
}

/// The walking distance from a home to a workplace.
///
/// Uses the road network when both ends are near one — walking round a lake on
/// a road is a real route, where the straight line through it is not. Falls
/// back to the straight line otherwise, which is the right answer on open
/// ground, and never claims a road is longer than walking directly.
pub fn commute_distance(home: Point, work: Point, roads: &RoadNetwork) -> Metres {
    let straight = home.distance_to(work);
    let by_road = (|| {
        let a = roads.nearest_node(home, ROAD_ACCESS)?;
        let b = roads.nearest_node(work, ROAD_ACCESS)?;
        let route = roads.route(a, b)?;
        Some(
            roads.position_of(a)?.distance_to(home)
                + route.distance
                + roads.position_of(b)?.distance_to(work),
        )
    })();
    match by_road {
        Some(d) if d.0 < straight.0 => d,
        _ => straight,
    }
}

/// Whether someone living at `home` could hold a job at `work`.
pub fn is_reachable(home: Point, work: Point, roads: &RoadNetwork) -> bool {
    commute_distance(home, work, roads).0 <= MAX_WALK.0
}

/// Match people to jobs they can reach, and report the staffing that results.
///
/// Buildings are filled in id order — commissioning order, the same tie-break
/// the archived build used — and each takes the nearest available workers
/// first. Ties break on citizen id so the assignment is reproducible.
///
/// Deliberately **not** a global pool. A job nobody can reach goes unfilled,
/// however many unemployed people the republic has, and that is the entire
/// behavioural difference from the model this replaces.
pub fn assign_labour(
    population: &mut Population,
    buildings: &Buildings,
    roads: &RoadNetwork,
) -> Vec<(BuildingId, u32)> {
    let people = population.records();
    let home_of = |record: &CitizenRecord| {
        buildings
            .get(record.home.0)
            .filter(|b| b.is_built())
            .map(|b| b.centre)
    };

    // Everyone of working age whose home still stands.
    let mut available: Vec<(CitizenId, Point)> = people
        .iter()
        .filter(|c| c.can_work())
        .filter_map(|c| home_of(c).map(|p| (c.id, p)))
        .collect();

    let mut assignment: Vec<(CitizenId, Option<BuildingId>)> =
        people.iter().map(|c| (c.id, None)).collect();
    let mut staffing = Vec::new();

    // Only finished buildings employ anyone. A site is worked by builders, who
    // are staff of a Construction Office, not of the thing being built.
    let mut workplaces: Vec<_> = buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().workers > 0)
        .collect();
    workplaces.sort_by_key(|b| b.id);

    for workplace in workplaces {
        let jobs = workplace.def().workers as usize;

        let mut candidates: Vec<(Metres, CitizenId)> = available
            .iter()
            .map(|&(id, home)| (commute_distance(home, workplace.centre, roads), id))
            .filter(|(distance, _)| distance.0 <= MAX_WALK.0)
            .collect();
        candidates.sort_by(|(da, ia), (db, ib)| da.0.total_cmp(&db.0).then_with(|| ia.cmp(ib)));

        let hired: Vec<CitizenId> = candidates
            .into_iter()
            .take(jobs)
            .map(|(_, id)| id)
            .collect();
        for id in &hired {
            if let Ok(index) = assignment.binary_search_by_key(id, |(i, _)| *i) {
                assignment[index].1 = Some(workplace.id);
            }
        }
        available.retain(|(id, _)| !hired.contains(id));
        staffing.push((workplace.id, hired.len() as u32));
    }

    population.apply_workplaces(&assignment);
    staffing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::{BuildingKind, Buildings};
    use crate::geology::{Deposit, DepositId, Geology, Layer, Mineral};
    use crate::terrain::Terrain;
    use crate::units::Tonnes;

    fn ground() -> Terrain {
        Terrain::flat(Metres(20_000.0))
    }

    fn coal_at(centre: Point) -> Geology {
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::Coal,
            centre,
            Metres(200.0),
            Metres(30.0),
            vec![Layer::new(Metres(10.0), Tonnes(1_000.0))],
        ));
        g
    }

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    #[test]
    fn citizens_are_stored_and_come_back_in_id_order() {
        let mut p = Population::new();
        let a = p.spawn_citizen(BuildingId(1), 30);
        let b = p.spawn_citizen(BuildingId(1), 40);
        assert_eq!(p.count(), 2);
        let ids: Vec<_> = p.records().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a, b]);
    }

    /// The determinism hazard the CitizenId exists for: after a despawn, ECS
    /// slot reuse can hand back a different iteration order. Sorted records
    /// must not care.
    #[test]
    fn order_survives_deaths_and_births() {
        let mut p = Population::new();
        let ids: Vec<_> = (0..6)
            .map(|i| p.spawn_citizen(BuildingId(1), 20 + i))
            .collect();
        assert!(p.remove(ids[2]));
        assert!(p.remove(ids[0]));
        let fresh = p.spawn_citizen(BuildingId(1), 25);

        let seen: Vec<_> = p.records().iter().map(|c| c.id).collect();
        let mut expected = vec![ids[1], ids[3], ids[4], ids[5], fresh];
        expected.sort();
        assert_eq!(seen, expected);
        assert!(!p.remove(ids[0]), "removing twice is not a second success");
    }

    #[test]
    fn a_population_survives_a_save() {
        let mut p = Population::new();
        for i in 0..20 {
            p.spawn_citizen(BuildingId(1 + i % 3), 20 + i);
        }
        p.remove(CitizenId(5));

        let wire = postcard::to_stdvec(&p).expect("serializes");
        let back: Population = postcard::from_bytes(&wire).expect("parses");
        assert_eq!(back, p);

        // And it must not reissue ids already in use.
        let mut back = back;
        let next = back.spawn_citizen(BuildingId(1), 30);
        assert!(
            next.0 > p.records().last().unwrap().id.0,
            "a reloaded republic reissued a live id"
        );
    }

    #[test]
    fn only_working_age_citizens_take_jobs() {
        let t = ground();
        let g = Geology::new();
        let mut b = Buildings::new();
        let home = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_000.0), &t, &g)
            .unwrap();
        let mill = b
            .place_built(BuildingKind::Sawmill, at(1_100.0, 1_000.0), &t, &g)
            .unwrap();

        let mut p = Population::new();
        p.spawn_citizen(home, 8); // a child
        p.spawn_citizen(home, 70); // retired
        for _ in 0..4 {
            p.spawn_citizen(home, 30);
        }

        let staffing = assign_labour(&mut p, &b, &RoadNetwork::new());
        assert_eq!(staffing, vec![(mill, 4)]);
        assert_eq!(p.employed(), 4);
    }

    /// The behavioural difference from the archived model, stated as a test:
    /// a job nobody can reach goes unfilled however many unemployed people the
    /// republic has.
    #[test]
    fn a_mine_out_of_walking_range_gets_no_one() {
        let t = ground();
        let far = at(9_000.0, 9_000.0);
        let g = coal_at(far);
        let mut b = Buildings::new();
        let home = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_000.0), &t, &g)
            .unwrap();
        let mine = b.place_built(BuildingKind::CoalMine, far, &t, &g).unwrap();

        let mut p = Population::new();
        for _ in 0..48 {
            p.spawn_citizen(home, 30);
        }

        let staffing = assign_labour(&mut p, &b, &RoadNetwork::new());
        assert_eq!(
            staffing,
            vec![(mine, 0)],
            "the old model would have staffed it"
        );
        assert_eq!(p.employed(), 0);
    }

    #[test]
    fn housing_beside_the_mine_staffs_it() {
        let t = ground();
        let far = at(9_000.0, 9_000.0);
        let g = coal_at(far);
        let mut b = Buildings::new();
        let mine = b.place_built(BuildingKind::CoalMine, far, &t, &g).unwrap();
        let camp = b
            .place_built(BuildingKind::Apartment, at(9_300.0, 9_000.0), &t, &g)
            .unwrap();

        let mut p = Population::new();
        for _ in 0..20 {
            p.spawn_citizen(camp, 30);
        }

        let staffing = assign_labour(&mut p, &b, &RoadNetwork::new());
        assert_eq!(staffing, vec![(mine, 14)], "a mining town staffs its mine");
    }

    /// The acceptance scenario for the whole module: build a town around a
    /// remote seam, work it out, close the mine — and the town has nothing
    /// left within reach. That is a mining town dying, and it is only possible
    /// because work has a location.
    #[test]
    fn a_mining_town_dies_when_its_work_does() {
        let t = ground();
        let far = at(9_000.0, 9_000.0);
        let mut g = coal_at(far);
        let mut b = Buildings::new();
        let mine = b.place_built(BuildingKind::CoalMine, far, &t, &g).unwrap();
        let camp = b
            .place_built(BuildingKind::Apartment, at(9_300.0, 9_000.0), &t, &g)
            .unwrap();
        // A city far away, with work — but not work these people can reach.
        let city = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_000.0), &t, &g)
            .unwrap();
        let city_mill = b
            .place_built(BuildingKind::Sawmill, at(1_200.0, 1_000.0), &t, &g)
            .unwrap();

        let mut p = Population::new();
        for _ in 0..20 {
            p.spawn_citizen(camp, 30);
        }
        for _ in 0..10 {
            p.spawn_citizen(city, 30);
        }

        let roads = RoadNetwork::new();
        assign_labour(&mut p, &b, &roads);
        assert_eq!(p.staff_of(mine), 14, "the town works its mine");

        // The seam runs out and the mine closes.
        g.get_mut(DepositId(1)).unwrap().extract(Tonnes(1_000.0));
        assert!(g.get(DepositId(1)).unwrap().is_exhausted());
        b.demolish(mine);

        assign_labour(&mut p, &b, &roads);
        let stranded = p.residents_of(camp);
        assert_eq!(stranded.len(), 20, "they still live there");
        assert!(
            stranded.iter().all(|c| c.workplace.0.is_none()),
            "the town should have no work within reach"
        );
        // The city is unaffected — this is a local collapse, not a global one.
        // Its sawmill stays fully staffed, and only by people who live there.
        assert_eq!(p.staff_of(city_mill), 6);
        assert!(
            p.records()
                .iter()
                .filter(|c| c.workplace.0 == Some(city_mill))
                .all(|c| c.home.0 == city),
            "the mining town cannot commute to the city's work"
        );
    }

    /// A road round an obstacle is a real route and should beat the straight
    /// line only when it is genuinely shorter, never when it is longer.
    #[test]
    fn roads_shorten_a_commute_but_never_lengthen_one() {
        let mut roads = RoadNetwork::new();
        let a = roads.add_node(at(0.0, 0.0));
        let b = roads.add_node(at(5_000.0, 0.0));
        roads.connect(a, b, crate::road::default_road_speed());

        // Both ends near the road: the road route is the same as the straight
        // line here, so the straight line must win or tie — never lose.
        let home = at(10.0, 10.0);
        let work = at(4_990.0, 10.0);
        let straight = home.distance_to(work);
        assert!(commute_distance(home, work, &roads).0 <= straight.0 + 1e-9);

        // Far from any road, the straight line is the answer.
        let wild_a = at(0.0, 9_000.0);
        let wild_b = at(500.0, 9_000.0);
        assert_eq!(
            commute_distance(wild_a, wild_b, &roads),
            wild_a.distance_to(wild_b)
        );
    }

    #[test]
    fn assignment_is_reproducible() {
        let t = ground();
        let g = Geology::new();
        let mut b = Buildings::new();
        let home = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_000.0), &t, &g)
            .unwrap();
        b.place_built(BuildingKind::Sawmill, at(1_150.0, 1_000.0), &t, &g)
            .unwrap();
        b.place_built(BuildingKind::Brickworks, at(1_000.0, 1_150.0), &t, &g)
            .unwrap();

        let build = || {
            let mut p = Population::new();
            for _ in 0..30 {
                p.spawn_citizen(home, 30);
            }
            p
        };
        let (mut first, mut second) = (build(), build());
        let a = assign_labour(&mut first, &b, &RoadNetwork::new());
        let c = assign_labour(&mut second, &b, &RoadNetwork::new());
        assert_eq!(a, c);
        assert_eq!(first.records(), second.records());
    }

    #[test]
    fn a_citizen_whose_home_was_demolished_holds_no_job() {
        let t = ground();
        let g = Geology::new();
        let mut b = Buildings::new();
        let home = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_000.0), &t, &g)
            .unwrap();
        b.place_built(BuildingKind::Sawmill, at(1_100.0, 1_000.0), &t, &g)
            .unwrap();

        let mut p = Population::new();
        for _ in 0..6 {
            p.spawn_citizen(home, 30);
        }
        assign_labour(&mut p, &b, &RoadNetwork::new());
        assert_eq!(p.employed(), 6);

        b.demolish(home);
        assign_labour(&mut p, &b, &RoadNetwork::new());
        assert_eq!(
            p.employed(),
            0,
            "nobody commutes from a building that is gone"
        );
    }
}
