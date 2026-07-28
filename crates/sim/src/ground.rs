//! The state of the ground: how wet it is, how frozen, and what that costs to
//! drive across.
//!
//! # Why this is state and not a function of the calendar
//!
//! [`crate::climate`] makes today's temperature and today's rain pure functions
//! of `(seed, day)`, so a forecast never perturbs anything. Soil is not like
//! that. Water that fell last week is still in the ground this week; snow that
//! fell in December is still lying in February and comes off all at once in
//! March. **Moisture and snow are accumulated state**, and modelling them as a
//! function of the date would throw away the only interesting thing about them.
//!
//! This is a deliberate departure from the plan, which proposed computing
//! softness purely from a window of recent days. Two things decided it. A
//! window long enough to hold a winter's snow is sixty days of substream draws
//! per query, and phase four asks this question once per vehicle per leg; and a
//! window cannot express the *carry* that makes a thaw — the pack has to
//! survive from one query to the next to be able to melt.
//!
//! What is kept from the plan is that nothing here is seasonal. Frost follows
//! the temperature, melt follows the temperature, and a warm February does the
//! same thing to the ground that a warm March does.
//!
//! # The spring thaw falls out; it is not written down
//!
//! Three rules, none of which mentions spring:
//!
//! - snow lies while it is below freezing and melts above it,
//! - meltwater goes into the topsoil like rain does,
//! - frost lags the air temperature, because soil has thermal mass.
//!
//! Run them through a winter and the worst going of the year lands a week or so
//! after the first warm spell: a season's snow arriving in the topsoil at once
//! while the frost that was holding the ground up is still on its way out. That
//! is the *rasputitsa*, and it is the seasonal event this whole model exists to
//! produce.

use crate::terrain::Surface;
use crate::units::{Metres, Point};
use serde::{Deserialize, Serialize};

/// The temperature at which water freezes. Named rather than written as `0.0`
/// so the comparisons read as decisions rather than as sign checks.
pub const FREEZE_C: f64 = 0.0;

/// How far below freezing the air has to sit for the ground to be fully frozen.
pub const FROST_RANGE_C: f64 = 8.0;

/// How much of the gap to today's conditions the frost closes in a day.
///
/// Soil has thermal mass: one mild afternoon does not thaw a frozen field and
/// one cold night does not freeze a wet one. This lag is what makes the thaw a
/// period rather than a moment.
pub const FROST_LAG: f64 = 0.12;

/// Millimetres of water the topsoil holds when it is saturated.
pub const SATURATION_MM: f64 = 40.0;

/// Millimetres of snow that melt per degree above freezing, per day.
pub const MELT_PER_DEGREE_MM: f64 = 2.5;

/// Share of its water the topsoil gives up on a warm, snow-free day.
pub const DRYING_PER_DAY: f64 = 0.10;

/// How warm it has to be for drying to run at full rate, above freezing.
pub const DRYING_FULL_AT_C: f64 = 15.0;

/// Millimetres of water that fill the **root zone** from empty.
///
/// Four times [`SATURATION_MM`], because this is not the same body of water as
/// the topsoil. `moisture` is the top few centimetres — what decides whether a
/// lorry sinks — and it is gone ten days after rain. A root draws on something
/// deeper and far slower, which is why a crop survives a fortnight of sun and a
/// lorry stops bogging after a weekend of it.
pub const ROOT_SATURATION_MM: f64 = 160.0;

/// How fast the root zone gives up water on a warm day — an eighth of
/// [`DRYING_PER_DAY`].
///
/// **Measured, not chosen:** using `moisture` as the crop's water supply killed
/// every harvest, because on the 188 days a year warm enough to grow anything
/// its median value is 0.000 and its p75 is 0.088. Tuning the crop threshold
/// against that would only have made the harvest depend on which day a rain
/// burst happened to land. The signal was wrong, not the constant.
pub const ROOT_DRYING_PER_DAY: f64 = 0.012;

