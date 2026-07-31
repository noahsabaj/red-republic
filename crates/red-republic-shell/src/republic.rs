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
use red_republic_sim::founding::{Shelf, ShelfFilter};
use red_republic_sim::time::TICK;
use red_republic_sim::units::Point;
use red_republic_sim::{BuildingKind, Command, Metres, World, WorldSpec};

use crate::{marshal, shelf, views};

/// In-game seconds bought by one real second, per speed setting.
///
/// Index 1 is the thesis: one for one. The jump from index 1 to index 2 is
/// deliberately enormous (a second to an hour) because there is nothing useful
/// between them — you are either watching a lorry cross a field or you are
/// getting through a week.
const SPEEDS: [f64; 6] = [0.0, 1.0, 3_600.0, 7_200.0, 14_400.0, 28_800.0];

/// A climate index into `ClimateId::ALL`, clamped rather than trusted.
///
/// Every index arriving from GDScript is clamped at this boundary. An out-of-range
/// climate from a stale scene would otherwise pick whatever `unwrap_or` chose,
/// which is a silently wrong posting rather than a visible error.
fn climate_at(index: i64) -> ClimateId {
    *ClimateId::ALL
        .get(index.clamp(0, ClimateId::ALL.len() as i64 - 1) as usize)
        .unwrap_or(&ClimateId::Plains)
}

/// A bloc index from GDScript, clamped rather than trusted: 0 East, 1 West.
fn market_at(index: i64) -> red_republic_sim::trade::Market {
    if index <= 0 {
        red_republic_sim::trade::Market::East
    } else {
        red_republic_sim::trade::Market::West
    }
}

