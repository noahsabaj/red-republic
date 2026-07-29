//! The ground: elevation and surface material.
//!
//! # A grid that describes, not a grid things are made of
//!
//! Terrain is sampled on a regular lattice because elevation and ground cover
//! are fields, and a field wants a lattice. **Nothing else in the simulation
//! is made of cells.** Buildings sit at continuous positions with real metric
//! footprints, roads are a graph, and citizens walk in metres. If a cell index
//! ever escapes this module into a position, that is the bug.
//!
//! # Cell size, chosen by measurement
//!
//! Resolution is a property of a [`Terrain`], not a constant, because the plan
//! required it to be **settled by measurement rather than assumption** — and a
//! constant cannot be swept. `cell_size_sweep` in `tests/baselines.rs` is that
//! measurement, and [`DEFAULT_CELL_SIZE`] is what it chose.
//!
//! What the sweep measured across 5 m … 40 m on a 10 km map:
//!
//! | cell | cells | memory | generation | smallest footprint |
//! |------|-------|--------|------------|--------------------|
//! | 5 m  | 4.0M  | 19.1 MB| 144 ms     | 2.4 x 2.0 cells |
//! | 10 m | 1.0M  | 4.8 MB | 36 ms      | 1.2 x 1.0 cells |
//! | 20 m | 250k  | 1.2 MB | 9 ms       | 0.6 x 0.5 cells |
//! | 40 m | 62k   | 0.3 MB | 2 ms       | 0.3 x 0.2 cells |
//!
//! The response is a clean quadratic with no cliff anywhere, so performance
//! does not pick a resolution — **the correctness floor does**. 10 m is the
//! coarsest size at which the smallest building in the table still covers a
//! cell in both axes. A Small House is 12 x 10 m, so at 20 m it is smaller than
//! one sample in both directions: its buildability check reads a single cell,
//! and a house and a shop become the same rounded square. That is the grid
//! artefact this build exists to remove, so it is a floor rather than a
//! preference, and `the_smallest_building_still_covers_a_cell` is what holds it
//! when someone authors a smaller building.
//!
//! The price of that floor is now known rather than guessed: 4.8 MB and 36 ms
//! per 10 km map. Both are affordable, and the 4x saving at 20 m buys nothing
//! the simulation can spend.
//!
//! # Cross-machine generation
//!
//! Terrain is generated with value noise built from integer hashing and
//! polynomial interpolation — `+ - * /` only. See [`crate::mapgen`] for why
//! nothing transcendental is permitted anywhere in worldgen.

use crate::units::{Metres, Point};
use serde::{Deserialize, Serialize};

/// The side length of one terrain sample cell, unless a map says otherwise.
///
/// Measured, not assumed — see the module docs for the sweep that chose it.
pub const DEFAULT_CELL_SIZE: Metres = Metres(10.0);

/// What is on the ground.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Surface {
    Grass,
    Forest,
    Rock,
    Water,
}

impl Surface {
    /// Whether a building may stand here. Water needs works that do not exist
    /// yet; everything else is a matter of clearing.
    pub fn is_buildable(self) -> bool {
        !matches!(self, Surface::Water)
    }

    /// Whether someone can walk across it.
    pub fn is_walkable(self) -> bool {
        !matches!(self, Surface::Water)
    }
}

/// The ground of one republic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Terrain {
    cells: u32,
    /// Metres across one sample cell. Carried on the map rather than read from
    /// a constant so resolution can be swept, and so a save always knows the
    /// resolution it was written at instead of inheriting whatever the build
    /// currently defaults to.
    cell_size: Metres,
    /// Metres above the datum, one per cell, row-major.
    height: Vec<f32>,
    surface: Vec<Surface>,
}

impl Terrain {
    /// A flat grass square `extent` metres on a side at the default
    /// resolution — the test fixture, and the base a generator builds on.
    pub fn flat(extent: Metres) -> Self {
        Self::flat_with(extent, DEFAULT_CELL_SIZE)
    }

    /// The same at a chosen resolution. What the sweep measures.
    pub fn flat_with(extent: Metres, cell_size: Metres) -> Self {
        assert!(cell_size.0 > 0.0, "a cell needs a positive size");
        let cells = Self::cells_for(extent, cell_size);
        let count = (cells as usize) * (cells as usize);
        Self {
            cells,
            cell_size,
            height: vec![0.0; count],
            surface: vec![Surface::Grass; count],
        }
    }

    fn cells_for(extent: Metres, cell_size: Metres) -> u32 {
        assert!(extent.0 > 0.0, "a republic needs a positive extent");
        (extent.0 / cell_size.0).ceil() as u32
    }

    /// Cells along one side.
    pub fn cells(&self) -> u32 {
        self.cells
    }

    /// How wide one sample cell is.
    pub fn cell_size(&self) -> Metres {
        self.cell_size
    }