/// How wet and how frozen the open ground is.
///
/// One figure for the whole republic. Weather is regional at this scale — a map
/// is ten kilometres across and it does not rain on half of it — so the
/// variation that matters is the *surface*, which is static and lives on the
/// terrain. See [`going`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Ground {
    /// Water in the topsoil, `0.0` bone dry to `1.0` saturated.
    ///
    /// **This is the trafficability figure, not the agricultural one.** It is
    /// the top few centimetres, it decides whether a lorry sinks, and it is dry
    /// most of the summer. Crops read [`Ground::water`].
    pub moisture: f64,
    /// Water in the **root zone**, `0.0` exhausted to `1.0` full.
    ///
    /// Fed by the same rain and meltwater as [`Ground::moisture`] and drained
    /// by the same warmth, but four times the reservoir and an eighth the
    /// drying rate — so it carries a crop through a dry fortnight the way real
    /// subsoil does. Kept as a peer field rather than derived, because the two
    /// answer genuinely different questions and neither is a function of the
    /// other.
    pub water: f64,
    /// Snow lying, in millimetres of water equivalent.
    pub snow: f64,
    /// How frozen the ground is, `0.0` soft to `1.0` set hard.
    pub frost: f64,
}

impl Default for Ground {
    /// A republic is founded on 1 March, at the end of a winter it did not
    /// simulate. Starting bone dry and unfrozen would hand every founding a
    /// spring that never happened; starting damp and part-frozen is the honest
    /// guess, and a week of real weather washes it out either way.
    fn default() -> Self {
        Self {
            moisture: 0.5,
            water: 0.5,
            snow: 0.0,
            frost: 0.3,
        }
    }
}

impl Ground {
    /// Take one day of weather.
    pub fn advance(&mut self, temperature_c: f64, precipitation_mm: f64) {
        let target = ((FREEZE_C - temperature_c) / FROST_RANGE_C).clamp(0.0, 1.0);
        self.frost += (target - self.frost) * FROST_LAG;

        let freezing = temperature_c < FREEZE_C;
        let melt = if freezing {
            0.0
        } else {
            ((temperature_c - FREEZE_C) * MELT_PER_DEGREE_MM).min(self.snow)
        };
        // Below freezing it falls as snow and lies; above it, it runs straight
        // into the ground along with whatever the pack is giving up.
        let fell_as_snow = if freezing { precipitation_mm } else { 0.0 };
        self.snow = (self.snow + fell_as_snow - melt).max(0.0);

        let water = if freezing { 0.0 } else { precipitation_mm } + melt;
        self.moisture = (self.moisture + water / SATURATION_MM).min(1.0);
        self.water = (self.water + water / ROOT_SATURATION_MM).min(1.0);

        // It dries out only when it is warm and there is nothing lying on top.
        if !freezing && self.snow <= 0.0 {
            let warmth = ((temperature_c - FREEZE_C) / DRYING_FULL_AT_C).clamp(0.0, 1.5);
            self.moisture = (self.moisture - DRYING_PER_DAY * warmth).max(0.0);
            self.water = (self.water - ROOT_DRYING_PER_DAY * warmth).max(0.0);
        }
    }

    /// How badly the open ground would bog a vehicle today: `0.0` firm, `1.0`
    /// impassable.
    ///
    /// **Frozen ground is hard however wet it is.** A frozen bog is a road, and
    /// that is not a quirk of the arithmetic — it is why winter haulage across
    /// country is easier than spring haulage, and why the thaw is the event
    /// rather than the rain.
    pub fn softness(&self) -> f64 {
        (self.moisture * (1.0 - self.frost)).clamp(0.0, 1.0)
    }

    /// What the going is on a particular surface today.
    pub fn going_on(&self, surface: Surface) -> f64 {
        (self.softness() * going(surface)).clamp(0.0, 1.0)
    }

    /// The same, rolled forward `days` from here.
    ///
    /// A forecast, and the reason it can exist at all is that temperature and
    /// rain are pure: rolling the recurrence forward from today costs one
    /// substream draw per day and moves nothing.
    pub fn forecast(
        &self,
        mut weather: impl FnMut(u64) -> (f64, f64),
        from_day: u64,
        days: u64,
    ) -> Ground {
        let mut ahead = *self;
        for step in 0..days {
            let (temperature, rain) = weather(from_day + step + 1);
            ahead.advance(temperature, rain);
        }
        ahead
    }
}

/// The side of one traversal cell.
///
/// A hundred metres, which makes a ten-kilometre republic a 100 x 100 lattice —
/// **ten thousand cells**, against the million the terrain grid holds. That
/// two-orders-of-magnitude gap is what makes routing across country affordable
/// at all, and it is why this is a lattice of its own rather than a field on
/// the terrain: what varies at ten metres is where a building can stand, and
/// what varies at a hundred is where a lorry would rather drive.
pub const GROUND_CELL: Metres = Metres(100.0);

/// How much longer fully soft ground takes to cross than firm ground.
pub const MUD_DRAG: f64 = 2.0;

