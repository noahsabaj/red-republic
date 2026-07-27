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
use crate::geology::DepositId;
use crate::resource::Resource;
use crate::resource::Stock;
use crate::time::TICK;
use crate::trade::{CUSTOMS_RANGE, CUSTOMS_THROUGHPUT_PER_DAY, Market, TradeAction};
use crate::transport;
use crate::units::{Metres, Seconds, Tonnes};
use crate::world::World;
use std::collections::BTreeMap;

/// Everything a system is allowed to change.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// Freight arriving. Again one kind: goods leave the supplier and reach the
    /// destination together, or tonnage is invented or destroyed.
    Deliver {
        from: BuildingId,
        to: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
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
    Deliver,
    Provision,
    Export,
    Import,
    Build,
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
            Mutation::Deliver { .. } => MutationKind::Deliver,
            Mutation::Provision { .. } => MutationKind::Provision,
            Mutation::Export { .. } => MutationKind::Export,
            Mutation::Import { .. } => MutationKind::Import,
            Mutation::Build { .. } => MutationKind::Build,
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
    ("construction", &[MutationKind::Build]),
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
    ("logistics", &[MutationKind::Deliver]),
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
        match *mutation {
            Mutation::Staff { building, count } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.staff = count;
                }
            }
            Mutation::Powered { building, on } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.powered = on;
                }
            }
            Mutation::Heated { building, on } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.heated = on;
                }
            }
            Mutation::Extract {
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
            Mutation::Consume {
                building,
                resource,
                tonnes,
            } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.stock.take(resource, tonnes);
                }
            }
            Mutation::Produce {
                building,
                resource,
                tonnes,
            } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    let room = b.storage_cap().saturating_sub(b.stock.get(resource));
                    b.stock.add(resource, tonnes.min(room));
                }
            }
            Mutation::Provision { building, fraction } => {
                if let Some(b) = world.buildings.get_mut(building) {
                    b.provisioned = fraction.clamp(0.0, 1.0);
                }
            }
            Mutation::Export {
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
            Mutation::Offer(contract) => {
                if world.contracts.get(contract.id).is_none() {
                    world.contracts.insert(contract);
                }
            }
            Mutation::CloseContract { contract, state } => {
                let day = world.clock.day_index();
                if let Some(c) = world.contracts.get_mut(contract) {
                    c.state = state;
                    c.closed_day = Some(day);
                }
            }
            Mutation::DropContract { contract } => {
                world.contracts.remove(contract);
            }
            Mutation::Relations { market, penalty } => {
                world.contracts.set_penalty(market, penalty);
            }
            Mutation::Fine { market, amount } => {
                world.treasury.debit(market, amount);
            }
            Mutation::Import {
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
            Mutation::Build { site, builder_days } => {
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
            Mutation::Deliver {
                from,
                to,
                resource,
                tonnes,
            } => {
                let taken = world
                    .buildings
                    .get_mut(from)
                    .map(|b| b.stock.take(resource, tonnes))
                    .unwrap_or(Tonnes::ZERO);
                if let Some(b) = world.buildings.get_mut(to) {
                    let room = b
                        .intake_capacity(resource)
                        .saturating_sub(b.stock.get(resource));
                    let landed = taken.min(room);
                    b.stock.add(resource, landed);
                    // Whatever would not fit goes back where it came from
                    // rather than evaporating — freight is conserved.
                    let rejected = taken.saturating_sub(landed);
                    if rejected.is_positive()
                        && let Some(source) = world.buildings.get_mut(from)
                    {
                        source.stock.add(resource, rejected);
                    }
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

    let mut sites: Vec<_> = world
        .buildings
        .all()
        .iter()
        .filter(|b| !b.is_built() && b.has_materials())
        .collect();
    sites.sort_by_key(|b| b.id);

    let mut out = Vec::new();
    for site in sites {
        if crew <= 0.0 {
            break;
        }
        let wanted = (site.def().labour - site.work_done).max(0.0);
        let days = crew.min(BUILDERS_PER_SITE * day).min(wanted);
        if days <= 0.0 {
            continue;
        }
        crew -= days;
        out.push(Mutation::Build {
            site: site.id,
            builder_days: days,
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
    let extent = world.terrain.extent();
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
        .filter(|b| world.border.distance_from(b.centre, extent).0 <= CUSTOMS_RANGE.0)
        .collect();
    houses.sort_by_key(|b| b.id);

    for house in houses {
        let mut clearance = Tonnes(CUSTOMS_THROUGHPUT_PER_DAY * house.staffing() * day);
        if !clearance.is_positive() {
            continue;
        }

        for rule in &world.trade_policy.rules {
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
pub fn production(world: &World) -> Vec<Mutation> {
    let day = tick_days();
    let mut out = Vec::new();

    for b in world.buildings.all() {
        if !b.is_built() {
            continue; // a site produces nothing
        }
        let def = b.def();
        // Boilers and bus depots burn their fuel elsewhere — the heating system
        // and the labour pass respectively — because both throttle to demand.
        // Letting production burn it too would double-charge them, and burning
        // it here at a flat rate would mean a boiler consumed a January's coal
        // in July.
        if def.heat_output > 0.0 || def.seats > 0 {
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

/// How much freight the republic can move in a day, in tonnes.
///
/// A placeholder standing in for a physical fleet. The archived build's lorries
/// were real machines with fuel, positions and jobs, and that is the right
/// model — this is a scalar so the *ranking* can be built and tested first,
/// since the ranking is the part that was hard-won.
pub const FREIGHT_TONNES_PER_DAY: f64 = 240.0;

/// Move goods to where their absence would cost the most.
///
/// Ported rules, not ported code. The ranking is the archived build's:
///
/// - urgency is **downtime averted**, not emptiness. A bin that was never going
///   to run dry averts nothing and scores nothing however empty it looks.
/// - drain is [`cover_days`], which measures intent rather than flow.
/// - two passes, and the second is the safety valve: everything that prevents
///   no downtime — topping up, stocking a shop nobody uses yet — runs on
///   whatever capacity the first pass left. Scarcity is the only regime in
///   which ranking bites at all.
pub fn logistics(world: &World) -> Vec<Mutation> {
    let mut budget = Tonnes(FREIGHT_TONNES_PER_DAY * tick_days());
    if !budget.is_positive() {
        return Vec::new();
    }
    let mut out = Vec::new();

    // Pass one: needs that prevent real downtime, worst cover first.
    let mut urgent: Vec<(f64, BuildingId, Resource)> = Vec::new();
    for b in world.buildings.all() {
        for &(resource, _) in b.def().inputs {
            if let Some(days) = cover_days(world, b.id, resource)
                && days < RESUPPLY_AT_DAYS
            {
                urgent.push((days, b.id, resource));
            }
        }
    }
    urgent.sort_by(|(da, ia, ra), (db, ib, rb)| {
        da.total_cmp(db)
            .then_with(|| ia.cmp(ib))
            .then_with(|| ra.cmp(rb))
    });

    for (_, destination, resource) in urgent {
        if !budget.is_positive() {
            break;
        }
        serve(world, destination, resource, &mut budget, &mut out);
    }

    // Shops, ranked emptiest first. This sits in the urgent pass because a
    // shop running dry is a republic that stops eating, which outranks any
    // factory stalling.
    let mut shops: Vec<(f64, BuildingId, Resource)> = Vec::new();
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
                shops.push((fill, b.id, resource));
            }
        }
    }
    shops.sort_by(|(fa, ia, ra), (fb, ib, rb)| {
        fa.total_cmp(fb)
            .then_with(|| ia.cmp(ib))
            .then_with(|| ra.cmp(rb))
    });
    for (_, destination, resource) in shops {
        if !budget.is_positive() {
            break;
        }
        serve(world, destination, resource, &mut budget, &mut out);
    }

    // Pass one and a half: sites waiting on materials. A site with nothing
    // arriving is a crew standing idle, so this sits above comfortable
    // top-ups but below a running building about to stall.
    let mut sites: Vec<(BuildingId, Resource)> = Vec::new();
    for b in world.buildings.all() {
        if b.is_built() {
            continue;
        }
        for &(resource, quantity) in b.def().materials {
            if b.stock.get(resource).0 < quantity {
                sites.push((b.id, resource));
            }
        }
    }
    sites.sort();
    for (destination, resource) in sites {
        if !budget.is_positive() {
            break;
        }
        serve(world, destination, resource, &mut budget, &mut out);
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
        let extent = world.terrain.extent();
        let mut houses: Vec<BuildingId> = world
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.kind == BuildingKind::Customs)
            .filter(|b| world.border.distance_from(b.centre, extent).0 <= CUSTOMS_RANGE.0)
            .map(|b| b.id)
            .collect();
        houses.sort();
        for house in houses {
            for &resource in &sells {
                if !budget.is_positive() {
                    break;
                }
                serve(world, house, resource, &mut budget, &mut out);
            }
        }
    }

    // Pass two: comfortable top-ups, on what the first pass left.
    let mut comfortable: Vec<(BuildingId, Resource)> = Vec::new();
    for b in world.buildings.all() {
        for &(resource, _) in b.def().inputs {
            if let Some(days) = cover_days(world, b.id, resource)
                && days >= RESUPPLY_AT_DAYS
            {
                comfortable.push((b.id, resource));
            }
        }
    }
    for (destination, resource) in comfortable {
        if !budget.is_positive() {
            break;
        }
        serve(world, destination, resource, &mut budget, &mut out);
    }

    out
}

/// Find the nearest building with a surplus and book a delivery.
fn serve(
    world: &World,
    destination: BuildingId,
    resource: Resource,
    budget: &mut Tonnes,
    out: &mut Vec<Mutation>,
) {
    let Some(to) = world.buildings.get(destination) else {
        return;
    };
    // A site's bill of materials can exceed its finished storage bin — a steel
    // mill needs 30 t of brick to build and holds 40 t of anything once open,
    // but a smaller building could easily need more than it will ever store.
    // Capping a site by its bin would stall it forever.
    let room = to
        .intake_capacity(resource)
        .saturating_sub(to.stock.get(resource));
    if !room.is_positive() {
        return;
    }

    // A supplier is anyone holding this who does not consume it.
    let mut suppliers: Vec<(f64, BuildingId, Tonnes)> = world
        .buildings
        .all()
        .iter()
        .filter(|b| b.id != destination)
        .filter(|b| b.stock.get(resource).is_positive())
        .filter(|b| !b.def().inputs.iter().any(|(r, _)| *r == resource))
        .filter(|b| !b.def().sells.contains(&resource))
        .map(|b| {
            (
                b.centre.distance_to(to.centre).0,
                b.id,
                b.stock.get(resource),
            )
        })
        .collect();
    suppliers.sort_by(|(da, ia, _), (db, ib, _)| da.total_cmp(db).then_with(|| ia.cmp(ib)));

    let Some(&(_, from, held)) = suppliers.first() else {
        return;
    };
    let tonnes = held.min(room).min(*budget);
    if !tonnes.is_positive() {
        return;
    }
    *budget = budget.saturating_sub(tonnes);
    out.push(Mutation::Deliver {
        from,
        to: destination,
        resource,
        tonnes,
    });
}

/// One step of the simulation.
///
/// The order below IS the simulation's definition. Labour first because
/// staffing decides everything downstream; power next because it gates
/// production; production; then freight, which moves what production made.
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
        for system in [labour, |w: &mut World| contracts(w)] {
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
        logistics,
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
        w.terrain = Terrain::flat(Metres(4_000.0));
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

        let moves = logistics(&w);
        match *moves.first().expect("something should move") {
            Mutation::Deliver { to, resource, .. } => {
                assert_eq!(to, starving, "the empty mill should be served first");
                assert_eq!(resource, Resource::Wood);
            }
            other => panic!("expected a delivery, got {other:?}"),
        }
    }

    /// Freight is conserved. Cargo that will not fit goes back where it came
    /// from rather than evaporating in transit.
    #[test]
    fn cargo_that_does_not_fit_is_returned_not_destroyed() {
        let mut w = bare();
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

        let total = |w: &World| {
            w.buildings.get(from).unwrap().stock.get(Resource::Wood)
                + w.buildings.get(to).unwrap().stock.get(Resource::Wood)
        };
        let before = total(&w);
        apply(
            &mut w,
            &[Mutation::Deliver {
                from,
                to,
                resource: Resource::Wood,
                tonnes: Tonnes(100.0),
            }],
        );
        assert!(
            (before.0 - total(&w).0).abs() < 1e-9,
            "freight was not conserved"
        );
        assert_eq!(
            w.buildings.get(to).unwrap().stock.get(Resource::Wood),
            Tonnes(40.0)
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
        // A construction office and the people to staff it.
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
            14,
            "only the office and depot offer work"
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
            20,
            "the finished post offers its six jobs"
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
        // bare() uses a 4 km map; put the house on whichever edge is foreign.
        let extent = w.terrain.extent();
        let on_border = match w.border {
            crate::trade::BorderEdge::North => at(2_000.0, 200.0),
            crate::trade::BorderEdge::South => at(2_000.0, extent.0 - 200.0),
            crate::trade::BorderEdge::West => at(200.0, 2_000.0),
            crate::trade::BorderEdge::East => at(extent.0 - 200.0, 2_000.0),
        };
        let customs = w
            .place_built(BuildingKind::Customs, on_border)
            .expect("on the border");
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
        let extent = w.terrain.extent();
        let on_border = match w.border {
            crate::trade::BorderEdge::North => at(2_000.0, 200.0),
            crate::trade::BorderEdge::South => at(2_000.0, extent.0 - 200.0),
            crate::trade::BorderEdge::West => at(200.0, 2_000.0),
            crate::trade::BorderEdge::East => at(extent.0 - 200.0, 2_000.0),
        };
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
        let extent = w.terrain.extent();
        let on_border = match w.border {
            crate::trade::BorderEdge::North => at(2_000.0, 200.0),
            crate::trade::BorderEdge::South => at(2_000.0, extent.0 - 200.0),
            crate::trade::BorderEdge::West => at(200.0, 2_000.0),
            crate::trade::BorderEdge::East => at(extent.0 - 200.0, 2_000.0),
        };
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
        let extent = w.terrain.extent();
        let on_border = match w.border {
            crate::trade::BorderEdge::North => at(2_000.0, 200.0),
            crate::trade::BorderEdge::South => at(2_000.0, extent.0 - 200.0),
            crate::trade::BorderEdge::West => at(200.0, 2_000.0),
            crate::trade::BorderEdge::East => at(extent.0 - 200.0, 2_000.0),
        };
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
        crate::scenario::found(&mut world, 120);
        world.trade_policy = crate::trade::TradePolicy::new()
            .sell(Resource::Coal, Market::East)
            .buy(Resource::Machinery, Market::West, Tonnes(4.0));
        world.treasury.credit(Market::West, 500.0);
        // Something under construction, so the construction system has work.
        let centre = world.buildings.all()[0].centre;
        let _ = world.place(
            BuildingKind::House,
            Point::new(centre.x, centre.y + Metres(700.0)),
        );

        // And a bus route to somewhere out of walking range, so the labour pass
        // actually spends seats and burns a depot's fuel. Without this the
        // republic is compact enough that nobody ever rides, and `labour`'s
        // declared Consume would look like a superset when it is not.
        let mut previous = world.roads.add_node(centre);
        for i in 1..=8 {
            let next = world.roads.add_node(Point::new(
                centre.x + Metres(f64::from(i) * 400.0),
                centre.y,
            ));
            world
                .roads
                .connect(previous, next, crate::road::default_road_speed());
            previous = next;
        }
        let far = Point::new(centre.x + Metres(3_200.0), centre.y);
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
                    ("logistics", logistics),
                ] {
                    let m = system(&world);
                    note(name, &m);
                    apply(&mut world, &m);
                }
                world.clock.advance();
            }
            // Keep the border stocked so imports as well as exports happen.
            if day % 30 == 0 {
                world.treasury.credit(Market::West, 200.0);
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
            w.terrain = Terrain::flat(Metres(12_000.0));

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
        w.terrain = Terrain::flat(Metres(12_000.0));

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

    /// A customs house on whatever edge is foreign.
    fn crossing(world: &mut World) -> BuildingId {
        let extent = world.terrain.extent();
        let inset = Metres(CUSTOMS_RANGE.0 / 2.0);
        let middle = extent / 2.0;
        let at = match world.border {
            crate::trade::BorderEdge::North => Point::new(middle, inset),
            crate::trade::BorderEdge::South => Point::new(middle, extent - inset),
            crate::trade::BorderEdge::West => Point::new(inset, middle),
            crate::trade::BorderEdge::East => Point::new(extent - inset, middle),
        };
        world
            .place_built(BuildingKind::Customs, at)
            .expect("border")
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
        let extent = w.terrain.extent();
        let inset = Metres(CUSTOMS_RANGE.0 / 2.0);
        // Two houses along the same edge.
        let sites: Vec<Point> = [0.35, 0.65]
            .iter()
            .map(|f| match w.border {
                crate::trade::BorderEdge::North => Point::new(extent * *f, inset),
                crate::trade::BorderEdge::South => Point::new(extent * *f, extent - inset),
                crate::trade::BorderEdge::West => Point::new(inset, extent * *f),
                crate::trade::BorderEdge::East => Point::new(extent - inset, extent * *f),
            })
            .collect();
        let houses: Vec<BuildingId> = sites
            .into_iter()
            .map(|p| w.place_built(BuildingKind::Customs, p).expect("border"))
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
