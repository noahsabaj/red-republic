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

    /// The settlers Moscow sends with a posting.
    ///
    /// Read from the simulation rather than copied into GDScript. The shell held
    /// its own number once and the two drifted apart, which is how a founding
    /// ended up with more jobs than people and a customs house nobody worked.
    #[func]
    fn founding_settlers(&self) -> i64 {
        red_republic_sim::scenario::SETTLERS as i64
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

    /// Every building crew that is out: `[x, y, heads, state, office]`, with
    /// state 0 riding, 1 working, 2 waiting for a lift.
    #[func]
    fn crew_parties(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::crew_parties)
    }

    /// Who the republic is made of: `[infants, pupils, students, workers,
    /// retired, unschooled, schooled, graduates]`.
    #[func]
    fn demographics(&self) -> PackedInt32Array {
        self.world
            .as_ref()
            .map_or_else(PackedInt32Array::new, views::demographics)
    }

    /// How the republic is treating its people: `[provisions, warmth, health,
    /// culture, schooling, work, overall]`, each `0.0..=1.0`.
    #[func]
    fn contentment(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::contentment)
    }

    /// The names of those components, in the same order.
    #[func]
    fn contentment_names(&self) -> PackedStringArray {
        views::contentment_names()
    }

    /// Mean health and mean loyalty, `[health, loyalty]`.
    #[func]
    fn wellbeing(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::wellbeing)
    }

    /// People coming and going: `[waiting, groups, settled, left, gave_up]`.
    #[func]
    fn migration_totals(&self) -> PackedInt32Array {
        self.world
            .as_ref()
            .map_or_else(PackedInt32Array::new, views::migration_totals)
    }

    /// Settlers standing at the frontier: `[x, y, heads, days_waited]`.
    #[func]
    fn newcomers(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::newcomers)
    }

    /// One home's contentment, component by component, then its overall — and
    /// then the index of the component costing it most, or `-1` if nothing is.
    ///
    /// What a panel prints when the player asks why an estate is unhappy. The
    /// worst component is the simulation's own answer rather than the shell
    /// working it out, because a weighted loss is balance and balance does not
    /// belong in a panel.
    #[func]
    fn home_contentment(&self, building: i64) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        let Some(w) = &self.world else { return out };
        let id = red_republic_sim::BuildingId(building.max(0) as u32);
        let Some(b) = w.buildings().get(id) else {
            return out;
        };
        for part in b.content.parts() {
            out.push(part as f32);
        }
        out.push(b.content.overall() as f32);
        let worst = b.content.worst().and_then(|name| {
            red_republic_sim::Contentment::NAMES
                .iter()
                .position(|n| *n == name)
        });
        out.push(worst.map_or(-1.0, |i| i as f32));
        out
    }

    /// How many builders are standing on a site, and how many its office still
    /// has to send. What a site panel needs to answer the only two questions
    /// worth asking of a half-built thing: is anyone on it, and if not, why not.
    #[func]
    fn site_crew(&self, building: i64) -> i64 {
        let Some(w) = &self.world else { return 0 };
        let id = red_republic_sim::BuildingId(building.max(0) as u32);
        i64::from(
            w.crews()
                .at_site(red_republic_sim::Destination::Building(id)),
        )
    }

    /// Builders an office has spare — its staff less everyone already out.
    #[func]
    fn office_spare(&self, office: i64) -> i64 {
        let Some(w) = &self.world else { return 0 };
        let id = red_republic_sim::BuildingId(office.max(0) as u32);
        let Some(b) = w.buildings().get(id) else {
            return 0;
        };
        i64::from(b.staff.saturating_sub(w.crews().posted(id)))
    }

    /// Call a crew off a site. Empty string on success, or the reason.
    ///
    /// The player's half of the mechanic: a site whose materials never arrive
    /// would otherwise hold its gang for ever, and deciding when a plan has gone
    /// wrong is not a judgment the simulation should be making.
    #[func]
    fn recall_crew(&mut self, building: i64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let id = red_republic_sim::BuildingId(building.max(0) as u32);
        match world.issue(Command::RecallCrew {
            site: red_republic_sim::Destination::Building(id),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// The republic's import policy: which frontier post sites buy through.
    ///
    /// `0` means the republic imports nothing, which is the default — auto-import
    /// spends hard currency, so it stays off until somebody names a post.
    #[func]
    fn import_post(&self) -> i64 {
        self.world
            .as_ref()
            .and_then(|w| w.build_policy().global())
            .map_or(0, |c| i64::from(c.0))
    }

    /// The post a single site buys through, `0` for none. Sites follow the
    /// republic's policy unless they have been given one of their own.
    #[func]
    fn site_import_post(&self, building: i64) -> i64 {
        let Some(w) = &self.world else { return 0 };
        let site = red_republic_sim::Destination::Building(red_republic_sim::BuildingId(
            building.max(0) as u32,
        ));
        w.build_policy()
            .crossing_for(site)
            .map_or(0, |c| i64::from(c.0))
    }

    /// Whether this site has an instruction of its own rather than following
    /// the republic's. What greys the "same as the republic" control.
    #[func]
    fn site_has_own_import_policy(&self, building: i64) -> bool {
        let Some(w) = &self.world else { return false };
        let site = red_republic_sim::Destination::Building(red_republic_sim::BuildingId(
            building.max(0) as u32,
        ));
        w.build_policy().is_overridden(site)
    }

    /// How much of a site's bill has already been bought abroad on its account.
    ///
    /// The Directorate buys a bill once. A site that is short *and* has spent
    /// its allowance is a site whose materials were taken somewhere else, which
    /// is a completely different problem from one that has not been bought for
    /// yet — and without this the two look identical.
    #[func]
    fn site_bought_abroad(&self, building: i64, resource: i64) -> f64 {
        let Some(w) = &self.world else { return 0.0 };
        let Some(&resource) = red_republic_sim::Resource::ALL.get(resource.max(0) as usize) else {
            return 0.0;
        };
        let site = red_republic_sim::Destination::Building(red_republic_sim::BuildingId(
            building.max(0) as u32,
        ));
        w.build_policy().bought_for(site, resource).0
    }

    /// Set where sites import through. `building` of 0 sets the republic's
    /// default; `crossing` of 0 means import nothing.
    #[func]
    fn set_import_post(&mut self, building: i64, crossing: i64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let site = (building > 0).then_some(red_republic_sim::Destination::Building(
            red_republic_sim::BuildingId(building.max(0) as u32),
        ));
        let crossing =
            (crossing > 0).then_some(red_republic_sim::CrossingId(crossing.max(0) as u32));
        match world.issue(Command::SetImportPolicy { site, crossing }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// Put a site back under the republic's default policy.
    #[func]
    fn clear_import_post(&mut self, building: i64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let site = red_republic_sim::Destination::Building(red_republic_sim::BuildingId(
            building.max(0) as u32,
        ));
        match world.issue(Command::ClearImportPolicy { site }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// Foreign builders the republic employs, by bloc: `[east, west]`.
    ///
    /// A standing hard-currency cost that nothing else on the panel would show:
    /// domestic labour is free in money and this is not, so a republic with a
    /// wage bill needs to be able to see it beside the treasury it comes out of.
    #[func]
    fn hired_builders(&self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        let Some(w) = &self.world else { return out };
        for market in red_republic_sim::Market::ALL {
            out.push(w.crews().hired_from_bloc(market) as i32);
        }
        out
    }

    /// What one foreign builder costs: `[placement_fee, daily_wage]`.
    #[func]
    fn hiring_terms(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        out.push(red_republic_sim::crews::HIRING_FEE as f32);
        out.push(red_republic_sim::crews::FOREIGN_WAGE as f32);
        out
    }

    /// Foreign builders on one office's books.
    #[func]
    fn office_hired(&self, office: i64) -> i64 {
        let Some(w) = &self.world else { return 0 };
        i64::from(
            w.crews()
                .hired_total(red_republic_sim::BuildingId(office.max(0) as u32)),
        )
    }

    /// Hire builders from a bloc for an office. `market` is 0 East, 1 West.
    ///
    /// Empty string on success, or the reason — which carries the fee against
    /// what the treasury holds, because a refusal a player cannot act on is
    /// the failure the whole refusal type exists to avoid.
    #[func]
    fn hire_foreign(&mut self, market: i64, office: i64, heads: i64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let market = if market == 0 {
            red_republic_sim::Market::East
        } else {
            red_republic_sim::Market::West
        };
        match world.issue(Command::HireForeign {
            market,
            office: red_republic_sim::BuildingId(office.max(0) as u32),
            heads: heads.max(0) as u32,
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
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
