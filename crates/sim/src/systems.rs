//! The simulation's systems, and the single writer that applies them.
//!
//! # Systems propose, one writer applies
//!
//! Carried forward from the archived build, because it is why blast radii
//! stayed small there. A system reads the world and returns [`Mutation`]s; it
//! never writes. [`apply`] is the only code that mutates, so "what can this
//! change?" is answerable by reading one enum instead of auditing a module.
//!
//! There is deliberately **no generic `{field, value}` mutation**. That is
//! "write anything" with extra steps, and it would leave nothing for a guard to
//! check. Coarse kinds are right at genuine transaction boundaries — extraction
//! is a deposit drawn down *and* a building's bin filled, because those never
//! happen apart and splitting them would let one land without the other.
//!
//! # Order is part of the definition
//!
//! Systems run in the order [`run_tick`] lists them, and their mutations apply
//! in emission order. Changing either changes the world. That is not a defect
//! to be engineered away — it is what makes a fixed seed mean something.
//!
//! # Rates are per day, ticks are minutes
//!
//! Every rate in the building table is tonnes per day. Systems scale by the
//! fraction of a day a tick represents, so the economy is continuous and a
//! fractional tonne is a real quantity rather than a rounding artefact.

use crate::building::{BuildingId, BuildingKind};
use crate::citizen::{CitizenId, assign_labour};
use crate::climate;
use crate::contract::{self, Contract, ContractId, ContractState};
use crate::crews::PartyId;
use crate::fleet::{Destination, Doing, Job, Role, VehicleId, VehicleKind, VehicleState, crewed};
use crate::geology::DepositId;
use crate::ground::Crossing;
use crate::journey::{self, Journey};
use crate::migration::GroupId;
use crate::resource::Resource;
use crate::resource::Stock;
use crate::roadworks::{self, RoadSiteId};
use crate::time::TICK;
use crate::trade::{CUSTOMS_RANGE, CUSTOMS_THROUGHPUT_PER_DAY, Market, TradeAction};
use crate::transport;
use crate::units::{Metres, Point, Seconds, Tonnes};
use crate::world::{BOG_STREAM, World};
use std::collections::BTreeMap;

/// Everything a system is allowed to change.
///
/// Not `Copy`: a [`Mutation::Dispatch`] carries a whole [`Journey`], and a
/// journey owns its waypoints. That is the price of freight being a plan rather
/// than a number, and it is paid a few hundred times a simulated day.
#[derive(Debug, Clone, PartialEq)]
pub enum Mutation {
    /// How many people turned up.
    Staff { building: BuildingId, count: u32 },
    /// Whether the grid could feed it.
    Powered { building: BuildingId, on: bool },
    /// Whether the boilers could reach it.
    Heated { building: BuildingId, on: bool },
    /// Ore out of the ground and into a bin. One kind, not two: the deposit
    /// draw and the bin fill are one transaction and must never land apart.
    Extract {
        deposit: DepositId,
        building: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
    },
    /// Inputs burned by a process.
    Consume {
        building: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
    },
    /// Output of a process.
    Produce {
        building: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
    },
    /// The day's weather worked through the ground: what the topsoil holds,
    /// what snow is lying, and how frozen it is.
    ///
    /// One kind because the three move together and only together: rain that
    /// falls below freezing becomes snow instead of moisture, and snow that
    /// melts becomes moisture instead of snow. Splitting them would make
    /// "it snowed and the ground got wetter" representable.
    /// The day's weather, and what it did to the snow lying on the ground.
    ///
    /// **One kind carrying both**, because they are one transaction: the day's
    /// snowfall IS what buries the roads, and a republic whose ground had
    /// advanced without its cover changing would be a republic where the
    /// clearance field and the weather disagreed about what month it was.
    ///
    /// `snowfall` is the share of the republic's clearance that the day's fall
    /// undoes, zero on a day nothing fell. When the pack has gone entirely the
    /// whole field is reset rather than decayed, so a road ploughed last
    /// February is not still credited for it next December.
    Weather {
        ground: crate::ground::Ground,
        snowfall: f64,
    },
    /// A garage takes delivery of a vehicle its establishment allows.
    Commission {
        garage: BuildingId,
        kind: VehicleKind,
    },
    /// A vehicle takes a job, tops up from its garage's tank, and sets out for
    /// the supplier. One kind: accepting the work, fuelling for it and setting
    /// off are the same decision, and a lorry that took a job without the fuel
    /// to finish it is the exact failure the dispatch-time check exists to
    /// prevent.
    Dispatch {
        vehicle: VehicleId,
        job: Job,
        journey: Journey,
        /// Tonnes drawn from the garage into the tank.
        refuel: Tonnes,
    },
    /// A leg finished and the next one begins. One kind because a vehicle never
    /// covers ground without burning fuel, nor burns fuel without covering
    /// ground.
    Advance {
        vehicle: VehicleId,
        leg: u32,
        leg_start: f64,
        leg_end: f64,
        burn: Tonnes,
    },
    /// A vehicle reached its supplier, took on what was actually there, and set
    /// out again.
    ///
    /// `state` is on the mutation because arriving at the supplier does not
    /// always mean delivering: a yard that has been emptied since the job was
    /// booked sends the lorry home rather than on to a destination it has
    /// nothing for.
    Load {
        vehicle: VehicleId,
        from: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
        journey: Journey,
        state: VehicleState,
        burn: Tonnes,
    },
    /// A vehicle reached its destination, put down what would fit, and turned
    /// for home. Whatever would not fit stays on the bed — freight is
    /// conserved, and it comes off in the garage yard.
    Unload {
        vehicle: VehicleId,
        to: Destination,
        resource: Resource,
        tonnes: Tonnes,
        journey: Journey,
        burn: Tonnes,
    },
    /// A vehicle got home: the job is done and anything still aboard is put
    /// into the garage.
    Park { vehicle: VehicleId, burn: Tonnes },
    /// A vehicle topping up at a filling point it has reached, away from its
    /// own garage.
    Refuel {
        vehicle: VehicleId,
        from: BuildingId,
        tonnes: Tonnes,
    },
    /// Ground packed down by something driving over it.
    ///
    /// Traffic is the only thing that makes a track, and the loop it closes is
    /// the point: the first lorry picks a line, that line gets marginally
    /// cheaper to drive, the next one picks the same line, and a route nobody
    /// planned hardens into one the republic can see.
    Wear { cells: Vec<usize>, by: f64 },
    /// A day of the ground coming back. Without it every line ever driven is
    /// permanent and a map fills with the ghosts of routes nobody uses.
    Fade { by: f64 },
    /// Goods moving along a belt or a pipe.
    ///
    /// **One kind, coarse on purpose**: taking a tonne out of one bin and
    /// putting it in another is a single transaction, and a half of it that
    /// landed alone would either destroy goods or mint them. The same reasoning
    /// as `Extract`, one level up.
    Convey {
        from: BuildingId,
        to: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
    },
    /// Smoke settling on a cell of the traversal lattice.
    Foul { cell: usize, by: f64 },
    /// A day of weather carrying it away. The counterpart of [`Mutation::Fade`]
    /// and needed for the same reason: without it every chimney the republic
    /// ever lit is permanent, and pulling a works down would leave its smoke
    /// behind for ever.
    Disperse { by: f64 },
    /// A worn corridor becomes a dirt track on the map: proper segments in
    /// the road network, which anything can then route over as a road.
    ///
    /// The end of the loop, and the reason roads in this republic are earned
    /// rather than drawn.
    Promote { cells: Vec<usize> },
    /// A vehicle stuck fast. It keeps its job, its load and its plan; what it
    /// has lost is the ability to go anywhere.
    ///
    /// The day travels with it because how long it has been there is what
    /// decides whether anybody is sent — see [`crate::fleet::HELP_AFTER`].
    Bog { vehicle: VehicleId, day: u64 },
    /// It got going again under its own power, because the ground came back to
    /// it. Resumes the leg it stalled before, timed from now.
    Free {
        vehicle: VehicleId,
        was: Doing,
        leg: u32,
        leg_start: f64,
        leg_end: f64,
    },
    /// A recovery vehicle hooked on, pulled one out, and turned for home.
    ///
    /// One kind for both halves, because a tow that half happened would leave a
    /// lorry in a field with a recovery vehicle standing beside it doing
    /// nothing. The casualty is set down at the **far** end of the crossing
    /// that beat it, not where it stuck: a rescue that puts it back in the same
    /// mud is a rescue that has to happen again on the next tick.
    Recover {
        recovery: VehicleId,
        casualty: VehicleId,
        was: Doing,
        casualty_leg: u32,
        casualty_start: f64,
        casualty_end: f64,
        /// The recovery vehicle's way home.
        journey: Journey,
        burn: Tonnes,
    },
    /// How much of a household's needs the shops actually met, 0..=1.
    /// What came off the shelves for the people who live here.
    ///
    /// **One mutation for the necessities and the comforts**, because they came
    /// off the same shelves in the same pass and a home provisioned without its
    /// comforts recorded would be a building whose two halves disagreed about
    /// what day it was. `drink` is carried apart from `comforts` because alcohol
    /// alone costs health — see [`crate::wellbeing::ALCOHOL_HEALTH_COST`] — and
    /// a combined figure could not say how much of it was vodka.
    Provision {
        building: BuildingId,
        fraction: f64,
        comforts: f64,
        drink: f64,
    },
    /// Goods leaving the republic and the currency that came back. One kind:
    /// the stock and the payment are the same transaction, and a sale where
    /// only one landed would either give goods away or mint money.
    ///
    /// `contract` carries the tender this load counts against, when it counts
    /// against one. Crediting it separately would let a delivery be paid for
    /// without being booked — which is the exact failure the archived build's
    /// coarse `exportSale` mutation existed to make impossible.
    Export {
        customs: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
        market: Market,
        payment: f64,
        contract: Option<ContractId>,
    },
    /// Goods arriving and the currency that paid for them.
    ///
    /// `for_site` names the site whose account this was bought on, when it was
    /// bought for one. Carried on the mutation rather than booked separately for
    /// the reason `Export` carries its contract: the goods landing, the money
    /// leaving and the site's allowance falling are one transaction, and an
    /// allowance that could fall without goods arriving — or goods that could
    /// arrive without the allowance falling — is how a republic buys the same
    /// wall eight times.
    Import {
        customs: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
        market: Market,
        cost: f64,
        for_site: Option<Destination>,
    },
    /// A crew set down at the site it is to work, and the bus turning for home.
    ///
    /// One kind, because a gang standing in a field with the bus still holding
    /// them is not a state anybody should be able to write. The counterpart of
    /// [`Mutation::Unload`], and it is separate from it for the reason a
    /// [`Job::Ferry`] is separate from a haul: what comes off is people, and
    /// what they then do is work rather than sit in a bin.
    Land {
        vehicle: VehicleId,
        party: PartyId,
        site: Destination,
        at: Point,
        journey: Journey,
        burn: Tonnes,
    },
    /// A crew picked up, and the bus turning for home.
    Embark {
        vehicle: VehicleId,
        party: PartyId,
        journey: Journey,
        burn: Tonnes,
    },
    /// Builder-days worked on a site, and the materials that went into them.
    /// One kind: work and materials are the same transaction, and a site that
    /// advanced without consuming would be building itself out of nothing.
    Build { site: BuildingId, builder_days: f64 },
    /// The same, on a power line or a heat main.
    ///
    /// Separate from [`Mutation::Lay`] for the reason that one is separate from
    /// [`Mutation::Build`]: it writes to a different structure, and its last
    /// builder-day **energises** the span — the line enters the network, every
    /// building already standing within reach of it is plugged in, and the site
    /// stops existing, all in one transaction. A finished span that carried
    /// nothing until somebody happened to place a building would be a grid that
    /// came alive for no reason the player could see.
    String {
        site: crate::utility::LineSiteId,
        builder_days: f64,
    },
    /// The same, on a road.
    ///
    /// Separate from [`Mutation::Build`] because it writes to a different
    /// structure, and because its last builder-day does something a building's
    /// never does: **the road opens**. The segments enter the network and the
    /// site stops existing, in the same transaction, because a finished road
    /// site that is not yet a road is not a state the simulation should be able
    /// to hold.
    Lay { site: RoadSiteId, builder_days: f64 },
    /// A bloc puts a tender on the table.
    Offer(Contract),
    /// A tender reaches its end, one way or the other.
    CloseContract {
        contract: ContractId,
        state: ContractState,
    },
    /// A tender leaves the table without being settled — an offer nobody took,
    /// or old history being pruned.
    DropContract { contract: ContractId },
    /// How a bloc feels about the republic, after a failure or a day of
    /// forgetting.
    Relations { market: Market, penalty: f64 },
    /// An advance came due with money still owed. The bloc writes it off and
    /// remembers. Coarse on purpose: the debt going, the fine landing and
    /// relations souring are one event, and a default that half happened would
    /// be a republic that owes nothing to a bloc that is not angry.
    DefaultOnLoan { market: Market },
    /// The day's wages for foreign builders, and whoever went home because the
    /// republic could not pay them.
    ///
    /// **One kind, and coarse on purpose.** Paying and losing people are the
    /// same daily transaction: what the purse could not cover *is* who leaves,
    /// and a republic that had paid nothing and lost nobody would be a republic
    /// employing foreign labour for free. `dismissed` is empty on almost every
    /// day, which is what a wage bill that is being met looks like.
    Wages {
        market: Market,
        paid: f64,
        dismissed: Vec<(BuildingId, u32)>,
    },
    /// A penalty for undelivered goods. Separate from [`Mutation::Export`]
    /// because no goods move: this is money leaving and nothing coming back.
    Fine { market: Market, amount: f64 },
    /// A day's work by a foreign firm on a site the republic contracted out.
    ///
    /// **One kind, and coarse on purpose**, for the same reason [`Mutation::
    /// Wages`] is: the work and the bill are one transaction. A day a
    /// contractor worked is a day the republic owes for, and the two can never
    /// happen apart — a version that advanced the site and paid separately
    /// could leave a republic with a building it never bought.
    ///
    /// `paid` is what the treasury could actually cover, which is not always
    /// `owed`: `Treasury::debit` refuses to go negative, so a republic that has
    /// run out simply stops making progress. That is the right failure — a
    /// stalled site is on the screen, and it is the same shape as an unpaid
    /// wage bill sending foreign workers home.
    Contracted {
        site: BuildingId,
        market: Market,
        builder_days: f64,
        paid: f64,
    },
    /// How well a home's people are being served, component by component.
    ///
    /// The counterpart of [`Mutation::Provision`], one level up: provisioning
    /// is what came off the shelves, and this is what the whole republic adds
    /// up to for the people living here.
    Content {
        building: BuildingId,
        content: crate::wellbeing::Contentment,
    },
    /// The day's health and loyalty, for everybody.
    ///
    /// **One kind carrying the whole population**, deliberately. This is a
    /// daily census rather than an event: everyone's health and loyalty move
    /// every day, and emitting four thousand separate mutations to say so
    /// would be four thousand allocations to express one pass.
    Morale {
        updates: Vec<(CitizenId, crate::citizen::Wellbeing)>,
    },
    /// A day of education: who sat in a classroom, and who is enrolled at a
    /// university today.
    ///
    /// A census for the same reason [`Mutation::Morale`] is, and coarse for a
    /// second one: enrolment has to be settable *and clearable* in one pass,
    /// because a university that loses its staff stops having students the same
    /// day. A mutation that could only enrol would leave students permanently
    /// out of the workforce of a republic that no longer teaches them.
    Schooling {
        attended: Vec<CitizenId>,
        enrolled: Vec<CitizenId>,
    },
    /// Birthdays. A year older, for everyone whose day it is.
    Ageing { citizens: Vec<CitizenId> },
    /// Who died.
    Death { citizens: Vec<CitizenId> },
    /// Who was born, and into which home.
    Birth { homes: Vec<BuildingId> },
    /// Who packed up and left the republic.
    ///
    /// Deliberately not a journey. Somebody who has decided to go does not need
    /// the republic's transport, and making them queue for a coach out would
    /// mean a failing republic *retained* people by failing harder.
    Emigrate { citizens: Vec<CitizenId> },
    /// People walking up to a frontier post, wanting in.
    ///
    /// They are at the border and nowhere else. Turning them into residents is
    /// a coach's job — see [`Mutation::Settle`].
    Immigrate { at: Point, heads: u32 },
    /// A group that stood at a post until its patience ran out.
    GiveUp { group: GroupId },
    /// A coach reached a group; they boarded, and it turns for the housing.
    Board {
        vehicle: VehicleId,
        group: GroupId,
        journey: Journey,
        burn: Tonnes,
    },
    /// Visitors from abroad walking up to a frontier post.
    ///
    /// They are at the border and nowhere else — the same shape as
    /// [`Mutation::Immigrate`], and for the same reason: somebody who appeared
    /// in a hotel would be the click-a-button shape this build refuses.
    Arrive {
        at: Point,
        heads: u32,
        market: Market,
    },
    /// A coach reached a party of visitors; they boarded, and it turns for the
    /// hotel it was sent to.
    Fetch {
        vehicle: VehicleId,
        visit: crate::tourism::VisitId,
        journey: Journey,
        burn: Tonnes,
    },
    /// A coach set its party down at a hotel, and their stay began.
    ///
    /// One kind for both halves, for the reason [`Mutation::Settle`] is one: a
    /// party that had left the coach without checking in would be people
    /// standing in a lobby that nothing in the simulation can see.
    CheckIn {
        vehicle: VehicleId,
        visit: crate::tourism::VisitId,
        hotel: BuildingId,
        at: Point,
        journey: Journey,
        burn: Tonnes,
    },
    /// A day of hard currency from visitors, and whose stay ended.
    ///
    /// **Coarse on purpose**: the sweep that counts the money is the sweep that
    /// ends a stay, and a party that went home without its last day's takings
    /// would be money the republic earned and did not get. `leaving` also
    /// carries parties that gave up at a post, because from the republic's side
    /// both are visitors it no longer has.
    Takings {
        market: Market,
        amount: f64,
        leaving: Vec<crate::tourism::VisitId>,
    },
    /// A plough went through: these cells are swept.
    ///
    /// The same shape as [`Mutation::Wear`] and emitted at the same moment for
    /// the same reason — work happens at leg boundaries, and what a leg did to
    /// the ground it crossed is known only once it has crossed it.
    Clear { cells: Vec<usize> },
    /// A coach set its group down at housing, and they became citizens.
    ///
    /// One kind for both halves, because a group that had left the coach
    /// without becoming residents would be people standing in a stairwell that
    /// nothing in the simulation can see.
    Settle {
        vehicle: VehicleId,
        group: GroupId,
        home: BuildingId,
        journey: Journey,
        burn: Tonnes,
    },
}

/// A [`Mutation`]'s kind, without its payload.
///
/// Exists so a system can *declare* what it is allowed to change and a test can
/// check the declaration. The archived build had exactly this and it earned its
/// keep: without it a new mechanic quietly widens an old system's blast radius,
/// and nobody notices until something far away moves for no reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MutationKind {
    Staff,
    Powered,
    Heated,
    Extract,
    Consume,
    Produce,
    Commission,
    Dispatch,
    Advance,
    Load,
    Unload,
    Park,
    Refuel,
    Land,
    Embark,
    Bog,
    Free,
    Recover,
    Wear,
    Fade,
    Convey,
    Foul,
    Disperse,
    Promote,
    Provision,
    Export,
    Import,
    Build,
    Lay,
    String,
    Weather,
    Offer,
    CloseContract,
    DropContract,
    Relations,
    Fine,
    DefaultOnLoan,
    Wages,
    Contracted,
    Content,
    Morale,
    Schooling,
    Ageing,
    Death,
    Birth,
    Emigrate,
    Immigrate,
    GiveUp,
    Clear,
    Arrive,
    Fetch,
    CheckIn,
    Takings,
    Board,
    Settle,
}

impl Mutation {
    pub fn kind(&self) -> MutationKind {
        match self {
            Mutation::Staff { .. } => MutationKind::Staff,
            Mutation::Powered { .. } => MutationKind::Powered,
            Mutation::Heated { .. } => MutationKind::Heated,
            Mutation::Extract { .. } => MutationKind::Extract,
            Mutation::Consume { .. } => MutationKind::Consume,
            Mutation::Produce { .. } => MutationKind::Produce,
            Mutation::Commission { .. } => MutationKind::Commission,
            Mutation::Dispatch { .. } => MutationKind::Dispatch,
            Mutation::Advance { .. } => MutationKind::Advance,
            Mutation::Load { .. } => MutationKind::Load,
            Mutation::Unload { .. } => MutationKind::Unload,
            Mutation::Park { .. } => MutationKind::Park,
            Mutation::Refuel { .. } => MutationKind::Refuel,
            Mutation::Land { .. } => MutationKind::Land,
            Mutation::Embark { .. } => MutationKind::Embark,
            Mutation::Wear { .. } => MutationKind::Wear,
            Mutation::Fade { .. } => MutationKind::Fade,
            Mutation::Convey { .. } => MutationKind::Convey,
            Mutation::Foul { .. } => MutationKind::Foul,
            Mutation::Disperse { .. } => MutationKind::Disperse,
            Mutation::Promote { .. } => MutationKind::Promote,
            Mutation::Bog { .. } => MutationKind::Bog,
            Mutation::Free { .. } => MutationKind::Free,
            Mutation::Recover { .. } => MutationKind::Recover,
            Mutation::Provision { .. } => MutationKind::Provision,
            Mutation::Export { .. } => MutationKind::Export,
            Mutation::Import { .. } => MutationKind::Import,
            Mutation::Build { .. } => MutationKind::Build,
            Mutation::Lay { .. } => MutationKind::Lay,
            Mutation::String { .. } => MutationKind::String,
            Mutation::Weather { .. } => MutationKind::Weather,
            Mutation::Offer(_) => MutationKind::Offer,
            Mutation::CloseContract { .. } => MutationKind::CloseContract,
            Mutation::DropContract { .. } => MutationKind::DropContract,
            Mutation::Relations { .. } => MutationKind::Relations,
            Mutation::DefaultOnLoan { .. } => MutationKind::DefaultOnLoan,
            Mutation::Fine { .. } => MutationKind::Fine,
            Mutation::Wages { .. } => MutationKind::Wages,
            Mutation::Contracted { .. } => MutationKind::Contracted,
            Mutation::Content { .. } => MutationKind::Content,
            Mutation::Morale { .. } => MutationKind::Morale,
            Mutation::Schooling { .. } => MutationKind::Schooling,
            Mutation::Ageing { .. } => MutationKind::Ageing,
            Mutation::Death { .. } => MutationKind::Death,
            Mutation::Birth { .. } => MutationKind::Birth,
            Mutation::Emigrate { .. } => MutationKind::Emigrate,
            Mutation::Immigrate { .. } => MutationKind::Immigrate,
            Mutation::GiveUp { .. } => MutationKind::GiveUp,
            Mutation::Board { .. } => MutationKind::Board,
            Mutation::Clear { .. } => MutationKind::Clear,
            Mutation::Arrive { .. } => MutationKind::Arrive,
            Mutation::Fetch { .. } => MutationKind::Fetch,
            Mutation::CheckIn { .. } => MutationKind::CheckIn,
            Mutation::Takings { .. } => MutationKind::Takings,
            Mutation::Settle { .. } => MutationKind::Settle,
        }
    }
}

/// What each system is allowed to change.
///
/// This is a **contract, not documentation**: `no_system_writes_outside_its_
/// declaration` fails the build if a system emits a kind that is not listed
/// here, and `every_declared_write_is_actually_emitted` fails if a declaration
/// claims more than the system does. The second half matters as much as the
/// first — a write-set that has quietly become a superset stops constraining
/// anything, and it does so silently.
pub const WRITE_SETS: &[(&str, &[MutationKind])] = &[
    ("power", &[MutationKind::Powered]),
    // One kind, because a contracted day's work and the bill for it are one
    // transaction — see `Mutation::Contracted`.
    ("contracting", &[MutationKind::Contracted]),
    ("heating", &[MutationKind::Heated, MutationKind::Consume]),
    // Construction consumes as well as builds: the plant a crew works with is
    // owned by its office and wears out in proportion to the builder-days
    // actually worked, so an office with idle crews wears nothing.
    (
        "construction",
        &[
            MutationKind::Build,
            MutationKind::Lay,
            MutationKind::String,
            MutationKind::Consume,
        ],
    ),
    // Getting the builders to the work. Its own system rather than a branch of
    // `dispatch`, because the two rank completely different things: freight
    // ranks by downtime averted, and a crew posting ranks by the commissioning
    // order the construction queue already works to.
    ("crews", &[MutationKind::Dispatch, MutationKind::Bog]),
    (
        "production",
        &[
            MutationKind::Consume,
            MutationKind::Produce,
            MutationKind::Extract,
        ],
    ),
    (
        "households",
        &[MutationKind::Consume, MutationKind::Provision],
    ),
    ("trade", &[MutationKind::Export, MutationKind::Import]),
    ("weather", &[MutationKind::Weather]),
    ("commissioning", &[MutationKind::Commission]),
    // Daily, like contracts and loans: a wage is a day's pay, and a per-tick
    // sweep would bill a republic 1,440 times for one of them.
    ("wages", &[MutationKind::Wages]),
    // Daily, and for the same reason contracts are: a deadline is a day index,
    // so a per-tick sweep would default a republic 1,440 times over one unpaid
    // advance.
    (
        "loans",
        &[
            MutationKind::DefaultOnLoan,
            MutationKind::Fine,
            MutationKind::Relations,
        ],
    ),
    // Dispatch emits `Bog` because a lorry can stick on the very first crossing
    // out of the yard, and a single-leg journey has no leg boundary for the
    // fleet system to catch it at.
    ("dispatch", &[MutationKind::Dispatch, MutationKind::Bog]),
    (
        "fleet",
        &[
            MutationKind::Advance,
            MutationKind::Load,
            MutationKind::Unload,
            MutationKind::Park,
            MutationKind::Refuel,
            MutationKind::Land,
            MutationKind::Embark,
            MutationKind::Bog,
            MutationKind::Free,
            MutationKind::Recover,
            MutationKind::Wear,
            MutationKind::Clear,
            MutationKind::Board,
            MutationKind::Settle,
            MutationKind::Fetch,
            MutationKind::CheckIn,
        ],
    ),
    ("tracks", &[MutationKind::Fade, MutationKind::Promote]),
    // Per tick, like the fleet: a belt runs continuously, and the point of
    // building one is that the goods are simply there rather than arriving in
    // eight-tonne lumps whenever a driver is free.
    ("belts", &[MutationKind::Convey]),
    // Daily. What the republic throws away, and what it breathes.
    ("sanitation", &[MutationKind::Produce]),
    ("pollution", &[MutationKind::Foul, MutationKind::Disperse]),
    ("labour", &[MutationKind::Staff, MutationKind::Consume]),
    // Daily. How a home is doing, and how each person in it feels about it —
    // one system because the second is a drift toward the first, and computing
    // them apart would mean walking the population twice to read the same
    // answer.
    (
        "contentment",
        &[MutationKind::Content, MutationKind::Morale],
    ),
    ("schooling", &[MutationKind::Schooling]),
    (
        "demography",
        &[
            MutationKind::Ageing,
            MutationKind::Death,
            MutationKind::Birth,
        ],
    ),
    (
        "migration",
        &[
            MutationKind::Emigrate,
            MutationKind::Immigrate,
            MutationKind::GiveUp,
        ],
    ),
    // Fetching settlers in from a frontier post. Its own dispatcher rather than
    // a branch of `crews`, because the two rank different things and draw on
    // different vehicles: a bus depot's coaches must never be spent on
    // foundations, nor a construction office's buses on immigrants.
    ("settling", &[MutationKind::Dispatch, MutationKind::Bog]),
    // The fourth pool, and the fourth dispatcher. What it ranks is a stretch of
    // buried road rather than a consignee, which is why it is not a branch of
    // `dispatch`.
    //
    // **No `Bog`, and that is a consequence of the vehicle rather than an
    // omission.** A plough's ground capability is a recovery vehicle's — above
    // the whole scale — so the roll can never come up against it, exactly as
    // intended: a machine that got stuck in the snow it was sent to shift would
    // need a machine sent after it. Declaring `Bog` here would have been a
    // declaration nothing could reach, which constrains nothing and looks fine.
    ("clearing", &[MutationKind::Dispatch]),
    // Visitors turning up, spending and going home. Daily, for the reason
    // wages and contracts are: a night in a hotel is a day's takings, and a
    // per-tick sweep would charge a party 1,440 times for one.
    ("tourism", &[MutationKind::Arrive, MutationKind::Takings]),
    // And the coach that fetches them. It shares the passenger pool with
    // `settling` and runs after it, which is what says a settler outranks a
    // visitor for the last coach.
    ("touring", &[MutationKind::Dispatch, MutationKind::Bog]),
    (
        "contracts",
        &[
            MutationKind::Offer,
            MutationKind::CloseContract,
            MutationKind::DropContract,
            MutationKind::Relations,
            MutationKind::Fine,
        ],
    ),
];

/// The single writer.
pub fn apply(world: &mut World, mutations: &[Mutation]) {
    for mutation in mutations {
        match mutation {
            &Mutation::Staff { building, count } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.staff = count;
                }
            }
            &Mutation::Powered { building, on } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.powered = on;
                }
            }
            &Mutation::Heated { building, on } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.heated = on;
                }
            }
            &Mutation::Extract {
                deposit,
                building,
                resource,
                tonnes,
            } => {
                let got = world
                    .geology
                    .get_mut(deposit)
                    .map(|d| d.extract(tonnes).tonnes)
                    .unwrap_or(Tonnes::ZERO);
                if let Some(b) = world.buildings.get_mut(building) {
                    let room = b.storage_cap().saturating_sub(b.stock.get(resource));
                    b.stock.add(resource, got.min(room));
                }
            }
            &Mutation::Consume {
                building,
                resource,
                tonnes,
            } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.stock.take(resource, tonnes);
                }
            }
            &Mutation::Produce {
                building,
                resource,
                tonnes,
            } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    let room = b.storage_cap().saturating_sub(b.stock.get(resource));
                    b.stock.add(resource, tonnes.min(room));
                }
            }
            Mutation::Clear { cells } => {
                for &cell in cells {
                    world.lattice.clear(cell);
                }
            }
            &Mutation::Provision {
                building,
                fraction,
                comforts,
                drink,
            } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.provisioned = fraction.clamp(0.0, 1.0);
                    b.comforted = comforts.clamp(0.0, 1.0);
                    b.drink = drink.clamp(0.0, 1.0);
                }
            }
            &Mutation::Export {
                customs,
                resource,
                tonnes,
                market,
                payment,
                contract,
            } => {
                let sold = world
                    .buildings
                    .get_mut(customs)
                    .map(|b| b.stock.take(resource, tonnes))
                    .unwrap_or(Tonnes::ZERO);
                // Pay for what actually crossed, not for what was hoped for.
                if tonnes.is_positive() {
                    world.treasury.credit(market, payment * (sold.0 / tonnes.0));
                }
                // And book it against the tender in the same breath, so a
                // delivery can never be paid for without being credited.
                let day = world.clock.day_index();
                if let Some(id) = contract
                    && let Some(c) = world.contracts.get_mut(id)
                {
                    c.delivered += sold;
                    if !c.outstanding().is_positive() && c.state == ContractState::Active {
                        c.state = ContractState::Done;
                        c.closed_day = Some(day);
                    }
                }
            }
            &Mutation::Offer(contract) => {
                if world.contracts.get(contract.id).is_none() {
                    world.contracts.insert(contract);
                }
            }
            &Mutation::CloseContract { contract, state } => {
                let day = world.clock.day_index();
                if let Some(c) = world.contracts.get_mut(contract) {
                    c.state = state;
                    c.closed_day = Some(day);
                }
            }
            &Mutation::DropContract { contract } => {
                world.contracts.remove(contract);
            }
            &Mutation::DefaultOnLoan { market } => {
                world.loans.default_on(market);
            }
            &Mutation::Relations { market, penalty } => {
                world.contracts.set_penalty(market, penalty);
            }
            &Mutation::Fine { market, amount } => {
                world.treasury.debit(market, amount);
            }
            Mutation::Wages {
                market,
                paid,
                dismissed,
            } => {
                world.treasury.debit(*market, *paid);
                for &(office, heads) in dismissed {
                    world.crews.let_go(office, *market, heads);
                }
            }
            &Mutation::Contracted {
                site,
                market,
                builder_days,
                paid,
            } => {
                world.treasury.debit(market, paid);
                if let Some(b) = world.buildings.get_mut(site) {
                    b.work_done += builder_days;
                }
                // A finished contract stops being a contract. Without this the
                // firm would go on billing for a building that is already up.
                if let Some(b) = world.buildings.get(site)
                    && b.is_built()
                    && let Some(b) = world.buildings.get_mut(site)
                {
                    b.contractor = None;
                }
            }
            &Mutation::Import {
                customs,
                resource,
                tonnes,
                market,
                cost,
                for_site,
            } => {
                let spent = world.treasury.debit(market, cost);
                if let Some(site) = for_site {
                    // Booked against the site in the same breath the money
                    // leaves, so an allowance cannot fall without goods and
                    // goods cannot arrive without the allowance falling.
                    world.build_policy.record_bought(site, resource, tonnes);
                }
                // Deliver in proportion to what was actually paid.
                let landed = if cost > 0.0 {
                    Tonnes(tonnes.0 * (spent / cost))
                } else {
                    tonnes
                };
                if let Some(b) = world.buildings.get_mut(customs) {
                    let room = b
                        .intake_capacity(resource)
                        .saturating_sub(b.stock.get(resource));
                    b.stock.add(resource, landed.min(room));
                }
            }
            &Mutation::Build { site, builder_days } => {
                let Some(b) = world.buildings.get(site) else {
                    continue;
                };
                let def = b.def();
                if def.labour <= 0.0 {
                    continue;
                }
                let remaining = (def.labour - b.work_done).max(0.0);
                let worked = builder_days.min(remaining);
                let share = worked / def.labour;
                // Materials are consumed in step with the work, so a site
                // half-built has half its materials in it and half in the
                // fabric. Bulldozing one back returns what is left.
                for &(resource, quantity) in def.materials {
                    if let Some(b) = world.buildings.get_mut(site) {
                        b.stock.take(resource, Tonnes(quantity * share));
                    }
                }
                if let Some(b) = world.buildings.get_mut(site) {
                    b.work_done += worked;
                }
                // The last builder-day opens the building, and the crew that
                // laid it is standing outside a finished thing with no work and
                // no lift. Releasing them here rather than in the crews system
                // is what keeps "there is a party working a site that is not a
                // site" out of the states the world can hold.
                if let Some(b) = world.buildings.get(site)
                    && b.is_built()
                {
                    let (at, id, def) = (b.centre, b.id, b.def());
                    world.crews.release(Destination::Building(id), at);
                    // An aerodrome *is* the air network, so opening one changes
                    // where every aeroplane in the republic can fly. Derived at
                    // the event that invalidates it, not per tick — the same
                    // discipline as the utility grids.
                    if def.medium == Some(crate::journey::Medium::Air) {
                        world.re_survey_airways();
                    }
                }
            }
            &Mutation::Weather { ground, snowfall } => {
                world.ground = ground;
                if ground.snow <= 0.0 {
                    world.lattice.thaw();
                } else if snowfall > 0.0 {
                    world.lattice.bury(snowfall);
                }
            }
            &Mutation::Lay { site, builder_days } => {
                let Some(road) = world.roadworks.get(site) else {
                    continue;
                };
                let labour = road.labour();
                if labour <= 0.0 {
                    continue;
                }
                let worked = builder_days.min((labour - road.work_done).max(0.0));
                // Materials go in step with the work, exactly as on a building.
                let bill = road.materials();
                let share = worked / labour;
                if let Some(road) = world.roadworks.get_mut(site) {
                    for (resource, quantity) in bill {
                        road.stock.take(resource, Tonnes(quantity.0 * share));
                    }
                    road.work_done += worked;
                }
                // And the last builder-day opens it. The crew is set down where
                // the site's depot stood — the site itself stops existing in
                // this same transaction, so a party still pointed at it would be
                // pointed at nothing.
                if world.roadworks.get(site).is_some_and(|r| r.is_finished())
                    && let Some(opened) = world.roadworks.remove(site)
                {
                    world
                        .crews
                        .release(Destination::RoadSite(site), opened.depot());
                    world.build_policy.forget(Destination::RoadSite(site));
                    // Into whichever network this grade joins — a finished
                    // railway is not a road that trains happen to use.
                    let grade = opened.grade;
                    roadworks::open(world.network_for(grade), &opened);
                }
            }
            &Mutation::String { site, builder_days } => {
                let Some(line) = world.lineworks.get(site) else {
                    continue;
                };
                let labour = line.labour();
                if labour <= 0.0 {
                    continue;
                }
                let worked = builder_days.min((labour - line.work_done).max(0.0));
                let bill = line.materials();
                let share = worked / labour;
                if let Some(line) = world.lineworks.get_mut(site) {
                    for (resource, quantity) in bill {
                        line.stock.take(resource, Tonnes(quantity.0 * share));
                    }
                    line.work_done += worked;
                }
                // The last builder-day energises it, and everything already
                // standing within reach is plugged in on the same transaction.
                // A span that existed but connected nobody until the next
                // placement would be a grid that came alive for no reason the
                // player could see.
                if world.lineworks.get(site).is_some_and(|l| l.is_finished())
                    && let Some(strung) = world.lineworks.remove(site)
                {
                    world
                        .crews
                        .release(Destination::LineSite(site), strung.depot());
                    world.build_policy.forget(Destination::LineSite(site));
                    world.utilities.energise(&strung);
                    world.wire_up(strung.kind);
                }
            }
            &Mutation::Commission { garage, kind } => {
                if let Some(yard) = world.buildings.get(garage).map(|b| b.centre) {
                    world.fleet.commission(kind, garage, yard);
                }
            }
            Mutation::Dispatch {
                vehicle,
                job,
                journey,
                refuel,
            } => {
                // Top up from the garage's tank on the way out. What is not
                // there is not taken; dispatch has already checked that what is
                // there covers the round trip.
                let home = world.fleet.get(*vehicle).map(|v| v.home);
                let drawn = home
                    .and_then(|h| world.buildings.get_mut(h))
                    .map(|b| b.stock.take(Resource::Fuel, *refuel))
                    .unwrap_or(Tonnes::ZERO);
                // A bus setting out on a ferry leaves **with the crew aboard**,
                // because the crew is at the office and the office is the bus's
                // home. That is why the state a dispatch enters is read off the
                // job rather than fixed: everything else drives out empty to
                // fetch something, and a ferry is already carrying it.
                let ferry = job.ferry().and_then(|(_, heads)| {
                    let v = world.fleet.get(*vehicle)?;
                    Some((v.home, v.at, heads))
                });
                if let Some((office, at, heads)) = ferry {
                    world.crews.send(office, heads, at, *vehicle);
                }
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = (v.fuel + drawn).min(v.def().tank);
                    v.job = Some(*job);
                    v.journey = Some(journey.clone());
                    v.state = if ferry.is_some() {
                        VehicleState::Delivering
                    } else {
                        VehicleState::Fetching
                    };
                }
            }
            Mutation::Land {
                vehicle,
                party,
                site,
                at,
                journey,
                burn,
            } => {
                if let Some(p) = world.crews.get_mut(*party) {
                    p.riding = None;
                    p.working = Some(*site);
                    p.at = *at;
                }
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    v.at = *at;
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
            Mutation::Embark {
                vehicle,
                party,
                journey,
                burn,
            } => {
                let boarded = world.crews.get_mut(*party).map(|p| {
                    p.working = None;
                    p.riding = Some(*vehicle);
                    p.at
                });
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    if let Some(at) = boarded {
                        v.at = at;
                    }
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
            &Mutation::Advance {
                vehicle,
                leg,
                leg_start,
                leg_end,
                burn,
            } => {
                if let Some(v) = world.fleet.get_mut(vehicle) {
                    v.fuel = v.fuel.saturating_sub(burn);
                    if let Some(j) = v.journey.as_mut() {
                        j.leg = leg;
                        j.leg_start = leg_start;
                        j.leg_end = leg_end;
                    }
                    if let Some(here) = v.journey.as_ref().map(|j| j.leg_from()) {
                        v.at = here;
                    }
                }
            }
            Mutation::Load {
                vehicle,
                from,
                resource,
                tonnes,
                journey,
                state,
                burn,
            } => {
                let taken = world
                    .buildings
                    .get_mut(*from)
                    .map(|b| b.stock.take(*resource, *tonnes))
                    .unwrap_or(Tonnes::ZERO);
                let bay = world.buildings.get(*from).map(|b| b.centre);
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    v.cargo.add(*resource, taken);
                    if let Some(bay) = bay {
                        v.at = bay;
                    }
                    v.journey = Some(journey.clone());
                    v.state = *state;
                }
            }
            Mutation::Unload {
                vehicle,
                to,
                resource,
                tonnes,
                journey,
                burn,
            } => {
                // Only what the lorry actually has comes off, and only as much
                // as the bin will take. The rest stays on the bed rather than
                // evaporating — freight is conserved end to end.
                let aboard = world
                    .fleet
                    .get(*vehicle)
                    .map(|v| v.cargo.get(*resource))
                    .unwrap_or(Tonnes::ZERO);
                let Some(consignee) = world.consignee(*to, *resource) else {
                    continue;
                };
                let landed = tonnes
                    .min(aboard)
                    .min(consignee.capacity.saturating_sub(consignee.held));
                match *to {
                    Destination::Building(id) => {
                        if let Some(b) = world.buildings.get_mut(id) {
                            b.stock.add(*resource, landed);
                        }
                    }
                    Destination::RoadSite(id) => {
                        if let Some(road) = world.roadworks.get_mut(id) {
                            road.stock.add(*resource, landed);
                        }
                    }
                    Destination::LineSite(id) => {
                        if let Some(line) = world.lineworks.get_mut(id) {
                            line.stock.add(*resource, landed);
                        }
                    }
                }
                let bay = Some(consignee.at);
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    v.cargo.take(*resource, landed);
                    if let Some(bay) = bay {
                        v.at = bay;
                    }
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
            Mutation::Wear { cells, by } => {
                for &cell in cells {
                    world.lattice.wear_in(cell, *by);
                }
            }
            &Mutation::Fade { by } => world.lattice.fade(by),
            &Mutation::Convey {
                from,
                to,
                resource,
                tonnes,
            } => {
                // Taken first, then put down, and only as much as was really
                // there: a belt conserves goods end to end exactly as a lorry
                // does.
                let lifted = world
                    .buildings
                    .get_mut(from)
                    .map(|b| b.stock.take(resource, tonnes))
                    .unwrap_or(Tonnes::ZERO);
                if let Some(b) = world.buildings.get_mut(to) {
                    let room = b
                        .intake_capacity(resource)
                        .saturating_sub(b.stock.get(resource));
                    b.stock.add(resource, lifted.min(room));
                }
            }
            &Mutation::Foul { cell, by } => world.lattice.foul(cell, by),
            &Mutation::Disperse { by } => world.lattice.disperse(by),
            Mutation::Promote { cells } => {
                // Each worn cell joins the worn cells beside it. A corridor is
                // one or two cells wide, so what comes out is a chain rather
                // than a mesh — and `are_connected` keeps a track that is
                // promoted again tomorrow from laying a second carriageway.
                let speed = crate::roadworks::Grade::Dirt.def().speed;
                let merge = crate::roadworks::JUNCTION_MERGE;
                for &cell in cells {
                    let here = world.lattice.centre_of(cell);
                    let a = world.roads.junction_at(here, merge);
                    for next in world.lattice.neighbours(cell) {
                        if !cells.contains(&next) {
                            continue;
                        }
                        let there = world.lattice.centre_of(next);
                        let b = world.roads.junction_at(there, merge);
                        if a != b && !world.roads.are_connected(a, b) {
                            world.roads.connect(a, b, speed);
                        }
                    }
                }
            }
            &Mutation::Bog { vehicle, day } => {
                if let Some(v) = world.fleet.get_mut(vehicle)
                    && let Some(was) = v.state.doing()
                {
                    v.state = VehicleState::Bogged {
                        was,
                        since_day: day,
                    };
                }
            }
            &Mutation::Free {
                vehicle,
                was,
                leg,
                leg_start,
                leg_end,
            } => {
                if let Some(v) = world.fleet.get_mut(vehicle) {
                    v.state = was.state();
                    if let Some(j) = v.journey.as_mut() {
                        j.leg = leg;
                        j.leg_start = leg_start;
                        j.leg_end = leg_end;
                    }
                }
            }
            Mutation::Recover {
                recovery,
                casualty,
                was,
                casualty_leg,
                casualty_start,
                casualty_end,
                journey,
                burn,
            } => {
                if let Some(stuck) = world.fleet.get_mut(*casualty) {
                    stuck.state = was.state();
                    if let Some(j) = stuck.journey.as_mut() {
                        j.leg = *casualty_leg;
                        j.leg_start = *casualty_start;
                        j.leg_end = *casualty_end;
                        // Set down at the far side of what beat it.
                        stuck.at = j.leg_to();
                    }
                }
                if let Some(v) = world.fleet.get_mut(*recovery) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    v.at = journey.path.first().copied().unwrap_or(v.at);
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
            &Mutation::Refuel {
                vehicle,
                from,
                tonnes,
            } => {
                let drawn = world
                    .buildings
                    .get_mut(from)
                    .map(|b| b.stock.take(Resource::Fuel, tonnes))
                    .unwrap_or(Tonnes::ZERO);
                if let Some(v) = world.fleet.get_mut(vehicle) {
                    v.fuel += drawn;
                }
            }
            &Mutation::Park { vehicle, burn } => {
                let Some(v) = world.fleet.get(vehicle) else {
                    continue;
                };
                let home = v.home;
                // Anyone riding is home: the heads go back into the office's
                // establishment, which is what makes them postable again. If
                // they were foreign labour on its way in, this is the moment
                // they join the books — before it they are travelling, and an
                // office cannot post people who are still at the border.
                if let Some(party) = world.crews.riding(vehicle).map(|p| p.id)
                    && let Some(party) = world.crews.dissolve(party)
                    && let Some(market) = party.hired_from
                {
                    world.crews.take_on(party.office, market, party.heads);
                }
                let aboard: Vec<(Resource, Tonnes)> = v.cargo.iter().collect();
                let yard = world.buildings.get(home).map(|b| b.centre);
                // Anything the lorry came back with is tipped in the yard, so
                // an undeliverable load is not carried around for ever.
                let mut landed: Vec<(Resource, Tonnes)> = Vec::new();
                if let Some(b) = world.buildings.get_mut(home) {
                    for (resource, tonnes) in aboard {
                        let room = b
                            .intake_capacity(resource)
                            .saturating_sub(b.stock.get(resource));
                        let fits = tonnes.min(room);
                        if fits.is_positive() {
                            b.stock.add(resource, fits);
                            landed.push((resource, fits));
                        }
                    }
                }
                if let Some(v) = world.fleet.get_mut(vehicle) {
                    v.fuel = v.fuel.saturating_sub(burn);
                    for (resource, tonnes) in landed {
                        v.cargo.take(resource, tonnes);
                    }
                    if let Some(yard) = yard {
                        v.at = yard;
                    }
                    v.state = VehicleState::Idle;
                    v.job = None;
                    v.journey = None;
                }
            }
            &Mutation::Content { building, content } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.content = content;
                }
            }
            Mutation::Morale { updates } => {
                world.population.set_wellbeing(updates);
            }
            Mutation::Schooling { attended, enrolled } => {
                world.population.school(attended, enrolled);
            }
            Mutation::Ageing { citizens } => {
                world.population.age_by_one(citizens);
            }
            Mutation::Death { citizens } => {
                for id in citizens {
                    world.population.remove(*id);
                }
            }
            Mutation::Birth { homes } => {
                for home in homes {
                    world.population.spawn_citizen(*home, 0);
                }
            }
            Mutation::Emigrate { citizens } => {
                let mut gone = 0;
                for id in citizens {
                    if world.population.remove(*id) {
                        gone += 1;
                    }
                }
                world.migration.record_departures(gone);
            }
            &Mutation::Immigrate { at, heads } => {
                world.migration.arrive(at, heads, world.clock.day_index());
            }
            &Mutation::GiveUp { group } => {
                world.migration.give_up(group);
            }
            &Mutation::Arrive { at, heads, market } => {
                world
                    .tourism
                    .arrive(at, heads, market, world.clock.day_index());
            }
            Mutation::Fetch {
                vehicle,
                visit,
                journey,
                burn,
            } => {
                let boarded = world.tourism.get_mut(*visit).map(|v| {
                    v.riding = Some(*vehicle);
                    v.at
                });
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    if let Some(at) = boarded {
                        v.at = at;
                    }
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Delivering;
                }
            }
            Mutation::CheckIn {
                vehicle,
                visit,
                hotel,
                at,
                journey,
                burn,
            } => {
                // A hotel pulled down or filled while the coach was in the air
                // is the same case a demolished estate is: the party has
                // nowhere to go and goes home, and the ledger records it rather
                // than losing them quietly.
                let room = world
                    .buildings
                    .get(*hotel)
                    .filter(|b| b.is_built())
                    .map(|b| b.def().beds.saturating_sub(world.tourism.booked_at(*hotel)))
                    .unwrap_or(0);
                let heads = world.tourism.get(*visit).map_or(0, |v| v.heads);
                if room >= heads && heads > 0 {
                    world
                        .tourism
                        .check_in(*visit, *hotel, *at, world.clock.day_index());
                } else {
                    world.tourism.end(*visit);
                }
                let yard = world
                    .fleet
                    .get(*vehicle)
                    .and_then(|v| world.buildings.get(v.home))
                    .map(|b| b.centre);
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    if let Some(at) = yard {
                        let _ = at;
                    }
                    v.at = *at;
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
            Mutation::Takings {
                market,
                amount,
                leaving,
            } => {
                if *amount > 0.0 {
                    world.treasury.credit(*market, *amount);
                    world.tourism.take(*market, *amount);
                }
                for visit in leaving {
                    world.tourism.end(*visit);
                }
            }
            Mutation::Board {
                vehicle,
                group,
                journey,
                burn,
            } => {
                let boarded = world.migration.get_mut(*group).map(|g| {
                    g.riding = Some(*vehicle);
                    g.at
                });
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    if let Some(at) = boarded {
                        v.at = at;
                    }
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Delivering;
                }
            }
            Mutation::Settle {
                vehicle,
                group,
                home,
                journey,
                burn,
            } => {
                // How many the block can actually take, now. A coach that set
                // out for an estate the player has since demolished — or filled
                // with somebody else's settlers — sets down who fits, and the
                // rest go home. That is a consequence of ordering a demolition
                // with people in the air, and the ledger records it rather than
                // losing them quietly.
                let occupied = world
                    .population
                    .residents_by_home()
                    .get(home)
                    .copied()
                    .unwrap_or(0);
                let room = world
                    .buildings
                    .get(*home)
                    .filter(|b| b.is_built())
                    .map(|b| b.def().residents.saturating_sub(occupied))
                    .unwrap_or(0);
                if let Some(g) = world.migration.settle(*group) {
                    let taken = g.heads.min(room);
                    for _ in 0..taken {
                        // Spread across working life so an intake is not a
                        // cohort that retires together. They arrive schooled
                        // because they were taught somewhere else — see
                        // `Population::spawn_citizen`.
                        let age = 20 + (world.population.count() as u32 % 30);
                        world.population.spawn_citizen(*home, age);
                    }
                    world.migration.record_turned_away(g.heads - taken);
                }
                let yard = world
                    .fleet
                    .get(*vehicle)
                    .and_then(|v| world.buildings.get(v.home))
                    .map(|b| b.centre);
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = v.fuel.saturating_sub(*burn);
                    if let Some(at) = world.buildings.get(*home).map(|b| b.centre).or(yard) {
                        v.at = at;
                    }
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Returning;
                }
            }
        }
    }
}

/// Fraction of a day one tick represents.
fn tick_days() -> f64 {
    TICK.0 / Seconds::from_days(1.0).0
}

/// Who the grid can feed.
///
/// Generation comes from plants that are staffed and have fuel. Demand is met
/// in commissioning order, which is deliberate but **provisional**: in the
/// archived build who goes dark was the player's decision, expressed as an
/// ordering over build categories, and that control belongs here too once
/// there is a screen to express it. Ordering by id at least makes the answer
/// reproducible rather than arbitrary.
/// How far a Transformer Station serves.
///
/// It is what a *consumer* connects to, and the reason there are two hops
/// rather than one: high-voltage line to a station, low-voltage station to the
/// street. Modelling it as one hop would make a pylon strung past a factory
/// enough to run it, which is not how electricity works and would make the
/// station a building with nothing to do.
pub const TRANSFORMER_RANGE: Metres = Metres(450.0);

/// What generation and demand one network sees.
#[derive(Debug, Clone, Copy, Default)]
struct Supply {
    /// Megawatts arriving after line losses.
    available: f64,
    /// Megawatts already committed on this pass.
    drawn: f64,
}

/// Who the grid can feed.
///
/// **Per network, not per republic.** Until the utility module existed a plant
/// anywhere on the map lit every building on it — the same free thing freight
/// was before lorries. Now a generator feeds only the network it is strung to,
/// and a consumer draws only from a **Transformer Station** within
/// [`TRANSFORMER_RANGE`] that is itself on a network with generation on it.
///
/// Losses are charged on the **span** of the network, so a grid that sprawls
/// across the map delivers less of what it makes than a compact one. That is
/// the argument for siting a plant near what it serves, and it is the only
/// thing that makes a long line a real trade rather than a formality.
///
/// Demand is still met in commissioning order within a network, which is
/// deliberate but **provisional**: who goes dark when there is not enough is
/// the player's decision, and the ordering at least makes the answer
/// reproducible until there is a screen to make it on.
pub fn power(world: &World) -> Vec<Mutation> {
    use crate::utility::Utility;

    let mut supply: BTreeMap<u32, Supply> = BTreeMap::new();
    for b in world.buildings.all() {
        if !b.is_built() {
            continue; // a half-built plant generates nothing
        }
        let def = b.def();
        if def.power_output <= 0.0 || b.staffing() <= 0.0 {
            continue;
        }
        let fuelled = def
            .inputs
            .iter()
            .all(|&(r, _)| b.stock.get(r).is_positive());
        if !fuelled {
            continue;
        }
        // A plant that is not strung to anything lights nothing — including
        // itself, which is correct: an isolated power station is a shed full of
        // turbines with nowhere to send the current.
        let Some(network) = world.utilities.network_of(b.id, Utility::Power) else {
            continue;
        };
        let span = world.utilities.span_of(network, Utility::Power);
        let kept = (1.0 - Utility::Power.def().loss_per_km * span.as_km()).clamp(0.0, 1.0);
        supply.entry(network).or_default().available += def.power_output * b.activity() * kept;
    }

    // The stations that actually serve anybody: built, staffed, and on a
    // network. Collected once rather than searched per consumer.
    let stations: Vec<(Point, u32)> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().transforms && b.staffing() > 0.0)
        .filter_map(|b| {
            world
                .utilities
                .network_of(b.id, Utility::Power)
                .map(|n| (b.centre, n))
        })
        .collect();

    let mut out = Vec::new();
    let mut consumers: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().power_draw > 0.0)
        .collect();
    consumers.sort_by_key(|b| b.id);

    for b in consumers {
        let draw = b.def().power_draw;
        // The nearest station in reach, and through it the network it is on.
        let network = stations
            .iter()
            .filter(|(at, _)| at.distance_to(b.centre).0 <= TRANSFORMER_RANGE.0)
            .min_by(|(a, na), (c, nc)| {
                a.distance_to(b.centre)
                    .0
                    .total_cmp(&c.distance_to(b.centre).0)
                    .then_with(|| na.cmp(nc))
            })
            .map(|(_, n)| *n);
        let on = match network.and_then(|n| supply.get_mut(&n).map(|s| (n, s))) {
            Some((_, s)) if s.drawn + draw <= s.available => {
                s.drawn += draw;
                true
            }
            _ => false,
        };
        out.push(Mutation::Powered { building: b.id, on });
    }
    out
}

/// The boilers: who stays warm, and what that costs in coal.
///
/// # Why this is not seasonal
///
/// Demand is a function of **today's temperature**, not of the month. That was
/// an explicit rule in the archived build and it is what makes winter a thing
/// that can catch a republic out: a mild January sips fuel and a cold snap in
/// an ordinary month burns through a stockpile. A calendar cannot produce that
/// event, and the event is the reason heating is simulated at all.
///
/// # The consequence is fuel, not comfort
///
/// A heating plant throttles to demand and burns coal in proportion to what it
/// actually made, so a hard winter is a **coal draw competing with the power
/// station** — from the same stockpile, on the same freight. That competition
/// is heating's real teeth today. Whether a cold building does anything else to
/// the people inside it is a question for a happiness model that does not exist
/// yet, and inventing a consequence for it now would be inventing balance.
pub fn heating(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let factor = climate::heat_demand_factor(world.temperature());

    // Nothing to do when it is warm out. Everything that wants heat is
    // trivially satisfied, which is not the same as being served — there is
    // simply nothing to serve.
    if factor <= 0.0 {
        return world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().heat > 0.0)
            .map(|b| Mutation::Heated {
                building: b.id,
                on: true,
            })
            .collect();
    }

    use crate::utility::Utility;

    // Heat is a **network** quantity, not a republic-wide one. A block is warm
    // only if a main runs past it and something on the *same* main is burning
    // coal — which is what makes district heating a town-scale decision rather
    // than a number that covers the map.
    let network_of =
        |b: &crate::building::Building| world.utilities.network_of(b.id, Utility::Heat);

    let mut demand: BTreeMap<u32, f64> = BTreeMap::new();
    for b in world.buildings.all() {
        if !b.is_built() || b.def().heat <= 0.0 {
            continue;
        }
        if let Some(n) = network_of(b) {
            *demand.entry(n).or_default() += b.def().heat * factor;
        }
    }

    let mut out = Vec::new();
    let mut produced: BTreeMap<u32, f64> = BTreeMap::new();
    let mut boilers: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().heat_output > 0.0)
        .collect();
    boilers.sort_by_key(|b| b.id);

    for boiler in boilers {
        // A boiler house with no main out of it heats nothing but itself, and
        // the simulation says so rather than quietly warming the republic.
        let Some(network) = network_of(boiler) else {
            continue;
        };
        let wanted = demand.get(&network).copied().unwrap_or(0.0);
        let def = boiler.def();
        // A boiler house is a building like any other: no crew or no
        // electricity for its pumps and it makes nothing. Exempting it would
        // mean a blackout in January quietly left the heating on.
        if def.power_draw > 0.0 && !boiler.powered {
            continue;
        }
        let capacity = def.heat_output * boiler.activity();
        if capacity <= 0.0 {
            continue;
        }
        // What survives the pipes. Heat leaks badly — seven per cent a
        // kilometre — so a main strung across the map delivers a fraction of
        // what the boiler burnt for, which is the whole reason heating is a
        // town-scale thing.
        let span = world.utilities.span_of(network, Utility::Heat);
        let kept = (1.0 - Utility::Heat.def().loss_per_km * span.as_km()).clamp(0.0, 1.0);

        // Throttle to what is still wanted **at the far end of the main**. A
        // boiler serving a mild day does not burn a cold day's coal, and a
        // boiler on a main that serves three blocks does not burn a city's
        // worth — but it must burn enough to cover what the pipes lose.
        //
        // Getting this wrong was the first bug this network produced, and only
        // a measurement found it: throttling to `wanted / capacity` and *then*
        // taking the loss out means the boiler is deliberately short by exactly
        // the loss, every time. The founding's third block went cold in January
        // with the boiler running at 71% and coal in the bunker.
        let throttle = if kept <= 0.0 {
            0.0
        } else {
            (wanted / (capacity * kept)).clamp(0.0, 1.0)
        };
        if throttle <= 0.0 {
            continue;
        }
        // And burn only in proportion to what it actually manages to make.
        let mut fuel_factor: f64 = 1.0;
        for &(resource, rate) in def.inputs {
            let needed = rate * day * boiler.activity() * throttle;
            if needed > 0.0 {
                fuel_factor =
                    fuel_factor.min((boiler.stock.get(resource).0 / needed).clamp(0.0, 1.0));
            }
        }
        let running = throttle * fuel_factor;
        if running <= 0.0 {
            continue;
        }
        for &(resource, rate) in def.inputs {
            out.push(Mutation::Consume {
                building: boiler.id,
                resource,
                tonnes: Tonnes(rate * day * boiler.activity() * running),
            });
        }
        let made = capacity * running * kept;
        *produced.entry(network).or_default() += made;
        if let Some(left) = demand.get_mut(&network) {
            *left = (*left - made).max(0.0);
        }
    }

    // Allocate in commissioning order, the same tie-break power uses and for
    // the same reason: who goes cold when there is not enough is the player's
    // decision to make once there is a screen to make it on, and until then the
    // answer at least has to be reproducible rather than arbitrary.
    let mut consumers: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().heat > 0.0)
        .collect();
    consumers.sort_by_key(|b| b.id);

    let mut budget = produced;
    for b in consumers {
        let need = b.def().heat * factor;
        // A block with no main past it is cold, whatever the republic is
        // burning elsewhere. This is the state the whole network exists to make
        // representable, and it is why heating is no longer a number.
        let Some(network) = network_of(b) else {
            out.push(Mutation::Heated {
                building: b.id,
                on: false,
            });
            continue;
        };
        let budget = budget.entry(network).or_default();
        // The epsilon is load-bearing, and the trajectory runner is what found
        // it: a boiler throttled to demand produces *exactly* demand, but
        // `demand` was accumulated by summing and `budget` is spent by
        // subtracting, and those two orders do not round the same way. Without
        // slack the last block on the list came up a few ulps short and went
        // cold on mild days — 67% warm housing in November while a January at
        // -15 was fine, which is the wrong way round and is what gave it away.
        let on = *budget >= need - 1e-9;
        if on {
            *budget -= need;
        }
        out.push(Mutation::Heated { building: b.id, on });
    }
    out
}

/// The most builders one site can absorb. Ported from the archive: throwing the
/// whole republic's crew at one foundation does not make it set faster.
///
/// It is now a **posting** limit rather than a per-day one, which is the same
/// number meaning something firmer. A crew of this size is put on a site and
/// stays there; what a republic can build at once is therefore the number of
/// gangs it can field and carry, not a pool of days divided up.
pub const BUILDERS_PER_SITE: u32 = 10;

/// Tonnes of machinery a single builder-day wears out.
///
/// The industrialisation tax, pointed at the one place in the republic that had
/// no machinery demand at all. It is charged to the **office**, because the
/// office owns the plant — which is what makes a second office a real decision
/// rather than only more people, and what finally gives `Machine Works`
/// something to make for somebody.
///
/// Authored against the office's declared appetite: twenty builders working a
/// full day wear 0.4 t, which is exactly what `ConstructionOffice` asks the
/// resupply ranking for.
pub const MACHINERY_PER_BUILDER_DAY: f64 = 0.02;

/// Putting up what has been ordered.
///
/// **Work is done by the builders standing on the site**, and they got there on
/// a bus — see [`crews`]. That is the whole difference from what this used to
/// be: every office in the republic pooled its staff into a number of
/// builder-days, and that number was spent on whatever was next in the queue
/// however far away it stood. A site on the far side of the map cost exactly
/// what a site next door cost.
///
/// A site progresses only when the materials for the work still to do are on
/// hand: a half-delivered site waits with its crew standing on it, which is what
/// makes freight priority matter during a build-out and what makes an idle gang
/// something the player can see and act on.
///
/// Sites are worked in commissioning order, which now decides which site gets a
/// crew rather than which site gets the days.
/// Builder-days a contracted firm works in a day, per site.
///
/// A gang of your own is ten people and works ten builder-days a day. A firm
/// brings its own gang and works rather faster, because it brings enough of them
/// — you are buying capacity you do not have to house, feed or train.
pub const CONTRACTOR_DAYS: f64 = 18.0;

/// What one contracted builder-day costs, in the bloc's own currency.
///
/// Several times what your own crews cost, which is the entire argument for
/// building a Construction Office and training people. A republic that never
/// stops contracting is a republic spending its grant on what it could have
/// done itself — and that is a decision the player gets to make badly.
pub const CONTRACTOR_RATE: f64 = 340.0;

/// Foreign firms working the sites the republic paid them to.
///
/// **The bootstrap.** A blank map has no Construction Office, no crews and no
/// materials, so nothing the republic owns can raise a building; this is the
/// only thing that can, and it is why a posting opens with money rather than a
/// town. After the opening it should be the expensive option nobody reaches for
/// twice.
///
/// A contracted site needs no crew and no materials — that is what is being
/// bought. It does still take its turn in the commissioning order, because a
/// republic with three contracts running is spending three times as fast and
/// should see them finish in the order it asked for them.
///
/// **A republic that runs out simply stops.** `Treasury::debit` refuses to go
/// negative, so an unaffordable day buys proportionally less work rather than
/// going into debt. That is the same failure shape as an unpaid wage bill, and
/// the same reason: a stalled site is visible and an overdraft nobody agreed to
/// is not.
pub fn contracting(world: &World) -> Vec<Mutation> {
    let mut out = Vec::new();
    let mut purse: Vec<(Market, f64)> = Market::ALL
        .iter()
        .map(|&m| (m, world.treasury.of(m)))
        .collect();

    // Commissioning order, which for a building is its id.
    let mut sites: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| !b.is_built() && b.contractor.is_some())
        .map(|b| {
            (
                b.id,
                b.contractor.expect("filtered"),
                b.def().labour - b.work_done,
            )
        })
        .collect();
    sites.sort_by_key(|&(id, _, _)| id);

    for (site, market, remaining) in sites {
        let Some(slot) = purse.iter_mut().find(|(m, _)| *m == market) else {
            continue;
        };
        let wanted = CONTRACTOR_DAYS.min(remaining.max(0.0));
        if wanted <= 0.0 {
            continue;
        }
        let owed = wanted * CONTRACTOR_RATE;
        // What the purse can actually cover, which decides how much of the day
        // the firm works rather than putting the republic into debt for it.
        let paid = owed.min(slot.1.max(0.0));
        if paid <= 0.0 {
            continue;
        }
        slot.1 -= paid;
        out.push(Mutation::Contracted {
            site,
            market,
            builder_days: wanted * (paid / owed),
            paid,
        });
    }
    out
}

pub fn construction(world: &World) -> Vec<Mutation> {
    let day = tick_days();

    // Buildings and roads, ranked together. A building's id *is* its place in
    // the commissioning order; a road site carries the count as it stood when
    // it was ordered, and ties go to the building because the building holding
    // that number was placed first. Ordering a road therefore takes its turn in
    // the queue like anything else, rather than jumping it or waiting behind
    // every factory in the republic.
    let mut out = Vec::new();
    // What each office's machinery bin holds as this pass sees it. The same
    // scratch-ledger discipline the households and trade passes use: two gangs
    // out of one office must not both be told they had the last of the plant.
    let mut plant: BTreeMap<BuildingId, Tonnes> = BTreeMap::new();

    for (_, site, wanted) in sites_in_order(world) {
        let Some(party) = world.crews.working_at(site) else {
            continue;
        };
        let Some(office) = world.buildings.get(party.office) else {
            continue;
        };

        // A dry machinery bin is a soft penalty and never a stall — the rule
        // `WORN_EFFICIENCY` already states for every other building, applied
        // here to the plant a crew works with. A republic that runs out of
        // machinery builds at half speed; it does not stop building, because
        // limping is recoverable and stopping is not.
        let held = *plant
            .entry(office.id)
            .or_insert_with(|| office.stock.get(Resource::Machinery));
        let worn = if held.is_positive() {
            1.0
        } else {
            WORN_EFFICIENCY
        };

        let days = (f64::from(party.heads) * day * worn).min(wanted);
        if days <= 0.0 {
            continue;
        }
        let plant_used = Tonnes(days * MACHINERY_PER_BUILDER_DAY).min(held);
        if plant_used.is_positive() {
            *plant.entry(office.id).or_default() = held.saturating_sub(plant_used);
            out.push(Mutation::Consume {
                building: office.id,
                resource: Resource::Machinery,
                tonnes: plant_used,
            });
        }
        out.push(match site {
            Destination::Building(id) => Mutation::Build {
                site: id,
                builder_days: days,
            },
            Destination::RoadSite(id) => Mutation::Lay {
                site: id,
                builder_days: days,
            },
            Destination::LineSite(id) => Mutation::String {
                site: id,
                builder_days: days,
            },
        });
    }
    out
}

/// Every site with its materials on hand, in commissioning order, with the work
/// each has left.
///
/// Shared by [`construction`] and [`crews`] because they must agree on the
/// queue: a crew posted to the third site while the first is worked would make
/// the commissioning order a thing the player could see and not rely on.
fn sites_in_order(world: &World) -> Vec<(u64, Destination, f64)> {
    let mut sites: Vec<(u64, Destination, f64)> = Vec::new();
    for b in world.buildings.all() {
        if b.is_built() || !b.has_materials() {
            continue;
        }
        // A site somebody else is being paid to build is not a site your crews
        // work, and it does not want your gravel either. You bought the labour
        // and the materials together, which is what makes a contractor cost
        // several times a builder-day and what makes it worth stopping.
        //
        // Without this a contracted site is worked twice over: billed to a
        // foreign firm AND staffed from your own offices. Caught by the
        // both-directions write-set guard, which found `contracting` never
        // emitting because the ordinary crews finished every contract before
        // the day boundary it runs on.
        if b.contractor.is_some() {
            continue;
        }
        let remaining = (b.def().labour - b.work_done).max(0.0);
        sites.push((u64::from(b.id.0), Destination::Building(b.id), remaining));
    }
    for road in world.roadworks.all() {
        if !road.has_materials() {
            continue;
        }
        let remaining = (road.labour() - road.work_done).max(0.0);
        sites.push((road.ordered, Destination::RoadSite(road.id), remaining));
    }
    for line in world.lineworks.all() {
        if !line.has_materials() {
            continue;
        }
        let remaining = (line.labour() - line.work_done).max(0.0);
        sites.push((line.ordered, Destination::LineSite(line.id), remaining));
    }
    sites.sort_by(|(oa, da, _), (ob, db, _)| oa.cmp(ob).then_with(|| da.cmp(db)));
    sites
}

/// Getting the builders to the work, and back again.
///
/// The physical half of construction, and the reason a remote site is expensive:
/// **an office's crews commute to it on the office's own buses**, over the
/// republic's own roads, burning the republic's own diesel. Noah's rule, in his
/// words: *"The office employs them and they commute office→site. No local
/// crew."*
///
/// Two things happen here, in this order and deliberately:
///
/// 1. **Collections first.** A gang standing beside a building it has just
///    finished is a gang the office cannot post anywhere — its heads are still
///    counted against the establishment. Fetching them back is what frees them,
///    so it outranks sending anybody new out.
/// 2. **Postings**, in the same commissioning order the construction queue works
///    to, so the site that would be built first is the site that gets a crew.
///
/// A bus that cannot make the round trip does not set out, which is the fleet's
/// rule applied unchanged: the alternative is a gang stranded in a field with no
/// bus, and unlike a load of gravel a stranded gang is people the republic has
/// lost the use of.
pub fn crews(world: &World) -> Vec<Mutation> {
    let mut buses = available(world, Role::Crew);
    if buses.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    // Heads posted and fuel drawn *by this pass*, on top of what the world
    // already shows. Two buses leaving one office in the same tick must not
    // both be told the same ten people are free to go.
    let mut going: BTreeMap<BuildingId, u32> = BTreeMap::new();
    let mut drawn: BTreeMap<BuildingId, Tonnes> = BTreeMap::new();

    // Gangs a bus is already on its way to. **A crew waiting for a lift stays
    // waiting until the bus actually arrives**, so without this every tick sends
    // another bus after the same ten people — measured, and it cost a republic
    // one of its two buses permanently: the second arrived to find nobody there
    // and sat in the field holding a job it could never finish. The same shape
    // as freight's `Booked`, and needed for the same reason.
    let coming: Vec<PartyId> = world
        .fleet
        .all()
        .iter()
        .filter_map(|v| v.job.and_then(Job::party))
        .collect();
    // And sites a gang is already on its way to. **A crew riding toward a site
    // is not yet working it**, so without this the second bus leaves for the
    // same foundation on the same tick — measured, and the trajectory runner
    // reported it plainly: twenty builders out for one road that can absorb
    // ten. Exactly the shape of the collection bug above, which is the argument
    // for both being written down here: a journey is a commitment, and a
    // dispatcher that only reads arrivals will make it twice.
    let booked: Vec<Destination> = world
        .fleet
        .all()
        .iter()
        .filter_map(|v| v.job.and_then(Job::ferry))
        .map(|(to, _)| to)
        .collect();
    let stranded: Vec<(PartyId, BuildingId, Point)> = world
        .crews
        .stranded()
        .filter(|p| !coming.contains(&p.id))
        .map(|p| (p.id, p.office, p.at))
        .collect();
    for (party, office, at) in stranded {
        if buses.is_empty() {
            break;
        }
        // Only its own office fetches a gang: the heads go back into *that*
        // establishment, and a bus from elsewhere would deliver them to a yard
        // that never lent them.
        send_bus(
            world,
            at,
            |v| (v.home == office).then_some(Job::Collect { party }),
            &mut buses,
            &mut drawn,
            &mut out,
        );
    }

    // How many an office could still send, counting what it has already sent on
    // earlier ticks and earlier in this one.
    let heads = |v: &crate::fleet::Vehicle, going: &BTreeMap<BuildingId, u32>| -> u32 {
        world
            .buildings
            .get(v.home)
            .map(|office| {
                // Its own staff plus whatever foreign labour it has taken on.
                // A hired builder is a builder — what stays different is that
                // the republic pays them daily in hard currency.
                (office.staff + world.crews.hired_total(office.id))
                    .saturating_sub(world.crews.posted(office.id))
                    .saturating_sub(going.get(&office.id).copied().unwrap_or(0))
                    .min(v.def().seats)
                    .min(BUILDERS_PER_SITE)
            })
            .unwrap_or(0)
    };

    for (_, site, _) in sites_in_order(world) {
        if buses.is_empty() {
            break;
        }
        // **Stop as soon as no bus has anybody to carry.** Without this a
        // republic with more sites than gangs — which is every republic in the
        // middle of a build-out — plans a cross-country route for every idle
        // bus against every unmanned site, on every tick, and throws all of it
        // away. That is the same shape that once took a test fixture from two
        // seconds to over five minutes, and it is why a check that costs
        // nothing goes in front of one that costs an A*.
        if !buses
            .iter()
            .filter_map(|&id| world.fleet.get(id))
            .any(|v| heads(v, &going) > 0)
        {
            break;
        }
        if world.crews.working_at(site).is_some() || booked.contains(&site) {
            continue;
        }
        let Some(at) = world.place_of(site) else {
            continue;
        };
        // How many go is a property of the bus that takes it — its seats, its
        // office's spare people, and the cap on what one site can absorb — so
        // the job is built by whichever bus is chosen rather than guessed
        // before one is.
        let Some(sent) = send_bus(
            world,
            at,
            |v| {
                let taking = heads(v, &going);
                (taking > 0).then_some(Job::Ferry {
                    to: site,
                    heads: taking,
                })
            },
            &mut buses,
            &mut drawn,
            &mut out,
        ) else {
            continue;
        };
        if let Some(v) = world.fleet.get(sent) {
            let taken = heads(v, &going);
            *going.entry(v.home).or_default() += taken;
        }
    }
    out
}

/// Send the nearest idle bus that `work` will give a job to and that can make
/// the round trip.
///
/// Returns the vehicle that took it. The round-trip check is the fleet's own
/// rule and it is not relaxed here: a bus that runs dry on the way back has
/// stranded people rather than a pallet.
///
/// `work` both decides suitability and builds the job, because for a ferry
/// those are one question — how many go depends on which bus goes.
fn send_bus(
    world: &World,
    target: Point,
    work: impl Fn(&crate::fleet::Vehicle) -> Option<Job>,
    buses: &mut Vec<VehicleId>,
    drawn: &mut BTreeMap<BuildingId, Tonnes>,
    out: &mut Vec<Mutation>,
) -> Option<VehicleId> {
    let mut nearest: Vec<(f64, usize)> = buses
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let v = world.fleet.get(*id)?;
            work(v).map(|_| (v.at.distance_to(target).0, i))
        })
        .collect();
    nearest.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

    let crossing = world.crossing();
    let now = world.clock.ticks() as f64;
    let day = world.clock.day_index();
    for (_, index) in nearest {
        let id = buses[index];
        let (Some(v), Some(yard)) = (
            world.fleet.get(id),
            world
                .fleet
                .get(id)
                .and_then(|v| world.buildings.get(v.home))
                .map(|b| b.centre),
        ) else {
            continue;
        };
        let Some(job) = work(v) else {
            continue;
        };
        let def = v.def();
        let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
        let (Some(outbound), Some(home_run)) = (leg(v.at, target), leg(target, yard)) else {
            continue;
        };
        let round_trip = outbound.distance() + home_run.distance();
        let held = world
            .buildings
            .get(v.home)
            .map(|b| b.stock.get(Resource::Fuel))
            .unwrap_or(Tonnes::ZERO)
            .saturating_sub(drawn.get(&v.home).copied().unwrap_or(Tonnes::ZERO));
        let top_up = def.tank.saturating_sub(v.fuel).min(held);
        if (v.fuel + top_up).0 < v.fuel_for(round_trip).0 {
            continue;
        }

        let stuck = sticks(world, &crossing, id, v.capability(), &outbound, 0, day);
        out.push(Mutation::Dispatch {
            vehicle: id,
            job,
            journey: outbound,
            refuel: top_up,
        });
        if stuck {
            out.push(Mutation::Bog { vehicle: id, day });
        }
        *drawn.entry(v.home).or_default() += top_up;
        buses.remove(index);
        return Some(id);
    }
    None
}

/// What one citizen eats in a day. Ported from the archived balance.
pub const FOOD_PER_CITIZEN: f64 = 0.015;

/// And wears out in a day.
pub const CLOTHES_PER_CITIZEN: f64 = 0.004;

/// What one citizen drinks in a day.
///
/// Well under half what they wear out and a fraction of what they eat, which is
/// the scale a comfort should sit at: a distillery supplies a town rather than
/// an estate, and the tonnage is small enough that the same crop going to a
/// food factory is nearly always the better call — which is what makes sending
/// it to the still a decision.
pub const ALCOHOL_PER_CITIZEN: f64 = 0.0015;

/// And wants in household electrics.
///
/// Smaller again, because these are **durables**. A radio, a television, a
/// refrigerator: bought once and kept for years, so what a republic has to
/// supply is a trickle rather than a ration. It is also why one Electronics
/// Combine can comfortably keep a whole republic in televisions while the same
/// output is worth ninety-five dollars a tonne at a Western post — the tension
/// this good exists for.
pub const ELECTRONICS_PER_CITIZEN: f64 = 0.0006;

/// How far people will go to a shop.
///
/// Shorter than the commute they will accept for work, deliberately: you walk
/// further for a job than for a loaf.
pub const SERVICE_RADIUS: Metres = Metres(800.0);

/// Households: what people take off the shelves.
///
/// Citizens do not consume from thin air. They draw from a State Store within
/// reach of where they live, which is what makes siting shops a real decision
/// and what makes a distant housing estate a supply problem rather than a
/// cosmetic one.
///
/// The allocation runs against a **scratch ledger** of what each store has
/// left, decremented as it serves. Two estates sharing one shop must not both
/// be told they were fed from the same tonne — that is the same reasoning the
/// archived build used its staged mutations for, and getting it wrong would
/// report a well-fed republic that is quietly starving.
pub fn households(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let mut out = Vec::new();

    // What each store has, as this pass sees it.
    let mut shelves: BTreeMap<BuildingId, Stock> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && !b.def().sells.is_empty())
        .map(|b| (b.id, b.stock))
        .collect();

    let mut homes: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().residents > 0)
        .collect();
    homes.sort_by_key(|b| b.id);

    // Counted once for the whole pass, not once per home. This used to call
    // `residents_of` inside the loop, which rebuilt and sorted the entire
    // population every time — at 4,000 citizens that was most of the cost of a
    // simulated day, and the baselines are what found it.
    let residents_by_home = world.population.residents_by_home();

    for home in homes {
        let residents = residents_by_home.get(&home.id).copied().unwrap_or(0) as usize;
        if residents == 0 {
            continue;
        }

        // Shops in reach, nearest first.
        let mut reachable: Vec<(f64, BuildingId)> = shelves
            .keys()
            .copied()
            .filter_map(|id| {
                let shop = world.buildings.get(id)?;
                let distance = shop.centre.distance_to(home.centre).0;
                (distance <= SERVICE_RADIUS.0).then_some((distance, id))
            })
            .collect();
        // Nearest shop first, ties on id so the answer is reproducible.
        reachable.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));
        let reachable: Vec<BuildingId> = reachable.into_iter().map(|(_, id)| id).collect();

        let mut met = 0.0;
        let mut wanted = 0.0;
        // The comforts are tallied **separately and per good**, not folded into
        // the tonnage above. Two reasons, and both matter: what they are worth
        // is a lift rather than a component, so mixing the tonnage in would make
        // a missing television read as a missing meal; and drink alone carries a
        // health cost, so how much of *it* reached the shelves has to survive
        // this loop.
        let mut comfort_shares: Vec<f64> = Vec::new();
        let mut drink = 0.0;

        for (resource, per_head) in [
            (Resource::Food, FOOD_PER_CITIZEN),
            (Resource::Clothes, CLOTHES_PER_CITIZEN),
            (Resource::Alcohol, ALCOHOL_PER_CITIZEN),
            (Resource::Electronics, ELECTRONICS_PER_CITIZEN),
        ] {
            let need = Tonnes(residents as f64 * per_head * day);
            let mut got = Tonnes::ZERO;
            let mut outstanding = need;

            for shop in &reachable {
                if !outstanding.is_positive() {
                    break;
                }
                let Some(stock) = shelves.get_mut(shop) else {
                    continue;
                };
                let taken = stock.take(resource, outstanding);
                if taken.is_positive() {
                    outstanding = outstanding.saturating_sub(taken);
                    got += taken;
                    out.push(Mutation::Consume {
                        building: *shop,
                        resource,
                        tonnes: taken,
                    });
                }
            }

            let share = if need.is_positive() {
                (got.0 / need.0).clamp(0.0, 1.0)
            } else {
                1.0
            };
            if resource.is_comfort() {
                // Each comfort counts for as much as the other, rather than by
                // weight. Electronics is a broad category and a household wants
                // a radio about as much as it wants a bottle; weighting by
                // tonnage would have made the whole thing a drink meter.
                comfort_shares.push(share);
                if resource == Resource::Alcohol {
                    drink = share;
                }
            } else {
                wanted += need.0;
                met += got.0;
            }
        }

        let fraction = if wanted > 0.0 {
            (met / wanted).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let comforts = if comfort_shares.is_empty() {
            0.0
        } else {
            comfort_shares.iter().sum::<f64>() / comfort_shares.len() as f64
        };
        out.push(Mutation::Provision {
            building: home.id,
            fraction,
            comforts,
            drink,
        });
    }

    out
}

/// What crosses the border.
///
/// Every customs house near the border clears up to its staffed throughput a
/// day, working the player's [`crate::trade::TradePolicy`] **in the order they
/// wrote it**. When throughput or money runs short the first rule is served
/// first, because which trade matters most is the player's judgement and not
/// the simulation's.
///
/// Exports are whatever freight has already staged at the customs house —
/// trade is physical, and a tonne that never got trucked to the border does not
/// get sold. Imports land in the customs house and have to be trucked onward
/// the same way.
pub fn trade(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let mut out = Vec::new();
    let mut purse = world.treasury;
    // What this pass has already booked against each tender. The same
    // scratch-ledger discipline the households system uses, for the same
    // reason: two customs houses clearing in one tick must not both be told
    // they delivered the last twenty tonnes of the same contract.
    let mut claimed: BTreeMap<ContractId, Tonnes> = BTreeMap::new();

    let mut houses: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.kind == BuildingKind::Customs)
        .filter(|b| world.frontier.distance_from(b.centre).0 <= CUSTOMS_RANGE.0)
        .collect();
    houses.sort_by_key(|b| b.id);

    for house in houses {
        let mut clearance = Tonnes(CUSTOMS_THROUGHPUT_PER_DAY * house.activity() * day);
        if !clearance.is_positive() {
            continue;
        }
        // A house clears for the bloc whose frontier post it stands at, and
        // only that bloc. This is what makes the two currencies geographic
        // rather than a dropdown: earning dollars means hauling to a Western
        // post, and if the only Western post is on the far side of the map
        // then that is what a dollar costs.
        let bloc = world.frontier.bloc_near(house.centre);

        for rule in world
            .trade_policy
            .rules
            .iter()
            .filter(|rule| rule.market == bloc)
        {
            if !clearance.is_positive() {
                break;
            }
            match rule.action {
                TradeAction::Sell => {
                    let held = house.stock.get(rule.resource);
                    let mut sold = held.min(clearance);
                    if !sold.is_positive() {
                        continue;
                    }
                    clearance = clearance.saturating_sub(sold);

                    // Live tenders take their share first, at the price locked
                    // when each was offered. Whatever is left goes at spot.
                    for id in world.contracts.claimants(rule.resource, rule.market) {
                        if !sold.is_positive() {
                            break;
                        }
                        let Some(c) = world.contracts.get(id) else {
                            continue;
                        };
                        let booked = claimed.get(&id).copied().unwrap_or(Tonnes::ZERO);
                        let against = sold.min(c.outstanding().saturating_sub(booked));
                        if !against.is_positive() {
                            continue;
                        }
                        let payment = against.0 * c.price_per_tonne;
                        purse.credit(rule.market, payment);
                        *claimed.entry(id).or_default() += against;
                        out.push(Mutation::Export {
                            customs: house.id,
                            resource: rule.resource,
                            tonnes: against,
                            market: rule.market,
                            payment,
                            contract: Some(id),
                        });
                        sold = sold.saturating_sub(against);
                    }

                    if !sold.is_positive() {
                        continue;
                    }
                    let payment = sold.0 * world.contracts.sell_price(rule.market, rule.resource);
                    purse.credit(rule.market, payment);
                    out.push(Mutation::Export {
                        customs: house.id,
                        resource: rule.resource,
                        tonnes: sold,
                        market: rule.market,
                        payment,
                        contract: None,
                    });
                }
                TradeAction::Buy { up_to } => {
                    let shortfall = up_to.saturating_sub(house.stock.get(rule.resource));
                    let wanted = shortfall.min(clearance);
                    if !wanted.is_positive() {
                        continue;
                    }
                    let unit = world.contracts.buy_price(rule.market, rule.resource);
                    let affordable = if unit > 0.0 {
                        Tonnes((purse.of(rule.market) / unit).min(wanted.0))
                    } else {
                        Tonnes::ZERO
                    };
                    if !affordable.is_positive() {
                        continue;
                    }
                    clearance = clearance.saturating_sub(affordable);
                    let cost = purse.debit(rule.market, affordable.0 * unit);
                    out.push(Mutation::Import {
                        customs: house.id,
                        resource: rule.resource,
                        tonnes: affordable,
                        market: rule.market,
                        cost,
                        // A standing trade rule buys for the republic, not for
                        // any one site: it is a stock level the player asked
                        // for, and nobody's account to charge.
                        for_site: None,
                    });
                }
            }
        }

        // Construction imports, on whatever clearance the standing policy left.
        //
        // Below the trade rules on purpose: a rule is a plan the player wrote
        // down and a site's shortfall is a consequence of one, so when a post
        // cannot clear everything in a day the plan goes first. Same reasoning
        // as freight's second pass.
        let Some(post) = world.frontier.nearest_crossing(house.centre, None) else {
            continue;
        };
        if post.at.distance_to(house.centre).0 > CUSTOMS_RANGE.0 {
            continue;
        }
        for (site, resource, short) in importable(world, post.id) {
            if !clearance.is_positive() {
                break;
            }
            let unit = world.contracts.buy_price(bloc, resource);
            if unit <= 0.0 {
                continue;
            }
            let wanted = short.min(clearance);
            let affordable = Tonnes((purse.of(bloc) / unit).min(wanted.0));
            if !affordable.is_positive() {
                continue;
            }
            clearance = clearance.saturating_sub(affordable);
            let cost = purse.debit(bloc, affordable.0 * unit);
            out.push(Mutation::Import {
                customs: house.id,
                resource,
                tonnes: affordable,
                market: bloc,
                cost,
                for_site: Some(site),
            });
            // Nothing is *reserved* for the site: the goods land in the post's
            // yard and the ordinary freight ranking takes them from there, which
            // is what keeps "no instant build" true for a republic that can
            // afford anything. What is bounded is the spending, not the goods.
        }
    }
    out
}

/// What the sites importing through a post still need, netted against what is
/// already standing at that post and already on a lorry heading for them.
///
/// The netting is the whole of the difficulty. A site's shortfall does not fall
/// when the goods are bought — it falls when they are *delivered*, and delivery
/// is hours of driving away. Buying the shortfall every tick until it lands
/// would spend a republic's hard currency several times over for one wall, and
/// the failure would look like a balance problem rather than a bug.
fn importable(
    world: &World,
    post: crate::trade::CrossingId,
) -> Vec<(Destination, Resource, Tonnes)> {
    let mut wanted: BTreeMap<Resource, Tonnes> = BTreeMap::new();
    let mut sites: Vec<(Destination, Resource, Tonnes)> = Vec::new();

    // The shortfall, capped by what the Directorate will still buy on this
    // site's account. The cap is what stops auto-import chasing a shortfall it
    // does not own — see `BuildPolicy::bought_for`.
    let mut consider = |site: Destination, resource: Resource, short: Tonnes, bill: f64| {
        if world.build_policy.crossing_for(site) != Some(post) {
            return;
        }
        let short = Tonnes(
            short
                .0
                .min(world.build_policy.allowance(site, resource, Tonnes(bill)).0),
        );
        if !short.is_positive() {
            return;
        }
        *wanted.entry(resource).or_default() += short;
        sites.push((site, resource, short));
    };

    for b in world.buildings.all() {
        if b.is_built() {
            continue;
        }
        for &(resource, bill) in b.def().materials {
            consider(
                Destination::Building(b.id),
                resource,
                b.material_outstanding(resource),
                bill,
            );
        }
    }
    for road in world.roadworks.all() {
        for (resource, bill) in road.materials() {
            consider(
                Destination::RoadSite(road.id),
                resource,
                road.material_outstanding(resource),
                bill.0,
            );
        }
    }
    if sites.is_empty() {
        return sites;
    }

    // What is already bought and not yet used: standing in the post's yard, or
    // on a lorry that is taking it to one of these sites.
    let mut covered: BTreeMap<Resource, Tonnes> = BTreeMap::new();
    for b in world.buildings.all() {
        if b.kind != BuildingKind::Customs || !b.is_built() {
            continue;
        }
        if world
            .frontier
            .nearest_crossing(b.centre, None)
            .is_none_or(|c| c.id != post)
        {
            continue;
        }
        for &resource in wanted.keys() {
            *covered.entry(resource).or_default() += b.stock.get(resource);
        }
    }
    for v in world.fleet.all() {
        let Some((_, to, resource, tonnes)) = v.job.and_then(Job::haul) else {
            continue;
        };
        if world.build_policy.crossing_for(to) == Some(post) && wanted.contains_key(&resource) {
            // **What it was sent for, not what is on the bed.** A lorry driving
            // out to collect has a job and an empty bed, and counting the bed
            // made that whole leg invisible — the post bought the shortfall
            // again on every tick the lorry was still on its way to fetch it.
            // Measured: 47 t of machinery bought for a 6 t bill.
            let promised = Tonnes(tonnes.0.max(v.cargo.get(resource).0));
            *covered.entry(resource).or_default() += promised;
        }
    }

    // Spread what is covered over the sites in the order they were commissioned,
    // so the site that would be built first is the one already supplied.
    sites.sort_by(|(da, ra, _), (db, rb, _)| da.cmp(db).then_with(|| ra.cmp(rb)));
    sites.retain_mut(|(_, resource, short)| {
        let held = covered.entry(*resource).or_default();
        let taken = (*held).min(*short);
        *held = held.saturating_sub(taken);
        *short = short.saturating_sub(taken);
        short.is_positive()
    });
    sites
}

/// Where people work, and what carrying them there costs.
///
/// One pass, because they are one decision: a job is only a job if there is a
/// way to get to it, and the seats spent getting there are the fuel the depots
/// burn. Splitting them would let the republic staff a factory by bus and then
/// separately discover it had no fuel to run the bus.
pub fn labour(world: &mut World) -> Vec<Mutation> {
    // The ways are read out before the population is borrowed mutably. They
    // are borrows of fields the labour pass never touches, and taking them
    // first is what says so to the compiler.
    let ways = crate::journey::Ways {
        roads: &world.roads,
        rails: &world.rails,
        tramway: &world.tramway,
        metro: &world.metro,
        water: &world.waterways,
        air: &world.airways,
    };
    let result = assign_labour(&mut world.population, &world.buildings, ways);
    let mut out: Vec<Mutation> = result
        .staffing
        .into_iter()
        .map(|(building, count)| Mutation::Staff { building, count })
        .collect();
    for (depot, tonnes) in transport::fuel_burn(&world.buildings, result.seats_used) {
        out.push(Mutation::Consume {
            building: depot,
            resource: Resource::Fuel,
            tonnes,
        });
    }
    out
}

/// Tenders from the two blocs: what is offered, what expires, what fails.
///
/// A **daily** system, and only meaningful at a day boundary — deadlines are
/// day indices and relations decay per day, so running it per tick would fine a
/// republic 1,440 times for one missed contract.
pub fn contracts(world: &World) -> Vec<Mutation> {
    let today = world.clock.day_index();
    let mut out = Vec::new();

    for c in world.contracts.all() {
        match c.state {
            ContractState::Offer if today > c.offer_expires_day => {
                out.push(Mutation::DropContract { contract: c.id });
            }
            ContractState::Active if today > c.deadline_day => {
                out.push(Mutation::CloseContract {
                    contract: c.id,
                    state: ContractState::Failed,
                });
                // The fine can never overdraw: an empty treasury simply pays
                // nothing, the same rule the treasury applies to everything.
                out.push(Mutation::Fine {
                    market: c.market,
                    amount: c.fine(),
                });
                out.push(Mutation::Relations {
                    market: c.market,
                    penalty: world.contracts.penalty(c.market) + contract::RELATIONS_HIT,
                });
            }
            ContractState::Done | ContractState::Failed => {
                // Prune old history so the ledger stays readable.
                if let Some(closed) = c.closed_day
                    && today - closed > 60
                {
                    out.push(Mutation::DropContract { contract: c.id });
                }
            }
            _ => {}
        }
    }

    // Blocs forget, slowly. A failure is a scar, not a permanent state.
    for market in [Market::East, Market::West] {
        let penalty = world.contracts.penalty(market);
        if penalty > 0.0 {
            out.push(Mutation::Relations {
                market,
                penalty: (penalty - contract::RELATIONS_DECAY_PER_DAY).max(0.0),
            });
        }
    }

    if let Some(offer) = tender(world) {
        out.push(Mutation::Offer(offer));
    }
    out
}

/// A bloc's periodic bulk order, or `None` if the conditions for one are not
/// met.
///
/// Drawn from a substream keyed by the month, never from the simulation's own
/// generator: the archived build found that offers drawn from the economy
/// stream meant merely *looking* at what was on offer shifted every later
/// economic roll.
fn tender(world: &World) -> Option<Contract> {
    let date = world.clock.date();
    // Only on the first of the month, and only every other month.
    if date.day != 1 {
        return None;
    }
    let month_index =
        u64::from(date.year - crate::time::EPOCH_YEAR) * 12 + u64::from(date.month - 1);
    if !month_index.is_multiple_of(contract::OFFER_EVERY_MONTHS) {
        return None;
    }
    // A tender needs somewhere to land. No crossing, no trade.
    if !world
        .buildings
        .all()
        .iter()
        .any(|b| b.is_built() && b.kind == BuildingKind::Customs)
    {
        return None;
    }
    if world.contracts.offers().count() >= contract::MAX_OPEN_OFFERS {
        return None;
    }

    let mut rng = world.substream(crate::world::CONTRACT_STREAM, month_index);

    // The blocs ask for what the republic demonstrably makes. Read from what
    // its finished buildings actually produce rather than from an accumulated
    // statistic — the structure is the fact, and it cannot drift out of date.
    let mut pool: Vec<Resource> = Resource::ALL
        .into_iter()
        .filter(|&r| {
            world
                .buildings
                .all()
                .iter()
                .any(|b| b.is_built() && b.def().outputs.iter().any(|&(o, _)| o == r))
        })
        .collect();
    if pool.is_empty() {
        pool = Resource::ALL.to_vec();
    }
    let resource = pool[rng.next_bounded(pool.len() as u64) as usize];
    let market = if rng.next_f64() < 0.5 {
        Market::East
    } else {
        Market::West
    };

    let (low, high) = match market {
        Market::East => contract::VALUE_BAND_EAST,
        Market::West => contract::VALUE_BAND_WEST,
    };
    let value = rng.next_range(low, high);
    let unit = world.contracts.buy_price(market, resource);
    let amount = if unit > 0.0 {
        (value / unit)
            .round()
            .clamp(contract::MIN_TONNES, contract::MAX_TONNES)
    } else {
        contract::MIN_TONNES
    };
    let premium = rng.next_range(contract::PREMIUM.0, contract::PREMIUM.1);
    let (min_days, max_days) = contract::DEADLINE_DAYS;
    let days = min_days + rng.next_bounded(max_days - min_days + 1);

    let today = world.clock.day_index();
    Some(Contract {
        // Ids come from the month, not from a counter the systems would have to
        // mutate: a system proposes and never writes, so it cannot reserve one.
        id: ContractId(month_index as u32 + 1),
        resource,
        market,
        amount: Tonnes(amount),
        delivered: Tonnes::ZERO,
        price_per_tonne: unit * (1.0 + premium),
        deadline_day: today + days,
        offer_expires_day: today + contract::OFFER_DAYS,
        state: ContractState::Offer,
        closed_day: None,
    })
}

/// Digging, and making.
///
/// Efficiency is the product of separate limiters — staffing, power, input
/// availability — each computed on its own so a stalled building can always be
/// asked *which* one stopped it. They are deliberately not folded together.
/// What a building with a dry machinery bin runs at.
///
/// Ported verbatim from the archived `BALANCE.wornEffMult`. **A soft penalty
/// and never a stall** — a republic that runs out of machinery limps, which is
/// recoverable, rather than stopping dead, which is not.
pub const WORN_EFFICIENCY: f64 = 0.5;

/// Below this air temperature nothing grows, whatever the ground is doing.
pub const GROWING_MIN_C: f64 = 5.0;
/// At and above this, warmth has stopped being the limiting factor.
pub const GROWING_WARM_C: f64 = 18.0;
/// Root-zone water below which crops are withering for want of it.
pub const DROUGHT_BELOW: f64 = 0.35;
/// What a farm still yields with the root zone completely exhausted.
///
/// **Not zero, and that is deliberate.** The Southern Steppe sits at a median
/// of 0.029 through its growing season — measured — so a drought curve running
/// to nothing would mean the steppe could never feed itself at all. The goal
/// says a wall has to be a design consequence rather than a balance hole, and
/// "this posting is agriculturally impossible" is the second. Dry farming is
/// poor, not futile.
pub const DROUGHT_FLOOR: f64 = 0.25;
/// Root-zone water at and above which the ground is wetter than the crop needs.
///
/// Set from the measured distributions so this is a genuinely wet spell rather
/// than the normal state: growing-season medians are 0.670 plains, 0.911 taiga,
/// 0.722 maritime.
pub const WATERED_AT: f64 = 0.85;
/// What a well-watered spell is worth. The archived build's `rain.farmMult`.
pub const WATERED_YIELD: f64 = 1.15;

/// How well crops are growing today, as a multiplier on a farm's output.
///
/// **Ported from the archived rule — rain feeds them, frost stops them,
/// drought withers them — but expressed against state rather than against a
/// weather word.** v1 read a discrete weather enum and a `droughtAfterDays`
/// counter; this build already carries soil moisture and frost continuously in
/// [`crate::ground::Ground`], so the same three behaviours fall out of asking
/// the ground what it is like.
///
/// **Nothing here mentions a season, and that is the point** — it is the same
/// discipline `ground.rs` follows to produce the spring thaw without a
/// calendar. Winter yields nothing because it is cold and the ground is frozen,
/// not because a month index says so, which means a mild winter and a cold
/// snap in May both do what they should without a special case.
///
/// Engine-owned and public so a UI can show the player *why* the harvest is
/// poor rather than presenting a bare number.
pub fn growing_conditions(world: &World) -> f64 {
    // Frozen ground: roots cannot work it, however warm the air briefly gets.
    let thawed = (1.0 - world.ground.frost).clamp(0.0, 1.0);

    // Warmth, ramping rather than switching, so a cold spring is a bad harvest
    // rather than no harvest.
    let warmth =
        ((world.temperature() - GROWING_MIN_C) / (GROWING_WARM_C - GROWING_MIN_C)).clamp(0.0, 1.0);

    // Water: withering below the drought line, full between, and a real bonus
    // when the ground is properly wet. Reads the **root zone**, not the
    // topsoil — see [`crate::ground::ROOT_DRYING_PER_DAY`] for the measurement
    // that forced the distinction.
    let m = world.ground.water;
    let water = if m < DROUGHT_BELOW {
        DROUGHT_FLOOR + (1.0 - DROUGHT_FLOOR) * (m / DROUGHT_BELOW).clamp(0.0, 1.0)
    } else if m < WATERED_AT {
        1.0
    } else {
        WATERED_YIELD
    };

    (thawed * warmth * water).max(0.0)
}

pub fn production(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let mut out = Vec::new();

    for b in world.buildings.all() {
        if !b.is_built() {
            continue; // a site produces nothing
        }
        let def = b.def();
        // Boilers, bus depots and garages burn their inputs elsewhere — the
        // heating system, the labour pass and the fleet respectively — because
        // all three throttle to demand. See
        // [`crate::building::BuildingDef::burns_its_own_inputs`], which is where
        // that property is stated rather than listed.
        if def.burns_its_own_inputs() {
            continue;
        }
        let mut efficiency = b.activity();
        if def.power_draw > 0.0 && !b.powered {
            // Unpowered work stops. The archived build had a per-building
            // brownout fraction; that authored property is worth restoring,
            // but stalling is the honest default until it is authored.
            efficiency = 0.0;
        }
        if efficiency <= 0.0 {
            continue;
        }

        // A farm answers to the ground and the air, not to the calendar — and
        // "the air" is now literal. Smoke on the fields costs yield, which is
        // what makes siting a steel works upwind of the collective farm a
        // decision with a price rather than a matter of taste.
        //
        // Applied here rather than folded into `growing_conditions`, because
        // that function is about the *weather* and is the same everywhere,
        // while pollution is a property of where this particular farm stands.
        if def.farms {
            let dirt = world.lattice.pollution_near(b.centre);
            efficiency *= growing_conditions(world) * (1.0 - dirt * SMOKE_YIELD_COST);
            if efficiency <= 0.0 {
                continue;
            }
        }

        // Worn machinery is a soft penalty, never a stall — the archived rule.
        // Deliberately *not* folded into `input_factor` below: an input a
        // building is short of throttles it toward zero, and machinery must
        // not, or a republic that runs out stops instead of limping.
        if def.wear > 0.0 && !b.stock.get(Resource::Machinery).is_positive() {
            efficiency *= WORN_EFFICIENCY;
        }

        // How much of what it wants is actually in the bins.
        let mut input_factor: f64 = 1.0;
        for &(resource, rate) in def.inputs {
            let wanted = Tonnes(rate * day * efficiency);
            if wanted.is_positive() {
                let have = b.stock.get(resource);
                input_factor = input_factor.min((have.0 / wanted.0).clamp(0.0, 1.0));
            }
        }
        efficiency *= input_factor;
        if efficiency <= 0.0 {
            continue;
        }

        for &(resource, rate) in def.inputs {
            out.push(Mutation::Consume {
                building: b.id,
                resource,
                tonnes: Tonnes(rate * day * efficiency),
            });
        }

        // Machines wear in proportion to how hard they are worked. `efficiency`
        // already carries staffing, power, growing conditions and input
        // shortfall, so an idle building wears nothing without a special case.
        if def.wear > 0.0 {
            let worn = Tonnes(def.wear * day * efficiency).min(b.stock.get(Resource::Machinery));
            if worn.is_positive() {
                out.push(Mutation::Consume {
                    building: b.id,
                    resource: Resource::Machinery,
                    tonnes: worn,
                });
            }
        }

        match (def.taps, b.tapped) {
            // An extractor pulls from the body it tapped.
            (Some(_), Some(deposit)) => {
                for &(resource, rate) in def.outputs {
                    out.push(Mutation::Extract {
                        deposit,
                        building: b.id,
                        resource,
                        tonnes: Tonnes(rate * day * efficiency),
                    });
                }
            }
            _ => {
                for &(resource, rate) in def.outputs {
                    out.push(Mutation::Produce {
                        building: b.id,
                        resource,
                        tonnes: Tonnes(rate * day * efficiency),
                    });
                }
            }
        }
    }
    out
}

/// How many days a building can keep going on what it has.
///
/// **Drain is what the building WANTS, not what it is getting.** This is the
/// trap the archived build documented at length: actual flow throttles to zero
/// when a bin runs dry, so urgency-from-flow means a starved building reports
/// needing nothing and is never resupplied. Measuring intent instead is what
/// makes the outage recoverable.
pub fn cover_days(world: &World, building: BuildingId, resource: Resource) -> Option<f64> {
    let b = world.buildings.get(building)?;
    let rate = b
        .def()
        .inputs
        .iter()
        .find(|(r, _)| *r == resource)
        .map(|&(_, rate)| rate)?;
    if rate <= 0.0 {
        return None;
    }
    Some(b.stock.get(resource).0 / rate)
}

/// Days of cover below which a building is worth a delivery.
pub const RESUPPLY_AT_DAYS: f64 = 3.0;

/// The least a lorry will roll for.
///
/// **Found by measurement, and it is the first thing a physical fleet needed
/// that a scalar did not.** With freight as a number, serving a demand the
/// instant it appeared was free. With lorries it is not: a farm producing six
/// tonnes a day holds four *kilograms* a tick after the last collection, and
/// dispatch cheerfully sent a lorry for it — every tick, with every lorry, for
/// ever. Five simulated days of that burnt 3.2 t of diesel, drove 2,000 km, and
/// starved a building site of the bricks standing in a depot nobody could spare
/// a vehicle for.
///
/// The fix is the rule a real dispatcher works to: let it accumulate, then send
/// one lorry. Two exceptions, both of which would otherwise deadlock:
///
/// - a **site** is served whatever the quantity, because its bill of materials
///   is finite and one-off. A building that needs a single tonne of machinery
///   to open would otherwise never open.
/// - a **full yard** is cleared whatever the quantity, because a producer whose
///   bin is full has stopped producing.
pub const MIN_LOAD: Tonnes = Tonnes(2.0);

/// Plan one leg for a vehicle, over whatever it is allowed to ride.
///
/// `None` where a confined vehicle cannot reach both ends — and **every caller
/// treats that as "this vehicle cannot do this job" and moves on to the next
/// one**, which is the same thing they already did when a lorry could not carry
/// enough fuel. That is why rails, water and air needed no rule in the
/// dispatcher: the refusal was already a shape it understood.
fn plan_leg(
    world: &World,
    crossing: &Crossing<'_>,
    def: &crate::fleet::VehicleDef,
    a: Point,
    b: Point,
    now: f64,
) -> Option<Journey> {
    journey::plan_for(
        def.medium,
        a,
        b,
        world.ways(),
        crossing,
        def.on_road,
        def.cross_country,
        now,
    )
}

/// Send the republic's lorries where their absence would cost the most.
///
/// This is the archived build's freight ranking, kept almost unchanged, with
/// the thing it produces swapped out from under it. The ranking was the
/// hard-won part and it survives:
///
/// - urgency is **downtime averted**, not emptiness. A bin that was never going
///   to run dry averts nothing and scores nothing however empty it looks.
/// - drain is [`cover_days`], which measures intent rather than flow.
/// - two passes, and the second is the safety valve: everything that prevents
///   no downtime — topping up, stocking a shop nobody uses yet — runs on
///   whatever capacity the first pass left. Scarcity is the only regime in
///   which ranking bites at all.
///
/// What changed is the output. A ranked demand used to become a `Deliver`,
/// which moved tonnage from one bin to another in the same instant however far
/// apart they stood. It now becomes a [`Job`] handed to an idle lorry, which
/// has to drive there. The budget changed with it: freight capacity is no
/// longer a scalar but **the vehicles that have drivers today**, so a republic
/// that wants to move more builds another depot and staffs it.
pub fn dispatch(world: &World) -> Vec<Mutation> {
    let mut idle = available(world, Role::Freight);
    let mut tows = available(world, Role::Recovery);
    if idle.is_empty() && tows.is_empty() {
        return Vec::new();
    }
    let mut booked = Booked::from_fleet(world);
    let mut out = Vec::new();

    // Recovery first. A stuck lorry is a job that has stopped with a load on
    // it, and every tick it stays stuck is a tick that load is not arriving —
    // which outranks any fresh haul, however urgent.
    // **The driver tries first.** A tow is not sent to a lorry that stuck an
    // hour ago: the ground is a daily quantity, so a vehicle gets its own
    // chance to drive out when the day turns before the republic spends a
    // recovery vehicle on it. That ordering is what makes self-release the
    // first resort and a tow the answer when it fails.
    let today = world.clock.day_index();
    let casualties: Vec<VehicleId> = world
        .fleet
        .all()
        .iter()
        .filter(|v| match v.state {
            VehicleState::Bogged { since_day, .. } => today >= since_day + crate::fleet::HELP_AFTER,
            _ => false,
        })
        .filter(|v| !booked.rescuing.contains(&v.id))
        .map(|v| v.id)
        .collect();
    for casualty in casualties {
        if tows.is_empty() {
            break;
        }
        send_help(world, casualty, &mut tows, &mut booked, &mut out);
    }
    if idle.is_empty() {
        return out;
    }

    // Pass one: needs that prevent real downtime, worst cover first.
    let mut urgent: Vec<(f64, Destination, Resource)> = Vec::new();
    for b in world.buildings.all() {
        for &(resource, _) in b.def().inputs {
            if let Some(days) = cover_days(world, b.id, resource)
                && days < RESUPPLY_AT_DAYS
            {
                urgent.push((days, Destination::Building(b.id), resource));
            }
        }
    }
    urgent.sort_by(|(da, ia, ra), (db, ib, rb)| {
        da.total_cmp(db)
            .then_with(|| ia.cmp(ib))
            .then_with(|| ra.cmp(rb))
    });

    for (_, destination, resource) in urgent {
        if idle.is_empty() {
            break;
        }
        serve(
            world,
            destination,
            resource,
            &mut idle,
            &mut booked,
            &mut out,
        );
    }

    // Shops, ranked emptiest first. This sits in the urgent pass because a
    // shop running dry is a republic that stops eating, which outranks any
    // factory stalling.
    let mut shops: Vec<(f64, Destination, Resource)> = Vec::new();
    for b in world.buildings.all() {
        if !b.is_built() {
            continue;
        }
        for &resource in b.def().sells {
            let fill = if b.storage_cap().0 > 0.0 {
                b.stock.get(resource).0 / b.storage_cap().0
            } else {
                1.0
            };
            if fill < 1.0 {
                shops.push((fill, Destination::Building(b.id), resource));
            }
        }
    }
    shops.sort_by(|(fa, ia, ra), (fb, ib, rb)| {
        fa.total_cmp(fb)
            .then_with(|| ia.cmp(ib))
            .then_with(|| ra.cmp(rb))
    });
    for (_, destination, resource) in shops {
        if idle.is_empty() {
            break;
        }
        serve(
            world,
            destination,
            resource,
            &mut idle,
            &mut booked,
            &mut out,
        );
    }

    // Pass one and a half: sites waiting on materials. A site with nothing
    // arriving is a crew standing idle, so this sits above comfortable
    // top-ups but below a running building about to stall.
    let mut sites: Vec<(Destination, Resource)> = Vec::new();
    for b in world.buildings.all() {
        if b.is_built() {
            continue;
        }
        for &(resource, _) in b.def().materials {
            if b.material_outstanding(resource).is_positive() {
                sites.push((Destination::Building(b.id), resource));
            }
        }
    }
    // A road under construction wants its gravel driven out to it like any
    // other site, and this is the pass where that happens.
    for road in world.roadworks.all() {
        for (resource, _) in road.materials() {
            if road.material_outstanding(resource).is_positive() {
                sites.push((Destination::RoadSite(road.id), resource));
            }
        }
    }
    sites.sort();
    for (destination, resource) in sites {
        if idle.is_empty() {
            break;
        }
        serve(
            world,
            destination,
            resource,
            &mut idle,
            &mut booked,
            &mut out,
        );
    }

    // Standing orders: what the player told a terminal or a distribution
    // office to keep on hand.
    //
    // **This is what makes a station a place goods go.** A station consumes
    // nothing and sells nothing, so without an order the ranking has no reason
    // to look at it at all — and a terminal nothing delivers to is an expensive
    // shed. Given one it is an ordinary destination, and everything else falls
    // out of machinery that already existed: lorries or trains bring the
    // tonnage in because it is a demand, and whatever needs it nearby draws on
    // the yard because a station holds goods it does not consume, which is the
    // definition of a supplier.
    //
    // It sits **below** the urgent passes and above the comfortable ones. A
    // stockpile is a plan rather than a need: a factory about to stall
    // outranks it, and a top-up somewhere already comfortable does not.
    let mut stores: Vec<(f64, Destination, Resource)> = Vec::new();
    for b in world.buildings.all() {
        if !b.is_built() || !b.def().stores_to_order {
            continue;
        }
        for resource in Resource::ALL {
            let wanted = b.orders.get(resource);
            if !wanted.is_positive() {
                continue;
            }
            let fill = (b.stock.get(resource).0 / wanted.0).clamp(0.0, 1.0);
            if fill < 1.0 {
                stores.push((fill, Destination::Building(b.id), resource));
            }
        }
    }
    stores.sort_by(|(fa, ia, ra), (fb, ib, rb)| {
        fa.total_cmp(fb)
            .then_with(|| ia.cmp(ib))
            .then_with(|| ra.cmp(rb))
    });
    for (_, destination, resource) in stores {
        if idle.is_empty() {
            break;
        }
        serve(
            world,
            destination,
            resource,
            &mut idle,
            &mut booked,
            &mut out,
        );
    }

    // Export staging: getting goods to the border prevents no downtime at all,
    // so it runs on whatever the urgent passes left. That is the archived
    // build's rule — housekeeping never preempts a real need.
    let sells: Vec<Resource> = world
        .trade_policy
        .rules
        .iter()
        .filter(|r| matches!(r.action, TradeAction::Sell))
        .map(|r| r.resource)
        .collect();
    if !sells.is_empty() {
        let mut houses: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.kind == BuildingKind::Customs)
            .filter(|b| world.frontier.distance_from(b.centre).0 <= CUSTOMS_RANGE.0)
            .map(|b| b.id)
            .collect();
        houses.sort();
        for house in houses {
            for &resource in &sells {
                if idle.is_empty() {
                    break;
                }
                serve(
                    world,
                    Destination::Building(house),
                    resource,
                    &mut idle,
                    &mut booked,
                    &mut out,
                );
            }
        }
    }

    // Pass two: comfortable top-ups, on what the first pass left.
    let mut comfortable: Vec<(Destination, Resource)> = Vec::new();
    for b in world.buildings.all() {
        for &(resource, _) in b.def().inputs {
            if let Some(days) = cover_days(world, b.id, resource)
                && days >= RESUPPLY_AT_DAYS
            {
                comfortable.push((Destination::Building(b.id), resource));
            }
        }
    }
    for (destination, resource) in comfortable {
        if idle.is_empty() {
            break;
        }
        serve(
            world,
            destination,
            resource,
            &mut idle,
            &mut booked,
            &mut out,
        );
    }

    out
}

/// Which vehicles could set out this tick, in commissioning order.
///
/// A garage may have no more vehicles in motion at once than it has drivers for
/// — [`crewed`] — so a shift that goes unstaffed shrinks what can leave the
/// yard. It does **not** recall the lorries already out: a republic short of
/// people does not abandon its vehicles in a field.
fn available(world: &World, role: Role) -> Vec<VehicleId> {
    let mut out = Vec::new();
    // `Buildings::all` is in commissioning order, which is id order, which is
    // what makes the answer reproducible.
    for garage in world.buildings.all() {
        if garage.def().vehicles.is_empty() {
            continue;
        }
        let running = world
            .fleet
            .of_garage(garage.id)
            .filter(|v| !v.is_idle())
            .count() as u32;
        let mut slots = crewed(garage).saturating_sub(running);
        for v in world.fleet.of_garage(garage.id) {
            if slots == 0 {
                break;
            }
            if !v.is_idle() {
                continue;
            }
            // A recovery vehicle does not haul, a lorry does not tow and a bus
            // carries neither, so the three pools never compete for the same
            // driver-slot twice.
            if v.def().role != role {
                continue;
            }
            out.push(v.id);
            slots -= 1;
        }
    }
    out
}

/// Send the nearest tow to a stuck lorry.
///
/// Priced like any other job — the round trip has to be affordable and the
/// tow has to be able to reach it — but with no load minimum and no supplier
/// to find, because the casualty *is* the demand.
fn send_help(
    world: &World,
    casualty: VehicleId,
    tows: &mut Vec<VehicleId>,
    booked: &mut Booked,
    out: &mut Vec<Mutation>,
) {
    let Some(stuck_at) = world.fleet.get(casualty).map(|v| v.at) else {
        return;
    };
    let now = world.clock.ticks() as f64;
    let crossing = world.crossing();

    let mut nearest: Vec<(f64, usize)> = tows
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            world
                .fleet
                .get(*id)
                .map(|v| (v.at.distance_to(stuck_at).0, i))
        })
        .collect();
    nearest.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

    for (_, index) in nearest {
        let id = tows[index];
        let Some(v) = world.fleet.get(id) else {
            continue;
        };
        let Some(yard) = world.buildings.get(v.home).map(|b| b.centre) else {
            continue;
        };
        let def = v.def();
        let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
        let (Some(outbound), Some(home_run)) = (leg(v.at, stuck_at), leg(stuck_at, yard)) else {
            continue;
        };
        let round_trip = outbound.distance() + home_run.distance();
        let top_up = def
            .tank
            .saturating_sub(v.fuel)
            .min(booked.fuel_left(world, v.home));
        if (v.fuel + top_up).0 < v.fuel_for(round_trip).0 {
            continue;
        }

        out.push(Mutation::Dispatch {
            vehicle: id,
            job: Job::Recover { casualty },
            journey: outbound,
            refuel: top_up,
        });
        *booked.drawn.entry(v.home).or_default() += top_up;
        booked.rescuing.push(casualty);
        tows.remove(index);
        return;
    }
}

/// Tonnage and fuel that are already spoken for.
///
/// Dispatch runs every tick and a haul takes many of them, so without this the
/// same empty bin would be served again next tick, and again, until every lorry
/// in the republic was carrying coal to a building that needed one load. The
/// scalar this replaced had no such problem because its deliveries landed
/// instantly; booking both ends of a job is what a fleet has to do instead.
#[derive(Default)]
struct Booked {
    /// Promised to a destination but not yet delivered.
    incoming: BTreeMap<(Destination, Resource), Tonnes>,
    /// Spoken for at a supplier but not yet collected.
    promised: BTreeMap<(BuildingId, Resource), Tonnes>,
    /// Fuel already drawn from each garage during this pass.
    drawn: BTreeMap<BuildingId, Tonnes>,
    /// Casualties somebody is already on the way to.
    rescuing: Vec<VehicleId>,
}

impl Booked {
    fn from_fleet(world: &World) -> Self {
        let mut booked = Self::default();
        for v in world.fleet.all() {
            let Some(job) = v.job else {
                continue;
            };
            if let Some(casualty) = job.casualty() {
                booked.rescuing.push(casualty);
                continue;
            }
            let Some((from, to, resource, tonnes)) = job.haul() else {
                continue;
            };
            // A stuck lorry still owes both ends: its load is real and it is
            // still going to arrive, just later than anybody hoped. Freeing
            // the booking would send a second lorry after the same tonnage.
            match v.state.doing() {
                // Still on its way to collect, so it is owed at both ends.
                Some(Doing::Fetching) => booked.reserve(from, to, resource, tonnes),
                // Already collected: the supplier's books are settled and only
                // the destination is still waiting.
                Some(Doing::Delivering) => {
                    *booked.incoming.entry((to, resource)).or_default() += v.cargo.get(resource);
                }
                Some(Doing::Returning) | None => {}
            }
        }
        booked
    }

    fn reserve(&mut self, from: BuildingId, to: Destination, resource: Resource, tonnes: Tonnes) {
        *self.incoming.entry((to, resource)).or_default() += tonnes;
        *self.promised.entry((from, resource)).or_default() += tonnes;
    }

    fn incoming(&self, to: Destination, resource: Resource) -> Tonnes {
        self.incoming
            .get(&(to, resource))
            .copied()
            .unwrap_or(Tonnes::ZERO)
    }

    fn promised(&self, from: BuildingId, resource: Resource) -> Tonnes {
        self.promised
            .get(&(from, resource))
            .copied()
            .unwrap_or(Tonnes::ZERO)
    }

    /// Fuel a garage still has to give out, after what this pass has taken.
    fn fuel_left(&self, world: &World, garage: BuildingId) -> Tonnes {
        world
            .buildings
            .get(garage)
            .map(|b| b.stock.get(Resource::Fuel))
            .unwrap_or(Tonnes::ZERO)
            .saturating_sub(self.drawn.get(&garage).copied().unwrap_or(Tonnes::ZERO))
    }
}

/// Find the nearest surplus, and the nearest lorry that can fetch it.
fn serve(
    world: &World,
    destination: Destination,
    resource: Resource,
    idle: &mut Vec<VehicleId>,
    booked: &mut Booked,
    out: &mut Vec<Mutation>,
) {
    let Some(to) = world.consignee(destination, resource) else {
        return;
    };
    // A site's bill of materials can exceed its finished storage bin — a steel
    // mill needs 30 t of brick to build and holds 40 t of anything once open,
    // but a smaller building could easily need more than it will ever store.
    // Capping a site by its bin would stall it forever.
    let room = to
        .capacity
        .saturating_sub(to.held)
        .saturating_sub(booked.incoming(destination, resource));
    if !room.is_positive() {
        return;
    }

    // A supplier is anyone holding this who does not consume it, less whatever
    // another lorry is already on its way to collect, **and less whatever the
    // player told it to keep**.
    //
    // That last clause is what makes a standing order an order. Without it an
    // order was a target with no floor: the goods arrived and the very next
    // pass took them straight out again, because a store holds what it does not
    // consume and that is the definition of a supplier. A distribution office
    // asked to keep fifty tonnes of coal in the north was a lorry park, and a
    // filling station was a building that could be delivered diesel all day and
    // never have any — which is how this was found.
    let mut suppliers: Vec<(f64, BuildingId, Tonnes)> = world
        .buildings
        .all()
        .iter()
        .filter(|b| Destination::Building(b.id) != destination)
        .filter(|b| !b.def().inputs.iter().any(|(r, _)| *r == resource))
        .filter(|b| !b.def().sells.contains(&resource))
        .map(|b| {
            let kept = if b.def().stores_to_order {
                b.orders.get(resource)
            } else {
                Tonnes::ZERO
            };
            (
                b.centre.distance_to(to.at).0,
                b.id,
                b.stock
                    .get(resource)
                    .saturating_sub(booked.promised(b.id, resource))
                    .saturating_sub(kept),
            )
        })
        .filter(|(_, _, spare)| spare.is_positive())
        .collect();
    suppliers.sort_by(|(da, ia, _), (db, ib, _)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

    let drop_at = to.at;
    let now = world.clock.ticks() as f64;

    // Nearest first, but **not nearest only**. A yard with a few kilograms left
    // in it is not worth a trip, and treating the closest one as the only one
    // meant the whole demand was refused rather than the next yard tried: the
    // republic's clothes ran out with fifty tonnes of them standing five
    // hundred metres further away, because the depot next door had 1.99 t left.
    for &(_, from, spare) in &suppliers {
        let Some(supplier) = world.buildings.get(from) else {
            continue;
        };
        let load_at = supplier.centre;

        // Let it accumulate rather than sending a lorry for four kilograms. See
        // [`MIN_LOAD`] for what happened before this was here, and for why a
        // site and a blocked yard are served whatever the quantity.
        //
        // The yard escape has to be narrow, and getting it wrong cost the
        // republic its whole diesel reserve: waiving the minimum whenever the
        // supplier's bin was full waived it *permanently*, because a producer
        // feeding a consumer sits at its cap by definition. The trip has to
        // actually clear the yard to count, which it only does when there is
        // less than a load left in it.
        let wanted = spare.min(room);
        let yard_full = supplier.stock.get(resource).0 + 1e-9 >= supplier.storage_cap().0;
        let clears_the_yard = yard_full && wanted.0 + 1e-9 >= spare.0;
        if wanted.0 < MIN_LOAD.0 && to.finished && !clears_the_yard {
            continue;
        }

        if dispatch_one(
            world,
            destination,
            resource,
            from,
            load_at,
            drop_at,
            wanted,
            now,
            idle,
            booked,
            out,
        ) {
            return;
        }
    }
}

/// Give one ranked demand to the best-placed lorry that can finish it.
///
/// Returns whether a lorry took it.
#[allow(clippy::too_many_arguments)]
fn dispatch_one(
    world: &World,
    destination: Destination,
    resource: Resource,
    from: BuildingId,
    load_at: Point,
    drop_at: Point,
    wanted: Tonnes,
    now: f64,
    idle: &mut Vec<VehicleId>,
    booked: &mut Booked,
    out: &mut Vec<Mutation>,
) -> bool {
    // The lorry that can be there soonest, and among those the one whose bed
    // best fits the load — idle lorries are parked at their garages, so the
    // distance decides which depot and the fit decides which vehicle.
    let mut nearest: Vec<(f64, f64, usize)> = idle
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            world.fleet.get(*id).map(|v| {
                (
                    v.at.distance_to(load_at).0,
                    (v.def().capacity.0 - wanted.0).abs(),
                    i,
                )
            })
        })
        .collect();
    nearest.sort_by(|(da, wa, ia), (db, wb, ib)| {
        da.total_cmp(db)
            .then_with(|| wa.total_cmp(wb))
            .then_with(|| ia.cmp(ib))
    });

    for (_, _, index) in nearest {
        let id = idle[index];
        let Some(v) = world.fleet.get(id) else {
            continue;
        };
        let tonnes = wanted.min(v.def().capacity);
        if !tonnes.is_positive() {
            return false;
        }
        let Some(yard) = world.buildings.get(v.home).map(|b| b.centre) else {
            continue;
        };

        // **A vehicle never accepts a job it cannot finish.** The archived
        // build's rule, and the reason running dry is a refusal here rather
        // than a lorry stranded halfway. The whole round trip is priced with
        // the same planner that will drive it, against the roads as they stand
        // — and a road laid while it is out can only make the trip shorter.
        let def = v.def();
        let crossing = world.crossing();
        let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
        // **The whole round trip has to be plannable, not only affordable.**
        // For a confined vehicle this is also the reach test: a locomotive
        // whose load, drop or yard is not on the rails gets `None` here and the
        // loop moves on to the next vehicle, exactly as it does for one that
        // cannot carry the fuel.
        let (Some(outbound), Some(laden), Some(home_run)) = (
            leg(v.at, load_at),
            leg(load_at, drop_at),
            leg(drop_at, yard),
        ) else {
            continue;
        };
        let round_trip = outbound.distance() + laden.distance() + home_run.distance();
        let top_up = def
            .tank
            .saturating_sub(v.fuel)
            .min(booked.fuel_left(world, v.home));

        // A pump within reach of either end of the haul is what turns the range
        // rule from a wall into something you can build past. **The rule itself
        // does not bend** -- a vehicle still never accepts a job it cannot
        // finish -- but a filling point changes what "finish" costs: it only
        // has to carry fuel as far as the pump, not all the way home.
        //
        // Counted at the ends rather than along the route on purpose. A lorry
        // pulls in off the road; it does not detour across a field to a pump,
        // and pretending otherwise would make the range depend on a corridor
        // width nobody chose.
        let refuel_en_route = [load_at, drop_at]
            .iter()
            .filter_map(|end| filling_point(world, *end))
            .map(|pump| pump.stock.get(Resource::Fuel).min(def.tank))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .unwrap_or(Tonnes::ZERO);

        if (v.fuel + top_up + refuel_en_route).0 < v.fuel_for(round_trip).0 {
            continue;
        }

        // It leaves empty, so the way out is rolled at its own capability.
        let stuck = sticks(
            world,
            &crossing,
            id,
            def.ground,
            &outbound,
            0,
            world.clock.day_index(),
        );
        out.push(Mutation::Dispatch {
            vehicle: id,
            job: Job::Haul {
                from,
                to: destination,
                resource,
                tonnes,
            },
            journey: outbound,
            refuel: top_up,
        });
        if stuck {
            out.push(Mutation::Bog {
                vehicle: id,
                day: world.clock.day_index(),
            });
        }
        booked.reserve(from, destination, resource, tonnes);
        *booked.drawn.entry(v.home).or_default() += top_up;
        idle.remove(index);
        return true;
    }
    false
}

/// How likely a crossing is to go wrong, given the going and what the vehicle
/// can take.
///
/// **Deterministic margin plus a probability roll**, which is the shape noahs
/// chose over a fully deterministic model: vehicles get stuck in real life, and
/// a model where the outcome is a foregone conclusion is a model where nobody
/// ever has to decide anything. What the margin buys back is explicability —
/// the odds are a function of two numbers the player can see, and
/// [`crate::world::World::bog_chance`] shows them.
///
/// Inside its capability nothing happens at all. Beyond it the odds climb with
/// how far beyond, and stop short of certain: a lorry with a bad crossing ahead
/// is in trouble, not doomed.
pub fn bog_chance(going: f64, capability: f64) -> f64 {
    let margin = capability - going;
    if margin >= 0.0 {
        return 0.0;
    }
    (-margin / BOG_SPAN).clamp(0.0, WORST_ODDS)
}

/// How far past its capability a vehicle has to be pushed for the odds to reach
/// [`WORST_ODDS`].
pub const BOG_SPAN: f64 = 1.2;

/// The worst the odds ever get. Never certain, because a crossing nobody could
/// ever make is a route the planner should have refused, not a dice roll.
pub const WORST_ODDS: f64 = 0.6;

/// The best chance a crew has of driving out unaided in a day.
pub const DIG_OUT: f64 = 0.35;

/// The chance a stuck vehicle gets itself out today.
///
/// Certain once the ground has genuinely come back under it — that is the rule
/// as designed, and it is kept. What is added is the case in between, and it
/// was added on measurement: over a simulated year *every single* casualty was
/// waiting for a tow, because a vehicle bogs precisely when the going is bad
/// and the going stays bad for weeks afterwards. The deterministic rule alone
/// made half the mechanic unreachable.
///
/// So a marginal bogging is treated as what it is — bad luck rather than bad
/// ground — and the crew digs it out in a few days. One that went well past
/// what the vehicle could take does not come out without help, which is what
/// keeps a recovery vehicle worth having.
pub fn dig_out_chance(going: f64, capability: f64) -> f64 {
    if going <= capability {
        return 1.0;
    }
    let how_badly = bog_chance(going, capability) / WORST_ODDS;
    ((1.0 - how_badly) * DIG_OUT).clamp(0.0, 1.0)
}

/// Move the fleet.
///
/// The entire per-tick cost of freight is here, and for most vehicles it is one
/// float comparison: has this leg finished yet. Only the handful whose leg has
/// actually ended do real work — arrive, load, re-plan, turn for home, or stick
/// fast. That is what [`crate::journey`] buys, and it is why freight can be
/// physical at all at the speeds this game runs.
///
/// A vehicle finishes **at most one leg per tick**, which is not a rule imposed
/// here but a consequence of [`crate::journey::MIN_LEG_TICKS`]: the next leg
/// always ends at least a tick after the one that just did.
pub fn fleet(world: &World) -> Vec<Mutation> {
    let now = world.clock.ticks() as f64;
    let day = world.clock.day_index();
    let new_day = world.clock.is_day_boundary();
    let crossing = world.crossing();
    let mut out = Vec::new();

    for v in world.fleet.all() {
        let Some(journey) = v.journey.as_ref() else {
            continue;
        };
        let def = v.def();

        // A stuck lorry is not driving anywhere, so it burns nothing and its leg
        // does not end. The only question asked of it is whether the ground has
        // come back to it — and it is asked **before** anything about arrival
        // times, because a vehicle that stuck in the middle of a crossing has a
        // leg that is still notionally running and would otherwise never be
        // looked at again.
        if let VehicleState::Bogged { was, .. } = v.state {
            let leg = journey.leg;
            let (from, to) = journey.leg_ends(leg);
            let odds = dig_out_chance(crossing.going_along(from, to), v.capability());
            let out_by_itself = odds >= 1.0
                || (new_day
                    && odds > 0.0
                    && world
                        .substream(BOG_STREAM, free_key(v.id, leg, day))
                        .next_f64()
                        < odds);
            if out_by_itself {
                let drag = crossing.drag_for(journey.leg_on_road(), from, to);
                let speed = journey.speed_on(leg, def.on_road, def.cross_country, drag);
                // It starts the crossing again rather than picking up where it
                // stopped: it is at a standstill in a field, not idling at a
                // waypoint.
                out.push(Mutation::Free {
                    vehicle: v.id,
                    was,
                    leg,
                    leg_start: now,
                    leg_end: now + journey::leg_ticks(from.distance_to(to), speed),
                });
            }
            continue;
        }

        if !journey.leg_done_by(now) {
            // Still on its way. The only thing that can happen to it before it
            // arrives is the ground turning underneath it, and the ground only
            // turns once a day — which is exactly the case the mechanic exists
            // for: dispatched in weather that was fine, stuck in weather that
            // is not.
            if new_day
                && sticks(
                    world,
                    &crossing,
                    v.id,
                    v.capability(),
                    journey,
                    journey.leg,
                    day,
                )
            {
                out.push(Mutation::Bog { vehicle: v.id, day });
            }
            continue;
        }

        let burn = v.fuel_for(journey.leg_distance());

        // A pump within reach of wherever this leg just ended: fill up.
        //
        // At **every** leg boundary rather than only at a journey's end, which
        // took a measurement to get right. A lorry passes a filling point in
        // the middle of a run far more often than it finishes a journey beside
        // one, and checking only destinations meant the pump served nobody --
        // the write-set guard reported the declaration as a superset three
        // times before this moved.
        let reached = journey.leg_ends(journey.leg).1;
        if let Some(pump) = filling_point(world, reached) {
            let drawn = def
                .tank
                .saturating_sub(v.fuel)
                .min(pump.stock.get(Resource::Fuel));
            if drawn.is_positive() {
                out.push(Mutation::Refuel {
                    vehicle: v.id,
                    from: pump.id,
                    tonnes: drawn,
                });
            }
        }

        if !journey.on_last_leg() {
            let ahead = journey.leg + 1;
            let (from, to) = journey.leg_ends(ahead);
            let drag = crossing.drag_for(journey.limit[ahead as usize].is_some(), from, to);
            let speed = journey.speed_on(ahead, def.on_road, def.cross_country, drag);
            let (leg, leg_start, leg_end) = journey.next_leg(speed);
            // The leg just finished did happen, so it moved the lorry and burnt
            // its diesel either way. What the roll decides is whether the *next*
            // one starts — and the plan has to move on to that leg first, or a
            // freed vehicle would be told to re-drive the one behind it.
            out.push(Mutation::Advance {
                vehicle: v.id,
                leg,
                leg_start,
                leg_end,
                burn,
            });
            out.extend(wore(world, v, journey.leg));
            out.extend(swept(world, v, journey.leg));
            // The crossing about to be attempted, evaluated against the ground
            // as it is *now* rather than as it was when the plan was made.
            if sticks(world, &crossing, v.id, v.capability(), journey, ahead, day) {
                out.push(Mutation::Bog { vehicle: v.id, day });
            }
            continue;
        }

        // The journey is over, and what that means depends which way the lorry
        // was pointed. The next one is timed from the *scheduled* arrival
        // rather than the tick it was noticed on, which is what keeps a long
        // haul from drifting a minute at every waypoint.
        let arrived = journey.destination();
        let depart = journey.leg_end;
        let Some(yard) = world.buildings.get(v.home).map(|b| b.centre) else {
            continue;
        };
        let onward = |target: Point| plan_leg(world, &crossing, def, arrived, target, depart);
        // The way home, worked out once because nearly every branch below ends
        // with it. A vehicle that cannot find one **parks where it stands**
        // rather than being carried home by fiat — which is the same answer the
        // line above gives a vehicle whose garage was pulled down while it was
        // out. Unreachable for a confined vehicle in ordinary play, because
        // dispatch prices the whole round trip before the job is taken, but it
        // is a state the world can hold and so it gets an answer.
        let Some(home_run) = onward(yard) else {
            out.push(Mutation::Park {
                vehicle: v.id,
                burn,
            });
            continue;
        };

        // Whatever it just drove over, it packed down a little -- and if it
        // had a blade on the front, it swept clear.
        out.extend(wore(world, v, journey.leg));
        out.extend(swept(world, v, journey.leg));

        match v.state {
            VehicleState::Returning => out.push(Mutation::Park {
                vehicle: v.id,
                burn,
            }),
            VehicleState::Fetching => match v.job {
                // Arrived at the casualty. Hooking on, pulling it out and
                // turning for home is one act, so it is one mutation — a tow
                // that half happened would leave a lorry in a field with a
                // recovery vehicle standing next to it doing nothing.
                Some(Job::Recover { casualty }) => {
                    let Some(stuck) = world.fleet.get(casualty) else {
                        // It freed itself while help was on the way. Go home.
                        out.push(Mutation::Load {
                            vehicle: v.id,
                            from: v.home,
                            resource: Resource::Fuel,
                            tonnes: Tonnes::ZERO,
                            journey: home_run.clone(),
                            state: VehicleState::Returning,
                            burn,
                        });
                        continue;
                    };
                    let Some(was) = stuck.state.doing() else {
                        continue;
                    };
                    let Some(plan) = stuck.journey.as_ref() else {
                        continue;
                    };
                    // The tow drags it through the bad patch rather than
                    // setting it down in the same one — otherwise the recovery
                    // is a coin flip away from being needed again immediately,
                    // and a mechanic whose answer is "try again" is not one.
                    let leg = plan.leg;
                    let (a, b) = plan.leg_ends(leg);
                    let stuck_def = stuck.def();
                    let drag = crossing.drag_for(plan.leg_on_road(), a, b);
                    let speed =
                        plan.speed_on(leg, stuck_def.on_road, stuck_def.cross_country, drag);
                    out.push(Mutation::Recover {
                        recovery: v.id,
                        casualty,
                        was,
                        casualty_leg: leg,
                        casualty_start: now,
                        casualty_end: now + journey::leg_ticks(a.distance_to(b), speed),
                        journey: home_run.clone(),
                        burn,
                    });
                }
                Some(Job::Haul {
                    from,
                    to,
                    resource,
                    tonnes: wanted,
                }) => {
                    let held = world
                        .buildings
                        .get(from)
                        .map(|b| b.stock.get(resource))
                        .unwrap_or(Tonnes::ZERO);
                    let tonnes = wanted.min(held).min(v.spare_capacity());
                    // A yard emptied since the job was booked sends the lorry
                    // home rather than on to a destination it has nothing for.
                    let onto = world.consignee(to, resource).map(|c| c.at);
                    let (state, next) = match (tonnes.is_positive(), onto) {
                        (true, Some(target)) => (VehicleState::Delivering, target),
                        _ => (VehicleState::Returning, yard),
                    };
                    // Loaded, and the first crossing of the way out is rolled
                    // against what it now weighs. A single-leg journey has no
                    // leg boundary to be caught at, so this is where a short
                    // haul across a wet field goes wrong.
                    // No way on to the drop means the way home, which was
                    // already found above — a loaded vehicle is never left
                    // standing because the far end went out of reach.
                    let plan = onward(next).unwrap_or_else(|| home_run.clone());
                    let laden = def.ground
                        - (tonnes.0 / def.capacity.0.max(f64::MIN_POSITIVE)).clamp(0.0, 1.0)
                            * def.load_penalty;
                    let stuck = sticks(world, &crossing, v.id, laden, &plan, 0, day);
                    out.push(Mutation::Load {
                        vehicle: v.id,
                        from,
                        resource,
                        tonnes,
                        journey: plan,
                        state,
                        burn,
                    });
                    if stuck {
                        out.push(Mutation::Bog { vehicle: v.id, day });
                    }
                }
                // Arrived at a gang waiting for a lift. They get on and the bus
                // turns for home; the heads rejoin the office's establishment
                // when it parks.
                Some(Job::Collect { party }) => match world.crews.get(party) {
                    Some(_) => out.push(Mutation::Embark {
                        vehicle: v.id,
                        party,
                        journey: home_run.clone(),
                        burn,
                    }),
                    // Nobody here. Go home rather than stand in a field holding
                    // a job that can never finish — the same answer the recovery
                    // case gives when a casualty frees itself on the way.
                    None => out.push(Mutation::Load {
                        vehicle: v.id,
                        from: v.home,
                        resource: Resource::Fuel,
                        tonnes: Tonnes::ZERO,
                        journey: home_run.clone(),
                        state: VehicleState::Returning,
                        burn,
                    }),
                },
                // Arrived at a frontier post where settlers are standing. They
                // get on and the coach turns for the housing it was sent to —
                // not for its own yard, which is what makes this a two-hop
                // journey rather than a collection.
                Some(Job::Settle { group, to }) => {
                    match (
                        world.migration.get(group).map(|g| g.heads),
                        world.buildings.get(to).map(|b| b.centre),
                    ) {
                        (Some(_), Some(estate)) if onward(estate).is_some() => {
                            out.push(Mutation::Board {
                                vehicle: v.id,
                                group,
                                journey: onward(estate).expect("just checked"),
                                burn,
                            })
                        }
                        // Either nobody is here or the estate has been pulled
                        // down under them. Go home rather than stand in a field
                        // holding a job that can never finish.
                        _ => out.push(Mutation::Load {
                            vehicle: v.id,
                            from: v.home,
                            resource: Resource::Fuel,
                            tonnes: Tonnes::ZERO,
                            journey: home_run.clone(),
                            state: VehicleState::Returning,
                            burn,
                        }),
                    }
                }
                // Arrived at a post where visitors are standing. They get on
                // and the coach turns for the hotel it was sent to.
                Some(Job::Tour { visit, to }) => {
                    match world.buildings.get(to).map(|b| b.centre) {
                        Some(door) if onward(door).is_some() => out.push(Mutation::Fetch {
                            vehicle: v.id,
                            visit,
                            journey: onward(door).expect("just checked"),
                            burn,
                        }),
                        // Either nobody is here or the hotel has gone. Go home
                        // rather than stand in a field holding a job that can
                        // never finish.
                        _ => out.push(Mutation::Load {
                            vehicle: v.id,
                            from: v.home,
                            resource: Resource::Fuel,
                            tonnes: Tonnes::ZERO,
                            journey: home_run.clone(),
                            state: VehicleState::Returning,
                            burn,
                        }),
                    }
                }
                // Reached the far end of what it was sent to clear. There is
                // nothing to pick up: it turns round, and the way home is
                // swept exactly as the way out was.
                Some(Job::Plough { .. }) => out.push(Mutation::Load {
                    vehicle: v.id,
                    from: v.home,
                    resource: Resource::Fuel,
                    tonnes: Tonnes::ZERO,
                    journey: home_run.clone(),
                    state: VehicleState::Returning,
                    burn,
                }),
                Some(Job::Ferry { .. }) | None => {}
            },
            // A bus is `Delivering` from the moment it leaves the office,
            // because the crew boarded in the yard. Arriving means setting them
            // down on the site they were sent to.
            VehicleState::Delivering if v.job.and_then(Job::ferry).is_some() => {
                let Some((site, _)) = v.job.and_then(Job::ferry) else {
                    continue;
                };
                let Some(party) = world.crews.riding(v.id).map(|p| p.id) else {
                    continue;
                };
                let plan = home_run.clone();
                let stuck = sticks(world, &crossing, v.id, def.ground, &plan, 0, day);
                out.push(Mutation::Land {
                    vehicle: v.id,
                    party,
                    site,
                    at: arrived,
                    journey: plan,
                    burn,
                });
                if stuck {
                    out.push(Mutation::Bog { vehicle: v.id, day });
                }
            }
            // A coach with settlers aboard, arriving at the estate. They come
            // off as residents; the coach turns for its depot.
            VehicleState::Delivering if v.job.and_then(Job::settling).is_some() => {
                let Some((group, home)) = v.job.and_then(Job::settling) else {
                    continue;
                };
                let plan = home_run.clone();
                let stuck = sticks(world, &crossing, v.id, def.ground, &plan, 0, day);
                out.push(Mutation::Settle {
                    vehicle: v.id,
                    group,
                    home,
                    journey: plan,
                    burn,
                });
                if stuck {
                    out.push(Mutation::Bog { vehicle: v.id, day });
                }
            }
            // A coach with visitors aboard, arriving at the hotel. They check
            // in and start spending; the coach turns for its depot.
            VehicleState::Delivering if matches!(v.job, Some(Job::Tour { .. })) => {
                let Some(Job::Tour { visit, to }) = v.job else {
                    continue;
                };
                let plan = home_run.clone();
                let stuck = sticks(world, &crossing, v.id, def.ground, &plan, 0, day);
                out.push(Mutation::CheckIn {
                    vehicle: v.id,
                    visit,
                    hotel: to,
                    at: arrived,
                    journey: plan,
                    burn,
                });
                if stuck {
                    out.push(Mutation::Bog { vehicle: v.id, day });
                }
            }
            VehicleState::Delivering => {
                let Some((_, to, resource, _)) = v.job.and_then(Job::haul) else {
                    continue;
                };
                let room = world
                    .consignee(to, resource)
                    .map(|c| c.capacity.saturating_sub(c.held))
                    .unwrap_or(Tonnes::ZERO);
                let plan = home_run.clone();
                // Empty now, so the way home is rolled at the vehicle's own
                // capability rather than at a laden one.
                let stuck = sticks(world, &crossing, v.id, def.ground, &plan, 0, day);
                out.push(Mutation::Unload {
                    vehicle: v.id,
                    to,
                    resource,
                    tonnes: v.cargo.get(resource).min(room),
                    journey: plan,
                    burn,
                });
                if stuck {
                    out.push(Mutation::Bog { vehicle: v.id, day });
                }
            }
            // A parked vehicle has no journey and a bogged one was handled
            // above, so neither is reachable here.
            VehicleState::Idle | VehicleState::Bogged { .. } => {}
        }
    }
    out
}

/// How near a filling point a vehicle must be to draw from it.
///
/// The same order as `ROAD_ACCESS`: a lorry pulls in off the road, it does not
/// drive across a field to reach a pump.
pub const REFUEL_RANGE: Metres = Metres(300.0);

/// The filling point a vehicle standing here could draw from.
///
/// A `GasStation` was in the building table from the start with nothing to do.
/// This is what it does: it turns the range rule from a wall into something you
/// can build past. A vehicle never accepts a job it cannot finish -- that rule
/// does not bend -- but where it can *refuel* changes what it can finish, which
/// is a range mechanic rather than a safety one.
fn filling_point(world: &World, at: Point) -> Option<&crate::building::Building> {
    world
        .buildings
        .of_kind(BuildingKind::GasStation)
        .filter(|b| b.is_built() && b.staffing() > 0.0)
        .filter(|b| b.stock.get(Resource::Fuel).is_positive())
        .filter(|b| b.centre.distance_to(at).0 <= REFUEL_RANGE.0)
        .min_by_key(|b| b.id)
}

/// The ground a vehicle just packed down finishing a leg.
///
/// Nothing on a road: tarmac and gravel do not take a rut, and a road that
/// wore itself into a better road would be a loop with no end.
fn wore(world: &World, v: &crate::fleet::Vehicle, leg: u32) -> Option<Mutation> {
    let journey = v.journey.as_ref()?;
    if journey.limit[leg as usize].is_some() {
        return None;
    }
    let (from, to) = journey.leg_ends(leg);
    let cells = world.lattice.cells_along(from, to);
    if cells.is_empty() {
        return None;
    }
    Some(Mutation::Wear {
        cells,
        // A laden lorry leaves more of a line than an empty one.
        by: crate::ground::WEAR_PER_PASS * (0.4 + 0.6 * v.load_fraction()),
    })
}

/// The snow a plough just pushed off the leg it finished.
///
/// The counterpart of [`wore`], and the mirror image of it in one telling way:
/// wear is refused **on** a road because tarmac does not rut, and clearing is
/// wanted on a road above all, because that is what a plough is for. It clears
/// off-road legs too — the machine has a blade on the front and the snow does
/// not care what is underneath — which is what lets a plough open the way to an
/// outlying works that has no road yet.
fn swept(world: &World, v: &crate::fleet::Vehicle, leg: u32) -> Option<Mutation> {
    if v.def().role != crate::fleet::Role::Clearance {
        return None;
    }
    let journey = v.journey.as_ref()?;
    let (from, to) = journey.leg_ends(leg);
    let cells = world.lattice.cells_along(from, to);
    if cells.is_empty() {
        return None;
    }
    Some(Mutation::Clear { cells })
}

/// Whether a vehicle sticks setting out on a given leg today.
///
/// A pure function of who, which leg, and what day, so it gives the same
/// answer however many times it is asked — which is what lets the roll be
/// made at every point a leg can begin without a vehicle getting several
/// bites at the same crossing.
///
/// Roads never bog. That is the whole argument for building one.
pub fn sticks(
    world: &World,
    crossing: &crate::ground::Crossing,
    vehicle: VehicleId,
    capability: f64,
    journey: &Journey,
    leg: u32,
    day: u64,
) -> bool {
    if leg >= journey.legs() || journey.limit[leg as usize].is_some() {
        return false;
    }
    let (from, to) = journey.leg_ends(leg);
    let odds = bog_chance(crossing.going_along(from, to), capability);
    odds > 0.0
        && world
            .substream(BOG_STREAM, bog_key(vehicle, leg, day))
            .next_f64()
            < odds
}

/// The same key for the digging-out roll, salted so that a crew's chance of
/// getting out is not the same draw that put them there.
fn free_key(vehicle: VehicleId, leg: u32, day: u64) -> u64 {
    bog_key(vehicle, leg, day) ^ 0x94D0_49BB_1331_11EB
}

/// Key the bogging roll by who, where in the journey, and when.
///
/// Mixed rather than xor-ed so that a low vehicle id on a low leg does not
/// collide with the day beside it — the same draw twice would make two
/// crossings share a fate for no reason anybody could see.
fn bog_key(vehicle: VehicleId, leg: u32, day: u64) -> u64 {
    u64::from(vehicle.0).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ u64::from(leg).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ day.wrapping_mul(0xBF58_476D_1CE4_E5B9)
}

/// The ground giving back what traffic put into it, and what is left over
/// becoming a road.
///
/// Daily, because both halves are daily quantities and because sweeping ten
/// thousand cells for connected corridors has no business happening 1,440
/// times a day.
///
/// **The feedback loop needs damping or it runs away.** Wear lowers the cost
/// of a cell, which concentrates traffic on it, which wears it further. Two
/// things bound it: packing saturates at a made track rather than improving
/// for ever, and a corridor that reaches the threshold is promoted *out* of
/// the lattice into the road network — after which traffic rides a road leg
/// instead of a cross-country one, stops wearing the cells, and lets them
/// fade. The loop closes rather than diverging.
pub fn tracks(world: &World) -> Vec<Mutation> {
    let mut out = vec![Mutation::Fade {
        by: crate::ground::WEAR_FADE_PER_DAY,
    }];
    for run in world.lattice.tracks_beyond(crate::ground::PROMOTE_AT) {
        // Already a road here? Then the corridor has been promoted before and
        // is only waiting to fade.
        let all_on_the_map = run.iter().all(|&cell| {
            world
                .roads
                .nearest_node(
                    world.lattice.centre_of(cell),
                    crate::roadworks::JUNCTION_MERGE,
                )
                .is_some()
        });
        if !all_on_the_map {
            out.push(Mutation::Promote { cells: run });
        }
    }
    out
}

/// The day's weather, worked through the ground.
///
/// Daily, and it has to be: soil moisture and lying snow are quantities per
/// *day*, and running the recurrence 1,440 times would dry a field out in an
/// afternoon and melt a winter's snow before lunch.
pub fn weather(world: &World) -> Vec<Mutation> {
    let (temperature, rain) = world.weather_on_day(world.clock.day_index());
    let mut ground = world.ground;
    let before = ground.snow;
    ground.advance(temperature, rain);
    // What fell today, as a share of a stopping depth. A day that melted snow
    // buries nothing: the pack shrinking does not undo a plough's work, and
    // taking the absolute difference would have had a thaw covering the roads.
    let fell = (ground.snow - before).max(0.0);
    vec![Mutation::Weather {
        ground,
        snowfall: (fell / crate::ground::SNOW_BLOCKS_MM).clamp(0.0, 1.0),
    }]
}

/// Vehicles arriving on a garage's strength.
///
/// Daily, because a depot does not take delivery of a lorry between one minute
/// and the next, and because reconciling every garage against its establishment
/// has no business running 1,440 times a day.
///
/// Written as a **reconciliation** rather than as a hook on construction
/// finishing: a depot that opened, a save that reloaded, and a future in which
/// an establishment can be enlarged all land in the same code path, and none of
/// them can be forgotten separately.
/// Advances that came due with money still owed.
///
/// **Daily**, for the same reason contracts are: a due day is a day index, so a
/// per-tick sweep would default a republic 1,440 times over one unpaid advance.
///
/// A default costs more than the money. The bloc writes the debt off, takes a
/// quarter of what was outstanding as a fine, and sours — worse than a missed
/// tender does, because failing to deliver goods is a bad month and failing to
/// repay an advance is a bad republic. That penalty rides the same relations
/// machinery a missed tender uses, so a defaulted-on bloc quotes worse prices
/// thereafter without a second mechanism for it.
pub fn loans(world: &World) -> Vec<Mutation> {
    let today = world.clock.day_index();
    let mut out = Vec::new();
    for loan in world.loans.overdue(today) {
        let lost = loan.outstanding();
        out.push(Mutation::DefaultOnLoan {
            market: loan.market,
        });
        out.push(Mutation::Fine {
            market: loan.market,
            amount: lost * crate::loan::DEFAULT_FINE,
        });
        out.push(Mutation::Relations {
            market: loan.market,
            penalty: crate::loan::DEFAULT_RELATIONS,
        });
    }
    out
}

/// The foreign wage bill, paid daily in each bloc's own money.
///
/// **A penalty denominated in something the republic still has.** The obvious
/// consequence of an unpaid wage bill is a fine, and this project has already
/// learned once — from loans — that a fine on an empty purse takes nothing and
/// is therefore free. What an unpayable wage costs here is the *worker*: as many
/// as the money did not cover pack up and go home, which bites precisely when
/// the republic has no money, and leaves it with the half-built site it hired
/// them for.
///
/// Daily rather than per-tick, for the same reason contracts are: a wage is a
/// day's pay, and a per-tick sweep would bill a republic 1,440 times for it.
///
/// Whoever leaves is taken from the **last** office to have hired, so a
/// republic losing half its foreign labour loses it from one place rather than
/// evenly from everywhere — a gang is a gang, and thinning every one of them is
/// worse than losing one outright.
pub fn wages(world: &World) -> Vec<Mutation> {
    let mut out = Vec::new();
    for market in Market::ALL {
        let employers = world.crews.employers(market);
        if employers.is_empty() {
            continue;
        }
        let heads: u32 = employers.iter().map(|(_, n)| n).sum();
        let owed = f64::from(heads) * crate::crews::FOREIGN_WAGE;
        let held = world.treasury.of(market);
        let paid = owed.min(held);

        // How many the shortfall could not cover, rounded **up**: a worker paid
        // three quarters of a day is not paid.
        let unpaid = ((owed - paid) / crate::crews::FOREIGN_WAGE).ceil().max(0.0) as u32;
        let mut dismissed = Vec::new();
        let mut left = unpaid.min(heads);
        for &(office, on_books) in employers.iter().rev() {
            if left == 0 {
                break;
            }
            let go = left.min(on_books);
            dismissed.push((office, go));
            left -= go;
        }
        out.push(Mutation::Wages {
            market,
            paid,
            dismissed,
        });
    }
    out
}

/// Every built, staffed building of a kind, as positions.
///
/// A service with nobody in it serves nobody — a clinic with no doctors is a
/// building, and the whole reason staffing is a fraction is so that questions
/// like this have an answer.
fn staffed_service(world: &World, kind: BuildingKind) -> Vec<Point> {
    world
        .buildings
        .all()
        .iter()
        .filter(|b| b.kind == kind && b.is_built() && b.staffing() > 0.0)
        .map(|b| b.centre)
        .collect()
}

/// Whether any of these is within reach of a home, and how well staffed the
/// nearest one is.
///
/// Returns `0.0..=1.0` rather than a boolean: a polyclinic running at a third
/// of its establishment is a third of a polyclinic, and rounding that to "you
/// have healthcare" would hide exactly the kind of quiet failure this whole
/// section of the goal exists to make visible.
fn service_cover(world: &World, home: Point, need: crate::building::Need) -> f64 {
    world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built())
        .filter(|b| b.centre.distance_to(home).0 <= SERVICE_RADIUS.0)
        .map(|b| {
            b.def()
                .serves
                .iter()
                .filter(|&&(what, _)| what == need)
                .map(|&(_, share)| share * b.activity())
                .sum::<f64>()
        })
        .sum::<f64>()
        .min(1.0)
}

/// How well the republic is serving the people in it, and how they feel about
/// it.
///
/// **Daily.** Contentment is a mood rather than an event, and a per-tick sweep
/// would walk the whole population 1,440 times to compute the same answer.
///
/// Two things happen here and they are one pass on purpose: a home's
/// [`crate::wellbeing::Contentment`] is computed from what is within reach of
/// it, and every resident's loyalty then drifts toward that number. Splitting
/// them would mean walking the population twice to read the same answer.
pub fn contentment(world: &World) -> Vec<Mutation> {
    let census = world.population.census_by_home();
    // Whether today is cold enough for heating to mean anything. Today's
    // temperature, never the month — the same rule the boilers answer to.
    let cold = crate::climate::heating_required(world.temperature());

    let mut out = Vec::new();
    let mut scores: BTreeMap<BuildingId, f64> = BTreeMap::new();

    let mut homes: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().residents > 0)
        .collect();
    homes.sort_by_key(|b| b.id);

    for home in homes {
        let here = census.get(&home.id).copied().unwrap_or_default();
        if here.residents == 0 {
            continue;
        }
        let content = crate::wellbeing::Contentment {
            provisions: home.provisioned,
            // A warm day asks nothing of the boilers, so an estate is not
            // unhappy about heat nobody is sending it in July.
            warmth: if !cold || home.def().heat <= 0.0 {
                1.0
            } else {
                f64::from(u8::from(home.heated))
            },
            health: service_cover(world, home.centre, crate::building::Need::Health),
            culture: service_cover(world, home.centre, crate::building::Need::Culture),
            // A block with no children is not unhappy about the lack of a
            // school, and a block full of them very much is.
            schooling: if here.pupils == 0 {
                1.0
            } else {
                service_cover(world, home.centre, crate::building::Need::Schooling)
            },
            work: if here.working_age == 0 {
                1.0
            } else {
                f64::from(here.employed) / f64::from(here.working_age)
            },
            // Bins that nobody has emptied, and the air the works upwind is
            // making. Both are "this is not a pleasant place to live", and a
            // resident cannot tell them apart, so they are one number.
            // Fire, police and the courts. Unlike the others this is **not**
            // waived when nobody needs it today, because the point of a fire
            // station is the day you do.
            safety: service_cover(world, home.centre, crate::building::Need::Safety),
            cleanliness: {
                let bin = home.storage_cap();
                let rubbish = if bin.is_positive() {
                    (home.stock.get(Resource::Waste).0 / bin.0).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let smoke = world.lattice.pollution_near(home.centre);
                (1.0 - rubbish.max(smoke)).clamp(0.0, 1.0)
            },
            // Drink and household electrics off a shelf in reach. A lift on top
            // of everything above rather than one more of them — see
            // `Contentment::comforts` for why that is the whole design.
            comforts: home.comforted,
        };
        scores.insert(home.id, content.overall());
        out.push(Mutation::Content {
            building: home.id,
            content,
        });
    }

    // What medical care is within reach of each home, computed once rather than
    // once per resident.
    let clinics = staffed_service(world, BuildingKind::Clinic);
    let cover_at = |home: Point| -> f64 {
        clinics
            .iter()
            .filter(|c| c.distance_to(home).0 <= SERVICE_RADIUS.0)
            .count()
            .min(1) as f64
    };

    let mut updates = Vec::new();
    for record in world.population.records() {
        let Some(home) = world.buildings.get(record.home.0) else {
            continue;
        };
        let target_loyalty = scores.get(&record.home.0).copied().unwrap_or(0.0);

        let served = crate::wellbeing::HEALTH_UNSERVED
            + (1.0 - crate::wellbeing::HEALTH_UNSERVED) * cover_at(home.centre);
        // Age tells on people whatever the republic does about it. This is what
        // makes an ageing population a problem rather than a statistic.
        let wear = 1.0 - f64::from(record.age.0.saturating_sub(50)) * 0.012;
        // And so does the drink. **The other half of a trade the player is
        // making on purpose**: supplying alcohol lifts a home's contentment and
        // costs the people in it health, scaled by how much of it the shops in
        // reach actually had. A republic that makes none pays nothing.
        let drink = crate::wellbeing::ALCOHOL_HEALTH_COST * home.drink.clamp(0.0, 1.0);
        // **What a long shift costs, and the reason shift length is not free.**
        // Adding a crew is a straight trade — three times the output for three
        // times the people. Lengthening a shift is not: it buys half again as
        // much day out of the crew already there. So it is charged to the crew,
        // in the two currencies people are actually paid in. Loyalty is the one
        // that bites, because it already reaches emigration.
        let (tired, resentful) = record
            .workplace
            .0
            .and_then(|id| world.buildings.get(id))
            .map(|work| crate::shifts::overwork_cost(work.hours))
            .unwrap_or((0.0, 0.0));
        let target_health = (served * wear.max(0.25) - drink - tired).clamp(0.0, 1.0);
        let health = record.wellbeing.health
            + (target_health - record.wellbeing.health) * crate::wellbeing::HEALTH_DRIFT;

        let target_loyalty = (target_loyalty - resentful).clamp(0.0, 1.0);
        let loyalty = record.wellbeing.loyalty
            + (target_loyalty - record.wellbeing.loyalty) * crate::wellbeing::LOYALTY_DRIFT;

        updates.push((
            record.id,
            crate::citizen::Wellbeing {
                health: health.clamp(0.0, 1.0),
                loyalty: loyalty.clamp(0.0, 1.0),
            },
        ));
    }
    if !updates.is_empty() {
        out.push(Mutation::Morale { updates });
    }
    out
}

/// Who sat in a classroom today.
///
/// **Daily**, and attendance is only counted where there is a *staffed* school
/// within reach: a school with no teachers teaches nobody, and a school on the
/// other side of the republic teaches somebody else's children.
///
/// Attendance stops accruing once somebody has enough of it. Without that cap a
/// child with a school for their whole childhood would bank ten years against a
/// five-year requirement and walk out of school a graduate, which would make the
/// university a building nobody had any reason to put up.
pub fn schooling(world: &World) -> Vec<Mutation> {
    let schools = staffed_service(world, BuildingKind::School);
    let universities = staffed_service(world, BuildingKind::University);
    let within = |places: &[Point], home: Point| {
        places
            .iter()
            .any(|p| p.distance_to(home).0 <= SERVICE_RADIUS.0)
    };

    let mut attended = Vec::new();
    let mut enrolled = Vec::new();
    // Anyone the world currently believes is at university. Tracked so that
    // enrolment can be *cleared*: a university that lost its staff has to stop
    // having students today, and a pass that only ever enrolled would leave
    // them permanently out of a workforce nobody was teaching.
    let mut studying = false;
    for record in world.population.records() {
        studying |= record.learning.studying;
        let Some(home) = world.buildings.get(record.home.0) else {
            continue;
        };
        let days = record.learning.days;
        if crate::citizen::SCHOOL_AGE.contains(&record.age.0) {
            if days < crate::citizen::SCHOOL_DAYS && within(&schools, home.centre) {
                attended.push(record.id);
            }
            continue;
        }
        // University: finished school, of an age to go, has not finished, and
        // there is one within reach.
        let degree = crate::citizen::SCHOOL_DAYS
            ..crate::citizen::SCHOOL_DAYS + crate::citizen::UNIVERSITY_DAYS;
        let wants =
            crate::citizen::UNIVERSITY_AGE.contains(&record.age.0) && degree.contains(&days);
        if wants && within(&universities, home.centre) {
            enrolled.push(record.id);
            attended.push(record.id);
        }
    }

    // Nothing taught and nobody to un-enrol. Emitting an empty census every day
    // would be noise, and worse: it would let the write-set guard pass for a
    // republic in which no child has ever sat in a classroom.
    if attended.is_empty() && enrolled.is_empty() && !studying {
        return Vec::new();
    }
    // Both lists are already in id order because `records` is, which is what
    // the binary searches on the applying side rely on.
    vec![Mutation::Schooling { attended, enrolled }]
}

/// The annual chance of dying at a given age and state of health.
///
/// A Gompertz-shaped curve: near flat through working life and steepening hard
/// after sixty, which is what an age pyramid actually looks like. Health
/// divides it, so a republic with polyclinics keeps its old people longer —
/// which is the entire argument for building one.
///
/// Exposed rather than private because it is showable: a panel that can say
/// "this district's people are dying at four times the republic's rate" is
/// worth more than one that reports a population going down.
pub fn mortality(age: u32, health: f64) -> f64 {
    let base = 0.002 + (f64::from(age) / 100.0).powi(8) * 2.0;
    (base / (0.5 + health.clamp(0.0, 1.0))).clamp(0.0, 1.0)
}

/// Nobody lives past this. A bound rather than a balance figure: without one a
/// citizen whose mortality roll keeps coming up safe lives for ever, and an
/// unbounded age walks straight into the `powi(8)` above.
pub const OLDEST: u32 = 105;

/// Children per fertile pair per year, at a home whose people are content.
///
/// First-pass, and deliberately below replacement on its own: a republic grows
/// mainly by attracting people, and births are what stop it hollowing out.
pub const BIRTHS_PER_PAIR_YEAR: f64 = 0.22;

/// Below this contentment, a household does not start a family.
pub const BIRTHS_NEED: f64 = 0.45;

/// Birthdays, deaths and births.
///
/// **Daily**, and birthdays are spread across the year by citizen id — see
/// [`crate::citizen::CitizenRecord::birthday`]. A republic where everybody aged
/// on the same day would be a republic where a whole cohort died on the same
/// day, and a population graph with that sawtooth in it would read as a bug
/// because it would be one.
pub fn demography(world: &World) -> Vec<Mutation> {
    let today = world.clock.day_of_year();
    let day = world.clock.day_index();
    let mut aged = Vec::new();
    let mut died = Vec::new();

    // Walked unsorted: every roll below is keyed by `(citizen, day)` from its
    // own substream, so who ages and who dies does not depend on the order they
    // were considered in. Only the payload does, and a day's birthdays are a
    // few hundredths of the republic — cheaper to sort than to sort everybody
    // to find them.
    for record in world.population.walk() {
        if record.birthday() != today {
            continue;
        }
        let turning = record.age.0 + 1;
        if turning > OLDEST {
            died.push(record.id);
            continue;
        }
        let odds = mortality(turning, record.wellbeing.health);
        let mut rng = world.substream(crate::world::LIFE_STREAM, life_key(record.id, day));
        if rng.next_f64() < odds {
            died.push(record.id);
        } else {
            aged.push(record.id);
        }
    }

    // Births. A home with room in it, a couple of an age to start a family, and
    // a life worth bringing somebody into.
    let census = world.population.census_by_home();
    let mut born = Vec::new();
    let mut homes: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().residents > 0)
        .collect();
    homes.sort_by_key(|b| b.id);
    for home in homes {
        let here = census.get(&home.id).copied().unwrap_or_default();
        let pairs = here.fertile / 2;
        if pairs == 0 || here.residents >= home.def().residents {
            continue;
        }
        let content = home.content.overall();
        if content < BIRTHS_NEED {
            continue;
        }
        let odds = BIRTHS_PER_PAIR_YEAR * f64::from(pairs) * content
            / f64::from(crate::time::DAYS_PER_YEAR);
        let mut rng = world.substream(crate::world::LIFE_STREAM, birth_key(home.id, day));
        if rng.next_f64() < odds {
            born.push(home.id);
        }
    }

    let mut out = Vec::new();
    // Sorted here rather than by walking sorted, and it has to be: the applying
    // side binary-searches these.
    aged.sort_unstable();
    died.sort_unstable();
    if !aged.is_empty() {
        out.push(Mutation::Ageing { citizens: aged });
    }
    if !died.is_empty() {
        out.push(Mutation::Death { citizens: died });
    }
    if !born.is_empty() {
        out.push(Mutation::Birth { homes: born });
    }
    out
}

/// A key for one citizen's yearly roll. Keyed by the day so the same person
/// gets a different draw every birthday.
fn life_key(citizen: CitizenId, day: u64) -> u64 {
    u64::from(citizen.0)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(day)
}

fn birth_key(home: BuildingId, day: u64) -> u64 {
    u64::from(home.0)
        .wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        .wrapping_add(day)
        .wrapping_add(1)
}

fn leaving_key(citizen: CitizenId, day: u64) -> u64 {
    u64::from(citizen.0)
        .wrapping_mul(0x27D4_EB2F_1656_67C5)
        .wrapping_add(day)
        .wrapping_add(2)
}

/// People coming and people going.
///
/// **Daily.** Emigration is per person and reads loyalty; immigration is a
/// property of the whole republic and reads how content it is on average.
///
/// The asymmetry is deliberate and it is the mechanic: leaving needs nothing
/// from the republic, and arriving needs a coach, a road and somewhere to live.
pub fn migration(world: &World) -> Vec<Mutation> {
    let day = world.clock.day_index();
    let mut out = Vec::new();

    // Who has had enough. Unsorted for the reason `demography` is: the roll is
    // keyed by `(citizen, day)`, so who goes is the same whatever order they
    // were asked in.
    let mut leaving = Vec::new();
    for record in world.population.walk() {
        let loyalty = record.wellbeing.loyalty;
        if loyalty >= crate::wellbeing::LOYALTY_LEAVES {
            continue;
        }
        // Children do not emigrate on their own. They go when the republic
        // stops housing anybody, which is what the rest of this models.
        if record.age.0 < crate::citizen::WORKING_AGE.start {
            continue;
        }
        let short = (crate::wellbeing::LOYALTY_LEAVES - loyalty) / crate::wellbeing::LOYALTY_LEAVES;
        let odds = crate::wellbeing::EMIGRATION_ODDS * short;
        let mut rng = world.substream(crate::world::LIFE_STREAM, leaving_key(record.id, day));
        if rng.next_f64() < odds {
            leaving.push(record.id);
        }
    }
    if !leaving.is_empty() {
        // Ordered so the mutation is the same every run, which is what the
        // journal and the save round-trip are entitled to assume.
        leaving.sort_unstable();
        out.push(Mutation::Emigrate { citizens: leaving });
    }

    // People who stood at the border until their patience ran out.
    for group in world.migration.all() {
        if group.has_given_up(day) {
            out.push(Mutation::GiveUp { group: group.id });
        }
    }

    // And people who want in. A republic is attractive on the average of what
    // it offers the people already living in it, weighted by how many that is —
    // one wretched outpost does not cancel a working city.
    let census = world.population.census_by_home();
    let (mut scored, mut heads) = (0.0, 0u32);
    let mut centre = (0.0, 0.0);
    for home in world.buildings.all() {
        if !home.is_built() || home.def().residents == 0 {
            continue;
        }
        let here = census.get(&home.id).copied().unwrap_or_default();
        if here.residents == 0 {
            continue;
        }
        scored += home.content.overall() * f64::from(here.residents);
        heads += here.residents;
        centre.0 += home.centre.x.0 * f64::from(here.residents);
        centre.1 += home.centre.y.0 * f64::from(here.residents);
    }
    if heads == 0 {
        return out;
    }
    let average = scored / f64::from(heads);
    if average < crate::wellbeing::CONTENT_ATTRACTS {
        return out;
    }

    // Somewhere for them to live, counting everyone already on their way in.
    let spare = world
        .buildings
        .housing()
        .saturating_sub(world.population.count() as u32)
        .saturating_sub(world.migration.waiting_heads());
    if spare == 0 {
        return out;
    }

    // How keen they are: how far past the threshold the republic is, so a
    // barely-adequate republic gets a trickle and a good one a stream.
    let keenness = ((average - crate::wellbeing::CONTENT_ATTRACTS)
        / (1.0 - crate::wellbeing::CONTENT_ATTRACTS))
        .clamp(0.0, 1.0);
    let mut rng = world.substream(crate::world::LIFE_STREAM, day.wrapping_add(3));
    if rng.next_f64() >= keenness * ARRIVAL_ODDS {
        return out;
    }

    let town = Point::new(
        Metres(centre.0 / f64::from(heads)),
        Metres(centre.1 / f64::from(heads)),
    );
    let Some(post) = world.frontier.nearest_crossing(town, None) else {
        return out;
    };
    out.push(Mutation::Immigrate {
        at: post.at,
        heads: spare.min(crate::wellbeing::ARRIVAL_PARTY),
    });
    out
}

/// The daily chance a fully content republic gets a group at its border.
///
/// About one a week at the top of the scale. First-pass, and the knob to feel
/// out against the trajectory runner: too high and a republic grows faster than
/// it can build housing, too low and migration is a mechanic nobody notices.
pub const ARRIVAL_ODDS: f64 = 0.15;

/// Fetching settlers in from a frontier post.
///
/// Its own dispatcher rather than a branch of [`crews`], because the pools must
/// never compete: a republic that stopped building because its buses were
/// fetching immigrants, or that stranded a hundred people at the border because
/// a foundation wanted a gang, would have two unrelated decisions sharing one
/// budget for no reason anybody chose.
///
/// The round trip priced here is the **whole** trip — post, then estate, then
/// yard — because a coach that ran dry with two dozen people aboard is the
/// stranded-gang failure with more people in it.
pub fn settling(world: &World) -> Vec<Mutation> {
    let mut coaches = available(world, Role::Passenger);
    if coaches.is_empty() {
        return Vec::new();
    }

    // Groups a coach is already on its way to. The lesson `crews` learnt twice:
    // a journey is a commitment, and a dispatcher that only reads arrivals will
    // make it twice.
    let coming: Vec<crate::migration::GroupId> = world
        .fleet
        .all()
        .iter()
        .filter_map(|v| v.job.and_then(Job::settling))
        .map(|(group, _)| group)
        .collect();
    // And housing somebody else's coach is already filling.
    let mut booked: BTreeMap<BuildingId, u32> = BTreeMap::new();
    for (group, home) in world
        .fleet
        .all()
        .iter()
        .filter_map(|v| v.job.and_then(Job::settling))
    {
        let heads = world.migration.get(group).map_or(0, |g| g.heads);
        *booked.entry(home).or_default() += heads;
    }

    let occupants = world.population.residents_by_home();
    let crossing = world.crossing();
    let now = world.clock.ticks() as f64;
    let day = world.clock.day_index();
    let mut drawn: BTreeMap<BuildingId, Tonnes> = BTreeMap::new();
    let mut out = Vec::new();

    let waiting: Vec<(crate::migration::GroupId, Point, u32)> = world
        .migration
        .unfetched()
        .filter(|g| !coming.contains(&g.id))
        .map(|g| (g.id, g.at, g.heads))
        .collect();

    for (group, at, heads) in waiting {
        if coaches.is_empty() {
            break;
        }
        // Where they are going. The emptiest block with room for them, ties on
        // id — an intake goes where there is most room rather than filling one
        // stairwell and leaving the next estate empty.
        let mut housing: Vec<(u32, BuildingId, Point)> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().residents > 0)
            .filter_map(|b| {
                let taken = occupants.get(&b.id).copied().unwrap_or(0)
                    + booked.get(&b.id).copied().unwrap_or(0);
                let room = b.def().residents.saturating_sub(taken);
                (room > 0).then_some((room, b.id, b.centre))
            })
            .collect();
        housing.sort_by(|(ra, ia, _), (rb, ib, _)| rb.cmp(ra).then_with(|| ia.cmp(ib)));
        let Some(&(room, home, yard_of_home)) = housing.first() else {
            continue;
        };

        // The nearest coach that can make post → estate → its own yard.
        let mut nearest: Vec<(f64, usize)> = coaches
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let v = world.fleet.get(*id)?;
                Some((v.at.distance_to(at).0, i))
            })
            .collect();
        nearest.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

        for (_, index) in nearest {
            let id = coaches[index];
            let (Some(v), Some(yard)) = (
                world.fleet.get(id),
                world
                    .fleet
                    .get(id)
                    .and_then(|v| world.buildings.get(v.home))
                    .map(|b| b.centre),
            ) else {
                continue;
            };
            let def = v.def();
            let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
            let (Some(outbound), Some(carrying), Some(home_run)) = (
                leg(v.at, at),
                leg(at, yard_of_home),
                leg(yard_of_home, yard),
            ) else {
                continue;
            };
            let whole = outbound.distance() + carrying.distance() + home_run.distance();
            let held = world
                .buildings
                .get(v.home)
                .map(|b| b.stock.get(Resource::Fuel))
                .unwrap_or(Tonnes::ZERO)
                .saturating_sub(drawn.get(&v.home).copied().unwrap_or(Tonnes::ZERO));
            let top_up = def.tank.saturating_sub(v.fuel).min(held);
            if (v.fuel + top_up).0 < v.fuel_for(whole).0 {
                continue;
            }

            let stuck = sticks(world, &crossing, id, v.capability(), &outbound, 0, day);
            out.push(Mutation::Dispatch {
                vehicle: id,
                job: Job::Settle { group, to: home },
                journey: outbound,
                refuel: top_up,
            });
            if stuck {
                out.push(Mutation::Bog { vehicle: id, day });
            }
            *drawn.entry(v.home).or_default() += top_up;
            *booked.entry(home).or_default() += heads.min(room);
            coaches.remove(index);
            break;
        }
    }
    out
}

/// What a place is worth to somebody who came to look at it, `0.0..=1.0`.
///
/// Culture within walking distance, and air worth breathing. Both are read off
/// machinery that already existed rather than authored again — `serves` cover
/// and the pollution lattice — because a visitor and a resident are asking a
/// similar question about a place and should get a consistent answer.
///
/// **Weighted toward culture**, because that is the half the player builds on
/// purpose. Clean air is mostly a matter of not putting the hotel downwind of
/// the steel works, which is a siting decision rather than a construction one,
/// and the two should not be worth the same.
///
/// [`crate::tourism::APPEAL_FLOOR`] is why an empty steppe posting with a hotel
/// still earns something: a multiplier reaching zero would make the mechanic
/// unreachable until some other building existed, which is a lock wearing a
/// balance curve's clothes.
pub fn appeal(world: &World, at: Point) -> f64 {
    let culture = service_cover(world, at, crate::building::Need::Culture);
    let clean = 1.0 - world.lattice.pollution_near(at);
    let raw = 0.65 * culture + 0.35 * clean.clamp(0.0, 1.0);
    (crate::tourism::APPEAL_FLOOR + (1.0 - crate::tourism::APPEAL_FLOOR) * raw).clamp(0.0, 1.0)
}

/// Visitors turning up, spending, and going home.
///
/// **Daily**, for the reason contracts and wages are: a night in a hotel is a
/// day's takings, and a per-tick sweep would charge a party 1,440 times for one.
///
/// Three things in one pass, and they belong together: who arrives is decided by
/// how many beds are free, which is decided by who left this morning.
pub fn tourism(world: &World) -> Vec<Mutation> {
    let day = world.clock.day_index();
    let mut out = Vec::new();

    // The day's takings, and whose stay ended. One mutation carrying both,
    // because they are one transaction: a party whose fortnight ended without
    // its last day's money would be a republic that earned something and did
    // not get it.
    let mut takings: BTreeMap<Market, f64> = BTreeMap::new();
    let mut leaving: Vec<crate::tourism::VisitId> = Vec::new();
    for visit in world.tourism.all() {
        if visit.has_given_up(day) {
            leaving.push(visit.id);
            continue;
        }
        let Some(hotel) = visit.staying_at else {
            continue;
        };
        // A hotel that has lost its staff stops earning. It does not throw
        // anybody out — they are already asleep in it — but nobody is being
        // served, and a republic should not be paid for a building it cannot
        // run.
        let open = world
            .buildings
            .get(hotel)
            .is_some_and(|b| b.is_built() && b.staffing() > 0.0);
        if open {
            let spend = f64::from(visit.heads)
                * crate::tourism::SPEND_PER_HEAD_PER_DAY
                * appeal(world, visit.at);
            *takings.entry(visit.market).or_default() += spend;
        }
        if visit.is_done(day) {
            leaving.push(visit.id);
        }
    }
    for (market, amount) in takings {
        out.push(Mutation::Takings {
            market,
            amount,
            leaving: if out.is_empty() {
                std::mem::take(&mut leaving)
            } else {
                Vec::new()
            },
        });
    }
    // Nobody spent anything but somebody still went home.
    if !leaving.is_empty() {
        out.push(Mutation::Takings {
            market: Market::East,
            amount: 0.0,
            leaving,
        });
    }

    // And who turns up. Bounded by beds that will actually be free, counting
    // the parties already on their way to them — the lesson `crews` learnt
    // twice, and the reason nobody arrives for a bed somebody else has.
    let free = world.free_beds();
    if free >= PARTY_FLOOR {
        let heads = free.min(crate::tourism::PARTY);
        // Keyed by day so the stream is reproducible and does not depend on how
        // many times anything was asked.
        let roll = world
            .substream(crate::world::TOURIST_STREAM, day)
            .next_f64();
        // How attractive the republic is decides how *often* a party comes
        // rather than how large it is, so a republic with one culture club gets
        // visitors occasionally and one with a full town gets them steadily.
        let best = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().beds > 0)
            .map(|b| appeal(world, b.centre))
            .fold(0.0f64, f64::max);
        if roll < best * ARRIVALS_PER_DAY {
            // From whichever bloc holds a post, and their money is that bloc's.
            // Which posts a republic can reach is what decides whether its
            // tourism earns dollars or roubles, exactly as it decides that for
            // its coal.
            let market = if roll < best * ARRIVALS_PER_DAY * 0.5 {
                Market::West
            } else {
                Market::East
            };
            if let Some(post) = world
                .frontier
                .crossings()
                .iter()
                .filter(|c| c.bloc == market)
                .min_by_key(|c| c.id)
            {
                out.push(Mutation::Arrive {
                    at: post.at,
                    heads,
                    market,
                });
            }
        }
    }

    out
}

/// The fewest free beds worth sending a party to.
///
/// A coach's worth, near enough. A party of two would cost the same journey as
/// a party of twenty, and a republic sending a bus across the map for two
/// visitors is one whose coaches are doing nothing better — which, if true, it
/// can fix by building another hotel.
const PARTY_FLOOR: u32 = 6;

/// The chance per day that a party turns up at a republic worth visiting.
///
/// About one every four days at full appeal, so a fortnight's stay overlaps
/// several parties and a hotel is busy rather than occasionally occupied.
const ARRIVALS_PER_DAY: f64 = 0.25;

/// Fetching visitors in from a frontier post.
///
/// **Shares the coach pool with `settling`, and runs after it.** That is a
/// decision rather than an accident: both are people standing at a border
/// waiting to be driven in, a republic that has decided to move people has
/// decided it once, and which one gets the last coach is a judgement — settlers
/// first, because somebody who wants to live here outranks somebody visiting.
/// Being second in the schedule is what says so.
pub fn touring(world: &World) -> Vec<Mutation> {
    let mut coaches = available(world, Role::Passenger);
    if coaches.is_empty() {
        return Vec::new();
    }

    let coming: Vec<crate::tourism::VisitId> = world
        .fleet
        .all()
        .iter()
        .filter_map(|v| match v.job {
            Some(Job::Tour { visit, .. }) => Some(visit),
            _ => None,
        })
        .collect();

    let crossing = world.crossing();
    let now = world.clock.ticks() as f64;
    let day = world.clock.day_index();
    let mut drawn: BTreeMap<BuildingId, Tonnes> = BTreeMap::new();
    let mut booked: BTreeMap<BuildingId, u32> = BTreeMap::new();
    let mut out = Vec::new();

    let waiting: Vec<(crate::tourism::VisitId, Point, u32)> = world
        .tourism
        .unfetched()
        .filter(|v| !coming.contains(&v.id))
        .map(|v| (v.id, v.at, v.heads))
        .collect();

    for (visit, at, heads) in waiting {
        if coaches.is_empty() {
            break;
        }
        // The emptiest hotel with room, ties on id — the same ranking settling
        // uses for housing, and for the same reason.
        let mut hotels: Vec<(u32, BuildingId, Point)> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().beds > 0 && b.staffing() > 0.0)
            .filter_map(|b| {
                let taken = world.tourism.booked_at(b.id) + booked.get(&b.id).copied().unwrap_or(0);
                let room = b.def().beds.saturating_sub(taken);
                (room > 0).then_some((room, b.id, b.centre))
            })
            .collect();
        hotels.sort_by(|(ra, ia, _), (rb, ib, _)| rb.cmp(ra).then_with(|| ia.cmp(ib)));
        let Some(&(room, hotel, door)) = hotels.first() else {
            continue;
        };

        let mut nearest: Vec<(f64, usize)> = coaches
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let v = world.fleet.get(*id)?;
                Some((v.at.distance_to(at).0, i))
            })
            .collect();
        nearest.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

        for (_, index) in nearest {
            let id = coaches[index];
            let (Some(v), Some(yard)) = (
                world.fleet.get(id),
                world
                    .fleet
                    .get(id)
                    .and_then(|v| world.buildings.get(v.home))
                    .map(|b| b.centre),
            ) else {
                continue;
            };
            let def = v.def();
            let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
            let (Some(outbound), Some(carrying), Some(home_run)) =
                (leg(v.at, at), leg(at, door), leg(door, yard))
            else {
                continue;
            };
            let whole = outbound.distance() + carrying.distance() + home_run.distance();
            let held = world
                .buildings
                .get(v.home)
                .map(|b| b.stock.get(Resource::Fuel))
                .unwrap_or(Tonnes::ZERO)
                .saturating_sub(drawn.get(&v.home).copied().unwrap_or(Tonnes::ZERO));
            let top_up = def.tank.saturating_sub(v.fuel).min(held);
            if (v.fuel + top_up).0 < v.fuel_for(whole).0 {
                continue;
            }

            let stuck = sticks(world, &crossing, id, v.capability(), &outbound, 0, day);
            out.push(Mutation::Dispatch {
                vehicle: id,
                job: Job::Tour { visit, to: hotel },
                journey: outbound,
                refuel: top_up,
            });
            if stuck {
                out.push(Mutation::Bog { vehicle: id, day });
            }
            *drawn.entry(v.home).or_default() += top_up;
            *booked.entry(hotel).or_default() += heads.min(room);
            coaches.remove(index);
            break;
        }
    }
    out
}

/// How buried a stretch has to be before a plough is worth sending.
///
/// Below this the snow is a nuisance rather than a problem and the diesel is
/// better spent elsewhere — a republic that sent a machine out for a dusting
/// would spend its winter driving ploughs around empty roads.
pub const PLOUGH_AT: f64 = 0.25;

/// Pushing the winter off the roads.
///
/// **Its own dispatcher rather than a branch of `dispatch`**, for the reason
/// every other pool here has one: what it ranks is nothing like a haul. Freight
/// ranks by downtime averted, crews by the commissioning order, settling by who
/// has waited longest — and this ranks by how buried a stretch of road is and
/// how much traffic it carries, which is a question about the map rather than
/// about a consignee.
///
/// It sends a plough to the far end of the worst-buried road it can reach and
/// lets it come home again. Nothing is loaded and nothing is delivered: the work
/// happens under the wheels at every leg boundary, which is why this needs no
/// arrival machinery beyond turning the machine round. See [`swept`].
///
/// **A road is picked over open ground deliberately.** A plough could clear
/// anything, and clearing a field helps nobody: what a republic loses to snow is
/// the roads it built, and those are the thing worth the diesel.
pub fn clearing(world: &World) -> Vec<Mutation> {
    // Nothing lying, nothing to do. The cheap exit matters: this runs every
    // tick and for most of the year the answer is no.
    if world.ground.snow_load() <= 0.0 {
        return Vec::new();
    }
    let mut ploughs = available(world, crate::fleet::Role::Clearance);
    if ploughs.is_empty() {
        return Vec::new();
    }

    let crossing = world.crossing();
    let now = world.clock.ticks() as f64;
    let mut drawn: BTreeMap<BuildingId, Tonnes> = BTreeMap::new();
    let mut out = Vec::new();

    // Where a plough is already headed, so two are not sent to the same drift.
    // The lesson `crews` learnt twice, applied before it could be relearnt here.
    let mut coming: Vec<Point> = world
        .fleet
        .all()
        .iter()
        .filter_map(|v| match v.job {
            Some(Job::Plough { to }) => Some(to),
            _ => None,
        })
        .collect();

    // Every road segment, worst buried first. Ties on the segment's own ends so
    // the answer does not depend on how the network happens to be ordered.
    let mut buried: Vec<(f64, Point, Point)> = world
        .roads
        .segments()
        .iter()
        .filter_map(|segment| {
            let (from, to) = world.roads.segment_ends(segment)?;
            let cells = world.lattice.cells_along(from, to);
            if cells.is_empty() {
                return None;
            }
            let cover: f64 = cells
                .iter()
                .map(|&c| world.ground.snow_load() * (1.0 - world.lattice.cleared_at(c)))
                .sum::<f64>()
                / cells.len() as f64;
            (cover >= PLOUGH_AT).then_some((cover, from, to))
        })
        .collect();
    buried.sort_by(|(ca, fa, ta), (cb, fb, tb)| {
        cb.total_cmp(ca)
            .then_with(|| fa.x.0.total_cmp(&fb.x.0))
            .then_with(|| fa.y.0.total_cmp(&fb.y.0))
            .then_with(|| ta.x.0.total_cmp(&tb.x.0))
            .then_with(|| ta.y.0.total_cmp(&tb.y.0))
    });

    for (_, from, to) in buried {
        if ploughs.is_empty() {
            break;
        }
        // The far end from whichever plough takes it, so the machine drives the
        // whole segment rather than touching one end of it.
        if coming
            .iter()
            .any(|at| at.distance_to(to).0 < crate::ground::GROUND_CELL.0)
        {
            continue;
        }

        let mut nearest: Vec<(f64, usize)> = ploughs
            .iter()
            .enumerate()
            .filter_map(|(i, id)| {
                let v = world.fleet.get(*id)?;
                Some((v.at.distance_to(from).0, i))
            })
            .collect();
        nearest.sort_by(|(da, ia), (db, ib)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

        for (_, index) in nearest {
            let id = ploughs[index];
            let (Some(v), Some(yard)) = (
                world.fleet.get(id),
                world
                    .fleet
                    .get(id)
                    .and_then(|v| world.buildings.get(v.home))
                    .map(|b| b.centre),
            ) else {
                continue;
            };
            let def = v.def();
            let leg = |a: Point, b: Point| plan_leg(world, &crossing, def, a, b, now);
            let (Some(outbound), Some(home_run)) = (leg(v.at, to), leg(to, yard)) else {
                continue;
            };
            // The rule that does not bend: a vehicle never accepts a job it
            // cannot finish. A plough stranded in a drift is the one thing worse
            // than a buried road.
            let whole = outbound.distance() + home_run.distance();
            let held = world
                .buildings
                .get(v.home)
                .map(|b| b.stock.get(Resource::Fuel))
                .unwrap_or(Tonnes::ZERO)
                .saturating_sub(drawn.get(&v.home).copied().unwrap_or(Tonnes::ZERO));
            let top_up = def.tank.saturating_sub(v.fuel).min(held);
            if (v.fuel + top_up).0 < v.fuel_for(whole).0 {
                continue;
            }

            // No bogging roll, deliberately: see the write set. A plough is
            // above the scale by construction.
            out.push(Mutation::Dispatch {
                vehicle: id,
                job: Job::Plough { to },
                journey: outbound,
                refuel: top_up,
            });
            *drawn.entry(v.home).or_default() += top_up;
            coming.push(to);
            ploughs.remove(index);
            break;
        }
    }
    out
}

/// Goods moving along a belt or a pipe, without a lorry.
///
/// **Per tick, like the fleet**, because a belt runs continuously and the point
/// of building one is that the goods are simply *there* rather than arriving in
/// eight-tonne lumps whenever a driver is free.
///
/// The trade against the fleet is the whole mechanic. A belt needs no vehicle,
/// no driver and no diesel, and it goes exactly where it was built and nowhere
/// else — so a mine feeding one plant four hundred metres away wants a belt,
/// and a mine feeding six things scattered over a valley wants lorries.
///
/// Ranked by who is emptiest, in the same spirit as freight: a consumer with a
/// day's cover left is served before one with a week's, because what a network
/// is for is keeping things running.
pub fn belts(world: &World) -> Vec<Mutation> {
    use crate::utility::Utility;
    let day = tick_days();
    let mut out = Vec::new();

    for kind in Utility::ALL {
        if !kind.moves_goods() {
            continue;
        }
        // What each network can still move this tick. A belt is a belt however
        // many sections it has: adding a kilometre makes it longer, not wider.
        let mut left: BTreeMap<u32, f64> = BTreeMap::new();
        // What this pass has already taken out of each yard, so two consumers
        // on one belt are not both told the same tonne is theirs.
        let mut taken: BTreeMap<(BuildingId, Resource), Tonnes> = BTreeMap::new();

        for resource in kind.def().carries.iter().copied() {
            // Everyone on a network of this kind who wants this, emptiest
            // first, ties on id so the answer is reproducible.
            let mut wanting: Vec<(f64, BuildingId, u32)> = Vec::new();
            for b in world.buildings.all() {
                if !b.is_built() {
                    continue;
                }
                let wants = b.def().inputs.iter().any(|&(r, _)| r == resource);
                if !wants {
                    continue;
                }
                let Some(network) = world.utilities.network_of(b.id, kind) else {
                    continue;
                };
                let cover = cover_days(world, b.id, resource).unwrap_or(f64::INFINITY);
                wanting.push((cover, b.id, network));
            }
            wanting.sort_by(|(ca, ia, _), (cb, ib, _)| ca.total_cmp(cb).then_with(|| ia.cmp(ib)));

            for (_, consumer, network) in wanting {
                let allowance = left
                    .entry(network)
                    .or_insert_with(|| kind.def().throughput * day);
                if *allowance <= 1e-12 {
                    continue;
                }
                let Some(c) = world.buildings.get(consumer) else {
                    continue;
                };
                let room = c
                    .intake_capacity(resource)
                    .saturating_sub(c.stock.get(resource));
                if !room.is_positive() {
                    continue;
                }

                // Whoever on the same network is holding it, fullest first —
                // the opposite ranking from the consumers, and for the same
                // reason: draw down the yard that is closest to blocking.
                let mut yards: Vec<(f64, BuildingId)> = world
                    .buildings
                    .all()
                    .iter()
                    .filter(|b| b.id != consumer && b.is_built())
                    .filter(|b| world.utilities.network_of(b.id, kind) == Some(network))
                    .filter_map(|b| {
                        let held = b.stock.get(resource).saturating_sub(
                            taken.get(&(b.id, resource)).copied().unwrap_or_default(),
                        );
                        held.is_positive().then_some((held.0, b.id))
                    })
                    .collect();
                yards.sort_by(|(ha, ia), (hb, ib)| hb.total_cmp(ha).then_with(|| ia.cmp(ib)));

                for (held, from) in yards {
                    if *allowance <= 1e-12 {
                        break;
                    }
                    let moved = Tonnes(held.min(room.0).min(*allowance));
                    if !moved.is_positive() {
                        continue;
                    }
                    *allowance -= moved.0;
                    *taken.entry((from, resource)).or_default() += moved;
                    out.push(Mutation::Convey {
                        from,
                        to: consumer,
                        resource,
                        tonnes: moved,
                    });
                    break;
                }
            }
        }
    }
    out
}

/// How far a building's smoke carries.
///
/// A generous radius on a hundred-metre lattice: a steel works fouls the valley
/// it stands in rather than the square it occupies, which is what makes zoning
/// a decision rather than a formality.
pub const SMOKE_RADIUS: Metres = Metres(600.0);

/// How much of a day's emission lands on each cell within that radius, at the
/// source. Falls off with distance.
pub const SMOKE_PER_UNIT: f64 = 0.02;

/// What a fully fouled field costs a harvest.
///
/// Not total: crops grow in industrial valleys, badly. A curve that reached
/// zero would make one steel works able to make agriculture impossible, which
/// is the shape the drought floor already exists to refuse.
pub const SMOKE_YIELD_COST: f64 = 0.6;

/// How much of the dirt the weather carries off each day.
///
/// First-pass, and the knob to feel out: too fast and a steel works is free,
/// too slow and a republic can never clean anything up. A share rather than a
/// quantity — see [`crate::ground::Lattice::disperse`] — so a source settles at
/// `emission / rate` and heavy industry saturates its own valley while light
/// industry merely smudges it. About eight weeks to bring a fully fouled cell
/// back to clean with nothing adding to it.
pub const DISPERSAL_PER_DAY: f64 = 0.12;

/// What the republic throws away, and what it burns and digs.
///
/// **Daily**, because rubbish is a rate rather than an event and a per-tick
/// pass would emit 1,440 mutations to say the same thing.
///
/// Housing produces per resident and everything else per unit of activity, so
/// an idle factory throws nothing away and an empty block fills no bins. Both
/// are authored on [`crate::building::BuildingDef`] rather than matched on by
/// kind, for the reason every table in this crate is.
pub fn sanitation(world: &World) -> Vec<Mutation> {
    let residents = world.population.residents_by_home();
    let mut out = Vec::new();
    for b in world.buildings.all() {
        if !b.is_built() {
            continue;
        }
        let def = b.def();
        if def.waste <= 0.0 {
            continue;
        }
        // A block's rubbish is a function of how many people live in it, not of
        // how large it is; a works' is a function of how hard it is working.
        let made = if def.residents > 0 {
            def.waste * f64::from(residents.get(&b.id).copied().unwrap_or(0))
        } else {
            def.waste * b.activity()
        };
        if made <= 0.0 {
            continue;
        }
        out.push(Mutation::Produce {
            building: b.id,
            resource: Resource::Waste,
            tonnes: Tonnes(made),
        });
    }
    out
}

/// Smoke, and the weather taking it away.
///
/// **Daily**, on the same traversal lattice wear rides on and for the same
/// reason: what varies at ten metres is where a building can stand, and what
/// varies at a hundred is where the smoke goes.
///
/// Emission scales with activity, so a stalled works fouls nothing — which
/// means a republic that has run out of coal is at least breathing. Dispersal
/// is unconditional, which is what makes cleaning up something the player can
/// actually do rather than a state they are stuck in.
pub fn pollution(world: &World) -> Vec<Mutation> {
    let mut cells: BTreeMap<usize, f64> = BTreeMap::new();
    for b in world.buildings.all() {
        if !b.is_built() {
            continue;
        }
        let def = b.def();
        if def.pollution <= 0.0 {
            continue;
        }
        // Activity, not establishment: a works standing idle for want of ore
        // makes no smoke, and a republic can see that in the overlay.
        let running = b.activity()
            * if def.power_draw > 0.0 && !b.powered {
                0.0
            } else {
                1.0
            };
        let emitted = def.pollution * running;
        if emitted <= 0.0 {
            continue;
        }
        for cell in world.lattice.cells_within(b.centre, SMOKE_RADIUS) {
            // Inverse-square-ish falloff, floored so the far edge of the radius
            // still counts for something rather than stepping to nothing.
            let away = world.lattice.centre_of(cell).distance_to(b.centre).0 / SMOKE_RADIUS.0;
            let share = (1.0 - away).clamp(0.0, 1.0).powi(2);
            *cells.entry(cell).or_default() += emitted * SMOKE_PER_UNIT * share;
        }
    }

    let mut out = Vec::new();
    for (cell, by) in cells {
        out.push(Mutation::Foul { cell, by });
    }
    out.push(Mutation::Disperse {
        by: DISPERSAL_PER_DAY,
    });
    out
}

pub fn commissioning(world: &World) -> Vec<Mutation> {
    let mut out = Vec::new();
    for garage in world.buildings.all() {
        if !garage.is_built() {
            continue;
        }
        for &(kind, establishment) in garage.def().vehicles {
            for _ in world.fleet.count_of(garage.id, kind)..establishment {
                out.push(Mutation::Commission {
                    garage: garage.id,
                    kind,
                });
            }
        }
    }
    out
}

/// One step of the simulation.
///
/// The order below IS the simulation's definition. Labour first because
/// staffing decides everything downstream; power next because it gates
/// production; production; then freight, which moves what production made.
///
/// Freight itself runs **arrivals before departures**: a lorry that reaches its
/// garage this tick is available for work on the same tick, rather than
/// standing in the yard for a minute because the two systems happened to be
/// listed the other way round.
pub fn run_tick(world: &mut World) -> Vec<Mutation> {
    let mut all = Vec::new();

    // Labour is a DAILY system, not a per-tick one. People do not change jobs
    // every minute, and running it per tick made a simulated day cost 656 ms at
    // only 4,000 citizens — measured, not guessed. Moving it to the day
    // boundary is the difference between a model that scales to a republic and
    // one that does not.
    //
    // Contracts are daily for a different reason, and a harder one: deadlines
    // are day indices and relations decay per day, so running the sweep per
    // tick would fine a republic 1,440 times for one missed delivery.
    if world.clock.is_day_boundary() {
        // Weather first: the day's going is what everything after it reads.
        for system in [
            |w: &mut World| weather(w),
            |w: &mut World| tracks(w),
            |w: &mut World| sanitation(w),
            |w: &mut World| pollution(w),
            labour,
            |w: &mut World| contracts(w),
            // After contracts, because a default sours relations the same way a
            // missed delivery does and the order the two land in is part of the
            // simulation's definition rather than an implementation detail.
            |w: &mut World| loans(w),
            |w: &mut World| commissioning(w),
            // After loans, because both spend hard currency and which one gets
            // the last rouble is part of the simulation's definition. Wages
            // come second: an advance falling due is a deadline the republic
            // agreed to, and a day's pay is not.
            |w: &mut World| wages(w),
            // After wages, and for the same reason wages come after loans: all
            // three spend the same purse, and which one gets the last rouble is
            // part of the simulation's definition rather than an accident of
            // ordering. Your own people are paid before somebody else's firm.
            |w: &mut World| contracting(w),
            // People, after labour: contentment reads how many of a home's
            // working-age residents hold a job, and that is what the labour
            // pass has just decided.
            |w: &mut World| contentment(w),
            |w: &mut World| schooling(w),
            // Demography after contentment, because a household decides whether
            // to have a child by how the republic is treating it today.
            |w: &mut World| demography(w),
            // And migration last of the daily systems, because both halves of
            // it read state the four above have just written: loyalty for who
            // leaves, contentment for who wants to come.
            |w: &mut World| migration(w),
            // And tourism last, because who arrives depends on how many beds
            // are free, which depends on who left this morning — and because
            // it reads the pollution and culture the passes above have settled.
            |w: &mut World| tourism(w),
        ] {
            let mutations = system(world);
            apply(world, &mutations);
            all.extend(mutations);
        }
    }

    for system in [
        power,
        heating,
        construction,
        production,
        households,
        trade,
        fleet,
        // Before dispatch: goods a belt has already moved are goods no lorry
        // needs to be sent for, and running it the other way round would send
        // a lorry for a load that was about to arrive on its own.
        belts,
        dispatch,
        // Departures, after arrivals, for the reason dispatch sits where it
        // does: a bus that reached its yard this tick can be sent out again on
        // the same tick rather than standing in it for a minute because two
        // systems happened to be listed the other way round.
        crews,
        settling,
        // After settling and sharing its pool: somebody who wants to live here
        // outranks somebody visiting, and the schedule is what says so.
        touring,
        clearing,
    ] {
        let mutations = system(world);
        apply(world, &mutations);
        all.extend(mutations);
    }

    world.clock.advance();
    all
}

/// Buildings that are stalled, and the reason — the diagnostic the separate
/// limiters exist to make possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stall {
    NoStaff,
    NoPower,
    NoInputs,
}

pub fn stall_reason(world: &World, building: BuildingId) -> Option<Stall> {
    let b = world.buildings.get(building)?;
    let def = b.def();
    // Anything that makes something the republic depends on can stall, and a
    // boiler house or a bus depot makes something even though its `outputs` are
    // empty — heat and journeys are not tonnage. Leaving them out here would
    // mean a cold republic could not be asked why.
    if def.outputs.is_empty() && def.power_output <= 0.0 && def.heat_output <= 0.0 && def.seats == 0
    {
        return None;
    }
    if b.staffing() <= 0.0 {
        return Some(Stall::NoStaff);
    }
    if def.power_draw > 0.0 && !b.powered {
        return Some(Stall::NoPower);
    }
    let starved = def
        .inputs
        .iter()
        .any(|&(r, _)| !b.stock.get(r).is_positive());
    if starved {
        return Some(Stall::NoInputs);
    }
    None
}

/// Standing in a building's shoes: what it would need per day to run flat out.
pub fn nominal_input_rate(kind: BuildingKind, resource: Resource) -> Tonnes {
    Tonnes(
        kind.def()
            .inputs
            .iter()
            .find(|(r, _)| *r == resource)
            .map(|&(_, rate)| rate)
            .unwrap_or(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::Buildings;
    use crate::citizen::Population;
    use crate::climate::ClimateId;
    use crate::geology::{Deposit, DepositId, Geology, Layer, Mineral};
    use crate::network::Network;
    use crate::terrain::Terrain;
    use crate::time::TICKS_PER_DAY;
    use crate::units::{Metres, Point};
    use crate::world::{World, WorldSpec};

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    /// A bare world: flat ground, no generated geology, nothing standing. Tests
    /// place exactly what they mean to reason about.
    fn bare() -> World {
        let mut w = World::new(WorldSpec {
            seed: 1,
            extent: Metres(4_000.0),
            climate: ClimateId::Plains,
        });
        w.set_terrain(Terrain::flat(Metres(4_000.0)));
        // Bone dry. Most tests here are about an economy rather than about mud,
        // and a fixture founded in a wet March would quietly turn half of them
        // into bogging tests. The ones that *are* about the ground wet it back
        // down on purpose.
        w.ground = crate::ground::Ground {
            moisture: 0.0,
            // The root zone is set mid-band on purpose — comfortably between
            // DROUGHT_BELOW and WATERED_AT, so growing conditions contribute
            // exactly 1.0 and no economy test here quietly becomes a drought
            // test. The same reasoning as the bone-dry topsoil above, for the
            // other reservoir. Farm tests set it themselves.
            water: 0.6,
            snow: 0.0,
            frost: 0.0,
        };
        w.geology = Geology::new();
        w.buildings = Buildings::new();
        w.roads = Network::new();
        w.population = Population::new();
        w
    }

    fn coal_body(world: &mut World, centre: Point, tonnes: f64) -> DepositId {
        let id = DepositId(1);
        world.geology.insert(Deposit::new(
            id,
            Mineral::Coal,
            centre,
            Metres(200.0),
            Metres(30.0),
            vec![Layer::new(Metres(10.0), Tonnes(tonnes))],
        ));
        id
    }

    /// House enough people beside a spot to staff what is there.
    fn staff_up(world: &mut World, beside: Point, count: usize) -> BuildingId {
        let home = world
            .place_built(BuildingKind::Apartment, beside)
            .expect("housing goes up");
        for _ in 0..count {
            world.population.spawn_citizen(home, 30);
        }
        home
    }

    /// Most tests here are about running an economy, not about building one,
    /// so they put finished buildings up. Construction has its own tests.
    fn place(world: &mut World, kind: BuildingKind, at: Point) -> BuildingId {
        world.place_built(kind, at).expect("open ground")
    }

    /// String a span and energise it there and then.
    ///
    /// Most tests here are about running a republic rather than about building
    /// one, so they get their grid the way they get their buildings: finished.
    /// The construction of a line has its own tests.
    fn energise(world: &mut World, kind: crate::utility::Utility, from: Point, to: Point) {
        let id = world.order_line(kind, from, to).expect("long enough");
        let site = world.lineworks.remove(id).expect("just ordered");
        world.utilities.energise(&site);
        world.wire_up(kind);
    }

    /// A staffed transformer station: what a consumer actually plugs into.
    fn substation(world: &mut World, at: Point) -> BuildingId {
        let id = place(world, BuildingKind::TransformerStation, at);
        world.buildings.get_mut(id).expect("just placed").staff =
            BuildingKind::TransformerStation.def().workers;
        id
    }

    /// A staffed, fuelled garage with its lorries already on the strength.
    ///
    /// Freight needs a garage, drivers and diesel before it needs a plan, so
    /// the tests that are about *ranking* rather than about haulage say so once
    /// here instead of each rediscovering it.
    fn haulage(world: &mut World, at: Point) -> BuildingId {
        let garage = place(world, BuildingKind::MotorDepot, at);
        let def = BuildingKind::MotorDepot.def();
        // Drivers, housed beside the yard. Setting `staff` by hand is enough
        // for a test that never ticks, but the labour pass empties it again at
        // the first day boundary — so the people have to be real.
        staff_up(
            world,
            Point::new(at.x, at.y - Metres(200.0)),
            def.workers as usize,
        );
        if let Some(b) = world.buildings.get_mut(garage) {
            b.staff = def.workers;
            b.stock.add(Resource::Fuel, Tonnes(5.0));
            // Spares as well as diesel. A depot with an empty parts bin runs
            // half its establishment, which is the maintenance rule — and a
            // fixture that leaves them out is testing a half-crewed republic
            // while claiming to test a working one. It caught itself: a test
            // asserting a 40 t bin is worth two loads started reporting five,
            // because the heavy lorry was the one in the shed.
            b.stock.add(Resource::Machinery, Tonnes(5.0));
        }
        let arrivals = commissioning(world);
        apply(world, &arrivals);
        garage
    }

    #[test]
    fn a_mine_works_its_body_and_the_body_shrinks() {
        let mut w = bare();
        let site = at(1_000.0, 1_000.0);
        let deposit = coal_body(&mut w, site, 5_000.0);
        let mine = place(&mut w, BuildingKind::CoalMine, site);
        // Forty rather than twenty: the mine wants fourteen, the plant fifteen
        // and the transformer station three, and the station is last in the
        // commissioning order — so a short fixture leaves the one building the
        // grid runs through unmanned and the mine dark for a reason that has
        // nothing to do with the seam.
        staff_up(&mut w, at(1_200.0, 1_000.0), 40);
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_700.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(50.0));
        // The mine draws six megawatts, so it needs a grid to draw them
        // through. What is being tested is the seam, not the wiring.
        energise(
            &mut w,
            crate::utility::Utility::Power,
            at(1_700.0, 1_000.0),
            at(1_300.0, 1_000.0),
        );
        substation(&mut w, at(1_300.0, 1_000.0));

        let before = w.geology.get(deposit).unwrap().remaining();
        let plant_coal_before = w.buildings.get(plant).unwrap().stock.get(Resource::Coal);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let after = w.geology.get(deposit).unwrap().remaining();

        assert!(after < before, "the seam was not worked");

        // The chain end to end: coal comes out of the ground, freight carries
        // it to the plant that burns it, and the plant is better off than if
        // it had only been burning its opening stock. The mine's own bin is
        // near empty precisely BECAUSE logistics is working — a full bin at a
        // pithead would mean nothing was collecting it.
        let plant_coal_after = w.buildings.get(plant).unwrap().stock.get(Resource::Coal);
        let burned = Tonnes(4.0); // the plant's daily appetite
        assert!(
            plant_coal_after > plant_coal_before - burned,
            "the plant burned {plant_coal_before:?} -> {plant_coal_after:?} with no resupply"
        );
        let held: Tonnes = w
            .buildings
            .all()
            .iter()
            .map(|b| b.stock.get(Resource::Coal))
            .sum();
        assert!(held.is_positive(), "no coal anywhere in the republic");
        let _ = mine;
    }

    #[test]
    fn an_unstaffed_building_does_nothing_and_says_why() {
        let mut w = bare();
        let site = at(1_000.0, 1_000.0);
        coal_body(&mut w, site, 5_000.0);
        let mine = place(&mut w, BuildingKind::CoalMine, site);

        for _ in 0..100 {
            w.tick();
        }
        assert_eq!(
            w.buildings.get(mine).unwrap().stock.get(Resource::Coal),
            Tonnes::ZERO
        );
        assert_eq!(stall_reason(&w, mine), Some(Stall::NoStaff));
    }

    /// Separate limiters earn their keep here: a mine with people but no grid
    /// must blame the grid, not the people.
    #[test]
    fn a_staffed_building_with_no_power_blames_the_grid() {
        let mut w = bare();
        let site = at(1_000.0, 1_000.0);
        coal_body(&mut w, site, 5_000.0);
        let mine = place(&mut w, BuildingKind::CoalMine, site);
        staff_up(&mut w, at(1_200.0, 1_000.0), 20);

        w.tick();
        assert!(w.buildings.get(mine).unwrap().staffing() > 0.0);
        assert_eq!(stall_reason(&w, mine), Some(Stall::NoPower));
    }

    #[test]
    fn a_factory_starved_of_inputs_says_so_and_recovers_when_fed() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        staff_up(&mut w, at(1_100.0, 1_000.0), 10);
        w.tick();
        assert_eq!(stall_reason(&w, mill), Some(Stall::NoInputs));

        w.buildings
            .get_mut(mill)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(20.0));
        w.tick();
        assert_eq!(stall_reason(&w, mill), None);
    }

    #[test]
    fn a_factory_turns_inputs_into_outputs() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        staff_up(&mut w, at(1_100.0, 1_000.0), 10);
        w.buildings
            .get_mut(mill)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(20.0));

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let b = w.buildings.get(mill).unwrap();
        assert!(b.stock.get(Resource::Planks).is_positive(), "no planks");
        assert!(b.stock.get(Resource::Wood).0 < 20.0, "no wood consumed");
    }

    /// Advance until the air itself is not the limiting factor, so a growing
    /// test is asking about the ground and nothing else.
    fn warm_day(w: &mut World) {
        for _ in 0..(360 * TICKS_PER_DAY) {
            if w.temperature() > GROWING_WARM_C {
                return;
            }
            w.tick();
        }
        panic!("no day warm enough to grow anything in a whole year");
    }

    /// The archived rule, and the reason it is a *tax* rather than a stall: a
    /// republic that runs out of machinery limps, which is recoverable.
    #[test]
    fn a_dry_machinery_bin_halves_output_and_never_stalls_it() {
        fn planks_after_a_day(machinery: f64) -> f64 {
            let mut w = bare();
            let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
            staff_up(&mut w, at(1_100.0, 1_000.0), 10);
            let b = w.buildings.get_mut(mill).unwrap();
            b.stock.add(Resource::Wood, Tonnes(40.0));
            if machinery > 0.0 {
                b.stock.add(Resource::Machinery, Tonnes(machinery));
            }
            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            w.buildings.get(mill).unwrap().stock.get(Resource::Planks).0
        }

        let healthy = planks_after_a_day(5.0);
        let worn = planks_after_a_day(0.0);
        assert!(healthy > 0.0, "the mill made nothing even with machinery");
        assert!(
            worn > 0.0,
            "a dry bin stalled the mill instead of wearing it"
        );
        assert!(
            (worn / healthy - WORN_EFFICIENCY).abs() < 1e-6,
            "worn output was {worn} against {healthy}, ratio {}, wanted {WORN_EFFICIENCY}",
            worn / healthy
        );
    }

    /// The whole shift mechanic in one measurement: a second crew is a second
    /// day's work out of the same building, and it costs a second crew.
    ///
    /// **Both halves matter and only the pair proves anything.** Output alone
    /// would pass on a change that gave the republic free goods; labour alone
    /// would pass on one that took the people and produced nothing with them.
    #[test]
    fn a_second_shift_is_a_second_day_of_work_and_a_second_crew() {
        fn planks_and_staff(shifts: u8, hands: usize) -> (f64, u32) {
            let mut w = bare();
            let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
            staff_up(&mut w, at(1_100.0, 1_000.0), hands);
            let b = w.buildings.get_mut(mill).unwrap();
            b.stock.add(Resource::Wood, Tonnes(400.0));
            b.stock.add(Resource::Machinery, Tonnes(50.0));
            w.issue(crate::command::Command::SetShifts {
                building: mill,
                shifts,
            })
            .expect("a sawmill is a workplace");
            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            let b = w.buildings.get(mill).unwrap();
            (b.stock.get(Resource::Planks).0, b.staff)
        }

        let crew = BuildingKind::Sawmill.def().workers as usize;
        let (one, one_staff) = planks_and_staff(1, crew);
        let (two, two_staff) = planks_and_staff(2, crew * 2);
        assert_eq!(one_staff as usize, crew, "one shift wants one crew");
        assert_eq!(two_staff as usize, crew * 2, "two shifts want two");
        assert!(
            (two / one - 2.0).abs() < 1e-6,
            "two shifts made {two} against one shift's {one}"
        );

        // And the half that makes it a decision rather than a free upgrade: ask
        // for two shifts with one crew's worth of people and you get one shift's
        // output, because only one crew turned up.
        let (short, short_staff) = planks_and_staff(2, crew);
        assert_eq!(short_staff as usize, crew);
        assert!(
            (short / one - 1.0).abs() < 1e-6,
            "a half-filled double shift made {short} against {one}"
        );
    }

    /// A longer shift buys hours out of the crew already there. It is the one
    /// lever in this mechanic that costs no extra people, which is exactly why
    /// it has to cost something — see the test below for what.
    #[test]
    fn a_long_shift_buys_hours_out_of_the_crew_already_there() {
        fn planks_in_a_day(hours: f64) -> f64 {
            let mut w = bare();
            let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
            staff_up(&mut w, at(1_100.0, 1_000.0), 10);
            let b = w.buildings.get_mut(mill).unwrap();
            b.stock.add(Resource::Wood, Tonnes(400.0));
            b.stock.add(Resource::Machinery, Tonnes(50.0));
            w.issue(crate::command::Command::SetNationalShiftHours { hours })
                .expect("a rosterable day");
            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            // **One day, not a month.** A sawmill holds forty tonnes and fills
            // its bin in well under one, so a stock read at day thirty says
            // "full" whatever the roster was — which is how the first version of
            // this test reported a ratio of 1.0 and proved nothing at all.
            w.buildings.get(mill).unwrap().stock.get(Resource::Planks).0
        }

        let standard = planks_in_a_day(crate::shifts::STANDARD_HOURS);
        let long = planks_in_a_day(12.0);
        assert!(standard > 0.0, "the mill made nothing at all");
        assert!(
            (long / standard - 1.5).abs() < 1e-6,
            "a twelve-hour day made {long} against an eight-hour day's {standard}"
        );
    }

    /// And what it costs: the people standing in it.
    ///
    /// **Two crews in one republic, living in the same block**, one on a long
    /// shift and one on a standard one. That is the fixture doing real work —
    /// comparing two separate republics could not isolate this, because a mill
    /// running half again as long also makes half again as much smoke and
    /// rubbish, and those land on contentment too. Sabotaging the constant with
    /// two republics left the assertion passing on the pollution alone.
    #[test]
    fn a_long_shift_costs_the_people_who_work_it_their_loyalty() {
        let mut w = bare();
        let long = place(&mut w, BuildingKind::Sawmill, at(1_050.0, 1_000.0));
        let normal = place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_000.0));
        // One block, so both crews go home to exactly the same contentment.
        staff_up(&mut w, at(1_100.0, 1_060.0), 12);
        for id in [long, normal] {
            let b = w.buildings.get_mut(id).unwrap();
            b.stock.add(Resource::Wood, Tonnes(4_000.0));
            b.stock.add(Resource::Machinery, Tonnes(500.0));
        }
        w.issue(crate::command::Command::SetShiftHours {
            scope: crate::command::ShiftScope::Building(long),
            hours: Some(14.0),
        })
        .expect("a rosterable day");

        for _ in 0..TICKS_PER_DAY * 60 {
            w.tick();
        }

        let mean_at = |id: BuildingId| {
            let people: Vec<f64> = w
                .population
                .records()
                .iter()
                .filter(|c| c.workplace.0 == Some(id))
                .map(|c| c.wellbeing.loyalty)
                .collect();
            assert!(!people.is_empty(), "nobody ended up working at {}", id.0);
            people.iter().sum::<f64>() / people.len() as f64
        };

        let tired = mean_at(long);
        let rested = mean_at(normal);
        assert!(
            rested - tired > 0.01,
            "fourteen-hour days cost their crew nothing: {tired} against {rested} \
             at the mill next door"
        );
    }

    /// Zero shifts mothballs a workplace: it takes nobody and makes nothing.
    ///
    /// A real thing to want — a factory starving its neighbours of labour it
    /// cannot feed — and the reason the roster goes down to zero rather than
    /// stopping at one.
    #[test]
    fn a_building_nobody_is_rostered_for_takes_nobody_and_makes_nothing() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        staff_up(&mut w, at(1_100.0, 1_000.0), 10);
        w.buildings
            .get_mut(mill)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(400.0));
        w.issue(crate::command::Command::SetShifts {
            building: mill,
            shifts: 0,
        })
        .expect("closing a workplace is allowed");
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let b = w.buildings.get(mill).unwrap();
        assert_eq!(b.staff, 0, "a closed mill was staffed anyway");
        assert_eq!(
            b.stock.get(Resource::Planks),
            Tonnes::ZERO,
            "a closed mill made planks"
        );
    }

    /// A rule made today covers a building put up yesterday, and one put up
    /// tomorrow.
    ///
    /// This is the invariant the whole design rests on: `Building::hours` is the
    /// resolved answer, cached where twenty systems read it, and it can never
    /// disagree with the policy because both live in `Buildings` and every path
    /// that changes either one goes through it. The test walks every building in
    /// a played republic and checks the pair.
    #[test]
    fn every_building_agrees_with_the_rule_that_covers_it() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        let clinic = place(&mut w, BuildingKind::Clinic, at(1_400.0, 1_000.0));

        w.issue(crate::command::Command::SetNationalShiftHours { hours: 10.0 })
            .unwrap();
        w.issue(crate::command::Command::SetShiftHours {
            scope: crate::command::ShiftScope::Kind(BuildingKind::Clinic),
            hours: Some(12.0),
        })
        .unwrap();
        w.issue(crate::command::Command::SetShiftHours {
            scope: crate::command::ShiftScope::Building(mill),
            hours: Some(14.0),
        })
        .unwrap();

        // Built after every rule was made, and covered by the kind rule anyway.
        let later = place(&mut w, BuildingKind::Clinic, at(1_800.0, 1_000.0));

        assert_eq!(w.buildings.get(mill).unwrap().hours, 14.0);
        assert_eq!(w.buildings.get(clinic).unwrap().hours, 12.0);
        assert_eq!(w.buildings.get(later).unwrap().hours, 12.0);

        // Clear the building's exception and it falls back to the national
        // standard, because a sawmill has no rule of its own kind.
        w.issue(crate::command::Command::SetShiftHours {
            scope: crate::command::ShiftScope::Building(mill),
            hours: None,
        })
        .unwrap();

        let policy = w.shift_policy().clone();
        for b in w.buildings.all() {
            assert_eq!(
                b.hours,
                policy.hours_for(b.kind, b.id),
                "{:?} {} is rostered for {} against a policy of {}",
                b.kind,
                b.id.0,
                b.hours,
                policy.hours_for(b.kind, b.id)
            );
        }
    }

    /// Wear is proportional to activity, so a building nobody staffs wears
    /// nothing — the half of the rule that a fixed daily drain would get wrong.
    #[test]
    fn machines_wear_only_when_they_are_worked() {
        let mut w = bare();
        let worked = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        // Far enough from the housing that nobody can walk to it, so labour
        // leaves it empty. This is the fixture doing real work: an unstaffed
        // twin is the only way to tell "scales with activity" from "drains
        // every day".
        let idle = place(&mut w, BuildingKind::Sawmill, at(3_600.0, 3_600.0));
        staff_up(&mut w, at(1_100.0, 1_000.0), 10);
        for id in [worked, idle] {
            let b = w.buildings.get_mut(id).unwrap();
            b.stock.add(Resource::Wood, Tonnes(40.0));
            b.stock.add(Resource::Machinery, Tonnes(5.0));
        }

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }

        let worked_left = w
            .buildings
            .get(worked)
            .unwrap()
            .stock
            .get(Resource::Machinery);
        let idle_left = w
            .buildings
            .get(idle)
            .unwrap()
            .stock
            .get(Resource::Machinery);
        assert_eq!(
            w.buildings.get(idle).unwrap().staff,
            0,
            "the idle twin got staffed — the fixture is not testing what it claims"
        );
        assert!(
            worked_left.0 < 5.0,
            "a working mill wore no machinery at all"
        );
        assert!(
            (idle_left.0 - 5.0).abs() < 1e-9,
            "an idle mill wore {:.6} t of machinery",
            5.0 - idle_left.0
        );
    }

    /// Rain feeds them, frost stops them, drought withers them — the archived
    /// rule, against ground state rather than a weather word.
    ///
    /// **Nothing here consults the month, and the frozen case proves it**: this
    /// is high summer by the calendar and the answer is still zero.
    #[test]
    fn frozen_ground_grows_nothing_however_warm_the_air() {
        let mut w = bare();
        warm_day(&mut w);
        w.ground.water = 0.6;

        w.ground.frost = 0.0;
        let thawed = growing_conditions(&w);
        assert!(
            thawed > 0.0,
            "a warm, watered, unfrozen day grew nothing at all"
        );

        w.ground.frost = 1.0;
        assert_eq!(
            growing_conditions(&w),
            0.0,
            "crops grew in ground frozen solid, in the middle of summer"
        );
    }

    /// Dry farming is poor, not futile — the floor exists because the Southern
    /// Steppe sits at a measured median of 0.029 through its growing season.
    #[test]
    fn a_drought_cuts_the_harvest_without_ending_it() {
        let mut w = bare();
        warm_day(&mut w);
        w.ground.frost = 0.0;

        w.ground.water = 0.6;
        let watered = growing_conditions(&w);
        w.ground.water = 0.0;
        let parched = growing_conditions(&w);

        assert!(parched < watered, "a drought cost the harvest nothing");
        assert!(
            parched > 0.0,
            "a drought ended the harvest outright, which would make the steppe unplayable"
        );
    }

    /// Both mechanics through the real system rather than the helper: a farm
    /// that cannot grow produces nothing *and* wears nothing, because growing
    /// conditions land in `efficiency` before wear is taken.
    #[test]
    fn a_farm_that_cannot_grow_produces_nothing_and_wears_nothing() {
        let mut w = bare();
        warm_day(&mut w);
        let farm = place(&mut w, BuildingKind::Farm, at(1_000.0, 1_000.0));
        // Clear of the farm's 240 m footprint, and still inside the 2 km a
        // worker will walk.
        staff_up(&mut w, at(1_400.0, 1_000.0), 12);
        w.buildings
            .get_mut(farm)
            .unwrap()
            .stock
            .add(Resource::Machinery, Tonnes(5.0));
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(
            w.buildings.get(farm).unwrap().staff > 0,
            "the farm never got staffed — the fixture proves nothing"
        );

        // Read `production` directly: a whole tick would let the weather system
        // rewrite the ground out from under the assertion.
        w.ground.frost = 0.0;
        w.ground.water = 0.6;
        let growing = production(&w);
        assert!(
            growing.iter().any(|m| matches!(m,
                Mutation::Produce { building, resource: Resource::Crops, .. } if *building == farm)),
            "a warm watered farm proposed no crops"
        );
        assert!(
            growing.iter().any(|m| matches!(m,
                Mutation::Consume { building, resource: Resource::Machinery, .. } if *building == farm)),
            "a working farm wore no machinery"
        );

        w.ground.frost = 1.0;
        let frozen = production(&w);
        assert!(
            !frozen.iter().any(|m| matches!(m,
                Mutation::Produce { building, .. } if *building == farm)),
            "a frozen farm still produced"
        );
        assert!(
            !frozen.iter().any(|m| matches!(m,
                Mutation::Consume { building, resource: Resource::Machinery, .. } if *building == farm)),
            "a frozen farm still wore its machinery out"
        );
    }

    /// The rule the archived build documented at length: drain is what a
    /// building WANTS, not what it is getting. A dry bin must still report a
    /// finite appetite, or a starved building says it needs nothing, is never
    /// resupplied, and the outage becomes permanent.
    #[test]
    fn a_dry_building_still_reports_what_it_needs() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        assert_eq!(cover_days(&w, mill, Resource::Wood), Some(0.0));
        assert_eq!(
            nominal_input_rate(BuildingKind::Sawmill, Resource::Wood),
            Tonnes(2.0),
            "intent is a property of the building, not of its current flow"
        );
    }

    #[test]
    fn cover_days_is_stock_over_intent() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(mill)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(6.0));
        // Six tonnes against a two-tonne appetite is three days.
        assert_eq!(cover_days(&w, mill, Resource::Wood), Some(3.0));
        assert_eq!(
            cover_days(&w, mill, Resource::Coal),
            None,
            "a sawmill burns no coal"
        );
    }

    #[test]
    fn freight_reaches_the_hungriest_first() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 800.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(100.0));

        let starving = place(&mut w, BuildingKind::Sawmill, at(1_100.0, 1_000.0));
        let comfortable = place(&mut w, BuildingKind::Sawmill, at(1_200.0, 1_000.0));
        w.buildings
            .get_mut(comfortable)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(30.0));

        let jobs = dispatch(&w);
        match jobs.first().expect("something should be dispatched") {
            Mutation::Dispatch { job, .. } => {
                let (from, to, resource, _) = job.haul().expect("a haul, not a rescue");
                assert_eq!(
                    to,
                    Destination::Building(starving),
                    "the empty mill should be served first"
                );
                assert_eq!(resource, Resource::Wood);
                assert_eq!(from, store, "the wood should come from the warehouse");
            }
            other => panic!("expected a dispatch, got {other:?}"),
        }
    }

    /// A garage and a day's worth of haulage to keep it busy: wood in a
    /// warehouse at one end, mills wanting it at the other.
    fn hauling_republic() -> World {
        let mut w = bare();
        haulage(&mut w, at(400.0, 400.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(600.0, 600.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(400.0));
        for i in 0..4 {
            place(
                &mut w,
                BuildingKind::Sawmill,
                at(2_600.0 + f64::from(i) * 300.0, 2_600.0),
            );
        }
        w
    }

    /// Where the shell would draw the fleet at a given fractional tick.
    fn drawn_at(w: &World, when: f64) -> Vec<Point> {
        w.fleet
            .all()
            .iter()
            .map(|v| v.journey.as_ref().map_or(v.at, |j| j.position_at(when)))
            .collect()
    }

    /// The property the shell leans on at every speed: a vehicle's position is
    /// a pure function of its plan and the time, so *when you look* cannot
    /// change *what you see*.
    ///
    /// The six speed settings differ only in how much simulation passes between
    /// two drawn frames — paused, real time (one real second is one simulated
    /// second), then a real second worth one, two, four and eight in-game
    /// hours. Every setting is compared against real time, which samples every
    /// tick there is: wherever a faster speed looks, it must see exactly what
    /// the slowest one saw, and must leave the same world behind.
    #[test]
    fn a_journey_is_the_same_wherever_you_sample_it() {
        // Ticks of simulation per real second at each running speed.
        const SPEEDS: [u64; 5] = [1, 60, 120, 240, 480];
        // Quarter-tick offsets, standing in for the frames a renderer draws
        // between two simulation steps.
        const FRAMES: [f64; 4] = [0.0, 0.25, 0.5, 0.75];

        let run = |stride: u64| {
            let mut w = hauling_republic();
            let mut track: BTreeMap<(u64, u32), Vec<Point>> = BTreeMap::new();
            while w.clock.ticks() < TICKS_PER_DAY {
                for _ in 0..stride {
                    w.tick();
                }
                let tick = w.clock.ticks();
                for (frame, offset) in FRAMES.iter().enumerate() {
                    track.insert((tick, frame as u32), drawn_at(&w, tick as f64 + offset));
                }
            }
            (track, w)
        };

        let (real_time, reference) = run(SPEEDS[0]);
        assert!(
            real_time.values().any(|p| p != &real_time[&(1, 0)]),
            "nothing moved all day, so this compares nothing"
        );

        for stride in &SPEEDS[1..] {
            let (track, world) = run(*stride);
            for (key, seen) in &track {
                assert_eq!(
                    real_time.get(key),
                    Some(seen),
                    "at {stride} ticks a second the fleet was drawn somewhere else on tick {}",
                    key.0
                );
            }
            assert_eq!(
                world, reference,
                "the speed changed the world, not just the view of it"
            );
        }

        // Paused is the sixth setting: no ticks pass, so nothing moves however
        // long the shell keeps drawing. Sampling takes `&self`, so this holds
        // by construction — stating it is what stops that quietly changing.
        let paused = hauling_republic();
        let first = drawn_at(&paused, 12.0);
        for _ in 0..100 {
            assert_eq!(drawn_at(&paused, 12.0), first);
        }
    }

    /// The hazard the save fingerprint cannot catch on its own.
    ///
    /// Two lorries whose legs end on the same tick must be dealt with in the
    /// same order every run. The fingerprint compares *states* long after the
    /// fact; this compares the *sequence of decisions*, which is what would
    /// change first if the fleet were ever walked in an unordered structure.
    #[test]
    fn leg_completions_are_processed_in_the_same_order_every_run() {
        let record = || {
            let mut w = hauling_republic();
            let mut log: Vec<(u64, VehicleId, MutationKind)> = Vec::new();
            for _ in 0..TICKS_PER_DAY {
                for m in w.tick() {
                    let vehicle = match &m {
                        Mutation::Advance { vehicle, .. }
                        | Mutation::Load { vehicle, .. }
                        | Mutation::Unload { vehicle, .. }
                        | Mutation::Park { vehicle, .. } => *vehicle,
                        _ => continue,
                    };
                    log.push((w.clock.ticks(), vehicle, m.kind()));
                }
            }
            log
        };

        let first = record();
        assert!(
            first.len() > 20,
            "only {} leg events — this proves nothing about order",
            first.len()
        );
        assert_eq!(first, record(), "two runs dealt with the fleet differently");

        // And the order is the one that makes it reproducible: ascending by
        // vehicle within a tick, because the fleet is walked in id order.
        for pair in first.windows(2) {
            if pair[0].0 == pair[1].0 {
                assert!(
                    pair[0].1 <= pair[1].1,
                    "vehicles were handled out of order within a tick: {pair:?}"
                );
            }
        }
    }

    /// Every tonne in the republic, wherever it is standing — including the
    /// tonnes currently on a lorry, which is the whole reason this helper
    /// exists. Freight used to be conserved trivially because it never left a
    /// bin; now there is a third place tonnage can be.
    fn afloat_and_ashore(w: &World, resource: Resource) -> Tonnes {
        w.buildings
            .all()
            .iter()
            .map(|b| b.stock.get(resource))
            .sum::<Tonnes>()
            + w.fleet.cargo_afloat(resource)
    }

    /// Freight is conserved at the loading bay. What will not fit stays on the
    /// bed rather than evaporating, and comes off in the garage yard.
    #[test]
    fn cargo_that_does_not_fit_stays_on_the_lorry() {
        let mut w = bare();
        let garage = haulage(&mut w, at(1_000.0, 800.0));
        let from = place(&mut w, BuildingKind::Warehouse, at(1_000.0, 1_000.0));
        let to = place(&mut w, BuildingKind::Sawmill, at(1_100.0, 1_000.0));
        w.buildings
            .get_mut(from)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(150.0));
        // The sawmill holds 40 t; fill it to 39 so only one tonne can land.
        w.buildings
            .get_mut(to)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(39.0));

        let lorry = w.fleet.all()[0].id;
        let before = afloat_and_ashore(&w, Resource::Wood);
        let nowhere = crate::journey::plan(
            at(0.0, 0.0),
            at(1.0, 0.0),
            &w.roads,
            &w.crossing(),
            crate::network::default_road_speed(),
            crate::network::default_road_speed(),
            0.0,
        );
        apply(
            &mut w,
            &[
                Mutation::Load {
                    vehicle: lorry,
                    from,
                    resource: Resource::Wood,
                    tonnes: Tonnes(8.0),
                    journey: nowhere.clone(),
                    state: VehicleState::Delivering,
                    burn: Tonnes::ZERO,
                },
                Mutation::Unload {
                    vehicle: lorry,
                    to: Destination::Building(to),
                    resource: Resource::Wood,
                    tonnes: Tonnes(8.0),
                    journey: nowhere,
                    burn: Tonnes::ZERO,
                },
            ],
        );

        assert_eq!(
            w.buildings.get(to).unwrap().stock.get(Resource::Wood),
            Tonnes(40.0),
            "the mill takes only what its bin will hold"
        );
        assert_eq!(
            w.fleet.get(lorry).unwrap().cargo.get(Resource::Wood),
            Tonnes(7.0),
            "the rest should still be on the bed"
        );
        assert!(
            (before.0 - afloat_and_ashore(&w, Resource::Wood).0).abs() < 1e-9,
            "freight was not conserved"
        );

        // And it is tipped in the yard rather than carried around for ever.
        apply(
            &mut w,
            &[Mutation::Park {
                vehicle: lorry,
                burn: Tonnes::ZERO,
            }],
        );
        assert_eq!(
            w.buildings.get(garage).unwrap().stock.get(Resource::Wood),
            Tonnes(7.0)
        );
        assert!(w.fleet.get(lorry).unwrap().cargo.is_empty());
        assert!(
            (before.0 - afloat_and_ashore(&w, Resource::Wood).0).abs() < 1e-9,
            "freight was not conserved on the way home"
        );
    }

    /// The whole loop, driven by the clock: a lorry leaves its garage, collects,
    /// delivers and comes home — and the tonnage is conserved at every tick
    /// along the way, including while it is on the road.
    #[test]
    fn a_lorry_fetches_loads_delivers_and_comes_home() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 800.0));
        let from = place(&mut w, BuildingKind::Warehouse, at(1_000.0, 1_000.0));
        let to = place(&mut w, BuildingKind::Sawmill, at(1_400.0, 1_000.0));
        w.buildings
            .get_mut(from)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(150.0));

        let before = afloat_and_ashore(&w, Resource::Wood);
        let mut seen_laden = false;
        for _ in 0..TICKS_PER_DAY {
            w.tick();
            assert!(
                (before.0 - afloat_and_ashore(&w, Resource::Wood).0).abs() < 1e-9,
                "freight was not conserved in transit"
            );
            seen_laden |= w.fleet.cargo_afloat(Resource::Wood).is_positive();
        }

        assert!(seen_laden, "no lorry was ever carrying anything");
        assert_eq!(
            w.buildings.get(to).unwrap().stock.get(Resource::Wood),
            Tonnes(40.0),
            "the mill should have been filled"
        );
        assert!(
            w.fleet.all().iter().all(|v| v.is_idle()),
            "a lorry never got home"
        );
        // Freight costs diesel now, and the garage is where it comes from. A
        // day of haulage that burnt nothing would mean the tanks were decorative.
        let left = w
            .buildings
            .all()
            .iter()
            .map(|b| b.stock.get(Resource::Fuel))
            .sum::<Tonnes>()
            + w.fleet.all().iter().map(|v| v.fuel).sum::<Tonnes>();
        let started = Tonnes(5.0) + w.fleet.all().iter().map(|v| v.def().tank).sum::<Tonnes>();
        assert!(
            left < started,
            "a day of haulage burnt no diesel at all: {:.4} of {:.4} t",
            left.0,
            started.0
        );
    }

    #[test]
    fn a_bin_never_overfills() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        apply(
            &mut w,
            &[Mutation::Produce {
                building: mill,
                resource: Resource::Planks,
                tonnes: Tonnes(10_000.0),
            }],
        );
        assert_eq!(
            w.buildings.get(mill).unwrap().stock.get(Resource::Planks),
            Tonnes(40.0)
        );
    }

    /// A plant with nobody in it generates nothing, and the grid says so.
    #[test]
    fn an_unstaffed_plant_leaves_the_republic_dark() {
        use crate::utility::Utility;
        let mut w = bare();
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(50.0));
        let factory = place(&mut w, BuildingKind::FoodFactory, at(1_400.0, 1_000.0));
        // Strung, with a station between the two. What is being tested is the
        // crew, so the grid is given rather than built.
        energise(
            &mut w,
            Utility::Power,
            at(1_000.0, 1_000.0),
            at(1_250.0, 1_000.0),
        );
        substation(&mut w, at(1_250.0, 1_000.0));

        w.tick();
        assert!(
            !w.buildings.get(factory).unwrap().powered,
            "lit with no crew"
        );

        // Labour runs at the day boundary — people start work tomorrow, not the
        // minute their housing goes up — so a full day has to pass.
        staff_up(&mut w, at(1_180.0, 1_000.0), 30);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(
            w.buildings.get(factory).unwrap().powered,
            "a staffed, fuelled plant should carry a 4 MW load"
        );
    }

    /// The consequence of making labour daily, stated so it is a decision
    /// rather than a surprise: housing built at noon does not staff anything
    /// until the next morning.
    #[test]
    fn people_start_work_the_day_after_they_arrive() {
        let mut w = bare();
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        w.tick(); // tick 0 is a day boundary; nobody lives here yet

        staff_up(&mut w, at(1_100.0, 1_000.0), 10);
        for _ in 0..10 {
            w.tick();
        }
        assert_eq!(w.buildings.get(mill).unwrap().staff, 0, "hired mid-shift");

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(w.buildings.get(mill).unwrap().staff > 0, "never hired");
    }

    /// A plant with a crew but no fuel is the other half of that: generation
    /// needs both, and neither alone is enough.
    #[test]
    fn a_plant_with_no_fuel_generates_nothing() {
        let mut w = bare();
        place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
        let factory = place(&mut w, BuildingKind::FoodFactory, at(1_400.0, 1_000.0));
        staff_up(&mut w, at(1_180.0, 1_000.0), 30);

        w.tick();
        assert!(!w.buildings.get(factory).unwrap().powered);
    }

    /// The whole construction path: order a building, truck the materials in,
    /// crews work it, and it opens.
    #[test]
    fn a_site_becomes_a_building_once_material_and_labour_arrive() {
        let mut w = bare();
        // A construction office and the people to staff it — and a garage,
        // because materials no longer teleport to a site: somebody has to
        // drive them there.
        haulage(&mut w, at(1_000.0, 600.0));
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_000.0),
        );
        staff_up(&mut w, at(1_150.0, 1_000.0), 20);
        // A depot holding the materials a woodcutter post needs.
        let depot = place(&mut w, BuildingKind::Depot, at(1_400.0, 1_000.0));
        w.buildings
            .get_mut(depot)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(50.0));

        // Order the post. It is a site: no jobs, no output.
        let staffed_jobs = w.buildings.jobs();
        let site = w
            .buildings
            .place(
                BuildingKind::Woodcutter,
                at(1_700.0, 1_000.0),
                &w.terrain,
                &w.geology,
            )
            .expect("open ground");
        assert!(!w.buildings.get(site).unwrap().is_built());
        assert_eq!(
            w.buildings.jobs(),
            staffed_jobs,
            "a site offers no work until it opens"
        );

        for _ in 0..TICKS_PER_DAY * 20 {
            w.tick();
        }

        let post = w.buildings.get(site).unwrap();
        assert!(
            post.is_built(),
            "still {:.0}% built after twenty days",
            post.progress() * 100.0
        );
        assert_eq!(
            w.buildings.jobs(),
            staffed_jobs + BuildingKind::Woodcutter.def().workers,
            "the finished post offers its six jobs on top of what was there"
        );
    }

    /// A republic with no Construction Office builds nothing, however much
    /// material it has stockpiled. Builders are people, not a global rate.
    #[test]
    fn nothing_is_built_without_a_construction_office() {
        let mut w = bare();
        staff_up(&mut w, at(1_150.0, 1_000.0), 20);
        let site = w
            .buildings
            .place(
                BuildingKind::Woodcutter,
                at(1_700.0, 1_000.0),
                &w.terrain,
                &w.geology,
            )
            .unwrap();
        w.buildings
            .get_mut(site)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(10.0));

        for _ in 0..TICKS_PER_DAY * 30 {
            w.tick();
        }
        assert!(!w.buildings.get(site).unwrap().is_built());
        assert_eq!(w.buildings.get(site).unwrap().work_done, 0.0);
    }

    /// A half-delivered site waits. This is what makes freight priority matter
    /// during a build-out: the crew is idle until the last tonne lands.
    #[test]
    fn a_site_short_of_materials_does_not_progress() {
        let mut w = bare();
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_000.0),
        );
        staff_up(&mut w, at(1_150.0, 1_000.0), 20);
        let site = w
            .buildings
            .place(
                BuildingKind::Woodcutter,
                at(1_700.0, 1_000.0),
                &w.terrain,
                &w.geology,
            )
            .unwrap();
        // Three of the four tonnes it needs.
        w.buildings
            .get_mut(site)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(3.0));

        for _ in 0..TICKS_PER_DAY * 5 {
            w.tick();
        }
        assert_eq!(w.buildings.get(site).unwrap().work_done, 0.0);

        w.buildings
            .get_mut(site)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(1.0));
        for _ in 0..TICKS_PER_DAY * 5 {
            w.tick();
        }
        assert!(w.buildings.get(site).unwrap().work_done > 0.0);
    }

    /// Materials are consumed in step with the work, not conjured at the end.
    #[test]
    fn building_consumes_its_materials_as_it_goes() {
        let mut w = bare();
        let site = w
            .buildings
            .place(
                BuildingKind::Woodcutter,
                at(1_700.0, 1_000.0),
                &w.terrain,
                &w.geology,
            )
            .unwrap();
        w.buildings
            .get_mut(site)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(4.0));

        let labour = BuildingKind::Woodcutter.def().labour;
        apply(
            &mut w,
            &[Mutation::Build {
                site,
                builder_days: labour / 2.0,
            }],
        );
        let b = w.buildings.get(site).unwrap();
        assert!((b.progress() - 0.5).abs() < 1e-9);
        assert!(
            (b.stock.get(Resource::Planks).0 - 2.0).abs() < 1e-9,
            "half the planks should be in the fabric"
        );
    }

    /// Work cannot run past what the site needs, so a big crew on a small job
    /// does not overshoot into a building that is 300% built.
    #[test]
    fn a_finished_site_absorbs_no_more_work() {
        let mut w = bare();
        let site = w
            .buildings
            .place(
                BuildingKind::Woodcutter,
                at(1_700.0, 1_000.0),
                &w.terrain,
                &w.geology,
            )
            .unwrap();
        w.buildings
            .get_mut(site)
            .unwrap()
            .stock
            .add(Resource::Planks, Tonnes(4.0));
        apply(
            &mut w,
            &[Mutation::Build {
                site,
                builder_days: 10_000.0,
            }],
        );
        let b = w.buildings.get(site).unwrap();
        assert_eq!(b.work_done, BuildingKind::Woodcutter.def().labour);
        assert_eq!(b.progress(), 1.0);
    }

    /// Citizens eat from a shop they can walk to, and the shop's shelves go
    /// down by what they took.
    #[test]
    fn people_eat_from_the_shop_and_the_shelves_empty() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 48);
        let shop = place(&mut w, BuildingKind::Store, at(1_300.0, 1_000.0));
        w.buildings
            .get_mut(shop)
            .unwrap()
            .stock
            .add(Resource::Food, Tonnes(20.0));
        w.buildings
            .get_mut(shop)
            .unwrap()
            .stock
            .add(Resource::Clothes, Tonnes(20.0));

        let before = w.buildings.get(shop).unwrap().stock.get(Resource::Food);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let after = w.buildings.get(shop).unwrap().stock.get(Resource::Food);

        // 48 people at 0.015 t a day is 0.72 t.
        assert!(
            ((before - after).0 - 0.72).abs() < 0.05,
            "ate {:.3} t, expected about 0.72",
            (before - after).0
        );
        assert!((w.buildings.get(home).unwrap().provisioned - 1.0).abs() < 1e-6);
    }

    /// A shop out of walking range is no shop at all. This is what makes
    /// siting retail a decision rather than decoration.
    #[test]
    fn an_estate_with_no_shop_in_reach_goes_unprovisioned() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 48);
        let shop = place(&mut w, BuildingKind::Store, at(3_500.0, 1_000.0));
        w.buildings
            .get_mut(shop)
            .unwrap()
            .stock
            .add(Resource::Food, Tonnes(20.0));

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(
            w.buildings.get(home).unwrap().provisioned,
            0.0,
            "fed from a shop 2.5 km away"
        );
        // And the shop still has its stock — nobody could reach it.
        assert_eq!(
            w.buildings.get(shop).unwrap().stock.get(Resource::Food),
            Tonnes(20.0)
        );
    }

    /// An empty shop feeds nobody, and the estate reports it rather than
    /// quietly consuming from nothing.
    #[test]
    fn an_empty_shop_leaves_the_estate_hungry() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 48);
        place(&mut w, BuildingKind::Store, at(1_300.0, 1_000.0));

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.buildings.get(home).unwrap().provisioned, 0.0);
    }

    /// Two estates sharing one shop must not both be told they were fed from
    /// the same tonne. Without a scratch ledger the second estate reads a full
    /// belly from stock that had already gone.
    #[test]
    fn two_estates_cannot_eat_the_same_food() {
        let mut w = bare();
        let a = staff_up(&mut w, at(1_000.0, 1_000.0), 48);
        let b = staff_up(&mut w, at(1_000.0, 1_200.0), 48);
        let shop = place(&mut w, BuildingKind::Store, at(1_100.0, 1_100.0));
        // Households run per tick, so scarcity has to be measured per tick:
        // 48 people want 48 * 0.015 / 1440 t of food in one minute. Stock
        // exactly one estate's worth, so the second must go without.
        let one_estate_one_tick = 48.0 * FOOD_PER_CITIZEN / f64::from(TICKS_PER_DAY as u32);
        w.buildings
            .get_mut(shop)
            .unwrap()
            .stock
            .add(Resource::Food, Tonnes(one_estate_one_tick));

        let mutations = households(&w);
        apply(&mut w, &mutations);

        let fed = w.buildings.get(a).unwrap().provisioned;
        let hungry = w.buildings.get(b).unwrap().provisioned;
        assert!(
            fed > hungry,
            "both estates reported {fed} and {hungry} from one shop's stock"
        );
        assert_eq!(hungry, 0.0, "the second estate ate food that was gone");
        assert!(
            w.buildings.get(shop).unwrap().stock.get(Resource::Food).0 >= -1e-9,
            "the shop went negative"
        );
    }

    /// Freight puts food on the shelves, and it outranks a factory's inputs —
    /// a republic that stops eating is worse than one whose sawmill idles.
    #[test]
    fn freight_stocks_the_shops_before_the_factories() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 700.0));
        let depot = place(&mut w, BuildingKind::Depot, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(depot)
            .unwrap()
            .stock
            .add(Resource::Food, Tonnes(100.0));
        let shop = place(&mut w, BuildingKind::Store, at(1_200.0, 1_000.0));

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(
            w.buildings
                .get(shop)
                .unwrap()
                .stock
                .get(Resource::Food)
                .is_positive(),
            "the shop was never stocked"
        );
    }

    /// A garage takes delivery of its establishment, and only of that.
    #[test]
    fn a_garage_commissions_its_establishment_once() {
        let mut w = bare();
        let garage = haulage(&mut w, at(1_000.0, 1_000.0));
        let establishment: u32 = BuildingKind::MotorDepot
            .def()
            .vehicles
            .iter()
            .map(|&(_, n)| n)
            .sum();
        assert_eq!(w.fleet.len(), establishment as usize);
        assert!(w.fleet.all().iter().all(|v| v.home == garage));

        // A second sweep on a garage already up to strength orders nothing.
        assert!(commissioning(&w).is_empty());
        for _ in 0..TICKS_PER_DAY * 5 {
            w.tick();
        }
        assert_eq!(w.fleet.len(), establishment as usize, "the fleet bred");
    }

    /// **A vehicle never accepts a job it cannot finish.** Carried from the
    /// archived build: running dry is a refusal in the yard, not a lorry
    /// stranded in a field halfway to a mine.
    #[test]
    fn a_lorry_will_not_take_a_job_it_cannot_fuel() {
        let mut w = bare();
        let garage = haulage(&mut w, at(1_000.0, 1_000.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(1_400.0, 1_000.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(100.0));
        place(&mut w, BuildingKind::Sawmill, at(1_800.0, 1_000.0));
        assert!(!dispatch(&w).is_empty(), "a fuelled fleet should take work");

        // Now dry: nothing in the tanks and nothing in the pump.
        w.buildings
            .get_mut(garage)
            .unwrap()
            .stock
            .set(Resource::Fuel, Tonnes::ZERO);
        let ids: Vec<_> = w.fleet.all().iter().map(|v| v.id).collect();
        for id in ids {
            w.fleet.get_mut(id).unwrap().fuel = Tonnes::ZERO;
        }
        assert!(
            dispatch(&w).is_empty(),
            "a dry fleet took a job it could not finish"
        );

        // And a day of it strands nobody: the lorries are all still in the yard.
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(w.fleet.all().iter().all(|v| v.is_idle()));
    }

    /// Freight is a plan, not a stampede. A need that one lorry is already on
    /// its way to meet does not get served again on the next tick.
    #[test]
    fn one_need_gets_one_lorry() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 1_000.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(1_400.0, 1_000.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(500.0));
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_800.0, 1_000.0));

        // The mill holds 40 t and a lorry carries 20 at most, so its need is
        // worth two loads and never three.
        let mut sent = 0;
        for _ in 0..TICKS_PER_DAY {
            for m in w.tick() {
                if let Mutation::Dispatch { job, .. } = &m
                    && job
                        .haul()
                        .is_some_and(|(_, to, _, _)| to == Destination::Building(mill))
                {
                    sent += 1;
                }
            }
        }
        assert_eq!(sent, 2, "the mill was served {sent} times for one bin");
        assert_eq!(
            w.buildings.get(mill).unwrap().stock.get(Resource::Wood),
            Tonnes(40.0)
        );
    }

    /// The rule a physical fleet needed and a scalar did not: let it build up
    /// rather than sending a lorry for four kilograms — but a *site* is served
    /// whatever the quantity, or a building needing one tonne of machinery
    /// never opens.
    #[test]
    fn a_lorry_does_not_roll_for_a_trickle_but_a_site_is_always_served() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 1_000.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(1_400.0, 1_000.0));
        {
            let stock = &mut w.buildings.get_mut(store).unwrap().stock;
            stock.add(Resource::Wood, Tonnes(0.5));
            stock.add(Resource::Machinery, Tonnes(0.5));
        }
        // A running mill with an empty bin wants that wood, and wants it
        // urgently — but half a tonne is not worth a lorry, so it waits for
        // more to pile up.
        place(&mut w, BuildingKind::Sawmill, at(1_800.0, 1_000.0));
        assert!(
            cover_days(&w, w.buildings.all().last().unwrap().id, Resource::Wood)
                .is_some_and(|d| d < RESUPPLY_AT_DAYS),
            "the mill is not actually short of wood, so this tests nothing"
        );
        assert!(
            dispatch(&w).is_empty(),
            "a lorry rolled for half a tonne into a bin that can wait"
        );

        // The same half tonne, wanted by a site, goes at once: a bill of
        // materials is finite and one-off, so waiting for it to grow is waiting
        // for something that never happens.
        w.buildings
            .place(
                BuildingKind::HeatingPlant,
                at(1_000.0, 1_500.0),
                &w.terrain,
                &w.geology,
            )
            .expect("open ground");
        assert!(
            dispatch(&w)
                .iter()
                .any(|m| matches!(m, Mutation::Dispatch { job, .. }
                    if job.haul().is_some_and(|(_, _, r, _)| r == Resource::Machinery))),
            "a site waiting on a small part was left waiting"
        );
    }

    /// Nearest first, but not nearest only.
    ///
    /// Found by trajectory, not by reasoning: the republic's clothes ran out
    /// with fifty tonnes of them standing five hundred metres further away,
    /// because the depot next door had 1.99 t left — below the load minimum,
    /// and treating the closest yard as the only yard turned that into a
    /// refusal of the whole demand instead of a walk to the next one.
    #[test]
    fn a_nearly_empty_yard_next_door_does_not_block_the_full_one_further_off() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 600.0));
        let mill = place(&mut w, BuildingKind::Sawmill, at(1_000.0, 1_000.0));
        let next_door = place(&mut w, BuildingKind::Depot, at(1_200.0, 1_000.0));
        let further_off = place(&mut w, BuildingKind::Warehouse, at(1_800.0, 1_000.0));
        // Below the minimum load next door; plenty a little way further on.
        w.buildings
            .get_mut(next_door)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(1.0));
        w.buildings
            .get_mut(further_off)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(50.0));

        let jobs = dispatch(&w);
        match jobs.first() {
            Some(Mutation::Dispatch { job, .. }) => {
                let (from, to, _, _) = job.haul().expect("a haul, not a rescue");
                assert_eq!(to, Destination::Building(mill));
                assert_eq!(
                    from, further_off,
                    "the dispatcher stopped at the yard that could not fill a lorry"
                );
            }
            other => panic!("the mill was left short of wood: {other:?}"),
        }
    }

    // ---- Ground and bogging ----

    /// Turn the ground to mud and keep it there, whatever the calendar says.
    fn soak(w: &mut World) {
        w.ground = crate::ground::Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 0.0,
            frost: 0.0,
        };
    }

    /// The odds are a function of two numbers a player can be shown, and they
    /// behave the way a player would expect: nothing inside capability, worse
    /// the further past it, and never a certainty.
    #[test]
    fn the_odds_against_a_crossing_are_showable_and_sane() {
        assert_eq!(bog_chance(0.2, 0.75), 0.0, "well inside its capability");
        assert_eq!(bog_chance(0.75, 0.75), 0.0, "exactly at it");
        let a_little = bog_chance(0.85, 0.75);
        let a_lot = bog_chance(1.0, 0.75);
        assert!(a_little > 0.0 && a_little < a_lot);
        assert!(a_lot <= WORST_ODDS, "a crossing is never a certainty");
        assert_eq!(bog_chance(1.0, -5.0), WORST_ODDS, "and it is capped");
    }

    /// A loaded lorry crosses less than an empty one, and a heavy lorry loaded
    /// crosses less than a light one loaded. That ordering is the whole reason
    /// the big lorry is a road vehicle.
    #[test]
    fn a_load_costs_a_vehicle_its_footing() {
        let mut fleet = crate::fleet::Fleet::new();
        let light = fleet.commission(VehicleKind::Lorry, BuildingId(1), at(0.0, 0.0));
        let heavy = fleet.commission(VehicleKind::HeavyLorry, BuildingId(1), at(0.0, 0.0));
        let empty = fleet.get(light).unwrap().capability();
        for (id, load) in [(light, 8.0), (heavy, 20.0)] {
            fleet
                .get_mut(id)
                .unwrap()
                .cargo
                .add(Resource::Coal, Tonnes(load));
        }
        let laden = fleet.get(light).unwrap().capability();
        assert!(
            laden < empty,
            "a full lorry crosses as much as an empty one"
        );
        assert!(
            fleet.get(heavy).unwrap().capability() < laden,
            "the heavy lorry is no worse off road with a load on"
        );
        assert!(
            VehicleKind::RecoveryVehicle.def().ground > 1.0,
            "the thing sent to rescue people must not need rescuing"
        );
    }

    /// The whole mechanic end to end: a lorry sent across a soaked field sticks,
    /// and the same lorry sent across the same field frozen does not.
    #[test]
    fn a_lorry_sticks_in_the_thaw_and_crosses_the_same_field_frozen() {
        let stuck_in = |ground: crate::ground::Ground| -> usize {
            let mut w = bare();
            w.ground = ground;
            haulage(&mut w, at(600.0, 600.0));
            let store = place(&mut w, BuildingKind::Warehouse, at(900.0, 900.0));
            w.buildings
                .get_mut(store)
                .unwrap()
                .stock
                .add(Resource::Wood, Tonnes(400.0));
            for i in 0..3 {
                place(
                    &mut w,
                    BuildingKind::Sawmill,
                    at(2_600.0 + f64::from(i) * 300.0, 2_600.0),
                );
            }
            let mut bogs = 0;
            for _ in 0..TICKS_PER_DAY {
                // Hold the weather still: this is a test about the ground, not
                // about how quickly a plains April dries out.
                w.ground = ground;
                for m in w.tick() {
                    if matches!(m, Mutation::Bog { .. }) {
                        bogs += 1;
                    }
                }
            }
            bogs
        };

        let thaw = stuck_in(crate::ground::Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 0.0,
            frost: 0.0,
        });
        let midwinter = stuck_in(crate::ground::Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 60.0,
            frost: 1.0,
        });
        assert!(thaw > 0, "nothing stuck in a soaked field");
        assert_eq!(
            midwinter, 0,
            "something stuck crossing ground frozen solid — a frozen bog is a road"
        );
    }

    /// Both ways out, and neither of them a button.
    #[test]
    fn a_stuck_lorry_is_dug_out_or_towed_out_and_never_simply_forgiven() {
        let mut w = bare();
        soak(&mut w);
        haulage(&mut w, at(600.0, 600.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(900.0, 900.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(400.0));
        for i in 0..3 {
            place(
                &mut w,
                BuildingKind::Sawmill,
                at(2_600.0 + f64::from(i) * 300.0, 2_600.0),
            );
        }

        let (mut bogged, mut dug_out, mut towed) = (0, 0, 0);
        for _ in 0..TICKS_PER_DAY * 20 {
            soak(&mut w);
            for m in w.tick() {
                match m {
                    Mutation::Bog { .. } => bogged += 1,
                    Mutation::Free { .. } => dug_out += 1,
                    Mutation::Recover { .. } => towed += 1,
                    _ => {}
                }
            }
        }
        assert!(bogged > 0, "nothing ever stuck in twenty days of mud");
        assert!(
            dug_out + towed > 0,
            "{bogged} lorries stuck and not one of them ever got out"
        );
        assert!(
            towed > 0,
            "nothing was ever towed, so the recovery vehicle is decoration"
        );
        // And a tow is a real journey by a real machine, not a button: the
        // recovery vehicle burns diesel getting there.
        let tow = w
            .fleet
            .all()
            .iter()
            .find(|v| v.def().recovers())
            .expect("the garage keeps one");
        assert!(
            tow.fuel < tow.def().tank,
            "the recovery vehicle never left the yard"
        );
    }

    /// One bad crossing must not dam a supply chain: everybody else routes past
    /// the casualty as if it were not there.
    #[test]
    fn a_bogged_lorry_does_not_stop_the_ones_behind_it() {
        let mut w = bare();
        soak(&mut w);
        haulage(&mut w, at(600.0, 600.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(900.0, 900.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(400.0));
        let mills: Vec<_> = (0..3)
            .map(|i| {
                place(
                    &mut w,
                    BuildingKind::Sawmill,
                    at(2_600.0 + f64::from(i) * 300.0, 2_600.0),
                )
            })
            .collect();

        let mut ever_stuck = false;
        for _ in 0..TICKS_PER_DAY * 20 {
            soak(&mut w);
            w.tick();
            ever_stuck |= w.fleet.bogged() > 0;
        }
        assert!(
            ever_stuck,
            "nothing ever stuck, so nothing was routed round"
        );
        let delivered: Tonnes = mills
            .iter()
            .map(|&id| w.buildings.get(id).unwrap().stock.get(Resource::Wood))
            .sum();
        assert!(
            delivered.is_positive(),
            "a stuck lorry stopped the whole republic"
        );
    }

    /// Off-road routing goes round what it cannot cross, and says so honestly
    /// when there is no way round.
    #[test]
    fn a_lorry_drives_round_a_lake_and_gives_up_on_an_island() {
        use crate::terrain::Surface;
        let mut terrain = Terrain::flat(Metres(3_000.0));
        // A wall of water across the middle, with a gap at the top.
        let mut x = 0.0;
        while x < 2_400.0 {
            let mut y = 1_200.0;
            while y < 1_800.0 {
                terrain.set_surface(at(x, y), Surface::Water);
                y += 10.0;
            }
            x += 10.0;
        }
        let mut w = bare();
        w.set_terrain(terrain);
        soak(&mut w);
        let crossing = w.crossing();

        let way = crossing
            .route(at(500.0, 500.0), at(500.0, 2_500.0))
            .expect("there is a gap to go round by");
        assert!(way.len() > 2, "it went straight through the lake");
        assert!(
            way.iter().any(|p| p.x.0 > 2_400.0),
            "it did not use the gap: {way:?}"
        );

        // Now wall it off completely, and the honest answer is that there is
        // no way — not a route straight across the water.
        let mut sealed = w.terrain.clone();
        let mut y = 1_200.0;
        while y < 1_800.0 {
            let mut x = 2_400.0;
            while x < 3_000.0 {
                sealed.set_surface(at(x, y), Surface::Water);
                x += 10.0;
            }
            y += 10.0;
        }
        w.set_terrain(sealed);
        assert!(
            w.crossing()
                .route(at(500.0, 500.0), at(500.0, 2_500.0))
                .is_none(),
            "it found a way across a lake with no gap in it"
        );
    }

    // ---- Wear, and roads that grow themselves ----

    /// The acceptance scenario for the whole ground-movement model, and the one
    /// mechanic in it that nobody has to order: a remote works with no road to
    /// it, hauled to often enough that the line the lorries picked packs down,
    /// hardens, and turns up on the map as a dirt track.
    ///
    /// The second half is the half that matters. Traffic too light to keep a
    /// corridor packed must **not** grow one, or every idle line a lorry ever
    /// drove becomes a permanent road and the map fills with ghosts.
    #[test]
    fn a_road_grows_where_the_lorries_go_and_nowhere_else() {
        let track_after = |days: u64, deliveries: bool| -> (usize, usize) {
            let mut w = bare();
            haulage(&mut w, at(600.0, 600.0));
            let store = place(&mut w, BuildingKind::Warehouse, at(900.0, 600.0));
            if deliveries {
                w.buildings
                    .get_mut(store)
                    .unwrap()
                    .stock
                    .add(Resource::Wood, Tonnes(4_000.0));
            }
            // A works out on its own, well beyond the town, with nothing but
            // open field between it and its wood.
            let works = place(&mut w, BuildingKind::Sawmill, at(3_200.0, 2_600.0));

            // The most ground that was ever packed at once. Measured as it
            // happens rather than at the end, because a corridor that reaches
            // the threshold is promoted onto the map and then fades — so the
            // evidence of packing is gone by the time the road is there.
            let mut packed = 0;
            for _ in 0..TICKS_PER_DAY * days {
                w.tick();
                packed = packed.max(w.lattice.worn_beyond(crate::ground::PROMOTE_AT).len());
                // It burns what it is brought, so the haul never stops.
                if let Some(b) = w.buildings.get_mut(works) {
                    b.stock.set(Resource::Wood, Tonnes::ZERO);
                }
            }
            (w.roads.segment_count(), packed)
        };

        let (with_traffic, packed) = track_after(200, true);
        assert!(
            with_traffic > 0,
            "two hundred days of hauling across open field grew no track at all"
        );
        assert!(packed > 2, "the corridor was never packed: {packed} cells");

        // The same republic with nothing to carry lays no road, because nothing
        // drove anywhere to lay one.
        let (idle, _) = track_after(200, false);
        assert_eq!(
            idle, 0,
            "a republic with nothing to haul grew {idle} segments of road"
        );
    }

    /// The damping, without which the feedback loop runs away.
    ///
    /// Wear makes a cell cheaper, which concentrates traffic on it, which wears
    /// it further. Two things stop that diverging: packing saturates at a made
    /// track, and a corridor that reaches the threshold is promoted **out** of
    /// the lattice into the road network — after which traffic rides a road leg
    /// rather than a cross-country one, stops packing the cells, and lets them
    /// fade back.
    #[test]
    fn packing_saturates_and_a_promoted_track_stops_being_worn() {
        let mut w = bare();
        let cell = w.lattice.cell_of(at(1_000.0, 1_000.0)).expect("on the map");
        for _ in 0..500 {
            w.lattice.wear_in(cell, crate::ground::WEAR_PER_PASS);
        }
        assert_eq!(w.lattice.wear_at(cell), 1.0, "packing ran away");

        // A worn corridor, promoted, is a road — and a road is not worn.
        for offset in 0..4 {
            let c = w
                .lattice
                .cell_of(at(1_000.0 + f64::from(offset) * 100.0, 1_000.0))
                .expect("on the map");
            w.lattice.wear_in(c, 1.0);
        }
        let promotions = tracks(&w);
        apply(&mut w, &promotions);
        assert!(
            w.roads.segment_count() > 0,
            "a fully packed corridor was not put on the map"
        );

        // And it fades from here, because nothing crossing a road packs it.
        let before = w.lattice.wear_at(cell);
        for _ in 0..50 {
            apply(
                &mut w,
                &[Mutation::Fade {
                    by: crate::ground::WEAR_FADE_PER_DAY,
                }],
            );
        }
        assert!(
            w.lattice.wear_at(cell) < before,
            "a corridor nobody uses any more never gives the ground back"
        );
    }

    /// Wear is what traffic does, so what does not drive over ground does not
    /// wear it — and a road is not worn by the lorries riding it.
    #[test]
    fn tarmac_takes_no_ruts() {
        let mut w = bare();
        haulage(&mut w, at(600.0, 1_000.0));
        let store = place(&mut w, BuildingKind::Warehouse, at(900.0, 1_000.0));
        w.buildings
            .get_mut(store)
            .unwrap()
            .stock
            .add(Resource::Wood, Tonnes(2_000.0));
        let mill = place(&mut w, BuildingKind::Sawmill, at(3_400.0, 1_000.0));
        // A proper road the whole way, so every haul rides it.
        let mut previous = w.roads.add_node(at(900.0, 1_000.0));
        for i in 1..=6 {
            let next = w.roads.add_node(at(900.0 + f64::from(i) * 420.0, 1_000.0));
            w.roads
                .connect(previous, next, crate::network::default_road_speed());
            previous = next;
        }
        let segments = w.roads.segment_count();

        for _ in 0..TICKS_PER_DAY * 60 {
            w.tick();
            if let Some(b) = w.buildings.get_mut(mill) {
                b.stock.set(Resource::Wood, Tonnes::ZERO);
            }
        }
        // Ground does wear where a lorry leaves the network to reach a door —
        // that is the mechanic working. What must never wear is the ground
        // under the road itself, however many lorries ride it.
        for i in 0..6 {
            let along = 900.0 + 420.0 * (f64::from(i) + 0.5);
            let cell = w.lattice.cell_of(at(along, 1_000.0)).expect("on the map");
            assert_eq!(
                w.lattice.wear_at(cell),
                0.0,
                "the road was rutted {along:.0} m along it"
            );
        }
        assert!(
            w.roads.segment_count() >= segments,
            "the road was somehow un-built"
        );
    }

    // ---- Roads ----

    /// The whole loop, and the last free thing in the simulation being paid
    /// for: a road is ordered, gravel is *driven* out to it by the same lorries
    /// that carry everything else, the crew lay it, and only then does anything
    /// drive on it.
    #[test]
    fn a_road_is_ordered_materialled_laid_and_only_then_drivable() {
        let mut w = bare();
        haulage(&mut w, at(1_000.0, 700.0));
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_150.0),
        );
        staff_up(&mut w, at(1_000.0, 1_300.0), 20);
        let yard = place(&mut w, BuildingKind::Warehouse, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(yard)
            .unwrap()
            .stock
            .add(Resource::Gravel, Tonnes(200.0));

        let ends = (at(1_400.0, 1_000.0), at(2_400.0, 1_000.0));
        let road = w
            .order_road(ends.0, ends.1, roadworks::Grade::Gravel)
            .expect("flat open ground");

        // An ordered road is not a road. Nothing routes over it and nothing is
        // any quicker for it existing.
        assert_eq!(w.roads.segment_count(), 0, "an ordered road is drivable");
        assert!(
            w.roadworks.get(road).unwrap().material(Resource::Gravel) > Tonnes::ZERO,
            "a gravel road that needs no gravel"
        );

        let mut delivered = false;
        for _ in 0..TICKS_PER_DAY * 30 {
            w.tick();
            delivered |= w
                .roadworks
                .get(road)
                .is_some_and(|r| r.stock.get(Resource::Gravel).is_positive());
            if w.roadworks.get(road).is_none() {
                break;
            }
        }

        assert!(delivered, "no gravel was ever driven out to the site");
        assert!(
            w.roadworks.get(road).is_none(),
            "the road never opened: {:.0}% built",
            w.roadworks
                .get(road)
                .map_or(100.0, |r| r.progress() * 100.0)
        );
        // The gravel came out of a yard rather than out of nowhere.
        assert!(
            w.buildings.get(yard).unwrap().stock.get(Resource::Gravel) < Tonnes(200.0),
            "the road was surfaced with gravel nobody moved"
        );
        // And it is a road now, joined end to end. The count is not pinned here
        // because the lorries that carried the gravel will have worn tracks of
        // their own by now, which is a different mechanic doing its job — the
        // segment spacing of a laid road is pinned in `roadworks` instead.
        let a = w.roads.nearest_node(ends.0, Metres(30.0)).expect("start");
        let b = w.roads.nearest_node(ends.1, Metres(30.0)).expect("end");
        let route = w.roads.route(a, b).expect("the two ends are not joined");
        assert!(
            route.nodes.len() >= 6,
            "a kilometre should be junctioned every 200 m, got {:?}",
            route.nodes.len()
        );
    }

    /// The queue is one queue. A road ordered before a factory is laid before
    /// the factory goes up, and a road ordered after it waits its turn.
    ///
    /// What the queue decides changed with crews and the test says so: it used
    /// to ration builder-*days* between sites, and it now decides which site
    /// gets a **gang**. Same rule, firmer consequence — a crew posted to a site
    /// stays there rather than being re-divided every tick. The office is
    /// deliberately staffed for exactly one gang, because a republic that can
    /// field two gangs works two sites at once and there is no queue to observe.
    #[test]
    fn the_crew_works_roads_and_buildings_in_the_order_they_were_ordered() {
        let worked = |road_first: bool| -> (f64, f64) {
            let mut w = bare();
            place(
                &mut w,
                BuildingKind::ConstructionOffice,
                at(1_000.0, 1_000.0),
            );
            staff_up(&mut w, at(1_000.0, 1_150.0), BUILDERS_PER_SITE as usize);

            let order_the_road = |w: &mut World| {
                w.order_road(
                    at(1_500.0, 1_000.0),
                    at(2_500.0, 1_000.0),
                    // Dirt, so the only thing deciding is the queue rather than
                    // which site happened to have its gravel first.
                    roadworks::Grade::Dirt,
                )
                .expect("flat open ground")
            };
            let place_the_site = |w: &mut World| {
                let id = w
                    .buildings
                    .place(
                        BuildingKind::Warehouse,
                        at(1_000.0, 1_400.0),
                        &w.terrain,
                        &w.geology,
                    )
                    .expect("open ground");
                for &(resource, quantity) in BuildingKind::Warehouse.def().materials {
                    w.buildings
                        .get_mut(id)
                        .unwrap()
                        .stock
                        .add(resource, Tonnes(quantity));
                }
                id
            };

            let (road, site) = if road_first {
                let road = order_the_road(&mut w);
                (road, place_the_site(&mut w))
            } else {
                let site = place_the_site(&mut w);
                (order_the_road(&mut w), site)
            };

            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            (
                w.roadworks.get(road).map_or(f64::INFINITY, |r| r.work_done),
                w.buildings.get(site).unwrap().work_done,
            )
        };

        let (road, building) = worked(true);
        assert!(
            road > 0.0 && building == 0.0,
            "the road was ordered first and got {road} days against the building's {building}"
        );
        let (road, building) = worked(false);
        assert!(
            building > 0.0 && road == 0.0,
            "the building was ordered first and got {building} days against the road's {road}"
        );
    }

    /// A site with everything it needs and a crew somewhere else.
    ///
    /// The office, the people, the materials and the bus, with only the
    /// distance between them varying.
    fn building_out_at(where_: Point) -> World {
        let mut w = bare();
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_000.0),
        );
        staff_up(&mut w, at(1_000.0, 1_150.0), 20);
        let site = w
            .buildings
            .place(BuildingKind::Warehouse, where_, &w.terrain, &w.geology)
            .expect("open ground");
        for &(resource, quantity) in BuildingKind::Warehouse.def().materials {
            w.buildings
                .get_mut(site)
                .unwrap()
                .stock
                .add(resource, Tonnes(quantity));
        }
        w
    }

    fn only_site(w: &World) -> BuildingId {
        w.buildings
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::Warehouse)
            .expect("placed")
            .id
    }

    /// The whole mechanic, end to end: builders are employed at an office,
    /// carried to a site by a bus, work it, and are fetched back when it opens.
    ///
    /// This is the acceptance test for the construction rework. Before it, every
    /// office in the republic contributed to one pool of builder-days that was
    /// spent on whatever was next in the queue, wherever it stood — a crew never
    /// travelled and a remote site cost exactly what a near one cost.
    #[test]
    fn builders_are_carried_to_a_site_and_fetched_back_when_it_opens() {
        let mut w = building_out_at(at(2_200.0, 2_200.0));
        let site = only_site(&w);
        let office = w
            .buildings
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::ConstructionOffice)
            .expect("placed")
            .id;

        // Nothing is built before anybody gets there. The bus has to make the
        // journey first, and that is the point of the whole change.
        w.tick();
        assert_eq!(
            w.buildings.get(site).unwrap().work_done,
            0.0,
            "a site was worked with nobody standing on it"
        );

        let mut landed = None;
        let mut finished = None;
        let mut collected = None;
        for tick in 0..(TICKS_PER_DAY * 40) {
            w.tick();
            if landed.is_none() && w.crews.at_site(Destination::Building(site)) > 0 {
                landed = Some(tick);
            }
            if finished.is_none() && w.buildings.get(site).unwrap().is_built() {
                finished = Some(tick);
            }
            if finished.is_some() && collected.is_none() && w.crews.posted(office) == 0 {
                collected = Some(tick);
            }
        }

        let landed = landed.expect("a crew was never put on the site");
        let finished = finished.expect("the site never opened");
        let collected = collected.expect("the crew was never brought home");
        assert!(landed > 0, "the crew was on site before the bus set out");
        assert!(finished > landed, "the work happened before the crew did");
        assert!(
            collected >= finished,
            "the crew came home before the site was done"
        );
        assert_eq!(
            w.crews.len(),
            0,
            "a party outlived the job it was sent to do"
        );
    }

    /// **The change the rework exists for.** The same building, the same office
    /// and the same crew: further away is slower, because the builders have to
    /// get there.
    ///
    /// Under the old pool this was flat — a site four kilometres out drew
    /// builder-days at exactly the rate one next door did.
    #[test]
    fn a_site_further_out_takes_longer_to_start() {
        let started = |where_: Point| -> u64 {
            let mut w = building_out_at(where_);
            let site = only_site(&w);
            for tick in 0..(TICKS_PER_DAY * 10) {
                w.tick();
                if w.buildings.get(site).unwrap().work_done > 0.0 {
                    return tick;
                }
            }
            u64::MAX
        };
        let near = started(at(1_200.0, 1_100.0));
        let far = started(at(3_400.0, 3_400.0));
        assert!(near < far, "near started at {near}, far at {far}");
        assert!(far < u64::MAX, "the far site never started at all");
    }

    /// A republic with no brickworks can still build, and what it buys lands at
    /// the post rather than at the site.
    ///
    /// **The rule this must not break is "no instant build".** Auto-import
    /// answers where a tonne of brick comes from; it does not shorten a build,
    /// waive a bill or skip a journey. So the assertion is deliberately in two
    /// halves: the goods appear at the customs house, and the site is *still
    /// short of them* until a lorry has driven them over.
    #[test]
    fn imported_materials_land_at_the_post_and_still_have_to_be_driven() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let house = base.customs.expect("the founding opens a crossing");
        let post = w
            .frontier
            .nearest_crossing(w.buildings.get(house).unwrap().centre, None)
            .expect("a post to stand at")
            .id;
        let bloc = w.frontier.bloc_near(w.buildings.get(house).unwrap().centre);

        // A site needing something no yard in the republic holds.
        let site = w
            .buildings
            .place(
                BuildingKind::MachineWorks,
                Point::new(base.centre.x + Metres(400.0), base.centre.y - Metres(600.0)),
                &w.terrain,
                &w.geology,
            )
            .expect("open ground");
        let dest = Destination::Building(site);
        assert!(
            w.buildings
                .get(site)
                .unwrap()
                .material_outstanding(Resource::Machinery)
                .is_positive(),
            "the site should be short of machinery to begin with"
        );

        // Nothing is imported until a post is named, however rich the republic.
        w.treasury.credit(bloc, 100_000.0);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(
            w.buildings
                .get(house)
                .unwrap()
                .stock
                .get(Resource::Machinery),
            Tonnes::ZERO,
            "a republic imported before anybody told it to"
        );

        w.build_policy.set_global(Some(post));
        let mut landed_at_post = false;
        for _ in 0..(TICKS_PER_DAY * 3) {
            w.tick();
            if w.buildings
                .get(house)
                .unwrap()
                .stock
                .get(Resource::Machinery)
                .is_positive()
            {
                landed_at_post = true;
                break;
            }
        }
        assert!(landed_at_post, "nothing was ever bought");
        assert!(
            w.buildings
                .get(site)
                .unwrap()
                .material_outstanding(Resource::Machinery)
                .is_positive(),
            "the goods reached the site without being driven there — this is the \
             instant build the whole design refuses"
        );

        // And an opted-out site buys nothing, even under a republic that does.
        w.build_policy.set_site(dest, None);
        let before = w.treasury.of(bloc);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert!(
            w.treasury.of(bloc) >= before - 1e-9,
            "an opted-out site went on spending"
        );
    }

    /// A shortfall does not fall when the goods are *bought*; it falls when they
    /// are *delivered*, and delivery is hours of driving away.
    ///
    /// Without netting off what is already standing at the post and already on a
    /// lorry, the republic buys the same wall several times over — and the
    /// failure looks like a balance problem rather than a bug, because all it
    /// does is empty a purse.
    #[test]
    fn a_wall_is_bought_once_however_long_the_lorry_takes() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let house = base.customs.expect("a crossing");
        let post = w
            .frontier
            .nearest_crossing(w.buildings.get(house).unwrap().centre, None)
            .expect("a post")
            .id;
        let bloc = w.frontier.bloc_near(w.buildings.get(house).unwrap().centre);
        let site = w
            .buildings
            .place(
                BuildingKind::MachineWorks,
                Point::new(base.centre.x + Metres(400.0), base.centre.y - Metres(600.0)),
                &w.terrain,
                &w.geology,
            )
            .expect("open ground");
        let bill: f64 = BuildingKind::MachineWorks
            .def()
            .materials
            .iter()
            .find(|(r, _)| *r == Resource::Machinery)
            .map(|&(_, q)| q)
            .expect("a machine works needs machinery");

        w.build_policy.set_global(Some(post));
        w.treasury.credit(bloc, 100_000.0);
        let mut bought = Tonnes::ZERO;
        for _ in 0..(TICKS_PER_DAY * 20) {
            for m in w.tick() {
                if let Mutation::Import {
                    resource: Resource::Machinery,
                    tonnes,
                    ..
                } = m
                {
                    bought += tonnes;
                }
            }
        }
        assert!(
            bought.is_positive(),
            "nothing was bought, so nothing is tested"
        );
        assert!(
            bought.0 <= bill + 1e-6,
            "bought {:.1} t of machinery for a {bill:.1} t bill",
            bought.0
        );

        // And the diversion that made this necessary is real, not hypothetical:
        // the goods land in a border yard and the republic's own freight ranking
        // decides where they go. Most of this wall ended up in the Construction
        // Office, which was about to run dry and outranks a foundation.
        //
        // **That is the failure mode this change chose**, and it is the right
        // way round: a site standing still is on the screen, and hard currency
        // draining into a border post is not. Fixing it is the player's — a
        // standing trade rule, or a Machine Works of their own.
        let elsewhere = w
            .buildings
            .all()
            .iter()
            .filter(|b| b.id != site && b.kind != BuildingKind::Customs)
            .map(|b| b.stock.get(Resource::Machinery).0)
            .sum::<f64>();
        assert!(
            elsewhere > 0.0,
            "nothing was diverted, so this test is not standing where it thinks"
        );
    }

    /// Foreign builders arrive at a frontier post and have to be fetched, and
    /// the republic pays them every day it keeps them.
    ///
    /// The three claims that make this a mechanic rather than a purchase: they
    /// land at the **border** and not in the yard, they only count toward what
    /// an office can post once a bus has brought them **in**, and the wage is
    /// **ongoing** in the bloc's own currency.
    #[test]
    fn hired_builders_arrive_at_the_border_and_are_paid_every_day() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let office = base.construction_office.expect("the founding places one");
        let market = w.bloc_near(w.buildings.get(office).unwrap().centre);
        w.treasury.credit(market, 5_000.0);
        let before = w.treasury.of(market);

        let hired = 6;
        w.issue(crate::command::Command::HireForeign {
            market,
            office,
            heads: hired,
        })
        .expect("a republic with money can hire");

        // The fee is charged now, and they are standing at the post — not in
        // the yard, and not yet on anybody's books.
        assert!(
            (before - w.treasury.of(market) - f64::from(hired) * crate::crews::HIRING_FEE).abs()
                < 1e-6,
            "the placement fee was not charged"
        );
        let party = w.crews.all().last().copied().expect("a gang was created");
        assert_eq!(party.hired_from, Some(market));
        assert_eq!(w.crews.hired_total(office), 0, "they are still travelling");
        assert!(
            w.frontier.distance_from(party.at).0 <= crate::trade::CROSSING_INSET.0 + 1.0,
            "they should be standing at a frontier post, not in the yard"
        );

        // A bus goes and gets them, and only then do they count.
        let mut arrived = None;
        for tick in 0..(TICKS_PER_DAY * 20) {
            w.tick();
            if w.crews.hired_total(office) > 0 {
                arrived = Some(tick);
                break;
            }
        }
        let arrived = arrived.expect("nobody ever fetched them");
        assert!(arrived > 0, "they were on the books before the bus set out");
        assert_eq!(w.crews.hired(office, market), hired);

        // And the bill runs. A day of wages is a day of wages.
        let purse = w.treasury.of(market);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let spent = purse - w.treasury.of(market);
        assert!(
            (spent - f64::from(hired) * crate::crews::FOREIGN_WAGE).abs() < 1e-6,
            "a day cost {spent}, not {} for {hired} builders",
            f64::from(hired) * crate::crews::FOREIGN_WAGE
        );
    }

    /// A republic that cannot pay loses the workers, not a fine it cannot
    /// afford either.
    ///
    /// **The lesson loans taught, applied before it could be relearned.**
    /// `Treasury::debit` refuses to go negative, so a penalty denominated in
    /// money takes nothing from a republic that has none — which is exactly the
    /// state an unpaid wage bill describes. What an unpayable wage costs is the
    /// worker, and that bites when there is no money at all.
    #[test]
    fn builders_the_republic_cannot_pay_go_home() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let office = base.construction_office.expect("an office");
        let market = w.bloc_near(w.buildings.get(office).unwrap().centre);
        w.treasury.credit(market, 5_000.0);
        w.issue(crate::command::Command::HireForeign {
            market,
            office,
            heads: 6,
        })
        .expect("hire");
        for _ in 0..(TICKS_PER_DAY * 20) {
            w.tick();
            if w.crews.hired_total(office) > 0 {
                break;
            }
        }
        assert_eq!(w.crews.hired(office, market), 6, "they never arrived");

        // Now the purse runs dry with the gang still on the books.
        w.treasury.debit(market, w.treasury.of(market));
        let mut days = 0;
        while w.crews.hired_total(office) > 0 && days < 30 {
            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            days += 1;
        }
        assert_eq!(
            w.crews.hired_total(office),
            0,
            "a broke republic kept its foreign builders for {days} days"
        );
        assert!(
            w.treasury.of(market) >= 0.0,
            "the treasury went negative paying a wage bill it could not meet"
        );
    }

    /// One site, one gang. A crew riding toward a foundation has already been
    /// committed to it.
    ///
    /// **Found by the trajectory runner, not by reasoning.** A founded republic
    /// with a single ordered road reported twenty builders out for a site that
    /// can absorb ten: the office's second bus left on the same tick as the
    /// first, because the gang aboard the first was riding rather than working
    /// and the site still read as unmanned. A journey is a commitment, and a
    /// dispatcher that only reads arrivals makes it twice.
    #[test]
    fn one_site_is_never_sent_two_gangs() {
        let mut w = building_out_at(at(2_600.0, 2_100.0));
        let site = only_site(&w);
        let mut most = 0;
        for _ in 0..(TICKS_PER_DAY * 12) {
            w.tick();
            most = most.max(w.crews.all().len());
            if w.buildings.get(site).unwrap().is_built() {
                break;
            }
        }
        assert!(most > 0, "no gang was ever sent, so nothing was tested");
        assert_eq!(
            most, 1,
            "{most} gangs were out at once for a republic with one site"
        );
    }

    /// Heads are conserved. A builder is riding, working or waiting — never two
    /// of those, and never counted against an office twice.
    ///
    /// The invariant the whole model rests on: `posted` is what an office
    /// subtracts from its staff before it can send anybody else, so a head
    /// double-counted is a republic that cannot build and a head lost is one
    /// that builds out of nothing.
    #[test]
    fn nobody_is_in_two_places_at_once() {
        let mut w = building_out_at(at(2_400.0, 1_800.0));
        let office = w
            .buildings
            .all()
            .iter()
            .find(|b| b.kind == BuildingKind::ConstructionOffice)
            .expect("placed")
            .id;
        for _ in 0..(TICKS_PER_DAY * 30) {
            w.tick();
            let mut counted = 0;
            for party in w.crews.all() {
                assert!(
                    !(party.riding.is_some() && party.working.is_some()),
                    "a gang was aboard a bus and on a site at once: {party:?}"
                );
                assert!(party.heads > 0, "an empty gang: {party:?}");
                counted += party.heads;
            }
            assert_eq!(
                counted,
                w.crews.posted(office),
                "the office's posted count and the gangs disagree"
            );
            let staff = w.buildings.get(office).unwrap().staff;
            assert!(
                w.crews.posted(office) <= staff,
                "the office has {} out of {staff} staff",
                w.crews.posted(office)
            );
        }
    }

    /// A dry machinery bin halves the work and never stops it.
    ///
    /// The rule `WORN_EFFICIENCY` already states for every other building,
    /// applied to the plant a crew works with — and the reason it is a soft
    /// penalty is the reason it is everywhere else: limping is recoverable and
    /// stopping is not.
    #[test]
    fn an_office_with_no_machinery_builds_at_half_speed() {
        let worked = |machinery: f64| -> f64 {
            let mut w = building_out_at(at(1_300.0, 1_200.0));
            let site = only_site(&w);
            let office = w
                .buildings
                .all()
                .iter()
                .find(|b| b.kind == BuildingKind::ConstructionOffice)
                .expect("placed")
                .id;
            for _ in 0..(TICKS_PER_DAY * 2) {
                // Topped up every tick, so this measures the rate rather than
                // how long one bin lasts.
                if let Some(b) = w.buildings.get_mut(office) {
                    b.stock.add(Resource::Machinery, Tonnes(machinery));
                }
                w.tick();
            }
            w.buildings.get(site).unwrap().work_done
        };
        let plant = worked(1.0);
        let none = worked(0.0);
        assert!(plant > 0.0 && none > 0.0, "one of them did no work at all");
        assert!(
            (none / plant - WORN_EFFICIENCY).abs() < 0.02,
            "a worn office did {none} against a plant-equipped {plant}"
        );
    }

    /// A site with no materials waits, exactly as a building does — and does
    /// not quietly get laid out of nothing.
    #[test]
    fn a_road_with_no_gravel_stands_unlaid() {
        let mut w = bare();
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_000.0),
        );
        staff_up(&mut w, at(1_000.0, 1_150.0), 20);
        let road = w
            .order_road(
                at(1_500.0, 1_000.0),
                at(2_500.0, 1_000.0),
                roadworks::Grade::Gravel,
            )
            .expect("flat open ground");

        // No quarry, no warehouse, no gravel anywhere in the republic.
        for _ in 0..TICKS_PER_DAY * 20 {
            w.tick();
        }
        assert_eq!(
            w.roadworks.get(road).expect("still a site").work_done,
            0.0,
            "a gravel road was laid without gravel"
        );
        assert_eq!(w.roads.segment_count(), 0);
    }

    /// The consequence the whole ground-movement model exists to create: the
    /// same haul is quicker once there is road under it, and the difference is
    /// measured in the simulation rather than asserted about it.
    #[test]
    fn a_road_gets_freight_there_faster_than_open_ground() {
        let delivered_by = |roads: bool| -> u64 {
            let mut w = bare();
            haulage(&mut w, at(300.0, 1_000.0));
            let store = place(&mut w, BuildingKind::Warehouse, at(400.0, 1_000.0));
            w.buildings
                .get_mut(store)
                .unwrap()
                .stock
                .add(Resource::Wood, Tonnes(400.0));
            let mill = place(&mut w, BuildingKind::Sawmill, at(3_400.0, 1_000.0));
            if roads {
                let mut previous = w.roads.add_node(at(400.0, 1_000.0));
                for i in 1..=6 {
                    let next = w.roads.add_node(at(400.0 + f64::from(i) * 500.0, 1_000.0));
                    w.roads
                        .connect(previous, next, crate::network::default_road_speed());
                    previous = next;
                }
            }
            for tick in 0..TICKS_PER_DAY {
                w.tick();
                if w.buildings.get(mill).unwrap().stock.get(Resource::Wood) >= Tonnes(20.0) {
                    return tick;
                }
            }
            u64::MAX
        };

        let across_country = delivered_by(false);
        let by_road = delivered_by(true);
        assert!(
            across_country < u64::MAX && by_road < u64::MAX,
            "neither run ever delivered"
        );
        assert!(
            by_road < across_country,
            "the road bought nothing: {by_road} minutes against {across_country}"
        );
    }

    /// Only what a building is authored to sell reaches its shelves — the
    /// property lives on the def, not in a list of kinds inside this module.
    ///
    /// Four things on the counter: the two people need and the two they are
    /// glad of. Both halves are asserted, because a shop that quietly stopped
    /// stocking drink would make the comfort lift unreachable and look like a
    /// balance problem.
    #[test]
    fn only_authored_retail_sells_anything() {
        assert_eq!(
            BuildingKind::Store.def().sells,
            &[
                Resource::Food,
                Resource::Clothes,
                Resource::Alcohol,
                Resource::Electronics
            ]
        );
        assert!(
            BuildingKind::Store
                .def()
                .sells
                .iter()
                .any(|r| r.is_comfort()),
            "nothing on the shelves is a comfort, so nothing can ever lift a home"
        );
        for def in crate::building::BUILDINGS {
            if def.kind != BuildingKind::Store {
                assert!(def.sells.is_empty(), "{} sells something", def.name);
            }
        }
    }

    /// A customs house on the border, stocked with coal, sells it and the
    /// treasury fills. This is the only way money enters the republic.
    #[test]
    fn exports_earn_currency_at_the_border() {
        let mut w = bare();
        // A house goes at a frontier POST, and it clears for the bloc whose
        // post it stands at. This one sells East, so it stands at an EASTERN
        // post -- which is the mechanic, not a fixture detail: earning roubles
        // means hauling to a post the Eastern Bloc holds.
        let post = w
            .frontier
            .nearest_crossing(at(2_000.0, 2_000.0), Some(Market::East))
            .expect("a frontier always has posts of both blocs")
            .at;
        let customs = w
            .place_built(BuildingKind::Customs, post)
            .expect("at a frontier post");
        staff_up(&mut w, at(2_000.0, 2_000.0), 20);
        w.buildings
            .get_mut(customs)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(100.0));
        w.trade_policy = crate::trade::TradePolicy::new().sell(Resource::Coal, Market::East);

        assert_eq!(w.treasury.rubles, 0.0);
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }

        assert!(w.treasury.rubles > 0.0, "nothing was earned");
        assert!(
            w.buildings.get(customs).unwrap().stock.get(Resource::Coal) < Tonnes(100.0),
            "the coal never left"
        );
        // A staffed house clears 30 t a day; coal sells at 2.5 x 0.8 roubles.
        assert!(
            (w.treasury.rubles - 30.0 * 2.5 * 0.8).abs() < 1.0,
            "earned {:.2} roubles",
            w.treasury.rubles
        );
    }

    /// A customs house away from the border is not a crossing. Trade is
    /// physical, and this is where that is enforced.
    #[test]
    fn a_customs_house_must_stand_on_the_border() {
        let mut w = bare();
        let middle = at(2_000.0, 2_000.0);
        assert_eq!(
            w.place_built(BuildingKind::Customs, middle),
            Err(crate::building::PlacementError::NotOnTheBorder)
        );
    }

    /// Imports cost money, and a republic with none gets nothing. No overdraft.
    #[test]
    fn imports_stop_when_the_money_runs_out() {
        let mut w = bare();
        let on_border = w
            .frontier
            .nearest_crossing(at(2_000.0, 2_000.0), None)
            .expect("a frontier always has posts")
            .at;
        let customs = w.place_built(BuildingKind::Customs, on_border).unwrap();
        staff_up(&mut w, at(2_000.0, 2_000.0), 20);
        w.trade_policy =
            crate::trade::TradePolicy::new().buy(Resource::Machinery, Market::West, Tonnes(10.0));

        // Penniless: nothing arrives.
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(
            w.buildings
                .get(customs)
                .unwrap()
                .stock
                .get(Resource::Machinery),
            Tonnes::ZERO
        );

        // Machinery is $50/t; $100 buys two tonnes and no more.
        w.treasury.dollars = 100.0;
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        let got = w
            .buildings
            .get(customs)
            .unwrap()
            .stock
            .get(Resource::Machinery);
        assert!(
            (got.0 - 2.0).abs() < 0.01,
            "bought {got:?} on a hundred dollars"
        );
        assert!(w.treasury.dollars < 0.01, "money was left unspent");
        assert!(w.treasury.dollars >= 0.0, "the republic went overdrawn");
    }

    /// Roubles cannot buy from the west. Which market you trade with is a
    /// decision, not a detail.
    #[test]
    fn the_wrong_currency_buys_nothing() {
        let mut w = bare();
        let on_border = w
            .frontier
            .nearest_crossing(at(2_000.0, 2_000.0), None)
            .expect("a frontier always has posts")
            .at;
        let customs = w.place_built(BuildingKind::Customs, on_border).unwrap();
        staff_up(&mut w, at(2_000.0, 2_000.0), 20);
        w.treasury.rubles = 10_000.0;
        w.trade_policy =
            crate::trade::TradePolicy::new().buy(Resource::Machinery, Market::West, Tonnes(10.0));

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(
            w.buildings
                .get(customs)
                .unwrap()
                .stock
                .get(Resource::Machinery),
            Tonnes::ZERO,
            "roubles bought western machinery"
        );
        assert_eq!(w.treasury.rubles, 10_000.0);
    }

    /// An unstaffed customs house clears nothing, however good the policy.
    #[test]
    fn an_unstaffed_crossing_clears_nothing() {
        let mut w = bare();
        let on_border = w
            .frontier
            .nearest_crossing(at(2_000.0, 2_000.0), None)
            .expect("a frontier always has posts")
            .at;
        let customs = w.place_built(BuildingKind::Customs, on_border).unwrap();
        w.buildings
            .get_mut(customs)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(100.0));
        w.trade_policy = crate::trade::TradePolicy::new().sell(Resource::Coal, Market::East);

        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        assert_eq!(w.treasury.rubles, 0.0);
        assert_eq!(
            w.buildings.get(customs).unwrap().stock.get(Resource::Coal),
            Tonnes(100.0)
        );
    }

    /// Money is foreign currency only: it buys nothing domestic. A republic
    /// with a full treasury and no materials still cannot build.
    #[test]
    fn a_full_treasury_builds_nothing_on_its_own() {
        let mut w = bare();
        w.treasury.rubles = 1_000_000.0;
        w.treasury.dollars = 1_000_000.0;
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_000.0, 1_000.0),
        );
        staff_up(&mut w, at(1_150.0, 1_000.0), 20);
        let site = w
            .place(BuildingKind::Woodcutter, at(1_700.0, 1_000.0))
            .unwrap();

        for _ in 0..TICKS_PER_DAY * 10 {
            w.tick();
        }
        assert_eq!(
            w.buildings.get(site).unwrap().work_done,
            0.0,
            "currency bought domestic construction"
        );
    }

    #[test]
    fn a_tick_is_a_minute_of_a_daily_rate() {
        assert!((tick_days() - 1.0 / 1_440.0).abs() < 1e-12);
    }

    /// The whole economy, run twice, must land in the same place.
    #[test]
    fn a_running_economy_is_reproducible() {
        let build = || {
            let mut w = bare();
            let site = at(1_000.0, 1_000.0);
            coal_body(&mut w, site, 20_000.0);
            place(&mut w, BuildingKind::CoalMine, site);
            let plant = place(&mut w, BuildingKind::PowerPlant, at(1_700.0, 1_000.0));
            w.buildings
                .get_mut(plant)
                .unwrap()
                .stock
                .add(Resource::Coal, Tonnes(80.0));
            staff_up(&mut w, at(1_300.0, 1_000.0), 40);
            w
        };
        let (mut a, mut b) = (build(), build());
        for _ in 0..TICKS_PER_DAY * 5 {
            a.tick();
            b.tick();
        }
        assert_eq!(a, b);
    }

    // ---- Write sets ----

    /// Run every system against a republic busy enough to exercise all of them,
    /// and collect what each one actually emitted.
    ///
    /// A founded town, ticked past its first winter with a tender on the books
    /// and freight moving — because a write-set checked against an idle world
    /// proves nothing about a working one.
    fn observed_writes() -> BTreeMap<&'static str, std::collections::BTreeSet<MutationKind>> {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        // Enough settlers to staff everything this fixture puts down, tail
        // included. A founding is allowed to be short of people; a *guard* is
        // not, because the systems at the tail of the commissioning order are
        // then never exercised and their declarations stop being checked.
        let base = crate::scenario::town(&mut world, 240);
        // Both rules point at the bloc of the house the founding actually
        // opened, because a house clears only for the bloc whose frontier post
        // it stands at. Which post the founding picks is decided by the land,
        // so hard-coding East here exercised `Export` or `Import` depending on
        // a coin flip and left the other declaration looking like a superset
        // when it was not. The both-directions guard caught that the moment
        // trade became geographic.
        //
        // One house rather than one per bloc, and that is not tidiness. A
        // second house at the *other* bloc's post can be six kilometres away,
        // and a distant customs house generates continuous long-haul freight —
        // every dispatch of which runs a cross-country A* over the lattice.
        // Adding one took this fixture from about two seconds to over five
        // minutes. Both declarations are reachable from a single house, so the
        // cost bought nothing.
        let trading_bloc = world
            .buildings
            .of_kind(BuildingKind::Customs)
            .next()
            .map_or(Market::East, |house| world.frontier.bloc_near(house.centre));
        world.trade_policy = crate::trade::TradePolicy::new()
            .sell(Resource::Coal, trading_bloc)
            .buy(Resource::Machinery, trading_bloc, Tonnes(4.0));
        world.treasury.credit(trading_bloc, 500.0);

        // A site somebody else is building, so `contracting` has something to
        // work and something to bill for. Without one it never emits, and the
        // both-directions guard correctly calls its declaration a superset.
        //
        // Contracted to `Market::East` specifically and funded separately from
        // the trade float above: a contractor is paid in its own bloc's
        // currency, and 500 roubles buys under two builder-days. A fixture that
        // could not pay would exercise the refusal rather than the work.

        // An advance this fixture will never repay, so `loans` has a default to
        // emit. Taken through `issue` rather than written in, because that is
        // the path a player uses and a fixture that skips it stops testing the
        // command layer it depends on.
        //
        // The shortest term is 360 days and the run below is longer, which is
        // deliberate: the due day has to fall comfortably inside the run rather
        // than on its last tick, or this guard starts failing for calendar
        // reasons that have nothing to do with write sets.
        world
            .issue(crate::command::Command::TakeLoan {
                market: trading_bloc,
                tier: 0,
            })
            .expect("no advance is outstanding at founding");
        // Something under construction, so the construction system has work.
        let centre = world.buildings.all()[0].centre;
        let _ = world.place(
            BuildingKind::House,
            Point::new(centre.x, centre.y + Metres(700.0)),
        );

        // And a road out to somewhere beyond walking range, **ordered rather
        // than conjured**: the crew has to lay it before anything can use it,
        // which is what makes `construction`'s declared `Lay` reachable. A dirt
        // track because a founded republic has no gravel quarry, so anything
        // dearer would sit unbuilt for ever and prove nothing.
        //
        // Once it opens the labour pass has somewhere out of walking range to
        // send people, so seats get spent and a bus depot's fuel gets burnt —
        // without which `labour`'s declared Consume looks like a superset when
        // it is not.
        //
        // The bearing is *searched* rather than picked, because the map now has
        // rivers in it and a dirt track may not cross one. A fixture that
        // always drove due east would fail whenever a channel happened to run
        // north-south through the republic — which is the mechanic working, and
        // no reason for this test to stop being about write sets.
        let far = [
            (3_200.0, 0.0),
            (0.0, 3_200.0),
            (-3_200.0, 0.0),
            (0.0, -3_200.0),
            (2_260.0, 2_260.0),
            (-2_260.0, -2_260.0),
        ]
        .into_iter()
        .map(|(dx, dy)| Point::new(centre.x + Metres(dx), centre.y + Metres(dy)))
        .find(|&far| {
            world
                .order_road(centre, far, crate::roadworks::Grade::Dirt)
                .is_ok()
        })
        .expect("some direction out of town takes a dirt track");
        if let Some(site) =
            crate::scenario::find_site(&world, BuildingKind::Woodcutter, far, Metres(500.0))
        {
            let _ = world.buildings.place_built(
                BuildingKind::Woodcutter,
                site,
                &world.terrain,
                &world.geology,
            );
        }
        // A filling point, so `fleet`'s declared `Refuel` is reachable.
        //
        // In the TOWN rather than at the far end, which took two attempts. A
        // pump needs an attendant, and the far end is beyond walking range with
        // no bus running yet, so an unstaffed pump serves nobody and the
        // declaration went on looking like a superset.
        //
        // Stocked directly rather than supplied by freight: a pump that has to
        // be *delivered* to is another consignee, and a distant consignee is
        // exactly what took this fixture from two seconds to five minutes once
        // already.
        // Beside the GARAGE, which took three attempts and a probe to get
        // right. The pump itself was never the problem -- a probe showed it
        // built, staffed and full every time -- it was that nothing ever came
        // within reach of it. A lorry's legs end where its work is, and the one
        // place every lorry provably ends up is its own yard.
        //
        // It is also the realistic siting: you put the filling station next to
        // the depot.
        let pump_near = base
            .motor_depot
            .and_then(|id| world.buildings.get(id).map(|b| b.centre))
            .unwrap_or(centre);
        if let Some(site) =
            crate::scenario::find_site(&world, BuildingKind::GasStation, pump_near, Metres(400.0))
            && let Ok(pump) = world.buildings.place_built(
                BuildingKind::GasStation,
                site,
                &world.terrain,
                &world.geology,
            )
            && let Some(b) = world.buildings.get_mut(pump)
        {
            b.stock.add(Resource::Fuel, Tonnes(40.0));
            // And a standing order, because that is how a pump is kept filled
            // now — see `a_filling_point_is_kept_filled`. Without it this
            // fixture observes a filling station that can only run down.
            let cap = b.storage_cap();
            b.orders.set(Resource::Fuel, cap);
        }
        if let Some(site) =
            crate::scenario::find_site(&world, BuildingKind::BusDepot, centre, Metres(800.0))
            && let Ok(depot) = world.buildings.place_built(
                BuildingKind::BusDepot,
                site,
                &world.terrain,
                &world.geology,
            )
        {
            let d = world.buildings.get_mut(depot).expect("just placed");
            d.staff = BuildingKind::BusDepot.def().workers;
            d.stock.add(Resource::Fuel, Tonnes(40.0));
            // Spares as well as diesel, or the depot runs half its coaches --
            // and this fixture exists to reach a bogging roll that a *coach*
            // makes on the way to a frontier post, measured at roughly one in
            // a hundred and thirty journeys. Halving the coaches was enough to
            // stop it firing at all, and the write-set guard's floor half said
            // so within a minute of maintenance existing.
            d.stock.add(Resource::Machinery, Tonnes(20.0));
        }

        // Empty housing, so the republic has somewhere to put people it
        // attracts. Without spare room nobody is offered a place, and `Board`
        // and `Settle` go unreached — which is the state a declaration stops
        // constraining anything in.
        //
        // A founding puts up three blocks and this fixture fills them past
        // capacity with 240 settlers, so this is not decoration: it is the
        // difference between a republic that can grow and one that cannot.
        for i in 0..5 {
            let want = Point::new(
                centre.x - Metres(400.0) - Metres(f64::from(i) * 120.0),
                centre.y + Metres(600.0),
            );
            if let Some(site) =
                crate::scenario::find_site(&world, BuildingKind::Apartment, want, Metres(700.0))
            {
                let _ = world.buildings.place_built(
                    BuildingKind::Apartment,
                    site,
                    &world.terrain,
                    &world.geology,
                );
            }
        }

        // A refinery in town and its crude four kilometres out, joined to the
        // road above by a spur at each end.
        //
        // This is here for `Advance`, and it took a measurement to work out
        // why it was needed. Every haul inside a founded town is a straight
        // line across open ground — one leg, no waypoints, so no `Advance`
        // ever — and simply *having* a road is not enough: freight only rides
        // it when both ends are within `ROAD_ACCESS` of a junction and the
        // detour is quicker than driving direct. Nothing else in the republic
        // holds oil, so this is the one haul that must run the length of the
        // network, which is what makes the declared write reachable at all.
        let refinery = crate::scenario::find_site(
            &world,
            BuildingKind::Refinery,
            Point::new(centre.x - Metres(700.0), centre.y - Metres(700.0)),
            Metres(1_200.0),
        )
        .and_then(|site| {
            world
                .buildings
                .place_built(BuildingKind::Refinery, site, &world.terrain, &world.geology)
                .ok()
        });
        let oilfield =
            crate::scenario::find_site(&world, BuildingKind::Warehouse, far, Metres(600.0))
                .and_then(|site| {
                    world
                        .buildings
                        .place_built(
                            BuildingKind::Warehouse,
                            site,
                            &world.terrain,
                            &world.geology,
                        )
                        .ok()
                });
        for (end, join) in [(refinery, centre), (oilfield, far)] {
            let Some(end) = end else { continue };
            let yard = world.buildings.get(end).expect("just placed").centre;
            // A spur onto the main road, ordered like the road itself. Both
            // ends of a spur land on a junction the main road will lay, so the
            // three roads become one network rather than three islands.
            let _ = world.order_road(yard, join, crate::roadworks::Grade::Dirt);
        }

        // A power line ordered and left to the crew, so `construction`'s
        // declared `String` is reachable. Ordered rather than energised: what
        // is being watched is the crew stringing it and the steel being driven
        // out, which is the whole reason a line is a site.
        // Its steel goes straight in rather than being driven out, because a
        // founded republic has no steel mill and never will inside this run —
        // the site would sit unbuilt for ever and prove nothing. What is being
        // reached is the stringing, not the supply chain.
        if let Ok(crate::command::Done::Strung(line)) =
            world.issue(crate::command::Command::OrderLine {
                kind: crate::utility::Utility::Power,
                from: centre,
                to: Point::new(centre.x + Metres(900.0), centre.y - Metres(300.0)),
            })
        {
            let bill = world.lineworks.get(line).map(|l| l.materials());
            if let (Some(bill), Some(site)) = (bill, world.lineworks.get_mut(line)) {
                for (resource, quantity) in bill {
                    site.stock.add(resource, quantity);
                }
            }
        }

        // A belt from the mine to the plant, energised on the spot. What is
        // being watched here is `Convey` rather than the stringing, which the
        // power line above already covers.
        if let (Some(mine), Some(plant)) = (base.mine, base.plant) {
            let ends = (
                world.buildings.get(mine).map(|b| b.centre),
                world.buildings.get(plant).map(|b| b.centre),
            );
            if let (Some(a), Some(b)) = ends
                && let Ok(id) = world.order_line(crate::utility::Utility::Conveyor, a, b)
                && let Some(site) = world.lineworks_mut().remove(id)
            {
                world.energise_now(&site);
            }
        }

        // A school, and children to fill it.
        //
        // The children are put there directly, and that is not laziness: a
        // child born inside this fixture is nought years old when it ends, and
        // school starts at six. Waiting for one would mean simulating twenty
        // republic-years to check a declaration. What is being reached is the
        // schooling pass, and it needs a pupil rather than a plausible
        // biography.
        if let Some(site) =
            crate::scenario::find_site(&world, BuildingKind::School, centre, Metres(600.0))
            && let Ok(school) = world.buildings.place_built(
                BuildingKind::School,
                site,
                &world.terrain,
                &world.geology,
            )
        {
            let _ = school;
            if let Some(home) = base.housing.first() {
                for age in [7, 9, 11, 13] {
                    world.population.spawn_citizen(*home, age);
                }
            }
        }

        // A hotel and a culture club, so `tourism` and `touring` have a whole
        // path to run: visitors only arrive where there are beds, and what they
        // pay for is what is near them. Both in town, because a hotel out at the
        // far end would be unstaffed and an unstaffed hotel takes nobody.
        for kind in [BuildingKind::Hotel, BuildingKind::CultureClub] {
            if let Some(site) = crate::scenario::find_site(&world, kind, centre, Metres(600.0)) {
                let _ = world
                    .buildings
                    .place_built(kind, site, &world.terrain, &world.geology);
            }
        }

        // A foreign gang on the books, so the wage bill is a thing that
        // happens. Issued as a command rather than written in, because the
        // whole path — fee, arrival at a post, the bus that fetches them — is
        // what this guard is watching.
        if let Some(office) = base.construction_office {
            let market = world.bloc_near(centre);
            world.treasury.credit(market, 4_000.0);
            let _ = world.issue(crate::command::Command::HireForeign {
                market,
                office,
                heads: 4,
            });
        }

        let mut seen: BTreeMap<&'static str, std::collections::BTreeSet<MutationKind>> =
            BTreeMap::new();
        let mut note = |name: &'static str, mutations: &[Mutation]| {
            let entry = seen.entry(name).or_default();
            for m in mutations {
                entry.insert(m.kind());
            }
        };

        // A year, so a winter and several tender cycles pass.
        // NOTE: this loop is a second copy of `run_tick`'s schedule, which is
        // the one thing about this guard that is not self-maintaining. A system
        // added to `run_tick` and not here goes unwatched — caught, but only
        // indirectly, by `every_system_and_every_mutation_kind_is_accounted_for`
        // demanding every system be declared and every declaration be reached.
        //
        // Longer than the shortest loan term, so the advance taken above comes
        // due well inside the run. See the note where it is taken.
        for day in 0..400u32 {
            for _ in 0..TICKS_PER_DAY {
                if world.clock.is_day_boundary() {
                    let m = labour(&mut world);
                    note("labour", &m);
                    apply(&mut world, &m);
                    let m = contracts(&world);
                    note("contracts", &m);
                    apply(&mut world, &m);
                    let m = loans(&world);
                    note("loans", &m);
                    apply(&mut world, &m);
                    let m = commissioning(&world);
                    note("commissioning", &m);
                    apply(&mut world, &m);
                    let m = wages(&world);
                    note("wages", &m);
                    apply(&mut world, &m);
                    let m = contracting(&world);
                    note("contracting", &m);
                    apply(&mut world, &m);
                    let m = weather(&world);
                    note("weather", &m);
                    apply(&mut world, &m);
                    let m = tracks(&world);
                    note("tracks", &m);
                    apply(&mut world, &m);
                    let m = sanitation(&world);
                    note("sanitation", &m);
                    apply(&mut world, &m);
                    let m = pollution(&world);
                    note("pollution", &m);
                    apply(&mut world, &m);
                    let m = contentment(&world);
                    note("contentment", &m);
                    apply(&mut world, &m);
                    let m = schooling(&world);
                    note("schooling", &m);
                    apply(&mut world, &m);
                    let m = demography(&world);
                    note("demography", &m);
                    apply(&mut world, &m);
                    let m = migration(&world);
                    note("migration", &m);
                    apply(&mut world, &m);
                    let m = tourism(&world);
                    note("tourism", &m);
                    apply(&mut world, &m);
                    // Accept everything, so deliveries and failures both happen.
                    let offers: Vec<_> = world.contracts.offers().map(|c| c.id).collect();
                    for id in offers {
                        world.contracts.accept(id);
                    }
                }
                for (name, system) in [
                    ("power", power as fn(&World) -> Vec<Mutation>),
                    ("heating", heating),
                    ("construction", construction),
                    ("production", production),
                    ("households", households),
                    ("trade", trade),
                    ("fleet", fleet),
                    ("belts", belts),
                    ("dispatch", dispatch),
                    ("crews", crews),
                    ("settling", settling),
                    ("touring", touring),
                    ("clearing", clearing),
                ] {
                    let m = system(&world);
                    note(name, &m);
                    apply(&mut world, &m);
                }
                world.clock.advance();
            }
            // Keep the border stocked so imports as well as exports happen,
            // and keep crude coming out of the ground at the far end so the
            // long haul recurs rather than happening once.
            if day % 30 == 0 {
                world.treasury.credit(Market::West, 200.0);
                if let Some(field) = oilfield
                    && let Some(b) = world.buildings.get_mut(field)
                {
                    b.stock.add(Resource::Oil, Tonnes(150.0));
                }
            }
        }
        // A wet fortnight with every gang called off its site, which is the only
        // way this fixture reaches `crews`' bog roll.
        //
        // **Measured before it was written, and the number is why it is here**:
        // over 600 simulated days a republic sent 130 crew buses out, median
        // journey two kilometres, and exactly *one* of them stuck — and that was
        // in taiga. An empty bus is the second most capable thing the republic
        // owns, so the roll almost always comes up safe, and a declaration the
        // guard can never see reached is a declaration that has stopped
        // constraining anything. Saturating the ground and stranding the crews
        // makes the crossing one no bus can make, which is the case the roll
        // exists for.
        world.ground.moisture = 1.0;
        world.ground.water = 1.0;
        world.ground.frost = 0.0;
        world.ground.snow = 0.0;
        let posted: Vec<Destination> = world.crews.all().iter().filter_map(|p| p.working).collect();
        for site in posted {
            let at = world.place_of(site).unwrap_or(centre);
            world.crews.release(site, at);
        }
        // And diesel, in the tanks and in the office. Four hundred days of
        // running leaves a republic dry, and a bus that cannot set out cannot
        // be rolled for anything — which is the difference between a mechanic
        // that never fires and a fixture that never reaches it.
        let offices: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.kind == BuildingKind::ConstructionOffice)
            .map(|b| b.id)
            .collect();
        for id in offices {
            if let Some(b) = world.buildings.get_mut(id) {
                b.stock.add(Resource::Fuel, Tonnes(5.0));
            }
        }
        let buses: Vec<VehicleId> = world
            .fleet
            .all()
            .iter()
            .filter(|v| v.def().role == Role::Crew)
            .map(|v| v.id)
            .collect();
        for id in buses {
            if let Some(v) = world.fleet.get_mut(id) {
                v.fuel = v.def().tank;
            }
        }
        // And somewhere to send them. By day 400 the republic has finished
        // everything it was given and every gang is home, so without a fresh
        // site there is no ferry to roll for. Its gravel is put straight in the
        // site rather than driven out, because what is being reached here is
        // the crossing and not the supply chain.
        // Six of them, in different directions, because the crossing is a roll
        // at odds of about one in four rather than a verdict — two ferries is a
        // coin flip and a guard that passes on a coin flip is not a guard.
        for i in 0..6 {
            let out = Metres(1_400.0 + f64::from(i) * 250.0);
            let away = if i % 2 == 0 { out } else { Metres(-out.0) };
            let Ok(site) = world.order_road(
                centre,
                Point::new(centre.x + away, centre.y + out),
                crate::roadworks::Grade::Dirt,
            ) else {
                continue;
            };
            let bill = world.roadworks.get(site).map(|r| r.materials());
            if let (Some(bill), Some(road)) = (bill, world.roadworks.get_mut(site)) {
                for (resource, quantity) in bill {
                    road.stock.add(resource, quantity);
                }
            }
        }
        for _ in 0..(TICKS_PER_DAY * 8) {
            // Called off every morning, so the two buses shuttle all fortnight
            // instead of posting one gang and going quiet.
            if world.clock.is_day_boundary() {
                let posted: Vec<Destination> =
                    world.crews.all().iter().filter_map(|p| p.working).collect();
                for site in posted {
                    let at = world.place_of(site).unwrap_or(centre);
                    world.crews.release(site, at);
                }
            }
            let m = crews(&world);
            note("crews", &m);
            apply(&mut world, &m);
            let m = fleet(&world);
            note("fleet", &m);
            apply(&mut world, &m);
            world.clock.advance();
        }

        // And a season in which nobody comes for the people at the border.
        //
        // `GiveUp` is the one migration outcome a working republic never
        // reaches, which is exactly why it needs reaching here: it is the bound
        // that stops a republic with no transport hoarding an unbounded crowd,
        // and a bound nobody has watched fire is a bound nobody knows works.
        // The depots are emptied and unstaffed so no coach can set out, and
        // only the daily systems are run — a hundred days of full ticks to
        // check one declaration would be the most expensive line in this file.
        let depots: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.kind == BuildingKind::BusDepot)
            .map(|b| b.id)
            .collect();
        for id in depots {
            if let Some(b) = world.buildings.get_mut(id) {
                b.staff = 0;
                b.stock.take(Resource::Fuel, Tonnes(1_000.0));
            }
        }
        // A republic worth coming to. Measured before it was written: at the
        // end of the run above the occupied blocks sit at 32-47% content, well
        // under the threshold that attracts anybody, so a group would never
        // arrive to give up in the first place. Provisioning and heat are set
        // directly because the systems that write them are not run in this
        // tail — what is being reached is migration, and paying for a year of
        // full ticks to get there would be the most expensive line in the file.
        let homes: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().residents > 0)
            .map(|b| b.id)
            .collect();
        for id in homes {
            if let Some(b) = world.buildings.get_mut(id) {
                b.provisioned = 1.0;
                b.heated = true;
            }
        }
        // Fresh empty housing, because four hundred days of a republic worth
        // living in fills everything that was spare — and with nowhere to put
        // anybody, nobody is offered a place and no group ever stands at the
        // border to give up.
        for i in 0..4 {
            let want = Point::new(
                centre.x + Metres(500.0) + Metres(f64::from(i) * 120.0),
                centre.y + Metres(800.0),
            );
            if let Some(site) =
                crate::scenario::find_site(&world, BuildingKind::Apartment, want, Metres(900.0))
            {
                let _ = world.buildings.place_built(
                    BuildingKind::Apartment,
                    site,
                    &world.terrain,
                    &world.geology,
                );
            }
        }
        for _ in 0..(crate::migration::PATIENCE * 2 + 5) {
            let m = contentment(&world);
            note("contentment", &m);
            apply(&mut world, &m);
            let m = migration(&world);
            note("migration", &m);
            apply(&mut world, &m);
            world.clock.advance_by(TICKS_PER_DAY);
        }
        // And a wet fortnight with coaches running, which is the only way this
        // fixture reaches `settling`'s bog roll.
        //
        // Same shape as the crew-bus phase above and for the same reason: a
        // coach is an empty road vehicle and the crossing almost always comes
        // up safe, so the declaration would look like a superset for ever.
        // Three things are needed together and each was found by needing it —
        // ground no coach can cross, groups to be sent for, and **separate
        // days**, because the roll is keyed by `(vehicle, leg, day)` and two
        // dispatches on one day are one draw asked twice.
        let depots: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.kind == BuildingKind::BusDepot)
            .map(|b| b.id)
            .collect();
        for id in &depots {
            if let Some(b) = world.buildings.get_mut(*id) {
                b.staff = BuildingKind::BusDepot.def().workers;
                b.stock.add(Resource::Fuel, Tonnes(60.0));
                // Spares, and this is the **fifth** thing it took. A depot with
                // an empty parts bin runs half its establishment, and half of
                // two coaches is one — which halved the draws at a roll that
                // already fires about once in a hundred and thirty journeys.
                // The guard's floor half caught it the same minute maintenance
                // existed, which is the whole argument for having that half.
                b.stock.add(Resource::Machinery, Tonnes(40.0));
            }
        }
        // Unworn ground, and this is the fourth thing it took. Four hundred
        // days of traffic packs the lattice around a town into made track, and
        // `going_in` multiplies the softness by that relief — so saturating the
        // weather over a corridor the republic has been driving on all year
        // still gives good going. Rebuilding the lattice from the same terrain
        // is what puts the field back to a field.
        let unworn = world.terrain.clone();
        world.set_terrain(unworn);
        world.ground.moisture = 1.0;
        world.ground.water = 1.0;
        world.ground.frost = 0.0;
        world.ground.snow = 0.0;
        // Standing a short way off the depot rather than at the frontier post,
        // and that is the third thing this took. A post is reachable by road
        // from town, and **a road leg carries a speed limit and never bogs** —
        // so leg zero was tarmac every time and the roll was never even
        // reached. A short hop is quicker straight than round by the network,
        // which is what makes it the cross-country leg this declaration is
        // about: a vehicle that sticks on the *first* crossing out of the yard,
        // where there is no later leg boundary for `fleet` to catch it at.
        let start = depots
            .first()
            .and_then(|id| world.buildings.get(*id))
            .map(|b| Point::new(b.centre.x, b.centre.y + Metres(400.0)));
        if let Some(start) = start {
            let today = world.clock.day_index();
            // Small groups, because what has to fit is the republic's spare
            // housing rather than a plausible wave of migration.
            for _ in 0..40 {
                world.migration.arrive(start, 4, today);
            }
        }
        for _ in 0..(30 * TICKS_PER_DAY) {
            let m = settling(&world);
            note("settling", &m);
            apply(&mut world, &m);
            let m = fleet(&world);
            note("fleet", &m);
            apply(&mut world, &m);
            world.clock.advance();
        }

        // `contracting` gets a world of its own, and that is the point.
        //
        // Putting a contracted site into the town above broke it: `demography`
        // stopped emitting `Birth`, whatever kind was contracted and whether or
        // not it ever finished. The town fixture is a judgement — "a republic
        // that works" — and everything built on it depends on every threshold
        // that judgement is compared against, which is the lesson this file has
        // now learned four separate times.
        //
        // So rather than widen a balanced fixture until it tips, this exercises
        // the one system that needs an empty map on an empty map. It is also the
        // honest setting for it: contracting exists *because* a blank slate has
        // no crews, and a republic with a working Construction Office should
        // never reach for it.
        {
            let mut blank = World::new(WorldSpec {
                seed: 1961,
                extent: Metres(6_000.0),
                climate: ClimateId::Plains,
            });
            let centre = crate::scenario::found(&mut blank);
            if let Some(at) = crate::scenario::find_site(
                &blank,
                BuildingKind::ConstructionOffice,
                centre,
                Metres(800.0),
            ) {
                let _ = blank.issue(crate::Command::ContractBuild {
                    kind: BuildingKind::ConstructionOffice,
                    at,
                    market: Market::East,
                });
                for _ in 0..5 {
                    let m = contracting(&blank);
                    note("contracting", &m);
                    apply(&mut blank, &m);
                }
            }
        }

        seen
    }

    /// A crew bus is rolled for the ground it is about to cross, exactly as a
    /// lorry is, and a saturated field beats it.
    ///
    /// Its own test because the odds are genuinely long — 130 ferries over 600
    /// simulated days produced one casualty — and a mechanic that fires once a
    /// republic-year is one nobody would notice had broken.
    #[test]
    fn a_crew_bus_sticks_in_ground_it_cannot_cross() {
        let sent = |soaked: bool, day: u64| -> (usize, usize) {
            let mut w = bare();
            for _ in 0..(day * TICKS_PER_DAY) {
                w.clock.advance();
            }
            place(
                &mut w,
                BuildingKind::ConstructionOffice,
                at(1_000.0, 1_000.0),
            );
            staff_up(&mut w, at(1_000.0, 1_150.0), 10);
            // Far enough out that the ferry is a real crossing rather than a
            // walk across the yard.
            let site = w
                .buildings
                .place(
                    BuildingKind::Warehouse,
                    at(3_000.0, 3_000.0),
                    &w.terrain,
                    &w.geology,
                )
                .expect("open ground");
            for &(resource, quantity) in BuildingKind::Warehouse.def().materials {
                w.buildings
                    .get_mut(site)
                    .unwrap()
                    .stock
                    .add(resource, Tonnes(quantity));
            }
            // The office's buses, and the people to drive and crew them.
            let m = commissioning(&w);
            apply(&mut w, &m);
            let m = labour(&mut w);
            apply(&mut w, &m);
            if soaked {
                w.ground.moisture = 1.0;
                w.ground.water = 1.0;
            }

            let m = crews(&w);
            (
                m.iter()
                    .filter(|x| matches!(x, Mutation::Dispatch { .. }))
                    .count(),
                m.iter()
                    .filter(|x| matches!(x, Mutation::Bog { .. }))
                    .count(),
            )
        };

        // Twenty separate days, because the crossing is a **roll** and not a
        // verdict: saturated grass is 0.3 past what an empty bus can take, which
        // is odds of about one in four rather than a certainty. Asserting on one
        // draw would be asserting on which key the substream happened to hand
        // out. The premise assertion is the dispatch count — without it the wet
        // case could pass by sending nobody at all.
        let mut dry_stuck = 0;
        let mut wet_stuck = 0;
        for day in 0..20 {
            let (out, stuck) = sent(false, day);
            assert_eq!(out, 1, "day {day}: a bus should be sent on firm ground");
            dry_stuck += stuck;
            let (out, stuck) = sent(true, day);
            assert_eq!(out, 1, "day {day}: the same bus is sent on wet ground");
            wet_stuck += stuck;
        }
        assert_eq!(dry_stuck, 0, "firm ground stopped nobody");
        assert!(
            wet_stuck > 0,
            "twenty crews were sent across saturated ground and every one of \
             them arrived — a bus is not being rolled for the crossing it is \
             about to make"
        );
    }

    /// The bootstrap: a republic that owns nothing can still get a building up.
    ///
    /// This is the one path off a blank map, so it is the one that must work
    /// with no Construction Office, no crews, no materials and nobody living
    /// here — which is exactly the state `scenario::found` leaves a world in.
    #[test]
    fn a_contracted_firm_builds_what_the_republic_cannot() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let centre = crate::scenario::found(&mut w);

        // The premise, asserted rather than assumed: nothing here.
        assert_eq!(w.buildings.all().len(), 0, "a posting starts empty");
        assert_eq!(w.population.count(), 0, "and nobody lives on it");
        let opening = w.treasury.of(Market::East);
        assert!(opening > 0.0, "but it does start with roubles");
        assert_eq!(w.treasury.of(Market::West), 0.0, "and no hard currency");

        let at =
            crate::scenario::find_site(&w, BuildingKind::ConstructionOffice, centre, Metres(800.0))
                .expect("somewhere to put an office");
        let done = w.issue(crate::Command::ContractBuild {
            kind: BuildingKind::ConstructionOffice,
            at,
            market: Market::East,
        });
        assert!(done.is_ok(), "the contract was refused: {done:?}");

        let site = w
            .buildings
            .all()
            .first()
            .map(|b| b.id)
            .expect("the site went down");
        assert!(
            !w.buildings.get(site).unwrap().is_built(),
            "it starts as a site"
        );

        for _ in 0..(30 * crate::time::TICKS_PER_DAY) {
            w.tick();
        }

        let b = w.buildings.get(site).expect("still there");
        assert!(
            b.is_built(),
            "a month of a paid firm did not finish an office: {:.0} of {:.0} builder-days",
            b.work_done,
            b.def().labour
        );
        assert!(
            w.treasury.of(Market::East) < opening,
            "the office went up and nobody was billed for it"
        );
        assert!(
            b.contractor.is_none(),
            "a finished contract is still billing"
        );
    }

    /// The guard the archived build proved worth having: a system may not
    /// quietly widen its blast radius.
    #[test]
    fn no_system_writes_outside_its_declaration() {
        let seen = observed_writes();
        for (name, declared) in WRITE_SETS {
            let Some(actual) = seen.get(name) else {
                panic!("{name} never ran — the guard is not watching it");
            };
            for kind in actual {
                assert!(
                    declared.contains(kind),
                    "{name} emitted {kind:?}, which is not in its write set"
                );
            }
        }
    }

    /// And the other half, which is the half that rots: a declaration that
    /// claims more than the system does has stopped constraining anything, and
    /// nothing about it looks wrong.
    #[test]
    fn every_declared_write_is_actually_emitted() {
        let seen = observed_writes();
        for (name, declared) in WRITE_SETS {
            let actual = seen.get(name).expect("every system runs");
            for kind in *declared {
                assert!(
                    actual.contains(kind),
                    "{name} declares {kind:?} but never emits it — the declaration is a superset"
                );
            }
        }
    }

    /// Every system is declared, and every mutation kind belongs to somebody.
    /// An undeclared system is one nothing is watching.
    #[test]
    fn every_system_and_every_mutation_kind_is_accounted_for() {
        let seen = observed_writes();
        let declared: std::collections::BTreeSet<&str> =
            WRITE_SETS.iter().map(|(n, _)| *n).collect();
        for name in seen.keys() {
            assert!(declared.contains(name), "{name} has no write set");
        }
        let owned: std::collections::BTreeSet<MutationKind> = WRITE_SETS
            .iter()
            .flat_map(|(_, kinds)| kinds.iter().copied())
            .collect();
        for kind in seen.values().flatten() {
            assert!(owned.contains(kind), "{kind:?} is emitted by nobody's set");
        }
    }

    // ---- Heating ----

    /// Put the clock on a given month without simulating the days between it.
    fn move_to_month(world: &mut World, month: u32) {
        let target = u64::from(month - 1) * u64::from(crate::time::DAYS_PER_MONTH);
        let today = world.clock.day_index() % u64::from(crate::time::DAYS_PER_YEAR);
        let ahead = (target + u64::from(crate::time::DAYS_PER_YEAR) - today)
            % u64::from(crate::time::DAYS_PER_YEAR);
        world.clock.advance_by(ahead * TICKS_PER_DAY);
    }

    /// A town with a boiler house, a grid to run its pumps, and enough people
    /// to staff both. A boiler is a building like any other — it needs a crew
    /// and electricity before it needs coal.
    fn heated_town(world: &mut World) -> (BuildingId, BuildingId) {
        use crate::utility::Utility;
        let flats = staff_up(world, at(1_000.0, 1_000.0), 40);
        let boiler = place(world, BuildingKind::HeatingPlant, at(1_200.0, 1_000.0));
        let grid = place(world, BuildingKind::PowerPlant, at(1_000.0, 1_400.0));
        world
            .buildings
            .get_mut(boiler)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(40.0));
        world
            .buildings
            .get_mut(grid)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(400.0));
        // A grid and a main, because neither power nor heat is a quantity any
        // more: a plant lights what it is strung to and a boiler warms what a
        // main runs past. The station goes beside the boiler, which is what its
        // pumps draw through.
        energise(
            world,
            Utility::Power,
            at(1_000.0, 1_400.0),
            at(1_150.0, 1_100.0),
        );
        substation(world, at(1_150.0, 1_100.0));
        energise(
            world,
            Utility::Heat,
            at(1_200.0, 1_000.0),
            at(1_000.0, 1_000.0),
        );
        let mutations = labour(world);
        apply(world, &mutations);
        (flats, boiler)
    }

    /// The whole point of driving heating from temperature rather than the
    /// calendar: a July boiler is not merely cheaper, it burns nothing at all.
    #[test]
    fn a_boiler_burns_coal_in_winter_and_none_in_summer() {
        let burn_over_a_day = |month: u32| {
            let mut w = bare();
            let (_, boiler) = heated_town(&mut w);
            move_to_month(&mut w, month);
            let before = w.buildings.get(boiler).unwrap().stock.get(Resource::Coal);
            for _ in 0..TICKS_PER_DAY {
                w.tick();
            }
            before.0 - w.buildings.get(boiler).unwrap().stock.get(Resource::Coal).0
        };

        let july = burn_over_a_day(7);
        let january = burn_over_a_day(1);
        assert_eq!(july, 0.0, "a boiler burnt {july} t of coal in July");
        assert!(
            january > 0.0,
            "a boiler burnt nothing in January — heating is not costing anything"
        );
    }

    /// Heating's real teeth today: it competes for the same coal the power
    /// station wants, off the same stockpile.
    #[test]
    fn a_winter_boiler_draws_on_the_same_coal_as_the_grid() {
        let mut w = bare();
        let (_, boiler) = heated_town(&mut w);
        move_to_month(&mut w, 1);

        // One shared pile, and the boiler is helping itself to it.
        let before = w.buildings.get(boiler).unwrap().stock.get(Resource::Coal);
        for _ in 0..TICKS_PER_DAY * 5 {
            w.tick();
        }
        let after = w.buildings.get(boiler).unwrap().stock.get(Resource::Coal);
        assert!(after.0 < before.0, "five winter days cost no coal at all");
        assert_eq!(
            stall_reason(&w, boiler),
            None,
            "the boiler should still be running"
        );
    }

    /// The climate a posting sits in has to cost something, or choosing one on
    /// the founding shelf is decoration. Same town, same month, same everything
    /// but the sky.
    #[test]
    fn a_harder_posting_burns_more_coal_for_the_same_winter() {
        let january_burn = |climate: ClimateId| {
            let mut w = bare();
            w.climate = climate;
            let (_, boiler) = heated_town(&mut w);
            move_to_month(&mut w, 1);
            let before = w.buildings.get(boiler).unwrap().stock.get(Resource::Coal);
            for _ in 0..TICKS_PER_DAY * 20 {
                w.tick();
            }
            before.0 - w.buildings.get(boiler).unwrap().stock.get(Resource::Coal).0
        };

        let mild = january_burn(ClimateId::Maritime);
        let plains = january_burn(ClimateId::Plains);
        let taiga = january_burn(ClimateId::Taiga);
        assert!(
            mild < plains && plains < taiga,
            "January cost {mild:.2} / {plains:.2} / {taiga:.2} t on maritime / plains / taiga"
        );
        // And the maritime posting is genuinely mild rather than merely
        // cheaper — a January that still needs full heat is not a soft option.
        assert!(mild < taiga * 0.5);
    }

    #[test]
    fn housing_goes_cold_when_there_is_no_boiler() {
        let mut w = bare();
        let flats = staff_up(&mut w, at(1_000.0, 1_000.0), 40);
        move_to_month(&mut w, 1);
        w.tick();
        assert!(
            !w.buildings.get(flats).unwrap().heated,
            "a republic with no boiler kept its flats warm anyway"
        );

        // And in summer nothing is cold, because nothing needs warming.
        move_to_month(&mut w, 7);
        w.tick();
        assert!(w.buildings.get(flats).unwrap().heated);
    }

    /// Supply is finite and allocated in commissioning order, so the boiler
    /// warms what it can and the rest go cold — not everyone a little.
    #[test]
    fn a_boiler_warms_what_it_can_reach_and_no_more() {
        let mut w = bare();
        // Eight blocks at 2 heat each is 16 units wanted; one boiler makes 8.
        let mut blocks = Vec::new();
        for i in 0..8 {
            blocks.push(place(
                &mut w,
                BuildingKind::Apartment,
                at(500.0 + f64::from(i) * 100.0, 500.0),
            ));
        }
        let boiler = place(&mut w, BuildingKind::HeatingPlant, at(500.0, 900.0));
        let grid = place(&mut w, BuildingKind::PowerPlant, at(900.0, 1_300.0));
        let home = staff_up(&mut w, at(700.0, 900.0), 40);
        w.buildings
            .get_mut(boiler)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(40.0));
        w.buildings
            .get_mut(grid)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(400.0));
        // The grid the boiler's pumps run on, and a main from the boiler along
        // the row of blocks. The main is what makes this a test about capacity
        // rather than about connection: every block is on it, so what decides
        // who is warm is how much the boiler can make.
        energise(
            &mut w,
            crate::utility::Utility::Power,
            at(900.0, 1_300.0),
            at(600.0, 950.0),
        );
        substation(&mut w, at(600.0, 950.0));
        energise(
            &mut w,
            crate::utility::Utility::Heat,
            at(500.0, 900.0),
            at(500.0, 500.0),
        );
        energise(
            &mut w,
            crate::utility::Utility::Heat,
            at(500.0, 500.0),
            at(1_200.0, 500.0),
        );
        // And the crew's own block, which wants its share like any other.
        energise(
            &mut w,
            crate::utility::Utility::Heat,
            at(500.0, 900.0),
            at(700.0, 900.0),
        );
        let mutations = labour(&mut w);
        apply(&mut w, &mutations);
        let mutations = power(&w);
        apply(&mut w, &mutations);

        // A design-cold January, so demand is at or beyond nominal.
        move_to_month(&mut w, 1);
        w.tick();

        let warm = blocks
            .iter()
            .filter(|&&id| w.buildings.get(id).unwrap().heated)
            .count();
        assert!(
            (1..8).contains(&warm),
            "{warm} of 8 blocks warm — the boiler is either infinite or useless"
        );
        // Commissioning order, so it is the earliest blocks that stay warm.
        assert!(w.buildings.get(blocks[0]).unwrap().heated);
        assert!(!w.buildings.get(blocks[7]).unwrap().heated);
        // The home the boiler crew live in is a block too and wants its share.
        assert!(w.buildings.get(home).unwrap().def().heat > 0.0);
    }

    // ---- Transport ----

    /// The acceptance scenario for transport, and the counterpart to
    /// `a_mining_town_dies_when_its_work_does`: the same town, the same closed
    /// mine, but a road and a bus depot — and now the people can reach work
    /// instead of being stranded.
    ///
    /// This is what transport is *for*. Without it the map can never be larger
    /// than a walk, and every deposit needs its own town whether the geography
    /// wants one there or not.
    #[test]
    fn a_road_and_a_bus_save_the_town_the_mine_left_behind() {
        use crate::network::default_road_speed;

        let build = |with_transport: bool| {
            let mut w = bare();
            w.set_terrain(Terrain::flat(Metres(12_000.0)));

            // A remote camp with a mine, and a city eight kilometres away with
            // work but nobody living in it.
            let far = at(9_000.0, 9_000.0);
            coal_body(&mut w, far, 10_000.0);
            let mine = place(&mut w, BuildingKind::CoalMine, far);
            let camp = staff_up(&mut w, at(9_300.0, 9_000.0), 40);
            // A steel mill rather than a machine works: the latter is graduate
            // work, and this test is about buses rather than about schools.
            let works = place(&mut w, BuildingKind::SteelMill, at(1_500.0, 1_000.0));

            if with_transport {
                // Road all the way, and a fuelled depot to run buses on it.
                let mut previous = w.roads.add_node(at(9_200.0, 9_000.0));
                for i in 1..=16 {
                    let next = w.roads.add_node(at(
                        9_200.0 - f64::from(i) * 500.0,
                        9_000.0 - f64::from(i) * 500.0,
                    ));
                    w.roads.connect(previous, next, default_road_speed());
                    previous = next;
                }
                let depot = place(&mut w, BuildingKind::BusDepot, at(9_500.0, 9_400.0));
                let d = w.buildings.get_mut(depot).unwrap();
                d.staff = BuildingKind::BusDepot.def().workers;
                d.stock.add(Resource::Fuel, Tonnes(40.0));
            }

            // The seam runs out and the mine closes.
            let mutations = labour(&mut w);
            apply(&mut w, &mutations);
            assert!(w.population.staff_of(mine) > 0, "the town worked its mine");
            w.buildings.demolish(mine);
            // A few days, not one. The depot's own drivers have to be assigned
            // before the depot can carry anyone, so the first pass after the
            // closure runs no buses and the town re-sorts itself over the next
            // one — which is what a bus network coming online actually looks
            // like.
            for _ in 0..3 {
                let mutations = labour(&mut w);
                apply(&mut w, &mutations);
            }
            (w, camp, works)
        };

        let (stranded, camp, _) = build(false);
        assert!(
            stranded
                .population
                .residents_of(camp)
                .iter()
                .all(|c| c.workplace.0.is_none()),
            "without transport the town should still be stranded"
        );

        let (saved, camp, works) = build(true);
        let riders = saved.population.residents_of(camp);
        assert!(
            riders.iter().any(|c| c.workplace.0 == Some(works)),
            "with road and buses the town should reach the city's work"
        );
        // Everyone working in the city rides — nobody could have walked eleven
        // kilometres. The depot's own drivers walk to the depot, which is the
        // point: the bus network needs staff of its own before it carries
        // anyone, and the first pass after the closure runs no buses at all.
        assert!(
            riders
                .iter()
                .filter(|c| c.workplace.0 == Some(works))
                .all(|c| c.rides()),
            "somebody apparently walked to the city"
        );
        assert!(saved.population.riders() > 0);
    }

    /// A bus with no fuel is a bus that does not run — and the fuel it does
    /// burn comes out of a real bin, so transport is an ongoing cost against
    /// the same refinery output everything else wants.
    #[test]
    fn carrying_commuters_burns_the_depots_fuel() {
        use crate::network::default_road_speed;
        let mut w = bare();
        w.set_terrain(Terrain::flat(Metres(12_000.0)));

        let mut previous = w.roads.add_node(at(1_000.0, 1_000.0));
        for i in 1..=12 {
            let next = w
                .roads
                .add_node(at(1_000.0 + f64::from(i) * 500.0, 1_000.0));
            w.roads.connect(previous, next, default_road_speed());
            previous = next;
        }
        staff_up(&mut w, at(1_000.0, 1_100.0), 30);
        // Schooled work, not graduate work: what is being tested is the
        // ride, and staff_up sends people who finished school.
        place(&mut w, BuildingKind::SteelMill, at(6_500.0, 1_100.0));
        let depot = place(&mut w, BuildingKind::BusDepot, at(1_000.0, 1_400.0));
        {
            let d = w.buildings.get_mut(depot).unwrap();
            d.staff = BuildingKind::BusDepot.def().workers;
            d.stock.add(Resource::Fuel, Tonnes(20.0));
        }

        let before = w.buildings.get(depot).unwrap().stock.get(Resource::Fuel);
        let mutations = labour(&mut w);
        apply(&mut w, &mutations);
        let after = w.buildings.get(depot).unwrap().stock.get(Resource::Fuel);

        assert!(w.population.riders() > 0, "nobody rode");
        assert!(after.0 < before.0, "carrying people cost no fuel");
    }

    // ---- Utilities ----

    /// A grid with no generation on it feeds nothing, however much the
    /// republic is making elsewhere.
    ///
    /// This is the whole milestone in one assertion, and it is deliberately
    /// **two complete grids** rather than one grid and a building in a field.
    /// The first version put the second factory four kilometres from any line
    /// and passed for the wrong reason: what made it dark was having no
    /// transformer station in reach, which is a different rule with its own
    /// test. Sabotage found it — giving every plant a network regardless left
    /// the test green — and that is exactly the shape of a check that has
    /// stopped reaching its subject.
    #[test]
    fn a_grid_with_no_generation_on_it_feeds_nothing() {
        use crate::utility::Utility;
        let mut w = bare();
        w.set_terrain(Terrain::flat(Metres(8_000.0)));
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(400.0));
        let wired = place(&mut w, BuildingKind::FoodFactory, at(1_600.0, 1_000.0));
        // A second, complete grid — its own span and its own staffed station —
        // with nothing generating on it.
        let orphaned = place(&mut w, BuildingKind::TextileMill, at(5_000.0, 5_000.0));
        staff_up(&mut w, at(1_300.0, 1_150.0), 60);
        staff_up(&mut w, at(5_200.0, 5_200.0), 30);

        energise(
            &mut w,
            Utility::Power,
            at(1_000.0, 1_000.0),
            at(1_400.0, 1_000.0),
        );
        substation(&mut w, at(1_400.0, 1_000.0));
        energise(
            &mut w,
            Utility::Power,
            at(5_000.0, 5_000.0),
            at(5_300.0, 5_000.0),
        );
        let away = substation(&mut w, at(5_300.0, 5_000.0));

        let m = labour(&mut w);
        apply(&mut w, &m);
        let m = power(&w);
        apply(&mut w, &m);

        // The premise: the far grid really is a grid, with a manned station on
        // it, and it is a different grid from the plant's.
        assert!(
            w.utilities.network_of(away, Utility::Power).is_some(),
            "the far station is not on a line, so this proves nothing"
        );
        assert!(
            w.buildings.get(away).unwrap().staffing() > 0.0,
            "the far station is unmanned, so this proves nothing"
        );
        assert_ne!(
            w.utilities.network_of(away, Utility::Power),
            w.utilities.network_of(plant, Utility::Power),
            "the two grids are the same grid, so this proves nothing"
        );

        assert!(
            w.buildings.get(wired).unwrap().powered,
            "a factory on the grid with the plant on it should be lit"
        );
        assert!(
            !w.buildings.get(orphaned).unwrap().powered,
            "a mill on a grid with no generation on it was lit anyway — power \
             is still a quantity rather than a network"
        );
    }

    /// A pylon past the door is not a connection. What a consumer plugs into is
    /// a transformer station, which is why that building exists at all.
    #[test]
    fn a_pylon_past_the_door_is_not_a_connection() {
        use crate::utility::Utility;
        let lit = |with_station: bool| {
            let mut w = bare();
            let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
            w.buildings
                .get_mut(plant)
                .unwrap()
                .stock
                .add(Resource::Coal, Tonnes(400.0));
            let works = place(&mut w, BuildingKind::FoodFactory, at(1_500.0, 1_000.0));
            staff_up(&mut w, at(1_250.0, 1_200.0), 60);
            // The line runs right past the factory either way.
            energise(
                &mut w,
                Utility::Power,
                at(1_000.0, 1_000.0),
                at(1_800.0, 1_000.0),
            );
            if with_station {
                substation(&mut w, at(1_300.0, 1_050.0));
            }
            let m = labour(&mut w);
            apply(&mut w, &m);
            let m = power(&w);
            apply(&mut w, &m);
            w.buildings.get(works).unwrap().powered
        };
        assert!(!lit(false), "a bare pylon ran a food factory");
        assert!(lit(true), "a staffed station in reach did not");
    }

    /// Losses are charged on the span of the network, so a grid strung across
    /// the map delivers less than a compact one. That is the argument for
    /// siting a plant near what it serves, and without it a long line is a
    /// formality.
    #[test]
    fn a_sprawling_grid_delivers_less_than_a_compact_one() {
        use crate::utility::Utility;
        // How many identical loads a plant can carry over a grid of a given
        // length, read off the **power system's own answer** rather than by
        // re-deriving the loss arithmetic here. The first version computed
        // `1 - loss * span` itself and would have passed against a build that
        // never applied it — a panel doing the simulation's maths, wearing a
        // test's clothes. Sabotage is what found that.
        let carried = |reach: f64| -> usize {
            let mut w = bare();
            w.set_terrain(Terrain::flat(Metres(40_000.0)));
            let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
            w.buildings
                .get_mut(plant)
                .unwrap()
                .stock
                .add(Resource::Coal, Tonnes(400.0));
            energise(
                &mut w,
                Utility::Power,
                at(1_000.0, 1_000.0),
                at(1_000.0 + reach, 1_000.0),
            );
            let station = substation(&mut w, at(1_300.0, 1_000.0));
            // Fifteen food factories at 4 MW is 60 MW, which is exactly what a
            // coal plant makes — so any loss at all costs the last one its
            // current, and a bigger loss costs more of them.
            // Clustered round the station rather than in a line, because a
            // consumer more than TRANSFORMER_RANGE from one is dark for a
            // reason that has nothing to do with losses. A row of fifteen ran
            // off the end of the station reach at the sixth and both grids
            // carried exactly six.
            let works: Vec<BuildingId> = (0..15)
                .map(|i| {
                    place(
                        &mut w,
                        BuildingKind::FoodFactory,
                        at(
                            1_300.0 + f64::from(i % 4) * 130.0 - 195.0,
                            1_000.0 + f64::from(i / 4) * 120.0 - 180.0,
                        ),
                    )
                })
                .collect();
            for id in works.iter().chain(&[plant, station]) {
                let full = w.buildings.get(*id).unwrap().def().workers;
                w.buildings.get_mut(*id).unwrap().staff = full;
            }
            let m = power(&w);
            apply(&mut w, &m);
            works
                .iter()
                .filter(|id| w.buildings.get(**id).unwrap().powered)
                .count()
        };
        let compact = carried(500.0);
        let sprawling = carried(20_000.0);
        assert!(
            compact > 0,
            "a compact grid carried nothing, so this proves nothing"
        );
        assert!(
            sprawling < compact,
            "twenty kilometres of line cost nothing: {sprawling} loads carried \
             against {compact} over half a kilometre"
        );
    }

    /// A block with no main past it is cold, whatever the republic is burning
    /// elsewhere — and the boiler burns enough to cover what the pipes lose.
    ///
    /// The second half took a measurement to find: throttling to demand and
    /// *then* taking the loss out leaves the boiler short by exactly the loss
    /// every time, which put the founding's third block in the dark in January
    /// with the boiler at 71% and coal in the bunker.
    #[test]
    fn a_main_is_what_makes_a_block_warm_and_the_boiler_covers_the_leak() {
        use crate::utility::Utility;
        let mut w = bare();
        let on_the_main = place(&mut w, BuildingKind::Apartment, at(1_000.0, 1_000.0));
        let off_it = place(&mut w, BuildingKind::Apartment, at(2_500.0, 2_500.0));
        let boiler = place(&mut w, BuildingKind::HeatingPlant, at(1_000.0, 1_400.0));
        let grid = place(&mut w, BuildingKind::PowerPlant, at(1_400.0, 1_400.0));
        for id in [boiler, grid] {
            w.buildings
                .get_mut(id)
                .unwrap()
                .stock
                .add(Resource::Coal, Tonnes(400.0));
        }
        staff_up(&mut w, at(1_200.0, 1_150.0), 60);
        energise(
            &mut w,
            Utility::Power,
            at(1_400.0, 1_400.0),
            at(1_150.0, 1_400.0),
        );
        substation(&mut w, at(1_150.0, 1_400.0));
        energise(
            &mut w,
            Utility::Heat,
            at(1_000.0, 1_400.0),
            at(1_000.0, 1_000.0),
        );

        let m = labour(&mut w);
        apply(&mut w, &m);
        move_to_month(&mut w, 1);
        w.tick();

        assert!(
            crate::climate::heating_required(w.temperature()),
            "January was not cold enough for this to be a test of anything"
        );
        assert!(
            w.buildings.get(on_the_main).unwrap().heated,
            "a block on the main went cold with the boiler running"
        );
        assert!(
            !w.buildings.get(off_it).unwrap().heated,
            "a block with no main within a kilometre and a half was warm anyway"
        );
    }

    /// Ordered, materialled, strung, and only then carrying anything — the same
    /// rule a road answers to, and the reason a line is a site.
    #[test]
    fn a_line_carries_nothing_until_the_crew_have_strung_it() {
        use crate::utility::Utility;
        let mut w = bare();
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(400.0));
        let works = place(&mut w, BuildingKind::FoodFactory, at(1_500.0, 1_000.0));
        let station = substation(&mut w, at(1_300.0, 1_000.0));
        place(
            &mut w,
            BuildingKind::ConstructionOffice,
            at(1_100.0, 1_300.0),
        );
        staff_up(&mut w, at(1_250.0, 1_250.0), 90);

        let ordered = w
            .issue(crate::command::Command::OrderLine {
                kind: Utility::Power,
                from: at(1_000.0, 1_000.0),
                to: at(1_300.0, 1_000.0),
            })
            .expect("a span worth surveying");
        let crate::command::Done::Strung(site) = ordered else {
            panic!("ordering a line should hand back the site");
        };

        let m = labour(&mut w);
        apply(&mut w, &m);
        let m = power(&w);
        apply(&mut w, &m);
        assert!(
            !w.buildings.get(works).unwrap().powered,
            "an ordered line carried current before anybody built it"
        );
        assert!(
            w.utilities.network_of(station, Utility::Power).is_none(),
            "the station is plugged into a site rather than a line"
        );

        // Its steel, then the crew.
        let bill = w.lineworks.get(site).map(|l| l.materials()).unwrap();
        for (resource, quantity) in bill {
            w.lineworks
                .get_mut(site)
                .unwrap()
                .stock
                .add(resource, quantity);
        }
        for _ in 0..TICKS_PER_DAY * 40 {
            w.tick();
        }

        assert!(
            w.lineworks.get(site).is_none(),
            "the span is still a site after forty days of a crew on it"
        );
        assert!(
            w.utilities.network_of(station, Utility::Power).is_some(),
            "the span was strung and the station was not plugged into it"
        );
        assert!(
            w.buildings.get(works).unwrap().powered,
            "the grid is built and the factory is still dark"
        );
    }

    /// Rubbish piles up where nobody collects it, and that is what a landfill
    /// is for. Both halves matter: without the first the building has no
    /// purpose, and without the second the republic has no answer.
    #[test]
    fn rubbish_piles_up_and_a_landfill_is_what_pulls_it_away() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 48);
        let m = labour(&mut w);
        apply(&mut w, &m);

        for _ in 0..60 {
            let m = sanitation(&w);
            apply(&mut w, &m);
            w.clock.advance_by(TICKS_PER_DAY);
        }
        let piled = w.buildings.get(home).unwrap().stock.get(Resource::Waste);
        assert!(
            piled.0 > 3.0,
            "two months of forty-eight people produced {piled:?} of rubbish"
        );

        // And it costs them: a full bin is a place people do not want to live.
        let m = contentment(&w);
        apply(&mut w, &m);
        let dirty = w.buildings.get(home).unwrap().content.cleanliness;
        assert!(
            dirty < 1.0,
            "a yard full of rubbish cost the block nothing at all"
        );

        // The landfill wants it, and the freight ranking already understands a
        // consumer that has run out of its input.
        let tip = place(&mut w, BuildingKind::Landfill, at(1_400.0, 1_000.0));
        haulage(&mut w, at(1_150.0, 1_150.0));
        for _ in 0..TICKS_PER_DAY * 20 {
            w.tick();
        }
        assert!(
            w.buildings.get(tip).unwrap().stock.get(Resource::Waste).0 > 0.0
                || w.buildings.get(home).unwrap().stock.get(Resource::Waste) < piled,
            "twenty days with a landfill and a garage and not a tonne moved"
        );
    }

    /// Smoke costs a harvest, and the weather clears it. Neither half is worth
    /// much without the other: permanent pollution is a state the player cannot
    /// get out of, and pollution that costs nothing is decoration.
    #[test]
    fn smoke_costs_a_harvest_and_the_weather_clears_it() {
        let mut w = bare();
        let farm = place(&mut w, BuildingKind::Farm, at(1_000.0, 1_000.0));
        assert_eq!(
            w.lattice.pollution_near(at(1_000.0, 1_000.0)),
            0.0,
            "the fixture starts clean, or this proves nothing"
        );

        // A steel works beside the fields, running.
        let works = place(&mut w, BuildingKind::SteelMill, at(1_300.0, 1_000.0));
        w.buildings.get_mut(works).unwrap().staff = BuildingKind::SteelMill.def().workers;
        w.buildings.get_mut(works).unwrap().powered = true;
        for _ in 0..90 {
            let m = pollution(&w);
            apply(&mut w, &m);
            w.clock.advance_by(TICKS_PER_DAY);
        }
        let fouled = w.lattice.pollution_near(at(1_000.0, 1_000.0));
        assert!(
            fouled > 0.1,
            "a season beside a steel works left {fouled:.3}"
        );

        // And the yield it costs, read through the same path production reads.
        let clean_yield = growing_conditions(&w);
        let dirty_yield = clean_yield * (1.0 - fouled * SMOKE_YIELD_COST);
        assert!(
            dirty_yield < clean_yield,
            "smoke on the fields cost the harvest nothing"
        );
        let _ = farm;

        // Pull it down and the air comes back.
        w.buildings.demolish(works);
        for _ in 0..60 {
            let m = pollution(&w);
            apply(&mut w, &m);
            w.clock.advance_by(TICKS_PER_DAY);
        }
        assert_eq!(
            w.lattice.pollution_near(at(1_000.0, 1_000.0)),
            0.0,
            "the works came down two months ago and the valley is still filthy"
        );
    }

    /// A belt is a haul that needs no lorry, no driver and no diesel — and it
    /// goes exactly where it was built and nowhere else.
    ///
    /// Both halves are the mechanic. Without the first there is no reason to
    /// build one; without the second there is no reason ever to keep a fleet.
    #[test]
    fn a_belt_moves_coal_with_no_lorry_and_only_where_it_was_built() {
        use crate::utility::Utility;
        let mut w = bare();
        let site = at(1_000.0, 1_000.0);
        coal_body(&mut w, site, 50_000.0);
        let mine = place(&mut w, BuildingKind::CoalMine, site);
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_400.0, 1_000.0));
        // A second plant, off the belt, to prove a belt is not a republic-wide
        // pipe with extra steps.
        let elsewhere = place(&mut w, BuildingKind::PowerPlant, at(3_000.0, 3_000.0));
        w.buildings
            .get_mut(mine)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(60.0));

        energise(&mut w, Utility::Conveyor, site, at(1_400.0, 1_000.0));
        // The premise: the two ends really are on one belt and the far plant is
        // not, or nothing below proves anything.
        assert_eq!(
            w.utilities.network_of(mine, Utility::Conveyor),
            w.utilities.network_of(plant, Utility::Conveyor),
        );
        assert!(
            w.utilities
                .network_of(elsewhere, Utility::Conveyor)
                .is_none(),
            "the far plant is on the belt, so this proves nothing"
        );
        assert!(w.fleet.is_empty(), "there is no fleet, which is the point");

        for _ in 0..TICKS_PER_DAY {
            let m = belts(&w);
            apply(&mut w, &m);
            w.clock.advance();
        }

        let delivered = w.buildings.get(plant).unwrap().stock.get(Resource::Coal);
        assert!(
            delivered.is_positive(),
            "a day on a belt moved no coal at all"
        );
        assert!(
            w.buildings.get(mine).unwrap().stock.get(Resource::Coal).0 < 60.0,
            "the coal arrived without leaving — a belt minted it"
        );
        assert_eq!(
            w.buildings
                .get(elsewhere)
                .unwrap()
                .stock
                .get(Resource::Coal),
            Tonnes::ZERO,
            "a plant with no belt to it was fed anyway"
        );
        // And a belt will not take what it is not for.
        assert!(!Utility::Conveyor.takes(Resource::Oil));
        assert!(Utility::Pipeline.takes(Resource::Oil));
    }

    /// A river divides a republic until somebody pays to span it, and only a
    /// bridge may span it.
    ///
    /// Until this refusal existed nothing asked what a road ran *over* — only
    /// its two ends — so a gravel road could be laid straight across a river at
    /// the price of gravel, while the design recorded water as impassable.
    #[test]
    fn only_a_bridge_crosses_water_and_it_costs_what_a_bridge_costs() {
        use crate::roadworks::Grade;
        let mut w = bare();
        // A river down the middle of the map.
        for step in 0..400 {
            let y = f64::from(step) * 10.0;
            for across in -1..=1 {
                w.terrain.set_surface(
                    at(2_000.0 + f64::from(across) * 10.0, y),
                    crate::terrain::Surface::Water,
                );
            }
        }
        let terrain = w.terrain.clone();
        w.set_terrain(terrain);

        let (west, east) = (at(1_800.0, 1_000.0), at(2_300.0, 1_000.0));
        assert_eq!(
            w.order_road(west, east, Grade::Gravel),
            Err(crate::roadworks::RoadError::NeedsABridge),
            "a gravel road was laid across a river"
        );
        assert!(
            w.order_road(west, east, Grade::Bridge).is_ok(),
            "a bridge could not span the river it exists for"
        );
        // And it is not merely a road with a different name: a kilometre of it
        // is steel and months of a crew.
        let bridge = Grade::Bridge.def();
        let paved = Grade::Paved.def();
        assert!(bridge.labour > paved.labour * 3.0);
        assert!(
            bridge.materials.iter().any(|&(r, _)| r == Resource::Steel),
            "a bridge is built out of no steel"
        );
        // A road that stays on one bank is unaffected.
        assert!(
            w.order_road(at(1_000.0, 1_000.0), at(1_800.0, 1_000.0), Grade::Gravel)
                .is_ok(),
            "a road nowhere near the water was refused"
        );
    }

    // ---- People ----

    /// Run the daily people systems for a stretch, without paying for the whole
    /// simulation.
    ///
    /// Everything in this section is about a mechanic that moves on the day
    /// boundary, so 1,440 ticks per day would be 1,440 times the cost for the
    /// same answer. What the caller loses is production and freight, which is
    /// why these fixtures set `provisioned` and `heated` directly.
    fn live_days(world: &mut World, days: u64) {
        for _ in 0..days {
            for system in [
                contentment as fn(&World) -> Vec<Mutation>,
                schooling,
                demography,
                migration,
            ] {
                let m = system(world);
                apply(world, &m);
            }
            world.clock.advance_by(TICKS_PER_DAY);
        }
    }

    /// A town that is fed, warm and employed, with the services it wants.
    fn contented_town(world: &mut World, at_point: Point, people: usize) -> BuildingId {
        let home = staff_up(world, at_point, people);
        if let Some(b) = world.buildings.get_mut(home) {
            b.provisioned = 1.0;
            b.heated = true;
        }
        home
    }

    /// One building for each thing the people need, built and staffed in reach
    /// of a town. What it takes to be a place people actually want to move to.
    ///
    /// **Separate from `contented_town` on purpose**, and the reason is a bug
    /// this nearly caused. Fed and warm was enough to attract settlers until
    /// `Safety` became a contentment component; after that a town with no fire
    /// station fell below the threshold, nobody arrived, and every migration
    /// test built on the old fixture passed by having nothing to measure.
    /// Folding the services into `contented_town` fixed that and broke
    /// something worse -- tests that assert a republic has *no* school or *no*
    /// clinic were suddenly given both.
    ///
    /// **The cheapest server per need, not the whole roster.** Building all
    /// thirteen wants about a hundred and forty workers, and housing them put
    /// the republic over its own capacity -- at which point nobody is offered
    /// a place and not one settler arrives, which is the same vacuum by a
    /// different route. Needs are walked off `Need::ALL` rather than listed
    /// here, so a new one does not hollow these tests out again.
    fn with_services(world: &mut World, near: Point) {
        let mut ring = 0.0;
        for need in crate::building::Need::ALL {
            let cheapest = crate::building::BUILDINGS
                .iter()
                .filter(|d| d.serves.iter().any(|&(what, _)| what == need))
                .min_by_key(|d| d.workers);
            let Some(def) = cheapest else { continue };
            ring += 140.0;
            let spot = Point::new(near.x + Metres(ring), near.y + Metres(260.0));
            if let Ok(id) = world.place_built(def.kind, spot) {
                world.buildings.get_mut(id).expect("just placed").staff = def.workers;
            }
        }
    }

    /// The first thing in this simulation that pushes back on the player.
    ///
    /// `provisioned` and `heated` were computed every tick for months with
    /// nothing reading either one — a republic could starve its estates and
    /// freeze them and lose nothing by it. This is what they were waiting for.
    #[test]
    fn a_republic_that_fails_its_people_loses_them() {
        let run = |fed: bool| -> (usize, f64) {
            let mut w = bare();
            let home = staff_up(&mut w, at(1_000.0, 1_000.0), 40);
            place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_000.0));
            let m = labour(&mut w);
            apply(&mut w, &m);
            if let Some(b) = w.buildings.get_mut(home) {
                b.provisioned = if fed { 1.0 } else { 0.0 };
                b.heated = fed;
            }
            // Winter, so warmth is a thing the republic is being asked for.
            move_to_month(&mut w, 1);
            // Households and heating are not run here, so what was set above
            // stays set — this is a test about the consequence, not about the
            // supply chain that produces it.
            // The **lowest** loyalty the run ever reached, not the last one.
            // Warmth is `1.0` on a warm day — a republic is not marked down for
            // heat nobody is asking it for — so a run that starts in January
            // and ends in September samples a summer, and a summer with no
            // heating demand reads two whole points better than the winter that
            // is actually breaking these people. Sampling the end of the run
            // reported 0.39 for a republic that had spent every winter at 0.20.
            // The same peak-versus-instant trap as everywhere else.
            let mut lowest = 1.0f64;
            for _ in 0..600 {
                let m = contentment(&w);
                apply(&mut w, &m);
                let m = migration(&w);
                apply(&mut w, &m);
                w.clock.advance_by(TICKS_PER_DAY);
                let (_, loyalty) = w.population.mean_wellbeing();
                lowest = lowest.min(loyalty);
            }
            (w.population.count(), lowest)
        };

        let (kept, loyal) = run(true);
        let (lost, disaffected) = run(false);
        assert_eq!(kept, 40, "a republic that serves its people keeps them");
        assert!(
            loyal > crate::wellbeing::LOYALTY_LEAVES,
            "a well-run republic should not be losing anyone: loyalty {loyal:.2}"
        );
        assert!(
            disaffected < crate::wellbeing::LOYALTY_LEAVES,
            "a starving, freezing republic held its people's loyalty at {disaffected:.2}"
        );
        assert!(
            lost < kept,
            "nobody left a republic with no food and no heat"
        );
    }

    /// An estate can say what it is short of, and the answer is weighted.
    ///
    /// The whole reason contentment is stored as a breakdown rather than a
    /// score: "your people are at 61%" is not something a player can act on.
    #[test]
    fn an_estate_says_which_thing_is_costing_it_most() {
        let mut w = bare();
        let home = contented_town(&mut w, at(1_000.0, 1_000.0), 40);
        // Work for exactly half of them, so `work` is genuinely partial rather
        // than a zero that would swamp everything else.
        place(&mut w, BuildingKind::SteelMill, at(1_200.0, 1_000.0));
        let m = labour(&mut w);
        apply(&mut w, &m);
        let m = contentment(&w);
        apply(&mut w, &m);

        let content = w.buildings.get(home).unwrap().content;
        assert_eq!(content.provisions, 1.0);
        assert_eq!(content.health, 0.0, "there is no clinic");
        assert_eq!(content.culture, 0.0, "there is no culture club");
        assert!(
            content.work > 0.0 && content.work < 1.0,
            "half of them work"
        );
        // Health is worth more than culture, so it is health that is named.
        assert_eq!(content.worst(), Some("Health"));

        // Build the clinic and the answer changes to what is now worst.
        let clinic = place(&mut w, BuildingKind::Clinic, at(1_000.0, 1_200.0));
        w.buildings.get_mut(clinic).unwrap().staff = BuildingKind::Clinic.def().workers;
        let m = contentment(&w);
        apply(&mut w, &m);
        let content = w.buildings.get(home).unwrap().content;
        // **A clinic is not a hospital**, and the share it supplies says so.
        // This asserted `1.0` when the Polyclinic was the only health building
        // in the game, which made "complete healthcare" an artefact of the
        // roster rather than a decision. A republic that wants its people fully
        // looked after builds the hospital and the pharmacy as well.
        let share = BuildingKind::Clinic
            .def()
            .serves
            .iter()
            .find(|&&(need, _)| need == crate::building::Need::Health)
            .map(|&(_, share)| share)
            .expect("a polyclinic serves health");
        assert!(
            (content.health - share).abs() < 1e-9,
            "the clinic is staffed and in reach, so health should be {share}, not {}",
            content.health
        );
        assert!(share < 1.0, "a clinic alone is complete healthcare again");
        assert_ne!(
            content.worst(),
            Some("Health"),
            "the clinic was built and the panel still blames the clinic"
        );
    }

    /// A clinic keeps people alive, and that is measurable rather than asserted.
    #[test]
    fn a_polyclinic_makes_an_old_republic_survivable() {
        // The same person, the same age, with and without care.
        let served = mortality(75, 0.95);
        let unserved = mortality(75, crate::wellbeing::HEALTH_UNSERVED);
        assert!(
            served < unserved,
            "healthcare made no difference to mortality: {served:.4} against {unserved:.4}"
        );
        // And age is what dominates it, not health.
        assert!(
            mortality(30, 0.2) < mortality(80, 1.0),
            "a sickly thirty-year-old is more likely to die than a healthy eighty-year-old"
        );
        assert!(mortality(OLDEST + 1, 1.0) <= 1.0, "odds stay odds");
    }

    /// The next generation is only employable if the republic taught them.
    ///
    /// This is what makes a school a building worth putting up rather than
    /// decoration: without one, a republic's own children cannot run its mines.
    #[test]
    fn a_school_is_what_makes_the_next_generation_employable() {
        use crate::citizen::Education;
        let taught = |with_school: bool| -> Education {
            let mut w = bare();
            let home = contented_town(&mut w, at(1_000.0, 1_000.0), 4);
            if with_school {
                let school = place(&mut w, BuildingKind::School, at(1_200.0, 1_000.0));
                w.buildings.get_mut(school).unwrap().staff = BuildingKind::School.def().workers;
            }
            // One child, born here rather than conjured as an adult — an adult
            // arrives schooled by construction, so a conjured one would prove
            // nothing at all.
            let child = w.population.spawn_citizen(home, 6);
            // The ten years between starting school and leaving it.
            live_days(&mut w, u64::from(crate::time::DAYS_PER_YEAR) * 10);
            w.population
                .records()
                .into_iter()
                .find(|c| c.id == child)
                .expect("the child is still alive")
                .education()
        };

        assert_eq!(
            taught(false),
            Education::Unschooled,
            "a republic with no school taught somebody anyway"
        );
        assert_eq!(
            taught(true),
            Education::Schooled,
            "ten years beside a staffed school and the child learnt nothing"
        );
    }

    /// A job nobody is qualified for goes unfilled, however many people are out
    /// of work — the education half of the same rule reach already enforces.
    #[test]
    fn a_refinery_will_not_open_without_graduates() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 60);
        let refinery = place(&mut w, BuildingKind::Refinery, at(1_300.0, 1_000.0));
        let m = labour(&mut w);
        apply(&mut w, &m);
        assert_eq!(
            w.population.staff_of(refinery),
            0,
            "sixty schooled workers staffed a refinery that wants graduates"
        );

        // Give the same people the schooling and the same building fills.
        let ids: Vec<CitizenId> = w.population.records().iter().map(|c| c.id).collect();
        let graduated: Vec<CitizenId> = ids.clone();
        for _ in 0..(crate::citizen::UNIVERSITY_DAYS) {
            w.population.school(&graduated, &[]);
        }
        let m = labour(&mut w);
        apply(&mut w, &m);
        assert_eq!(
            w.population.staff_of(refinery),
            BuildingKind::Refinery.def().workers,
            "graduates could not staff the refinery either"
        );
        let _ = home;
    }

    /// A student is a working-age adult who is not working, and that is the
    /// cost of a university rather than a side effect of one.
    #[test]
    fn a_university_takes_its_students_out_of_the_workforce() {
        let mut w = bare();
        let home = contented_town(&mut w, at(1_000.0, 1_000.0), 4);
        let works = place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_000.0));
        let university = place(&mut w, BuildingKind::University, at(1_000.0, 1_250.0));
        w.buildings.get_mut(university).unwrap().staff = BuildingKind::University.def().workers;

        // Six of them, of an age to go and schooled enough to be taken.
        let students: Vec<CitizenId> = (0..6)
            .map(|_| w.population.spawn_citizen(home, 17))
            .collect();
        let m = schooling(&w);
        apply(&mut w, &m);
        let m = labour(&mut w);
        apply(&mut w, &m);

        let enrolled = w
            .population
            .records()
            .into_iter()
            .filter(|c| students.contains(&c.id))
            .collect::<Vec<_>>();
        assert!(
            enrolled.iter().all(|c| c.learning.studying),
            "nobody enrolled at a staffed university within reach"
        );
        assert!(
            enrolled.iter().all(|c| c.workplace.0.is_none()),
            "a student took a job at the sawmill"
        );
        assert_eq!(
            w.population.by_stage()[3],
            4,
            "the four adults work and the six students do not"
        );

        // And an unstaffed university stops having students the same day —
        // the half a set-only flag would get wrong.
        w.buildings.get_mut(university).unwrap().staff = 0;
        let m = schooling(&w);
        apply(&mut w, &m);
        assert!(
            w.population
                .records()
                .iter()
                .filter(|c| students.contains(&c.id))
                .all(|c| !c.learning.studying),
            "a university with no staff still has students"
        );
        let m = labour(&mut w);
        apply(&mut w, &m);
        assert!(
            w.population.staff_of(works) > 0,
            "they never went back to work"
        );
    }

    /// The acceptance scenario for migration: a republic worth living in
    /// attracts people, and they arrive **at the border** and have to be
    /// carried in.
    ///
    /// Both halves matter. Without the first the population is whatever it was
    /// founded with for ever; without the second an immigrant is a number going
    /// up, which is the click-a-button shape this build refuses.
    #[test]
    fn settlers_arrive_at_a_post_and_have_to_be_fetched() {
        // Services, or the town is not attractive enough for anybody to come
        // and the test has nothing to measure. See `with_services`.
        let mut w = bare();
        let home = contented_town(&mut w, at(1_000.0, 1_000.0), 40);
        // Empty housing for them to be brought to, and work so the republic
        // reads as somewhere worth coming.
        contented_town(&mut w, at(1_400.0, 1_000.0), 0);
        with_services(&mut w, at(1_000.0, 1_000.0));
        place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_100.0));
        let depot = place(&mut w, BuildingKind::BusDepot, at(1_000.0, 1_300.0));
        w.buildings.get_mut(depot).unwrap().staff = BuildingKind::BusDepot.def().workers;
        w.buildings
            .get_mut(depot)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(20.0));
        let m = labour(&mut w);
        apply(&mut w, &m);
        let m = commissioning(&w);
        apply(&mut w, &m);
        let m = contentment(&w);
        apply(&mut w, &m);

        let before = w.population.count();
        assert!(
            w.buildings.get(home).unwrap().content.overall() >= crate::wellbeing::CONTENT_ATTRACTS,
            "the fixture republic is not attractive, so this proves nothing"
        );

        // A season, running the daily people systems and the coaches.
        // Sampled **inside** the tick loop, and that took a run to learn: the
        // post is two kilometres out and a coach does the round trip in under
        // half an hour of simulated time, so a group that arrives at the end of
        // one day is housed before the end of the next. Sampling once a day saw
        // an empty border every single time and reported that nobody had ever
        // come. The same peak-versus-instant trap the crew panel hit.
        let mut ever_standing = 0u32;
        for _ in 0..120 {
            for _ in 0..TICKS_PER_DAY {
                ever_standing =
                    ever_standing.max(w.migration.unfetched().map(|g| g.heads).sum::<u32>());
                let m = settling(&w);
                apply(&mut w, &m);
                let m = fleet(&w);
                apply(&mut w, &m);
                w.clock.advance();
            }
            for system in [contentment as fn(&World) -> Vec<Mutation>, migration] {
                let m = system(&w);
                apply(&mut w, &m);
            }
        }

        assert!(
            ever_standing > 0,
            "nobody ever stood at the frontier, so nothing was fetched"
        );
        assert!(
            w.migration.settled() > 0,
            "settlers arrived at the border and no coach ever brought one in"
        );
        assert_eq!(
            w.population.count(),
            before + w.migration.settled() as usize,
            "the head count does not match what was carried in"
        );
    }

    /// And a republic that cannot reach them does not accumulate a crowd.
    ///
    /// The bound that makes the mechanic safe: a republic with no coaches is
    /// told it cannot house people rather than quietly hoarding a queue at the
    /// border for ever.
    #[test]
    fn settlers_nobody_fetches_go_home() {
        let mut w = bare();
        contented_town(&mut w, at(1_000.0, 1_000.0), 40);
        contented_town(&mut w, at(1_400.0, 1_000.0), 0);
        with_services(&mut w, at(1_000.0, 1_000.0));
        place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_100.0));
        let m = labour(&mut w);
        apply(&mut w, &m);

        live_days(&mut w, crate::migration::PATIENCE * 2);
        assert!(
            w.migration.gave_up() > 0,
            "a republic with no coaches held people at its border indefinitely"
        );
        assert!(
            w.migration.all().len() < 20,
            "{} groups are still standing there — the bound is not bounding",
            w.migration.all().len()
        );
    }

    /// People are born, they age, and they die — and the republic's shape
    /// changes because of it.
    #[test]
    fn a_republic_ages_and_renews_itself() {
        let mut w = bare();
        let home = contented_town(&mut w, at(1_000.0, 1_000.0), 40);
        place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_000.0));
        let m = labour(&mut w);
        apply(&mut w, &m);
        let m = contentment(&w);
        apply(&mut w, &m);

        let oldest_before = w
            .population
            .records()
            .iter()
            .map(|c| c.age.0)
            .max()
            .expect("somebody lives here");
        assert_eq!(w.population.by_stage()[0], 0, "no infants at founding");

        live_days(&mut w, u64::from(crate::time::DAYS_PER_YEAR) * 3);

        let records = w.population.records();
        let oldest_after = records.iter().map(|c| c.age.0).max().expect("still alive");
        assert!(
            oldest_after > oldest_before,
            "three years passed and nobody got any older"
        );
        assert!(
            w.population.by_stage()[0] > 0,
            "a content republic with room in its blocks had no children in three years"
        );
        assert!(
            records.iter().any(|c| c.home.0 == home),
            "everybody left the only home there is"
        );
    }

    /// Deaths and births are a pure function of who and when, so a republic
    /// replays identically. The determinism rule applied to the one system in
    /// here that rolls dice about people.
    #[test]
    fn demography_is_reproducible() {
        let build = || {
            let mut w = bare();
            contented_town(&mut w, at(1_000.0, 1_000.0), 40);
            place(&mut w, BuildingKind::Sawmill, at(1_150.0, 1_000.0));
            let m = labour(&mut w);
            apply(&mut w, &m);
            live_days(&mut w, u64::from(crate::time::DAYS_PER_YEAR) * 2);
            w
        };
        let (a, b) = (build(), build());
        assert_eq!(a.population.records(), b.population.records());
        assert_eq!(a.migration, b.migration);
    }

    // ---- Contracts ----

    /// A default costs the money, a fine, and the bloc's patience.
    ///
    /// The mechanic is only worth having if failing to repay is worse than
    /// never borrowing. All three consequences are asserted because a default
    /// that only wrote the debt off would be a free loan with a waiting period.
    #[test]
    fn failing_to_repay_an_advance_costs_more_than_the_advance() {
        let mut w = bare();
        let bloc = Market::East;

        w.issue(crate::command::Command::TakeLoan {
            market: bloc,
            tier: 0,
        })
        .expect("nothing is owed yet");

        let advanced = w.treasury.of(bloc);
        assert_eq!(advanced, crate::loan::TIERS[0].principal);
        let owed = w.loans.outstanding(bloc);
        assert!(owed > advanced, "an advance costs interest");
        let relations_before = w.contracts.penalty(bloc);

        // Spend it, so the default lands on a republic that cannot simply pay.
        w.treasury.debit(bloc, advanced);

        // Run past the due day. The system is daily, so this also proves the
        // republic is not fined 1,440 times for one unpaid advance.
        let due = crate::loan::TIERS[0].term_days;
        for _ in 0..(due + 2) * TICKS_PER_DAY {
            w.tick();
        }

        assert!(w.loans.of(bloc).is_none(), "the debt was written off");
        assert_eq!(w.loans.defaulted, 1, "and counted, exactly once");
        assert!(
            w.contracts.penalty(bloc) > relations_before,
            "the bloc did not sour, so every future price is unchanged"
        );
        // The consequence that does not need money to bite, and the reason
        // this mechanic is not a free loan with a waiting period. The treasury
        // refuses to go negative on purpose, so a fine on an empty purse takes
        // nothing at all -- which this test found the first time it ran.
        assert!(
            !w.loans.will_lend(bloc),
            "the bloc would lend again to a republic that never repaid it"
        );
        assert_eq!(
            w.issue(crate::command::Command::TakeLoan {
                market: bloc,
                tier: 0
            }),
            Err(crate::command::Refused::Loan(
                crate::loan::LoanError::Defaulted
            ))
        );
    }

    /// A pump serves what is within reach of it and nothing else.
    ///
    /// The reach is what makes this a *place* rather than a global fuel pool.
    /// It also has to be staffed and hold something: an unstaffed pump is a
    /// building, and an empty one is a disappointment. Each precondition is
    /// asserted separately because a probe found that all three held while the
    /// mechanic still never fired -- the miss was geometric, and a test that
    /// only checked "does it refuel" would have said nothing about which.
    #[test]
    fn a_filling_point_serves_what_is_within_reach_of_it() {
        let mut w = bare();
        let centre = at(2_000.0, 2_000.0);
        staff_up(&mut w, centre, 60);
        let site = crate::scenario::find_site(&w, BuildingKind::GasStation, centre, Metres(700.0))
            .expect("somewhere for a pump");
        let pump = w
            .place_built(BuildingKind::GasStation, site)
            .expect("a pump");

        // Staffed and stocked, but reach still decides.
        for _ in 0..TICKS_PER_DAY {
            w.tick();
        }
        w.buildings
            .get_mut(pump)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(40.0));
        let at_pump = w.buildings.get(pump).unwrap().centre;
        assert!(
            w.buildings.get(pump).unwrap().staffing() > 0.0,
            "the premise: an unstaffed pump serves nobody, so reach would be              untested"
        );

        assert!(
            filling_point(&w, at_pump).is_some(),
            "standing on it and it does not serve you"
        );
        let just_inside = Point::new(at_pump.x + REFUEL_RANGE * 0.9, at_pump.y);
        assert!(filling_point(&w, just_inside).is_some(), "within reach");
        let well_outside = Point::new(at_pump.x + REFUEL_RANGE * 3.0, at_pump.y);
        assert!(
            filling_point(&w, well_outside).is_none(),
            "a pump is a place, not a fuel pool the whole map draws from"
        );

        // Empty is a disappointment rather than a filling point.
        let held = w.buildings.get(pump).unwrap().stock.get(Resource::Fuel);
        w.buildings
            .get_mut(pump)
            .unwrap()
            .stock
            .take(Resource::Fuel, held);
        assert!(
            filling_point(&w, at_pump).is_none(),
            "an empty pump is still offering to fill you up"
        );
    }

    /// A pump that runs dry and is never refilled is a building that works
    /// once — and that is what this was.
    ///
    /// `cover_days` reads `inputs` and a `GasStation` has none, so the resupply
    /// ranking had no reason to bring one a tonne of diesel ever. The founding
    /// hand-stocked forty tonnes, the test above hand-stocked forty tonnes, and
    /// between them they hid a filling station that could only ever run down.
    /// The fix is `orders: true` — a standing order is what makes a place a
    /// destination — and this is the test that watches a lorry turn up.
    ///
    /// Run against a *drained* pump, because a full one proves nothing: the
    /// premise is asserted before the tick loop for exactly that reason.
    #[test]
    fn a_filling_point_is_kept_filled() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let depot = base.depot.expect("the founding sites a council depot");
        let near_depot = w.buildings.get(depot).unwrap().centre;
        let site =
            crate::scenario::find_site(&w, BuildingKind::GasStation, near_depot, Metres(600.0))
                .expect("somewhere for a pump");
        let pump = w
            .place_built(BuildingKind::GasStation, site)
            .expect("a pump");

        // Imported diesel standing at the border, which is where a republic
        // with no refinery gets it. The pump is empty; before the standing
        // order existed, that stayed true for ever.
        let customs = base.customs.expect("the founding opens a crossing");
        w.buildings
            .get_mut(customs)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(60.0));

        assert!(
            !w.buildings
                .get(pump)
                .unwrap()
                .stock
                .get(Resource::Fuel)
                .is_positive(),
            "the premise: a pump with fuel in it would pass without a delivery"
        );
        assert!(
            cover_days(&w, pump, Resource::Fuel).is_none(),
            "the premise: a pump has no appetite, which is exactly why the \
             ordinary resupply ranking never looked at it"
        );

        let cap = w.buildings.get(pump).unwrap().storage_cap();
        w.issue(crate::command::Command::SetStandingOrder {
            building: pump,
            resource: Resource::Fuel,
            tonnes: cap,
        })
        .expect("a filling station is a place the player may stock");

        for _ in 0..(TICKS_PER_DAY * 4) {
            w.tick();
        }

        assert!(
            w.buildings
                .get(pump)
                .unwrap()
                .stock
                .get(Resource::Fuel)
                .is_positive(),
            "four days and no lorry brought the filling station any diesel"
        );
    }

    /// A standing order is an order, not a suggestion.
    ///
    /// **Found by watching a pump that was being delivered to and never had
    /// any.** A store holds goods it does not consume, which is exactly the
    /// definition of a supplier in `serve` — so an order used to set a target
    /// with no floor under it. Freight brought the diesel in and the next pass
    /// took it straight back out to whatever burnt fuel nearby, five times over
    /// four days, and the building ended each one empty.
    ///
    /// The premise is asserted first, because a republic with nothing that
    /// wants the goods would pass this without the rule existing.
    #[test]
    fn what_a_store_is_told_to_keep_is_not_taken_out_again() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        // A founded republic burns coal in its boiler and its power plant, so
        // there is always somebody who would like this stock.
        let plant = base.plant.expect("the founding lights a power plant");
        let near = w.buildings.get(plant).unwrap().centre;
        let site =
            crate::scenario::find_site(&w, BuildingKind::DistributionOffice, near, Metres(900.0))
                .expect("somewhere for an office");
        let office = w
            .place_built(BuildingKind::DistributionOffice, site)
            .expect("an office");
        assert!(
            BuildingKind::PowerPlant
                .def()
                .inputs
                .iter()
                .any(|&(r, _)| r == Resource::Coal),
            "the premise: this test needs something nearby that wants the coal"
        );

        let keep = Tonnes(50.0);
        w.issue(crate::command::Command::SetStandingOrder {
            building: office,
            resource: Resource::Coal,
            tonnes: keep,
        })
        .expect("a distribution office is a store");
        w.buildings
            .get_mut(office)
            .unwrap()
            .stock
            .set(Resource::Coal, keep);
        // Empty the plant's bunker so its appetite is real and pressing.
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .take(Resource::Coal, Tonnes(1e9));

        for _ in 0..(TICKS_PER_DAY * 3) {
            w.tick();
        }

        let left = w.buildings.get(office).unwrap().stock.get(Resource::Coal);
        assert!(
            left.0 >= keep.0 - 1e-6,
            "the office was told to keep {keep:?} and has {left:?} — an order \
             that does not hold anything back is a target, not an order"
        );

        // And the other half: a surplus **above** the order is still fair game,
        // or a store would be a hole goods fall into.
        w.buildings
            .get_mut(office)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(30.0));
        for _ in 0..(TICKS_PER_DAY * 3) {
            w.tick();
        }
        let after = w.buildings.get(office).unwrap().stock.get(Resource::Coal);
        assert!(
            after.0 < keep.0 + 30.0 - 1e-6,
            "nothing drew on the surplus, so a store hoards whatever it is given"
        );
    }

    /// A customs house at an EASTERN frontier post.
    ///
    /// The bloc matters: a house clears only for the bloc whose post it stands
    /// at, and every tender fixture here deals with the East.
    fn crossing(world: &mut World) -> BuildingId {
        let middle = world.terrain.extent() / 2.0;
        let at = world
            .frontier
            .nearest_crossing(Point::new(middle, middle), Some(Market::East))
            .expect("a frontier always has posts of both blocs")
            .at;
        world
            .place_built(BuildingKind::Customs, at)
            .expect("at a frontier post")
    }

    fn live_tender(world: &mut World, amount: f64, deadline_in: u64) -> ContractId {
        let id = world.contracts.reserve_id();
        let today = world.clock.day_index();
        world.contracts.insert(Contract {
            id,
            resource: Resource::Coal,
            market: Market::East,
            amount: Tonnes(amount),
            delivered: Tonnes::ZERO,
            // Well over the spot price, so a payment at this rate is
            // unmistakably the contract's and not the market's.
            price_per_tonne: 10.0,
            deadline_day: today + deadline_in,
            offer_expires_day: today + 30,
            state: ContractState::Active,
            closed_day: None,
        });
        id
    }

    /// A delivery pays the price locked at offer time, not today's — which is
    /// the whole reason to accept a tender rather than sell at spot.
    #[test]
    fn delivering_a_tender_pays_the_locked_price_and_books_the_tonnage() {
        let mut w = bare();
        let house = crossing(&mut w);
        let id = live_tender(&mut w, 20.0, 60);
        w.trade_policy = crate::trade::TradePolicy::new().sell(Resource::Coal, Market::East);
        w.buildings
            .get_mut(house)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(20.0));
        w.buildings.get_mut(house).unwrap().staff = BuildingKind::Customs.def().workers;

        // A day of clearing at the border.
        for _ in 0..TICKS_PER_DAY {
            let mutations = trade(&w);
            apply(&mut w, &mutations);
        }

        let contract = *w.contracts.get(id).expect("the tender is still there");
        assert!(contract.delivered.is_positive(), "nothing was booked");
        // Paid at 10/t, far above coal's spot price of 2.5 x 0.8.
        let spot = Market::East.sell_price(Resource::Coal);
        assert!(
            w.treasury.rubles > contract.delivered.0 * spot,
            "paid {} for {} t — that is the spot price, not the contract's",
            w.treasury.rubles,
            contract.delivered.0
        );
    }

    /// Filling a tender closes it, in the same transaction as the last tonne.
    #[test]
    fn a_tender_filled_in_full_closes_itself() {
        let mut w = bare();
        let house = crossing(&mut w);
        let id = live_tender(&mut w, 5.0, 60);
        w.trade_policy = crate::trade::TradePolicy::new().sell(Resource::Coal, Market::East);
        w.buildings
            .get_mut(house)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(50.0));
        w.buildings.get_mut(house).unwrap().staff = BuildingKind::Customs.def().workers;

        for _ in 0..TICKS_PER_DAY {
            let mutations = trade(&w);
            apply(&mut w, &mutations);
        }
        let contract = *w.contracts.get(id).expect("still on the books");
        assert_eq!(contract.state, ContractState::Done);
        assert!(contract.closed_day.is_some());
        assert!(!contract.outstanding().is_positive());
    }

    /// A missed deadline costs money and goodwill — and the goodwill comes back
    /// on its own, so a bad year is a scar rather than a permanent state.
    #[test]
    fn a_missed_deadline_fines_the_republic_and_sours_the_bloc() {
        let mut w = bare();
        crossing(&mut w);
        let id = live_tender(&mut w, 100.0, 1);
        w.treasury.credit(Market::East, 500.0);
        let before = w.treasury.rubles;

        // Two days: the deadline passes on the first day boundary after it.
        for _ in 0..TICKS_PER_DAY * 3 {
            w.tick();
        }

        let contract = *w.contracts.get(id).expect("failures stay on the books");
        assert_eq!(contract.state, ContractState::Failed);
        assert!(w.treasury.rubles < before, "a failure cost nothing");
        let soured = w.contracts.penalty(Market::East);
        assert!(soured > 0.0, "the bloc did not mind at all");
        // And the west is unaffected — relations are per-bloc.
        assert_eq!(w.contracts.penalty(Market::West), 0.0);

        // Now let it forget.
        for _ in 0..TICKS_PER_DAY * 40 {
            w.tick();
        }
        assert!(
            w.contracts.penalty(Market::East) < soured,
            "relations never recovered"
        );
    }

    /// The fine can never overdraw. An empty treasury simply pays nothing,
    /// which is the same rule the treasury applies to every other spend.
    #[test]
    fn a_fine_cannot_put_the_republic_into_debt() {
        let mut w = bare();
        crossing(&mut w);
        live_tender(&mut w, 1_000.0, 1);
        assert_eq!(w.treasury.rubles, 0.0);

        for _ in 0..TICKS_PER_DAY * 3 {
            w.tick();
        }
        assert_eq!(w.treasury.rubles, 0.0, "the republic went into debt");
    }

    /// No crossing, no tender. Trade is physical all the way down: a bloc
    /// cannot offer business to a republic with nowhere to receive it.
    #[test]
    fn no_tender_arrives_without_a_crossing_to_land_at() {
        let mut w = bare();
        // A year of simulated time with no customs house at all.
        for _ in 0..TICKS_PER_DAY * 120 {
            w.tick();
        }
        assert_eq!(w.contracts.all().len(), 0);
    }

    /// An offer nobody takes is withdrawn rather than standing forever.
    #[test]
    fn an_offer_nobody_accepts_is_withdrawn() {
        let mut w = bare();
        crossing(&mut w);
        let id = w.contracts.reserve_id();
        let today = w.clock.day_index();
        w.contracts.insert(Contract {
            id,
            resource: Resource::Coal,
            market: Market::East,
            amount: Tonnes(10.0),
            delivered: Tonnes::ZERO,
            price_per_tonne: 4.0,
            deadline_day: today + 90,
            offer_expires_day: today + 2,
            state: ContractState::Offer,
            closed_day: None,
        });

        for _ in 0..TICKS_PER_DAY * 4 {
            w.tick();
        }
        assert!(
            w.contracts.get(id).is_none(),
            "a stale offer stayed on the table"
        );
    }

    /// An offer only becomes an obligation when the player says so. Nothing in
    /// the simulation may accept one on their behalf.
    #[test]
    fn a_tender_binds_nobody_until_it_is_accepted() {
        let mut w = bare();
        crossing(&mut w);
        let id = w.contracts.reserve_id();
        let today = w.clock.day_index();
        w.contracts.insert(Contract {
            id,
            resource: Resource::Coal,
            market: Market::East,
            amount: Tonnes(10.0),
            delivered: Tonnes::ZERO,
            price_per_tonne: 4.0,
            // Deadline already in the past: an *offer* must not fail for it.
            deadline_day: today,
            offer_expires_day: today + 30,
            state: ContractState::Offer,
            closed_day: None,
        });

        for _ in 0..TICKS_PER_DAY * 3 {
            w.tick();
        }
        assert_eq!(
            w.contracts.get(id).map(|c| c.state),
            Some(ContractState::Offer),
            "an unaccepted offer was treated as an obligation"
        );
        assert_eq!(w.treasury.rubles, 0.0);
    }

    /// Two customs houses clearing in the same tick must not both be credited
    /// with filling the same last tonnes — the scratch-ledger guard, and the
    /// same class of bug the households system has its own ledger for.
    #[test]
    fn two_crossings_cannot_fill_the_same_tender_twice() {
        let mut w = bare();
        // Two houses at two different frontier posts. The posts exist at
        // worldgen, so this takes the first two rather than inventing sites --
        // and two posts on a four-post frontier is the ordinary case rather
        // than a contrivance.
        let sites: Vec<Point> = w
            .frontier
            .crossings()
            .iter()
            .filter(|c| c.bloc == Market::East)
            .take(2)
            .map(|c| c.at)
            .collect();
        assert_eq!(
            sites.len(),
            2,
            "this needs two EASTERN posts: a house clears only for the bloc it              stands at, so two posts of different blocs cannot contend for the              same tender at all"
        );
        let houses: Vec<BuildingId> = sites
            .into_iter()
            .map(|p| {
                w.place_built(BuildingKind::Customs, p)
                    .expect("at a frontier post")
            })
            .collect();

        let id = live_tender(&mut w, 10.0, 60);
        w.trade_policy = crate::trade::TradePolicy::new().sell(Resource::Coal, Market::East);
        for &house in &houses {
            let b = w.buildings.get_mut(house).unwrap();
            b.stock.add(Resource::Coal, Tonnes(30.0));
            b.staff = BuildingKind::Customs.def().workers;
        }

        for _ in 0..TICKS_PER_DAY {
            let mutations = trade(&w);
            apply(&mut w, &mutations);
        }

        let contract = *w.contracts.get(id).expect("on the books");
        assert!(
            contract.delivered.0 <= 10.0 + 1e-9,
            "the tender was over-delivered to {} t against an order of 10 t",
            contract.delivered.0
        );
    }

    // ---- Rail, water and air: three confined media -------------------------

    /// Lay a finished way of any grade, the way `energise` lays a finished
    /// span. Most tests here are about running a republic rather than building
    /// one; the construction of a way has its own tests.
    fn lay(world: &mut World, grade: crate::roadworks::Grade, from: Point, to: Point) {
        let id = world.order_road(from, to, grade).expect("orderable");
        let site = world.roadworks.remove(id).expect("just ordered");
        crate::roadworks::open(world.network_for(grade), &site);
    }

    /// A staffed, fuelled terminal with its vehicles on the strength.
    fn terminal(world: &mut World, kind: BuildingKind, at: Point) -> BuildingId {
        let id = world.place_built(kind, at).expect("beside its way");
        let def = kind.def();
        let b = world.buildings.get_mut(id).unwrap();
        b.staff = def.workers;
        b.stock.add(Resource::Fuel, Tonnes(30.0));
        for &(vehicle, n) in def.vehicles {
            for _ in 0..n {
                world.fleet.commission(vehicle, id, at);
            }
        }
        id
    }

    /// The whole mechanic in one assertion, and the reason rails needed no rule
    /// in the dispatcher: **a train cannot plan a journey off the rails**, so a
    /// job it cannot reach is one it is never offered.
    #[test]
    fn a_train_can_only_plan_where_there_are_rails() {
        let mut w = bare();
        let def = VehicleKind::Locomotive.def();
        let (a, b) = (at(500.0, 1_000.0), at(3_000.0, 1_000.0));

        {
            let crossing = w.crossing();
            assert!(
                crate::journey::plan_for(
                    def.medium,
                    a,
                    b,
                    w.ways(),
                    &crossing,
                    def.on_road,
                    def.cross_country,
                    0.0
                )
                .is_none(),
                "a locomotive planned a journey across a republic with no railway in it"
            );
        }

        lay(&mut w, crate::roadworks::Grade::Railway, a, b);
        let crossing = w.crossing();
        let plan = crate::journey::plan_for(
            def.medium,
            a,
            b,
            w.ways(),
            &crossing,
            def.on_road,
            def.cross_country,
            0.0,
        )
        .expect("the rails now run all the way");
        assert!(
            plan.limit.iter().all(|l| l.is_some()),
            "a leg of a rail journey was off the network"
        );
    }

    /// The counterpart, and the one that would pass for the wrong reason if the
    /// networks were shared: a **road** all the way from A to B must not let a
    /// train run. Peer of `the_two_networks_never_touch` on the utility side.
    #[test]
    fn a_train_cannot_ride_a_road() {
        let mut w = bare();
        let (a, b) = (at(500.0, 1_000.0), at(3_000.0, 1_000.0));
        lay(&mut w, crate::roadworks::Grade::Paved, a, b);
        assert!(
            w.roads.segment_count() > 0,
            "the fixture laid no road, so this proves nothing"
        );
        assert_eq!(w.rails.segment_count(), 0, "a road went into the rails");

        let def = VehicleKind::Locomotive.def();
        let crossing = w.crossing();
        assert!(
            crate::journey::plan_for(
                def.medium,
                a,
                b,
                w.ways(),
                &crossing,
                def.on_road,
                def.cross_country,
                0.0
            )
            .is_none(),
            "a locomotive routed itself down a paved road"
        );
    }

    /// A terminal has to stand beside the way it serves, and the refusal says
    /// which way. Same shape of rule as the customs house's.
    #[test]
    fn a_station_cannot_be_built_away_from_the_rails() {
        let mut w = bare();
        let (a, b) = (at(500.0, 1_000.0), at(3_000.0, 1_000.0));
        lay(&mut w, crate::roadworks::Grade::Railway, a, b);

        assert_eq!(
            w.place(BuildingKind::RailwayStation, at(1_500.0, 3_000.0)),
            Err(crate::building::PlacementError::NoWayThere(
                crate::journey::Medium::Rail
            )),
            "a station went up two kilometres from the nearest rail"
        );
        assert!(
            w.place(BuildingKind::RailwayStation, at(1_500.0, 1_060.0))
                .is_ok(),
            "a station beside the line was refused"
        );
    }

    /// **A station with no standing order is a shed.** Nothing in the republic
    /// wants to deliver to it, because it consumes nothing and sells nothing —
    /// and that is deliberate rather than an oversight, because a terminal that
    /// hoovered up whatever was passing would be making the player's
    /// distribution decisions for them.
    #[test]
    fn a_standing_order_is_what_makes_a_terminal_a_destination() {
        let run = |order: f64| -> Tonnes {
            let mut w = bare();
            let (a, b) = (at(500.0, 1_000.0), at(2_500.0, 1_000.0));
            lay(&mut w, crate::roadworks::Grade::Railway, a, b);
            // A road too, or the lorries that stock the station cannot reach it.
            lay(&mut w, crate::roadworks::Grade::Gravel, a, b);
            let station = terminal(&mut w, BuildingKind::RailwayStation, at(2_500.0, 1_060.0));

            // Somewhere with coal, and lorries to move it.
            let pit = place(&mut w, BuildingKind::Warehouse, at(500.0, 1_060.0));
            w.buildings
                .get_mut(pit)
                .unwrap()
                .stock
                .add(Resource::Coal, Tonnes(200.0));
            terminal(&mut w, BuildingKind::MotorDepot, at(760.0, 1_060.0));
            staff_up(&mut w, at(1_000.0, 1_060.0), 80);

            if order > 0.0 {
                w.issue(crate::command::Command::SetStandingOrder {
                    building: station,
                    resource: Resource::Coal,
                    tonnes: Tonnes(order),
                })
                .expect("a station keeps goods to order");
            }
            for _ in 0..TICKS_PER_DAY * 6 {
                w.tick();
            }
            w.buildings.get(station).unwrap().stock.get(Resource::Coal)
        };

        let unordered = run(0.0);
        let ordered = run(60.0);
        assert_eq!(
            unordered,
            Tonnes::ZERO,
            "coal was delivered to a station nobody ordered any to"
        );
        assert!(
            ordered.is_positive(),
            "a station with a standing order for 60 t received nothing"
        );
        // And the order is a ceiling, not a suggestion.
        assert!(
            ordered.0 <= 60.0 + 1e-6,
            "the order was for 60 t and {:.1} t turned up",
            ordered.0
        );
    }

    /// Drink lifts a block and costs the people in it health, and both halves
    /// are real in a running republic.
    ///
    /// **The trade the player is being asked to make**, and it is asserted in
    /// both directions from the same fixture so neither half can be true on its
    /// own. The premise — that the vodka actually reaches the shelves — is
    /// checked before either, because a shop nobody stocked would give the same
    /// answer as a mechanic that does nothing.
    #[test]
    fn drink_lifts_a_block_and_costs_it_health() {
        let run = |stock_drink: bool| -> (f64, f64, f64) {
            let mut w = bare();
            let town = at(2_000.0, 2_000.0);
            let home = staff_up(&mut w, town, 40);
            let shop = crate::scenario::find_site(&w, BuildingKind::Store, town, Metres(300.0))
                .expect("somewhere for a shop");
            let store = w.place_built(BuildingKind::Store, shop).expect("a shop");
            {
                let b = w.buildings.get_mut(store).unwrap();
                b.staff = BuildingKind::Store.def().workers;
                // Fed and clothed either way, so the only thing that differs
                // between the two runs is the drink.
                b.stock.add(Resource::Food, Tonnes(30.0));
                b.stock.add(Resource::Clothes, Tonnes(30.0));
                if stock_drink {
                    b.stock.add(Resource::Alcohol, Tonnes(30.0));
                }
            }
            for _ in 0..(TICKS_PER_DAY * 20) {
                w.tick();
            }
            let block = w.buildings.get(home).unwrap();
            let (health, _) = w.population().mean_wellbeing();
            (block.content.overall(), block.content.lift(), health)
        };

        let (dry_score, dry_lift, dry_health) = run(false);
        let (wet_score, wet_lift, wet_health) = run(true);

        assert_eq!(
            dry_lift, 0.0,
            "the premise: a republic with no drink should be lifted by nothing"
        );
        assert!(
            wet_lift > 0.0,
            "the premise: the vodka never reached anybody, so neither half below \
             is being tested"
        );

        assert!(
            wet_score > dry_score,
            "drink was on the shelves and the block is no happier: \
             {dry_score:.3} then {wet_score:.3}"
        );
        assert!(
            wet_health < dry_health,
            "the republic drank for twenty days and is exactly as healthy: \
             {dry_health:.4} then {wet_health:.4}"
        );

        // And the needs half is untouched, which is what stops the lift being
        // mistaken for the republic having fed anybody better.
        let needs = wet_score - wet_lift;
        assert!(
            (needs - dry_score).abs() < 1e-9,
            "supplying drink moved what the republic is judged on: \
             {dry_score:.4} against {needs:.4}"
        );
    }

    /// Visitors arrive at a post, are driven to a hotel, and leave money.
    ///
    /// The whole mechanic end to end, and every premise is asserted first
    /// because each is a way this could pass while doing nothing: no beds, no
    /// coach, or a republic already rich enough that the takings vanish in the
    /// noise.
    #[test]
    fn visitors_are_fetched_from_the_border_and_pay_in_their_own_money() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let centre = base.centre;

        // Hands to spare. The founding sends **exactly** enough people for its
        // own jobs, so anything commissioned afterwards stands unstaffed for
        // ever — which is the staffing order working, and would make this a test
        // of that rather than of tourism. An estate and sixty people is what
        // lets the buildings below actually open.
        let estate = crate::scenario::find_site(&w, BuildingKind::Apartment, centre, Metres(900.0))
            .expect("somewhere for another block");
        let block = w
            .place_built(BuildingKind::Apartment, estate)
            .expect("a block");
        for _ in 0..60 {
            w.population.spawn_citizen(block, 30);
        }

        // A hotel, a coach depot to fetch with, and something worth coming for.
        let mut opened = Vec::new();
        for kind in [
            BuildingKind::Hotel,
            BuildingKind::BusDepot,
            BuildingKind::CultureClub,
        ] {
            let site = crate::scenario::find_site(&w, kind, centre, Metres(900.0))
                .unwrap_or_else(|| panic!("somewhere for a {kind:?}"));
            let id = w.place_built(kind, site).expect("open ground");
            let b = w.buildings.get_mut(id).unwrap();
            b.staff = kind.def().workers;
            b.stock.add(Resource::Fuel, Tonnes(30.0));
            opened.push(id);
        }
        for &(kind, n) in BuildingKind::BusDepot.def().vehicles {
            for _ in 0..n {
                w.fleet.commission(kind, opened[1], centre);
            }
        }

        assert!(
            w.free_beds() > 0,
            "the premise: nowhere for a visitor to sleep"
        );
        assert!(
            w.fleet
                .all()
                .iter()
                .any(|v| v.def().role == crate::fleet::Role::Passenger),
            "the premise: nothing to fetch anybody with"
        );
        let hotel_at = w.buildings.get(opened[0]).unwrap().centre;
        assert!(
            w.appeal_at(hotel_at) > crate::tourism::APPEAL_FLOOR,
            "the premise: a hotel beside nothing earns the floor, and this test \
             wants to see the culture club counted"
        );

        let before = (w.treasury.rubles, w.treasury.dollars);
        for _ in 0..(TICKS_PER_DAY * 60) {
            w.tick();
        }

        assert!(
            w.tourism.visited() > 0,
            "sixty days and nobody ever reached a hotel"
        );
        let earned = w.tourism.earned(Market::East) + w.tourism.earned(Market::West);
        assert!(earned > 0.0, "visitors stayed and spent nothing");
        let after = (w.treasury.rubles, w.treasury.dollars);
        assert!(
            after.0 > before.0 || after.1 > before.1,
            "the takings never reached the treasury"
        );

        // Their money is their bloc's. A republic whose only reachable posts are
        // Western earns dollars from tourism exactly as it does from coal, which
        // is the whole reason this is geographic rather than a flat income.
        for market in [Market::East, Market::West] {
            if w.tourism.earned(market) > 0.0 {
                let posts = w
                    .frontier
                    .crossings()
                    .iter()
                    .filter(|c| c.bloc == market)
                    .count();
                assert!(posts > 0, "{market:?} money from a bloc with no post here");
            }
        }
    }

    /// What a hotel is worth is what is near it, and it is showable.
    ///
    /// A player who cannot see why one hotel earns three times another has a
    /// building with a random yield. The floor is asserted too, because a
    /// multiplier that reached zero would make the whole mechanic unreachable
    /// on an empty posting — a lock wearing a balance curve's clothes.
    #[test]
    fn a_hotel_is_worth_what_stands_around_it() {
        let mut w = bare();
        let bare_spot = at(3_000.0, 500.0);
        let town = at(1_000.0, 1_000.0);
        staff_up(&mut w, town, 40);

        let empty = w.appeal_at(bare_spot);
        assert!(
            empty >= crate::tourism::APPEAL_FLOOR,
            "an empty posting earns nothing at all for having built a hotel"
        );

        let club = crate::scenario::find_site(&w, BuildingKind::CultureClub, town, Metres(300.0))
            .expect("somewhere for a club");
        let id = w
            .place_built(BuildingKind::CultureClub, club)
            .expect("a club");
        w.buildings.get_mut(id).unwrap().staff = BuildingKind::CultureClub.def().workers;

        let with_culture = w.appeal_at(town);
        assert!(
            with_culture > empty,
            "a culture club next door was worth nothing: {empty:.2} then {with_culture:.2}"
        );

        // And smoke takes it back off, which is why siting a hotel downwind of
        // the works is a decision rather than a detail.
        for cell in w.lattice.cells_within(town, Metres(150.0)) {
            w.lattice.foul(cell, 1.0);
        }
        let in_a_smog = w.appeal_at(town);
        assert!(
            in_a_smog < with_culture,
            "a hotel in a smog is worth as much as one in clean air: \
             {with_culture:.2} then {in_a_smog:.2}"
        );
    }

    /// The winter arrives, a plough goes out, and the road comes back.
    ///
    /// The whole mechanic end to end, and the premises are asserted first
    /// because every one of them is a way this could pass while doing nothing:
    /// a republic with no snow, no ploughs or no roads would sail through a test
    /// that only looked at the end state.
    #[test]
    fn a_republic_ploughs_its_own_roads_out() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Taiga,
        });
        let base = crate::scenario::town(&mut w, crate::scenario::SETTLERS);

        // Road out to the crossing. The founding does not lay one — the
        // trajectory runner orders it, and so does a player — so it is ordered
        // here rather than assumed, and finished outright because six weeks of
        // waiting for a crew would make this a construction test.
        let depot = base.depot.expect("a council depot");
        let yard = w.buildings.get(depot).unwrap().centre;
        let house = base.customs.expect("a crossing");
        let border = w.buildings.get(house).unwrap().centre;
        lay(&mut w, crate::roadworks::Grade::Gravel, yard, border);

        // A day for the founding to staff itself and put the ploughs on the
        // depot's strength.
        for _ in 0..(TICKS_PER_DAY * 2) {
            w.tick();
        }

        let ploughs = w
            .fleet
            .all()
            .iter()
            .filter(|v| v.def().role == crate::fleet::Role::Clearance)
            .count();
        assert!(
            ploughs > 0,
            "the premise: the council depot keeps ploughs, and without one \
             nothing below is testing anything"
        );
        assert!(
            !w.roads.segments().is_empty(),
            "the premise: the founding lays a track to its crossing, and a \
             republic with no roads has nothing to clear"
        );

        // Midwinter, laid on rather than waited for: this test is about the
        // plough, and driving it there through six simulated months would make
        // it a test of the taiga calendar.
        w.ground.snow = crate::ground::SNOW_BLOCKS_MM * 2.0;
        w.ground.frost = 1.0;
        w.lattice.bury(1.0);
        let before = w.roads_unswept();
        assert!(
            before > 0.9,
            "the premise: the republic has to actually be under snow, and it is \
             {before:.2} buried"
        );

        let road = w.roads.segments()[0].clone();
        let (from, to) = w.roads.segment_ends(&road).expect("a segment has ends");
        let buried_leg = w.crossing().road_drag_along(from, to);
        assert!(
            buried_leg > 1.0,
            "the premise: a buried road has to cost something, and this one \
             drags {buried_leg:.2}"
        );

        let mut went_out = 0;
        for _ in 0..(TICKS_PER_DAY * 5) {
            for m in w.tick() {
                if let Mutation::Dispatch { job, .. } = &m
                    && matches!(job, Job::Plough { .. })
                {
                    went_out += 1;
                }
            }
            // Hold the winter: `weather` would otherwise melt the pack and the
            // thaw would clear the lattice for reasons that are not a plough.
            w.ground.snow = crate::ground::SNOW_BLOCKS_MM * 2.0;
        }

        assert!(went_out > 0, "nothing was sent out into the snow");
        let after = w.roads_unswept();
        assert!(
            after < before,
            "ploughs went out {went_out} times and the republic is as buried as \
             it was: {before:.3} then {after:.3}"
        );
        let swept_leg = w.crossing().road_drag_along(from, to);
        assert!(
            swept_leg < buried_leg,
            "the road was ploughed and a lorry is no quicker on it: {buried_leg:.2} \
             then {swept_leg:.2}"
        );
    }

    /// A store takes the shapes it was built for, and says so at the door.
    ///
    /// The refusal matters as much as the rule. Letting the order stand and
    /// quietly never fill would be indistinguishable, from the panel, from a
    /// republic with no lorries — and the player would have no way to find out
    /// which, because `intake_capacity` returns zero silently.
    #[test]
    fn a_store_refuses_an_order_it_could_never_fill() {
        let mut w = bare();
        let tank = place(&mut w, BuildingKind::StorageTank, at(1_000.0, 1_000.0));
        let silo = place(&mut w, BuildingKind::GrainSilo, at(1_400.0, 1_000.0));

        w.issue(crate::command::Command::SetStandingOrder {
            building: tank,
            resource: Resource::Fuel,
            tonnes: Tonnes(100.0),
        })
        .expect("a tank holds fuel");

        let refused = w
            .issue(crate::command::Command::SetStandingOrder {
                building: tank,
                resource: Resource::Coal,
                tonnes: Tonnes(100.0),
            })
            .expect_err("a tank is not a coal bunker");
        assert_eq!(
            refused.to_string(),
            "Coal will not go in a Storage Tank",
            "the reason has to be a sentence a tooltip can print"
        );

        w.issue(crate::command::Command::SetStandingOrder {
            building: silo,
            resource: Resource::Crops,
            tonnes: Tonnes(100.0),
        })
        .expect("a silo holds grain");
        assert!(
            w.issue(crate::command::Command::SetStandingOrder {
                building: silo,
                resource: Resource::Steel,
                tonnes: Tonnes(100.0),
            })
            .is_err(),
            "a silo took delivery of steel beams"
        );

        // And the panel is offered exactly what would be accepted, so a player
        // is never shown a choice the simulation would refuse.
        let offered = w.orderable(tank);
        assert!(offered.contains(&Resource::Fuel));
        assert!(!offered.contains(&Resource::Coal));
        assert!(
            offered
                .iter()
                .all(|r| r.form() == crate::resource::Form::Liquid),
            "a tank offered something that is not a liquid"
        );
        let home = place(&mut w, BuildingKind::Apartment, at(1_800.0, 1_000.0));
        assert!(
            w.orderable(home).is_empty(),
            "a block of flats was offered a stockpile to keep"
        );
    }

    /// The one network nobody builds. A republic on a river has bulk haulage
    /// from day one, and one sited away from water does not — a difference in
    /// the land rather than in what the player did.
    #[test]
    fn navigable_water_comes_from_the_ground_rather_than_from_a_crew() {
        let mut w = bare();
        assert_eq!(
            w.waterways.node_count(),
            0,
            "the flat test fixture has no water, so this would prove nothing"
        );

        // Cut a channel five hundred metres wide across the map.
        let mut ground = crate::terrain::Terrain::flat(Metres(4_000.0));
        for cy in 0..ground.cells() {
            for cx in 0..ground.cells() {
                let p = ground.cell_centre(cx, cy);
                if (1_000.0..1_500.0).contains(&p.y.0) {
                    ground.set_surface(p, crate::terrain::Surface::Water);
                }
            }
        }
        w.set_terrain(ground);

        assert!(
            w.waterways.node_count() > 0,
            "a river across the map produced no fairway"
        );
        let def = VehicleKind::Barge.def();
        let crossing = w.crossing();
        assert!(
            crate::journey::plan_for(
                def.medium,
                at(300.0, 1_250.0),
                at(3_700.0, 1_250.0),
                w.ways(),
                &crossing,
                def.on_road,
                def.cross_country,
                0.0
            )
            .is_some(),
            "a barge could not follow a channel running the width of the map"
        );
        // And it cannot leave it.
        assert!(
            crate::journey::plan_for(
                def.medium,
                at(300.0, 1_250.0),
                at(2_000.0, 3_000.0),
                w.ways(),
                &crossing,
                def.on_road,
                def.cross_country,
                0.0
            )
            .is_none(),
            "a barge sailed onto dry land"
        );
    }

    /// An aeroplane lands at an aerodrome. Giving air a network whose nodes are
    /// the aerodromes puts it under exactly the rule the other two obey; the
    /// alternative was a special case in the dispatcher saying so.
    #[test]
    fn an_aeroplane_flies_between_aerodromes_and_nowhere_else() {
        let mut w = bare();
        let def = VehicleKind::Freighter.def();
        let (north, south) = (at(800.0, 800.0), at(3_200.0, 3_200.0));

        fn reaches(w: &World, a: Point, b: Point) -> bool {
            let def = VehicleKind::Freighter.def();
            let crossing = w.crossing();
            crate::journey::plan_for(
                def.medium,
                a,
                b,
                w.ways(),
                &crossing,
                def.on_road,
                def.cross_country,
                0.0,
            )
            .is_some()
        }

        assert!(!reaches(&w, north, south), "flew with no aerodrome at all");
        terminal(&mut w, BuildingKind::Aerodrome, north);
        w.re_survey_airways();
        assert!(
            !reaches(&w, north, south),
            "flew to a place with no aerodrome on it"
        );
        terminal(&mut w, BuildingKind::Aerodrome, south);
        w.re_survey_airways();
        assert!(reaches(&w, north, south), "two aerodromes and no route");
        let _ = def;
    }

    /// Nothing on rails, water or in the air can bog. It falls out of the rule
    /// roads already had — `sticks` skips any leg with a speed limit on it, and
    /// **every** leg of a confined journey has one — rather than needing a
    /// medium check of its own, which is why this asserts it rather than
    /// trusting the reading.
    #[test]
    fn a_confined_vehicle_never_bogs_however_bad_the_ground() {
        let mut w = bare();
        // Saturate the ground: a lorry would be in serious trouble here.
        w.ground.moisture = 1.0;
        let (a, b) = (at(500.0, 1_000.0), at(3_000.0, 1_000.0));
        lay(&mut w, crate::roadworks::Grade::Railway, a, b);
        let station = terminal(&mut w, BuildingKind::RailwayStation, at(500.0, 1_060.0));
        let train = w
            .fleet
            .of_garage(station)
            .find(|v| v.kind == VehicleKind::Locomotive)
            .expect("a station keeps locomotives")
            .id;

        let def = VehicleKind::Locomotive.def();
        let crossing = w.crossing();
        let plan = crate::journey::plan_for(
            def.medium,
            a,
            b,
            w.ways(),
            &crossing,
            def.on_road,
            def.cross_country,
            0.0,
        )
        .expect("the rails run");
        assert!(
            crossing.going_at(at(1_750.0, 1_000.0)) > 0.5,
            "the fixture's ground is firm, so this proves nothing"
        );
        for leg in 0..plan.legs() {
            assert!(
                !sticks(&w, &crossing, train, def.ground, &plan, leg, 0),
                "a locomotive bogged on leg {leg}"
            );
        }
    }

    /// The trade in one number. A locomotive shifts fifteen lorry-loads behind
    /// one driver, for a third of the fuel per tonne-kilometre — and it goes
    /// exactly where the track was laid.
    #[test]
    fn a_train_moves_far_more_for_far_less_than_a_lorry() {
        let train = VehicleKind::Locomotive.def();
        let lorry = VehicleKind::Lorry.def();
        assert!(train.capacity.0 >= lorry.capacity.0 * 10.0);
        let per_tonne_km = |d: &crate::fleet::VehicleDef| d.fuel_per_km / d.capacity.0;
        assert!(
            per_tonne_km(train) < per_tonne_km(lorry) / 2.0,
            "a train is no cheaper per tonne-kilometre than a lorry, so rails buy nothing"
        );
        // And the barge is cheaper still, on a network nobody paid to lay.
        assert!(per_tonne_km(VehicleKind::Barge.def()) < per_tonne_km(train));
        // Air is the other end: fast, small, and for what cannot wait.
        let air = VehicleKind::Freighter.def();
        assert!(per_tonne_km(air) > per_tonne_km(lorry) * 5.0);
        assert!(air.on_road.as_mps() > train.on_road.as_mps() * 3.0);
    }

    /// The fairway is **derived** from the ground, and a derived thing that is
    /// also stored can drift from what it was derived from.
    ///
    /// It is stored rather than rebuilt on load, for the same reason the
    /// traversal lattice is: automatic and correct beats a step somebody has to
    /// remember. The risk that buys is this one, so it is asserted rather than
    /// reasoned about — `set_terrain` is the only thing that may replace the
    /// ground, and it must replace the water with it. A republic whose river
    /// moved and whose barges did not would be one where nothing can get
    /// anywhere for no visible reason, which is exactly the failure the lattice
    /// rule exists to prevent.
    #[test]
    fn the_fairway_always_matches_the_ground_it_was_read_from() {
        let mut w = bare();
        let mut ground = crate::terrain::Terrain::flat(Metres(4_000.0));
        for cy in 0..ground.cells() {
            for cx in 0..ground.cells() {
                let p = ground.cell_centre(cx, cy);
                if (1_000.0..1_600.0).contains(&p.y.0) {
                    ground.set_surface(p, crate::terrain::Surface::Water);
                }
            }
        }
        w.set_terrain(ground);
        assert!(
            w.waterways.node_count() > 0,
            "the fixture cut no channel, so this proves nothing"
        );

        let round_tripped = World::from_bytes(&w.to_bytes()).expect("a save it just wrote");
        assert_eq!(
            round_tripped.waterways,
            crate::network::navigable(round_tripped.terrain()),
            "the saved fairway is not what the saved ground implies"
        );
        assert_eq!(round_tripped.waterways, w.waterways);
    }

    /// A **real** generated republic has water a barge could use, not merely
    /// the synthetic channel the fixture above cuts by hand.
    ///
    /// This is the premise assertion for the whole water half, and it caught
    /// the thing that made it worth writing. The fairway was first sampled at
    /// 100 m and required the whole cell to be wet, so nothing narrower than a
    /// hundred metres counted: the standard founding reported 1.2 km of
    /// navigable water, all of it lake, with every river excluded. A barge
    /// mechanic on that map is a mechanic with no subject — and every unit test
    /// of it would still have passed, because they cut their own channels.
    #[test]
    fn a_founded_republic_has_water_worth_putting_a_barge_on() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        crate::scenario::town(&mut w, crate::scenario::SETTLERS);
        let km = w
            .network(crate::journey::Medium::Water)
            .total_length()
            .as_km();
        assert!(
            km > 5.0,
            "a founded republic has {km:.1} km of navigable water — a barge has nowhere to go"
        );
    }

    // ---- Passenger modes ---------------------------------------------------

    /// A tram carries people the bus could not, over track the bus cannot use.
    ///
    /// The whole passenger half in one test: the same two places, the same
    /// people, and the only difference is which way the republic built.
    #[test]
    fn a_tramway_reaches_work_a_bus_route_does_not() {
        let far = at(3_400.0, 1_000.0);
        let home = at(400.0, 1_000.0);

        let mut w = bare();
        // No road between them at all, so a bus is no help however many seats
        // it has: a bus rides the road network and there is none.
        let depot = terminal(&mut w, BuildingKind::BusDepot, at(500.0, 1_000.0));
        w.buildings
            .get_mut(depot)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(20.0));
        assert!(
            crate::transport::reach_by(
                home,
                far,
                w.ways(),
                &crate::transport::services(&w.buildings)
            )
            .is_none(),
            "somebody rode a bus across a republic with no road in it"
        );

        // Now lay tramway and put a tram depot on it.
        lay(&mut w, crate::roadworks::Grade::Tramway, home, far);
        let trams = terminal(&mut w, BuildingKind::TramDepot, at(900.0, 1_060.0));
        w.buildings.get_mut(trams).unwrap().powered = true;
        let services = crate::transport::services(&w.buildings);
        let commute = crate::transport::reach_by(home, far, w.ways(), &services)
            .expect("the tramway runs the whole way");
        assert_eq!(
            commute.medium(),
            Some(crate::journey::Medium::Tram),
            "the journey was made by something other than the tram"
        );
    }

    /// **A pool per way, not one pool.**
    ///
    /// A republic whose trams are full has not thereby run out of buses, and
    /// one pool would make the choice between laying tramway and buying more
    /// buses mean nothing.
    ///
    /// This test is the second attempt and the first is the instructive part.
    /// It built two `Service` values by hand and called `reach_by` with one of
    /// them emptied — which proves the *planner* skips a service with no seats
    /// and says nothing whatever about the labour pass, where the pools are
    /// actually drawn down. Sabotaging that bookkeeping left it green. It now
    /// runs the pass: work reachable **only** by tram, a barely-staffed tram
    /// depot and a fully-staffed bus depot, so booking against the wrong pool
    /// carries far more people than there are tram seats.
    #[test]
    fn a_full_service_does_not_draw_seats_from_another() {
        let mut w = bare();
        let home_at = at(400.0, 1_000.0);
        let work_at = at(3_400.0, 1_000.0);
        // Tramway all the way, and deliberately no road between the two: a bus
        // rides the road network, so every one of these journeys must be a
        // tram or none of them can happen.
        lay(&mut w, crate::roadworks::Grade::Tramway, home_at, work_at);

        // Buses, plentiful and useless here.
        let buses = terminal(&mut w, BuildingKind::BusDepot, at(600.0, 1_060.0));
        w.buildings
            .get_mut(buses)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(60.0));

        // Trams, barely staffed, so the pool is small and countable.
        let trams = w
            .place_built(BuildingKind::TramDepot, at(1_000.0, 1_060.0))
            .expect("beside the tramway");
        w.buildings.get_mut(trams).unwrap().powered = true;
        w.buildings.get_mut(trams).unwrap().staff = 1;
        let tram_seats = crate::transport::services(&w.buildings)
            .into_iter()
            .find(|s| s.medium == crate::journey::Medium::Tram)
            .expect("the trams run")
            .seats;
        assert!(
            tram_seats > 0 && tram_seats < 200,
            "the tram pool is {tram_seats} seats — too big or too small to discriminate"
        );

        // Workplaces out at the far end, and **more jobs than tram seats** --
        // which is the premise, and asserting it is what caught the first
        // version of this test. One mill has fewer jobs than the pool has
        // seats, so the pool could never be exceeded however badly the
        // bookkeeping was broken, and the sabotage sailed through.
        let mut jobs = 0u32;
        for i in 0..5 {
            let site = at(2_500.0 + f64::from(i) * 250.0, 1_120.0);
            if let Ok(mill) = w.place_built(BuildingKind::TextileMill, site) {
                jobs += w.buildings.get(mill).unwrap().def().workers;
            }
        }
        staff_up(&mut w, home_at, 400);
        assert!(
            jobs > tram_seats,
            "{jobs} jobs against {tram_seats} tram seats — the pool cannot be exceeded, \
             so a broken booking would look exactly like a working one"
        );

        let ways = crate::journey::Ways {
            roads: &w.roads,
            rails: &w.rails,
            tramway: &w.tramway,
            metro: &w.metro,
            water: &w.waterways,
            air: &w.airways,
        };
        let labour = crate::citizen::assign_labour(&mut w.population, &w.buildings, ways);
        assert!(
            labour.seats_used > 0,
            "nobody rode at all, so this proves nothing about which pool paid"
        );
        assert!(
            labour.seats_used <= tram_seats,
            "{} seats were spent against a tram pool of {tram_seats} — \
             the bus pool paid for tram journeys",
            labour.seats_used
        );
    }

    /// **A trolleybus burns no oil**, and that is the entire trade: a republic
    /// that strings wire runs its buses on its own generation instead of on
    /// fuel it may have to buy. What it costs is that the wire has to be there.
    #[test]
    fn a_trolleybus_runs_on_the_grid_and_a_bus_runs_on_oil() {
        let mut w = bare();
        let depot = w
            .place_built(BuildingKind::TrolleybusDepot, at(1_000.0, 1_000.0))
            .expect("open ground");
        w.buildings.get_mut(depot).unwrap().staff = BuildingKind::TrolleybusDepot.def().workers;

        // Unpowered, it carries nobody however well staffed and however much
        // fuel is standing in the yard.
        w.buildings
            .get_mut(depot)
            .unwrap()
            .stock
            .add(Resource::Fuel, Tonnes(50.0));
        w.buildings.get_mut(depot).unwrap().powered = false;
        assert_eq!(
            crate::transport::seats(&w.buildings),
            0,
            "a trolleybus depot ran on diesel standing in its yard"
        );

        w.buildings.get_mut(depot).unwrap().powered = true;
        assert!(
            crate::transport::seats(&w.buildings) > 0,
            "a powered, staffed trolleybus depot carried nobody"
        );
        // And it books no fuel, however many seats are used.
        assert!(
            crate::transport::fuel_burn(&w.buildings, 500)
                .iter()
                .all(|&(id, _)| id != depot),
            "the republic was billed for diesel a trolleybus did not burn"
        );
    }

    /// The two new ways are their own networks, and that is a balance rule
    /// rather than a modelling flourish: sharing one would let a republic lay
    /// tramway at a third of a railway's price and run freight trains down it.
    #[test]
    fn a_freight_train_cannot_ride_a_tramway() {
        let mut w = bare();
        let (a, b) = (at(500.0, 1_000.0), at(3_000.0, 1_000.0));
        lay(&mut w, crate::roadworks::Grade::Tramway, a, b);
        assert!(
            w.network(crate::journey::Medium::Tram).segment_count() > 0,
            "the fixture laid no tramway, so this proves nothing"
        );
        assert_eq!(
            w.network(crate::journey::Medium::Rail).segment_count(),
            0,
            "a tramway went into the railways"
        );

        let def = VehicleKind::Locomotive.def();
        let crossing = w.crossing();
        assert!(
            crate::journey::plan_for(
                def.medium,
                a,
                b,
                w.ways(),
                &crossing,
                def.on_road,
                def.cross_country,
                0.0
            )
            .is_none(),
            "a hundred-and-twenty-tonne train ran down a street tramway"
        );
    }

    /// A metro goes under a river rather than over one, and it is the only way
    /// that does. Everything else meets `NeedsABridge`.
    #[test]
    fn only_a_tunnel_crosses_water_without_a_bridge() {
        let mut w = bare();
        let mut ground = crate::terrain::Terrain::flat(Metres(4_000.0));
        for cy in 0..ground.cells() {
            for cx in 0..ground.cells() {
                let p = ground.cell_centre(cx, cy);
                if (1_900.0..2_100.0).contains(&p.x.0) {
                    ground.set_surface(p, crate::terrain::Surface::Water);
                }
            }
        }
        w.set_terrain(ground);
        let (a, b) = (at(1_000.0, 1_000.0), at(3_000.0, 1_000.0));

        for grade in [
            crate::roadworks::Grade::Gravel,
            crate::roadworks::Grade::Railway,
            crate::roadworks::Grade::Tramway,
        ] {
            assert_eq!(
                w.order_road(a, b, grade),
                Err(crate::roadworks::RoadError::NeedsABridge),
                "{} crossed a river for nothing",
                grade.def().name
            );
        }
        for grade in [
            crate::roadworks::Grade::Bridge,
            crate::roadworks::Grade::RailBridge,
            crate::roadworks::Grade::MetroTunnel,
        ] {
            assert!(
                w.order_road(a, b, grade).is_ok(),
                "{} could not span a river",
                grade.def().name
            );
        }
    }

    // ---- The services roster -----------------------------------------------

    /// **Every need has somebody who can meet it.** A component of contentment
    /// that no building in the table serves is a component the player is
    /// marked down for and can do nothing about — which is the opposite of the
    /// goal's first condition, where everything modelled is not only visible
    /// but controllable wherever it is a decision.
    #[test]
    fn every_need_can_be_met_by_something_the_republic_can_build() {
        for need in crate::building::Need::ALL {
            let servers: Vec<&'static str> = crate::building::BUILDINGS
                .iter()
                .filter(|d| d.serves.iter().any(|&(what, _)| what == need))
                .map(|d| d.name)
                .collect();
            assert!(
                !servers.is_empty(),
                "nothing in the republic serves {need:?}, so the people are \
                 marked down for something they cannot be given"
            );
            // And enough of them to reach full cover, or the need is a
            // permanent deduction wearing a service's clothes.
            let best: f64 = crate::building::BUILDINGS
                .iter()
                .flat_map(|d| d.serves.iter())
                .filter(|&&(what, _)| what == need)
                .map(|&(_, share)| share)
                .sum();
            assert!(
                best >= 1.0,
                "{need:?} tops out at {best:.2} with everything built — \
                 a republic can never fully meet it"
            );
        }
    }

    /// A share, not a flag. No single building is complete provision of
    /// anything, and the cover from several adds up.
    ///
    /// This is a **deliberate change** to a figure that was an artefact: the
    /// Polyclinic used to supply complete health cover because it was the only
    /// health building in the game.
    #[test]
    fn services_add_up_and_no_one_building_is_all_of_anything() {
        for def in crate::building::BUILDINGS {
            for &(need, share) in def.serves {
                assert!(
                    share > 0.0 && share < 1.0,
                    "{} supplies {share} of {need:?} on its own",
                    def.name
                );
            }
        }

        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 40);
        let clinic = place(&mut w, BuildingKind::Clinic, at(1_000.0, 1_200.0));
        w.buildings.get_mut(clinic).unwrap().staff = BuildingKind::Clinic.def().workers;
        let m = contentment(&w);
        apply(&mut w, &m);
        let with_clinic = w.buildings.get(home).unwrap().content.health;

        let pharmacy = place(&mut w, BuildingKind::Pharmacy, at(1_180.0, 1_200.0));
        w.buildings.get_mut(pharmacy).unwrap().staff = BuildingKind::Pharmacy.def().workers;
        let m = contentment(&w);
        apply(&mut w, &m);
        let with_both = w.buildings.get(home).unwrap().content.health;

        assert!(
            with_both > with_clinic,
            "a pharmacy beside a clinic added nothing: {with_clinic} then {with_both}"
        );
        assert!(with_both <= 1.0, "cover ran past complete");
    }

    /// An unstaffed service serves nobody. A hospital with no doctors is a
    /// building, and the whole reason staffing is a fraction is so that
    /// questions like this have an answer rather than a yes.
    #[test]
    fn a_service_nobody_works_at_covers_nobody() {
        let mut w = bare();
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 40);
        let station = place(&mut w, BuildingKind::FireStation, at(1_000.0, 1_200.0));

        let m = contentment(&w);
        apply(&mut w, &m);
        assert_eq!(
            w.buildings.get(home).unwrap().content.safety,
            0.0,
            "an empty fire station made the estate feel safe"
        );

        w.buildings.get_mut(station).unwrap().staff = BuildingKind::FireStation.def().workers;
        let m = contentment(&w);
        apply(&mut w, &m);
        assert!(
            w.buildings.get(home).unwrap().content.safety > 0.0,
            "a staffed fire station covered nobody"
        );
    }

    /// Out of reach is out of mind. A hospital on the far side of the republic
    /// is not this estate's hospital.
    #[test]
    fn a_service_out_of_reach_covers_nobody() {
        let mut w = bare();
        let home = staff_up(&mut w, at(600.0, 600.0), 40);
        let far = at(600.0 + SERVICE_RADIUS.0 + 400.0, 600.0);
        let hospital = place(&mut w, BuildingKind::Hospital, far);
        w.buildings.get_mut(hospital).unwrap().staff = BuildingKind::Hospital.def().workers;

        let m = contentment(&w);
        apply(&mut w, &m);
        assert_eq!(
            w.buildings.get(home).unwrap().content.health,
            0.0,
            "a hospital {} m away covered the estate",
            SERVICE_RADIUS.0 + 400.0
        );
    }

    /// **Safety is never waived for want of demand**, unlike warmth on a warm
    /// day or schooling in a block with no children. The point of a fire
    /// station is the day you need it, not the average day — so a republic
    /// without one is marked down on the first day and every day after.
    #[test]
    fn safety_is_not_waived_the_way_warmth_and_schooling_are() {
        let mut w = bare();
        // A block of adults in high summer: nothing is asking for heat and
        // there are no children to school, so both of those come back full.
        let home = staff_up(&mut w, at(1_000.0, 1_000.0), 20);
        let m = contentment(&w);
        apply(&mut w, &m);
        let content = w.buildings.get(home).unwrap().content;
        assert_eq!(
            content.schooling, 1.0,
            "a block with no children was marked down for schools"
        );
        assert_eq!(
            content.safety, 0.0,
            "a republic with no fire station, no militia and no court was not marked down"
        );
    }
}