/// How much of a cell has to be water before nothing can cross it.
const DROWNED: f64 = 0.25;

/// How much one laden pass packs a cell down.
///
/// Fifty passes to turn open field into a made track, less the fading. That is
/// deliberately a season's worth of traffic rather than a week's: a track a
/// republic did not plan should arrive slowly enough to be noticed happening.
pub const WEAR_PER_PASS: f64 = 0.02;

/// How much of its packing a cell loses in a day.
///
/// Without this every line any lorry ever drove is permanent, and a map ends up
/// covered in the ghosts of routes nobody uses. With it a corridor has to be
/// *kept* — roughly a pass every other day merely holds station.
pub const WEAR_FADE_PER_DAY: f64 = 0.01;

/// The packing at which a corridor stops being a worn line and becomes a track
/// on the map.
pub const PROMOTE_AT: f64 = 0.85;

/// The shortest run of worn cells worth calling a road: three hundred metres.
/// Below that it is a gateway, not a route.
pub const MIN_TRACK_CELLS: usize = 3;

/// The lattice a vehicle crosses country over.
///
/// Carries the **static** part of the going — what the surface is — because
/// that is what varies by place. How wet it is today is one number for the
/// whole republic and lives on [`Ground`]. Phase five's wear rides here too,
/// on the same cells, so the thing that records a corridor and the thing that
/// routes over one are the same structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lattice {
    cells: u32,
    /// Persisted rather than read from [`GROUND_CELL`], for the same reason the
    /// terrain carries its own resolution: a save always knows what it was
    /// written at, and re-measuring stays a one-line experiment.
    cell_size: Metres,
    /// Static going multiplier per cell, `f32::INFINITY` where nothing crosses.
    surface: Vec<f32>,
    /// How worn each cell is, `0.0` untouched to `1.0` a made track.
    wear: Vec<f32>,
}

impl Lattice {
    /// Build the lattice by sampling the terrain.
    ///
    /// Each cell reads a 5 x 5 grid of the ground under it, so a cell is a
    /// summary of what is actually there rather than of whatever happened to
    /// be at its centre. A cell that is a quarter water is water: you cannot
    /// drive round the corner of a lake inside a hundred-metre square.
    pub fn from_terrain(terrain: &crate::terrain::Terrain) -> Self {
        let cell_size = GROUND_CELL;
        let cells = (terrain.extent().0 / cell_size.0).ceil().max(1.0) as u32;
        let total = (cells as usize) * (cells as usize);
        let mut surface = vec![1.0f32; total];

        const SAMPLES: u32 = 5;
        for cy in 0..cells {
            for cx in 0..cells {
                let mut sum = 0.0;
                let mut dry = 0u32;
                let mut wet = 0u32;
                for sy in 0..SAMPLES {
                    for sx in 0..SAMPLES {
                        let at = Point::new(
                            Metres(
                                (f64::from(cx) + (f64::from(sx) + 0.5) / f64::from(SAMPLES))
                                    * cell_size.0,
                            ),
                            Metres(
                                (f64::from(cy) + (f64::from(sy) + 0.5) / f64::from(SAMPLES))
                                    * cell_size.0,
                            ),
                        );
                        match terrain.surface_at(at) {
                            Some(Surface::Water) | None => wet += 1,
                            Some(other) => {
                                sum += going(other);
                                dry += 1;
                            }
                        }
                    }
                }
                let seen = wet + dry;
                let drowned = seen == 0 || f64::from(wet) / f64::from(seen) >= DROWNED;
                surface[(cy as usize) * (cells as usize) + cx as usize] = if drowned {
                    f32::INFINITY
                } else {
                    (sum / f64::from(dry)) as f32
                };
            }
        }
        Self {
            cells,
            cell_size,
            surface,
            wear: vec![0.0; total],
        }
    }

    pub fn cells(&self) -> u32 {
        self.cells
    }

    pub fn cell_size(&self) -> Metres {
        self.cell_size
    }

    /// The cell a point falls in, or `None` off the map.
    pub fn cell_of(&self, at: Point) -> Option<usize> {
        if at.x.0 < 0.0 || at.y.0 < 0.0 {
            return None;
        }
        let cx = (at.x.0 / self.cell_size.0) as u32;
        let cy = (at.y.0 / self.cell_size.0) as u32;
        if cx >= self.cells || cy >= self.cells {
            return None;
        }
        Some((cy as usize) * (self.cells as usize) + cx as usize)
    }

