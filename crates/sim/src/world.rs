//! The world: all simulation state, and the tick that advances it.
//!
//! # Determinism rules that live here
//!
//! Two constraints bind anything added to [`World`], and both are easy to
//! violate by accident:
//!
//! 1. **No `HashMap` or `HashSet` in simulation state.** Their iteration order
//!    is randomised per process, so a system that walks one produces a
//!    different order every run and the world diverges. Use `Vec` or
//!    `BTreeMap`. This is the single most likely way determinism gets lost.
//! 2. **No wall-clock, no thread-local, no address-dependent behaviour.** The
//!    only clock is [`SimClock`] and the only randomness is [`Rng`].
//!
//! # Saves
//!
//! [`World`] derives its serialization rather than hand-writing it, on purpose.
//! The archived build's save round-trip test existed to catch fields someone
//! forgot to add to `serialize()`; a derive removes that whole class of bug
//! instead of testing for it. What the round-trip test still earns is proof
//! that reloading resumes the *same future*, which no derive can give you.

use crate::building::Buildings;
use crate::citizen::Population;
use crate::climate::{self, ClimateId};
use crate::contract::Contracts;
use crate::geology::Geology;
use crate::mapgen;
use crate::rng::{Rng, RngState};
use crate::road::RoadNetwork;
use crate::terrain::{self, Terrain};
use crate::time::SimClock;
use crate::trade::{BorderEdge, TradePolicy, Treasury};
use crate::units::Metres;
use serde::{Deserialize, Serialize};

/// Bumped whenever a save can no longer be read by the current code. A load
/// that finds an older version runs migrations; one that finds a newer version
/// refuses, because guessing at a format from the future corrupts silently.
pub const SAVE_VERSION: u32 = 1;

/// Substream identifier for terrain generation.
pub const TERRAIN_STREAM: u64 = 2;

/// Substream identifier for the border edge.
pub const BORDER_STREAM: u64 = 3;

/// Substream identifier for the weather. Its own stream so that reading a
/// forecast never perturbs the economy — the archived build's rule, learned
/// from contract offers.
pub const WEATHER_STREAM: u64 = 4;

/// Substream identifier for foreign trade tenders.
pub const CONTRACT_STREAM: u64 = 5;

/// Mix a seed with a stream identifier.
///
/// The same derivation [`World::substream`] uses, available before a `World`
/// exists — worldgen needs it to build the thing that would otherwise own it.
pub fn derive(seed: u64, purpose: u64) -> u64 {
    let mut h = seed;
    h ^= purpose.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h.rotate_left(31)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The save was written by a newer build than this one.
    FromTheFuture { found: u32, supported: u32 },
    /// The bytes are not a save this build can read.
    Corrupt(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::FromTheFuture { found, supported } => write!(
                f,
                "save format {found} is newer than this build understands ({supported})"
            ),
            SaveError::Corrupt(why) => write!(f, "save could not be read: {why}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// A versioned save. The version travels *outside* the world so it can be read
/// before anything else is interpreted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Save {
    pub version: u32,
    pub world: World,
}

/// Everything needed to found a republic. The founding screen's output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldSpec {
    pub seed: u64,
    /// How far the map reaches, in metres, on each side.
    pub extent: Metres,
    /// Which posting this is. A filter on the founding shelf, and the reason
    /// two candidates from the same seed can be different places to live.
    pub climate: ClimateId,
}

impl WorldSpec {
    /// A ten-kilometre republic on the plains — the working default.
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            extent: Metres(10_000.0),
            climate: ClimateId::Plains,
        }
    }
}

