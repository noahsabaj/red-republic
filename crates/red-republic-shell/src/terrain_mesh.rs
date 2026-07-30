//! The heightmap, as one mesh.
//!
//! # Why one mesh and not a chunked, LOD-ed, streamed one
//!
//! Because it was measured. A 10 km map at 10 m resolution is a million cells
//! and 1,996,002 triangles, and building plus uploading the whole thing took
//! **30 ms, once, at load**. Frame time afterwards was 0.37 ms p50 — 2.2% of a
//! 60 fps budget. Every height came through the public `height_at`, and a
//! million of those calls cost 3.2 ms, which is what settled the question of
//! whether `Terrain` needed a bulk accessor. It does not.
//!
//! So chunking and LOD are not needed at this size, and adding them now would
//! be architecture bought against a problem nobody has. What *is* real is the
//! first frame: 146 ms of pipeline compilation, which means a loading screen is
//! a genuine requirement rather than a nicety.
//!
//! # Resolution is the terrain's, not ours
//!
//! `Terrain::cell_size()` is carried on the map rather than read from a
//! constant, so a save always knows what it was written at. This walks whatever
//! it finds instead of assuming ten metres.

use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::prelude::*;
use red_republic_sim::units::Point;
use red_republic_sim::{Metres, Surface, Terrain};

/// Build the surface arrays for an `ArrayMesh`.
///
/// Returns the array-of-arrays Godot expects from
/// `ArrayMesh::add_surface_from_arrays`, with vertices, normals, UVs, colours
/// and indices filled in.
///
/// Colour carries the surface kind rather than a texture, so a terrain material
/// can read `COLOR` and decide what grass, forest, rock and water look like
/// without this crate holding an opinion about it. That keeps the art decision
/// in the material where it can be changed without recompiling Rust.
pub fn surface(terrain: &Terrain) -> VarArray {
    let cells = terrain.cells();
    let size = terrain.cell_size().0;
    let verts_per_side = cells + 1;

    let mut vertices = PackedVector3Array::new();
    let mut normals = PackedVector3Array::new();
    let mut uvs = PackedVector2Array::new();
    let mut colors = PackedColorArray::new();
    let mut indices = PackedInt32Array::new();

    let height_at = |vx: u32, vy: u32| -> f64 {
        // Vertices sit on cell corners; sample the cell that owns each corner,
        // clamped at the far edges so the skirt closes.
        let cx = vx.min(cells.saturating_sub(1));
        let cy = vy.min(cells.saturating_sub(1));
        let p = terrain.cell_centre(cx, cy);
        terrain.height_at(p).unwrap_or(Metres(0.0)).0
    };

    for vy in 0..verts_per_side {
        for vx in 0..verts_per_side {
            let x = f64::from(vx) * size;
            let z = f64::from(vy) * size;
            let h = height_at(vx, vy);
            vertices.push(Vector3::new(x as f32, h as f32, z as f32));

            // Central differences over the neighbouring corners. Cheap, and at
            // ten metres it is indistinguishable from anything cleverer.
            let hl = height_at(vx.saturating_sub(1), vy);
            let hr = height_at((vx + 1).min(verts_per_side - 1), vy);
            let hd = height_at(vx, vy.saturating_sub(1));
            let hu = height_at(vx, (vy + 1).min(verts_per_side - 1));
            let normal =
                Vector3::new((hl - hr) as f32, (2.0 * size) as f32, (hd - hu) as f32).normalized();
            normals.push(normal);

            uvs.push(Vector2::new(
                vx as f32 / verts_per_side as f32,
                vy as f32 / verts_per_side as f32,
            ));

            let cx = vx.min(cells.saturating_sub(1));
            let cy = vy.min(cells.saturating_sub(1));
            let p = terrain.cell_centre(cx, cy);
            colors.push(surface_colour(terrain.surface_at(p)));
        }
    }

    for y in 0..cells {
        for x in 0..cells {
            let i = (y * verts_per_side + x) as i32;
            let right = i + 1;
            let below = i + verts_per_side as i32;
            let both = below + 1;
            // Two triangles, wound so the face points UP.
            //
            // Godot treats counter-clockwise as front-facing and culls the
            // back. Wound the other way the whole map faces the underworld and
            // renders as nothing at all -- which is exactly what happened, and
            // the numbers could not see it: 361,201 vertices uploaded fine and
            // the frame was empty. Only a rendered frame said so.
            indices.push(i);
            indices.push(right);
            indices.push(below);
            indices.push(right);
            indices.push(both);
            indices.push(below);
        }
    }

    let mut arrays = VarArray::new();
    arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
    arrays.set(ArrayType::VERTEX.ord() as usize, &vertices.to_variant());
    arrays.set(ArrayType::NORMAL.ord() as usize, &normals.to_variant());
    arrays.set(ArrayType::TEX_UV.ord() as usize, &uvs.to_variant());
    arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
    arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());
    arrays
}

/// Which primitive the arrays above describe.
pub fn primitive() -> PrimitiveType {
    PrimitiveType::TRIANGLES
}

