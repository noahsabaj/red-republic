//! Turning simulation state into something Godot can take in bulk.
//!
//! # One rule, and it was measured rather than reasoned
//!
//! At 1,205 buildings:
//!
//! | shape | cost | share of a 16.7 ms frame |
//! |---|---|---|
//! | raw FFI call | 0.21 µs | ~0 |
//! | `Array<VarDictionary>`, one per entity | **8,640 µs** | **52%** |
//! | flat `PackedFloat32Array` | **27 µs** | 0.16% |
//!
//! 316× apart. The idiomatic Godot shape — a dictionary per entity — is
//! catastrophic at scale, and the packed array is free. So **nothing in this
//! module returns a collection of objects.** Bulk reads return a flat array
//! with a documented stride and the caller slices it.
//!
//! Note what the first row means: a raw call costs nothing, so a chatty
//! *small* interface is fine. The rule is about bulk, not about call count.
//!
//! # Axes
//!
//! The simulation is a metric plane with a heightmap over it: a `Point` is
//! `(x, y)` in metres and height is looked up. Godot is Y-up. So the mapping,
//! written once here and nowhere else:
//!
//! ```text
//! sim (x, y) + height h  ->  godot Vector3 { x, y: h, z: y }
//! ```

use godot::prelude::*;
use red_republic_sim::units::Point;
use red_republic_sim::{Metres, Terrain, World};

/// Floats per instance in a `MultiMesh` transform buffer (3 rows of 4).
///
/// Godot's own layout for `MultiMesh::set_buffer` with `TRANSFORM_3D` and no
/// colour or custom data. Matching it exactly is what makes the upload a
/// memcpy rather than a conversion.
pub const TRANSFORM_STRIDE: usize = 12;

/// Where a simulation point sits in the world, on the ground.
pub fn ground_position(terrain: &Terrain, at: Point) -> Vector3 {
    let h = terrain.height_at(at).unwrap_or(Metres(0.0)).0;
    Vector3::new(at.x.0 as f32, h as f32, at.y.0 as f32)
}

/// Write one axis-aligned transform into a `MultiMesh` buffer.
///
/// `sx`/`sy`/`sz` scale the unit mesh to the thing's real metric footprint,
/// which is the whole reason buildings do not all look like the same rounded
/// square: a `BuildingDef` authors real width and depth in metres.
fn push_transform(out: &mut PackedFloat32Array, at: Vector3, sx: f32, sy: f32, sz: f32) {
    // Row-major 3x4: [basis.x | origin.x], [basis.y | origin.y], ...
    out.push(sx);
    out.push(0.0);
    out.push(0.0);
    out.push(at.x);
    out.push(0.0);
    out.push(sy);
    out.push(0.0);
    out.push(at.y);
    out.push(0.0);
    out.push(0.0);
    out.push(sz);
    out.push(at.z);
}

/// Every building as a `MultiMesh` transform buffer.
///
/// Ready for `MultiMesh::set_buffer` with no further conversion. Instance `i`
/// occupies `[i * TRANSFORM_STRIDE .. (i + 1) * TRANSFORM_STRIDE]`, and the
/// order is the order [`red_republic_sim::Buildings::all`] returns — which is
/// commissioning order, so an instance index is stable for as long as nothing
/// is demolished.
///
/// The unit mesh is expected to be a 1 m cube standing on the ground, so the
/// Y scale is the building's height and the origin is lifted half of it.
pub fn building_transforms(world: &World) -> PackedFloat32Array {
    let buildings = world.buildings().all();
    let terrain = world.terrain();
    let mut out = PackedFloat32Array::new();
    out.resize(0);

    for b in buildings {
        let def = b.def();
        let ground = ground_position(terrain, b.centre);
        // Height is not authored on a building, so it is derived from footprint
        // — a warehouse is long and low, a plant is tall. This is a rendering
        // decision and belongs here rather than in the simulation, which has no
        // opinion about how tall anything looks.
        let height = storey_height(def.width.0, def.depth.0) as f32;
        push_transform(
            &mut out,
            Vector3::new(ground.x, ground.y + height * 0.5, ground.z),
            def.width.0 as f32,
            height,
            def.depth.0 as f32,
        );
    }
    out
}

/// How tall a building of this footprint stands, in metres.
///
/// Rendering only. Kept as one function so the whole skyline changes together
/// and no single building gets a magic number of its own.
fn storey_height(width: f64, depth: f64) -> f64 {
    let footprint = (width * depth).sqrt();
    // Small things are roughly cubic, large things flatten out: a 10 m hut is
    // about 6 m tall, a 60 m works about 16 m, which keeps a factory reading as
    // a factory rather than a tower.
    (footprint * 0.6).min(4.0 + footprint * 0.2).max(3.0)
}

/// Every vehicle's position at a fractional tick, as `[x, y, z]` triples.
///
/// `now` is an absolute tick with a fraction — `world.clock().ticks()` plus how
/// far into the current tick real time has carried. [`Journey::position_at`] is
/// a pure function of `(plan, time)`, so this interpolates smoothly at 60 fps
/// while the simulation advances only in whole ticks, and every game speed
/// draws the same world.
///
/// [`Journey::position_at`]: red_republic_sim::Journey::position_at
pub fn vehicle_positions(world: &World, now: f64) -> PackedFloat32Array {
    let terrain = world.terrain();
    let mut out = PackedFloat32Array::new();
    for v in world.fleet().all() {
        let at = match v.journey.as_ref() {
            Some(journey) => journey.position_at(now),
            None => v.at,
        };
        let p = ground_position(terrain, at);
        out.push(p.x);
        out.push(p.y);
        out.push(p.z);
    }
    out
}

/// Every road segment as `[ax, ay, az, bx, by, bz]` sextuples.
pub fn road_segments(world: &World) -> PackedFloat32Array {
    let roads = world.roads();
    let terrain = world.terrain();
    let mut out = PackedFloat32Array::new();
    for segment in roads.segments() {
        let Some((from, to)) = roads.segment_ends(segment) else {
            continue;
        };
        for p in [ground_position(terrain, from), ground_position(terrain, to)] {
            out.push(p.x);
            out.push(p.y);
            out.push(p.z);
        }
    }
    out
}