/// All mutable simulation state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub clock: SimClock,
    /// The sequential simulation stream. Systems draw from this in a fixed
    /// order, which is why the order systems run in is load-bearing.
    pub rng: Rng,
    /// The ground.
    pub terrain: Terrain,
    /// What is under the ground, and how much of it is left.
    pub geology: Geology,
    /// What stands on it.
    pub buildings: Buildings,
    /// The roads between them.
    pub roads: RoadNetwork,
    /// The people.
    pub population: Population,
    /// Which edge of the map is foreign soil. A customs house must stand near
    /// it, and nothing else about the republic may depend on where it is.
    pub border: BorderEdge,
    /// Hard currency. Earned at the border, spent at the border, and never on
    /// anything domestic.
    pub treasury: Treasury,
    /// Standing instructions to the customs house.
    pub trade_policy: TradePolicy,
    /// Tenders from the two blocs: offers, live deals and recent history.
    pub contracts: Contracts,
    /// The posting's climate. Fixed at founding — you do not get a milder
    /// winter by asking for one.
    pub climate: ClimateId,
    /// The founding seed, kept so derived substreams can be recomputed from
    /// it at any time without disturbing `rng`.
    seed: u64,
}

impl World {
    /// Found a republic: generate its ground and its geology, and start the
    /// clock.
    ///
    /// Worldgen draws from substreams rather than `rng`, so the main
    /// simulation stream is untouched at tick zero regardless of how much map
    /// was generated — which is what lets the founding screen generate a shelf
    /// of candidates without any of them affecting the one that gets played.
    pub fn new(spec: WorldSpec) -> Self {
        let terrain = terrain::generate_terrain(
            derive(spec.seed, TERRAIN_STREAM),
            spec.extent,
            &terrain::DEFAULT_TERRAIN,
        );
        let geology = mapgen::generate_geology(
            derive(spec.seed, mapgen::GEOLOGY_STREAM),
            spec.extent,
            &mapgen::DEFAULT_PLAN,
        );
        Self {
            clock: SimClock::new(),
            rng: Rng::from_seed(spec.seed),
            terrain,
            geology,
            buildings: Buildings::new(),
            roads: RoadNetwork::new(),
            population: Population::new(),
            // One edge per seed, drawn from its own substream so the choice
            // does not shift when terrain or geology generation changes.
            border: BorderEdge::ALL
                [(Rng::from_seed(derive(spec.seed, BORDER_STREAM)).next_bounded(4)) as usize],
            treasury: Treasury::default(),
            trade_policy: TradePolicy::new(),
            contracts: Contracts::default(),
            climate: spec.climate,
            seed: spec.seed,
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Today's outdoor temperature, in degrees Celsius.
    ///
    /// A pure function of `(seed, climate, day)` drawn from the weather
    /// substream, so it is the same every time it is asked and asking never
    /// moves the simulation's own generator. That also means a forecast is just
    /// this function called with a later day.
    pub fn temperature(&self) -> f64 {
        self.temperature_on_day(self.clock.day_index())
    }

    /// The same for any day, past or future — which is what a forecast is.
    pub fn temperature_on_day(&self, day_index: u64) -> f64 {
        let deviation = self.substream(WEATHER_STREAM, day_index).next_f64();
        let day_of_year = (day_index % u64::from(crate::time::DAYS_PER_YEAR)) as u32;
        climate::temperature_on(self.climate.def(), day_of_year, deviation)
    }

    /// How far a point is from foreign soil.
    pub fn distance_to_border(&self, at: crate::units::Point) -> Metres {
        self.border.distance_from(at, self.terrain.extent())
    }

    /// Put a building up, applying every rule including the ones that need to
    /// know where the border is.
    ///
    /// [`crate::building::Buildings::place`] cannot check the border itself —
    /// it has no idea where the border is, and giving it one would mean handing
    /// the whole world to every placement. This is the layer that knows.
    pub fn place(
        &mut self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<crate::building::BuildingId, crate::building::PlacementError> {
        if kind == crate::building::BuildingKind::Customs
            && self.distance_to_border(at).0 > crate::trade::CUSTOMS_RANGE.0
        {
            return Err(crate::building::PlacementError::NotOnTheBorder);
        }
        self.buildings.place(kind, at, &self.terrain, &self.geology)
    }

    /// The same, already finished — the founding grant.
    pub fn place_built(
        &mut self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<crate::building::BuildingId, crate::building::PlacementError> {
        let id = self.place(kind, at)?;
        if let Some(b) = self.buildings.get_mut(id) {
            b.work_done = b.def().labour;
        }
        Ok(id)
    }

    /// Advance one fixed step.
    ///
    /// Systems will be sequenced here, in an order that is part of the
    /// simulation's definition rather than an implementation detail: they run
    /// in source order, draw from `rng` in that order, and changing the order
    /// changes the world. For now it only moves the clock.
    pub fn tick(&mut self) {
        crate::systems::run_tick(self);
    }

    /// A generator derived from the founding seed, independent of how far the
    /// main stream has advanced.
    ///
    /// This is how a subsystem draws without perturbing everything else. The
    /// archived build learned this the hard way with contract offers: drawing
    /// them from the economy stream meant that merely *looking* at what was on
    /// offer shifted every later economic roll. A derived stream is a pure
    /// function of (seed, purpose, index), so it can be recomputed at any time
    /// and in any order.
    pub fn substream(&self, purpose: u64, index: u64) -> Rng {
        let h = derive(self.seed, purpose) ^ index.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        Rng::from_seed(h)
    }

    /// The current position of the main stream, for inspection and tests.
    pub fn rng_state(&self) -> RngState {
        self.rng.state()
    }

    pub fn to_save(&self) -> Save {
        Save {
            version: SAVE_VERSION,
            world: self.clone(),
        }
    }

    /// Rebuild a world from a save, running migrations for older formats.
    pub fn from_save(save: Save) -> Result<Self, SaveError> {
        if save.version > SAVE_VERSION {
            return Err(SaveError::FromTheFuture {
                found: save.version,
                supported: SAVE_VERSION,
            });
        }
        // Migrations for versions below SAVE_VERSION go here, oldest first.
        // There are none yet because there is only one version.
        Ok(save.world)
    }

    /// Write a save, in the crate's own wire format.
    ///
    /// **The format is not the caller's choice**, and that is the whole reason
    /// this method exists rather than the crate handing out a serde value and
    /// letting a shell pick. The requirement is bit-exact `f64` round-tripping,
    /// found by measurement — `serde_json` changed 91,767 of 200,000 sampled
    /// values because its *parser* is not correctly rounded — and a requirement
    /// a caller can opt out of is not a requirement. `postcard` stores bit
    /// patterns rather than digits, and
    /// [`crate::world::tests::the_save_format_round_trips_floats_bit_exactly`]
    /// guards the format this function actually uses.
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(&self.to_save()).expect("a world is always serializable")
    }

    /// Read a save written by [`World::to_bytes`].
    ///
    /// The version is decoded **on its own, before the world is parsed**, which
    /// is what makes "this save is newer than this build" a distinguishable
    /// answer. A future format that changed the shape of [`World`] would fail
    /// mid-parse if the whole blob were read first, and the player would be told
    /// their save was corrupt when it is merely from a newer build. That is the
    /// point of the version travelling outside the world, and reading it in one
    /// pass with everything else would have quietly thrown it away.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let (version, _) = postcard::take_from_bytes::<u32>(bytes)
            .map_err(|e| SaveError::Corrupt(e.to_string()))?;
        if version > SAVE_VERSION {
            return Err(SaveError::FromTheFuture {
                found: version,
                supported: SAVE_VERSION,
            });
        }
        let save: Save =
            postcard::from_bytes(bytes).map_err(|e| SaveError::Corrupt(e.to_string()))?;
        Self::from_save(save)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::TICKS_PER_DAY;

    /// A 1 km republic — big enough to be a real map, small enough that
    /// fingerprinting it by serialization stays cheap in a test.
    fn spec(seed: u64) -> WorldSpec {
        WorldSpec {
            seed,
            extent: Metres(1_000.0),
            climate: ClimateId::Plains,
        }
    }

    /// A stable 64-bit fingerprint of the whole world.
    ///
    /// Deliberately **not** `std::hash::DefaultHasher`: its algorithm is
    /// explicitly not guaranteed stable across Rust releases, so a tripwire
    /// built on it would be both flaky and itself a determinism violation.
    ///
    /// Hashing the *serialized* form rather than hand-picked fields is the
    /// same reasoning as deriving the save: a field added to `World` enters
    /// the fingerprint automatically, so this cannot rot into checking a
    /// subset of the state while reporting a pass.
    fn fingerprint(world: &World) -> u64 {
        let json = postcard::to_stdvec(world).expect("world must serialize");
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in json {
            h ^= u64::from(byte);
            h = h.wrapping_mul(0x1000_0000_01b3);
        }
        h
    }

    /// Stands in for the systems that do not exist yet: advance the clock and
    /// draw from the main stream the way a real day of simulation will. When
    /// systems land, this becomes a call to run them — the assertions around
    /// it do not change.
    fn simulate_days(world: &mut World, days: u64) {
        for _ in 0..days * TICKS_PER_DAY {
            world.tick();
            world.rng.next_u64();
        }
    }

    /// The tripwire. Ninety days, twice, from one seed.
    #[test]
    fn two_runs_from_the_same_seed_end_identically() {
        let mut a = World::new(spec(1961));
        let mut b = World::new(spec(1961));
        simulate_days(&mut a, 90);
        simulate_days(&mut b, 90);
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_end_differently() {
        let mut a = World::new(spec(1));
        let mut b = World::new(spec(2));
        simulate_days(&mut a, 30);
        simulate_days(&mut b, 30);
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }

    /// The other half of the tripwire, and the one that catches missed state:
    /// save mid-run, reload, then advance the original and the reloaded copy
    /// the same distance. If anything failed to persist, the futures diverge.
    #[test]
    fn a_reloaded_world_resumes_the_same_future() {
        let mut live = World::new(spec(7));
        simulate_days(&mut live, 45);

        // Through the crate's own save API, so this exercises the format the
        // simulation actually writes rather than one a test picked.
        let wire = live.to_bytes();
        let mut reloaded = World::from_bytes(&wire).expect("save must load");
        assert_eq!(reloaded, live, "a fresh reload must equal what was saved");

        simulate_days(&mut live, 45);
        simulate_days(&mut reloaded, 45);
        assert_eq!(
            fingerprint(&live),
            fingerprint(&reloaded),
            "the reloaded world diverged — some state did not survive the save"
        );
    }

    /// Proof that the fingerprint grows with the world rather than checking a
    /// frozen subset: geology was added to `World` after the tripwire existed,
    /// and depleting a seam has to move the fingerprint without anyone having
    /// updated the hash.
    #[test]
    fn state_added_to_the_world_enters_the_fingerprint_automatically() {
        use crate::units::Tonnes;

        let mut world = World::new(spec(1961));
        let id = world.geology.all()[0].id;
        let before = fingerprint(&world);

        world
            .geology
            .get_mut(id)
            .expect("the map generated that body")
            .extract(Tonnes(100.0));

        assert_ne!(
            before,
            fingerprint(&world),
            "working a seam left the fingerprint unchanged — it is not covering geology"
        );
    }

    /// Geology is simulation state, so it has to survive a save like anything
    /// else. Extraction depletes it, and a reload that forgot would hand the
    /// player back a full seam.
    #[test]
    fn a_worked_seam_survives_the_save() {
        use crate::geology::Mineral;
        use crate::units::Tonnes;

        let mut world = World::new(spec(3));
        let id = world
            .geology
            .all()
            .iter()
            .find(|d| d.mineral == Mineral::Coal)
            .expect("every map is planned to hold coal")
            .id;

        let before = world.geology.remaining_of(Mineral::Coal);
        world
            .geology
            .get_mut(id)
            .expect("the map generated that body")
            .extract(Tonnes(250.0));
        let after = world.geology.remaining_of(Mineral::Coal);
        assert_eq!(after, before - Tonnes(250.0), "the seam was worked");

        let reloaded = World::from_bytes(&world.to_bytes()).expect("loads");

        assert_eq!(
            reloaded.geology.remaining_of(Mineral::Coal),
            after,
            "the reload refilled a seam the republic had already worked"
        );
    }

    /// The save format must round-trip `f64` bit-exactly, and this is the
    /// guard that keeps it that way.
    ///
    /// Found by measurement, not by reasoning: the first save format tried was
    /// JSON, and `a_reloaded_world_resumes_the_same_future` failed on a single
    /// deposit coordinate coming back one ULP different. Sampling 200,000 f64
    /// values through `serde_json` showed 91,767 of them changing — the digits
    /// it *writes* are correct and its *parser* is not correctly rounded. A
    /// simulation whose state is full of f64 cannot use a format like that,
    /// and the failure mode is the worst kind: silent, tiny, and only visible
    /// once two runs have diverged far enough to notice.
    #[test]
    fn the_save_format_round_trips_floats_bit_exactly() {
        let mut rng = Rng::from_seed(20_260_726);
        for _ in 0..50_000 {
            // Draw across the whole exponent range, not just [0, 1) — a format
            // can be exact for small values and lossy for large ones.
            let x = f64::from_bits(rng.next_u64());
            if !x.is_finite() {
                continue;
            }
            let wire = postcard::to_stdvec(&x).expect("serializes");
            let back: f64 = postcard::from_bytes(&wire).expect("parses");
            assert_eq!(
                back.to_bits(),
                x.to_bits(),
                "{x:?} did not survive the save format"
            );
        }
    }

    #[test]
    fn a_save_from_the_future_is_refused_rather_than_guessed_at() {
        let save = Save {
            version: SAVE_VERSION + 1,
            world: World::new(spec(1)),
        };
        assert_eq!(
            World::from_save(save),
            Err(SaveError::FromTheFuture {
                found: SAVE_VERSION + 1,
                supported: SAVE_VERSION,
            })
        );
    }

    /// The version has to be readable *without* the world parsing, or a save
    /// from a future build reads as corrupt and the player is told the wrong
    /// thing about their own file.
    ///
    /// Verified by sabotage: the bytes after the version are deliberate
    /// rubbish, so this can only pass if the version was read first.
    #[test]
    fn a_future_version_is_recognised_before_the_world_is_parsed() {
        let mut bytes = postcard::to_stdvec(&(SAVE_VERSION + 1)).expect("a u32 serializes");
        bytes.extend_from_slice(b"not a world at all");
        assert_eq!(
            World::from_bytes(&bytes),
            Err(SaveError::FromTheFuture {
                found: SAVE_VERSION + 1,
                supported: SAVE_VERSION,
            })
        );
    }

    #[test]
    fn rubbish_bytes_are_refused_rather_than_half_loaded() {
        assert!(matches!(
            World::from_bytes(&[0u8; 3]),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn substreams_are_independent_of_the_main_stream() {
        let world = World::new(spec(1961));
        let before = world.rng_state();
        let mut drawn = world.substream(1, 0);
        for _ in 0..100 {
            drawn.next_u64();
        }
        assert_eq!(
            world.rng_state(),
            before,
            "drawing from a substream moved the main stream"
        );
    }

    #[test]
    fn substreams_are_recomputable_and_distinct() {
        let world = World::new(spec(1961));
        // Same coordinates, same stream — no matter when you ask.
        assert_eq!(
            world.substream(3, 9).next_u64(),
            world.substream(3, 9).next_u64()
        );
        // Different purpose or index, different stream.
        assert_ne!(
            world.substream(3, 9).next_u64(),
            world.substream(4, 9).next_u64()
        );
        assert_ne!(
            world.substream(3, 9).next_u64(),
            world.substream(3, 10).next_u64()
        );
    }

    #[test]
    fn ninety_days_of_ticks_land_on_the_right_date() {
        let mut world = World::new(spec(1));
        simulate_days(&mut world, 90);
        assert_eq!(world.clock.days_elapsed(), 90);
        // Founding is 1 March. In 30-day months, ninety days is exactly March,
        // April and May — so the ninetieth day is 1 June, not the 31st of a
        // month that does not exist here.
        let date = world.clock.date();
        assert_eq!((date.year, date.month, date.day), (1960, 6, 1));
    }
}
