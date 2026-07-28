//! The node that owns a republic.
//!
//! Godot calls `tick()` and reads. It never touches simulation state — it
//! cannot, because everything below `World` is `pub(crate)` in the other crate
//! and this one enables no features. The two verbs are advance time and issue a
//! command, exactly as they are for any other consumer.
//!
//! # Six speeds, and one of them is the point
//!
//! 0 is paused. **1 is real-time: one real second is one in-game second**, the
//! hook this whole design is built around. Then 1 real second buys 1, 2, 4 or 8
//! in-game hours.
//!
//! A tick is sixty simulated seconds, so real-time advances the simulation once
//! a minute and the fastest setting runs 480 ticks a second. Between ticks
//! nothing is interpolated except position, and position is interpolated
//! *exactly* rather than approximately: `Journey::position_at` is a pure
//! function of `(plan, time)` at any fractional tick, so every speed draws the
//! same world and `a_journey_is_the_same_wherever_you_sample_it` proves it.

use godot::classes::{INode3D, Node3D};
use godot::prelude::*;
use red_republic_sim::climate::ClimateId;
use red_republic_sim::time::TICK;
use red_republic_sim::units::Point;
use red_republic_sim::{BuildingKind, Command, Metres, World, WorldSpec};

use crate::marshal;

/// In-game seconds bought by one real second, per speed setting.
///
/// Index 1 is the thesis: one for one. The jump from index 1 to index 2 is
/// deliberately enormous (a second to an hour) because there is nothing useful
/// between them — you are either watching a lorry cross a field or you are
/// getting through a week.
const SPEEDS: [f64; 6] = [0.0, 1.0, 3_600.0, 7_200.0, 14_400.0, 28_800.0];

/// The most ticks one frame may run, whatever the frame took.
///
/// Without a cap a slow frame asks for more simulation, which makes the next
/// frame slower, which asks for more still. The republic falls behind real time
/// instead of the window locking up, which is the right way to lose.
const MAX_TICKS_PER_FRAME: u32 = 2_000;

#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct Republic {
    world: Option<World>,
    /// Where the founding put the town.
    ///
    /// Kept because it is not recoverable from the world afterwards and the
    /// camera needs it: the site is chosen from the geology — the shallowest
    /// coal body — so a posting is routinely nowhere near the middle of the
    /// map, and opening on the map's centre means opening on empty ground.
    centre: Point,
    speed: usize,
    /// How far into the current tick real time has carried, in `0.0..1.0`.
    /// Added to the whole tick count to give the fractional `now` that position
    /// sampling wants.
    fraction: f64,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Republic {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            world: None,
            centre: Point::new(Metres(0.0), Metres(0.0)),
            // Founded paused. A republic that starts running before anyone has
            // looked at it is a republic whose first winter arrives during the
            // loading screen.
            speed: 0,
            fraction: 0.0,
            base,
        }
    }

    fn process(&mut self, delta: f64) {
        let per_second = SPEEDS[self.speed];
        if per_second <= 0.0 {
            return;
        }
        let Some(world) = self.world.as_mut() else {
            return;
        };

        self.fraction += delta * per_second / TICK.0;
        let mut whole = self.fraction.floor();
        if whole >= f64::from(MAX_TICKS_PER_FRAME) {
            whole = f64::from(MAX_TICKS_PER_FRAME);
            // Drop the backlog rather than carrying it: falling behind real
            // time is recoverable, and a debt that compounds is not.
            self.fraction = 0.0;
        } else {
            self.fraction -= whole;
        }
        for _ in 0..(whole as u32) {
            world.tick();
        }
    }
}

#[godot_api]
impl Republic {
    /// Found a republic and place its starting base.
    ///
    /// `climate` indexes `ClimateId::ALL`: 0 plains, 1 taiga, 2 steppe,
    /// 3 maritime.
    #[func]
    fn found(&mut self, seed: i64, extent_m: f64, climate: i64, settlers: i64) {
        let climate = *ClimateId::ALL
            .get(climate.clamp(0, ClimateId::ALL.len() as i64 - 1) as usize)
            .unwrap_or(&ClimateId::Plains);
        let mut world = World::new(WorldSpec {
            seed: seed as u64,
            extent: Metres(extent_m),
            climate,
        });
        let base = red_republic_sim::scenario::found(&mut world, settlers.max(0) as usize);
        self.centre = base.centre;
        self.world = Some(world);
        self.fraction = 0.0;
    }

    /// Whether a republic has been founded yet.
    #[func]
    fn is_founded(&self) -> bool {
        self.world.is_some()
    }

    #[func]
    fn set_speed(&mut self, speed: i64) {
        self.speed = speed.clamp(0, SPEEDS.len() as i64 - 1) as usize;
    }

    #[func]
    fn speed(&self) -> i64 {
        self.speed as i64
    }