/// A flat identifier per surface kind, carried in vertex colour.
///
/// Deliberately **not** the final look. These are channel markers a material
/// reads, so the art direction lives in the shader rather than in Rust — which
/// is what lets the look be chosen from mockups without recompiling.
fn surface_colour(surface: Option<Surface>) -> Color {
    match surface {
        Some(Surface::Grass) | None => Color::from_rgba(1.0, 0.0, 0.0, 1.0),
        Some(Surface::Forest) => Color::from_rgba(0.0, 1.0, 0.0, 1.0),
        Some(Surface::Rock) => Color::from_rgba(0.0, 0.0, 1.0, 1.0),
        Some(Surface::Water) => Color::from_rgba(0.0, 0.0, 0.0, 1.0),
    }
}

/// Sample a point's height, for anything that needs to sit on the ground.
pub fn height_at(terrain: &Terrain, at: Point) -> f64 {
    terrain.height_at(at).unwrap_or(Metres(0.0)).0
}

/// Floats per `MultiMesh` instance: a 3x4 transform, then an RGBA colour.
const FLOATS_PER_TREE: usize = 16;

/// Where the trees of one species stand, as a `MultiMesh` instance buffer.
///
/// # Why this is here and not in GDScript
///
/// The first version walked the finished mesh's 361,201 vertices in GDScript,
/// which hung a render for eight minutes before anyone killed it. This does the
/// same work against the terrain directly and hands back a buffer laid out
/// exactly as Godot's `MultiMesh` wants it, so the shell side is one
/// `set_buffer` call and there is no per-instance work anywhere.
///
/// That is the marshalling rule the whole boundary is built on, applied to the
/// largest thing that crosses it: never a structure per entity, always one
/// packed array with a documented stride. Here the stride is
/// [`FLOATS_PER_TREE`] — twelve for the transform, four for the colour.
///
/// # Determinism
///
/// Placement is a pure function of the cell it is in, so a republic's woods are
/// in the same place every time it is loaded. This is presentation and nothing
/// in the simulation depends on it; it still must not *look* like it changes,
/// because a forest that reshuffles itself on every load is the loudest possible
/// tell that the world is not real.
///
/// # Colour
///
/// The colour written here is a neutral brightness lift — grey, never a hue.
/// What species of tree is what colour is an art decision and it stays on the
/// Godot side in the material, the same rule that keeps the terrain's tones in
/// the shader. This only says *how much* light this particular tree catches.
pub fn forest_buffer(
    terrain: &Terrain,
    species: u32,
    species_count: u32,
    spacing: Metres,
) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    if species_count == 0 {
        return out;
    }

    let cells = terrain.cells();
    let cell = terrain.cell_size().0;
    // How many cells apart to consider planting. A wood wants trees every
    // several metres, and the terrain is sampled every ten.
    let step = ((spacing.0 / cell).round() as u32).max(1);

    let mut cy = 0;
    while cy < cells {
        let mut cx = 0;
        while cx < cells {
            let centre = terrain.cell_centre(cx, cy);
            if matches!(terrain.surface_at(centre), Some(Surface::Forest)) {
                let h = splitmix(u64::from(cx) << 32 | u64::from(cy));
                // Which species stands here. Deciding it from the same hash for
                // every species means the three calls partition the same set of
                // sites rather than each planting its own overlapping forest.
                if (h % u64::from(species_count)) as u32 == species {
                    let jitter_x = unit(h >> 8) - 0.5;
                    let jitter_z = unit(h >> 20) - 0.5;
                    let x = centre.x.0 + jitter_x * 2.0 * spacing.0;
                    let z = centre.y.0 + jitter_z * 2.0 * spacing.0;
                    let ground = terrain
                        .height_at(Point {
                            x: Metres(x),
                            y: Metres(z),
                        })
                        .unwrap_or(Metres(0.0))
                        .0;

                    // Real stands have saplings in them, and a wood of identical
                    // trees is the giveaway.
                    let scale = 0.55 + unit(h >> 32) * 0.95;
                    let yaw = unit(h >> 44) * std::f64::consts::TAU;
                    let (sin, cos) = yaw.sin_cos();

                    // Godot's instance transform is a 3x4 matrix stored row by
                    // row: three basis columns then the origin, per row.
                    out.push((scale * cos) as f32);
                    out.push(0.0);
                    out.push((scale * sin) as f32);
                    out.push(x as f32);

                    out.push(0.0);
                    out.push(scale as f32);
                    out.push(0.0);
                    out.push(ground as f32);

                    out.push((-scale * sin) as f32);
                    out.push(0.0);
                    out.push((scale * cos) as f32);
                    out.push(z as f32);

                    let lift = 0.78 + unit(h >> 52) * 0.44;
                    out.push(lift as f32);
                    out.push(lift as f32);
                    out.push(lift as f32);
                    out.push(1.0);
                }
            }
            cx += step;
        }
        cy += step;
    }

    debug_assert_eq!(out.len() % FLOATS_PER_TREE, 0);
    out
}

/// SplitMix64. Cheap, well-distributed, and a pure function of its input, which
/// is the only property that matters here.
fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The low bits of a hash as a `0.0..1.0`.
fn unit(h: u64) -> f64 {
    f64::from((h & 0xFFFF) as u32) / 65535.0
}
