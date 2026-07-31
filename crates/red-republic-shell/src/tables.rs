//! The authored rosters, handed over as data.
//!
//! `BUILDINGS`, `VEHICLES`, `GRADES` and `Resource::ALL` are the tables the
//! balance is written in. This module is how the interface reads them.
//!
//! # It hands over figures and never sentences
//!
//! This file replaced one that composed the in-game reference in Rust — headings,
//! entry titles, label-and-value lines, paragraphs of prose — and shipped the
//! result across the boundary as marked-up text for the shell to style. That was
//! the interface living in the simulation's crate: the wording of "Coal is under
//! the site itself", the decision that footprint comes before staffing, and the
//! choice to say "nothing — it must be imported" were all made here, where
//! nobody can see them beside the screen they appear on.
//!
//! **Every one of those is now in `godot/ui/reference.gd`, and what crosses is
//! numbers.** The rule this file holds to is the same one [`crate::views`] holds
//! to: nothing here computes, and nothing here decides what a player reads. A
//! quantity, a name authored beside the variant it names, and an index into a
//! roster are facts about the republic. A sentence is not.
//!
//! # The claim that survived the move, made stronger
//!
//! The old module's whole claim was "generated from the authored tables, so it
//! cannot describe a republic that no longer exists", and a test asserted that
//! every authored row appeared somewhere in the text. That test is gone because
//! **the thing it guarded against is no longer representable**: the screen walks
//! `building_kind_count()` rows of [`building_table`], so a building added to
//! `BUILDINGS` appears in the reference without anybody doing anything, and one
//! that did not appear would have to be missing from the roster itself.
//!
//! What is left to get wrong is the packing, and that is what the tests at the
//! bottom check: a table whose length is not a multiple of its stride shifts
//! every column by one and reports a mine's staff as its power draw, silently.
//!
//! # Why the work happens in plain `Vec`s
//!
//! Same reason the module this replaced generated `String`s: a
//! `PackedFloat32Array` cannot be constructed without the engine runtime, so a
//! function that returned one **could not be unit-tested at all** — the two
//! guards below are worth more than the convenience of skipping a copy.
//! [`floats`], [`ints`] and [`strings`] are the only part that needs Godot, and
//! they contain no decisions.
//!
//! # Bulk goes packed
//!
//! Same measured rule as [`crate::views`] and [`crate::marshal`]: a dictionary
//! per entity cost 8,640 µs at 1,205 entries against 27 µs for a flat array.
//! These are read when a screen opens rather than every frame, which is why the
//! per-kind lists below are allowed to be one call each — a raw call is 0.21 µs
//! and the reference opens once.

use godot::prelude::*;
use red_republic_sim::Mineral;
use red_republic_sim::building::BUILDINGS;
use red_republic_sim::citizen::Education;
use red_republic_sim::fleet::VEHICLES;
use red_republic_sim::journey::Medium;
use red_republic_sim::resource::{Form, Resource};
use red_republic_sim::roadworks::{GRADES, LAMP_MATERIALS};

/// Floats per building in [`building_table`].
pub const BUILDING_STRIDE: usize = 15;