    /// How far the map reaches, in metres.
    pub fn extent(&self) -> Metres {
        Metres(f64::from(self.cells) * self.cell_size.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        let e = self.extent().0;
        (0.0..e).contains(&p.x.0) && (0.0..e).contains(&p.y.0)
    }

    /// Index of the cell a point falls in, or `None` off the map.
    fn index_of(&self, p: Point) -> Option<usize> {
        if !self.contains(p) {
            return None;
        }
        let cx = (p.x.0 / self.cell_size.0) as u32;
        let cy = (p.y.0 / self.cell_size.0) as u32;
        Some((cy as usize) * (self.cells as usize) + (cx as usize))
    }

    /// The centre of a cell, in metres — the bridge from lattice back to the
    /// continuous world, and the only direction that conversion should ever go
    /// outside this module.
    pub fn cell_centre(&self, cx: u32, cy: u32) -> Point {
        Point::new(
            Metres((f64::from(cx) + 0.5) * self.cell_size.0),
            Metres((f64::from(cy) + 0.5) * self.cell_size.0),
        )
    }

    pub fn surface_at(&self, p: Point) -> Option<Surface> {
        self.index_of(p).map(|i| self.surface[i])
    }

    pub fn height_at(&self, p: Point) -> Option<Metres> {
        self.index_of(p).map(|i| Metres(f64::from(self.height[i])))
    }

    /// Whether a straight run between two points crosses open water.
    ///
    /// Sampled at half-cell steps so a river cannot be stepped over. What a
    /// road order asks before it decides whether it is looking at a road or at
    /// a bridge — and until this existed nothing asked at all, so a gravel road
    /// could be laid straight across a river at the price of gravel.
    pub fn crosses_water(&self, from: Point, to: Point) -> bool {
        let distance = from.distance_to(to).0;
        let steps = ((distance / (self.cell_size().0 * 0.5)).ceil() as u32).clamp(1, 512);
        (0..=steps).any(|step| {
            let t = f64::from(step) / f64::from(steps);
            let at = Point::new(
                Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                Metres(from.y.0 + (to.y.0 - from.y.0) * t),
            );
            self.surface_at(at) == Some(Surface::Water)
        })
    }

    pub fn set_surface(&mut self, p: Point, surface: Surface) {
        if let Some(i) = self.index_of(p) {
            self.surface[i] = surface;
        }
    }

    pub fn set_height(&mut self, p: Point, height: Metres) {
        if let Some(i) = self.index_of(p) {
            self.height[i] = height.0 as f32;
        }
    }

    /// Whether every cell a rectangle touches is buildable — the check a
    /// placement makes. Samples on the cell lattice rather than testing corner
    /// points, so a footprint cannot straddle a lake by landing its corners on
    /// dry ground.
    pub fn area_is_buildable(&self, centre: Point, width: Metres, depth: Metres) -> bool {
        self.every_cell_in(centre, width, depth, |s| s.is_buildable())
    }

    fn every_cell_in(
        &self,
        centre: Point,
        width: Metres,
        depth: Metres,
        predicate: impl Fn(Surface) -> bool,
    ) -> bool {
        let half_w = width.0 / 2.0;
        let half_d = depth.0 / 2.0;
        let min_x = centre.x.0 - half_w;
        let max_x = centre.x.0 + half_w;
        let min_y = centre.y.0 - half_d;
        let max_y = centre.y.0 + half_d;

        if min_x < 0.0 || min_y < 0.0 {
            return false;
        }
        let e = self.extent().0;
        if max_x > e || max_y > e {
            return false;
        }

        let cell_min_x = (min_x / self.cell_size.0) as u32;
        let cell_max_x = ((max_x / self.cell_size.0).ceil() as u32).min(self.cells);
        let cell_min_y = (min_y / self.cell_size.0) as u32;
        let cell_max_y = ((max_y / self.cell_size.0).ceil() as u32).min(self.cells);

        for cy in cell_min_y..cell_max_y {
            for cx in cell_min_x..cell_max_x {
                let i = (cy as usize) * (self.cells as usize) + (cx as usize);
                if !predicate(self.surface[i]) {
                    return false;
                }
            }
        }
        true
    }

    /// Fraction of the map covered by one surface — a candidate card wants to
    /// say how much of a posting is forest or water.
    pub fn fraction_of(&self, surface: Surface) -> f64 {
        if self.surface.is_empty() {
            return 0.0;
        }
        let hits = self.surface.iter().filter(|&&s| s == surface).count();
        hits as f64 / self.surface.len() as f64
    }
}

// ---- Deterministic value noise ----
//
// Integer hashing plus polynomial interpolation: every operation is exact on
// any IEEE-754 machine, which is what worldgen requires.

fn hash_cell(seed: u64, x: i64, y: i64) -> u64 {
    let mut h = seed;
    h ^= (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.rotate_left(29);
    h ^= (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = h.rotate_left(31);
    h ^= h >> 27;
    h.wrapping_mul(0x94D0_49BB_1331_11EB)
}

/// Lattice value in `[0, 1)`.
fn lattice(seed: u64, x: i64, y: i64) -> f64 {
    const SCALE: f64 = 1.0 / (1u64 << 53) as f64;
    (hash_cell(seed, x, y) >> 11) as f64 * SCALE
}

/// Smoothstep — a polynomial, so exact everywhere.
fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// One octave of value noise at a given feature size, in `[0, 1)`.
fn value_noise(seed: u64, p: Point, feature: Metres) -> f64 {
    let fx = p.x.0 / feature.0;
    let fy = p.y.0 / feature.0;
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = smooth(fx - x0);
    let ty = smooth(fy - y0);
    let (ix, iy) = (x0 as i64, y0 as i64);

    let n00 = lattice(seed, ix, iy);
    let n10 = lattice(seed, ix + 1, iy);
    let n01 = lattice(seed, ix, iy + 1);
    let n11 = lattice(seed, ix + 1, iy + 1);

    let top = n00 + (n10 - n00) * tx;
    let bottom = n01 + (n11 - n01) * tx;
    top + (bottom - top) * ty
}

/// Several octaves summed — big shapes with small detail on top.
fn fractal_noise(seed: u64, p: Point, feature: Metres, octaves: u32) -> f64 {
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut sum = 0.0;
    let mut size = feature.0;
    for octave in 0..octaves {
        total += value_noise(seed.wrapping_add(u64::from(octave)), p, Metres(size)) * amplitude;
        sum += amplitude;
        amplitude *= 0.5;
        size *= 0.5;
    }
    total / sum
}

/// How the ground is shaped. Placeholders, like the geology plan.
#[derive(Debug, Clone, PartialEq)]
pub struct TerrainPlan {
    /// How finely the ground is sampled. See the module docs for the sweep.
    pub cell_size: Metres,
    /// Size of the largest landform.
    pub feature_size: Metres,
    pub octaves: u32,
    /// Height range from lowest point to highest.
    pub relief: Metres,
    /// Noise below this becomes water; above `rock_above`, rock.
    pub water_below: f64,
    pub forest_above: f64,
    pub rock_above: f64,
    /// How much ground has to drain through a cell before it is a river,
    /// as a fraction of the whole map's area.
    ///
    /// A fraction rather than a cell count so river density is the same on a
    /// 6 km map and a 10 km one — the alternative is a constant that means
    /// "a stream" at one size and "nothing at all" at another.
    pub river_catchment: f64,
    /// The catchment at which a river is wide enough to spill into the cells
    /// beside it. Rivers are not all one width, and the wide ones are the ones
    /// worth putting a barge on.
    pub broad_catchment: f64,
}

pub const DEFAULT_TERRAIN: TerrainPlan = TerrainPlan {
    cell_size: DEFAULT_CELL_SIZE,
    feature_size: Metres(2_000.0),
    octaves: 4,
    relief: Metres(120.0),
    water_below: 0.32,
    forest_above: 0.52,
    rock_above: 0.78,
    // Swept rather than picked, over five thresholds and three seeds, against
    // two things at once: how far the longest connected channel runs, and how
    // many of sixteen bearings a 2 km road can take without meeting water.
    //
    // Low thresholds give a dendritic mesh of streams — every bearing blocked
    // on every seed, which is not a republic divided by a river but one that
    // cannot build a road. High ones give a channel so slight it is a puddle.
    // At 0.100 the main river still runs 2.1–4.4 km across a 6 km map while two
    // seeds of three are genuinely cut in half and the third is not, which is
    // the land varying rather than a rule applying.
    river_catchment: 0.100,
    broad_catchment: 0.250,
};

/// Generate the ground of a square map.
///
/// The noise field is a continuous function of position, so changing
/// `plan.cell_size` re-samples the *same* landscape more or less finely rather
/// than generating a different one — which is what makes a resolution sweep a
/// fair comparison instead of four unrelated maps.
pub fn generate_terrain(seed: u64, extent: Metres, plan: &TerrainPlan) -> Terrain {
    let mut terrain = Terrain::flat_with(extent, plan.cell_size);
    let cells = terrain.cells();

    for cy in 0..cells {
        for cx in 0..cells {
            let p = terrain.cell_centre(cx, cy);
            let n = fractal_noise(seed, p, plan.feature_size, plan.octaves);
            let i = (cy as usize) * (cells as usize) + (cx as usize);

            terrain.surface[i] = if n < plan.water_below {
                Surface::Water
            } else if n > plan.rock_above {
                Surface::Rock
            } else if n > plan.forest_above {
                Surface::Forest
            } else {
                Surface::Grass
            };
            terrain.height[i] = (n * plan.relief.0) as f32;
        }
    }

    carve_rivers(&mut terrain, plan);
    terrain
}

/// Cut river channels into a generated map by following the water downhill.
///
/// # Why the generator needed this
///
/// Thresholding noise gives **lakes, not rivers**: isolated basins wherever the
/// field dips below `water_below`, with nothing joining them. Measured across
/// three seeds before this existed, a 6 km map had between 0% and 3.2% water in
/// at most six disconnected bodies, the largest of them 0.57 km². That is a map
/// on which [`crate::roadworks::Grade::Bridge`] almost never has a river to
/// span and a barge has nowhere to go — two mechanics with no subject.
///
/// A river is not a shape you draw, it is where the water ends up, so this is
/// the standard hydrological answer: every cell sheds its own rain, each shove
/// goes to the lowest neighbour, and a cell carrying more than
/// [`TerrainPlan::river_catchment`] of the map is a channel. The result is
/// dendritic and connected by construction — tributaries join, channels widen
/// downstream, and the whole thing runs to a lake or off the edge, because that
/// is where downhill goes.
///
/// # Determinism
///
/// Comparisons, additions and an index tie-break: no transcendentals, no hash
/// iteration, nothing address-dependent. The frontier is ordered by
/// `(height, index)`, which is a total order over every float including the
/// ties — ordering by height alone would leave equal cells to pop in whatever
/// order the heap felt like, and both the fill and the accumulation depend on
/// that order. Same bar as the rest of worldgen: a shared seed is a promise
/// between players.
fn carve_rivers(terrain: &mut Terrain, plan: &TerrainPlan) {
    let n = terrain.cells as usize;
    let count = n * n;
    if count == 0 {
        return;
    }

    // **Fill the hollows before following the water.** Measured on the first
    // version, which did not: 601 disconnected channels on a 6 km map, none
    // spanning more than 1.7 km. Multi-octave noise is full of local minima, so
    // a channel ran from wherever it first crossed the threshold to the nearest
    // pit a few hundred metres later and stopped. Flow accumulation only tells
    // you where water goes if there is somewhere for it to go.
    //
    // The fill hands back the order it visited cells in, which is ascending by
    // filled height — so reversing it is the descending order the accumulation
    // needs, and **the sort that used to produce it is not needed at all**. That
    // was worth finding: sorting a million cells was the single largest cost in
    // worldgen, and the answer had already been computed by the pass before it.
    let (drained, uphill) = fill_depressions(&terrain.height, n);

    // Every cell sheds its own rain, and passes on whatever reached it.
    let mut flow = vec![1.0_f32; count];
    // Which way each cell drains, kept so the channel can be carved along the
    // line the water actually takes rather than guessed at afterwards.
    let mut down = vec![u32::MAX; count];
    for &i in uphill.iter().rev() {
        let i = i as usize;
        let (x, y) = (i % n, i / n);
        let here = drained[i];

        // The steepest way down, ties to the lower index so two equal
        // neighbours always resolve the same way.
        let mut lowest: Option<(f32, usize)> = None;
        for dy in -1_i64..=1 {
            for dx in -1_i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= n as i64 || ny >= n as i64 {
                    continue;
                }
                let j = ny as usize * n + nx as usize;
                let h = drained[j];
                if h >= here {
                    continue;
                }
                // Ties keep the neighbour found first, and the neighbours are
                // walked in a fixed order — so two equally low ways down always
                // resolve the same way on any machine.
                if lowest.is_none_or(|(best, _)| h.total_cmp(&best).is_lt()) {
                    lowest = Some((h, j));
                }
            }
        }

        // Nowhere lower to go. After the fill this only happens at the map
        // edge, which is where the water leaves the republic.
        if let Some((_, j)) = lowest {
            flow[j] += flow[i];
            down[i] = j as u32;
        }
    }

    let river = (plan.river_catchment * count as f64) as f32;
    let broad = (plan.broad_catchment * count as f64) as f32;
    if river <= 0.0 {
        return;
    }

    // Two passes, because widening reads the channel it is widening: marking
    // as we go would let a freshly widened cell seed further widening and the
    // rivers would creep outward across the map.
    let channels: Vec<usize> = (0..count).filter(|&i| flow[i] >= river).collect();
    for &i in &channels {
        terrain.surface[i] = Surface::Water;

        // **A diagonal step is not a continuous river.** Water flows to any of
        // eight neighbours, so a channel running across the grain is a chain of
        // cells that touch only at their corners — which reads as a river on a
        // map and is not one on the ground: a road crosses it between the
        // corners without ever being over water, and a boat cannot follow it at
        // all. Measured before this was here: 187 disconnected bodies on a 6 km
        // map, which is one river reported as thirty. Filling the corner the
        // water turns through makes the channel a real division of the map.
        let j = down[i] as usize;
        if down[i] == u32::MAX || j >= count {
            continue;
        }
        let (x, y) = (i % n, i / n);
        let (jx, jy) = (j % n, j / n);
        if jx != x && jy != y {
            // Of the two cells that square off the corner, take the lower —
            // that is the one the water would actually have cut through.
            let (a, b) = (y * n + jx, jy * n + x);
            let corner = if terrain.height[a].total_cmp(&terrain.height[b]).is_le() {
                a
            } else {
                b
            };
            terrain.surface[corner] = Surface::Water;
        }
    }
    for &i in &channels {
        if flow[i] < broad {
            continue;
        }
        let (x, y) = (i % n, i / n);
        for (dx, dy) in [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x as i64 + dx, y as i64 + dy);
            if nx < 0 || ny < 0 || nx >= n as i64 || ny >= n as i64 {
                continue;
            }
            terrain.surface[ny as usize * n + nx as usize] = Surface::Water;
        }
    }
}

/// Raise every hollow to the level of its lowest lip, so that from anywhere on
/// the map the water can find its way to the edge.
///
/// This is priority-flood: start from the border, always take the lowest cell
/// seen so far, and pull each neighbour up to at least that height. A cell can
/// only ever be reached across the *lowest* rim between it and the outside, so
/// the height it is pulled up to is exactly the level a real pond in that
/// hollow would stand at.
///
/// The returned surface is used for routing water only. **The terrain keeps its
/// real heights** — a filled basin is not a hill that got taller, it is a place
/// where a lake would sit, and the map should still say how deep it is.
///
/// Also returns the order cells were visited in, which is ascending by filled
/// height. That is exactly the order flow accumulation needs, reversed, and
/// handing it back saves sorting a million cells for an answer this pass has
/// already worked out.
///
/// # Why a bucket queue and not a heap
///
/// Measured. A binary heap over a million cells was the largest single cost in
/// worldgen once the redundant sort had gone — a 10 km map spent about 90 ms of
/// its 156 in there, and the founding shelf shows *six* candidates and
/// re-derives all of them whenever a filter changes, so it landed on the
/// player's first screen three times over.
///
/// The heap is doing more work than the problem needs. Priority-flood only ever
/// pushes at a level at or above the one it is draining, so the frontier is
/// **monotone** — which is the same property that lets Dijkstra with bounded
/// integer weights use a bucket queue. Heights fall in a known range, so the
/// levels can be an array of queues walked once from the bottom, and the whole
/// pass becomes linear.
///
/// # Determinism
///
/// The level index is `(h - lowest) / STEP` truncated, which is exact IEEE
/// arithmetic and identical on any machine. Within a level cells come out in
/// the order they went in, which is fixed by the algorithm rather than by a
/// comparator. The price is that two cells within one centimetre of each other
/// are treated as level, so a filled pond can stand up to a centimetre off
/// where an exact fill would put it — against 120 m of relief, and only in
/// deciding which way a river leaves a plateau.
fn fill_depressions(height: &[f32], n: usize) -> (Vec<f32>, Vec<u32>) {
    /// How finely the frontier is banded. A centimetre: fine enough that the
    /// fill is exact for anything the simulation can see, coarse enough that
    /// the level array stays small.
    const STEP: f32 = 0.01;

    let count = n * n;
    let mut filled = height.to_vec();
    let mut visited: Vec<u32> = Vec::with_capacity(count);
    if count == 0 {
        return (filled, visited);
    }

    let mut lowest = f32::INFINITY;
    let mut highest = f32::NEG_INFINITY;
    for &h in height {
        if h < lowest {
            lowest = h;
        }
        if h > highest {
            highest = h;
        }
    }
    let level_of = |h: f32| -> usize {
        let band = (h - lowest) / STEP;
        if band <= 0.0 { 0 } else { band as usize }
    };
    // One past the top, so a cell raised to the highest lip still has a home.
    let levels = level_of(highest) + 2;

    let mut closed = vec![false; count];
    let mut frontier: Vec<Vec<u32>> = vec![Vec::new(); levels];
    let push = |i: usize, filled: &[f32], frontier: &mut Vec<Vec<u32>>| {
        frontier[level_of(filled[i]).min(levels - 1)].push(i as u32);
    };

    for k in 0..n {
        for i in [k, (n - 1) * n + k, k * n, k * n + n - 1] {
            if !closed[i] {
                closed[i] = true;
                push(i, &filled, &mut frontier);
            }
        }
    }

    // Walk the levels from the bottom. A level can grow while it is being
    // drained — a neighbour pulled up to the current lip lands back in this
    // same band — so the inner loop indexes rather than iterates.
    for level in 0..levels {
        let mut cursor = 0;
        while cursor < frontier[level].len() {
            let i = frontier[level][cursor];
            cursor += 1;
            visited.push(i);
            let i = i as usize;
            let (x, y) = (i % n, i / n);
            let here = filled[i];
            for dy in -1_i64..=1 {
                for dx in -1_i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= n as i64 || ny >= n as i64 {
                        continue;
                    }
                    let j = ny as usize * n + nx as usize;
                    if closed[j] {
                        continue;
                    }
                    closed[j] = true;
                    // **A hair above, never level.** Raising a hollow to exactly
                    // its lip makes the basin floor perfectly flat, and a flat
                    // has no lower neighbour — so the water arrives in the
                    // middle of a filled lake and stops there, which is the
                    // whole failure this pass exists to fix, moved one step
                    // further along. Because the fill spreads outward *from* the
                    // outlet, adding one ulp per step leaves a gradient that
                    // runs back the way it came, which is the way out.
                    let lip = here.next_up();
                    if filled[j] < lip {
                        filled[j] = lip;
                    }
                    push(j, &filled, &mut frontier);
                }
            }
        }
        // Done with this band; give the memory back rather than holding a
        // million cells' worth of queues to the end of the pass.
        frontier[level] = Vec::new();
    }

