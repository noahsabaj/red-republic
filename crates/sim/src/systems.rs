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
use crate::time::TICK;
use crate::units::{Seconds, Tonnes};
use crate::world::World;

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
                    let room = b.storage_cap().saturating_sub(b.stock.get(resource));
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
        .filter(|b| b.def().power_draw > 0.0)
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
    let room = to.storage_cap().saturating_sub(to.stock.get(resource));
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

    let mutations = labour(world);
    apply(world, &mutations);
    all.extend(mutations);

    for system in [power, production, logistics] {
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
            .place(
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

    fn place(world: &mut World, kind: BuildingKind, at: Point) -> BuildingId {
        world
            .buildings
            .place(kind, at, &world.terrain, &world.geology)
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

        staff_up(&mut w, at(1_180.0, 1_000.0), 30);
        w.tick();
        assert!(
            w.buildings.get(factory).unwrap().powered,
            "a staffed, fuelled plant should carry a 4 MW load"
        );
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
