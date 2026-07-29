//! Recorded performance baselines.
//!
//! Measured, never estimated — the archived build's habit, and the reason its
//! performance conversations were about facts. Each test prints what it
//! measured and asserts only a ceiling loose enough that it fails on a real
//! regression rather than on a busy machine.
//!
//! Run them with output:
//!
//! ```text
//! cargo test --release --test baselines -- --nocapture
//! ```
//!
//! The numbers in the assertions are the ceilings, not the measurements. Read
//! the printed output for what it actually costs today.

use red_republic_sim::building::{BUILDINGS, BuildingKind};
use red_republic_sim::citizen::assign_labour;
use red_republic_sim::climate::ClimateId;
use red_republic_sim::founding::{SIZES, Shelf, ShelfFilter};
use red_republic_sim::geology::Mineral;
use red_republic_sim::network::{Network, default_road_speed};
use red_republic_sim::scenario;
use red_republic_sim::terrain::{
    DEFAULT_CELL_SIZE, DEFAULT_TERRAIN, TerrainPlan, generate_terrain,
};
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::units::{Metres, Point};
use red_republic_sim::world::{World, WorldSpec};
use std::time::Instant;

fn at(x: f64, y: f64) -> Point {
    Point::new(Metres(x), Metres(y))
}

/// The measurement the plan required before terrain resolution could be
/// chosen — "measure before choosing", and this is the measuring.
///
/// It sweeps rather than testing one value, because the *shape* of the response
/// is what says whether you have the wrong tuning or the wrong lever. Here the
/// shape is a clean quadratic in cells-per-side with no cliff anywhere, so
/// resolution is a straight trade of memory and generation time against how
/// finely a footprint can be resolved — which means the decision is made by the
/// correctness floor, not by performance.
///
/// The floor: buildability samples the cell lattice, so a building smaller than
/// one cell in an axis reads a single sample and stops being distinguishable
/// from any other small building. The smallest in the table is a Small House at
/// 12 x 10 m, so 10 m is the coarsest resolution that holds — and everything
/// coarser is rejected however cheap it is.
#[test]
fn cell_size_sweep() {
    const EXTENT: Metres = Metres(10_000.0);
    let (min_w, min_d) = BUILDINGS.iter().fold((f64::MAX, f64::MAX), |(w, d), def| {
        (w.min(def.width.0), d.min(def.depth.0))
    });

    println!("[BASELINE cells] smallest footprint in the table: {min_w} x {min_d} m");
    for metres in [5.0, 10.0, 20.0, 40.0] {
        let plan = TerrainPlan {
            cell_size: Metres(metres),
            ..DEFAULT_TERRAIN
        };
        let start = Instant::now();
        let terrain = generate_terrain(1961, EXTENT, &plan);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        let cells = u64::from(terrain.cells());
        let total = cells * cells;
        // A height (f32) and a surface (one byte) per cell.
        let bytes = total * 5;
        let resolves = min_w >= metres && min_d >= metres;
        println!(
            "[BASELINE cells] {metres:>4} m: {cells} per side, {total} cells, {:.1} MB, {elapsed:.0} ms gen, smallest footprint {:.1} x {:.1} cells{}",
            bytes as f64 / 1_048_576.0,
            min_w / metres,
            min_d / metres,
            if resolves { "" } else { "  <- BELOW THE FLOOR" }
        );
    }

    // What the sweep decided, pinned so a later change to the building table
    // that breaks it fails here rather than silently making small buildings
    // indistinguishable.
    assert!(
        min_w >= DEFAULT_CELL_SIZE.0 && min_d >= DEFAULT_CELL_SIZE.0,
        "the smallest building ({min_w} x {min_d} m) no longer covers a {} m cell",
        DEFAULT_CELL_SIZE.0
    );
}

