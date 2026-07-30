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
use crate::network::Network;
use crate::transport::{self, Commute, Mode};
use crate::units::{Metres, Point, Speed};
use bevy_ecs::prelude::*;
use bevy_ecs::query::QueryState;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// How far someone will walk to work, each way.
///
/// Soviet citizens walked a long way to work; this is a forgiving figure.
/// Beyond it a job is out of reach **on foot** — [`crate::transport`] is what
/// extends it, and it does so by being built rather than by relaxing this.
pub const MAX_WALK: Metres = Metres(2_000.0);

/// How far a junction can be from a building and still serve it.
pub const ROAD_ACCESS: Metres = Metres(300.0);

/// The years during which someone holds a job.
pub const WORKING_AGE: std::ops::Range<u32> = 16..60;

/// The years someone is at school.
pub const SCHOOL_AGE: std::ops::Range<u32> = 6..16;

/// The years someone may be at university, if they finished school.
///
/// It overlaps [`WORKING_AGE`] deliberately and that overlap **is** the cost: a
/// student is a working-age adult who is not working. A republic that sends its
/// young people to university is short of hands for three years to be better
/// off afterwards, which is the whole trade and the reason education is not
/// simply a free upgrade.
pub const UNIVERSITY_AGE: std::ops::Range<u32> = 16..19;

/// Days of attendance that make somebody schooled.
///
/// Five years out of the ten between [`SCHOOL_AGE`]'s ends, so a school built
/// halfway through a child's schooling still catches them and one built the
/// year they leave does not. Attendance is only counted on days a *staffed*
/// school is within reach of where they live — a school with no teachers
/// teaches nobody.
pub const SCHOOL_DAYS: u32 = 5 * crate::time::DAYS_PER_YEAR;

/// Days at university, on top of school, that make somebody a graduate.
pub const UNIVERSITY_DAYS: u32 = 3 * crate::time::DAYS_PER_YEAR;

/// An unhurried adult pace.
pub fn walking_speed() -> Speed {
    Speed::from_kph(5.0)
}

/// What somebody was taught.
///
/// Ordered, and the ordering is load-bearing: [`crate::building::BuildingDef`]
/// authors the minimum a job needs and the labour pass compares against it, so
/// a graduate can work a quarry and an unschooled labourer cannot run a
/// refinery.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum Education {
    /// Nobody taught them anything. The state a republic with no school
    /// produces, one generation later.
    #[default]
    Unschooled,
    Schooled,
    Graduate,
}

impl Education {
    pub const ALL: [Education; 3] = [
        Education::Unschooled,
        Education::Schooled,
        Education::Graduate,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Education::Unschooled => "Unschooled",
            Education::Schooled => "Schooled",
            Education::Graduate => "Graduate",
        }
    }
}

/// Where somebody is in their life.
///
/// Derived rather than stored, from age and whether they are enrolled — a
/// stored copy would be a second source of truth for something age already
/// answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum LifeStage {
    /// Too young for school.
    Infant,
    /// Of school age.
    Pupil,
    /// Of working age and at university instead.
    Student,
    Worker,
    Retired,
}

impl LifeStage {
    pub const ALL: [LifeStage; 5] = [
        LifeStage::Infant,
        LifeStage::Pupil,
        LifeStage::Student,
        LifeStage::Worker,
        LifeStage::Retired,
    ];

    pub fn name(self) -> &'static str {
        match self {
            LifeStage::Infant => "Infants",
            LifeStage::Pupil => "Pupils",
            LifeStage::Student => "Students",
            LifeStage::Worker => "Workers",
            LifeStage::Retired => "Retired",
        }
    }
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

/// What somebody has been taught, and whether they are still being taught it.
///
/// One component rather than two because they are read together everywhere:
/// what you know is a function of the days you attended, and whether you are
/// attending is what decides whether you can hold a job today.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Learning {
    /// Days of attendance, at school and then at university.
    pub days: u32,
    /// Enrolled at a university right now, and therefore not available for
    /// work. Written by the schooling pass, never authored.
    pub studying: bool,
}