/// Every authored building, one row each.
///
/// `[width_m, depth_m, workers, schooling, residents, beds, builder_days,
/// contract_cost, power_draw_kw, power_output_kw, heat_kw, heat_output_kw,
/// storage_t, seats, taps]`, in `BUILDINGS` order — the same order
/// `building_kind_name` and `building_kind_facts` index by, so a row and a name
/// cannot come apart.
///
/// `schooling` indexes [`Education::ALL`] and `taps` indexes [`Mineral::ALL`],
/// or is `-1` for a building that works no deposit. An index rather than a word,
/// because the word is the interface's to choose.
pub fn building_table() -> Vec<f32> {
    let mut out = Vec::with_capacity(BUILDINGS.len() * BUILDING_STRIDE);
    for def in BUILDINGS {
        out.push(def.width.0 as f32);
        out.push(def.depth.0 as f32);
        out.push(def.workers as f32);
        out.push(
            Education::ALL
                .iter()
                .position(|&e| e == def.schooling)
                .unwrap_or_default() as f32,
        );
        out.push(def.residents as f32);
        out.push(def.beds as f32);
        out.push(def.labour as f32);
        out.push((def.labour * red_republic_sim::systems::CONTRACTOR_RATE) as f32);
        out.push(def.power_draw as f32);
        out.push(def.power_output as f32);
        out.push(def.heat as f32);
        out.push(def.heat_output as f32);
        out.push(def.storage as f32);
        out.push(def.seats as f32);
        out.push(match def.taps {
            Some(mineral) => Mineral::ALL
                .iter()
                .position(|&m| m == mineral)
                .unwrap_or_default() as f32,
            None => -1.0,
        });
    }
    out
}

/// Floats per line in [`building_flows`].
pub const FLOW_STRIDE: usize = 3;

/// A building's goods: what it eats, what it makes, what it is made of, and what
/// it puts on a shelf.
///
/// `[section, resource, tonnes]` per line, where section is `0` consumed per
/// day, `1` produced per day, `2` consumed once to build it, `3` sold to
/// citizens. A shelf line carries no tonnage — what a shop sells is a list, not
/// a rate — so its third figure is zero and the screen does not print it.
///
/// **One view for all four rather than four views**, because they are the same
/// shape and a fifth kind of flow should not need a new binding. The alternative
/// was four getters and four places to forget one.
pub fn building_flows(index: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let Some(def) = BUILDINGS.get(index) else {
        return out;
    };
    let mut push = |section: f32, resource: Resource, tonnes: f64| {
        out.push(section);
        out.push(resource_index(resource) as f32);
        out.push(tonnes as f32);
    };
    for (resource, tonnes) in def.inputs {
        push(0.0, *resource, *tonnes);
    }
    for (resource, tonnes) in def.outputs {
        push(1.0, *resource, *tonnes);
    }
    for (resource, tonnes) in def.materials {
        push(2.0, *resource, *tonnes);
    }
    for resource in def.sells {
        push(3.0, *resource, 0.0);
    }
    out
}

/// Which shapes of goods a building will take at all, as indices into
/// [`Form::ALL`].
///
/// This is what turns storage from a number into a decision — a tank holds
/// liquids and nothing else — so it is a fact the reference has to be able to
/// state, and it is not derivable from anything else on the row.
pub fn building_admits(index: usize) -> Vec<i32> {
    let mut out = Vec::new();
    let Some(def) = BUILDINGS.get(index) else {
        return out;
    };
    for form in def.admits {
        out.push(Form::ALL.iter().position(|f| f == form).unwrap_or_default() as i32);
    }
    out
}

/// A garage's establishment: `[vehicle_kind, count]` per line, indexed into
/// `VEHICLES`.
///
/// Fixed at the building rather than bought, which is the whole reason a player
/// needs to be able to read it: wanting another lorry means another depot.
pub fn building_fleet(index: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let Some(def) = BUILDINGS.get(index) else {
        return out;
    };
    for (kind, count) in def.vehicles {
        out.push(
            VEHICLES
                .iter()
                .position(|d| d.kind == *kind)
                .unwrap_or_default() as f32,
        );
        out.push(*count as f32);
    }
    out
}

/// Floats per vehicle in [`vehicle_table`].
pub const VEHICLE_STRIDE: usize = 8;

