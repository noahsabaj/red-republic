//! The founding shelf, marshalled for the cards.
//!
//! Founding is **choosing your land**, in two beats: a shelf of six candidate
//! postings, then naming the one you took. This module is the first beat's
//! reads — the numbers on a card, and the picture beside them.
//!
//! # Nothing here judges a posting
//!
//! `CandidateStats::promise` is the simulation's own weighting and it is what
//! orders and colours the cards. This file does not add a second opinion: a
//! shell that scored land would be a second version of a balance decision, and
//! only one of the two would be tested. Every underlying figure goes on the card
//! so the player can disagree with the weighting — which is the point of showing
//! them rather than only the score.
//!
//! # The picture is drawn from terrain, not from a render
//!
//! A card's minimap is rasterised straight out of `Terrain`: surface kind for
//! hue, relief for shading. It is deliberately **not** a screenshot of the 3D
//! stage, because six off-screen viewports to draw six thumbnails is six times
//! the setup cost of the one stage the player is actually looking at, and a
//! thumbnail does not need a camera, a sun or a material.

use godot::prelude::*;
use red_republic_sim::founding::{Candidate, CandidateStats, Shelf};
use red_republic_sim::units::Point;
use red_republic_sim::{ClimateId, Metres, Surface, Terrain};

/// Floats per candidate in [`cards`].
///
/// A stride rather than a dictionary per card, for the same measured reason
/// every bulk read in this crate is packed. Six cards would not have justified
/// it on cost; it is here so the layout is documented in one place and the card
/// scene reads the same shape as every other table in the shell.
pub const CARD_STRIDE: usize = 13;

/// Every candidate's decisive facts, packed, in shelf order.
///
/// Per card: `[index, promise, buildable, water, forest, coal, iron, oil,
/// groundwater, coal_reach_m, crossings_east, crossings_west, coldest_c]`.
///
/// `coal_reach_m` is `-1.0` when the survey found no workable coal at all, which
/// is a real hand and not a missing number — a posting with no coal in reach is
/// one the player should be able to see and refuse.
pub fn cards(shelf: &Shelf) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for candidate in &shelf.candidates {
        push_card(&mut out, candidate);
    }
    out
}

fn push_card(out: &mut PackedFloat32Array, candidate: &Candidate) {
    let s: &CandidateStats = &candidate.stats;
    out.push(candidate.index as f32);
    out.push(s.promise() as f32);
    out.push(s.buildable as f32);
    out.push(s.water as f32);
    out.push(s.forest as f32);
    out.push(s.coal.0 as f32);
    out.push(s.iron.0 as f32);
    out.push(s.oil.0 as f32);
    out.push(s.groundwater.0 as f32);
    out.push(s.coal_reach.map_or(-1.0, |d| d.0 as f32));
    out.push(s.crossings_east as f32);
    out.push(s.crossings_west as f32);
    out.push(s.coldest_month_c as f32);
}

/// A candidate's land as an RGB8 raster, `size` by `size`, row-major.
///
/// Hue comes from the surface kind and brightness from a hillshade, so relief
/// reads on a thumbnail the way it does on a survey map. Both are needed: a
/// flat-tinted minimap of a 10 km map is four colours in blobs and tells the
/// player nothing about whether the ground is workable, and pure hillshade
/// cannot tell a lake from a valley floor.
///
/// The colours are chosen **here** rather than in the simulation, the same rule
/// `terrain.gdshader` answers to — the art direction must be changeable without
/// recompiling the simulation's renderer. They are close to the survey look's
/// terrain tones on purpose, so a card and the stage behind it agree.
pub fn minimap(terrain: &Terrain, size: u32) -> PackedByteArray {
    let size = size.clamp(16, 512);
    let mut out = PackedByteArray::new();
    out.resize((size as usize) * (size as usize) * 3);

    let extent = terrain.extent().0;
    let step = extent / f64::from(size);
    // Sampled at pixel centres. Sampling at corners puts the last row and
    // column outside the map, where `surface_at` returns None and the edge
    // renders as a one-pixel border of default grass.
    let half = step * 0.5;

    // Relief is normalised against the map's own range rather than a fixed
    // scale: a 65 m spread is the whole relief of a 6 km map, and shading it
    // against an absolute metre range would render every candidate flat.
    let (low, high) = relief_range(terrain, size, step, half);
    let span = (high - low).max(1.0);

    for py in 0..size {
        for px in 0..size {
            let p = Point::new(
                Metres(f64::from(px) * step + half),
                Metres(f64::from(py) * step + half),
            );
            let surface = terrain.surface_at(p).unwrap_or(Surface::Grass);
            let height = terrain.height_at(p).map_or(low, |h| h.0);
            let shade = 0.72 + 0.42 * ((height - low) / span);
            let (r, g, b) = tone(surface);
            let at = ((py as usize) * (size as usize) + px as usize) * 3;
            out[at] = byte(r * shade);
            out[at + 1] = byte(g * shade);
            out[at + 2] = byte(b * shade);
        }
    }
    out
}

/// The lowest and highest ground the thumbnail will sample.
///
/// Read at the thumbnail's own resolution rather than the terrain's: sampling a
/// million cells to shade forty thousand pixels is a hundred-fold cost for a
/// range that differs by centimetres, and the range has to match what is
/// actually drawn or the darkest pixel is not black.
fn relief_range(terrain: &Terrain, size: u32, step: f64, half: f64) -> (f64, f64) {
    let mut low = f64::MAX;
    let mut high = f64::MIN;
    for py in 0..size {
        for px in 0..size {
            let p = Point::new(
                Metres(f64::from(px) * step + half),
                Metres(f64::from(py) * step + half),
            );
            if let Some(h) = terrain.height_at(p) {
                low = low.min(h.0);
                high = high.max(h.0);
            }
        }
    }
    if low > high { (0.0, 1.0) } else { (low, high) }
}

/// Base hue per surface kind, before shading.
fn tone(surface: Surface) -> (f64, f64, f64) {
    match surface {
        Surface::Grass => (0.42, 0.48, 0.35),
        Surface::Forest => (0.24, 0.34, 0.26),
        Surface::Rock => (0.55, 0.55, 0.53),
        Surface::Water => (0.30, 0.44, 0.52),
    }
}

fn byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The climates on offer, in `ClimateId::ALL` order.
pub fn climate_names() -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for climate in ClimateId::ALL {
        out.push(&GString::from(climate.def().name));
    }
    out
}

/// One climate's authored year: twelve monthly mean temperatures, then twelve
/// monthly rainfall figures in millimetres per day.
///
/// What the posting briefing is made of. A climate's *name* tells a player
/// nothing about what a winter there costs, and the whole design decision behind
/// authoring twelve months rather than a sine curve was that a table can express
/// a late spring — which is invisible unless somebody shows the table.
pub fn climate_year(climate: ClimateId) -> PackedFloat32Array {
    let def = climate.def();
    let mut out = PackedFloat32Array::new();
    for mean in def.monthly_mean_c {
        out.push(mean as f32);
    }
    for rain in def.monthly_rain_mm {
        out.push(rain as f32);
    }
    out
}

/// The map sizes on offer, in `founding::SIZES` order.
pub fn size_names() -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for (name, _) in red_republic_sim::founding::SIZES {
        out.push(&GString::from(name));
    }
    out
}

/// Each size in metres, in the same order.
pub fn size_extents() -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for (_, extent) in red_republic_sim::founding::SIZES {
        out.push(extent.0 as f32);
    }
    out
}