/// Generating a 10 km republic — the cost the founding screen's candidate
/// shelf pays per candidate map, so it bounds how many can be offered.
#[test]
fn worldgen_cost() {
    let start = Instant::now();
    const RUNS: u32 = 5;
    for seed in 0..RUNS {
        let _ = World::new(WorldSpec {
            seed: u64::from(seed),
            extent: Metres(10_000.0),
            climate: ClimateId::Plains,
        });
    }
    let each = start.elapsed().as_secs_f64() * 1000.0 / f64::from(RUNS);
    println!("[BASELINE worldgen] 10 km republic: {each:.1} ms per map");
    assert!(each < 2_000.0, "worldgen took {each:.1} ms");
}

/// Geology queries — what the survey overlay and the candidate cards read.
#[test]
fn geology_query_cost() {
    let world = World::new(WorldSpec {
        seed: 1961,
        extent: Metres(10_000.0),
        climate: ClimateId::Plains,
    });
    let bodies = world.geology().all().len();

    const QUERIES: u32 = 100_000;
    let start = Instant::now();
    let mut found = 0u32;
    for i in 0..QUERIES {
        let p = at(f64::from(i % 10_000), f64::from((i * 7) % 10_000));
        if world
            .geology()
            .distance_to_nearest(p, Mineral::Coal)
            .is_some()
        {
            found += 1;
        }
    }
    let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(QUERIES);
    println!("[BASELINE geology] {bodies} bodies, nearest-coal query: {each:.2} us ({found} hits)");
    assert!(each < 50.0, "a geology query took {each:.2} us");
}

/// Routing on a road network the size a real republic would grow — the cost
/// freight pays per dispatch.
#[test]
fn routing_cost() {
    // A 20x20 grid of junctions 300 m apart: 400 nodes, 760 segments, about
    // what a 6 km town with a proper road grid looks like.
    const SIDE: u32 = 20;
    let mut roads = Network::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            roads.add_node(at(f64::from(x) * 300.0, f64::from(y) * 300.0));
        }
    }
    let id = |x: u32, y: u32| red_republic_sim::network::NodeId(y * SIDE + x);
    for y in 0..SIDE {
        for x in 0..SIDE {
            if x + 1 < SIDE {
                roads.connect(id(x, y), id(x + 1, y), default_road_speed());
            }
            if y + 1 < SIDE {
                roads.connect(id(x, y), id(x, y + 1), default_road_speed());
            }
        }
    }

    const QUERIES: u32 = 2_000;
    let start = Instant::now();
    let mut total = 0.0;
    for i in 0..QUERIES {
        let a = id(i % SIDE, (i / SIDE) % SIDE);
        let b = id((i * 3) % SIDE, (i * 5) % SIDE);
        if let Some(route) = roads.route(a, b) {
            total += route.distance.0;
        }
    }
    let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(QUERIES);
    println!(
        "[BASELINE routing] {} nodes, {} segments: {each:.1} us per route (mean {:.0} m)",
        roads.node_count(),
        roads.segment_count(),
        total / f64::from(QUERIES)
    );
    assert!(each < 5_000.0, "a route took {each:.1} us");
}

/// Labour assignment against a growing population — the O(citizens x jobs)
/// pass, and the first thing that will need attention at scale.
#[test]
fn labour_scaling() {
    for &citizens in &[500usize, 2_000, 8_000] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        let base = scenario::found(&mut world, citizens);
        let jobs = world.buildings().jobs();

        let start = Instant::now();
        const PASSES: u32 = 20;
        for _ in 0..PASSES {
            {
                // Timed on its own: the axis is the labour pass, not a tick.
                let (buildings, roads) = (
                    world.buildings().clone(),
                    world
                        .network(red_republic_sim::journey::Medium::Road)
                        .clone(),
                );
                assign_labour(
                    world.population_mut(),
                    &buildings,
                    red_republic_sim::journey::Ways::on_roads(&roads),
                );
            }
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / f64::from(PASSES);
        println!(
            "[BASELINE labour] {citizens} citizens, {jobs} jobs, {} buildings: {each:.2} ms per assignment",
            world.buildings().all().len()
        );
        assert!(!base.housing.is_empty());
        assert!(each < 500.0, "labour assignment took {each:.2} ms");
    }
}

