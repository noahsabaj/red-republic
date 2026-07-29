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
use red_republic_sim::{BuildingKind, Metres, Terrain, World};

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
    transforms_of(world, None)
}

/// Where the republic actually is, for a world that was loaded rather than
/// founded.
///
/// The founding hands back the site it chose, but a save does not carry it — and
/// it does not need to, because it was only ever an answer to "where should the
/// camera open". A republic being resumed has buildings to aim at, so this is the
/// mean of where they stand.
///
/// The map's centre is the wrong answer and is the bug this exists to avoid: a
/// posting is sited on the shallowest coal body, so it is routinely nowhere near
/// the middle, and framing the map means opening on empty ground. That was
/// caught once already by looking at a rendered frame.
pub fn centre_of(world: &World) -> Point {
    let buildings = world.buildings().all();
    if buildings.is_empty() {
        let half = world.terrain().extent().0 * 0.5;
        return Point::new(Metres(half), Metres(half));
    }
    let count = buildings.len() as f64;
    let (x, y) = buildings
        .iter()
        .fold((0.0, 0.0), |(x, y), b| (x + b.centre.x.0, y + b.centre.y.0));
    Point::new(Metres(x / count), Metres(y / count))
}

/// The same, for one kind only.
///
/// The kit gives each kind its own mesh, so each needs its own `MultiMesh`.
/// Reading them apart here rather than sorting in GDScript keeps the shell
/// handing over exactly one packed buffer per instance batch, which is the
/// shape the 316x measurement argued for.
pub fn building_transforms_of_kind(world: &World, kind: BuildingKind) -> PackedFloat32Array {
    transforms_of(world, Some(kind))
}

fn transforms_of(world: &World, only: Option<BuildingKind>) -> PackedFloat32Array {
    let buildings = world.buildings().all();
    let terrain = world.terrain();
    let mut out = PackedFloat32Array::new();
    out.resize(0);

    for b in buildings {
        if only.is_some_and(|k| k != b.kind) {
            continue;
        }
        let ground = ground_position(terrain, b.centre);
        // Height is not authored on a building, so it is derived from footprint
        // — a warehouse is long and low, a plant is tall. This is a rendering
        // decision and belongs here rather than in the simulation, which has no
        // opinion about how tall anything looks.
        // The mesh the kit assembles already stands on the ground at its real
        // metric size, so the instance transform is a position and nothing
        // else. Height is authored in `building_art.gd` on the Godot side,
        // because it changes nothing about how a republic runs and a field no
        // system reads has no business being simulation state.
        push_transform(&mut out, ground, 1.0, 1.0, 1.0);
    }
    out
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

/// Floats per road segment: two end points and the limit on it.
pub const ROAD_STRIDE: usize = 7;

/// Every road segment as `[ax, ay, az, bx, by, bz, speed_kph]`.
///
/// The speed limit travels with the segment because a grade is a decision
/// rather than a nicer word — a lorry does 25 km/h on a dirt track whatever it
/// is capable of — so a dirt track and tarmac have to be told apart on sight.
/// The renderer reads the limit rather than a grade enum, which keeps this
/// working when a grade is added.
pub fn road_segments(world: &World) -> PackedFloat32Array {
    let roads = world.network(red_republic_sim::journey::Medium::Road);
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
        out.push((segment.speed.as_mps() * 3.6) as f32);
    }
    out
}
