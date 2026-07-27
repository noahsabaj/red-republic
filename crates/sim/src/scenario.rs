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
use crate::terrain::CELL_SIZE;
use crate::units::{Metres, Point};
use crate::world::World;

/// What the founding managed to put down.
#[derive(Debug, Clone, PartialEq)]
pub struct StartingBase {
    pub housing: Vec<BuildingId>,
    pub mine: Option<BuildingId>,
    pub plant: Option<BuildingId>,
    pub woodcutter: Option<BuildingId>,
    pub sawmill: Option<BuildingId>,
    /// Where the town centre ended up.
    pub centre: Point,
}

/// Search outward from `near` for somewhere this kind of building will stand.
///
/// A square spiral on the terrain lattice: deterministic, and it finds the
/// nearest workable site rather than the first one a random search stumbles
/// into.
pub fn find_site(world: &World, kind: BuildingKind, near: Point, within: Metres) -> Option<Point> {
    let step = CELL_SIZE.0 * 2.0;
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

/// Found a town: housing, a mine on the nearest coal, a plant to feed it, and
/// the beginnings of a timber chain.
///
/// Everything is sited within walking distance of the housing, because a job
/// nobody can reach is not a job — see [`crate::citizen`].
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

    // Moscow's opening grant: enough coal to light the plant before the mine
    // produces anything.
    if let Some(plant) = base.plant
        && let Some(b) = world.buildings.get_mut(plant)
    {
        b.stock
            .add(crate::resource::Resource::Coal, crate::units::Tonnes(60.0));
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
    use crate::world::WorldSpec;

    fn founded(seed: u64) -> (World, StartingBase) {
        let mut w = World::new(WorldSpec {
            seed,
            extent: Metres(6_000.0),
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