impl Learning {
    /// Somebody who has never seen a classroom.
    pub const NONE: Self = Self {
        days: 0,
        studying: false,
    };

    /// Somebody who finished school. What Moscow sends with a posting, and the
    /// bar the next generation has to be given a school to clear.
    pub const SCHOOLED: Self = Self {
        days: SCHOOL_DAYS,
        studying: false,
    };

    /// What the days add up to.
    pub fn attainment(&self) -> Education {
        if self.days >= SCHOOL_DAYS + UNIVERSITY_DAYS {
            Education::Graduate
        } else if self.days >= SCHOOL_DAYS {
            Education::Schooled
        } else {
            Education::Unschooled
        }
    }
}

/// How somebody is, and how they feel about the republic.
///
/// Both `0.0..=1.0`, both individual rather than per-estate: mortality reads
/// health and emigration reads loyalty, and neither is a question about a
/// building.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Wellbeing {
    pub health: f64,
    /// How they feel about living here. Follows the contentment of their home
    /// slowly — see [`crate::wellbeing::LOYALTY_DRIFT`].
    pub loyalty: f64,
}

impl Wellbeing {
    /// Somebody who has just arrived: in good health, and neither committed nor
    /// disaffected.
    pub const ARRIVING: Self = Self {
        health: 0.9,
        loyalty: 0.6,
    };
}

impl Default for Wellbeing {
    fn default() -> Self {
        Self::ARRIVING
    }
}

/// One citizen, flattened — the save representation and the ordered view.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CitizenRecord {
    pub id: CitizenId,
    pub home: Home,
    pub workplace: Workplace,
    pub age: Age,
    /// The journey they actually make. Written by the labour pass alongside the
    /// workplace, because the two are one decision: a job is only a job if
    /// there is a way to get to it.
    pub commute: Commute,
    pub learning: Learning,
    pub wellbeing: Wellbeing,
}

impl CitizenRecord {
    /// Whether they are available to hold a job.
    ///
    /// A student is a working-age adult who is *not* working, and that is what
    /// makes a university a cost as well as an investment.
    pub fn can_work(&self) -> bool {
        WORKING_AGE.contains(&self.age.0) && !self.learning.studying
    }

    pub fn education(&self) -> Education {
        self.learning.attainment()
    }

    pub fn stage(&self) -> LifeStage {
        if self.learning.studying {
            LifeStage::Student
        } else if self.age.0 < SCHOOL_AGE.start {
            LifeStage::Infant
        } else if SCHOOL_AGE.contains(&self.age.0) {
            LifeStage::Pupil
        } else if WORKING_AGE.contains(&self.age.0) {
            LifeStage::Worker
        } else {
            LifeStage::Retired
        }
    }

    /// The day of the year they were born on.
    ///
    /// Derived from the id rather than stored, so ageing is spread evenly over
    /// the year instead of the whole republic having a birthday at once. A
    /// cohort that ages on one day is a cohort that *dies* on one day, and a
    /// population graph with that sawtooth in it is a modelling artefact a
    /// player would rightly read as a bug.
    pub fn birthday(&self) -> u32 {
        self.id.0 % crate::time::DAYS_PER_YEAR
    }

    /// Whether this person needs a seat on a bus to hold their job.
    pub fn rides(&self) -> bool {
        self.commute.is_carried()
    }
}

/// What one home holds, counted in a single walk of the population.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HomeCensus {
    pub residents: u32,
    /// Of working age and not at university.
    pub working_age: u32,
    /// Of those, how many hold a job.
    pub employed: u32,
    /// Of school age, whether or not a school exists to send them to.
    pub pupils: u32,
    /// Of an age to be starting a family.
    pub fertile: u32,
}

/// The years during which a household may grow.
pub const FERTILE_AGE: std::ops::Range<u32> = 20..40;

