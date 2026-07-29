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
use crate::citizen::assign_labour;
use crate::climate;
use crate::contract::{self, Contract, ContractId, ContractState};
use crate::fleet::{Destination, Doing, Job, VehicleId, VehicleKind, VehicleState, crewed};
use crate::geology::DepositId;
use crate::journey::{self, Journey};
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
    Weather(crate::ground::Ground),
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
    Provision { building: BuildingId, fraction: f64 },
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
    Import {
        customs: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
        market: Market,
        cost: f64,
    },
    /// Builder-days worked on a site, and the materials that went into them.
    /// One kind: work and materials are the same transaction, and a site that
    /// advanced without consuming would be building itself out of nothing.
    Build { site: BuildingId, builder_days: f64 },
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
    /// A penalty for undelivered goods. Separate from [`Mutation::Export`]
    /// because no goods move: this is money leaving and nothing coming back.
    Fine { market: Market, amount: f64 },
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
    Bog,
    Free,
    Recover,
    Wear,
    Fade,
    Promote,
    Provision,
    Export,
    Import,
    Build,
    Lay,
    Weather,
    Offer,
    CloseContract,
    DropContract,
    Relations,
    Fine,
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
            Mutation::Wear { .. } => MutationKind::Wear,
            Mutation::Fade { .. } => MutationKind::Fade,
            Mutation::Promote { .. } => MutationKind::Promote,
            Mutation::Bog { .. } => MutationKind::Bog,
            Mutation::Free { .. } => MutationKind::Free,
            Mutation::Recover { .. } => MutationKind::Recover,
            Mutation::Provision { .. } => MutationKind::Provision,
            Mutation::Export { .. } => MutationKind::Export,
            Mutation::Import { .. } => MutationKind::Import,
            Mutation::Build { .. } => MutationKind::Build,
            Mutation::Lay { .. } => MutationKind::Lay,
            Mutation::Weather(_) => MutationKind::Weather,
            Mutation::Offer(_) => MutationKind::Offer,
            Mutation::CloseContract { .. } => MutationKind::CloseContract,
            Mutation::DropContract { .. } => MutationKind::DropContract,
            Mutation::Relations { .. } => MutationKind::Relations,
            Mutation::Fine { .. } => MutationKind::Fine,
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
    ("heating", &[MutationKind::Heated, MutationKind::Consume]),
    ("construction", &[MutationKind::Build, MutationKind::Lay]),
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
            MutationKind::Bog,
            MutationKind::Free,
            MutationKind::Recover,
            MutationKind::Wear,
        ],
    ),
    ("tracks", &[MutationKind::Fade, MutationKind::Promote]),
    ("labour", &[MutationKind::Staff, MutationKind::Consume]),
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
            &Mutation::Provision { building, fraction } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.provisioned = fraction.clamp(0.0, 1.0);
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
            &Mutation::Relations { market, penalty } => {
                world.contracts.set_penalty(market, penalty);
            }
            &Mutation::Fine { market, amount } => {
                world.treasury.debit(market, amount);
            }
            &Mutation::Import {
                customs,
                resource,
                tonnes,
                market,
                cost,
            } => {
                let spent = world.treasury.debit(market, cost);
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
            }
            &Mutation::Weather(ground) => {
                world.ground = ground;
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
                // And the last builder-day opens it.
                if world.roadworks.get(site).is_some_and(|r| r.is_finished())
                    && let Some(opened) = world.roadworks.remove(site)
                {
                    roadworks::open(&mut world.roads, &opened);
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
                if let Some(v) = world.fleet.get_mut(*vehicle) {
                    v.fuel = (v.fuel + drawn).min(v.def().tank);
                    v.job = Some(*job);
                    v.journey = Some(journey.clone());
                    v.state = VehicleState::Fetching;
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
            &Mutation::Park { vehicle, burn } => {
                let Some(v) = world.fleet.get(vehicle) else {
                    continue;
                };
                let home = v.home;
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
pub fn power(world: &World) -> Vec<Mutation> {
    let mut available = 0.0;
    for b in world.buildings.all() {
        if !b.is_built() {
            continue; // a half-built plant generates nothing
        }
        let def = b.def();
        if def.power_output > 0.0 && b.staffing() > 0.0 {
            let fuelled = def
                .inputs
                .iter()
                .all(|&(r, _)| b.stock.get(r).is_positive());
            if fuelled {
                available += def.power_output * b.staffing();
            }
        }
    }

    let mut out = Vec::new();
    let mut drawn = 0.0;
    let mut consumers: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().power_draw > 0.0)
        .collect();
    consumers.sort_by_key(|b| b.id);

    for b in consumers {
        let draw = b.def().power_draw;
        let on = drawn + draw <= available;
        if on {
            drawn += draw;
        }
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

    let mut demand: f64 = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built())
        .map(|b| b.def().heat * factor)
        .sum();

    let mut out = Vec::new();
    let mut produced = 0.0;
    let mut boilers: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.def().heat_output > 0.0)
        .collect();
    boilers.sort_by_key(|b| b.id);

    for boiler in boilers {
        let def = boiler.def();
        // A boiler house is a building like any other: no crew or no
        // electricity for its pumps and it makes nothing. Exempting it would
        // mean a blackout in January quietly left the heating on.
        if def.power_draw > 0.0 && !boiler.powered {
            continue;
        }
        let capacity = def.heat_output * boiler.staffing();
        if capacity <= 0.0 {
            continue;
        }
        // Throttle to what is still wanted. A boiler serving a mild day does
        // not burn a cold day's coal.
        let throttle = (demand / capacity).clamp(0.0, 1.0);
        if throttle <= 0.0 {
            continue;
        }
        // And burn only in proportion to what it actually manages to make.
        let mut fuel_factor: f64 = 1.0;
        for &(resource, rate) in def.inputs {
            let wanted = rate * day * boiler.staffing() * throttle;
            if wanted > 0.0 {
                fuel_factor =
                    fuel_factor.min((boiler.stock.get(resource).0 / wanted).clamp(0.0, 1.0));
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
                tonnes: Tonnes(rate * day * boiler.staffing() * running),
            });
        }
        let made = capacity * running;
        produced += made;
        demand = (demand - made).max(0.0);
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
        // The epsilon is load-bearing, and the trajectory runner is what found
        // it: a boiler throttled to demand produces *exactly* demand, but
        // `demand` was accumulated by summing and `budget` is spent by
        // subtracting, and those two orders do not round the same way. Without
        // slack the last block on the list came up a few ulps short and went
        // cold on mild days — 67% warm housing in November while a January at
        // -15 was fine, which is the wrong way round and is what gave it away.
        let on = budget >= need - 1e-9;
        if on {
            budget -= need;
        }
        out.push(Mutation::Heated { building: b.id, on });
    }
    out
}

/// The most builders one site can absorb in a day. Ported from the archive:
/// throwing the whole republic's crew at one foundation does not make it set
/// faster.
pub const BUILDERS_PER_SITE: f64 = 10.0;

/// Putting up what has been ordered.
///
/// Builders are the staff of Construction Offices — a republic with no office
/// builds nothing, however much material it has stockpiled. A site progresses
/// only when **all** its materials are on hand: a half-delivered site waits,
/// which is what makes freight priority matter during a build-out.
///
/// Sites are worked in commissioning order and finished one at a time rather
/// than progressed evenly. That is the archived build's rule and it was learned
/// the hard way — spreading crews across every site meant nothing opened, and
/// ranking by nearness-to-completion starved new sites permanently.
pub fn construction(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let mut crew = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.is_built() && b.kind == BuildingKind::ConstructionOffice)
        .map(|b| f64::from(b.staff))
        .sum::<f64>()
        * day;
    if crew <= 0.0 {
        return Vec::new();
    }

    // Buildings and roads, ranked together. A building's id *is* its place in
    // the commissioning order; a road site carries the count as it stood when
    // it was ordered, and ties go to the building because the building holding
    // that number was placed first. Ordering a road therefore takes its turn in
    // the queue like anything else, rather than jumping it or waiting behind
    // every factory in the republic.
    let mut sites: Vec<(u64, Destination, f64)> = Vec::new();
    for b in world.buildings.all() {
        if b.is_built() || !b.has_materials() {
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
    sites.sort_by(|(oa, da, _), (ob, db, _)| oa.cmp(ob).then_with(|| da.cmp(db)));

    let mut out = Vec::new();
    for (_, site, wanted) in sites {
        if crew <= 0.0 {
            break;
        }
        let days = crew.min(BUILDERS_PER_SITE * day).min(wanted);
        if days <= 0.0 {
            continue;
        }
        crew -= days;
        out.push(match site {
            Destination::Building(id) => Mutation::Build {
                site: id,
                builder_days: days,
            },
            Destination::RoadSite(id) => Mutation::Lay {
                site: id,
                builder_days: days,
            },
        });
    }
    out
}

/// What one citizen eats in a day. Ported from the archived balance.
pub const FOOD_PER_CITIZEN: f64 = 0.015;

/// And wears out in a day.
pub const CLOTHES_PER_CITIZEN: f64 = 0.004;

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
        for (resource, per_head) in [
            (Resource::Food, FOOD_PER_CITIZEN),
            (Resource::Clothes, CLOTHES_PER_CITIZEN),
        ] {
            let need = Tonnes(residents as f64 * per_head * day);
            wanted += need.0;
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
                    met += taken.0;
                    out.push(Mutation::Consume {
                        building: *shop,
                        resource,
                        tonnes: taken,
                    });
                }
            }
        }

        let fraction = if wanted > 0.0 {
            (met / wanted).clamp(0.0, 1.0)
        } else {
            1.0
        };
        out.push(Mutation::Provision {
            building: home.id,
            fraction,
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
        let mut clearance = Tonnes(CUSTOMS_THROUGHPUT_PER_DAY * house.staffing() * day);
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
                    });
                }
            }
        }
    }
    out
}

