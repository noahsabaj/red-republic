//! The reachability guard: a binding nobody calls is a feature nobody has.
//!
//! # The failure this exists to make impossible
//!
//! `tests/exposure.rs` watches one boundary — simulation to shell — and it has
//! been holding it honestly. This watches the *next* one, and it was missing:
//! every view can be exposed on `Republic` and every one of them can be
//! unreachable, because `godot/` is where a player actually gets to it.
//!
//! That is not hypothetical. It is what this repository shipped, repeatedly, and
//! the pattern is always identical — a binding lands with the simulation work
//! and the screen that would call it is left for later:
//!
//! - `place()` existed for five milestones with no build menu, so nothing could
//!   be built at all;
//! - `order_way()` existed for six with no `.gd` file calling it, so on a map
//!   that starts empty nothing could be connected to anything;
//! - and thirteen bindings — `home_contentment`, `site_crew`, `standing_orders`,
//!   `orderable`, `recall_crew`, `hire_foreign` and the rest — hung off a
//!   building inspector that did not exist, which is W&R's core interaction.
//!
//! Each was found by a person noticing. That is the thing being replaced here:
//! the first condition of the goal says nothing the simulation knows may be
//! invisible, and a fact that reaches the shell and stops there is invisible in
//! exactly the way that matters.
//!
//! # What it is, stated plainly
//!
//! A **source scan**, with the same three defences `exposure.rs` carries and for
//! the same reason — this project has watched a scan go blind and report success
//! for four releases:
//!
//! 1. it globs rather than naming files, so a new `.gd` or a new shell module
//!    needs no change here;
//! 2. it has a floor under the count of bindings found, so a broken parse
//!    collapses to "nothing to check" and fails rather than passing;
//! 3. it prints the count, so a number that drops is visible in a passing log.
//!
//! What it cannot tell you is whether a binding is called *well*, or called on a
//! screen a player can reach. `journal_page` mentioned once in a comment would
//! satisfy it — which is why comments are stripped — and a call inside a screen
//! nothing opens would satisfy it too. It catches the case that actually
//! happens.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Below this many discovered bindings, assume the parse broke rather than that
/// the shell shrank. The real population was 180 when this was written.
const FLOOR: usize = 100;

/// Bindings deliberately not called from GDScript, each with the reason.
///
/// **Kept empty, and that is the intended state.** A `#[func]` exists for one
/// purpose: to be called across the boundary. An entry here is a claim that
/// something else is going on, and like `exposure.rs`'s `HIDDEN` it is checked
/// in both directions — see [`no_exemption_outlives_what_it_excuses`] — so a
/// binding that gets wired up later cannot keep an excuse it no longer needs.
const NOT_FOR_GODOT: &[(&str, &str)] = &[];

/// Bindings with no screen **yet**, each naming the screen it is waiting for.
///
/// # Why this list exists rather than an empty guard
///
/// The first run of this test found fifty-seven orphans. Forty-five of them were
/// the five gaps that milestone closed; ten were a backlog written down here,
/// and **the backlog is now empty** — every one of them has the screen its
/// entry named:
///
/// - `order_line` has the Lines table in the build menu and the same two-click
///   tool the ways use, so a power station can be strung to something;
/// - `road_sites` are drawn on the map, filling in from the near end;
/// - `bog_chance` and `vehicle_destination_fullness` are the two figures the
///   vehicle panel exists for;
/// - `lamp_cost` is priced under the ways table;
/// - `lit_road_km` is the street-lighting line in the HUD, laid against alight;
/// - `temperature_on_day` composes the year screen;
/// - `lattice_cell_size` says how coarse an overlay is, on the overlay;
/// - `shelf_size` is what the founding screen sizes its card list from;
/// - `save_version` stamps the archive and every file name in it.
///
/// **Empty is the intended state, and it is worth keeping.** Writing a gap down
/// here is the point of the list: a backlog in a comment rots silently and a
/// backlog the build reads cannot.
/// [`no_exemption_outlives_what_it_excuses`] checks it in both directions, so
/// wiring one up **fails the build** until its line is deleted — an entry cannot
/// outlive the gap it describes. Anything new is not on the list, so it lands in
/// the main assertion where it belongs.
///
/// Each reason names the screen, not the binding. "There is no vehicle
/// inspector" is a thing somebody can go and build; "unused" is not.
const NOT_YET_REACHED: &[(&str, &str)] = &[];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the shell crate sits two levels under the repository root")
        .to_path_buf()
}

