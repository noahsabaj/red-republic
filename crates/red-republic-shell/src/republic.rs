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

use crate::{marshal, views};

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

    /// Run the republic forward whole days, as fast as the machine will.
    ///
    /// For capture and measurement runs that need a republic with roads worn
    /// into it and lorries on the move, rather than one still standing at its
    /// founding. It is `tick` in a loop and nothing else — there is no way to
    /// skip time, only to spend it.
    #[func]
    fn advance_days(&mut self, days: i64) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        for _ in 0..(days.max(0) as u64 * red_republic_sim::time::TICKS_PER_DAY) {
            world.tick();
        }
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

    /// Transforms for one building kind, by its index in the table.
    #[func]
    fn building_transforms_of_kind(&self, kind: i64) -> PackedFloat32Array {
        let (Some(w), Some(def)) = (
            self.world.as_ref(),
            red_republic_sim::building::BUILDINGS.get(kind.max(0) as usize),
        ) else {
            return PackedFloat32Array::new();
        };
        marshal::building_transforms_of_kind(w, def.kind)
    }

    /// A kind's real footprint in metres, which the kit scales its walls to.
    #[func]
    fn building_kind_size(&self, kind: i64) -> Vector2 {
        red_republic_sim::building::BUILDINGS
            .get(kind.max(0) as usize)
            .map_or(Vector2::ZERO, |d| {
                Vector2::new(d.width.0 as f32, d.depth.0 as f32)
            })
    }

    /// How many of a kind are standing. Lets the shell skip a kind entirely
    /// rather than uploading an empty buffer for each of twenty-eight.
    #[func]
    fn building_count_of_kind(&self, kind: i64) -> i64 {
        let (Some(w), Some(def)) = (
            self.world.as_ref(),
            red_republic_sim::building::BUILDINGS.get(kind.max(0) as usize),
        ) else {
            return 0;
        };
        w.buildings().of_kind(def.kind).count() as i64
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

    // ---- Panel reads. See `views`. -----------------------------------------

    /// The resources, in `Resource::ALL` order — the same order `stockpiles`
    /// comes back in, so a table never has to guess which column is which.
    #[func]
    fn resource_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for resource in red_republic_sim::Resource::ALL {
            out.push(&GString::from(resource.name()));
        }
        out
    }

    #[func]
    fn deposits(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::deposits)
    }

    /// The frontier as a polyline, `[x, y, bloc, along]` per sample.
    #[func]
    fn frontier_line(&self, samples: i64) -> PackedFloat32Array {
        match &self.world {
            Some(w) => views::frontier_line(w, samples.clamp(8, 4_096) as usize),
            None => PackedFloat32Array::new(),
        }
    }

    /// The frontier posts, `[x, y, bloc, id]`.
    #[func]
    fn crossings(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::crossings)
    }

    /// Which bloc's frontier is nearest a point: 0 East, 1 West.
    ///
    /// What a customs house built here would trade with, and therefore which
    /// currency this corner of the republic earns.
    #[func]
    fn bloc_near(&self, x: f64, y: f64) -> i64 {
        let Some(w) = &self.world else { return 0 };
        views::bloc_index(w.bloc_near(Point::new(Metres(x), Metres(y)))) as i64
    }

    #[func]
    fn going_field(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::going_field)
    }

    #[func]
    fn wear_field(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::wear_field)
    }

    #[func]
    fn road_sites(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::road_sites)
    }

    #[func]
    fn stockpiles(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::stockpiles)
    }

    /// `[temperature_c, rain_mm]` per day, starting today.
    #[func]
    fn forecast(&self, days: i64) -> PackedFloat32Array {
        match &self.world {
            Some(w) => views::forecast(w, days.max(0) as u64),
            None => PackedFloat32Array::new(),
        }
    }

    #[func]
    fn lattice_cells(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| i64::from(w.lattice().cells()))
    }

    #[func]
    fn lattice_cell_size(&self) -> f64 {
        self.world
            .as_ref()
            .map_or(0.0, |w| w.lattice().cell_size().0)
    }

    /// Ground height at a point, so anything drawn flat on the map sits on the
    /// map rather than on a plane at zero.
    #[func]
    fn ground_height(&self, x: f64, y: f64) -> f64 {
        self.world.as_ref().map_or(0.0, |w| {
            crate::terrain_mesh::height_at(w.terrain(), Point::new(Metres(x), Metres(y)))
        })
    }

    #[func]
    fn going_at(&self, x: f64, y: f64) -> f64 {
        self.world
            .as_ref()
            .map_or(0.0, |w| views::going_at(w, x, y))
    }

    #[func]
    fn distance_to_border(&self, x: f64, y: f64) -> f64 {
        self.world
            .as_ref()
            .map_or(0.0, |w| views::distance_to_border(w, x, y))
    }

    #[func]
    fn precipitation_mm(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| w.precipitation())
    }

    #[func]
    fn temperature_on_day(&self, day: i64) -> f64 {
        self.world
            .as_ref()
            .map_or(0.0, |w| w.temperature_on_day(day.max(0) as u64))
    }

    /// The odds a vehicle sticks setting out on a leg, `0.0..=1.0`.
    ///
    /// The showable half of the bogging model, and most of the explicability a
    /// probability normally costs: a panel can put this in front of the player
    /// *before* they commit a lorry to a crossing. Hiding it would leave the
    /// one deliberately random mechanic in the game feeling arbitrary.
    #[func]
    fn bog_chance(&self, vehicle: i64, leg: i64) -> f64 {
        let Some(w) = &self.world else { return 0.0 };
        let Some(v) = w.fleet().all().get(vehicle.max(0) as usize) else {
            return 0.0;
        };
        w.bog_chance(v.id, leg.max(0) as u32)
    }

    /// What is waiting at the place a vehicle is delivering to: held over
    /// capacity, as a fraction. `-1.0` when it is not carrying to anywhere.
    #[func]
    fn vehicle_destination_fullness(&self, vehicle: i64) -> f64 {
        let Some(w) = &self.world else { return -1.0 };
        let Some(v) = w.fleet().all().get(vehicle.max(0) as usize) else {
            return -1.0;
        };
        let Some((_, to, resource, _)) = v.job.and_then(|j| j.haul()) else {
            return -1.0;
        };
        match w.consignee(to, resource) {
            Some(c) if c.capacity.0 > 0.0 => c.held.0 / c.capacity.0,
            _ => -1.0,
        }
    }

    /// Tenders on the table and running, one line each.
    #[func]
    fn contract_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.contracts().all().len() as i64)
    }

    #[func]
    fn contract_line(&self, index: i64) -> GString {
        let Some(w) = &self.world else {
            return GString::from("");
        };
        let Some(c) = w.contracts().all().get(index.max(0) as usize) else {
            return GString::from("");
        };
        GString::from(
            format!(
                "{:?} · {:.0}/{:.0} t {} · due day {} · {:?}",
                c.market,
                c.delivered.0,
                c.amount.0,
                c.resource.name(),
                c.deadline_day,
                c.state
            )
            .as_str(),
        )
    }

    /// The republic's standing instructions to its customs houses, in the
    /// player's own order — which matters, because the first rule is served
    /// first when throughput or money runs short.
    #[func]
    fn trade_rule_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.trade_policy().rules.len() as i64)
    }

    #[func]
    fn trade_rule_line(&self, index: i64) -> GString {
        let Some(w) = &self.world else {
            return GString::from("");
        };
        let Some(rule) = w.trade_policy().rules.get(index.max(0) as usize) else {
            return GString::from("");
        };
        let what = match rule.action {
            red_republic_sim::TradeAction::Sell => "sell".to_string(),
            red_republic_sim::TradeAction::Buy { up_to } => format!("buy up to {:.0} t", up_to.0),
        };
        GString::from(format!("{} {} · {:?}", what, rule.resource.name(), rule.market).as_str())
    }

    /// What each bloc has advanced and what is still owed, one line each.
    ///
    /// A republic that cannot see its own debts cannot plan around the day they
    /// come due — and a default costs a quarter of what is outstanding plus
    /// relations that price every future trade with that bloc.
    #[func]
    fn loan_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.loans().all().len() as i64)
    }

    #[func]
    fn loan_line(&self, index: i64) -> GString {
        let Some(w) = &self.world else {
            return GString::from("");
        };
        let Some(loan) = w.loans().all().get(index.max(0) as usize) else {
            return GString::from("");
        };
        let today = w.clock().day_index();
        GString::from(
            format!(
                "{:?}: {:.0} of {:.0} owed · {} days",
                loan.market,
                loan.outstanding(),
                loan.owed,
                loan.days_left(today),
            )
            .as_str(),
        )
    }

    /// How much a bloc is still owed. Zero when nothing is outstanding.
    #[func]
    fn owed_to(&self, market_index: i64) -> f64 {
        let Some(w) = &self.world else { return 0.0 };
        let market = if market_index == 0 {
            red_republic_sim::Market::East
        } else {
            red_republic_sim::Market::West
        };
        w.loans().outstanding(market)
    }

    /// The republic's record: advances cleared, and advances defaulted on.
    #[func]
    fn loans_cleared(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| i64::from(w.loans().cleared))
    }

    #[func]
    fn loans_defaulted(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| i64::from(w.loans().defaulted))
    }

    /// Everything the player has done, in order. A republic that can show its
    /// own history is one whose save can be replayed and whose bug report is
    /// reproducible.
    #[func]
    fn journal_len(&self) -> i64 {
        self.world.as_ref().map_or(0, |w| w.journal().len() as i64)
    }

    #[func]
    fn journal_line(&self, index: i64) -> GString {
        let Some(w) = &self.world else {
            return GString::from("");
        };
        let Some(entry) = w.journal().entries().get(index.max(0) as usize) else {
            return GString::from("");
        };
        GString::from(format!("t{} {:?}", entry.tick, entry.command).as_str())
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