/// A shelf filter from the two indices the founding screen holds.
fn filter(size: i64, climate: i64) -> ShelfFilter {
    let sizes = red_republic_sim::founding::SIZES;
    let extent = sizes
        .get(size.clamp(0, sizes.len() as i64 - 1) as usize)
        .map_or(sizes[1].1, |(_, extent)| *extent);
    ShelfFilter {
        extent,
        climate: climate_at(climate),
    }
}

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
    /// The candidate postings, while the founding screen is open.
    ///
    /// Held here rather than in a node of its own so there is no handing a
    /// `World` between two Godot objects. It is dropped the moment a posting is
    /// taken, which is what makes its memory transient — 81 MB at the largest
    /// size, measured, and the reason [`Republic::close_shelf`] exists as
    /// something the menu calls rather than something a destructor eventually
    /// does.
    shelf: Option<Shelf>,
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
            shelf: None,
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
    fn found(&mut self, seed: i64, extent_m: f64, climate: i64) {
        let climate = *ClimateId::ALL
            .get(climate.clamp(0, ClimateId::ALL.len() as i64 - 1) as usize)
            .unwrap_or(&ClimateId::Plains);
        let mut world = World::new(WorldSpec {
            seed: seed as u64,
            extent: Metres(extent_m),
            climate,
        });
        // The map is empty. `found` picks the site and opens the rouble grant;
        // every building after that is the player's to contract or to build.
        self.centre = red_republic_sim::scenario::found(&mut world);
        self.world = Some(world);
        self.fraction = 0.0;
    }

    /// Stand the fixture town up on top of a founded republic — **for capture
    /// runs only**.
    ///
    /// A blank map is what a player is given and it is also a republic with
    /// nothing in it, so `--screen labour` on one renders a heading and an empty
    /// table. That capture would pass while checking nothing: the layout bugs
    /// this repository has actually shipped were all in rows, and a screen with
    /// no rows has none to get wrong.
    ///
    /// `scenario::town` is the nineteen-building hand the founding used to
    /// deal, kept as the fixture forty-odd simulation tests measure against.
    /// Using it here is the same use — something to look at — and it is reached
    /// only from `--screen`, never from a menu.
    #[func]
    fn found_fixture_town(&mut self, citizens: i64) {
        if let Some(world) = self.world.as_mut() {
            let base = red_republic_sim::scenario::town(world, citizens.max(0) as usize);
            // A lit street through the middle of it, opened rather than ordered.
            // **Here so that every capture of the fixture renders lamp
            // geometry**, which is the only way a winding or culling mistake in
            // it gets caught — this project has shipped two of those and both
            // were invisible to every number. Paved lit road is four buildings
            // deep in a chain the fixture has not built, so waiting for the crew
            // would mean no capture ever saw one.
            world.lay_demo_street(base.centre);
        }
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

    /// Run forward to a given hour of the day, going round midnight if need be.
    ///
    /// **A capture flag needs to be able to reach the state it captures.** The
    /// day/night work landed with no way to render anything but the hour a run
    /// happened to found at, which is the same shape as the sky work shipping
    /// with the pitch pinned at fifty degrees: five screenshots taken at five
    /// hours came back identical and would have been read as five hours looking
    /// the same.
    ///
    /// Time is spent rather than skipped, exactly as [`Republic::advance_days`]
    /// spends it. There is no way in this game to make time pass without it
    /// passing.
    #[func]
    fn advance_to_hour(&mut self, hour: f64) {
        let per_day = red_republic_sim::time::TICKS_PER_DAY;
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let wanted = ((hour.clamp(0.0, 24.0) / 24.0) * per_day as f64) as u64 % per_day;
        let now = world.clock().ticks() % per_day;
        let ticks = (wanted + per_day - now) % per_day;
        for _ in 0..ticks {
            world.tick();
        }
    }

    /// The grant a posting opens with, in roubles.
    ///
    /// Read from the simulation rather than copied into GDScript, for the reason
    /// the settler count used to be: the shell held its own number once and the
    /// two drifted apart. This replaces `founding_settlers`, which described a
    /// hand of buildings and people that a republic is no longer given.
    #[func]
    fn founding_grant(&self) -> f64 {
        red_republic_sim::scenario::GRANT_ROUBLES
    }

    /// Whether a republic has been founded yet.
    #[func]
    fn is_founded(&self) -> bool {
        self.world.is_some()
    }

    // ---- Founding: choosing your land ------------------------------------
    //
    // Two beats. `derive_shelf` and the reads under it are the first — a shelf
    // of candidate postings with the few facts that decide a start. `take_posting`
    // is the second, and it is the one that needs a name.

    /// Generate a shelf of candidate postings from one master seed.
    ///
    /// `size` indexes `founding::SIZES` and `climate` indexes `ClimateId::ALL`.
    /// **This blocks**, and the figures are measured: 219 ms at 6 km, 611 ms at
    /// 10 km, 1,630 ms at 16 km. The caller shows a loading state at every size
    /// — the number that used to be recorded for the default size was the one for
    /// the smallest, and the correction is why this note is here rather than a
    /// vague warning.
    #[func]
    fn derive_shelf(&mut self, master_seed: i64, size: i64, climate: i64) {
        self.shelf = Some(Shelf::derive(master_seed as u64, filter(size, climate)));
    }

    /// Re-derive the shelf under a new filter, keeping the same candidate seeds.
    ///
    /// The design decision this exists to hold: a filter **transforms** the
    /// shelf rather than dealing a fresh hand, so flipping to taiga shows what
    /// that does to land the player was already considering. Candidate `n` stays
    /// candidate `n`, which is what makes comparing them across a filter change
    /// mean anything.
    #[func]
    fn refilter_shelf(&mut self, size: i64, climate: i64) {
        if let Some(current) = self.shelf.as_ref() {
            self.shelf = Some(current.refilter(filter(size, climate)));
        }
    }

    /// Let the candidate worlds go.
    ///
    /// Called when the founding screen closes, whether a posting was taken or
    /// the player went back. A shelf of six 16 km maps is 81 MB and there is no
    /// reason to hold it while a republic is being played.
    #[func]
    fn close_shelf(&mut self) {
        self.shelf = None;
    }

    #[func]
    fn shelf_size(&self) -> i64 {
        self.shelf.as_ref().map_or(0, |s| s.candidates.len() as i64)
    }

    /// Every candidate's decisive facts. See `shelf::CARD_STRIDE` for the layout.
    #[func]
    fn shelf_cards(&self) -> PackedFloat32Array {
        self.shelf
            .as_ref()
            .map_or_else(PackedFloat32Array::new, shelf::cards)
    }

    /// One candidate's land as an RGB8 raster, `size` by `size`.
    #[func]
    fn shelf_minimap(&self, index: i64, size: i64) -> PackedByteArray {
        let Some(candidate) = self
            .shelf
            .as_ref()
            .and_then(|s| s.get(index.max(0) as usize))
        else {
            return PackedByteArray::new();
        };
        shelf::minimap(candidate.world.terrain(), size.max(0) as u32)
    }

    /// A candidate's seed, which is the land's identity.
    ///
    /// The one thing a player might write down and share, so it is shown rather
    /// than hidden — and it is the reason `World::seed` is exempt from the
    /// exposure guard as *map identity* rather than republic state.
    #[func]
    fn candidate_seed(&self, index: i64) -> i64 {
        self.shelf
            .as_ref()
            .and_then(|s| s.get(index.max(0) as usize))
            .map_or(0, |c| c.spec.seed as i64)
    }

    /// Take a posting: found the chosen candidate and name it.
    ///
    /// Empty string on success, or the reason it was refused — which for a
    /// blank or overlong name is a sentence the founding screen prints under the
    /// name box. A republic is never named for the player, because a default
    /// would be a placeholder string reaching the title bar and the save list.
    ///
    /// The two halves are deliberately one call. A founded but unnamed republic
    /// is a state nothing should be able to observe: the naming would be a second
    /// step that could be skipped, and every screen downstream would need a
    /// branch for a republic with no name.
    #[func]
    fn take_posting(&mut self, index: i64, name: GString) -> GString {
        let Some(shelf) = self.shelf.as_ref() else {
            return GString::from("no shelf of postings has been generated");
        };
        let Some(mut world) = shelf.found(index.max(0) as usize) else {
            return GString::from("that posting is not on the shelf");
        };
        if let Err(why) = world.issue(Command::NameRepublic {
            name: name.to_string(),
        }) {
            // Refused before anything is kept, so a rejected name leaves the
            // founding screen exactly as it was rather than half-founded.
            return GString::from(why.to_string().as_str());
        }
        self.centre = red_republic_sim::scenario::found(&mut world);
        self.world = Some(world);
        self.fraction = 0.0;
        self.speed = 0;
        self.shelf = None;
        GString::from("")
    }

    /// What the player called this republic.
    #[func]
    fn republic_name(&self) -> GString {
        self.world
            .as_ref()
            .map_or_else(|| GString::from(""), |w| GString::from(w.name()))
    }

    /// The longest name that will be accepted, so the name box can say so
    /// before the player is refused rather than after.
    #[func]
    fn name_limit(&self) -> i64 {
        red_republic_sim::world::NAME_LIMIT as i64
    }

    /// Rename the republic. Empty string on success, or the reason.
    #[func]
    fn rename(&mut self, name: GString) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        match world.issue(Command::NameRepublic {
            name: name.to_string(),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// One climate's authored year: twelve mean temperatures, then twelve
    /// rainfall figures. What the posting briefing is made of.
    #[func]
    fn climate_year(&self, climate: i64) -> PackedFloat32Array {
        shelf::climate_year(climate_at(climate))
    }

    #[func]
    fn climate_names(&self) -> PackedStringArray {
        shelf::climate_names()
    }

    #[func]
    fn size_names(&self) -> PackedStringArray {
        shelf::size_names()
    }

    #[func]
    fn size_extents(&self) -> PackedFloat32Array {
        shelf::size_extents()
    }

    /// The climate this republic was posted to, as an index into
    /// `ClimateId::ALL`. Fixed at founding — you do not get a milder winter by
    /// asking for one.
    #[func]
    fn climate(&self) -> i64 {
        let Some(w) = &self.world else { return 0 };
        ClimateId::ALL
            .iter()
            .position(|&c| c == w.climate())
            .unwrap_or(0) as i64
    }

    // ---- Saving and loading ------------------------------------------------

    /// Write the republic to a file. Empty string on success, or the reason.
    ///
    /// The bytes are the simulation crate's own format and the shell adds
    /// nothing to them — no header, no sidecar. That is deliberate: a save whose
    /// name lived beside it in a second file is a save that can be renamed apart
    /// from the republic it holds, and the format's requirement of bit-exact
    /// `f64` round-tripping is not something a caller may opt out of.
    #[func]
    fn save_to(&self, path: GString) -> GString {
        let Some(world) = self.world.as_ref() else {
            return GString::from("no republic has been founded");
        };
        // Written to a neighbouring file and then moved into place. A save
        // interrupted halfway is the one failure that costs a player a republic
        // rather than a minute, and a partial write over the previous save
        // destroys the thing it was meant to replace.
        let path = path.to_string();
        let temporary = format!("{path}.part");
        if let Err(why) = std::fs::write(&temporary, world.to_bytes()) {
            return GString::from(format!("could not write the save: {why}").as_str());
        }
        match std::fs::rename(&temporary, &path) {
            Ok(()) => GString::from(""),
            Err(why) => {
                let _ = std::fs::remove_file(&temporary);
                GString::from(format!("could not replace the save: {why}").as_str())
            }
        }
    }

    /// Load a republic from a file. Empty string on success, or the reason.
    ///
    /// The world is only put in place once it has parsed, so a failed load
    /// leaves whatever was being played untouched. Reading a save into a
    /// half-state and then reporting an error would lose a republic to a typo.
    #[func]
    fn load_from(&mut self, path: GString) -> GString {
        let bytes = match std::fs::read(path.to_string()) {
            Ok(bytes) => bytes,
            Err(why) => return GString::from(format!("could not read the save: {why}").as_str()),
        };
        match World::from_bytes(&bytes) {
            Ok(world) => {
                // The founding centre is not in the save and does not need to
                // be: it was only ever where the camera should open, and a
                // republic being resumed has buildings to aim at instead.
                self.centre = marshal::centre_of(&world);
                self.world = Some(world);
                self.fraction = 0.0;
                self.speed = 0;
                self.shelf = None;
                GString::from("")
            }
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// What a save says about itself, without opening it.
    ///
    /// `[name, date, days, population, climate, extent_km]` as strings, or a
    /// single-entry array holding the reason it could not be read. Empty when the
    /// file is not there at all, which is not an error worth a sentence — a load
    /// screen simply does not list it.
    ///
    /// This is the read that lets the load screen exist. Listing saves by loading
    /// them does not scale: a republic's map is megabytes, and `postcard` is
    /// positional, so there is no reaching a field near the end of `World`
    /// without decoding the terrain on the way past.
    #[func]
    fn save_preview(&self, path: GString) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let Ok(bytes) = std::fs::read(path.to_string()) else {
            return out;
        };
        match World::preview_from_bytes(&bytes) {
            Ok(preview) => {
                let (year, month, day) = preview.date;
                out.push(&GString::from(preview.name.as_str()));
                out.push(&GString::from(
                    format!("{year:04}-{month:02}-{day:02}").as_str(),
                ));
                out.push(&GString::from(preview.day.to_string().as_str()));
                out.push(&GString::from(preview.population.to_string().as_str()));
                out.push(&GString::from(
                    ClimateId::ALL
                        .iter()
                        .position(|&c| c == preview.climate)
                        .unwrap_or(0)
                        .to_string()
                        .as_str(),
                ));
                out.push(&GString::from(
                    format!("{:.0}", preview.extent_m / 1_000.0).as_str(),
                ));
            }
            Err(why) => out.push(&GString::from(why.to_string().as_str())),
        }
        out
    }

    /// The format version this build writes, so a save file name can carry it
    /// and a player can see why an old one will not open.
    #[func]
    fn save_version(&self) -> i64 {
        i64::from(red_republic_sim::world::SAVE_VERSION)
    }

    /// The reference, as marked lines. See [`crate::reference`] for the markers.
    ///
    /// Generated from the authored tables every time it is asked for, rather than
    /// cached: it is a few hundred lines built once when a screen opens, and a
    /// cache would be a copy that can be stale — which is the one thing this
    /// document exists not to be.
    #[func]
    fn reference(&self) -> PackedStringArray {
        crate::reference::document_for_godot()
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

    /// How far through the day it is, `0.0` midnight to `1.0` midnight.
    ///
    /// **Fractional, not whole ticks**, for the same reason [`Republic::now`]
    /// is: at real-time speed the simulation advances once a minute, and a sun
    /// that jumped once a minute would read as a strobe rather than as a day.
    /// The renderer interpolates; the simulation does not care what o'clock it
    /// is, because everything it decides about the dark it decides from a
    /// roster.
    #[func]
    fn time_of_day(&self) -> f64 {
        let ticks = red_republic_sim::time::TICKS_PER_DAY as f64;
        (self.now() % ticks) / ticks
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

    /// Where one species of tree stands, as a `MultiMesh` instance buffer.
    ///
    /// Sixteen floats an instance — a 3x4 transform then an RGBA colour — which
    /// is exactly the layout `MultiMesh.buffer` takes, so the Godot side is one
    /// assignment with no per-tree work. `species` and `species_count` partition
    /// the same set of sites between the meshes, so three calls plant one wood
    /// rather than three overlapping ones.
    ///
    /// Forest was a colour on the ground until this existed. The first attempt
    /// scattered in GDScript by walking the finished mesh's 361,201 vertices and
    /// hung a render for eight minutes.
    #[func]
    fn forest_buffer(
        &self,
        species: i64,
        species_count: i64,
        spacing_m: f64,
        chunk: i64,
        chunks_per_side: i64,
    ) -> PackedFloat32Array {
        match &self.world {
            Some(w) => crate::terrain_mesh::forest_buffer(
                w.terrain(),
                species.max(0) as u32,
                species_count.max(0) as u32,
                Metres(spacing_m),
                chunk.max(0) as u32,
                chunks_per_side.max(1) as u32,
            ),
            None => PackedFloat32Array::new(),
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

    /// What shape each resource comes in, in the same order as
    /// [`Self::resource_names`].
    ///
    /// Storage is a decision now — a tank takes liquids, a silo takes grain and
    /// cement, a bay takes heaps — and a player who learns that taxonomy only by
    /// being refused an order is a player being ambushed. The stockpile table
    /// carries it as a column.
    #[func]
    fn resource_forms(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        if let Some(w) = self.world.as_ref() {
            for form in w.resource_forms() {
                out.push(&GString::from(form));
            }
        }
        out
    }

    /// Which resources may be ordered at a place, by index into
    /// [`Self::resource_names`]. Empty for anything that is not a store.
    #[func]
    fn orderable(&self, building: i64) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        let Some(w) = self.world.as_ref() else {
            return out;
        };
        for resource in w.orderable(red_republic_sim::BuildingId(building.max(0) as u32)) {
            let index = red_republic_sim::Resource::ALL
                .iter()
                .position(|&r| r == resource)
                .unwrap_or_default();
            out.push(index as i32);
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

    /// Every energised span: `[ax, ay, bx, by, kind]`, kind 0 power, 1 heat.
    #[func]
    fn utility_lines(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::utility_lines)
    }

    /// Spans ordered and not yet carrying: `[ax, ay, bx, by, progress, kind]`.
    #[func]
    fn utility_sites(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::utility_sites)
    }

    /// The grid at a glance: `[power_km, heat_km, on_power, on_heat, dark,
    /// cold]`.
    #[func]
    fn utility_totals(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::utility_totals)
    }

    /// How dirty each lattice cell is, row-major.
    #[func]
    fn pollution_field(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::pollution_field)
    }

    /// How buried in snow each lattice cell is, row-major.
    #[func]
    fn snow_field(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::snow_field)
    }

    /// Tourism at a glance. See `views::tourism_totals` for the layout.
    #[func]
    fn tourism(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::tourism_totals)
    }

    /// What a hotel sited here would be worth to a visitor, `0.0..=1.0`.
    ///
    /// A placement query, in the same family as `going_at`: siting a hotel is a
    /// decision about what stands around it, and a player who cannot ask before
    /// they build has a building with a random yield.
    #[func]
    fn appeal_at(&self, x: f64, y: f64) -> f64 {
        self.world
            .as_ref()
            .map_or(0.0, |w| w.appeal_at(Point::new(Metres(x), Metres(y))))
    }

    /// The winter at a glance: `[lying, buried]`, each `0.0..=1.0`.
    ///
    /// How much is down, and how much of it nobody has shifted. Two numbers
    /// rather than one because they are different failures — a republic under
    /// deep snow with its roads swept is working, and one under a dusting it has
    /// not touched is about to stop.
    #[func]
    fn snow(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(w) = self.world.as_ref() {
            out.push(w.snow_cover() as f32);
            out.push(w.roads_unswept() as f32);
        }
        out
    }

    /// Order a power line or a heat main. Empty string on success, or the
    /// reason it was refused.
    #[func]
    fn order_line(&mut self, kind: i64, ax: f64, ay: f64, bx: f64, by: f64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let kind = *red_republic_sim::Utility::ALL
            .get(kind.max(0) as usize)
            .unwrap_or(&red_republic_sim::Utility::Power);
        match world.issue(Command::OrderLine {
            kind,
            from: Point::new(Metres(ax), Metres(ay)),
            to: Point::new(Metres(bx), Metres(by)),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    // ---- The four ways through -------------------------------------------

    /// Every span of one way as `[ax, ay, bx, by, speed_kph]`, indexed into
    /// `Medium::ALL`. One call for all four rather than four calls, so a fifth
    /// network draws itself without the shell being edited.
    #[func]
    fn ways(&self, medium: i64) -> PackedFloat32Array {
        let Some(world) = self.world.as_ref() else {
            return PackedFloat32Array::new();
        };
        let Some(&medium) = red_republic_sim::journey::Medium::ALL.get(medium.max(0) as usize)
        else {
            return PackedFloat32Array::new();
        };
        views::ways(world, medium)
    }

    /// Kilometres of each way, in `Medium::ALL` order.
    #[func]
    fn way_lengths(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::way_lengths)
    }

    /// The names of the ways, in `Medium::ALL` order.
    #[func]
    fn way_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for medium in red_republic_sim::journey::Medium::ALL {
            out.push(medium.name());
        }
        out
    }

    /// How many vehicles ride each way, then how many of those are out:
    /// `Medium::ALL` order twice over.
    #[func]
    fn fleet_by_medium(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::fleet_by_medium)
    }

    /// Order a way of any grade — a road, a bridge or a railway. Empty string
    /// on success, or the reason it was refused.
    #[func]
    fn order_way(
        &mut self,
        grade: i64,
        ax: f64,
        ay: f64,
        bx: f64,
        by: f64,
        lamps: bool,
    ) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let Some(&grade) = red_republic_sim::roadworks::GRADES
            .get(grade.max(0) as usize)
            .map(|d| &d.grade)
        else {
            return GString::from("no such grade");
        };
        match world.issue(Command::OrderRoad {
            from: Point::new(Metres(ax), Metres(ay)),
            to: Point::new(Metres(bx), Metres(by)),
            grade,
            lamps,
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// Every vehicle kind the republic can own, in `VEHICLES` order.
    ///
    /// The index a `vehicle_positions` row carries is a position in this list,
    /// and `vehicle_art.gd` keys off the names rather than the indices: an index
    /// is a position in a Rust table, so a reordering there would silently
    /// repaint the fleet, where a rename fails the art check and says so.
    #[func]
    fn vehicle_kind_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for def in red_republic_sim::fleet::VEHICLES {
            out.push(def.name);
        }
        out
    }

    /// Every grade the republic can lay, in table order:
    /// `name`, then `speed_kph|labour_per_km|medium_index` for each.
    #[func]
    fn grade_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for def in red_republic_sim::roadworks::GRADES {
            out.push(def.name);
        }
        out
    }

    /// What each grade costs and what it can do, in `GRADES` order:
    /// `[speed_kph, builder_days_per_km, medium_index, takes_lamps]` per grade.
    ///
    /// `takes_lamps` is what greys out the lighting option, and it is read off
    /// the authored table rather than decided in GDScript — a panel that made
    /// its own rule about which roads take lamps would be a second copy of a
    /// rule the simulation already refuses on.
    #[func]
    fn grade_facts(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        for def in red_republic_sim::roadworks::GRADES {
            out.push((def.speed.as_mps() * 3.6) as f32);
            out.push(def.labour as f32);
            out.push(
                red_republic_sim::journey::Medium::ALL
                    .iter()
                    .position(|&m| m == def.carries)
                    .unwrap_or_default() as f32,
            );
            out.push(if def.lamps { 1.0 } else { 0.0 });
        }
        out
    }

    /// How much road the republic has laid, in kilometres.
    #[func]
    fn road_length_km(&self) -> f64 {
        self.world.as_ref().map_or(0.0, |w| {
            w.network(red_republic_sim::journey::Medium::Road)
                .total_length()
                .as_km()
        })
    }

    /// How many ways are ordered and not yet drivable.
    #[func]
    fn road_site_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |w| w.roadworks().len() as i64)
    }

    /// Lit street: `[built_km, burning_km]`.
    ///
    /// Two numbers rather than one, because they mean different things to a
    /// player: the first is what was paid for and the second is what is on. A
    /// gap between them is a grid that has run short, which is a thing to fix
    /// rather than a thing to build.
    #[func]
    fn lit_road_km(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(w) = &self.world {
            let (built, alight) = w
                .network(red_republic_sim::journey::Medium::Road)
                .lit_length();
            out.push(built.as_km() as f32);
            out.push(alight.as_km() as f32);
        }
        out
    }

    /// What a kilometre of street lighting costs on top of the road under it:
    /// `[builder_days, megawatts, tonnes...]` interleaved as
    /// `[resource_index, tonnes]` after the first two.
    #[func]
    fn lamp_cost(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        out.push(red_republic_sim::roadworks::LAMP_LABOUR as f32);
        out.push(red_republic_sim::roadworks::LAMP_MW_PER_KM as f32);
        for &(resource, tonnes) in red_republic_sim::roadworks::LAMP_MATERIALS {
            out.push(
                red_republic_sim::Resource::ALL
                    .iter()
                    .position(|&r| r == resource)
                    .unwrap_or_default() as f32,
            );
            out.push(tonnes as f32);
        }
        out
    }

    // ---- Standing orders --------------------------------------------------

    /// What a place has been told to keep: `[resource_index, held, ordered]`
    /// per line. Empty for anything that does not keep goods to order.
    #[func]
    fn standing_orders(&self, building: i64) -> PackedFloat32Array {
        let Some(world) = self.world.as_ref() else {
            return PackedFloat32Array::new();
        };
        views::standing_orders(world, red_republic_sim::BuildingId(building.max(0) as u32))
    }

    /// Tell a terminal or a distribution office what to keep on hand. Zero
    /// cancels the order. Empty string on success, or the reason it was
    /// refused.
    #[func]
    fn set_standing_order(&mut self, building: i64, resource: i64, tonnes: f64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let Some(&resource) = red_republic_sim::Resource::ALL.get(resource.max(0) as usize) else {
            return GString::from("no such resource");
        };
        match world.issue(Command::SetStandingOrder {
            building: red_republic_sim::BuildingId(building.max(0) as u32),
            resource,
            tonnes: red_republic_sim::Tonnes(tonnes.max(0.0)),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    // ---- Shifts -----------------------------------------------------------
    //
    // The roster is the one place a player decides how much of the day the
    // republic is awake for, so all three levels of it are readable and
    // writable from here. Hours are refused rather than clamped on the way in;
    // every setter hands back the refusal so the panel can print it.

    /// The national working day, in hours per shift.
    #[func]
    fn national_shift_hours(&self) -> f64 {
        self.world
            .as_ref()
            .map_or(red_republic_sim::shifts::STANDARD_HOURS, |w| {
                w.shift_policy().national()
            })
    }

    /// The shortest and longest shift the republic will roster, and the most
    /// crews one building may run: `[min_hours, max_hours, max_shifts]`.
    ///
    /// Read rather than repeated in GDScript, so a slider cannot offer a value
    /// the command will refuse.
    #[func]
    fn shift_limits(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        out.push(red_republic_sim::shifts::MIN_HOURS as f32);
        out.push(red_republic_sim::shifts::MAX_HOURS as f32);
        out.push(f32::from(red_republic_sim::shifts::MAX_SHIFTS));
        out
    }

    #[func]
    fn set_national_shift_hours(&mut self, hours: f64) -> GString {
        self.command(Command::SetNationalShiftHours { hours })
    }

    /// The rule for each kind that has one: `[kind_index, hours]` per line.
    #[func]
    fn kind_shift_rules(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::kind_shift_rules)
    }

    /// Set a rule for a whole category of workplace; a negative `hours` clears
    /// it so the national standard applies again.
    #[func]
    fn set_kind_shift_hours(&mut self, kind: i64, hours: f64) -> GString {
        let Some(&kind) = red_republic_sim::building::BUILDINGS
            .get(kind.max(0) as usize)
            .map(|d| &d.kind)
        else {
            return GString::from("no such building");
        };
        self.command(Command::SetShiftHours {
            scope: red_republic_sim::command::ShiftScope::Kind(kind),
            hours: (hours >= 0.0).then_some(hours),
        })
    }

    /// Every standing workplace and its roster:
    /// `[id, kind_index, staff, jobs, shifts, hours, rule]` per line.
    #[func]
    fn workplaces(&self) -> PackedFloat32Array {
        self.world
            .as_ref()
            .map_or_else(PackedFloat32Array::new, views::workplaces)
    }

    /// How many crews one workplace runs. Zero mothballs it.
    #[func]
    fn set_shifts(&mut self, building: i64, shifts: i64) -> GString {
        self.command(Command::SetShifts {
            building: red_republic_sim::BuildingId(building.max(0) as u32),
            shifts: shifts.clamp(0, 255) as u8,
        })
    }

    /// Where one workplace stands in the labour plan: `0` last, `1` ordinary,
    /// `2` first — an index into `Priority::ALL`, the same one `workplaces`
    /// reports.
    ///
    /// An index rather than a name because the panel already has the names from
    /// `priority_names`, and matching on a translated string in GDScript is how
    /// a control quietly stops working.
    #[func]
    fn set_priority(&mut self, building: i64, standing: i64) -> GString {
        let all = red_republic_sim::Priority::ALL;
        let Some(&priority) = all.get(standing.clamp(0, all.len() as i64 - 1) as usize) else {
            return GString::from("there is no such standing");
        };
        self.command(Command::SetPriority {
            building: red_republic_sim::BuildingId(building.max(0) as u32),
            priority,
        })
    }

    /// How many people the republic could send to work a site of its own.
    ///
    /// Zero means "Build" would put down a foundation nothing will ever work,
    /// which is what the build screen uses to stop offering it.
    #[func]
    fn builders(&self) -> i64 {
        self.world.as_ref().map_or(0, |w| i64::from(w.builders()))
    }

    /// What the standings are called, worst first.
    #[func]
    fn priority_names(&self) -> PackedStringArray {
        red_republic_sim::Priority::ALL
            .iter()
            .map(|p| GString::from(p.name()))
            .collect()
    }

    /// An exception for one building; a negative `hours` clears it.
    #[func]
    fn set_building_shift_hours(&mut self, building: i64, hours: f64) -> GString {
        self.command(Command::SetShiftHours {
            scope: red_republic_sim::command::ShiftScope::Building(red_republic_sim::BuildingId(
                building.max(0) as u32,
            )),
            hours: (hours >= 0.0).then_some(hours),
        })
    }

    /// Issue a command and hand back the refusal, or an empty string.
    ///
    /// The shape every setter above wants, written once: a player-facing
    /// refusal is a sentence, and a panel that invents its own wording is a
    /// second place the rules are described.
    fn command(&mut self, command: Command) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        match world.issue(command) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// Whether this building keeps goods to order — what greys out the panel.
    #[func]
    fn keeps_to_order(&self, building: i64) -> bool {
        self.world.as_ref().is_some_and(|w| {
            w.buildings()
                .get(red_republic_sim::BuildingId(building.max(0) as u32))
                .is_some_and(|b| b.def().stores_to_order)
        })
    }

    /// The names of the two networks, in `Utility::ALL` order.
    #[func]
    fn utility_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for kind in red_republic_sim::Utility::ALL {
            out.push(kind.name());
        }
        out
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
        // The comfort lift, between the score and the worst component, so the
        // panel's layout matches `views::contentment`'s: components, overall,
        // lift. It is not one of the components and it is never the worst thing
        // about anywhere.
        out.push(b.content.lift() as f32);
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

    /// Pay a foreign firm to build it instead. `market` is 0 East, 1 West.
    ///
    /// The only way to raise anything on a blank map, which is what every map
    /// now starts as: there is no Construction Office to work a site and nobody
    /// to staff one. Returns the refusal sentence, or empty on success.
    #[func]
    fn contract(&mut self, kind: i64, x: f64, y: f64, market: i64) -> GString {
        let Some(world) = self.world.as_mut() else {
            return GString::from("no republic has been founded");
        };
        let Some(&kind) = red_republic_sim::building::BUILDINGS
            .get(kind.max(0) as usize)
            .map(|d| &d.kind)
        else {
            return GString::from("no such building");
        };
        match world.issue(Command::ContractBuild {
            kind,
            at: Point::new(Metres(x), Metres(y)),
            market: market_at(market),
        }) {
            Ok(_) => GString::from(""),
            Err(why) => GString::from(why.to_string().as_str()),
        }
    }

    /// What a build menu needs to show about one kind, in one packed read.
    ///
    /// `[workers, builder_days, contract_cost, width_m, depth_m]`. A packed
    /// array rather than a dictionary per kind, for the reason every bulk read
    /// across this boundary is: measured at 316x apart. This one is small, but
    /// the menu asks it for every kind on the roster every time it opens.
    ///
    /// `contract_cost` is what a foreign firm charges to raise it, which is the
    /// number that actually decides the opening — a player with a rouble grant
    /// and no crews is choosing what they can afford, not what they want.
    #[func]
    fn building_kind_facts(&self, index: i64) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        let Some(def) = red_republic_sim::building::BUILDINGS.get(index.max(0) as usize) else {
            return out;
        };
        out.push(def.workers as f32);
        out.push(def.labour as f32);
        out.push((def.labour * red_republic_sim::systems::CONTRACTOR_RATE) as f32);
        out.push(def.width.0 as f32);
        out.push(def.depth.0 as f32);
        out
    }

    /// What the treasury holds: `[roubles, dollars]`.
    #[func]
    fn purse(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(w) = &self.world {
            out.push(w.treasury().of(red_republic_sim::trade::Market::East) as f32);
            out.push(w.treasury().of(red_republic_sim::trade::Market::West) as f32);
        }
        out
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
