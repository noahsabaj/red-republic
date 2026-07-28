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
