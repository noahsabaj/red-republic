//! The exposure guard: nothing the simulation knows may be invisible.
//!
//! This is the mechanism behind the first condition of the goal. That condition
//! used to read as a terminus — closable only once the simulation stopped
//! growing — which made it unreachable the moment feature parity was adopted.
//! It is now a per-change rule: **every simulation fact ships with its UI in the
//! same commit**. A rule nobody can check is a rule that decays, so this is the
//! check.
//!
//! # What it does
//!
//! Reads the public view methods off `World` in the simulation crate, then
//! looks for each one somewhere in the shell or the Godot project. A view the
//! player has no way to see is a fact the simulation knows and hides.
//!
//! # What it is not, stated plainly
//!
//! This is a **source scan**, and this repository has a memory of scans going
//! blind: one bound to a literal path stopped seeing its subject after a
//! refactor and reported success for four releases. Three things are done about
//! that, and they are the reason this is worth having rather than worth
//! trusting:
//!
//! 1. **It scans globs, never a fixed file list.** Adding a shell module or a
//!    `.gd` file needs no change here.
//! 2. **It has a floor tied to the real population.** If the parse ever stops
//!    finding views — a syntax change, a file move — the count collapses and
//!    the floor fails, rather than "no views found" being indistinguishable
//!    from "every view is exposed".
//! 3. **It reports the count.** A number that drops is visible in the log even
//!    when the test passes.
//!
//! What it genuinely cannot tell you is whether a view is exposed *well*.
//! Mentioning `cover_days` in a comment would satisfy it. It catches the case
//! that actually happens — a view added to the simulation and never wired to
//! anything — and not the case where somebody wires it badly.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Below this many discovered views, assume the parse broke rather than that
/// the simulation shrank. The real population was 25 when this was written.
const FLOOR: usize = 18;

/// Views that are deliberately not surfaced, each with the reason.
///
/// A list like this is exactly the smell the project's own rules warn about, so
/// it is kept short and every entry carries why. An entry that stops being true
/// is caught by the unused-exemption check at the bottom.
const EXEMPT: &[(&str, &str)] = &[
    (
        "seed",
        "shown on the founding screen as the map's identity, not as republic state",
    ),
    (
        "substream",
        "a determinism primitive; the player sees its consequences, never it",
    ),
    (
        "rng_state",
        "inspection and tests only — a cursor into a stream has no player meaning",
    ),
    (
        "to_save",
        "the save path, exercised by save/load rather than displayed",
    ),
    ("from_save", "as to_save"),
    ("to_bytes", "as to_save"),
    ("from_bytes", "as to_save"),
    ("new", "a constructor"),
    ("issue", "the write path, not a view"),
    ("tick", "the clock, not a view"),
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/red-republic-shell.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the shell crate sits two levels under the repository root")
        .to_path_buf()
}

/// Every `pub fn` on `World`, by name.
fn views() -> BTreeSet<String> {
    let source = fs::read_to_string(repo_root().join("crates/sim/src/world.rs"))
        .expect("the simulation's world module");
    // Only the inherent impl. Two things are cut off deliberately:
    //
    // - the test module, whose helpers are not player-facing;
    // - `world::fixtures`, which is behind a feature no renderer enables and
    //   exists so the benchmark harness can stand buildings up. Counting those
    //   as "views the player cannot see" is true and useless — the player is
    //   not supposed to see them, and neither is the shell.
    let body = source
        .split("#[cfg(feature = \"fixtures\")]")
        .next()
        .expect("there is always a first part")
        .split("#[cfg(test)]")
        .next()
        .expect("there is always a first part");

    let mut found = BTreeSet::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub fn ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }
    found
}

/// Everything the shell and the Godot project are made of.
fn shell_sources() -> String {
    let root = repo_root();
    let mut text = String::new();
    for dir in ["crates/red-republic-shell/src", "godot"] {
        collect(&root.join(dir), &mut text);
    }
    text
}

fn collect(dir: &Path, out: &mut String) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // The import cache is generated and would make this pass on stale
            // copies of files that no longer exist.
            if path.file_name().is_some_and(|n| n == ".godot") {
                continue;
            }
            collect(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e == "rs" || e == "gd" || e == "tscn" || e == "gdshader")
        {
            // This test's own source must not count: it names every view, which
            // would make it satisfy itself.
            if path.file_name().is_some_and(|n| n == "exposure.rs") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

#[test]
fn every_view_the_simulation_offers_reaches_the_player() {
    let views = views();

    assert!(
        views.len() >= FLOOR,
        "only {} public views were found on World, below the floor of {FLOOR}. \
         The parse has almost certainly broken rather than the simulation having \
         shrunk, and a scan that finds nothing reports success for everything.",
        views.len()
    );

    let sources = shell_sources();
    let exempt: BTreeSet<&str> = EXEMPT.iter().map(|(name, _)| *name).collect();

    let mut hidden: Vec<&str> = Vec::new();
    for view in &views {
        if exempt.contains(view.as_str()) {
            continue;
        }
        if !sources.contains(view.as_str()) {
            hidden.push(view);
        }
    }

    println!(
        "exposure: {} views on World, {} exempt, {} hidden",
        views.len(),
        exempt.len(),
        hidden.len()
    );

    assert!(
        hidden.is_empty(),
        "the simulation knows these and the player cannot see them: {}\n\n\
         Condition 1 of the goal is that nothing the simulation knows is \
         invisible, as a per-change rule: a new simulation fact ships with its \
         UI in the same commit. Either surface it, or add it to EXEMPT with the \
         reason it is not a thing a player should see.",
        hidden.join(", ")
    );
}

#[test]
fn no_exemption_outlives_the_view_it_excuses() {
    let views = views();
    let stale: Vec<&str> = EXEMPT
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !views.contains(*name))
        .collect();

    assert!(
        stale.is_empty(),
        "these exemptions name views that no longer exist: {}. \
         An exemption for something gone is a hole nobody is watching.",
        stale.join(", ")
    );
}