type CitizenQuery = (
    &'static CitizenId,
    &'static Home,
    &'static Workplace,
    &'static Age,
    &'static Commute,
    &'static Learning,
    &'static Wellbeing,
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
    ///
    /// **An adult conjured into the republic arrives schooled, and a newborn
    /// does not.** That is a rule rather than a convenience: the only two ways
    /// somebody appears here at an adult age are the founding hand Moscow sends
    /// and immigrants walking up to a post, and both of those had their
    /// schooling somewhere else. A person who grows up *inside* the republic
    /// never comes through here as an adult — they are born at nought and age,
    /// so what they know is whatever the republic gave them, which is the whole
    /// point of the attribute.
    ///
    /// Doing it by construction means no caller can create an educated adult by
    /// forgetting to say where they were taught.
    pub fn spawn_citizen(&mut self, home: BuildingId, age: u32) -> CitizenId {
        let learning = if age >= WORKING_AGE.start {
            Learning::SCHOOLED
        } else {
            Learning::NONE
        };
        let id = CitizenId(self.next_id);
        self.next_id += 1;
        self.world.spawn((
            id,
            Home(home),
            Workplace(None),
            Age(age),
            Commute::NONE,
            learning,
            Wellbeing::ARRIVING,
        ));
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
    ///
    /// # This allocates and sorts the whole republic
    ///
    /// Call it when you need everyone in a defined order, and never inside a
    /// loop over something else. The baselines have already caught this once:
    /// the households system called [`Population::residents_of`] — which builds
    /// this same vector — once per home per tick, and at 4,000 citizens that
    /// was **212 ms per simulated day against 23 ms** once the work was done in
    /// one pass. Nothing about the calling code looked wrong.
    ///
    /// For a count, an aggregate or a per-building tally there is a single-pass
    /// method beside this one; for a shell reading it every frame there is no
    /// excuse at all.
    pub fn records(&self) -> Vec<CitizenRecord> {
        let mut out: Vec<CitizenRecord> = self
            .query
            .iter_manual(&self.world)
            .map(
                |(&id, &home, &workplace, &age, &commute, &learning, &wellbeing)| CitizenRecord {
                    id,
                    home,
                    workplace,
                    age,
                    commute,
                    learning,
                    wellbeing,
                },
            )
            .collect();
        out.sort_by_key(|c| c.id);
        out
    }

    /// How many people there are.
    ///
    /// Single-pass on purpose: a count does not care what order it counted in,
    /// so paying for [`Population::records`]'s allocation and sort to read
    /// `.len()` off the result was pure waste on a method this hot.
    pub fn count(&self) -> usize {
        self.query.iter_manual(&self.world).count()
    }

    /// How many hold a job. Order-independent, so single-pass.
    pub fn employed(&self) -> usize {
        self.query
            .iter_manual(&self.world)
            .filter(|(_, _, workplace, _, _, _, _)| workplace.0.is_some())
            .count()
    }

    /// How many people are in each life stage, in one pass.
    ///
    /// Indexed by [`LifeStage::ALL`], because the shell reads it as a packed
    /// array and labels it by position.
    pub fn by_stage(&self) -> [u32; LifeStage::ALL.len()] {
        let mut out = [0u32; LifeStage::ALL.len()];
        for record in self.walk() {
            let stage = record.stage();
            if let Some(i) = LifeStage::ALL.iter().position(|s| *s == stage) {
                out[i] += 1;
            }
        }
        out
    }

    /// How many people hold each level of education, in one pass.
    pub fn by_education(&self) -> [u32; Education::ALL.len()] {
        let mut out = [0u32; Education::ALL.len()];
        for record in self.walk() {
            let level = record.education();
            if let Some(i) = Education::ALL.iter().position(|e| *e == level) {
                out[i] += 1;
            }
        }
        out
    }

    /// Mean health and mean loyalty, in one pass. `(0.0, 0.0)` when empty.
    pub fn mean_wellbeing(&self) -> (f64, f64) {
        let (mut health, mut loyalty, mut n) = (0.0, 0.0, 0u32);
        for (_, _, _, _, _, _, w) in self.query.iter_manual(&self.world) {
            health += w.health;
            loyalty += w.loyalty;
            n += 1;
        }
        if n == 0 {
            return (0.0, 0.0);
        }
        (health / f64::from(n), loyalty / f64::from(n))
    }

    /// Everyone, unsorted, as flattened records.
    ///
    /// **For aggregates and for keyed per-person rolls** — anything whose
    /// answer depends on the order it walked in must go through
    /// [`Population::records`], which allocates and sorts the whole republic.
    ///
    /// The distinction is worth stating because it is not obvious: a mortality
    /// roll is keyed by `(citizen, day)` from its own substream, so *who* dies
    /// does not depend on what order they were considered in. What does depend
    /// on order is the mutation's payload, and sorting a day's handful of
    /// birthdays is nothing beside sorting four thousand people to find them.
    ///
    /// **Measured, and the honest figure is small**: at 4,000 citizens a
    /// simulated day costs 26–27 ms with four full sorts in the daily people
    /// pass and 25–26 ms with two. Worth having and not worth claiming more
    /// for. The first reading said 45 ms against 33 ms and both were wrong —
    /// the baseline suite runs its twelve tests concurrently, so a figure taken
    /// while eleven others are competing for the cores is not comparable with
    /// one taken alone. Run the axis on its own before believing a number moved.
    pub(crate) fn walk(&self) -> impl Iterator<Item = CitizenRecord> + '_ {
        self.query.iter_manual(&self.world).map(
            |(&id, &home, &workplace, &age, &commute, &learning, &wellbeing)| CitizenRecord {
                id,
                home,
                workplace,
                age,
                commute,
                learning,
                wellbeing,
            },
        )
    }

    pub fn residents_of(&self, building: BuildingId) -> Vec<CitizenRecord> {
        self.records()
            .into_iter()
            .filter(|c| c.home.0 == building)
            .collect()
    }

    /// How many people live in each building, in one pass.
    ///
    /// Exists because [`Population::residents_of`] is the wrong tool inside a
    /// loop over buildings and the baselines proved it: the households system
    /// called it once per home per tick, and each call built and sorted the
    /// entire population. At 4,000 citizens that was most of the cost of a
    /// simulated day. This walks the ECS once and counts.
    pub fn residents_by_home(&self) -> BTreeMap<BuildingId, u32> {
        let mut counts = BTreeMap::new();
        for (_, home, _, _, _, _, _) in self.query.iter_manual(&self.world) {
            *counts.entry(home.0).or_insert(0) += 1;
        }
        counts
    }

    /// How many people live in each building, how many of them are of working
    /// age, how many of those hold a job, and how many are of school age — all
    /// in one pass.
    ///
    /// One walk rather than four, because the contentment pass needs every one
    /// of these per home and this population is walked once per day already.
    /// The lesson `residents_of` taught, applied before it could be relearned.
    pub fn census_by_home(&self) -> BTreeMap<BuildingId, HomeCensus> {
        let mut out: BTreeMap<BuildingId, HomeCensus> = BTreeMap::new();
        for record in self.walk() {
            let entry = out.entry(record.home.0).or_default();
            entry.residents += 1;
            if record.can_work() {
                entry.working_age += 1;
                if record.workplace.0.is_some() {
                    entry.employed += 1;
                }
            }
            if SCHOOL_AGE.contains(&record.age.0) {
                entry.pupils += 1;
            }
            if FERTILE_AGE.contains(&record.age.0) {
                entry.fertile += 1;
            }
        }
        out
    }

    /// How many people work at a building. Order-independent, so single-pass.
    pub fn staff_of(&self, building: BuildingId) -> u32 {
        self.query
            .iter_manual(&self.world)
            .filter(|(_, _, workplace, _, _, _, _)| workplace.0 == Some(building))
            .count() as u32
    }

    /// How many people currently hold a job they can only reach by bus.
    pub fn riders(&self) -> u32 {
        self.query
            .iter_manual(&self.world)
            .filter(|(_, _, _, _, commute, _, _)| commute.is_carried())
            .count() as u32
    }

    /// Age everybody in the list by a year.
    pub(crate) fn age_by_one(&mut self, ids: &[CitizenId]) {
        let mut query = self.world.query::<(&CitizenId, &mut Age)>();
        for (id, mut age) in query.iter_mut(&mut self.world) {
            if ids.binary_search(id).is_ok() {
                age.0 += 1;
            }
        }
    }

    /// Write a day of schooling: who attended, and who is enrolled at a
    /// university right now.
    ///
    /// `studying` is set from the census wholesale rather than toggled, because
    /// enrolment is a daily question — a university that lost its staff stops
    /// having students the same day, and a flag that could only be set would
    /// leave graduates-in-waiting permanently out of the workforce.
    pub(crate) fn school(&mut self, attended: &[CitizenId], enrolled: &[CitizenId]) {
        let mut query = self.world.query::<(&CitizenId, &mut Learning)>();
        for (id, mut learning) in query.iter_mut(&mut self.world) {
            if attended.binary_search(id).is_ok() {
                learning.days += 1;
            }
            let studying = enrolled.binary_search(id).is_ok();
            if learning.studying != studying {
                learning.studying = studying;
            }
        }
    }

    /// Write the day's health and loyalty.
    pub(crate) fn set_wellbeing(&mut self, updates: &[(CitizenId, Wellbeing)]) {
        let mut query = self.world.query::<(&CitizenId, &mut Wellbeing)>();
        for (id, mut wellbeing) in query.iter_mut(&mut self.world) {
            if let Ok(index) = updates.binary_search_by_key(id, |(i, _)| *i) {
                *wellbeing = updates[index].1;
            }
        }
    }

    /// Apply a set of workplace assignments and the journeys they imply.
    ///
    /// Both together, never separately: a workplace without its journey would
    /// leave a citizen holding a job with a stale commute attached, and the
    /// commute is what says whether that job is reachable at all.
    fn apply_workplaces(&mut self, assignment: &[(CitizenId, Option<BuildingId>, Commute)]) {
        let mut query = self
            .world
            .query::<(&CitizenId, &mut Workplace, &mut Commute)>();
        for (id, mut workplace, mut commute) in query.iter_mut(&mut self.world) {
            if let Ok(index) = assignment.binary_search_by_key(id, |(i, _, _)| *i) {
                workplace.0 = assignment[index].1;
                *commute = assignment[index].2;
            }
        }
    }

    fn from_records(records: &[CitizenRecord], next_id: u32) -> Self {
        let mut population = Self::new();
        for record in records {
            population.world.spawn((
                record.id,
                record.home,
                record.workplace,
                record.age,
                record.commute,
                record.learning,
                record.wellbeing,
            ));
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
pub fn commute_distance(home: Point, work: Point, roads: &Network) -> Metres {
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

/// Whether someone living at `home` could walk to a job at `work`.
///
/// On foot only. Whether they could get there *at all* is
/// [`crate::transport::reach`], which also knows about buses.
pub fn is_reachable(home: Point, work: Point, roads: &Network) -> bool {
    commute_distance(home, work, roads).0 <= MAX_WALK.0
}

/// What one labour pass decided.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Labour {
    /// Who turned up where.
    pub staffing: Vec<(BuildingId, u32)>,
    /// Seats spent carrying people who could not have walked. What the depots
    /// burn fuel for.
    pub seats_used: u32,
}

/// Match people to jobs they can reach, and report the staffing that results.
///
/// Buildings are filled in id order — commissioning order, the same tie-break
/// the archived build used — and each takes the workers with the shortest
/// journey first. Ties break on citizen id so the assignment is reproducible.
///
/// Deliberately **not** a global pool. A job nobody can reach goes unfilled,
/// however many unemployed people the republic has, and that is the entire
/// behavioural difference from the model this replaces.
///
/// # Seats are spent last
///
/// Candidates who can walk are hired before candidates who would need a seat,
/// whatever their journey times. A seat given to someone who could have walked
/// is a seat denied to someone who could not, and the republic has a finite
/// number of them — so the ordering is not a preference about journey length,
/// it is what stops the bus network being consumed by people who never needed
/// it.
pub fn assign_labour(
    population: &mut Population,
    buildings: &Buildings,
    ways: crate::journey::Ways<'_>,
) -> Labour {
    let people = population.records();
    let home_of = |record: &CitizenRecord| {
        buildings
            .get(record.home.0)
            .filter(|b| b.is_built())
            .map(|b| b.centre)
    };

    // Everyone of working age whose home still stands, with what they know.
    let mut available: Vec<(CitizenId, Point, Education)> = people
        .iter()
        .filter(|c| c.can_work())
        .filter_map(|c| home_of(c).map(|p| (c.id, p, c.education())))
        .collect();

    let mut assignment: Vec<(CitizenId, Option<BuildingId>, Commute)> =
        people.iter().map(|c| (c.id, None, Commute::NONE)).collect();
    let mut staffing = Vec::new();

    // What the depots can carry today. Fixed for the whole pass rather than
    // recomputed per workplace, because it is one fleet serving the republic
    // and not a fresh allowance for every factory.
    // **A pool per way, not one pool.** A republic whose trams are full has not
    // thereby run out of buses, and one pool would make the choice between
    // laying tramway and buying more buses mean nothing.
    let mut services = transport::services(buildings);
    let mut seats_used = 0u32;

    // Only finished buildings employ anyone. A site is worked by builders, who
    // are staff of a Construction Office, not of the thing being built.
    let mut workplaces: Vec<_> = buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.jobs() > 0)
        .collect();
    workplaces.sort_by_key(|b| b.id);

    for workplace in workplaces {
        // **Every shift, not every post.** The authored `workers` is one crew,
        // so a works the player has put on three shifts is asking the republic
        // for three crews — and it goes short exactly as it would if it were
        // three separate factories. That is the whole cost of running the night,
        // and it is charged here rather than anywhere else.
        let jobs = workplace.jobs() as usize;
        // What the job needs to have been taught. A republic with no school is
        // a republic whose next generation cannot run its own mines, which is
        // the entire point of the attribute — and it is checked here rather
        // than in the schooling pass because reachability and qualification are
        // the same question: whether this person can hold this job.
        let needs = workplace.def().schooling;

        // Rank: walkers first, then by journey time, then by id.
        let mut candidates: Vec<(u8, f64, CitizenId, Commute)> = available
            .iter()
            .filter(|&&(_, _, taught)| taught >= needs)
            .filter_map(|&(id, home, _)| {
                let commute = transport::reach_by(home, workplace.centre, ways, &services)?;
                let rank = match commute.mode {
                    Mode::Foot => 0,
                    Mode::Ride(_) => 1,
                    Mode::None => return None,
                };
                Some((rank, commute.time.0, id, commute))
            })
            .collect();
        candidates.sort_by(|(ra, ta, ia, _), (rb, tb, ib, _)| {
            ra.cmp(rb)
                .then_with(|| ta.total_cmp(tb))
                .then_with(|| ia.cmp(ib))
        });

        let mut hired: Vec<CitizenId> = Vec::with_capacity(jobs);
        for (_, _, id, commute) in candidates {
            if hired.len() == jobs {
                break;
            }
            if let Some(medium) = commute.medium() {
                let Some(service) = services.iter_mut().find(|s| s.medium == medium) else {
                    continue;
                };
                if service.seats == 0 {
                    // That service is full. Everyone behind this candidate is
                    // either also a rider or ranked worse, so keep scanning
                    // rather than stopping — a nearer rider might still be a
                    // walker for a later workplace.
                    continue;
                }
                service.seats -= 1;
                seats_used += 1;
            }
            if let Ok(index) = assignment.binary_search_by_key(&id, |(i, _, _)| *i) {
                assignment[index].1 = Some(workplace.id);
                assignment[index].2 = commute;
            }
            hired.push(id);
        }

        available.retain(|(id, _, _)| !hired.contains(id));
        staffing.push((workplace.id, hired.len() as u32));
    }

    population.apply_workplaces(&assignment);
    Labour {
        staffing,
        seats_used,
    }
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

        let labour = assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));
        assert_eq!(labour.staffing, vec![(mill, 4)]);
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

        let labour = assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));
        assert_eq!(
            labour.staffing,
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

        let labour = assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));
        assert_eq!(
            labour.staffing,
            vec![(mine, 14)],
            "a mining town staffs its mine"
        );
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

        let roads = Network::new();
        assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&roads));
        assert_eq!(p.staff_of(mine), 14, "the town works its mine");

        // The seam runs out and the mine closes.
        g.get_mut(DepositId(1)).unwrap().extract(Tonnes(1_000.0));
        assert!(g.get(DepositId(1)).unwrap().is_exhausted());
        b.demolish(mine);

        assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&roads));
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
        let mut roads = Network::new();
        let a = roads.add_node(at(0.0, 0.0));
        let b = roads.add_node(at(5_000.0, 0.0));
        roads.connect(a, b, crate::network::default_road_speed());

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
        let a = assign_labour(
            &mut first,
            &b,
            crate::journey::Ways::on_roads(&Network::new()),
        );
        let c = assign_labour(
            &mut second,
            &b,
            crate::journey::Ways::on_roads(&Network::new()),
        );
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
        assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));
        assert_eq!(p.employed(), 6);

        b.demolish(home);
        assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));
        assert_eq!(
            p.employed(),
            0,
            "nobody commutes from a building that is gone"
        );
    }

    /// The single-pass counters answer exactly what walking the sorted
    /// population answers.
    ///
    /// `count`, `employed`, `staff_of` and `riders` each built and sorted the
    /// whole republic to read a number off the end of it. Counting does not
    /// care what order it counted in, so the sort was pure waste on the hottest
    /// reads in the crate — but "pure waste" is a claim, and this is the
    /// assertion that it was only waste and not also the answer.
    #[test]
    fn the_single_pass_counters_agree_with_walking_every_record() {
        let mut b = Buildings::new();
        let t = ground();
        let g = coal_at(at(1_000.0, 1_000.0));
        let home = b
            .place_built(BuildingKind::Apartment, at(1_000.0, 1_100.0), &t, &g)
            .expect("housing");
        let mine = b
            .place_built(BuildingKind::CoalMine, at(1_000.0, 1_000.0), &t, &g)
            .expect("a mine on the deposit");

        let mut p = Population::new();
        for i in 0..12 {
            p.spawn_citizen(home, 20 + i);
        }
        assign_labour(&mut p, &b, crate::journey::Ways::on_roads(&Network::new()));

        let records = p.records();
        assert_eq!(p.count(), records.len());
        assert_eq!(
            p.employed(),
            records.iter().filter(|c| c.workplace.0.is_some()).count()
        );
        assert_eq!(
            p.staff_of(mine),
            records
                .iter()
                .filter(|c| c.workplace.0 == Some(mine))
                .count() as u32
        );
        assert_eq!(
            p.riders(),
            records.iter().filter(|c| c.rides()).count() as u32
        );

        // The premise: if nobody ever got hired, three of those four
        // comparisons are zero against zero and prove nothing.
        assert!(p.employed() > 0, "nobody was hired, so this proved nothing");
    }
}
