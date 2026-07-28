//! Founding a republic that can actually run.
//!
//! The successor to the archived build's `campaign.ts`: a scripted opening
//! that the trajectory runner and the balance tests both use, so a change to
//! the balance is measured against the same republic every time.
//!
//! It is deliberately not clever. It sites a town on real generated ground and
//! reports what it managed to place — a seed whose coal is under a lake is a
//! legitimate outcome and the caller should see it, not have it papered over.

use crate::building::{BuildingId, BuildingKind};
use crate::citizen::MAX_WALK;
use crate::geology::Mineral;
use crate::resource::Resource;
use crate::trade::{BorderEdge, CUSTOMS_RANGE};
use crate::units::{Metres, Point, Tonnes};
use crate::world::World;

/// What the founding managed to put down.
#[derive(Debug, Clone, PartialEq)]
pub struct StartingBase {
    pub housing: Vec<BuildingId>,
    pub mine: Option<BuildingId>,
    pub plant: Option<BuildingId>,
    pub woodcutter: Option<BuildingId>,
    pub sawmill: Option<BuildingId>,
    pub store: Option<BuildingId>,
    pub construction_office: Option<BuildingId>,
    pub depot: Option<BuildingId>,
    /// The garage the republic's lorries live in. Without it nothing moves.
    pub motor_depot: Option<BuildingId>,
    pub farm: Option<BuildingId>,
    pub food_factory: Option<BuildingId>,
    pub textile_mill: Option<BuildingId>,
    pub boiler: Option<BuildingId>,
    /// `None` when the border could not take a crossing on this seed.
    pub customs: Option<BuildingId>,
    /// Where the town centre ended up.
    pub centre: Point,
}