/// A full simulated day of a founded republic — the number that decides how
/// fast the game can run.
#[test]
fn simulated_day_cost() {
    for &citizens in &[200usize, 1_000, 4_000] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(6_000.0),
            climate: ClimateId::Plains,
        });
        scenario::found(&mut world, citizens);

        let start = Instant::now();
        for _ in 0..TICKS_PER_DAY {
            world.tick();
        }
        let day = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[BASELINE tick] {citizens} citizens, {} buildings: {day:.0} ms per simulated day ({:.3} ms per tick)",
            world.buildings().all().len(),
            day / TICKS_PER_DAY as f64
        );
        assert!(day < 60_000.0, "a simulated day took {day:.0} ms");
    }
}

/// The founding shelf: six candidate maps, generated to be compared.
///
/// This is the one screen whose cost the player waits on, so it is worth a
/// number rather than a shrug. During the design interview I told noahs the
/// shelf was essentially free, reasoning from the archived build's tile
/// generator — at metric scale it is not free, and this is what it actually
/// costs.
#[test]
fn founding_shelf_cost() {
    for &(label, extent) in &SIZES {
        let start = Instant::now();
        let shelf = Shelf::derive(
            1961,
            ShelfFilter {
                extent,
                climate: ClimateId::Plains,
            },
        );
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        // Re-filtering regenerates, because a filter transforms the land. That
        // is the interaction the player actually performs, so it is the one
        // worth measuring — not the first paint.
        let start = Instant::now();
        let _ = shelf.refilter(ShelfFilter {
            extent,
            climate: ClimateId::Taiga,
        });
        let refilter = start.elapsed().as_secs_f64() * 1000.0;

        // What the shelf is holding, now that a candidate keeps the land it was
        // surveyed from rather than a spec to rebuild it from. Reported because
        // the alternative to keeping it was regenerating, that trade was decided
        // on this number, and six maps of the largest size is the worst case the
        // screen can be put in. Serialized length rather than a heap probe: it
        // is the actual byte count of the data retained, and it needs no
        // allocator shim to read.
        let held: usize = shelf
            .candidates
            .iter()
            .map(|c| postcard::to_stdvec(&c.world).map_or(0, |v| v.len()))
            .sum();

        println!(
            "[BASELINE shelf] {label} ({:.0} km): {} candidates in {elapsed:.0} ms, \
             refilter {refilter:.0} ms, holding {:.1} MB",
            extent.as_km(),
            shelf.candidates.len(),
            held as f64 / 1_048_576.0
        );
        assert!(elapsed < 10_000.0, "a shelf took {elapsed:.0} ms");
    }
}

/// A simulated day against a growing **building** count.
///
/// `simulated_day_cost` scales citizens and holds buildings at the fifteen a
/// founding puts down, which is a village. A real republic is hundreds of
/// buildings, and this is the axis nothing was watching — it was found by
/// accident while measuring something else entirely, which is the argument for
/// having a baseline per axis rather than per feature.
#[test]
fn building_scaling() {
    for &extra in &[0usize, 200, 600] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(10_000.0),
            climate: ClimateId::Plains,
        });
        scenario::found(&mut world, 400);
        let mut placed = 0;
        for i in 0..extra {
            let p = at(
                300.0 + (i % 60) as f64 * 90.0,
                300.0 + (i / 60) as f64 * 90.0,
            );
            if world.establish(BuildingKind::House, p).is_ok() {
                placed += 1;
            }
        }

        let start = Instant::now();
        for _ in 0..TICKS_PER_DAY {
            world.tick();
        }
        let day = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[BASELINE buildings] {} buildings ({placed} extra): {day:.0} ms per simulated day",
            world.buildings().all().len()
        );
        assert!(day < 120_000.0, "a simulated day took {day:.0} ms");
    }
}