    (filled, visited)
}

#[cfg(test)]
mod tests {

    use super::*;

    const SMALL: Metres = Metres(1_000.0); // 100 x 100 cells

    #[test]
    fn a_flat_map_is_the_size_it_was_asked_for() {
        let t = Terrain::flat(SMALL);
        assert_eq!(t.cells(), 100);
        assert_eq!(t.extent(), SMALL);
        assert!(t.contains(Point::new(Metres(999.0), Metres(0.0))));
        assert!(!t.contains(Point::new(Metres(1_000.0), Metres(0.0))));
        assert!(!t.contains(Point::new(Metres(-1.0), Metres(0.0))));
    }

    #[test]
    fn points_map_to_the_cell_they_fall_in() {
        let mut t = Terrain::flat(SMALL);
        t.set_surface(Point::new(Metres(35.0), Metres(65.0)), Surface::Rock);
        // Anywhere in the same 10 m cell reads the same value.
        assert_eq!(
            t.surface_at(Point::new(Metres(31.0), Metres(69.9))),
            Some(Surface::Rock)
        );
        // The neighbouring cell is untouched.
        assert_eq!(
            t.surface_at(Point::new(Metres(41.0), Metres(65.0))),
            Some(Surface::Grass)
        );
        assert_eq!(t.surface_at(Point::new(Metres(2_000.0), Metres(0.0))), None);
    }

