//! The panel-facing reads: what the UI asks the republic about itself.
//!
//! [`crate::marshal`] is geometry — where things are, so they can be drawn.
//! This is everything else: what is in a yard, why a building has stopped, what
//! the weather will do, how a crossing sits before a lorry is committed to it.
//!
//! # Nothing here computes
//!
//! Every number is read from a view the simulation already owns. That rule is
//! load-bearing rather than tidy: the archived build's UI never re-derived
//! simulation maths either, and the moment a panel starts working out its own
//! answer there are two versions of the balance and only one of them is tested.
//! If a panel needs a number that does not exist, the number gets added to the
//! simulation — not to this file.
//!
//! # Bulk goes packed
//!
//! Same measured rule as the geometry: a dictionary per entity cost 8,640 µs at
//! 1,205 buildings against 27 µs for a flat array. Anything that scales with
//! the size of the republic comes back as `PackedFloat32Array` with a stride
//! documented on the function. Single values come back as themselves, because a
//! raw call is 0.21 µs and a chatty small interface is free.

use godot::prelude::*;
use red_republic_sim::resource::Resource;
use red_republic_sim::units::Point;
use red_republic_sim::{Metres, World};

/// Floats per deposit in [`deposits`].
pub const DEPOSIT_STRIDE: usize = 8;

/// Every body of mineral the survey has found.
///
/// `[mineral, x, y, radius, top, remaining, initial, working_depth]` per body.
/// Read from `Geology::survey`, which is the engine-owned view the founding
/// screen already reads — so a card and an overlay cannot disagree about what
/// is under the ground.
///
/// Resources are invisible on the terrain by design. This is the overlay that
/// makes them legible, and without it the whole three-dimensional subsurface is
/// something the simulation knows and the player cannot.
pub fn deposits(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for reading in world.geology().survey() {
        out.push(mineral_index(reading.mineral) as f32);
        out.push(reading.centre.x.0 as f32);
        out.push(reading.centre.y.0 as f32);
        out.push(reading.radius.0 as f32);
        out.push(reading.top.0 as f32);
        out.push(reading.remaining.0 as f32);
        out.push(reading.initial.0 as f32);
        out.push(reading.working_depth.0 as f32);
    }
    out
}

fn mineral_index(m: red_republic_sim::Mineral) -> usize {
    red_republic_sim::Mineral::ALL
        .iter()
        .position(|&x| x == m)
        .unwrap_or(0)
}

/// How hard the going is over the whole traversal lattice, row-major.
///
/// One value per lattice cell, **`0.0` firm to `1.0` impassable** — a badness,
/// not a quality, and the direction is the whole trap. An earlier version of
/// this comment said the opposite, the overlay ramp was built from the comment
/// rather than the source, and a bone-dry July map came out painted entirely
/// red. `going_is_a_badness_and_not_a_quality` pins it where the meaning lives.
/// The lattice is
/// 100 m where the terrain is 10 m — ten thousand cells on a 10 km map against
/// the terrain's million — which is exactly what makes an overlay of it cheap
/// enough to rebuild whenever the ground changes.
///
/// The ground being state rather than calendar is one of the simulation's
/// sharper ideas and it was entirely invisible: the worst going of the year
/// arrives a few weeks into spring, on its own, and nobody could see it.
pub fn going_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let crossing = world.crossing();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    for y in 0..cells {
        for x in 0..cells {
            let index = (y * cells + x) as usize;
            out.push(crossing.going_in(index) as f32);
        }
    }
    out
}

/// How worn each lattice cell is, row-major, `0.0` untouched to `1.0` a made
/// track.
///
/// Traffic packs the ground it crosses and a corridor past the threshold is
/// promoted into the road network. Showing the wear is what turns that from a
/// road appearing out of nowhere into a road you watched form.
pub fn wear_field(world: &World) -> PackedFloat32Array {
    let lattice = world.lattice();
    let cells = lattice.cells();
    let mut out = PackedFloat32Array::new();
    for y in 0..cells {
        for x in 0..cells {
            out.push(lattice.wear_at((y * cells + x) as usize) as f32);
        }
    }
    out
}

/// Floats per road site in [`road_sites`].
pub const SITE_STRIDE: usize = 6;

/// Roads ordered and not yet drivable.
///
/// `[ax, ay, bx, by, progress, speed_kph]`. Nothing routes over a site, so a
/// player looking at a half-built road needs to see that it is half-built
/// rather than wonder why no lorry will use it.
pub fn road_sites(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for site in world.roadworks().all() {
        out.push(site.from.x.0 as f32);
        out.push(site.from.y.0 as f32);
        out.push(site.to.x.0 as f32);
        out.push(site.to.y.0 as f32);
        out.push(site.progress() as f32);
        out.push((site.grade.def().speed.as_mps() * 3.6) as f32);
    }
    out
}

/// What every yard in the republic is holding, by resource.
///
/// One total per `Resource::ALL`, so the stockpile table is a single read of
/// thirteen floats rather than a walk over every building from GDScript.
pub fn stockpiles(world: &World) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for resource in Resource::ALL {
        let held: f64 = world
            .buildings()
            .all()
            .iter()
            .map(|b| b.stock.get(resource).0)
            .sum();
        out.push(held as f32);
    }
    out
}

/// The weather ahead: `[temperature_c, rain_mm]` per day, starting today.
///
/// Temperature is a pure function of `(seed, climate, day)` drawn from its own
/// substream, so asking about a future day perturbs nothing and costs nothing.
/// Heating demand follows **today's temperature and never the month**, which is
/// what makes a cold snap something a republic can be caught out by — and a
/// forecast is the only thing that makes being caught out feel like a mistake
/// rather than an ambush.
pub fn forecast(world: &World, days: u64) -> PackedFloat32Array {
    let today = world.clock().day_index();
    let mut out = PackedFloat32Array::new();
    for offset in 0..days {
        let (temperature, rain) = world.weather_on_day(today + offset);
        out.push(temperature as f32);
        out.push(rain as f32);
    }
    out
}

/// Going at one point, for a placement or a route the player is considering.
pub fn going_at(world: &World, x: f64, y: f64) -> f64 {
    world.going_at(Point::new(Metres(x), Metres(y)))
}

/// How far a point is from foreign soil.
pub fn distance_to_border(world: &World, x: f64, y: f64) -> f64 {
    world.distance_to_border(Point::new(Metres(x), Metres(y))).0
}
