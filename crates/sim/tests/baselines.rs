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

use red_republic_sim::building::BuildingKind;
use red_republic_sim::citizen::assign_labour;
use red_republic_sim::geology::Mineral;
use red_republic_sim::road::{RoadNetwork, default_road_speed};
use red_republic_sim::scenario;
use red_republic_sim::time::TICKS_PER_DAY;
use red_republic_sim::units::{Metres, Point};
use red_republic_sim::world::{World, WorldSpec};
use std::time::Instant;

fn at(x: f64, y: f64) -> Point {
    Point::new(Metres(x), Metres(y))
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
    });
    let bodies = world.geology.all().len();

    const QUERIES: u32 = 100_000;
    let start = Instant::now();
    let mut found = 0u32;
    for i in 0..QUERIES {
        let p = at(f64::from(i % 10_000), f64::from((i * 7) % 10_000));
        if world
            .geology
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
    let mut roads = RoadNetwork::new();
    for y in 0..SIDE {
        for x in 0..SIDE {
            roads.add_node(at(f64::from(x) * 300.0, f64::from(y) * 300.0));
        }
    }
    let id = |x: u32, y: u32| red_republic_sim::road::NodeId(y * SIDE + x);
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
        });
        let base = scenario::found(&mut world, citizens);
        let jobs = world.buildings.jobs();

        let start = Instant::now();
        const PASSES: u32 = 20;
        for _ in 0..PASSES {
            assign_labour(&mut world.population, &world.buildings, &world.roads);
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / f64::from(PASSES);
        println!(
            "[BASELINE labour] {citizens} citizens, {jobs} jobs, {} buildings: {each:.2} ms per assignment",
            world.buildings.all().len()
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
        });
        scenario::found(&mut world, citizens);

        let start = Instant::now();
        for _ in 0..TICKS_PER_DAY {
            world.tick();
        }
        let day = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "[BASELINE tick] {citizens} citizens, {} buildings: {day:.0} ms per simulated day ({:.3} ms per tick)",
            world.buildings.all().len(),
            day / TICKS_PER_DAY as f64
        );
        assert!(day < 60_000.0, "a simulated day took {day:.0} ms");
    }
}

/// Placement gets slower as the republic fills up, because every candidate is
/// tested against every standing building.
#[test]
fn placement_scaling() {
    let mut world = World::new(WorldSpec {
        seed: 7,
        extent: Metres(10_000.0),
    });
    let start = Instant::now();
    let mut placed = 0u32;
    for i in 0..600u32 {
        let p = at(
            500.0 + f64::from(i % 30) * 150.0,
            500.0 + f64::from(i / 30) * 150.0,
        );
        if world
            .buildings
            .place(BuildingKind::House, p, &world.terrain, &world.geology)
            .is_ok()
        {
            placed += 1;
        }
    }
    let each = start.elapsed().as_secs_f64() * 1e6 / f64::from(placed.max(1));
    println!("[BASELINE placement] {placed} houses placed: {each:.1} us per placement");
    assert!(each < 20_000.0, "a placement took {each:.1} us");
}