    #[test]
    fn cell_centres_land_in_their_own_cell() {
        let t = Terrain::flat(SMALL);
        for (cx, cy) in [(0u32, 0u32), (7, 3), (99, 99)] {
            let centre = t.cell_centre(cx, cy);
            assert_eq!(t.index_of(centre), Some((cy as usize) * 100 + cx as usize));
        }
    }

    /// The check that stops a footprint straddling a lake by landing its
    /// corners on dry ground.
    #[test]
    fn a_footprint_over_any_water_is_not_buildable() {
        let mut t = Terrain::flat(SMALL);
        let centre = Point::new(Metres(500.0), Metres(500.0));
        assert!(t.area_is_buildable(centre, Metres(40.0), Metres(40.0)));

        // One wet cell inside the footprint, well away from its corners.
        t.set_surface(Point::new(Metres(495.0), Metres(495.0)), Surface::Water);
        assert!(!t.area_is_buildable(centre, Metres(40.0), Metres(40.0)));
    }

    #[test]
    fn a_footprint_running_off_the_map_is_not_buildable() {
        let t = Terrain::flat(SMALL);
        assert!(!t.area_is_buildable(
            Point::new(Metres(5.0), Metres(500.0)),
            Metres(40.0),
            Metres(40.0)
        ));
        assert!(!t.area_is_buildable(
            Point::new(Metres(995.0), Metres(500.0)),
            Metres(40.0),
            Metres(40.0)
        ));
    }

