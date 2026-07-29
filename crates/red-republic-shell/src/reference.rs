//! The reference the game explains itself with.
//!
//! No advisor and no scripted tutorial. Instead: a document **generated from the
//! authored tables** — `BUILDINGS`, `Resource::ALL`, `GRADES`, `VEHICLES` — so a
//! building added to the simulation documents itself in the same build, and the
//! reference cannot describe a republic that no longer exists. That is the only
//! version of documentation this project's rules would accept, and
//! `every_authored_row_reaches_the_reference` is what makes the claim checkable
//! rather than aspirational.
//!
//! # Why generation happens here rather than in GDScript
//!
//! The alternative was marshalling every field of every table across the boundary
//! and composing sentences on the other side. `BuildingDef` alone has twenty
//! fields including four lists of `(Resource, f64)` pairs; packing that is a lot
//! of stride documentation for a screen nobody reads at 60 fps. Composing here
//! keeps one copy of "what a building's row means" next to the crate that
//! authored it.
//!
//! What deliberately stays on the Godot side is **how it looks**: this emits
//! marked lines and the shell's reference screen decides what a heading is. The
//! same split as `terrain.gdshader` reading a surface channel rather than Rust
//! choosing what grass looks like.
//!
//! # The markers
//!
//! Each line begins with one character saying what it is. A convention rather
//! than a format, small enough to hold in your head:
//!
//! | marker | meaning |
//! |---|---|
//! | `#` | section heading |
//! | `>` | entry title |
//! | `:` | a label and a value, separated by a tab |
//! | `.` | a paragraph |
//! | `-` | a bullet |
//!
//! # The prose is about invariants, never about numbers
//!
//! A handful of lines here are hand-written, because "a vehicle never accepts a
//! job it cannot finish" is not derivable from a table and a stranger cannot play
//! without it. Every one of them states a **rule that the design holds fixed**,
//! and never a quantity — quantities come from the tables, where they cannot
//! drift out of step. That boundary is the whole reason hand-written text is
//! allowed in a file whose point is generation.

use godot::prelude::*;
use red_republic_sim::building::BUILDINGS;
use red_republic_sim::fleet::VEHICLES;
use red_republic_sim::resource::Resource;
use red_republic_sim::roadworks::GRADES;

/// The whole reference, as marked lines.
///
/// Plain `String`s and no Godot types anywhere, which is not incidental: a
/// `PackedStringArray` cannot be constructed without the engine runtime, so a
/// generator that returned one could not be unit-tested at all. The two guards at
/// the bottom of this file are the reference's entire claim to being generated
/// rather than written, and they are worth more than the convenience.
pub fn document() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |line: String| out.push(line);

    playing(&mut push);
    resources(&mut push);
    buildings(&mut push);
    vehicles(&mut push);
    grades(&mut push);

    out
}

/// The same, for the Godot side.
pub fn document_for_godot() -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for line in document() {
        out.push(&GString::from(line.as_str()));
    }
    out
}