    pub fn centre_of(&self, index: usize) -> Point {
        let cx = (index % self.cells as usize) as f64;
        let cy = (index / self.cells as usize) as f64;
        Point::new(
            Metres((cx + 0.5) * self.cell_size.0),
            Metres((cy + 0.5) * self.cell_size.0),
        )
    }

    pub fn wear_at(&self, index: usize) -> f64 {
        f64::from(self.wear[index])
    }

    /// Wear a cell in, capped at a made track.
    pub fn wear_in(&mut self, index: usize, by: f64) {
        let worn = (f64::from(self.wear[index]) + by).clamp(0.0, 1.0);
        self.wear[index] = worn as f32;
    }

    /// Let every cell recover a little. Without this, every route ever driven
    /// is permanent.
    pub fn fade(&mut self, by: f64) {
        for cell in &mut self.wear {
            *cell = (f64::from(*cell) - by).max(0.0) as f32;
        }
    }

    /// Every cell worn beyond a threshold, in index order.
    pub fn worn_beyond(&self, threshold: f64) -> Vec<usize> {
        self.wear
            .iter()
            .enumerate()
            .filter(|&(_, &w)| f64::from(w) >= threshold)
            .map(|(i, _)| i)
            .collect()
    }

    /// The eight cells around one, in a fixed order.
    pub fn neighbours(&self, index: usize) -> Vec<usize> {
        let side = self.cells as i64;
        let (cx, cy) = ((index as i64) % side, (index as i64) / side);
        let mut out = Vec::with_capacity(8);
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (cx + dx, cy + dy);
                if nx >= 0 && ny >= 0 && nx < side && ny < side {
                    out.push((ny * side + nx) as usize);
                }
            }
        }
        out
    }

    /// Every distinct cell a straight run passes through, in the order it meets
    /// them. What a leg actually drove over, and therefore what it packed down.
    pub fn cells_along(&self, from: Point, to: Point) -> Vec<usize> {
        let distance = from.distance_to(to).0;
        let steps = ((distance / (self.cell_size.0 * 0.5)).ceil() as u32).clamp(1, 512);
        let mut out: Vec<usize> = Vec::new();
        for step in 0..=steps {
            let t = f64::from(step) / f64::from(steps);
            let at = Point::new(
                Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                Metres(from.y.0 + (to.y.0 - from.y.0) * t),
            );
            if let Some(cell) = self.cell_of(at)
                && out.last() != Some(&cell)
                && !out.contains(&cell)
            {
                out.push(cell);
            }
        }
        out
    }

    /// Connected runs of cells worn past a threshold, longest-lived first by
    /// index so the answer is reproducible.
    ///
    /// Runs shorter than [`MIN_TRACK_CELLS`] are left alone: a couple of worn
    /// squares outside a loading bay is a yard, not a road, and promoting it
    /// would litter the network with stubs.
    pub fn tracks_beyond(&self, threshold: f64) -> Vec<Vec<usize>> {
        let worn = self.worn_beyond(threshold);
        let mut seen = vec![false; (self.cells as usize) * (self.cells as usize)];
        let mut runs = Vec::new();
        for &start in &worn {
            if seen[start] {
                continue;
            }
            let mut run = Vec::new();
            let mut frontier = vec![start];
            seen[start] = true;
            while let Some(cell) = frontier.pop() {
                run.push(cell);
                for next in self.neighbours(cell) {
                    if !seen[next] && f64::from(self.wear[next]) >= threshold {
                        seen[next] = true;
                        frontier.push(next);
                    }
                }
            }
            if run.len() >= MIN_TRACK_CELLS {
                run.sort_unstable();
                runs.push(run);
            }
        }
        runs
    }
}

/// The lattice plus today's conditions: everything needed to cost a crossing.
#[derive(Debug, Clone, Copy)]
pub struct Crossing<'a> {
    pub lattice: &'a Lattice,
    /// Today's softness, from [`Ground::softness`].
    pub softness: f64,
}

/// How much of the going a made track takes away.
///
/// A worn corridor is packed down and drains where a field does not, so the
/// first thing traffic buys itself is firmer ground — which is what makes the
/// feedback loop that grows a road.
pub const WEAR_RELIEF: f64 = 0.8;

