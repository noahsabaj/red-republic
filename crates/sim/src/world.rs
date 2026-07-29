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
use crate::command::{Command, Done, Journal, Outcome, Refused};
use crate::contract::Contracts;
use crate::crews::Crews;
use crate::fleet::Destination;
use crate::fleet::Fleet;
use crate::geology::Geology;
use crate::ground::{Crossing, Ground, Lattice};
use crate::loan::Loans;
use crate::mapgen;
use crate::network::Network;
use crate::policy::BuildPolicy;
use crate::resource::Resource;
use crate::rng::{Rng, RngState};
use crate::roadworks::{Grade, RoadError, RoadSiteId, RoadWorks};
use crate::terrain::{self, Terrain};
use crate::time::SimClock;
use crate::trade::{Frontier, Market, TradePolicy, Treasury};
use crate::units::Metres;
use crate::units::{Point, Tonnes};
use serde::{Deserialize, Serialize};

/// Bumped whenever a save can no longer be read by the current code. A load
/// that finds an older version runs migrations; one that finds a newer version
/// refuses, because guessing at a format from the future corrupts silently.
///
/// 2: the physical fleet. Vehicles are persisted state, so a save written
/// before they existed no longer describes a whole world.
///
/// 3: roads under construction, and journey legs carrying a speed limit rather
/// than a flag.
///
/// 4: the state of the ground. Soil moisture and lying snow accumulate, so
/// they are state rather than a function of the date.
///
/// 5: the traversal lattice, and vehicles that can be stuck in it.
///
/// 7: the journal. A save now carries what the player did, not only what the
/// republic became, which is what makes a save replayable and a reported bug
/// reproducible from the save alone.
///
/// 8: the frontier replaces the single border edge, and the republic can carry
/// advances from the blocs.
///
/// 9: building crews. Where a gang is standing and what it is working is
/// persisted state — a save written before them describes a republic whose
/// sites are being built by nobody.
///
/// 10: the build policy, and what has already been bought on each site's
/// account. The second half is what makes a reloaded republic stop buying a
/// wall it has already paid for.
///
/// 11: foreign labour. Who a republic has hired, from which bloc, and which
/// gangs are still travelling in from a frontier post.
pub const SAVE_VERSION: u32 = 12;

/// The first version the format ever carried.
///
/// Load-bearing rather than trivia: it is what separates *an older save* from
/// *not a save*. `from_bytes` decodes a leading varint before it knows whether
/// the bytes are a save at all, and arbitrary rubbish frequently decodes to a
/// small number — three zero bytes give version 0. Without a floor, every such
/// blob would be reported as coming from an older build, which is a more
/// confident lie than "corrupt" and sends whoever reads it looking for a
/// migration that was never missing.
pub const FIRST_SAVE_VERSION: u32 = 1;

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

/// Substream identifier for whether a crossing goes wrong.
///
/// Its own stream, keyed by vehicle, leg and day, so that a bogging is a pure
/// function of who tried what and when. Two consequences worth having: the same
/// run always sticks the same lorry in the same field, and the **odds are
/// showable** — a panel can say how a crossing sits before the player commits
/// to it, which is most of the explicability a probability model normally
/// costs.
pub const BOG_STREAM: u64 = 6;