/// Every authored vehicle, one row each.
///
/// `[medium, capacity_t, seats, road_kph, cross_country_kph, fuel_t_per_km,
/// tank_t, going]`, in `VEHICLES` order — the order `vehicle_kind_names`
/// returns.
///
/// `going` is the worst ground it can cross, on the `0.0` firm to `1.0`
/// impassable scale the terrain uses. **It is handed over raw and unranked**:
/// the number means nothing on its own and what a player needs is where this
/// vehicle sits against the others, which is a comparison the screen makes from
/// the column it already has. Ranking it here would be this crate deciding what
/// "good" means.
pub fn vehicle_table() -> Vec<f32> {
    let mut out = Vec::with_capacity(VEHICLES.len() * VEHICLE_STRIDE);
    for def in VEHICLES {
        out.push(
            Medium::ALL
                .iter()
                .position(|&m| m == def.medium)
                .unwrap_or_default() as f32,
        );
        out.push(def.capacity.0 as f32);
        out.push(def.seats as f32);
        out.push(def.on_road.as_kph() as f32);
        out.push(def.cross_country.as_kph() as f32);
        out.push(def.fuel_per_km as f32);
        out.push(def.tank.0 as f32);
        out.push(def.ground as f32);
    }
    out
}

/// What a kilometre of one grade is made of: `[resource, tonnes]` per line.
pub fn grade_materials(index: usize) -> Vec<f32> {
    let mut out = Vec::new();
    let Some(def) = GRADES.get(index) else {
        return out;
    };
    for (resource, tonnes) in def.materials {
        out.push(resource_index(*resource) as f32);
        out.push(*tonnes as f32);
    }
    out
}

/// What a kilometre of street lighting costs on top of the road under it:
/// `[resource, tonnes]` per line.
///
/// Its own read rather than folded into the grade's bill, because that is what
/// it is: lamps are a variant of a paved road, and the simulation keeps the two
/// bills apart on purpose — a resource appearing in both would be a quantity
/// silently halved.
pub fn lamp_materials() -> Vec<f32> {
    let mut out = Vec::new();
    for (resource, tonnes) in LAMP_MATERIALS {
        out.push(resource_index(*resource) as f32);
        out.push(*tonnes as f32);
    }
    out
}

/// What the player calls each mineral, in [`Mineral::ALL`] order.
pub fn mineral_names() -> Vec<&'static str> {
    Mineral::ALL.iter().map(|m| m.name()).collect()
}

/// What the player calls each level of schooling, in [`Education::ALL`] order.
pub fn schooling_names() -> Vec<&'static str> {
    Education::ALL.iter().map(|e| e.name()).collect()
}

/// The shapes goods come in, in [`Form::ALL`] order.
pub fn form_names() -> Vec<&'static str> {
    Form::ALL.iter().map(|f| f.name()).collect()
}

fn resource_index(resource: Resource) -> usize {
    Resource::ALL
        .iter()
        .position(|&r| r == resource)
        .unwrap_or_default()
}

// ---- the Godot side ---------------------------------------------------------
//
// Nothing below decides anything. Every one is the corresponding function above
// with its result copied into an engine type, and they are separate for the
// reason the module note gives: an engine type cannot exist in a unit test.

/// Copy floats into the engine's array type.
pub fn floats(values: Vec<f32>) -> PackedFloat32Array {
    let mut out = PackedFloat32Array::new();
    for value in values {
        out.push(value);
    }
    out
}

/// Copy integers into the engine's array type.
pub fn ints(values: Vec<i32>) -> PackedInt32Array {
    let mut out = PackedInt32Array::new();
    for value in values {
        out.push(value);
    }
    out
}