impl Crossing<'_> {
    /// How bad the going is in one cell today, `0.0` firm to `1.0` impassable.
    pub fn going_in(&self, index: usize) -> f64 {
        let surface = f64::from(self.lattice.surface[index]);
        if !surface.is_finite() {
            return f64::INFINITY;
        }
        let relief = 1.0 - WEAR_RELIEF * self.lattice.wear_at(index);
        (self.softness * surface * relief).clamp(0.0, 1.0)
    }

    /// How much longer a cell takes to cross than firm open ground.
    pub fn drag_in(&self, index: usize) -> f64 {
        let going = self.going_in(index);
        if !going.is_finite() {
            return f64::INFINITY;
        }
        1.0 + MUD_DRAG * going
    }

    /// The going at a point, or `1.0` off the map.
    pub fn going_at(&self, at: Point) -> f64 {
        self.lattice
            .cell_of(at)
            .map_or(1.0, |cell| self.going_in(cell).min(1.0))
    }

    /// The mean drag over a straight run, sampled at cell resolution.
    ///
    /// What a leg is actually timed at. Sampled rather than integrated because
    /// a leg is a straight line over a lattice and the answer only has to be
    /// as good as the lattice is.
    pub fn drag_along(&self, from: Point, to: Point) -> f64 {
        let distance = from.distance_to(to).0;
        let steps = ((distance / self.lattice.cell_size.0).ceil() as u32).clamp(1, 128);
        let mut total = 0.0;
        for step in 0..steps {
            let t = (f64::from(step) + 0.5) / f64::from(steps);
            let at = Point::new(
                Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                Metres(from.y.0 + (to.y.0 - from.y.0) * t),
            );
            let drag = match self.lattice.cell_of(at) {
                Some(cell) => self.drag_in(cell),
                None => 1.0,
            };
            // An impassable sample is not infinitely slow, it is a route that
            // should not have been planned. Priced high but finite, so the
            // planner rejects it in favour of anything else rather than
            // producing an arrival time of NaN.
            total += if drag.is_finite() {
                drag
            } else {
                1.0 + MUD_DRAG * 8.0
            };
        }
        total / f64::from(steps)
    }

    /// The **worst** going anywhere along a straight run.
    ///
    /// Worst rather than mean, because a lorry does not average its way across
    /// a field: it sticks in the one soft patch, and the rest of the crossing
    /// being firm is no help at all once it has.
    pub fn going_along(&self, from: Point, to: Point) -> f64 {
        let distance = from.distance_to(to).0;
        let steps = ((distance / (self.lattice.cell_size.0 * 0.5)).ceil() as u32).clamp(1, 256);
        (0..=steps)
            .map(|step| {
                let t = f64::from(step) / f64::from(steps);
                let at = Point::new(
                    Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                    Metres(from.y.0 + (to.y.0 - from.y.0) * t),
                );
                self.going_at(at)
            })
            .fold(0.0, f64::max)
    }

    /// Whether a straight run between two points crosses anything impassable.
    pub fn is_clear(&self, from: Point, to: Point) -> bool {
        let distance = from.distance_to(to).0;
        // Half-cell steps, so a lake cannot be stepped over.
        let steps = ((distance / (self.lattice.cell_size.0 * 0.5)).ceil() as u32).clamp(1, 256);
        (0..=steps).all(|step| {
            let t = f64::from(step) / f64::from(steps);
            let at = Point::new(
                Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                Metres(from.y.0 + (to.y.0 - from.y.0) * t),
            );
            match self.lattice.cell_of(at) {
                Some(cell) => self.drag_in(cell).is_finite(),
                // Off the map is not something to route through, but the ends
                // of a journey may legitimately sit on the boundary.
                None => true,
            }
        })
    }

    /// The best, worst and mean drag along a straight run, in one pass.
    ///
    /// `None` if anything on the way is impassable.
    pub fn profile_along(&self, from: Point, to: Point) -> Option<(f64, f64, f64)> {
        let distance = from.distance_to(to).0;
        let steps = ((distance / (self.lattice.cell_size.0 * 0.5)).ceil() as u32).clamp(1, 256);
        let (mut best, mut worst, mut total) = (f64::INFINITY, 0.0f64, 0.0);
        for step in 0..=steps {
            let t = f64::from(step) / f64::from(steps);
            let at = Point::new(
                Metres(from.x.0 + (to.x.0 - from.x.0) * t),
                Metres(from.y.0 + (to.y.0 - from.y.0) * t),
            );
            let drag = match self.lattice.cell_of(at) {
                Some(cell) => self.drag_in(cell),
                // The ends of a journey may legitimately sit on the boundary.
                None => 1.0,
            };
            if !drag.is_finite() {
                return None;
            }
            best = best.min(drag);
            worst = worst.max(drag);
            total += drag;
        }
        Some((best, worst, total / f64::from(steps + 1)))
    }

    /// A way across country, as waypoints.
    ///
    /// A* over the lattice, costed in metres-times-drag with straight-line
    /// distance as the heuristic — admissible because drag is never below one.
    /// Ties break on the lower cell index, so two equally good ways across a
    /// field always resolve the same way.
    ///
    /// `None` when there is no way through at all, which is the honest answer
    /// for a building on the far side of a lake.
    pub fn route(&self, from: Point, to: Point) -> Option<Vec<Point>> {
        let start = self.lattice.cell_of(from)?;
        let goal = self.lattice.cell_of(to)?;
        if !self.drag_in(start).is_finite() || !self.drag_in(goal).is_finite() {
            return None;
        }
        if start == goal {
            return Some(vec![from, to]);
        }

        // Uniform ground: the straight line is exactly optimal and no search can
        // do better, so do not run one. Most of a map is one kind of ground and
        // most crossings are short, which makes this the common case rather than
        // the lucky one — and it is the difference between cross-country routing
        // being affordable and not.
        if let Some((best, worst, _)) = self.profile_along(from, to)
            && worst <= best * 1.02
        {
            return Some(vec![from, to]);
        }

        let side = self.lattice.cells as usize;
        let total = side * side;
        let goal_at = self.lattice.centre_of(goal);
        let mut best = vec![f64::INFINITY; total];
        let mut came_from = vec![usize::MAX; total];
        let mut settled = vec![false; total];
        let mut heap = std::collections::BinaryHeap::new();

        best[start] = 0.0;
        heap.push(Step {
            estimate: self.lattice.centre_of(start).distance_to(goal_at).0 * HURRY,
            cost: 0.0,
            cell: start,
        });

        while let Some(Step { cost, cell, .. }) = heap.pop() {
            if settled[cell] {
                continue;
            }
            settled[cell] = true;
            if cell == goal {
                break;
            }
            let (cx, cy) = ((cell % side) as i64, (cell / side) as i64);
            let here = self.lattice.centre_of(cell);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (nx, ny) = (cx + dx, cy + dy);
                    if nx < 0 || ny < 0 || nx >= side as i64 || ny >= side as i64 {
                        continue;
                    }
                    let next = (ny as usize) * side + nx as usize;
                    if settled[next] {
                        continue;
                    }
                    let drag = self.drag_in(next);
                    if !drag.is_finite() {
                        continue;
                    }
                    let there = self.lattice.centre_of(next);
                    let candidate = cost + here.distance_to(there).0 * drag;
                    if candidate < best[next] {
                        best[next] = candidate;
                        came_from[next] = cell;
                        heap.push(Step {
                            estimate: candidate + there.distance_to(goal_at).0 * HURRY,
                            cost: candidate,
                            cell: next,
                        });
                    }
                }
            }
        }

        if !settled[goal] {
            return None;
        }

        // A* answers "which way round", and on open ground the answer is
        // "straight". Snapping a straight crossing to cell centres would make a
        // lorry zigzag between hundred-metre squares for no reason, so the
        // direct line wins whenever it is clear and not materially dearer. The
        // slack absorbs the difference between measuring centre-to-centre and
        // measuring door-to-door.
        let straight = from.distance_to(to).0 * self.drag_along(from, to);
        if self.is_clear(from, to) && straight <= best[goal] * 1.1 + self.lattice.cell_size.0 {
            return Some(vec![from, to]);
        }

        let mut cells = vec![goal];
        let mut cursor = goal;
        while came_from[cursor] != usize::MAX {
            cursor = came_from[cursor];
            cells.push(cursor);
        }
        cells.reverse();

        let mut path = vec![from];
        // The first and last cell centres are detours away from the real ends.
        for &cell in &cells[1..cells.len().saturating_sub(1)] {
            path.push(self.lattice.centre_of(cell));
        }
        path.push(to);
        Some(path)
    }
}