    #[test]
    fn generation_reproduces_for_a_seed_and_differs_between_seeds() {
        let a = generate_terrain(1961, SMALL, &DEFAULT_TERRAIN);
        let b = generate_terrain(1961, SMALL, &DEFAULT_TERRAIN);
        let c = generate_terrain(1962, SMALL, &DEFAULT_TERRAIN);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// The cross-machine pin, same role as the geology one.
    #[test]
    fn generated_ground_is_pinned_across_machines() {
        let t = generate_terrain(1961, SMALL, &DEFAULT_TERRAIN);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |bytes: [u8; 4]| {
            for byte in bytes {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        };
        for (height, surface) in t.height.iter().zip(t.surface.iter()) {
            eat(height.to_bits().to_le_bytes());
            eat([*surface as u8, 0, 0, 0]);
        }
        assert_eq!(h, 4_544_466_606_985_391);
    }

    /// A landscape, not noise: neighbouring cells should usually agree, or
    /// the map is static rather than terrain.
    #[test]
    fn the_ground_is_continuous_rather_than_speckled() {
        let t = generate_terrain(7, SMALL, &DEFAULT_TERRAIN);
        let cells = t.cells() as usize;
        let mut same = 0;
        let mut total = 0;
        for cy in 0..cells {
            for cx in 0..cells - 1 {
                let i = cy * cells + cx;
                total += 1;
                if t.surface[i] == t.surface[i + 1] {
                    same += 1;
                }
            }
        }
        let agreement = same as f64 / total as f64;
        assert!(
            agreement > 0.9,
            "neighbouring cells agreed only {agreement:.2} of the time — that is noise, not terrain"
        );
    }

    /// Every water body on a map, as `(cells, longest side of its bounding
    /// box in cells)`, walking **orthogonally** — the connectivity a boat has
    /// and a road crossing is measured against, not the eight-way one the
    /// water was routed with.
    fn water_bodies(t: &Terrain) -> Vec<(usize, usize)> {
        let n = t.cells() as usize;
        let mut seen = vec![false; n * n];
        let mut bodies = Vec::new();
        for start in 0..n * n {
            if seen[start] || t.surface[start] != Surface::Water {
                continue;
            }
            let (mut stack, mut size) = (vec![start], 0usize);
            seen[start] = true;
            let (mut mnx, mut mxx, mut mny, mut mxy) = (n, 0usize, n, 0usize);
            while let Some(i) = stack.pop() {
                size += 1;
                let (x, y) = (i % n, i / n);
                mnx = mnx.min(x);
                mxx = mxx.max(x);
                mny = mny.min(y);
                mxy = mxy.max(y);
                for (dx, dy) in [(-1_i64, 0_i64), (1, 0), (0, -1), (0, 1)] {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= n as i64 || ny >= n as i64 {
                        continue;
                    }
                    let j = ny as usize * n + nx as usize;
                    if !seen[j] && t.surface[j] == Surface::Water {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            bodies.push((size, (mxx - mnx).max(mxy - mny)));
        }
        bodies.sort_unstable_by_key(|&(_, span)| std::cmp::Reverse(span));
        bodies
    }

    /// The premise every water mechanic in this build rests on.
    ///
    /// Before rivers were carved, thresholding noise gave lakes: measured
    /// across three seeds, a 6 km map had between **0.0% and 3.2%** water in at
    /// most six disconnected bodies, the largest 0.57 km² and none spanning
    /// more than 1.6 km — and one seed had **no water at all**. A bridge had
    /// nothing to span and a barge had nowhere to go, so two mechanics were
    /// being built against a map that could not exercise either.
    ///
    /// This asserts the premise rather than describing it, because a premise
    /// recorded in prose stops being true silently. It is the test that fails
    /// if a future generator change quietly drains the map.
    #[test]
    fn every_map_gets_a_river_that_crosses_a_good_part_of_it() {
        for seed in [1961_u64, 7, 42] {
            let t = generate_terrain(seed, Metres(6_000.0), &DEFAULT_TERRAIN);
            let bodies = water_bodies(&t);
            let (_, span) = *bodies.first().expect("some water");
            let km = span as f64 * t.cell_size().0 / 1_000.0;
            assert!(
                km >= 2.0,
                "seed {seed}: the longest water body spans {km:.1} km of a 6 km map"
            );
        }
    }

    /// A river has to be a **continuous** thing, or it is not a barrier.
    ///
    /// Water is routed to any of eight neighbours, so a channel running across
    /// the grain comes out as cells that touch only at their corners. That
    /// reads as a river on a map and is not one on the ground: a road crosses
    /// between the corners without ever being over water, and nothing can
    /// follow it. Measured before the corners were squared off: **187**
    /// disconnected bodies on a 6 km map, which is one river reported as thirty.
    #[test]
    fn a_river_is_orthogonally_continuous_rather_than_a_chain_of_corners() {
        let t = generate_terrain(1961, Metres(6_000.0), &DEFAULT_TERRAIN);
        let bodies = water_bodies(&t);
        assert!(
            bodies.len() < 40,
            "{} separate water bodies — the channels are breaking into corner-touching fragments",
            bodies.len()
        );
        // And the big one is genuinely a channel rather than a blob: long
        // against its own area.
        let (cells, span) = bodies[0];
        assert!(
            span * span > cells,
            "the largest body is {cells} cells across a {span}-cell span — that is a lake, not a river"
        );
    }

    /// Water runs downhill and leaves. A channel that stops in the middle of
    /// the map is a hollow that was never filled, which is the failure that
    /// made the first two attempts at this produce hundreds of stubs.
    #[test]
    fn the_main_river_reaches_the_edge_of_the_map() {
        let t = generate_terrain(1961, Metres(6_000.0), &DEFAULT_TERRAIN);
        let n = t.cells() as usize;
        let touches_edge = (0..n).any(|k| {
            [k, (n - 1) * n + k, k * n, k * n + n - 1]
                .iter()
                .any(|&i| t.surface[i] == Surface::Water)
        });
        assert!(touches_edge, "no water leaves the republic");
    }

    #[test]
    fn a_generated_map_has_a_mix_of_surfaces() {
        let t = generate_terrain(1961, Metres(4_000.0), &DEFAULT_TERRAIN);
        for surface in [Surface::Grass, Surface::Forest, Surface::Water] {
            assert!(
                t.fraction_of(surface) > 0.01,
                "{surface:?} covered {:.3} of the map",
                t.fraction_of(surface)
            );
        }
        let total: f64 = [
            Surface::Grass,
            Surface::Forest,
            Surface::Rock,
            Surface::Water,
        ]
        .iter()
        .map(|&s| t.fraction_of(s))
        .sum();
        assert!((total - 1.0).abs() < 1e-9, "fractions must cover the map");
    }

    /// The correctness floor the resolution sweep is decided against.
    ///
    /// Buildability is sampled on the cell lattice, so a footprint smaller than
    /// one cell in an axis is tested against a single sample and a house and a
    /// shop become the same rounded square. That is precisely the grid artefact
    /// this build exists to remove, so it is a hard floor rather than a
    /// preference: if a smaller building is ever authored, the resolution has to
    /// follow it down and this test says so before anyone measures speed.
    #[test]
    fn the_smallest_building_still_covers_a_cell() {
        use crate::building::BUILDINGS;
        let narrowest = BUILDINGS
            .iter()
            .min_by(|a, b| a.width.0.total_cmp(&b.width.0))
            .expect("the table is not empty");
        let shallowest = BUILDINGS
            .iter()
            .min_by(|a, b| a.depth.0.total_cmp(&b.depth.0))
            .expect("the table is not empty");
        assert!(
            narrowest.width.0 >= DEFAULT_CELL_SIZE.0,
            "{} is {} m wide, narrower than a {} m cell",
            narrowest.name,
            narrowest.width.0,
            DEFAULT_CELL_SIZE.0
        );
        assert!(
            shallowest.depth.0 >= DEFAULT_CELL_SIZE.0,
            "{} is {} m deep, shallower than a {} m cell",
            shallowest.name,
            shallowest.depth.0,
            DEFAULT_CELL_SIZE.0
        );
    }

    /// Resolution travels with the map. A coarser map is the same landscape
    /// sampled less finely, not a different one — which is what makes the
    /// sweep a fair comparison.
    #[test]
    fn a_map_carries_its_own_resolution() {
        let coarse = TerrainPlan {
            cell_size: Metres(50.0),
            ..DEFAULT_TERRAIN
        };
        let fine = generate_terrain(1961, SMALL, &DEFAULT_TERRAIN);
        let rough = generate_terrain(1961, SMALL, &coarse);

        assert_eq!(fine.cell_size(), Metres(10.0));
        assert_eq!(rough.cell_size(), Metres(50.0));
        assert_eq!(fine.cells(), 100);
        assert_eq!(rough.cells(), 20);
        // Same square kilometre either way.
        assert_eq!(fine.extent(), rough.extent());

        // And the same landscape underneath: a point well inside a coarse cell
        // reads the same surface from both, because both sample one field.
        let sampled = rough.cell_centre(7, 11);
        assert_eq!(rough.surface_at(sampled), fine.surface_at(sampled));
    }

    /// A save written at one resolution must reload at that resolution, not at
    /// whatever the build currently defaults to.
    #[test]
    fn resolution_survives_a_save() {
        let coarse = generate_terrain(
            5,
            SMALL,
            &TerrainPlan {
                cell_size: Metres(25.0),
                ..DEFAULT_TERRAIN
            },
        );
        let wire = postcard::to_stdvec(&coarse).expect("serializes");
        let back: Terrain = postcard::from_bytes(&wire).expect("parses");
        assert_eq!(back.cell_size(), Metres(25.0));
        assert_eq!(back, coarse);
    }

    #[test]
    fn water_is_neither_buildable_nor_walkable() {
        assert!(!Surface::Water.is_buildable());
        assert!(!Surface::Water.is_walkable());
        for s in [Surface::Grass, Surface::Forest, Surface::Rock] {
            assert!(s.is_buildable(), "{s:?}");
            assert!(s.is_walkable(), "{s:?}");
        }
    }
}
