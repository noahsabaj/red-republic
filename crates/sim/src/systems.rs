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
use crate::geology::DepositId;
use crate::resource::Resource;
use crate::resource::Stock;
use crate::time::TICK;
use crate::trade::{CUSTOMS_RANGE, CUSTOMS_THROUGHPUT_PER_DAY, Market, TradeAction};
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
    Export {
        customs: BuildingId,
        resource: Resource,
        tonnes: Tonnes,
        market: Market,
        payment: f64,
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
}

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

    for home in homes {
        let residents = world.population.residents_of(home.id).len();
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
                    let sold = held.min(clearance);
                    if !sold.is_positive() {
                        continue;
                    }
                    clearance = clearance.saturating_sub(sold);
                    purse.credit(rule.market, sold.0 * rule.market.sell_price(rule.resource));
                    out.push(Mutation::Export {
                        customs: house.id,
                        resource: rule.resource,
                        tonnes: sold,
                        market: rule.market,
                        payment: sold.0 * rule.market.sell_price(rule.resource),
                    });
                }
                TradeAction::Buy { up_to } => {
                    let shortfall = up_to.saturating_sub(house.stock.get(rule.resource));
                    let wanted = shortfall.min(clearance);
                    if !wanted.is_positive() {
                        continue;
                    }
                    let unit = rule.market.buy_price(rule.resource);
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

/// Where people work.
pub fn labour(world: &mut World) -> Vec<Mutation> {
    let staffing = assign_labour(&mut world.population, &world.buildings, &world.roads);
    staffing
        .into_iter()
        .map(|(building, count)| Mutation::Staff { building, count })
        .collect()
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
    if world.clock.is_day_boundary() {
        let mutations = labour(world);
        apply(world, &mutations);
        all.extend(mutations);
    }

    for system in [
        power,
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
    if def.outputs.is_empty() && def.power_output <= 0.0 {
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
}