/// Search outward from `near` for somewhere this kind of building will stand.
///
/// A square spiral on the terrain lattice: deterministic, and it finds the
/// nearest workable site rather than the first one a random search stumbles
/// into.
pub fn find_site(world: &World, kind: BuildingKind, near: Point, within: Metres) -> Option<Point> {
    // Two cells at whatever resolution this map was generated at, so the
    // search stays proportionate to the ground it is searching.
    let step = world.terrain.cell_size().0 * 2.0;
    let rings = (within.0 / step).ceil() as i64;
    for ring in 0..=rings {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                // Only the perimeter of this ring — the inside was searched
                // on an earlier pass.
                if ring > 0 && dx.abs() != ring && dy.abs() != ring {
                    continue;
                }
                let candidate = Point::new(
                    Metres(near.x.0 + dx as f64 * step),
                    Metres(near.y.0 + dy as f64 * step),
                );
                if world
                    .buildings
                    .can_place(kind, candidate, &world.terrain, &world.geology)
                    .is_ok()
                {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Put a customs house on the border, walking along it for somewhere it will
/// stand.
///
/// Returns `None` when the whole border is water or otherwise unbuildable. A
/// republic that cannot trade is a legitimate hand to be dealt, and hiding it
/// would make the founding screen's candidate cards a lie.
fn found_crossing(world: &mut World, near: Point) -> Option<BuildingId> {
    let extent = world.terrain.extent();
    // A little inside the border, so the whole footprint is on home soil.
    let inset = Metres(CUSTOMS_RANGE.0 / 2.0);
    // Read the border once: the closure below cannot hold a borrow of `world`
    // while the loop needs it mutably to place.
    let border = world.border;
    let along = move |t: f64| match border {
        BorderEdge::North => Point::new(Metres(t), inset),
        BorderEdge::South => Point::new(Metres(t), extent - inset),
        BorderEdge::West => Point::new(inset, Metres(t)),
        BorderEdge::East => Point::new(extent - inset, Metres(t)),
    };
    let anchor = match border {
        BorderEdge::North | BorderEdge::South => near.x.0,
        BorderEdge::West | BorderEdge::East => near.y.0,
    };

    // Search outward from the point on the border nearest the town, so the
    // crossing lands as close to home as the ground allows.
    let step = 100.0;
    let steps = (extent.0 / step) as i64;
    for i in 0..=steps {
        for direction in [1.0, -1.0] {
            let t = anchor + direction * f64::from(i as i32) * step;
            if t < 0.0 || t > extent.0 {
                continue;
            }
            let at = along(t);
            if let Ok(id) = world.place_built(BuildingKind::Customs, at) {
                return Some(id);
            }
        }
    }
    None
}

/// Found a town: housing, a mine on the nearest coal, a plant to feed it, and
/// the beginnings of a timber chain.
///
/// Everything is sited within walking distance of the housing, because a job
/// nobody can reach is not a job — see [`crate::citizen`].
///
/// **The order below is a priority order.** Labour fills workplaces in
/// commissioning order, so whatever is founded last is what goes unmanned when
/// the republic is short of people. `citizens` is worth checking against
/// [`crate::building::Buildings::jobs`] afterwards: a founding with fewer
/// settlers than jobs is a legitimate hand, but it will be the tail of this
/// list that stands idle.
pub fn found(world: &mut World, citizens: usize) -> StartingBase {
    // Site the town on the shallowest coal body, since that is what a republic
    // would actually do.
    let mut coal: Vec<_> = world
        .geology
        .all()
        .iter()
        .filter(|d| d.mineral == Mineral::Coal && !d.is_exhausted())
        .map(|d| (d.top.0, d.centre, d.id))
        .collect();
    coal.sort_by(|(ta, _, ia), (tb, _, ib)| ta.total_cmp(tb).then_with(|| ia.cmp(ib)));

    let centre = coal
        .first()
        .map(|&(_, c, _)| c)
        .unwrap_or_else(|| Point::new(world.terrain.extent() / 2.0, world.terrain.extent() / 2.0));

    let mut base = StartingBase {
        housing: Vec::new(),
        mine: None,
        plant: None,
        woodcutter: None,
        sawmill: None,
        store: None,
        construction_office: None,
        depot: None,
        motor_depot: None,
        farm: None,
        food_factory: None,
        textile_mill: None,
        boiler: None,
        customs: None,
        centre,
    };

    let put = |world: &mut World, kind: BuildingKind, near: Point, reach: Metres| {
        find_site(world, kind, near, reach).and_then(|at| {
            world
                .buildings
                .place_built(kind, at, &world.terrain, &world.geology)
                .ok()
        })
    };

    base.mine = put(world, BuildingKind::CoalMine, centre, Metres(400.0));
    // Housing close enough to walk to everything.
    for i in 0..3 {
        let offset = Metres(300.0 + f64::from(i) * 120.0);
        if let Some(id) = put(
            world,
            BuildingKind::Apartment,
            Point::new(centre.x + offset, centre.y),
            Metres(600.0),
        ) {
            base.housing.push(id);
        }
    }
    base.plant = put(
        world,
        BuildingKind::PowerPlant,
        Point::new(centre.x, centre.y + Metres(500.0)),
        Metres(800.0),
    );
    // The boiler house goes up with the power station, not after the shops.
    //
    // **The order things are founded in IS their staffing priority**, because
    // labour fills workplaces in commissioning order — so a boiler sited last
    // is the first thing to go unmanned when a republic is short of people, and
    // an under-populated founding then freezes while its textile mill runs. Heat
    // and power are the two life-support systems and they are placed together.
    base.boiler = put(
        world,
        BuildingKind::HeatingPlant,
        Point::new(centre.x + Metres(200.0), centre.y - Metres(450.0)),
        Metres(MAX_WALK.0 / 2.0),
    );
    // And haulage, for the same reason and in the same breath. Freight is
    // physical now: without a garage and its lorries, nothing reaches anything
    // and the republic starves beside its own full bins. That makes the motor
    // depot life-support, and life-support goes in before the shops.
    base.motor_depot = put(
        world,
        BuildingKind::MotorDepot,
        Point::new(centre.x - Metres(300.0), centre.y - Metres(500.0)),
        Metres(MAX_WALK.0 / 2.0),
    );
    base.woodcutter = put(
        world,
        BuildingKind::Woodcutter,
        Point::new(centre.x - Metres(600.0), centre.y),
        Metres(MAX_WALK.0 / 2.0),
    );
    base.sawmill = put(
        world,
        BuildingKind::Sawmill,
        Point::new(centre.x - Metres(400.0), centre.y),
        Metres(MAX_WALK.0 / 2.0),
    );
    // A shop the estates can walk to, and the office that builds everything
    // ordered after this.
    base.store = put(
        world,
        BuildingKind::Store,
        Point::new(centre.x + Metres(250.0), centre.y + Metres(150.0)),
        Metres(400.0),
    );
    base.construction_office = put(
        world,
        BuildingKind::ConstructionOffice,
        Point::new(centre.x, centre.y - Metres(300.0)),
        Metres(600.0),
    );
    // A depot to hold the grant, so freight has somewhere to draw from.
    base.depot = put(
        world,
        BuildingKind::Depot,
        Point::new(centre.x + Metres(150.0), centre.y - Metres(200.0)),
        Metres(600.0),
    );
    // The food chain. Without it the opening grant is eaten in two months and
    // the republic starves for ever after — which is exactly what the
    // trajectory runner showed before this was here, and the reason it exists.
    base.farm = put(
        world,
        BuildingKind::Farm,
        Point::new(centre.x, centre.y + Metres(900.0)),
        Metres(MAX_WALK.0 / 2.0),
    );
    base.food_factory = put(
        world,
        BuildingKind::FoodFactory,
        Point::new(centre.x + Metres(400.0), centre.y + Metres(400.0)),
        Metres(MAX_WALK.0 / 2.0),
    );
    // And clothes. Without it the republic sits at 79% provisioned for ever —
    // fed but never clothed — which the runner showed plainly enough that it
    // is worth naming here.
    base.textile_mill = put(
        world,
        BuildingKind::TextileMill,
        Point::new(centre.x - Metres(200.0), centre.y + Metres(400.0)),
        Metres(MAX_WALK.0 / 2.0),
    );

    // A crossing on whatever edge is foreign. Sited by walking the border, so
    // a seed whose border is under water simply has no crossing — which the
    // caller can see rather than have hidden.
    base.customs = found_crossing(world, centre);

    // Moscow's opening grant. Coal to light the plant before the mine
    // produces, and food and clothes so the first winter is not immediate
    // starvation. Everything domestic — no currency, per the border rule.
    if let Some(plant) = base.plant
        && let Some(b) = world.buildings.get_mut(plant)
    {
        b.stock.add(Resource::Coal, Tonnes(60.0));
    }
    if let Some(depot) = base.depot
        && let Some(b) = world.buildings.get_mut(depot)
    {
        b.stock.add(Resource::Food, Tonnes(120.0));
        b.stock.add(Resource::Clothes, Tonnes(40.0));
        b.stock.add(Resource::Planks, Tonnes(80.0));
        b.stock.add(Resource::Bricks, Tonnes(80.0));
        // Diesel, because a republic with no refinery yet still has to move
        // things. It is a grant and not an allowance: it runs out, and what
        // replaces it is an oil chain or a trade rule that buys fuel. That
        // deadline is the point of granting a finite amount rather than
        // waiving the cost.
        b.stock.add(Resource::Fuel, Tonnes(20.0));
    }

    // Settlers, spread evenly over what housing exists.
    if !base.housing.is_empty() {
        for i in 0..citizens {
            let home = base.housing[i % base.housing.len()];
            // Ages spread across working life so the republic is not a single
            // cohort that retires all at once.
            let age = 18 + (i % 40) as u32;
            world.population.spawn_citizen(home, age);
        }
    }

    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::climate::ClimateId;
    use crate::world::WorldSpec;

    fn founded(seed: u64) -> (World, StartingBase) {
        let mut w = World::new(WorldSpec {
            seed,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = found(&mut w, 60);
        (w, base)
    }

    #[test]
    fn founding_is_reproducible() {
        let (a, base_a) = founded(1961);
        let (b, base_b) = founded(1961);
        assert_eq!(base_a, base_b);
        assert_eq!(a, b);
    }

    #[test]
    fn a_founded_republic_has_people_with_homes_and_a_mine() {
        let (w, base) = founded(1961);
        assert!(!base.housing.is_empty(), "nowhere to live");
        assert!(base.mine.is_some(), "no mine on the coal it was sited for");
        assert_eq!(w.population.count(), 60);
        for citizen in w.population.records() {
            assert!(
                w.buildings.get(citizen.home.0).is_some(),
                "a citizen lives in a building that is not there"
            );
        }
    }

    /// The founding rule that matters: everything is within walking distance,
    /// or the town cannot staff itself.
    #[test]
    fn the_whole_town_is_within_walking_distance_of_its_housing() {
        let (w, base) = founded(1961);
        let home = w
            .buildings
            .get(base.housing[0])
            .expect("housing exists")
            .centre;
        for id in [base.mine, base.plant, base.sawmill].into_iter().flatten() {
            let site = w.buildings.get(id).expect("placed").centre;
            assert!(
                crate::citizen::is_reachable(home, site, &w.roads),
                "a founded building is out of walking range"
            );
        }
    }

    #[test]
    fn a_founded_republic_actually_runs() {
        let (mut w, base) = founded(1961);
        for _ in 0..crate::time::TICKS_PER_DAY * 10 {
            w.tick();
        }
        assert!(w.population.employed() > 0, "nobody found work in ten days");
        if let Some(mine) = base.mine {
            assert!(
                w.buildings.get(mine).unwrap().staff > 0,
                "the mine never got a crew"
            );
        }
    }

    /// The gap the trajectory runner found, closed and guarded.
    ///
    /// A founded republic was fed and clothed and sat at **0% warm housing**
    /// from its first October onward, every winter, for ever — because nobody
    /// had thought to give it a boiler house. The same class of gap as the
    /// missing farm and the missing textile mill, and found the same way:
    /// by reading a trajectory rather than by reasoning about the founding.
    #[test]
    fn a_founded_republic_survives_its_first_winter_warm() {
        let mut w = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        // Enough settlers to man the town it is given. A short-staffed founding
        // is a legitimate hand, but it is not what this test is about.
        let base = found(&mut w, 120);
        assert!(base.boiler.is_some(), "no boiler house was founded");

        // Straight to deep winter, then a fortnight of it.
        let to_january = 306 - w.clock.days_elapsed();
        w.clock.advance_by(to_january * crate::time::TICKS_PER_DAY);
        for _ in 0..crate::time::TICKS_PER_DAY * 14 {
            w.tick();
        }

        assert!(
            crate::climate::heating_required(w.temperature()),
            "January was not cold enough to be a test of anything"
        );
        let cold: Vec<_> = w
            .buildings
            .all()
            .iter()
            .filter(|b| b.is_built() && b.def().heat > 0.0 && !b.heated)
            .map(|b| b.def().name)
            .collect();
        assert!(cold.is_empty(), "these went cold in January: {cold:?}");
    }

    #[test]
    fn site_search_finds_ground_and_gives_up_honestly() {
        let (w, _) = founded(1961);
        let centre = Point::new(Metres(3_000.0), Metres(3_000.0));
        assert!(find_site(&w, BuildingKind::House, centre, Metres(2_000.0)).is_some());
        // Nowhere near the map, so nowhere to build.
        let off = Point::new(Metres(50_000.0), Metres(50_000.0));
        assert!(find_site(&w, BuildingKind::House, off, Metres(100.0)).is_none());
    }
}