/// Substream identifier for births, deaths and people deciding to leave.
///
/// One stream for all three, keyed by who and when, for the same reason bogging
/// has its own: a republic's demography must not shift because a lorry took a
/// different route. Keyed rather than drawn in sequence, so asking whether a
/// particular person died this year is a pure function and not a position in a
/// queue.
pub const LIFE_STREAM: u64 = 7;

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
    /// The save was written by an older build and no migration exists for it.
    ///
    /// Reported rather than attempted, because **postcard is not
    /// self-describing**: an older save is a bare byte sequence laid out to an
    /// older `World`, and decoding it against today's shape does not reliably
    /// fail — it can succeed and hand back a world whose fields have quietly
    /// slid past each other. Silent corruption is the one outcome a save format
    /// may never have, and it is the same reason `serde_json` was rejected.
    FromThePast { found: u32, supported: u32 },
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
            SaveError::FromThePast { found, supported } => write!(
                f,
                "save format {found} is older than this build reads ({supported}), \
                 and no migration exists for it"
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

/// Somewhere a load can be put down, whatever kind of thing it is.
///
/// An engine-owned view in the sense the shell decision made load-bearing: the
/// UI and the dispatcher both read it rather than re-deriving it, and it stays
/// coarse on purpose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Consignee {
    pub at: Point,
    /// How much of the resource is already there.
    pub held: Tonnes,
    /// The most of it this place will take.
    pub capacity: Tonnes,
    /// Whether it is a finished building rather than a site still being built.
    /// A site's need is finite and one-off, which is why freight serves it
    /// whatever the quantity — see `systems::MIN_LOAD`.
    pub finished: bool,
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
///
/// # Every field is `pub(crate)`, and that is the point
///
/// Systems live in this crate and are unaffected — they still read
/// `world.buildings` directly. Anything *outside* the crate reads through the
/// view methods below and writes through [`World::issue`], which is the only
/// public path that changes anything except [`World::tick`].
///
/// Before this, every field here was `pub` and every structure under it handed
/// out `get_mut`, so a shell could write what no system was allowed to write —
/// the `{field, value}` escape hatch the single-writer rule exists to refuse,
/// left open to the UI by accident. See [`crate::command`] for the whole
/// argument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub(crate) clock: SimClock,
    /// The sequential simulation stream. Systems draw from this in a fixed
    /// order, which is why the order systems run in is load-bearing.
    pub(crate) rng: Rng,
    /// The ground.
    pub(crate) terrain: Terrain,
    /// What is under the ground, and how much of it is left.
    pub(crate) geology: Geology,
    /// What stands on it.
    pub(crate) buildings: Buildings,
    /// The roads between them.
    pub(crate) roads: Network,
    /// The railways. A peer of `roads` rather than a variant of it, because
    /// what keeps a train off a road is that the road is not in here.
    pub(crate) rails: Network,
    /// Street track, and the metro's own tunnels. Separate from `rails` on
    /// purpose: one network would let a republic lay tramway at a third the
    /// price and run freight trains down it.
    pub(crate) tramway: Network,
    pub(crate) metro: Network,
    /// Navigable water. **The one network nobody builds** — it is derived from
    /// the ground the moment the ground exists, and rebuilt with it.
    pub(crate) waterways: Network,
    /// The aerodromes, joined to one another. Rebuilt when one opens.
    pub(crate) airways: Network,
    /// The ways that have been ordered and are not yet usable. One queue for
    /// roads and rails alike: a railway takes its turn behind whatever was
    /// ordered before it, the same as everything else the crew builds.
    pub(crate) roadworks: RoadWorks,
    /// How wet and how frozen the open ground is today.
    pub(crate) ground: Ground,
    /// The coarse lattice vehicles cross country over, and where the ground has
    /// been worn into tracks.
    pub(crate) lattice: Lattice,
    /// The lorries that move everything, and where each of them is.
    pub(crate) fleet: Fleet,
    /// The people.
    pub(crate) population: Population,
    /// The whole perimeter, which bloc holds each stretch of it, and where the
    /// crossings stand. Placed at worldgen — you do not build a frontier post,
    /// you build road out to one.
    pub(crate) frontier: Frontier,
    /// Hard currency. Earned at the border, spent at the border, and never on
    /// anything domestic.
    pub(crate) treasury: Treasury,
    /// Standing instructions to the customs house.
    pub(crate) trade_policy: TradePolicy,
    /// Tenders from the two blocs: offers, live deals and recent history.
    pub(crate) contracts: Contracts,
    /// Advances the blocs have made, at most one per bloc.
    pub(crate) loans: Loans,
    /// The building crews that are out: who they belong to, where they are
    /// standing, and what they are working.
    pub(crate) crews: Crews,
    /// Where sites buy materials the republic has not made, and through which
    /// post. Empty by default — a republic imports nothing until told to.
    pub(crate) build_policy: BuildPolicy,
    /// People standing at the frontier waiting to be let in, and the running
    /// tally of everyone who has come and gone.
    pub(crate) migration: crate::migration::Migration,
    /// The republic's energised power lines and heat mains, and who is plugged
    /// into them.
    pub(crate) utilities: crate::utility::Networks,
    /// Lines that have been ordered and are not yet carrying anything.
    pub(crate) lineworks: crate::utility::LineWorks,
    /// The posting's climate. Fixed at founding — you do not get a milder
    /// winter by asking for one.
    pub(crate) climate: ClimateId,
    /// Everything the player has actually done, in order.
    ///
    /// Persisted with the world, so a save is a record of how its republic came
    /// to be and not only what it currently is. It is what gives the
    /// determinism rule's *same inputs* half something to hold constant.
    pub(crate) journal: Journal,
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
        let lattice = Lattice::from_terrain(&terrain);
        let waterways = crate::network::navigable(&terrain);
        Self {
            clock: SimClock::new(),
            rng: Rng::from_seed(spec.seed),
            terrain,
            geology,
            buildings: Buildings::new(),
            roads: Network::new(),
            rails: Network::new(),
            tramway: Network::new(),
            metro: Network::new(),
            waterways,
            airways: Network::new(),
            roadworks: RoadWorks::new(),
            ground: Ground::default(),
            lattice,
            fleet: Fleet::new(),
            population: Population::new(),
            // Drawn from its own substream so the frontier does not shift when
            // terrain or geology generation changes.
            frontier: Frontier::generate(
                spec.extent,
                &mut Rng::from_seed(derive(spec.seed, BORDER_STREAM)),
            ),
            treasury: Treasury::default(),
            trade_policy: TradePolicy::new(),
            contracts: Contracts::default(),
            loans: Loans::new(),
            crews: Crews::new(),
            build_policy: BuildPolicy::new(),
            migration: crate::migration::Migration::new(),
            utilities: crate::utility::Networks::new(),
            lineworks: crate::utility::LineWorks::new(),
            climate: spec.climate,
            journal: Journal::new(),
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
        self.weather_on_day(day_index).0
    }

    /// Millimetres of rain today. Below freezing it falls as snow, which is
    /// [`crate::ground`]'s business rather than the sky's.
    pub fn precipitation(&self) -> f64 {
        self.weather_on_day(self.clock.day_index()).1
    }

    /// The whole day's weather, drawn from one substream in a fixed order.
    ///
    /// Temperature and rain share a stream and are drawn in this order always,
    /// so adding a third reading later must append rather than interleave — a
    /// draw inserted in the middle would change every temperature in every
    /// existing seed.
    pub fn weather_on_day(&self, day_index: u64) -> (f64, f64) {
        let mut stream = self.substream(WEATHER_STREAM, day_index);
        let deviation = stream.next_f64();
        let wetness = stream.next_f64();
        let day_of_year = (day_index % u64::from(crate::time::DAYS_PER_YEAR)) as u32;
        let def = self.climate.def();
        (
            climate::temperature_on(def, day_of_year, deviation),
            climate::precipitation_on(def, day_of_year, wetness),
        )
    }

    /// Lay different ground.
    ///
    /// The **only** way the terrain should be replaced, because the traversal
    /// lattice is a summary of it: setting one without the other leaves
    /// vehicles routing over a map that is no longer there, and the symptom is
    /// a republic where nothing can get anywhere for no visible reason.
    /// `pub(crate)`, and it was still `pub` after the seal landed — the M2
    /// sweep counted it as a verb and then forgot to close it. The exposure
    /// guard found it, which is the second thing that guard has been worth.
    ///
    /// It is not a player action and never will be. `world::fixtures` reaches
    /// it for benchmark setup and is inside the crate; a shell has no business
    /// replacing the ground under a running republic.
    // Only `world::fixtures` reaches it today, so it is dead in a build with
    // that feature off. Kept rather than folded into the caller because it is
    // the one place terrain and lattice are replaced together, and the rule
    // that they must be is what stops a republic where nothing can get
    // anywhere for no visible reason.
    #[cfg_attr(not(feature = "fixtures"), allow(dead_code))]
    pub(crate) fn set_terrain(&mut self, terrain: Terrain) {
        self.lattice = Lattice::from_terrain(&terrain);
        self.waterways = crate::network::navigable(&terrain);
        self.terrain = terrain;
    }

    /// Every way through the republic, for the planner.
    ///
    /// One accessor rather than four fields threaded through every call site,
    /// because a planner handed the wrong network answers confidently and is
    /// wrong — see [`crate::journey::Ways`].
    pub(crate) fn ways(&self) -> crate::journey::Ways<'_> {
        crate::journey::Ways {
            roads: &self.roads,
            rails: &self.rails,
            tramway: &self.tramway,
            metro: &self.metro,
            water: &self.waterways,
            air: &self.airways,
        }
    }

    /// The network a finished way of this grade joins.
    pub(crate) fn network_for(&mut self, grade: Grade) -> &mut Network {
        match grade.def().carries {
            crate::journey::Medium::Rail => &mut self.rails,
            crate::journey::Medium::Tram => &mut self.tramway,
            crate::journey::Medium::Metro => &mut self.metro,
            _ => &mut self.roads,
        }
    }

    /// Rebuild the aerodrome network.
    ///
    /// Every aerodrome joined to every other, because an aeroplane between two
    /// of them flies straight there. Derived at the event that invalidates it —
    /// a landing ground opening or being pulled down — rather than per tick,
    /// the same discipline the utility grids and the traversal lattice follow.
    pub(crate) fn re_survey_airways(&mut self) {
        self.airways = Network::new();
        let fields: Vec<Point> = self
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().medium == Some(crate::journey::Medium::Air))
            .map(|b| b.centre)
            .collect();
        let nodes: Vec<_> = fields.iter().map(|&at| self.airways.add_node(at)).collect();
        for (i, &a) in nodes.iter().enumerate() {
            for &b in &nodes[i + 1..] {
                self.airways.connect(a, b, crate::network::AIRWAY_SPEED);
            }
        }
    }

    /// The lattice and today's conditions together: what it costs to cross
    /// country right now.
    pub fn crossing(&self) -> Crossing<'_> {
        Crossing {
            lattice: &self.lattice,
            softness: self.ground.softness(),
        }
    }

    /// How badly the open ground would bog a vehicle today, at a given place.
    pub fn going_at(&self, at: Point) -> f64 {
        self.crossing().going_at(at)
    }

    /// The odds against a vehicle on the crossing it is about to make, as a
    /// share `0.0..=1.0`.
    ///
    /// The showable half of the bogging model. A panel can put this in front of
    /// the player *before* the lorry sets out — "this crossing is thirty per
    /// cent against you, loaded twelve tonnes on going of point eight" — which
    /// is what a probability model owes back for the explicability it takes.
    pub fn bog_chance(&self, vehicle: crate::fleet::VehicleId, leg: u32) -> f64 {
        let Some(v) = self.fleet.get(vehicle) else {
            return 0.0;
        };
        let Some(journey) = v.journey.as_ref() else {
            return 0.0;
        };
        if leg >= journey.legs() || journey.limit[leg as usize].is_some() {
            return 0.0;
        }
        let (from, to) = journey.leg_ends(leg);
        crate::systems::bog_chance(self.crossing().going_along(from, to), v.capability())
    }

    /// How far a point is from foreign soil.
    pub fn distance_to_border(&self, at: crate::units::Point) -> Metres {
        self.frontier.distance_from(at)
    }

    /// The frontier: its stretches, their blocs, and the crossings on it.
    pub fn frontier(&self) -> &Frontier {
        &self.frontier
    }

    /// Which bloc's frontier is nearest a point — what a crossing here would
    /// trade with.
    pub fn bloc_near(&self, at: crate::units::Point) -> Market {
        self.frontier.bloc_near(at)
    }

    /// The rules that need to know where the border is.
    ///
    /// Written once and shared by [`Self::place`] and [`Self::can_place`],
    /// because the moment the preview and the commit each carry their own copy
    /// they are free to disagree — and they did.
    fn border_rule(
        &self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<(), crate::building::PlacementError> {
        // A customs house goes AT a frontier post, not merely near the
        // frontier. The posts are placed at worldgen and you build road out to
        // the one you want; that is what makes which bloc you trade with a
        // siting decision rather than a dropdown, and it is why the whole
        // perimeter being border does not mean you can trade from anywhere.
        if kind == crate::building::BuildingKind::Customs {
            let near_a_post = self
                .frontier
                .nearest_crossing(at, None)
                .is_some_and(|c| c.at.distance_to(at).0 <= crate::trade::CUSTOMS_RANGE.0);
            if !near_a_post {
                return Err(crate::building::PlacementError::NotOnTheBorder);
            }
        }

        // A terminal has to stand beside the way it serves. Exactly the same
        // shape of rule as the customs house's, and for the same reason: a
        // station four hundred metres from the nearest rail is not a station
        // that works badly, it is a building no train can reach, and letting it
        // be built would leave the player with a very expensive shed and
        // nothing saying why nothing calls at it.
        //
        // An aerodrome is deliberately exempt: it *is* the network, so
        // requiring one to stand near an existing one would mean a republic
        // could never build its first.
        // Two exemptions, both for reasons rather than convenience. A road
        // vehicle is **free-ranging** — it can drive across a field to reach a
        // depot, so a bus garage away from the tarmac is inconvenient rather
        // than impossible. And an aerodrome *is* the air network, so requiring
        // one near an existing one would mean a republic could never build its
        // first.
        if let Some(medium) = kind.def().medium
            && medium != crate::journey::Medium::Air
            && !medium.free_ranging()
            && self
                .network(medium)
                .nearest_node(at, crate::journey::TERMINAL_REACH)
                .is_none()
        {
            return Err(crate::building::PlacementError::NoWayThere(medium));
        }
        Ok(())
    }

    /// Would this go here? Every rule [`Self::place`] applies, without
    /// committing — what a placement preview asks.
    ///
    /// [`crate::building::Buildings::can_place`] is the wrong call for a shell
    /// to make directly and this exists because it was the only one available.
    /// That layer has no idea where the border is, so it answered *yes* for a
    /// customs house anywhere on the map while [`Self::place`] refused it: a
    /// ghost rendering green over ground that would reject it. A preview which
    /// asks a different question from the commit is not a preview.
    pub fn can_place(
        &self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<Option<crate::geology::DepositId>, crate::building::PlacementError> {
        self.border_rule(kind, at)?;
        self.buildings
            .can_place(kind, at, &self.terrain, &self.geology)
    }

    /// Put a building up, applying every rule including the ones that need to
    /// know where the border is.
    ///
    /// [`crate::building::Buildings::place`] cannot check the border itself —
    /// it has no idea where the border is, and giving it one would mean handing
    /// the whole world to every placement. This is the layer that knows.
    /// `pub(crate)` because [`crate::command::Command::Place`] is the public
    /// way in: a placement that skips the journal is a placement a replay
    /// cannot reproduce.
    pub(crate) fn place(
        &mut self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<crate::building::BuildingId, crate::building::PlacementError> {
        self.border_rule(kind, at)?;
        let id = self
            .buildings
            .place(kind, at, &self.terrain, &self.geology)?;
        // Plugged in at the moment it goes up, not searched for per tick.
        self.wire_in(id);
        Ok(id)
    }

    /// Order a road between two points.
    ///
    /// It is a **site**, not a road: nothing routes over it, nothing commutes
    /// along it, and no lorry is quicker for its existing until the crew have
    /// finished it and the gravel has been driven out.
    ///
    /// Both ends have to be on ground that will take a road. What happens in
    /// between is deliberately not checked yet — a road across water is a
    /// bridge, and a bridge is a decision this build has not made.
    /// `pub(crate)`; [`crate::command::Command::OrderRoad`] is the public way
    /// in, for the same reason as [`World::place`].
    pub(crate) fn order_road(
        &mut self,
        from: Point,
        to: Point,
        grade: Grade,
    ) -> Result<RoadSiteId, RoadError> {
        for end in [from, to] {
            if !self
                .terrain
                .surface_at(end)
                .is_some_and(|s| s.is_buildable())
            {
                return Err(RoadError::Unbuildable);
            }
        }
        // And what it runs OVER, which nothing used to ask. A road across water
        // is a bridge, and a bridge costs steel and concrete by the kilometre
        // and months of a crew — so until this check existed a river was an
        // obstacle the cheapest grade in the table crossed for the price of
        // gravel, while the design said water was impassable.
        if !grade.def().spans_water() && self.terrain.crosses_water(from, to) {
            return Err(RoadError::NeedsABridge);
        }
        // Where this sits in the republic's commissioning order. Buildings are
        // ranked by their own id, which counts the same sequence — so a road
        // takes the count as it stands, meaning "after everything standing
        // now", and the construction system breaks the tie in the buildings'
        // favour because the building with that number was ordered first.
        let ordered = self.buildings.commissioned();
        self.roadworks.order(from, to, grade, ordered)
    }

    /// Order a power line or a heat main between two points.
    ///
    /// A **site**, exactly as a road is: it carries nothing until the crew have
    /// strung it and the steel has been driven out. `pub(crate)`;
    /// [`crate::command::Command::OrderLine`] is the public way in.
    pub(crate) fn order_line(
        &mut self,
        kind: crate::utility::Utility,
        from: Point,
        to: Point,
    ) -> Result<crate::utility::LineSiteId, crate::utility::LineError> {
        let ordered = self.buildings.commissioned();
        self.lineworks.order(kind, from, to, ordered)
    }

    /// Plug a building into whatever runs close enough to it.
    ///
    /// Called at placement rather than searched for per tick, which is the
    /// whole reason the attachment is stored: testing every building against
    /// every span 1,440 times a simulated day is the shape of cost this
    /// codebase has already been caught by once.
    pub(crate) fn wire_in(&mut self, building: crate::building::BuildingId) {
        if let Some(at) = self.buildings.get(building).map(|b| b.centre) {
            self.utilities.attach_all(building, at);
        }
    }

    /// The founding grant's grid: energise a span without building it.
    ///
    /// `pub(crate)` and used only by [`crate::scenario::found`], for exactly
    /// the reason [`World::place_built`] is: there is no player action that
    /// makes a finished span appear, so exposing one would hand a shell a cheat
    /// with no meaning inside the fiction.
    pub(crate) fn energise_now(&mut self, site: &crate::utility::LineSite) {
        self.utilities.energise(site);
        self.wire_up(site.kind);
    }

    pub(crate) fn lineworks_mut(&mut self) -> &mut crate::utility::LineWorks {
        &mut self.lineworks
    }

    /// Plug in everything that now stands within reach of a newly energised
    /// span. The counterpart of [`World::wire_in`], from the line's side.
    pub(crate) fn wire_up(&mut self, kind: crate::utility::Utility) {
        let standing: Vec<_> = self
            .buildings
            .all()
            .iter()
            .map(|b| (b.id, b.centre))
            .collect();
        for (id, at) in standing {
            self.utilities.attach(id, at, kind);
        }
    }

    /// What dispatch needs to know about somewhere goods can go.
    ///
    /// One view over two different structures, which is the point: the ranking,
    /// the load minimum and the delivery all ask the same four questions of a
    /// building and of a road site, and neither should have to know which it is
    /// looking at.
    pub fn consignee(&self, to: Destination, resource: Resource) -> Option<Consignee> {
        match to {
            Destination::Building(id) => {
                let b = self.buildings.get(id)?;
                Some(Consignee {
                    at: b.centre,
                    held: b.stock.get(resource),
                    capacity: b.intake_capacity(resource),
                    finished: b.is_built(),
                })
            }
            Destination::RoadSite(id) => {
                let s = self.roadworks.get(id)?;
                Some(Consignee {
                    at: s.depot(),
                    held: s.stock.get(resource),
                    capacity: s.intake_capacity(resource),
                    // Always a site: it stops existing the moment it is not.
                    finished: false,
                })
            }
            Destination::LineSite(id) => {
                let s = self.lineworks.get(id)?;
                Some(Consignee {
                    at: s.depot(),
                    held: s.stock.get(resource),
                    capacity: s.intake_capacity(resource),
                    finished: false,
                })
            }
        }
    }

    /// Where something goods or people can be sent to actually stands.
    ///
    /// The place half of [`World::consignee`], without having to name a resource
    /// to ask. A crew bus wants exactly this and nothing else about the site: a
    /// gang is set down at a place, and whether that place has room for bricks
    /// is not a question about people.
    pub fn place_of(&self, to: Destination) -> Option<crate::units::Point> {
        match to {
            Destination::Building(id) => self.buildings.get(id).map(|b| b.centre),
            Destination::RoadSite(id) => self.roadworks.get(id).map(|s| s.depot()),
            Destination::LineSite(id) => self.lineworks.get(id).map(|s| s.depot()),
        }
    }

    /// The same, already finished — the founding grant.
    ///
    /// Scenario setup rather than play, and `pub(crate)` for a stronger reason
    /// than the others: there is no player action that makes a finished
    /// building appear, so exposing one would be handing a shell a cheat with
    /// no in-fiction meaning. [`crate::scenario::found`] is the public caller.
    pub(crate) fn place_built(
        &mut self,
        kind: crate::building::BuildingKind,
        at: crate::units::Point,
    ) -> Result<crate::building::BuildingId, crate::building::PlacementError> {
        let id = self.place(kind, at)?;
        if let Some(b) = self.buildings.get_mut(id) {
            b.work_done = b.def().labour;
        }
        // Opening an aerodrome changes where every aeroplane can fly. The
        // construction system does this when the last builder-day lands; a
        // building conjured up finished skips that, so it does it here. Two
        // callers rather than one is the price of a founding hand that arrives
        // already built.
        if kind.def().medium == Some(crate::journey::Medium::Air) {
            self.re_survey_airways();
        }
        Ok(id)
    }

    /// Advance one fixed step, returning everything that changed.
    ///
    /// Systems are sequenced in [`crate::systems::run_tick`], in an order that
    /// is part of the simulation's definition rather than an implementation
    /// detail: they run in source order, draw from `rng` in that order, and
    /// changing the order changes the world.
    ///
    /// The mutations come back because they are the only honest account of what
    /// a tick did — the trajectory runner totals freight from them, and a shell
    /// will read them for events rather than diffing the world against itself.
    /// Ignoring the result is fine and costs nothing.
    pub fn tick(&mut self) -> Vec<crate::systems::Mutation> {
        crate::systems::run_tick(self)
    }

    /// Carry out a player command, or say why not.
    ///
    /// **The only public way to change a republic except [`World::tick`].**
    /// Everything under [`World`] is `pub(crate)`, so a shell has exactly two
    /// verbs: advance time, and ask for something.
    ///
    /// An accepted command is recorded in the journal as it is applied; a
    /// refused one changes nothing and is not recorded, because replaying a
    /// no-op is not replay. That recording is what finally gives the
    /// determinism rule's *same seed and same inputs* half an **inputs** to
    /// hold constant — see
    /// [`crate::world::tests::a_republic_replays_from_its_own_journal`].
    pub fn issue(&mut self, command: Command) -> Outcome {
        let done = self.carry_out(&command)?;
        self.journal.record(self.clock.ticks(), command);
        Ok(done)
    }

    /// The half of [`World::issue`] that does the work, split out so the
    /// journal records exactly what succeeded and nothing else.
    fn carry_out(&mut self, command: &Command) -> Outcome {
        match *command {
            Command::Place { kind, at } => self
                .place(kind, at)
                .map(Done::Commissioned)
                .map_err(Refused::Placement),

            // Two refusals here, and both exist to make an orphaned crew
            // unrepresentable rather than to detect one afterwards. Pulling down
            // a site with a gang standing on it, or an office whose gangs are
            // out, would leave people belonging to a building that no longer
            // exists — and no amount of tidying up after the fact answers "so
            // where are they now?".
            Command::Demolish { building } => {
                if self.buildings.get(building).is_none() {
                    return Err(Refused::NoSuchBuilding(building));
                }
                if self
                    .crews
                    .working_at(Destination::Building(building))
                    .is_some()
                {
                    return Err(Refused::CrewOnSite);
                }
                if self.crews.posted(building) > 0 {
                    return Err(Refused::CrewsOut);
                }
                if self.buildings.demolish(building) {
                    // A building id is never reused, so a stale policy entry
                    // would be harmless to read and would sit in every save
                    // for ever. This is one of the few structures keyed by
                    // something that stops existing.
                    self.build_policy.forget(Destination::Building(building));
                    self.utilities.detach(building);
                    Ok(Done::Nothing)
                } else {
                    Err(Refused::NoSuchBuilding(building))
                }
            }

            // Down tools. They stop work where they stand and wait for the
            // office to send a bus — there is no way to make people appear back
            // at the office, which is the same rule that makes construction
            // physical in the first place.
            Command::RecallCrew { site } => match self.crews.working_at(site) {
                Some(_) => {
                    let at = self.place_of(site).unwrap_or_default();
                    self.crews.release(site, at);
                    Ok(Done::Nothing)
                }
                None => Err(Refused::NoCrewThere),
            },

            Command::OrderRoad { from, to, grade } => self
                .order_road(from, to, grade)
                .map(Done::Ordered)
                .map_err(Refused::Road),

            Command::OrderLine { kind, from, to } => self
                .order_line(kind, from, to)
                .map(Done::Strung)
                .map_err(Refused::Line),

            Command::AcceptContract { contract } => {
                if self.contracts.accept(contract) {
                    Ok(Done::Nothing)
                } else {
                    Err(Refused::NoSuchOffer(contract))
                }
            }

            Command::DeclineContract { contract } => {
                if self.contracts.decline(contract) {
                    Ok(Done::Nothing)
                } else {
                    Err(Refused::NoSuchOffer(contract))
                }
            }

            // An advance credits the treasury the moment it is taken, and
            // repaying debits it. Both are transactions at a genuine boundary —
            // money moving and a debt moving never happen apart — which is the
            // case the single-writer rule allows a coarse operation for.
            Command::TakeLoan { market, tier } => {
                let today = self.clock.day_index();
                let principal = self.loans.take(market, tier as usize, today)?;
                self.treasury.credit(market, principal);
                Ok(Done::Nothing)
            }

            Command::RepayLoan { market, amount } => {
                let owed = self.loans.outstanding(market);
                if owed <= 0.0 {
                    return Err(Refused::Loan(crate::loan::LoanError::NothingOwed));
                }
                // Capped at what is owed AND at what is held, so a repayment
                // can never overdraw a purse that has no overdraft.
                let payable = amount.min(owed).min(self.treasury.of(market));
                if payable <= 0.0 {
                    return Err(Refused::Loan(crate::loan::LoanError::CannotAfford));
                }
                let paid = self.loans.repay(market, payable)?;
                self.treasury.debit(market, paid);
                Ok(Done::Nothing)
            }

            // The crossing is checked here rather than when it is read, so a
            // policy in the world always names a post that exists — otherwise a
            // typo would sit in a save quietly importing nothing, which looks
            // exactly like a republic that cannot afford anything.
            Command::SetImportPolicy { site, crossing } => {
                if let Some(id) = crossing
                    && self.frontier.get(id).is_none()
                {
                    return Err(Refused::NoSuchCrossing(id));
                }
                match site {
                    Some(site) => self.build_policy.set_site(site, crossing),
                    None => self.build_policy.set_global(crossing),
                }
                Ok(Done::Nothing)
            }

            Command::ClearImportPolicy { site } => {
                if self.build_policy.clear_site(site) {
                    Ok(Done::Nothing)
                } else {
                    Err(Refused::NoSuchPolicy)
                }
            }

            Command::SetStandingOrder {
                building,
                resource,
                tonnes,
            } => {
                let Some(b) = self.buildings.get(building) else {
                    return Err(Refused::NoSuchBuilding(building));
                };
                if !b.def().stores_to_order {
                    return Err(Refused::NotAStore);
                }
                let holds = b.storage_cap();
                if tonnes.0 > holds.0 {
                    return Err(Refused::OverCapacity {
                        asked: tonnes.0,
                        holds: holds.0,
                    });
                }
                if let Some(b) = self.buildings.get_mut(building) {
                    b.orders.set(resource, tonnes);
                }
                Ok(Done::Nothing)
            }

            // Hiring is a transaction at a genuine boundary: the fee leaves and
            // the workers set out, and neither happens without the other.
            Command::HireForeign {
                market,
                office,
                heads,
            } => {
                if heads == 0 {
                    return Err(Refused::NobodyToHire);
                }
                let Some(b) = self.buildings.get(office) else {
                    return Err(Refused::NoSuchBuilding(office));
                };
                if b.kind != crate::building::BuildingKind::ConstructionOffice || !b.is_built() {
                    return Err(Refused::NotAConstructionOffice);
                }
                // They arrive at their own bloc's post, which is what makes
                // hiring from the far bloc a haulage decision rather than a
                // dropdown — the same geography a dollar already has.
                let Some(post) = self.frontier.nearest_crossing(b.centre, Some(market)) else {
                    return Err(Refused::NoPostOfThatBloc(market));
                };
                let at = post.at;
                let fee = f64::from(heads) * crate::crews::HIRING_FEE;
                let held = self.treasury.of(market);
                if held + 1e-9 < fee {
                    return Err(Refused::CannotAffordHiring { needed: fee, held });
                }
                self.treasury.debit(market, fee);
                self.crews.arrive(office, market, heads, at);
                Ok(Done::Nothing)
            }

            Command::AddTradeRule { .. }
            | Command::RemoveTradeRule { .. }
            | Command::MoveTradeRule { .. } => {
                crate::command::edit_rules(&mut self.trade_policy.rules, command)
            }
        }
    }

    /// Everything the player has done, in order.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    // ---- Views -------------------------------------------------------------
    //
    // The read half of the boundary. Borrowed, so nothing outside the crate can
    // write through them, and coarse on purpose: the measured marshalling rule
    // is that a chatty *small* interface is free (a raw FFI call is 0.21 µs)
    // while a bulky *structured* one is not (a dictionary per entity at 1,205
    // buildings cost 8.6 ms against 27 µs for a packed array — 316× apart).

    pub fn clock(&self) -> SimClock {
        self.clock
    }

    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }

    pub fn geology(&self) -> &Geology {
        &self.geology
    }

    pub fn buildings(&self) -> &Buildings {
        &self.buildings
    }

    /// One of the republic's four ways through.
    ///
    /// One accessor rather than four, and it is the only public way to read a
    /// network — a shell that has to name `roads` and `rails` separately is a
    /// shell that draws three of them and forgets the fourth when a fifth
    /// arrives. `Medium` is the roster, and iterating it is what makes a new
    /// network visible without anybody remembering to add it.
    pub fn network(&self, medium: crate::journey::Medium) -> &Network {
        match medium {
            crate::journey::Medium::Road => &self.roads,
            crate::journey::Medium::Rail => &self.rails,
            crate::journey::Medium::Tram => &self.tramway,
            crate::journey::Medium::Metro => &self.metro,
            crate::journey::Medium::Water => &self.waterways,
            crate::journey::Medium::Air => &self.airways,
        }
    }

    pub fn roadworks(&self) -> &RoadWorks {
        &self.roadworks
    }

    /// What a place has been told to keep on hand, and how much of it is there.
    ///
    /// The panel side of [`crate::command::Command::SetStandingOrder`]: a
    /// player who cannot see the order they set cannot tell a station that is
    /// filling from one nothing is being sent to.
    pub fn standing_orders(
        &self,
        building: crate::building::BuildingId,
    ) -> Vec<(Resource, Tonnes, Tonnes)> {
        let Some(b) = self.buildings.get(building) else {
            return Vec::new();
        };
        if !b.def().stores_to_order {
            return Vec::new();
        }
        Resource::ALL
            .into_iter()
            .map(|r| (r, b.stock.get(r), b.orders.get(r)))
            .filter(|(_, _, ordered)| ordered.is_positive())
            .collect()
    }

    pub fn ground(&self) -> Ground {
        self.ground
    }

    pub fn lattice(&self) -> &Lattice {
        &self.lattice
    }

    pub fn fleet(&self) -> &Fleet {
        &self.fleet
    }

    pub fn population(&self) -> &Population {
        &self.population
    }

    pub fn treasury(&self) -> Treasury {
        self.treasury
    }

    pub fn trade_policy(&self) -> &TradePolicy {
        &self.trade_policy
    }

    pub fn contracts(&self) -> &Contracts {
        &self.contracts
    }

    /// The advances the republic is carrying, and its record of paying them.
    /// The building crews that are out.
    pub fn crews(&self) -> &Crews {
        &self.crews
    }

    /// Where sites buy what the republic has not made.
    pub fn build_policy(&self) -> &BuildPolicy {
        &self.build_policy
    }

    /// Who is standing at the frontier wanting in, and the running tally of
    /// everyone who has arrived, left, or given up waiting.
    pub fn migration(&self) -> &crate::migration::Migration {
        &self.migration
    }

    /// The energised power lines and heat mains, and who is plugged into them.
    pub fn utilities(&self) -> &crate::utility::Networks {
        &self.utilities
    }

    /// Lines ordered and not yet carrying anything.
    pub fn lineworks(&self) -> &crate::utility::LineWorks {
        &self.lineworks
    }

    pub fn loans(&self) -> &Loans {
        &self.loans
    }

    pub fn climate(&self) -> ClimateId {
        self.climate
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
        // There are none, so an older save is refused rather than accepted.
        //
        // The comment that used to sit here said there was no ladder "because
        // there is only one version", while the constant read 6 — and the code
        // under it returned `Ok` for every version below. That combination is
        // the worst of both: it claims a migration path, provides none, and
        // succeeds anyway. Verified before changing it — nothing in the repo
        // holds a save written by an older build, and no test loads one, so
        // this branch has never had a real subject.
        if save.version < FIRST_SAVE_VERSION {
            return Err(SaveError::Corrupt(format!(
                "save format {} has never existed",
                save.version
            )));
        }
        if save.version < SAVE_VERSION {
            return Err(SaveError::FromThePast {
                found: save.version,
                supported: SAVE_VERSION,
            });
        }
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
    ///
    /// **Both directions are decided before the world is touched**, and the
    /// backward one matters more than it looks. `postcard` is not
    /// self-describing, so decoding an older layout against today's `World` is
    /// not reliably an error — it can succeed on shifted bytes. Refusing on the
    /// version is the only check that happens before that can occur.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        let (version, _) = postcard::take_from_bytes::<u32>(bytes)
            .map_err(|e| SaveError::Corrupt(e.to_string()))?;
        if version > SAVE_VERSION {
            return Err(SaveError::FromTheFuture {
                found: version,
                supported: SAVE_VERSION,
            });
        }
        if version < FIRST_SAVE_VERSION {
            // Not an old save — not a save. Versions have started at
            // FIRST_SAVE_VERSION since the format existed, so anything below it
            // is arbitrary bytes whose first varint happened to decode, and
            // calling that "from an older build" would be a worse lie than the
            // one this check replaced. Three zero bytes are the worked example.
            return Err(SaveError::Corrupt(format!(
                "save format {version} has never existed"
            )));
        }
        if version < SAVE_VERSION {
            return Err(SaveError::FromThePast {
                found: version,
                supported: SAVE_VERSION,
            });
        }
        let save: Save =
            postcard::from_bytes(bytes).map_err(|e| SaveError::Corrupt(e.to_string()))?;
        Self::from_save(save)
    }
}

/// Construction and measurement access for the benchmark harness.
///
/// Behind the `fixtures` feature, which nothing but `tests/baselines.rs`
/// enables — so a shell cannot reach any of it, and the seal on [`World`] holds
/// where it matters.
///
/// It exists because the baselines rule is **one baseline per axis**, and an
/// axis cannot be isolated through [`World::issue`]. Timing the labour pass at
/// four thousand citizens means calling the labour pass with four thousand
/// citizens; founding a republic and ticking it would time every system at once
/// and report the wrong number confidently. Standing up ten thousand buildings
/// to measure placement scaling is the same problem — no player does that, and
/// building them through construction would measure construction.
///
/// The rule this keeps: **nothing here is reachable from a build that renders
/// anything.** A hatch nobody can open from the shell is not a hatch in the
/// shell.
#[cfg(feature = "fixtures")]
pub mod fixtures {
    use super::World;
    use crate::building::{BuildingId, BuildingKind, Buildings, PlacementError};
    use crate::citizen::Population;
    use crate::ground::{Ground, Lattice};
    use crate::terrain::Terrain;
    use crate::units::Point;

    impl World {
        /// Stand a finished building up, as the founding grant does.
        pub fn establish(
            &mut self,
            kind: BuildingKind,
            at: Point,
        ) -> Result<BuildingId, PlacementError> {
            self.place_built(kind, at)
        }

        /// Swap the ground, rebuilding the traversal lattice with it.
        pub fn replace_terrain(&mut self, terrain: Terrain) {
            self.set_terrain(terrain);
        }

        /// The population, mutably, so a pass over it can be timed on its own.
        pub fn population_mut(&mut self) -> &mut Population {
            &mut self.population
        }

        /// Put the ground in a chosen state.
        ///
        /// Cross-country routing cost depends entirely on how soft the going
        /// is, so measuring it on whatever the weather happened to do would be
        /// measuring the weather. The routing baseline soaks the map on purpose.
        pub fn set_ground(&mut self, ground: Ground) {
            self.ground = ground;
        }

        /// The traversal lattice, mutably, so wear can be laid down before it
        /// is routed over rather than driven in over a simulated year.
        pub fn lattice_mut(&mut self) -> &mut Lattice {
            &mut self.lattice
        }

        /// The buildings, mutably, for standing a great many of them up at once.
        pub fn buildings_mut(&mut self) -> &mut Buildings {
            &mut self.buildings
        }
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

    /// The mirror of the test above, and it exists because sabotage caught the
    /// first version of this guard failing to reach its subject.
    ///
    /// `a_save_from_an_older_build_is_refused_rather_than_misread` builds its
    /// bytes from a *valid* world, so deleting the version check in
    /// [`World::from_bytes`] changed nothing: parsing succeeded and
    /// [`World::from_save`] refused it one call later. The test went on passing
    /// against a build with the thing it was written for removed.
    ///
    /// A body that cannot be parsed is what tells the two apart. With the check
    /// the version decides; without it, parsing fails first and the player is
    /// told their save is corrupt when it is merely old.
    #[test]
    fn an_older_version_is_recognised_before_the_world_is_parsed() {
        let mut bytes = postcard::to_stdvec(&(SAVE_VERSION - 1)).expect("a u32 serializes");
        bytes.extend_from_slice(b"not a world at all");
        assert_eq!(
            World::from_bytes(&bytes),
            Err(SaveError::FromThePast {
                found: SAVE_VERSION - 1,
                supported: SAVE_VERSION,
            })
        );
    }

    /// Rubbish is corrupt, not old.
    ///
    /// Three zero bytes decode to version 0, which sits below
    /// [`FIRST_SAVE_VERSION`] — no save has ever carried it. Without that floor
    /// this reports "from an older build", which is a more confident lie than
    /// "corrupt" and sends whoever reads it hunting a migration that was never
    /// missing.
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

    /// A preview must answer exactly what the commit will answer.
    ///
    /// [`crate::building::Buildings::can_place`] is border-blind, so before
    /// [`World::can_place`] existed a shell previewing a customs house was told
    /// *yes* on ground [`World::place`] then refused — a ghost rendering green
    /// over a placement that could not happen.
    ///
    /// This asserts the two **agree** rather than asserting one particular
    /// answer, so it goes on protecting the invariant when the border model is
    /// replaced. The premise assertion at the end is not decoration: sweep a
    /// map that never produces an off-border point and this test would pass
    /// against a build with the rule deleted.
    #[test]
    fn a_placement_preview_answers_exactly_what_the_placement_will() {
        use crate::building::{BuildingKind, PlacementError};

        let mut world = World::new(spec(1961));
        let extent = world.terrain.extent().0;
        let mut refused_for_the_border = 0;

        for i in 0..=10u32 {
            for j in 0..=10u32 {
                let at = Point::new(
                    Metres(extent * f64::from(i) / 10.0),
                    Metres(extent * f64::from(j) / 10.0),
                );
                let previewed = world.can_place(BuildingKind::Customs, at);
                if previewed == Err(PlacementError::NotOnTheBorder) {
                    refused_for_the_border += 1;
                }
                let committed = world.place(BuildingKind::Customs, at);
                match (previewed, committed) {
                    (Err(p), Err(c)) => {
                        assert_eq!(p, c, "preview and placement disagree at {at:?}")
                    }
                    (Ok(_), Ok(_)) => {}
                    (p, c) => panic!("preview said {p:?} but placement said {c:?} at {at:?}"),
                }
            }
        }

        assert!(
            refused_for_the_border > 0,
            "the sweep never reached a point away from the border, \
             so agreement was proved about nothing"
        );
    }

    /// A republic replays from its own journal.
    ///
    /// **This is what the determinism rule's *same inputs* half was missing.**
    /// Before commands existed there was no such thing as an input, so
    /// `a_reloaded_world_resumes_the_same_future` proved replay for a world
    /// nobody was playing — a real guard whose subject was half absent.
    ///
    /// Two republics from the same seed. One is played: roads ordered,
    /// buildings commissioned and pulled down, tenders taken, trade rules added
    /// and reordered, all at scattered ticks. The other is handed nothing but
    /// the first one's journal and told to re-run it. They must end the same
    /// world, byte for byte.
    ///
    /// The premise assertions are load-bearing. A journal that came out empty,
    /// or a script whose commands were all refused, would make two untouched
    /// republics agree trivially and prove nothing at all.
    #[test]
    fn a_republic_replays_from_its_own_journal() {
        use crate::building::BuildingKind;
        use crate::command::Command;
        use crate::roadworks::Grade;
        use crate::trade::{Market, TradeAction};

        let at = |x: f64, y: f64| Point::new(Metres(x), Metres(y));

        // What a session looks like: commands landing on scattered ticks with
        // the world running in between.
        let script: Vec<(u64, Command)> = vec![
            (
                0,
                Command::AddTradeRule {
                    resource: Resource::Coal,
                    market: Market::East,
                    action: TradeAction::Sell,
                },
            ),
            (
                0,
                Command::AddTradeRule {
                    resource: Resource::Food,
                    market: Market::West,
                    action: TradeAction::Buy {
                        up_to: Tonnes(50.0),
                    },
                },
            ),
            (3, Command::MoveTradeRule { from: 1, to: 0 }),
            (
                5,
                Command::Place {
                    kind: BuildingKind::House,
                    at: at(300.0, 300.0),
                },
            ),
            (
                11,
                Command::Place {
                    kind: BuildingKind::Warehouse,
                    at: at(420.0, 300.0),
                },
            ),
            (
                17,
                Command::OrderRoad {
                    from: at(300.0, 300.0),
                    to: at(700.0, 300.0),
                    grade: Grade::Dirt,
                },
            ),
            (40, Command::RemoveTradeRule { index: 0 }),
        ];

        let run = |script: &[(u64, Command)]| -> World {
            let mut world = World::new(spec(1961));
            for tick in 0..90u64 {
                for (at_tick, command) in script {
                    if *at_tick == tick {
                        let _ = world.issue(command.clone());
                    }
                }
                world.tick();
            }
            world
        };

        let played = run(&script);

        // The premise: this proves nothing unless the script actually did
        // things. A journal of refusals is a journal of no-ops.
        assert!(
            played.journal().len() >= 5,
            "only {} commands were carried out; the script was mostly refused \
             and this test would pass on two untouched republics",
            played.journal().len()
        );
        assert!(
            played.journal().entries().iter().any(|e| e.tick > 0),
            "every command landed on tick zero, so nothing tested that the \
             journal replays commands at the right moment"
        );

        // Replay: a fresh republic from the same seed, given only the journal.
        let mut replayed = World::new(spec(1961));
        for tick in 0..90u64 {
            for command in played.journal().on_tick(tick).cloned().collect::<Vec<_>>() {
                let _ = replayed.issue(command);
            }
            replayed.tick();
        }

        assert_eq!(
            fingerprint(&played),
            fingerprint(&replayed),
            "same seed and same inputs did not produce the same world"
        );
        assert_eq!(played, replayed);
    }

    /// A refusal changes nothing and is not written down.
    ///
    /// Both halves matter. If a refused command left a mark, the journal would
    /// no longer be the set of things that moved the world; if it were recorded
    /// anyway, a replay would spend its time re-refusing.
    #[test]
    fn a_refused_command_changes_nothing_and_is_not_recorded() {
        use crate::building::{BuildingKind, PlacementError};
        use crate::command::{Command, Refused};

        let mut world = World::new(spec(1961));
        let before = fingerprint(&world);

        // A customs house away from the border: refused by a rule, not by the
        // ground, so this does not depend on what the terrain happened to be.
        let refused = world.issue(Command::Place {
            kind: BuildingKind::Customs,
            at: Point::new(Metres(500.0), Metres(500.0)),
        });
        assert_eq!(
            refused,
            Err(Refused::Placement(PlacementError::NotOnTheBorder))
        );

        // And demolishing something that is not there.
        assert_eq!(
            world.issue(Command::Demolish {
                building: crate::building::BuildingId(9_999),
            }),
            Err(Refused::NoSuchBuilding(crate::building::BuildingId(9_999)))
        );

        assert_eq!(fingerprint(&world), before, "a refusal moved the world");
        assert!(world.journal().is_empty(), "a refusal was written down");

        // The reason is a sentence, not a debug dump — this is what a panel
        // prints and what greys out a button with a tooltip.
        assert_eq!(
            refused.unwrap_err().to_string(),
            "a customs house must stand at the national border"
        );
    }

    /// You cannot pull down a site with builders standing on it, nor an office
    /// whose gangs are out.
    ///
    /// **Prevention rather than detection**, and that is the whole reason these
    /// two refusals exist. Allowing either would leave people belonging to a
    /// building that no longer exists, and no amount of tidying up afterwards
    /// answers "so where are they now?" — the office's establishment would be
    /// short by ten and nothing would say why. Refusing makes the state
    /// unrepresentable instead, and hands the player a sentence naming what to
    /// do about it.
    #[test]
    fn a_building_nobody_can_account_for_afterwards_cannot_be_pulled_down() {
        use crate::building::BuildingKind;
        use crate::command::{Command, Refused};
        use crate::fleet::Destination;

        let mut world = World::new(spec(1961));
        let base = crate::scenario::found(&mut world, 120);
        let office = base.construction_office.expect("the founding places one");

        // A site far enough out that the crew is genuinely away from the yard.
        let centre = base.centre;
        let site = world
            .issue(Command::Place {
                kind: BuildingKind::Warehouse,
                at: Point::new(centre.x + Metres(700.0), centre.y + Metres(700.0)),
            })
            .expect("open ground");
        let crate::command::Done::Commissioned(site) = site else {
            panic!("a placement returns the building it commissioned");
        };

        // Before anybody is on it, it comes down like anything else would.
        assert!(world.crews.at_site(Destination::Building(site)).eq(&0));

        // Now put a gang on it and try again.
        let mut posted = false;
        for _ in 0..(crate::time::TICKS_PER_DAY * 20) {
            world.tick();
            if world.crews.at_site(Destination::Building(site)) > 0 {
                posted = true;
                break;
            }
        }
        assert!(posted, "no crew was ever sent to the site");

        assert_eq!(
            world.issue(Command::Demolish { building: site }),
            Err(Refused::CrewOnSite)
        );
        assert_eq!(
            world.issue(Command::Demolish { building: office }),
            Err(Refused::CrewsOut)
        );
        assert_eq!(
            Refused::CrewsOut.to_string(),
            "this office still has crews out at sites; bring them in first"
        );

        // Recalling them is the way through, and it is a command rather than
        // something the simulation decides on the player's behalf.
        assert_eq!(
            world.issue(Command::RecallCrew {
                site: Destination::Building(site)
            }),
            Ok(crate::command::Done::Nothing)
        );
        assert_eq!(world.crews.at_site(Destination::Building(site)), 0);
        assert_eq!(
            world.issue(Command::Demolish { building: site }),
            Ok(crate::command::Done::Nothing)
        );
        // But the office still owes them a bus, so it is still not going
        // anywhere — which is the point: the people did not evaporate with the
        // foundation.
        assert_eq!(
            world.issue(Command::Demolish { building: office }),
            Err(Refused::CrewsOut)
        );
        assert_eq!(
            world.issue(Command::RecallCrew {
                site: Destination::Building(site)
            }),
            Err(Refused::NoCrewThere)
        );
    }

    /// Pulling a building down while a lorry is driving to it does not strand
    /// anything.
    ///
    /// `Demolish` is the first command that can invalidate what another system
    /// is already holding: a vehicle's job names a `Destination`, a citizen's
    /// `Workplace` and `Home` name buildings, and the dispatcher has already
    /// ranked a demand against a yard that is about to stop existing.
    ///
    /// **What is asserted is that it resolves, not that it never happens.** A
    /// lorry already on the road to a building that has just come down is
    /// correct and transient — it finishes its leg, finds nothing to collect or
    /// nowhere to put its load, and turns for home. The first version of this
    /// test demanded that no job ever reference a missing building for even one
    /// tick, which is not the invariant and is not physical.
    ///
    /// It is also the second version. The first sampled once, thirty days
    /// later, and the premise assertion caught it out: a founded republic's
    /// fleet holds a job on only 12.9% of ticks — 101 dispatches and 198 t
    /// moved over the same month, so freight is working, it is simply idle most
    /// of the time — and the single instant it looked at had every lorry
    /// parked.
    #[test]
    fn a_republic_survives_having_a_building_pulled_out_from_under_its_lorries() {
        use crate::command::Command;
        use crate::fleet::{Destination, VehicleState};

        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        crate::scenario::found(&mut world, 120);
        simulate_days(&mut world, 20);

        // Premise: somebody is actually driving to the thing being demolished.
        let target = world
            .fleet()
            .all()
            .iter()
            .filter_map(|v| v.job)
            .filter_map(|job| job.haul())
            .find_map(|(_, to, _, _)| match to {
                Destination::Building(id) => Some(id),
                Destination::RoadSite(_) | Destination::LineSite(_) => None,
            })
            .expect("no lorry was en route to a building, so this proves nothing");
        let population_before = world.population().count();
        assert!(population_before > 0, "an empty republic proves nothing");

        assert_eq!(
            world.issue(Command::Demolish { building: target }),
            Ok(crate::command::Done::Nothing)
        );
        assert!(world.buildings().get(target).is_none(), "it is still there");

        // How long the republic carries a job pointing at nothing, and whether
        // freight keeps flowing while it does.
        let mut still_pointing_at_it = 0u32;
        let mut dispatches = 0u32;
        // Demography is a legitimate reason for the head count to move over a
        // month, so the population claim below is accounted rather than fixed:
        // people are born and die whatever the player demolishes, and an
        // assertion that ignores that is one that fails for the wrong reason.
        let mut born = 0usize;
        let mut gone = 0usize;
        for _ in 0..30 * TICKS_PER_DAY {
            for m in world.tick() {
                match &m {
                    crate::systems::Mutation::Dispatch { .. } => dispatches += 1,
                    crate::systems::Mutation::Birth { homes } => born += homes.len(),
                    crate::systems::Mutation::Death { citizens } => gone += citizens.len(),
                    crate::systems::Mutation::Emigrate { citizens } => gone += citizens.len(),
                    _ => {}
                }
            }
            if world.fleet().all().iter().any(|v| {
                v.job
                    .and_then(|j| j.haul())
                    .is_some_and(|(from, to, _, _)| {
                        from == target || to == Destination::Building(target)
                    })
            }) {
                still_pointing_at_it += 1;
            }
        }

        // Measured at 9 ticks — nine simulated minutes, which is the lorry
        // finishing the leg it was on and turning for home. The bound is four
        // hours: loose enough that a longer haul cannot make it flaky, and two
        // orders of magnitude tighter than the failure it exists to catch,
        // which is a job that points at nothing for ever.
        assert!(
            still_pointing_at_it < 240,
            "a lorry carried a job to a demolished building for              {still_pointing_at_it} ticks; it took 9 when this was measured"
        );
        assert!(
            dispatches > 0,
            "the republic dispatched nothing for a month after one demolition,              so freight did not survive it"
        );
        assert!(
            world
                .fleet()
                .all()
                .iter()
                .all(|v| v.state == VehicleState::Idle
                    || matches!(v.state, VehicleState::Bogged { .. })
                    || v.journey.is_some()),
            "a vehicle is neither parked, stuck, nor going anywhere"
        );
        assert_eq!(
            world.population().count(),
            population_before + born - gone,
            "the head count moved by more than the births and deaths account \
             for — demolishing a building deleted people"
        );
    }

    /// The journal survives a save, because it is part of the world.
    #[test]
    fn a_save_carries_the_journal_that_built_it() {
        use crate::building::BuildingKind;
        use crate::command::Command;

        let mut world = World::new(spec(1961));
        world
            .issue(Command::Place {
                kind: BuildingKind::House,
                at: Point::new(Metres(300.0), Metres(300.0)),
            })
            .expect("open ground");
        simulate_days(&mut world, 2);

        let reloaded = World::from_bytes(&world.to_bytes()).expect("a save this build wrote");
        assert_eq!(reloaded.journal(), world.journal());
        assert_eq!(reloaded.journal().len(), 1);
    }

    /// A save from an older build is refused, not quietly reinterpreted.
    ///
    /// `postcard` is not self-describing: an older save is a bare byte sequence
    /// laid out to an older `World`, so decoding it against today's shape is
    /// not reliably an error. It can succeed on shifted bytes and hand back a
    /// world that looks valid and is not — the same silent-corruption failure
    /// that ruled out `serde_json`. The old code returned `Ok` for every
    /// version below the current one while its comment claimed there was only
    /// ever one version.
    #[test]
    fn a_save_from_an_older_build_is_refused_rather_than_misread() {
        let world = World::new(spec(1961));
        let older = SAVE_VERSION - 1;
        let expected = SaveError::FromThePast {
            found: older,
            supported: SAVE_VERSION,
        };

        assert_eq!(
            World::from_save(Save {
                version: older,
                world: world.clone(),
            }),
            Err(expected.clone()),
        );

        // And through the bytes, which is the path a shell actually uses.
        let mut bytes = postcard::to_stdvec(&older).expect("a u32 serializes");
        bytes.extend_from_slice(&postcard::to_stdvec(&world).expect("a world serializes"));
        assert_eq!(World::from_bytes(&bytes), Err(expected));
    }
}