/// The cost of a physical fleet, against how many vehicles there are.
///
/// Two numbers, because they answer different questions. The **leg scan** is
/// what every vehicle costs every tick whether or not it is doing anything —
/// the plan-based motion model's whole claim is that this is one float
/// comparison, so it should be small and close to linear. **Dispatch** is the
/// expensive half: it prices a round trip with the same planner that will drive
/// it, which is three route plans per candidate lorry, and it was written that
/// way deliberately rather than estimated. This is the number that says whether
/// that was affordable.
///
/// The axis matters because `Vec` storage and an O(vehicles) scan both assume
/// the fleet stays in the hundreds. If a republic ever runs thousands, the scan
/// wants to become a queue keyed by arrival time — and this is what would say
/// so.
#[test]
fn fleet_scaling() {
    for &garages in &[1usize, 8, 24] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(10_000.0),
            climate: ClimateId::Plains,
        });
        scenario::found(&mut world, 1_500);
        let centre = world.buildings().all()[0].centre;
        for i in 0..garages.saturating_sub(1) {
            let p = at(
                centre.x.0 + 300.0 + (i % 6) as f64 * 200.0,
                centre.y.0 + 600.0 + (i / 6) as f64 * 200.0,
            );
            let _ = world.establish(BuildingKind::MotorDepot, p);
        }
        // Three days to commission the vehicles, staff the depots and get the
        // fleet rolling, so what follows measures a working republic rather
        // than an empty yard.
        for _ in 0..TICKS_PER_DAY * 3 {
            world.tick();
        }

        let start = Instant::now();
        let mut busiest = 0;
        for _ in 0..TICKS_PER_DAY {
            world.tick();
            busiest = busiest.max(world.fleet().running());
        }
        let day = start.elapsed().as_secs_f64() * 1000.0;

        const PASSES: u32 = 500;
        let start = Instant::now();
        for _ in 0..PASSES {
            let _ = red_republic_sim::systems::fleet(&world);
        }
        let scan = start.elapsed().as_secs_f64() * 1e9 / f64::from(PASSES);

        let start = Instant::now();
        for _ in 0..PASSES {
            let _ = red_republic_sim::systems::dispatch(&world);
        }
        let dispatch = start.elapsed().as_secs_f64() * 1e6 / f64::from(PASSES);

        println!(
            "[BASELINE fleet] {} vehicles ({busiest} on the road at once), {} buildings: {day:.0} ms per simulated day · leg scan {scan:.0} ns/tick · dispatch {dispatch:.1} us/tick",
            world.fleet().len(),
            world.buildings().all().len(),
        );
        assert!(!world.fleet().is_empty(), "no vehicles were commissioned");
        assert!(day < 120_000.0, "a simulated day took {day:.0} ms");
    }
}