/// How a republic works, in the fewest rules that let somebody start.
///
/// Invariants only — see the module note. Each of these is a design rule this
/// build enforces rather than a figure that could be retuned underneath it.
fn playing(push: &mut impl FnMut(String)) {
    push("#How a republic works".into());

    push(">Time".into());
    push(
        ".One real second is one in-game second at the first speed. The others buy \
         an hour, two, four or eight of in-game time per second. Nothing is ever \
         skipped — there is no way to make time pass without spending it."
            .into(),
    );

    push(">Nothing happens instantly".into());
    push(
        ".Everything ordered is a site: a bill of materials, builder-days, and a \
         place your lorries deliver to. There is no button that makes a site \
         finish. Anything sourced inside the republic costs labour and materials \
         and no money at all."
            .into(),
    );

    push(">Builders are people who travel".into());
    push(
        ".A Construction Office employs builders who commute to it like any other \
         workplace, and it runs buses out to its sites. A remote site is expensive \
         in vehicles, roads and time rather than in money. An office cannot be \
         pulled down with its crews out, and neither can a site with a gang on it \
         — recall them first."
            .into(),
    );

    push(">Freight is vehicles".into());
    push(
        ".Garages own vehicles, and a depot's establishment is fixed: wanting more \
         lorries means another depot and the people to crew it. A vehicle never \
         accepts a job it cannot finish, so running dry is a refusal in the yard \
         rather than a lorry stranded in a field."
            .into(),
    );

    push(">Roads are preferred, never required".into());
    push(
        ".A vehicle costs the road route against the open-ground straight line and \
         takes whichever is quicker, so a road is something it chooses because it \
         is faster. Traffic packs the ground it crosses, and a corridor worn hard \
         enough becomes a dirt track on the map that nobody ordered. Roads never \
         bog; that is the whole argument for building one."
            .into(),
    );

    push(">Water has to be bridged".into());
    push(
        ".A bridge is its own grade and the only one that may span water. Until \
         somebody pays for one, a river divides the republic."
            .into(),
    );

    push(">Power and heat are networks".into());
    push(
        ".A plant lights only what it is strung to and a boiler warms only what a \
         main runs past. A consumer plugs into a transformer station, and the \
         station plugs into the line — a pylon strung past a factory does not run \
         it. Heat leaks far faster than power does per kilometre, which is why \
         district heating is a town-scale thing and a remote camp wants its own \
         boiler."
            .into(),
    );

    push(">The border has two sides".into());
    push(
        ".The whole perimeter is frontier, divided into stretches held by the \
         Western Alliance or the Eastern Bloc, with four posts on your own ground. \
         You do not build a crossing; you build road out to one. A customs house \
         clears only for the bloc whose post it stands at — so earning dollars \
         means hauling to a Western post, and if the only Western post is across \
         the map, that is what a dollar costs."
            .into(),
    );

    push(">People arrive, and people leave".into());
    push(
        ".Contentment is a breakdown rather than a score, and the weakest component \
         is what to fix. A republic that serves its people attracts more; one that \
         fails them loses them. Settlers arrive at a frontier post and have to be \
         fetched by a coach — nobody appears inside a housing block. A group \
         nobody comes for goes home."
            .into(),
    );

    push(">Work has to be reachable".into());
    push(
        ".A job nobody can reach goes unfilled however many people are out of work, \
         and so does a job nobody is qualified for. Roughly two kilometres on \
         foot; further only with transport, and transport is bounded by journey \
         time rather than distance — which is what makes a faster road genuinely \
         extend reach."
            .into(),
    );

    push(">The land is the difficulty".into());
    push(
        ".There are no difficulty levels. A taiga posting costs measurably more to \
         run than a plains one, and choosing a hard posting is a choice made \
         inside the fiction. Socialist republics are not a walk in the park."
            .into(),
    );
}

fn resources(push: &mut impl FnMut(String)) {
    push("#Goods".into());
    push(
        ".What shape a good comes in decides which store will take it. A tank holds \
         liquids, a silo holds grain and cement, a bay holds heaps. This governs \
         deliveries only — a coal mine does not refuse its own coal."
            .into(),
    );
    for resource in Resource::ALL {
        push(format!(">{}", resource.name()));
        push(format!(":Form\t{}", resource.form().name()));
        let made_by = BUILDINGS
            .iter()
            .filter(|d| d.outputs.iter().any(|(r, _)| *r == resource))
            .map(|d| d.name)
            .collect::<Vec<_>>();
        let used_by = BUILDINGS
            .iter()
            .filter(|d| d.inputs.iter().any(|(r, _)| *r == resource))
            .map(|d| d.name)
            .collect::<Vec<_>>();
        // "Nothing makes this" is a real and useful answer — it means the good
        // has to be imported — so the line appears either way rather than being
        // omitted when empty.
        push(format!(
            ":Made by\t{}",
            if made_by.is_empty() {
                "nothing — it must be imported".to_string()
            } else {
                made_by.join(", ")
            }
        ));
        push(format!(
            ":Used by\t{}",
            if used_by.is_empty() {
                "nothing".to_string()
            } else {
                used_by.join(", ")
            }
        ));
    }
}