/// Where people work, and what carrying them there costs.
///
/// One pass, because they are one decision: a job is only a job if there is a
/// way to get to it, and the seats spent getting there are the fuel the depots
/// burn. Splitting them would let the republic staff a factory by bus and then
/// separately discover it had no fuel to run the bus.
pub fn labour(world: &mut World) -> Vec<Mutation> {
    let result = assign_labour(&mut world.population, &world.buildings, &world.roads);
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
        let mut efficiency = b.staffing();
        if def.power_draw > 0.0 && !b.powered {
            // Unpowered work stops. The archived build had a per-building
            // brownout fraction; that authored property is worth restoring,
            // but stalling is the honest default until it is authored.
            efficiency = 0.0;
        }
        if efficiency <= 0.0 {
            continue;
        }

        // A farm answers to the ground and the air, not to the calendar.
        if def.farms {
            efficiency *= growing_conditions(world);
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
    let mut idle = available(world, false);
    let mut tows = available(world, true);
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
fn available(world: &World, recovery: bool) -> Vec<VehicleId> {
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
            // A recovery vehicle does not haul and a lorry does not tow, so
            // the two pools never compete for the same driver-slot twice.
            if v.def().recovers != recovery {
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
        let leg = |a: Point, b: Point| {
            journey::plan(
                a,
                b,
                &world.roads,
                &crossing,
                def.on_road,
                def.cross_country,
                now,
            )
        };
        let outbound = leg(v.at, stuck_at);
        let round_trip = outbound.distance() + leg(stuck_at, yard).distance();
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
    // another lorry is already on its way to collect.
    let mut suppliers: Vec<(f64, BuildingId, Tonnes)> = world
        .buildings
        .all()
        .iter()
        .filter(|b| Destination::Building(b.id) != destination)
        .filter(|b| !b.def().inputs.iter().any(|(r, _)| *r == resource))
        .filter(|b| !b.def().sells.contains(&resource))
        .map(|b| {
            (
                b.centre.distance_to(to.at).0,
                b.id,
                b.stock
                    .get(resource)
                    .saturating_sub(booked.promised(b.id, resource)),
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
        let leg = |a: Point, b: Point| {
            journey::plan(
                a,
                b,
                &world.roads,
                &crossing,
                def.on_road,
                def.cross_country,
                now,
            )
        };
        let outbound = leg(v.at, load_at);
        let round_trip =
            outbound.distance() + leg(load_at, drop_at).distance() + leg(drop_at, yard).distance();
        let top_up = def
            .tank
            .saturating_sub(v.fuel)
            .min(booked.fuel_left(world, v.home));
        if (v.fuel + top_up).0 < v.fuel_for(round_trip).0 {
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
                let drag = crossing.drag_along(from, to);
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

        if !journey.on_last_leg() {
            let ahead = journey.leg + 1;
            let (from, to) = journey.leg_ends(ahead);
            let drag = crossing.drag_along(from, to);
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
        let onward = |target: Point| {
            journey::plan(
                arrived,
                target,
                &world.roads,
                &crossing,
                def.on_road,
                def.cross_country,
                depart,
            )
        };

        // Whatever it just drove over, it packed down a little.
        out.extend(wore(world, v, journey.leg));

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
                            journey: onward(yard),
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
                    let drag = crossing.drag_along(a, b);
                    let speed =
                        plan.speed_on(leg, stuck_def.on_road, stuck_def.cross_country, drag);
                    out.push(Mutation::Recover {
                        recovery: v.id,
                        casualty,
                        was,
                        casualty_leg: leg,
                        casualty_start: now,
                        casualty_end: now + journey::leg_ticks(a.distance_to(b), speed),
                        journey: onward(yard),
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
                    let plan = onward(next);
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
                None => {}
            },
            VehicleState::Delivering => {
                let Some((_, to, resource, _)) = v.job.and_then(Job::haul) else {
                    continue;
                };
                let room = world
                    .consignee(to, resource)
                    .map(|c| c.capacity.saturating_sub(c.held))
                    .unwrap_or(Tonnes::ZERO);
                let plan = onward(yard);
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
    ground.advance(temperature, rain);
    vec![Mutation::Weather(ground)]
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
            labour,
            |w: &mut World| contracts(w),
            |w: &mut World| commissioning(w),
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
        dispatch,
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
    use crate::road::RoadNetwork;
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
        w.roads = RoadNetwork::new();
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
            .buildings
            .place_built(
                BuildingKind::Apartment,
                beside,
                &world.terrain,
                &world.geology,
            )
            .expect("housing goes up");
        for _ in 0..count {
            world.population.spawn_citizen(home, 30);
        }
        home
    }

    /// Most tests here are about running an economy, not about building one,
    /// so they put finished buildings up. Construction has its own tests.
    fn place(world: &mut World, kind: BuildingKind, at: Point) -> BuildingId {
        world
            .buildings
            .place_built(kind, at, &world.terrain, &world.geology)
            .expect("open ground")
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
        staff_up(&mut w, at(1_200.0, 1_000.0), 20);
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_700.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(50.0));

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
            crate::road::default_road_speed(),
            crate::road::default_road_speed(),
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
        let mut w = bare();
        let plant = place(&mut w, BuildingKind::PowerPlant, at(1_000.0, 1_000.0));
        w.buildings
            .get_mut(plant)
            .unwrap()
            .stock
            .add(Resource::Coal, Tonnes(50.0));
        let factory = place(&mut w, BuildingKind::FoodFactory, at(1_400.0, 1_000.0));

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
            .find(|v| v.def().recovers)
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
                .connect(previous, next, crate::road::default_road_speed());
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
    #[test]
    fn the_crew_works_roads_and_buildings_in_the_order_they_were_ordered() {
        let worked = |road_first: bool| -> (f64, f64) {
            let mut w = bare();
            place(
                &mut w,
                BuildingKind::ConstructionOffice,
                at(1_000.0, 1_000.0),
            );
            staff_up(&mut w, at(1_000.0, 1_150.0), 20);

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
                        .connect(previous, next, crate::road::default_road_speed());
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
    #[test]
    fn only_authored_retail_sells_anything() {
        assert_eq!(
            BuildingKind::Store.def().sells,
            &[Resource::Food, Resource::Clothes]
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
        crate::scenario::found(&mut world, 240);
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
        let far = Point::new(centre.x + Metres(3_200.0), centre.y);
        world
            .order_road(centre, far, crate::roadworks::Grade::Dirt)
            .expect("the town centre and the far end are both buildable");
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

        let mut seen: BTreeMap<&'static str, std::collections::BTreeSet<MutationKind>> =
            BTreeMap::new();
        let mut note = |name: &'static str, mutations: &[Mutation]| {
            let entry = seen.entry(name).or_default();
            for m in mutations {
                entry.insert(m.kind());
            }
        };

        // A year, so a winter and several tender cycles pass.
        for day in 0..360u32 {
            for _ in 0..TICKS_PER_DAY {
                if world.clock.is_day_boundary() {
                    let m = labour(&mut world);
                    note("labour", &m);
                    apply(&mut world, &m);
                    let m = contracts(&world);
                    note("contracts", &m);
                    apply(&mut world, &m);
                    let m = commissioning(&world);
                    note("commissioning", &m);
                    apply(&mut world, &m);
                    let m = weather(&world);
                    note("weather", &m);
                    apply(&mut world, &m);
                    let m = tracks(&world);
                    note("tracks", &m);
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
                    ("dispatch", dispatch),
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
        seen
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
        use crate::road::default_road_speed;

        let build = |with_transport: bool| {
            let mut w = bare();
            w.set_terrain(Terrain::flat(Metres(12_000.0)));

            // A remote camp with a mine, and a city eight kilometres away with
            // work but nobody living in it.
            let far = at(9_000.0, 9_000.0);
            coal_body(&mut w, far, 10_000.0);
            let mine = place(&mut w, BuildingKind::CoalMine, far);
            let camp = staff_up(&mut w, at(9_300.0, 9_000.0), 40);
            let works = place(&mut w, BuildingKind::MachineWorks, at(1_500.0, 1_000.0));

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
        use crate::road::default_road_speed;
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
        place(&mut w, BuildingKind::MachineWorks, at(6_500.0, 1_100.0));
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

    // ---- Contracts ----

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
}