/// Routing across country over the traversal lattice.
///
/// **The assumption the plan named as most likely to be false.** Freight prices
/// a round trip with three route plans per candidate lorry, and each one may
/// run A* over ten thousand cells — so if this is dear, physical freight is
/// dear everywhere. Measured across the two cases that differ most: a straight
/// crossing of open ground, where the answer is a straight line, and a route
/// that has to find its way round water.
#[test]
fn cross_country_routing_cost() {
    use red_republic_sim::terrain::Surface;

    for &extent in &[6_000.0, 10_000.0] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(extent),
            climate: ClimateId::Plains,
        });
        // Soaked, so every cell has a real cost and the search cannot shortcut
        // on everything being equally free.
        world.set_ground(red_republic_sim::ground::Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 0.0,
            frost: 0.0,
        });
        let cells = world.lattice().cells();
        let crossing = world.crossing();

        const QUERIES: u32 = 400;
        let start = Instant::now();
        let mut found = 0;
        for i in 0..QUERIES {
            let a = at(
                200.0 + f64::from(i % 17) * 111.0,
                200.0 + f64::from(i % 23) * 97.0,
            );
            let b = at(extent - 400.0, extent - 400.0);
            if crossing.route(a, b).is_some() {
                found += 1;
            }
        }
        let open = start.elapsed().as_secs_f64() * 1e6 / f64::from(QUERIES);

        // The same map with a wall of water across it, so the straight line is
        // never available and every query is a real search.
        let mut walled = world.terrain().clone();
        let mut x = 0.0;
        while x < extent * 0.8 {
            let mut y = extent * 0.45;
            while y < extent * 0.55 {
                walled.set_surface(at(x, y), Surface::Water);
                y += 10.0;
            }
            x += 10.0;
        }
        world.replace_terrain(walled);
        let crossing = world.crossing();
        let start = Instant::now();
        for i in 0..QUERIES {
            let a = at(200.0 + f64::from(i % 17) * 111.0, 200.0);
            let _ = crossing.route(a, at(extent - 400.0, extent - 400.0));
        }
        let round = start.elapsed().as_secs_f64() * 1e6 / f64::from(QUERIES);

        println!(
            "[BASELINE ground] {extent:.0} m map, {cells}x{cells} lattice ({} cells): straight crossing {open:.0} us ({found}/{QUERIES} routable), round an obstacle {round:.0} us",
            cells * cells
        );
        assert!(round < 100_000.0, "an off-road route took {round:.0} us");
    }
}

/// The daily wear sweep: fading every cell, and finding the corridors that have
/// packed down far enough to go on the map.
///
/// Both are O(cells) over the whole lattice and both run once a day, so this is
/// the cost of emergent roads existing at all. Measured against a lattice that
/// is heavily worn, because an empty one finds nothing and proves nothing.
#[test]
fn wear_sweep_cost() {
    for &extent in &[6_000.0, 10_000.0] {
        let mut world = World::new(WorldSpec {
            seed: 1961,
            extent: Metres(extent),
            climate: ClimateId::Plains,
        });
        let cells = world.lattice().cells();
        // Wear in a long diagonal corridor plus a scattering, so the sweep has
        // real runs to find and real cells to walk past.
        let side = f64::from(cells);
        for i in 0..cells {
            let along = f64::from(i) * 100.0 + 50.0;
            if let Some(cell) = world.lattice().cell_of(at(along, along)) {
                world.lattice_mut().wear_in(cell, 1.0);
            }
            if let Some(cell) = world.lattice().cell_of(at(along, side * 100.0 - along)) {
                world.lattice_mut().wear_in(cell, 0.9);
            }
        }

        const PASSES: u32 = 200;
        let start = Instant::now();
        let mut found = 0;
        for _ in 0..PASSES {
            found = red_republic_sim::systems::tracks(&world).len();
        }
        let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(PASSES);
        println!(
            "[BASELINE wear] {cells}x{cells} lattice ({} cells): daily sweep {each:.0} us ({found} mutations)",
            cells * cells
        );
        assert!(each < 20_000.0, "the daily wear sweep took {each:.0} us");
    }
}

/// Placement gets slower as the republic fills up, because every candidate is
/// tested against every standing building.
#[test]
fn placement_scaling() {
    let mut world = World::new(WorldSpec {
        seed: 7,
        extent: Metres(10_000.0),
        climate: ClimateId::Plains,
    });
    let start = Instant::now();
    let mut placed = 0u32;
    for i in 0..600u32 {
        let p = at(
            500.0 + f64::from(i % 30) * 150.0,
            500.0 + f64::from(i / 30) * 150.0,
        );
        let (terrain, geology) = (world.terrain().clone(), world.geology().clone());
        if world
            .buildings_mut()
            .place(BuildingKind::House, p, &terrain, &geology)
            .is_ok()
        {
            placed += 1;
        }
    }
    let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(placed.max(1));
    println!("[BASELINE placement] {placed} houses placed: {each:.1} us per placement");
    assert!(each < 20_000.0, "a placement took {each:.1} us");
}