fn buildings(push: &mut impl FnMut(String)) {
    push("#Buildings".into());
    for def in BUILDINGS {
        push(format!(">{}", def.name));
        push(format!(
            ":Footprint\t{:.0} x {:.0} m",
            def.width.0, def.depth.0
        ));
        if def.workers > 0 {
            push(format!(
                ":Staff\t{} ({})",
                def.workers,
                schooling_text(def.schooling)
            ));
        }
        if def.residents > 0 {
            push(format!(":Houses\t{} people", def.residents));
        }
        if !def.inputs.is_empty() {
            push(format!(":Consumes\t{}", pairs(def.inputs)));
        }
        if !def.outputs.is_empty() {
            push(format!(":Produces\t{}", pairs(def.outputs)));
        }
        if def.power_draw > 0.0 {
            push(format!(":Power\t{:.0} kW", def.power_draw));
        }
        if def.power_output > 0.0 {
            push(format!(":Generates\t{:.0} kW", def.power_output));
        }
        if def.heat > 0.0 {
            push(format!(":Heat\t{:.0} kW", def.heat));
        }
        if def.heat_output > 0.0 {
            push(format!(":Boiler\t{:.0} kW", def.heat_output));
        }
        if let Some(mineral) = def.taps {
            push(format!(":Works\t{mineral:?} deposits"));
        }
        if def.storage > 0.0 {
            push(format!(":Holds\t{:.0} t", def.storage));
        }
        if !def.admits.is_empty() {
            push(format!(
                ":Accepts\t{}",
                def.admits
                    .iter()
                    .map(|f| f.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !def.vehicles.is_empty() {
            push(format!(
                ":Fleet\t{}",
                def.vehicles
                    .iter()
                    .map(|(kind, n)| format!("{n} x {}", vehicle_name(*kind)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        push(format!(
            ":To build\t{}, {:.0} builder-days",
            pairs(def.materials),
            def.labour
        ));
    }
}

fn vehicles(push: &mut impl FnMut(String)) {
    push("#Vehicles".into());
    push(
        ".A vehicle belongs to the garage that was built for it, and it is only as \
         useful as the drivers the garage can staff."
            .into(),
    );
    for def in VEHICLES {
        push(format!(">{}", def.name));
        push(format!(":Rides\t{}", def.medium.name()));
        if def.capacity.0 > 0.0 {
            push(format!(":Carries\t{:.0} t", def.capacity.0));
        }
        if def.seats > 0 {
            push(format!(":Seats\t{}", def.seats));
        }
        push(format!(
            ":Speed\t{:.0} km/h on road, {:.0} km/h cross-country",
            def.on_road.as_kph(),
            def.cross_country.as_kph()
        ));
        push(format!(
            ":Fuel\t{:.2} t/km, {:.0} t tank",
            def.fuel_per_km, def.tank.0
        ));
        push(format!(":Going\t{}", going_text(def.ground)));
    }
}

fn grades(push: &mut impl FnMut(String)) {
    push("#Ways".into());
    push(
        ".A journey leg carries the way's own speed limit, not the vehicle's — a \
         lorry does the limit of the track it is on whatever it is capable of. \
         Which is what makes a grade a decision."
            .into(),
    );
    for def in GRADES {
        push(format!(">{}", def.name));
        push(format!(":Carries\t{}", def.carries.name()));
        push(format!(":Limit\t{:.0} km/h", def.speed.as_kph()));
        push(format!(
            ":Per kilometre\t{}, {:.0} builder-days",
            pairs(def.materials),
            def.labour
        ));
    }
}

fn pairs(list: &[(Resource, f64)]) -> String {
    if list.is_empty() {
        return "nothing".to_string();
    }
    list.iter()
        .map(|(resource, amount)| format!("{:.1} t {}", amount, resource.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn vehicle_name(kind: red_republic_sim::VehicleKind) -> &'static str {
    VEHICLES
        .iter()
        .find(|d| d.kind == kind)
        .map_or("vehicle", |d| d.name)
}

/// What a job's schooling requirement means in words.
///
/// A job nobody is qualified for goes unfilled however many people are out of
/// work, so this is not a footnote — it is why a refinery will not open.
fn schooling_text(needs: red_republic_sim::Education) -> &'static str {
    match needs {
        red_republic_sim::Education::Unschooled => "no schooling needed",
        red_republic_sim::Education::Schooled => "schooled",
        red_republic_sim::Education::Graduate => "graduates",
    }
}

/// A vehicle's cross-country capability, in words a player can rank.
///
/// The number is a margin against how hard the going is, which means nothing on
/// its own. What a player needs is where this vehicle sits against the others.
fn going_text(ground: f64) -> String {
    let hardest = VEHICLES
        .iter()
        .map(|d| d.ground)
        .fold(f64::MIN, f64::max)
        .max(1.0);
    let share = ground / hardest;
    let word = if share >= 0.95 {
        "the best in the republic"
    } else if share >= 0.7 {
        "good"
    } else if share >= 0.45 {
        "fair"
    } else {
        "poor — keep it on made ground"
    };
    format!("{word} ({ground:.1})")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference's whole claim: it is generated, so it cannot go stale.
    ///
    /// Every authored row must appear. A building added to `BUILDINGS` and absent
    /// here would be a building the game does not explain — and since there is no
    /// tutorial and no advisor, undocumented means undiscoverable.
    ///
    /// This is the test that turns "generated from the authored tables" from a
    /// design intention into something that fails the build.
    #[test]
    fn every_authored_row_reaches_the_reference() {
        let lines = document();
        let text = lines.join("\n");

        // The premise. A scan for names inside a document that failed to
        // generate would report every name missing, which is the loud direction
        // — but a document of *only* the hand-written prose would still contain
        // enough words to be mistaken for one, so the section count is checked
        // rather than assumed.
        let sections = lines.iter().filter(|l| l.starts_with('#')).count();
        assert_eq!(
            sections, 5,
            "the reference has {sections} sections; the generator emits five, so \
             either one stopped emitting or a new one arrived unmeasured"
        );

        let mut missing: Vec<String> = Vec::new();
        for def in BUILDINGS {
            if !text.contains(def.name) {
                missing.push(format!("building {}", def.name));
            }
        }
        for resource in Resource::ALL {
            if !text.contains(resource.name()) {
                missing.push(format!("resource {}", resource.name()));
            }
        }
        for def in VEHICLES {
            if !text.contains(def.name) {
                missing.push(format!("vehicle {}", def.name));
            }
        }
        for def in GRADES {
            if !text.contains(def.name) {
                missing.push(format!("grade {}", def.name));
            }
        }

        assert!(
            missing.is_empty(),
            "the reference does not mention: {}. \
             There is no tutorial and no advisor in this game, so anything the \
             reference omits is something a player has no way to find out.",
            missing.join(", ")
        );
    }

    /// Every line carries a marker the screen knows how to render.
    ///
    /// An unmarked line would render as nothing at all, which is the silent
    /// failure this convention is otherwise wide open to.
    #[test]
    fn every_line_is_marked() {
        for line in document() {
            let marker = line.chars().next().expect("no line is empty");
            assert!(
                "#>:.-".contains(marker),
                "line starts with {marker:?}, which the reference screen cannot \
                 render: {line}"
            );
            if marker == ':' {
                assert!(
                    line.contains('\t'),
                    "a label line needs a tab between label and value: {line}"
                );
            }
        }
    }
}