/// How hard the search is pushed towards the goal.
///
/// A* with an admissible heuristic — plain distance — is exact, and on soaked
/// ground it is also nearly Dijkstra: the true cost of a cell is distance times
/// drag, so a heuristic that assumes drag of one underestimates threefold and
/// prunes almost nothing. Measured at 745 us for one crossing of a ten-kilometre
/// map, against a freight system that prices three routes per candidate lorry.
///
/// Weighting the heuristic gives that back. The route may be up to this much
/// longer than the true best in the worst case; in practice it is the same route
/// found far sooner, and a lorry taking a slightly wider line round a bog is not
/// a thing anybody can see. Exactness was never the requirement — a plausible
/// way across was.
const HURRY: f64 = 1.6;

/// A* frontier entry: a min-heap on the estimate, then on the cell, both
/// totally ordered so two equally good ways resolve the same way every run.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Step {
    estimate: f64,
    cost: f64,
    cell: usize,
}

impl Eq for Step {}

impl Ord for Step {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .estimate
            .total_cmp(&self.estimate)
            .then_with(|| other.cell.cmp(&self.cell))
    }
}

impl PartialOrd for Step {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// How much worse than open grass a surface is to cross.
///
/// A multiplier on the day's softness rather than a figure of its own, because
/// what varies by place is how badly the ground *takes* water, not how much
/// fell on it. Rock is the useful case: it is hard going and it never turns to
/// mud, so a stony route is the one that stays open in the thaw.
pub fn going(surface: Surface) -> f64 {
    match surface {
        Surface::Grass => 1.0,
        // Roots, stumps and no run-up. Worse than open field when it is wet.
        Surface::Forest => 1.3,
        // Hard on a lorry and hard under it. Softness barely touches it.
        Surface::Rock => 0.35,
        // Not going anywhere.
        Surface::Water => f64::INFINITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::climate::{ClimateId, precipitation_on, temperature_on};
    use crate::rng::Rng;
    use crate::time::DAYS_PER_YEAR;

    /// A year of weather on one climate, from a fixed seed, as
    /// `(day_of_year, temperature, rain, ground)` after each day.
    fn a_year(id: ClimateId, seed: u64, years: u32) -> Vec<(u32, f64, f64, Ground)> {
        let climate = id.def();
        let mut ground = Ground::default();
        let mut out = Vec::new();
        for day in 0..u64::from(DAYS_PER_YEAR) * u64::from(years) {
            let day_of_year = (day % u64::from(DAYS_PER_YEAR)) as u32;
            let mut stream = Rng::from_seed(seed ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let temperature = temperature_on(climate, day_of_year, stream.next_f64());
            let rain = precipitation_on(climate, day_of_year, stream.next_f64());
            ground.advance(temperature, rain);
            out.push((day_of_year, temperature, rain, ground));
        }
        out
    }

    #[test]
    fn rain_is_bursty_but_averages_to_what_was_authored() {
        let climate = ClimateId::Plains.def();
        let mut rng = Rng::from_seed(1961);
        let day = 180; // midsummer
        let mut total = 0.0;
        let mut dry = 0;
        const DAYS: u32 = 20_000;
        for _ in 0..DAYS {
            let fell = precipitation_on(climate, day, rng.next_f64());
            if fell <= 0.0 {
                dry += 1;
            }
            total += fell;
        }
        let mean = total / f64::from(DAYS);
        assert!(
            (mean - climate.rain_on(day)).abs() < 0.05,
            "mean {mean:.3} against the authored {:.3}",
            climate.rain_on(day)
        );
        let dry_share = f64::from(dry) / f64::from(DAYS);
        assert!(
            (dry_share - 0.7).abs() < 0.02,
            "{:.0}% of days were dry",
            dry_share * 100.0
        );
    }

    /// Frozen ground is hard however wet it is. This is the rule the whole
    /// seasonal shape hangs off.
    #[test]
    fn a_frozen_bog_is_a_road() {
        let soaked_and_frozen = Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 100.0,
            frost: 1.0,
        };
        assert_eq!(soaked_and_frozen.softness(), 0.0);
        let soaked_and_thawed = Ground {
            frost: 0.0,
            ..soaked_and_frozen
        };
        assert_eq!(soaked_and_thawed.softness(), 1.0);
    }

    #[test]
    fn dry_ground_is_firm_whatever_the_season() {
        let dry = Ground {
            moisture: 0.0,
            water: 0.0,
            snow: 0.0,
            frost: 0.0,
        };
        assert_eq!(dry.softness(), 0.0);
    }

    /// The seasonal event the model exists to produce, and nothing in the code
    /// mentions spring.
    ///
    /// Over a simulated year the worst going must land in the weeks after the
    /// thaw begins — a winter's snow arriving in the topsoil at once while the
    /// frost that was holding the ground up is still on its way out.
    #[test]
    fn the_worst_going_of_the_year_is_the_spring_thaw() {
        for id in [ClimateId::Plains, ClimateId::Taiga] {
            // Two years, and read the second, so the ground is not still
            // carrying the founding guess.
            let year = a_year(id, 1961, 2);
            let second: Vec<_> = year[DAYS_PER_YEAR as usize..].to_vec();
            let (worst_day, worst) = second
                .iter()
                .map(|&(day, _, _, g)| (day, g.softness()))
                .fold(
                    (0, -1.0),
                    |best, next| if next.1 > best.1 { next } else { best },
                );

            // Months are thirty days here, so March is days 60..90 and May ends
            // at 150. The thaw should be in that window on both postings.
            assert!(
                (60..150).contains(&worst_day),
                "{}: the worst going of the year was day {worst_day} at {worst:.2}",
                id.def().name
            );
            assert!(worst > 0.5, "{}: the thaw was dry", id.def().name);

            // And midwinter must be *better* going than the thaw, which is the
            // counter-intuitive half.
            let midwinter = second
                .iter()
                .filter(|&&(day, _, _, _)| (0..30).contains(&day))
                .map(|&(_, _, _, g)| g.softness())
                .fold(0.0, f64::max);
            assert!(
                midwinter < worst,
                "{}: January ({midwinter:.2}) was worse going than the thaw ({worst:.2})",
                id.def().name
            );
        }
    }

    /// A dry hot posting should not be a mud bath, or the climates are not a
    /// choice about anything.
    #[test]
    fn the_steppe_is_firmer_going_than_the_maritime_coast() {
        let worst = |id: ClimateId| {
            a_year(id, 1961, 2)[DAYS_PER_YEAR as usize..]
                .iter()
                .map(|&(_, _, _, g)| g.softness())
                .fold(0.0, f64::max)
        };
        let mean = |id: ClimateId| {
            let year = a_year(id, 1961, 2);
            let second = &year[DAYS_PER_YEAR as usize..];
            second.iter().map(|&(_, _, _, g)| g.softness()).sum::<f64>() / second.len() as f64
        };
        assert!(
            mean(ClimateId::Steppe) < mean(ClimateId::Maritime),
            "steppe {:.2} against maritime {:.2}",
            mean(ClimateId::Steppe),
            mean(ClimateId::Maritime)
        );
        assert!(worst(ClimateId::Maritime) > 0.5, "the coast never got soft");
    }

    /// Snow has to actually pile up over a winter, or there is nothing to melt.
    #[test]
    fn a_taiga_winter_lays_snow_and_the_spring_takes_it_away() {
        let year = a_year(ClimateId::Taiga, 7, 2);
        let second = &year[DAYS_PER_YEAR as usize..];
        let deepest = second
            .iter()
            .map(|&(_, _, _, g)| g.snow)
            .fold(0.0, f64::max);
        assert!(deepest > 40.0, "only {deepest:.0} mm of snow all winter");
        let midsummer = second
            .iter()
            .find(|&&(day, _, _, _)| day == 190)
            .expect("the year has a midsummer")
            .3;
        assert_eq!(midsummer.snow, 0.0, "snow lying in July");
    }

    #[test]
    fn rock_stays_firm_when_grass_turns_to_mud() {
        let wet = Ground {
            moisture: 1.0,
            water: 1.0,
            snow: 0.0,
            frost: 0.0,
        };
        assert_eq!(wet.going_on(Surface::Grass), 1.0);
        assert!(wet.going_on(Surface::Rock) < 0.5);
        assert!(wet.going_on(Surface::Forest) >= wet.going_on(Surface::Grass));
        assert_eq!(wet.going_on(Surface::Water), 1.0, "water is impassable");
    }

    /// A forecast is the same recurrence rolled forward, and asking for one
    /// moves nothing.
    #[test]
    fn a_forecast_is_what_the_next_days_will_actually_do() {
        let climate = ClimateId::Plains.def();
        let weather = |day: u64| {
            let mut stream = Rng::from_seed(11 ^ day.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let day_of_year = (day % u64::from(DAYS_PER_YEAR)) as u32;
            (
                temperature_on(climate, day_of_year, stream.next_f64()),
                precipitation_on(climate, day_of_year, stream.next_f64()),
            )
        };

        let mut lived = Ground::default();
        for day in 0..10u64 {
            let (t, r) = weather(day + 1);
            lived.advance(t, r);
        }
        let before = Ground::default();
        let forecast = before.forecast(weather, 0, 10);
        assert_eq!(forecast, lived, "the forecast was not what happened");
        assert_eq!(before, Ground::default(), "forecasting changed the ground");
    }
}