    /// The fractional tick to sample positions at.
    ///
    /// Whole ticks plus how far real time has carried into the next one. This
    /// is what makes a lorry move smoothly at 60 fps while the simulation
    /// advances once a minute at real-time speed.
    #[func]
    fn now(&self) -> f64 {
        match &self.world {
            Some(w) => w.clock().ticks() as f64 + self.fraction,
            None => 0.0,
        }
    }

    // ---- Bulk reads. Packed arrays only; see `marshal`. --------------------

    #[func]
    fn building_transforms(&self) -> PackedFloat32Array {
        match &self.world {
            Some(w) => marshal::building_transforms(w),
            None => PackedFloat32Array::new(),
        }
    }

    #[func]
    fn vehicle_positions(&self) -> PackedFloat32Array {
        match &self.world {
            Some(w) => marshal::vehicle_positions(w, self.now()),
            None => PackedFloat32Array::new(),
        }
    }

    #[func]
    fn road_segments(&self) -> PackedFloat32Array {
        match &self.world {
            Some(w) => marshal::road_segments(w),
            None => PackedFloat32Array::new(),
        }
    }

    /// The terrain as a Godot surface array, ready for `ArrayMesh`.
    #[func]
    fn terrain_surface(&self) -> VarArray {
        match &self.world {
            Some(w) => crate::terrain_mesh::surface(w.terrain()),
            None => VarArray::new(),
        }
    }

    // ---- Small reads. A raw call is 0.21 µs, so these are free. ------------

    /// Where the town was founded, in metres. What the camera opens on.
    #[func]
    fn centre_x(&self) -> f64 {
        self.centre.x.0
    }

    #[func]
    fn centre_y(&self) -> f64 {
        self.centre.y.0
    }

    #[func]
    fn map_extent(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| w.terrain().extent().0)
    }

    #[func]
    fn date_text(&self) -> GString {
        let Some(w) = &self.world else {
            return GString::from("");
        };
        let d = w.clock().date();
        GString::from(format!("{:04}-{:02}-{:02}", d.year, d.month, d.day).as_str())
    }

    #[func]
    fn population(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.population().count() as i64)
    }

    #[func]
    fn employed(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.population().employed() as i64)
    }

    #[func]
    fn building_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.buildings().all().len() as i64)
    }

    #[func]
    fn vehicle_count(&self) -> i64 {
        self.world.as_ref().map_or(0, |w| w.fleet().len() as i64)
    }

    #[func]
    fn rubles(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| w.treasury().rubles)
    }

    #[func]
    fn dollars(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| w.treasury().dollars)
    }

    #[func]
    fn temperature_c(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| w.temperature())
    }

    // ---- The one write. ----------------------------------------------------

    /// Commission a building. Returns an empty string on success, or the
    /// reason it was refused — a sentence, meant to be shown.
    #[func]
    fn place(&mut self, kind: i64, x: f64, y: f64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let Some(&kind) = red_republic_sim::building::BUILDINGS
            .get(kind.max(0) as usize)
            .map(|d| &d.kind)
        else {
            return GString::from("no such building");
        };
        match world.issue(Command::Place {
            kind,
            at: Point::new(Metres(x), Metres(y)),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// Whether a building of this kind could go here — the placement preview.
    ///
    /// Asks exactly the question [`Republic::place`] asks, because
    /// `World::can_place` is the same rule the commit uses. A preview that asks
    /// a different question is a ghost that renders green over ground the
    /// placement will refuse.
    #[func]
    fn can_place(&self, kind: i64, x: f64, y: f64) -> GString {
        let Some(world) = &self.world else {
            return GString::from("no republic has been founded");
        };
        let Some(&kind) = red_republic_sim::building::BUILDINGS
            .get(kind.max(0) as usize)
            .map(|d| &d.kind)
        else {
            return GString::from("no such building");
        };
        match world.can_place(kind, Point::new(Metres(x), Metres(y))) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// How many building kinds there are, and what each is called. Small reads,
    /// so the build menu can ask once at load.
    #[func]
    fn building_kind_count(&self) -> i64 {
        red_republic_sim::building::BUILDINGS.len() as i64
    }

    #[func]
    fn building_kind_name(&self, index: i64) -> GString {
        red_republic_sim::building::BUILDINGS
            .get(index.max(0) as usize)
            .map_or_else(|| GString::from(""), |d| GString::from(d.name))
    }

    /// Suppresses the unused-field warning on `base` while nothing needs it.
    #[allow(dead_code)]
    fn base_node(&self) -> &Base<Node3D> {
        &self.base
    }

    /// The kind at an index, for callers that want to hold one.
    #[allow(dead_code)]
    fn kind_at(index: usize) -> Option<BuildingKind> {
        red_republic_sim::building::BUILDINGS
            .get(index)
            .map(|d| d.kind)
    }
}