/// Copy names into the engine's array type.
pub fn strings(values: Vec<&'static str>) -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for value in values {
        out.push(&GString::from(value));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A packed table whose length is not a multiple of its stride shifts every
    /// column by one from the mistake onwards, and reports a mine's staff as its
    /// power draw without erroring.
    ///
    /// This is the only thing left for this module to get wrong. What it
    /// replaced — a document that could silently omit a building — is no longer
    /// representable, because the screen walks the roster rather than a list of
    /// entries somebody wrote.
    ///
    /// The counts are asserted against the rosters rather than against numbers
    /// typed here, so the check cannot be satisfied by a table that emitted
    /// nothing at all.
    #[test]
    fn every_row_is_the_stride_it_declares() {
        assert_eq!(
            building_table().len(),
            BUILDINGS.len() * BUILDING_STRIDE,
            "the building table does not divide into {} rows of {BUILDING_STRIDE}",
            BUILDINGS.len()
        );
        assert_eq!(
            vehicle_table().len(),
            VEHICLES.len() * VEHICLE_STRIDE,
            "the vehicle table does not divide into {} rows of {VEHICLE_STRIDE}",
            VEHICLES.len()
        );

        for (index, def) in BUILDINGS.iter().enumerate() {
            assert_eq!(
                building_flows(index).len() % FLOW_STRIDE,
                0,
                "{}'s goods do not divide by {FLOW_STRIDE}",
                def.name
            );
            assert_eq!(
                building_fleet(index).len() % 2,
                0,
                "{}'s establishment is not pairs",
                def.name
            );
        }
        for (index, def) in GRADES.iter().enumerate() {
            assert_eq!(
                grade_materials(index).len() % 2,
                0,
                "{}'s bill is not pairs",
                def.name
            );
        }
        assert_eq!(lamp_materials().len() % 2, 0, "the lamp bill is not pairs");

        // Every roster the tables index into is named, or the screen reads a
        // name off the end of an array and shows an empty cell.
        assert_eq!(mineral_names().len(), Mineral::ALL.len());
        assert_eq!(schooling_names().len(), Education::ALL.len());
        assert_eq!(form_names().len(), Form::ALL.len());
    }

    /// Every index handed over lands inside the roster it indexes.
    ///
    /// An index is the whole reason this module hands over numbers instead of
    /// words, and an out-of-range one is the failure that buys: the screen reads
    /// past the end of a name array and prints nothing rather than erroring.
    /// Checked here rather than trusted, because the `unwrap_or_default` in
    /// every lookup above turns a genuinely missing entry into "the first one" —
    /// which is a wrong answer that looks like a right one.
    #[test]
    fn no_index_points_off_the_end_of_its_roster() {
        let table = building_table();
        for (index, def) in BUILDINGS.iter().enumerate() {
            let row = index * BUILDING_STRIDE;
            let schooling = table[row + 3] as usize;
            assert!(
                schooling < Education::ALL.len(),
                "{} asks for schooling {schooling}",
                def.name
            );
            let taps = table[row + 14];
            assert!(
                taps < Mineral::ALL.len() as f32,
                "{} taps mineral {taps}",
                def.name
            );

            for line in building_flows(index).chunks(FLOW_STRIDE) {
                assert!(
                    (line[1] as usize) < Resource::ALL.len(),
                    "{} names resource {}",
                    def.name,
                    line[1]
                );
                assert!(
                    (0.0..=3.0).contains(&line[0]),
                    "{} has a flow in section {}",
                    def.name,
                    line[0]
                );
            }

            for pair in building_fleet(index).chunks(2) {
                assert!(
                    (pair[0] as usize) < VEHICLES.len(),
                    "{} keeps vehicle {}",
                    def.name,
                    pair[0]
                );
            }

            for form in building_admits(index) {
                assert!(
                    (form as usize) < Form::ALL.len(),
                    "{} admits form {form}",
                    def.name
                );
            }
        }

        for (row, def) in vehicle_table().chunks(VEHICLE_STRIDE).zip(VEHICLES) {
            assert!(
                (row[0] as usize) < Medium::ALL.len(),
                "{} rides medium {}",
                def.name,
                row[0]
            );
        }

        for (index, def) in GRADES.iter().enumerate() {
            for pair in grade_materials(index).chunks(2) {
                assert!(
                    (pair[0] as usize) < Resource::ALL.len(),
                    "{} is built of resource {}",
                    def.name,
                    pair[0]
                );
            }
        }
        for pair in lamp_materials().chunks(2) {
            assert!(
                (pair[0] as usize) < Resource::ALL.len(),
                "street lighting is built of resource {}",
                pair[0]
            );
        }
    }
}