/// Every `#[func]` the shell registers, by name.
///
/// Reads the attribute rather than every `fn`, because the shell has plenty of
/// private helpers that are nobody's business on the other side. The name is
/// taken off the *next* `fn` line, which is how gdext requires them to be
/// written.
fn bindings() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut sources = Vec::new();
    collect(
        &repo_root().join("crates/red-republic-shell/src"),
        &["rs"],
        &mut sources,
    );
    for text in sources {
        let mut lines = text.lines().map(str::trim).peekable();
        while let Some(line) = lines.next() {
            if line != "#[func]" {
                continue;
            }
            // gdext allows attributes between `#[func]` and the signature, so
            // walk forward to the `fn` rather than assuming the next line is it.
            for following in lines.by_ref() {
                let stripped = following
                    .strip_prefix("fn ")
                    .or_else(|| following.strip_prefix("pub fn "));
                if let Some(rest) = stripped {
                    let name: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        found.insert(name);
                    }
                    break;
                }
                if following.starts_with("#[") || following.is_empty() {
                    continue;
                }
                break;
            }
        }
    }
    found
}

/// Everything in `godot/`, **with comments stripped**.
///
/// The stripping is what makes this mean anything. Without it a binding is
/// "reached" the moment somebody explains in a doc comment why it exists — and
/// this file's own module note names half a dozen of them, which would have made
/// the guard satisfy itself if it scanned prose.
///
/// Deliberately `godot/` alone. `exposure.rs` scans the shell as well, correctly:
/// it asks whether a simulation fact got *anywhere*. This asks whether it got all
/// the way, and a binding called only from Rust has not.
fn godot_sources() -> String {
    let mut parts = Vec::new();
    collect(&repo_root().join("godot"), &["gd", "tscn"], &mut parts);
    parts.join("\n")
}

/// Drop `#` line comments, so a binding named in prose does not read as one that
/// is wired up.
///
/// Crude in the same way and to the same degree as `exposure.rs`'s: a `#` inside
/// a GDScript string truncates that line. The failure direction is a false
/// *alarm*, which is the safe way round for this to be wrong.
fn strip_comments(text: &str) -> String {
    text.lines()
        .map(|line| match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect(dir: &Path, extensions: &[&str], out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // The import cache is generated, and would let this pass on stale
            // copies of files that no longer exist.
            if path.file_name().is_some_and(|n| n == ".godot") {
                continue;
            }
            collect(&path, extensions, out);
            continue;
        }
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !extensions.contains(&extension) {
            continue;
        }
        // This test's own source must not count: it names bindings in its
        // module note, which would make it satisfy itself.
        if path.file_name().is_some_and(|n| n == "reachable.rs") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            out.push(if extension == "rs" {
                text
            } else {
                strip_comments(&text)
            });
        }
    }
}

#[test]
fn every_binding_the_shell_registers_is_called_from_the_game() {
    let bindings = bindings();

    assert!(
        bindings.len() >= FLOOR,
        "only {} bindings were found on the shell, below the floor of {FLOOR}. \
         The parse has almost certainly broken rather than the shell having \
         shrunk, and a scan that finds nothing reports success for everything.",
        bindings.len()
    );

    let sources = godot_sources();
    let exempt: BTreeSet<&str> = NOT_FOR_GODOT
        .iter()
        .chain(NOT_YET_REACHED)
        .map(|(name, _)| *name)
        .collect();

    let mut orphans: Vec<&str> = Vec::new();
    for binding in &bindings {
        if exempt.contains(binding.as_str()) {
            continue;
        }
        if !sources.contains(binding.as_str()) {
            orphans.push(binding);
        }
    }

    println!(
        "reachable: {} bindings on the shell, {} with no screen yet, {} orphaned",
        bindings.len(),
        exempt.len(),
        orphans.len()
    );

    assert!(
        orphans.is_empty(),
        "the shell offers these and no screen calls them: {}\n\n\
         A binding nobody calls is a feature nobody has, and this is the exact \
         shape the build menu, road laying and the building inspector were all \
         missing in — the simulation was finished and the player could not \
         reach it. Either give it a screen, or add it to NOT_FOR_GODOT with the \
         reason it is not something a player needs.",
        orphans.join(", ")
    );
}

/// An exemption may not outlive its subject, in either of the two ways it can.
///
/// The same pair of assertions `exposure.rs` makes, for the same reason. A claim
/// that has become false should fail even when it has become false in the *safe*
/// direction: a binding exempted as "not for Godot" and then wired up leaves an
/// entry that would keep this test quiet if the screen calling it were ever
/// removed.
#[test]
fn no_exemption_outlives_what_it_excuses() {
    let bindings = bindings();

    let stale: Vec<&str> = NOT_FOR_GODOT
        .iter()
        .chain(NOT_YET_REACHED)
        .map(|(name, _)| *name)
        .filter(|name| !bindings.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "these exemptions name bindings that no longer exist: {}. \
         An exemption for something gone is a hole nobody is watching.",
        stale.join(", ")
    );

    let sources = godot_sources();
    let wired: Vec<&str> = NOT_FOR_GODOT
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| sources.contains(*name))
        .collect();
    assert!(
        wired.is_empty(),
        "these bindings are listed as not for Godot and the game calls them \
         anyway: {}. Delete the exemption so they are checked like every other \
         one.",
        wired.join(", ")
    );
}
