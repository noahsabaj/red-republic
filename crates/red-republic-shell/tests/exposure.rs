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

/// Public functions on `World` that are **not views at all**.
///
/// Constructors, the clock, the write path, the save path. Exempting these is a
/// statement about what they are rather than a claim that the player cannot see
/// them, so nothing here is expected to be absent from the shell — several are
/// called by it, and that is neither required nor a problem.
///
/// Kept apart from [`HIDDEN`] because the two lists rot differently, and one
/// check cannot cover both. See [`no_exemption_outlives_what_it_excuses`].
const NOT_A_VIEW: &[(&str, &str)] = &[
    ("new", "a constructor"),
    ("issue", "the write path, not a view"),
    ("tick", "the clock, not a view"),
    (
        "to_save",
        "the save path — exercised by save/load rather than displayed",
    ),
    ("from_save", "as to_save"),
    ("to_bytes", "as to_save"),
    ("from_bytes", "as to_save"),
    (
        "preview_from_bytes",
        "as to_save — it is how a save is listed",
    ),
];

/// Views the player is **deliberately** not shown, each with the reason.
///
/// A list like this is exactly the smell the project's own rules warn about, so
/// it is kept short and every entry carries why. Unlike [`NOT_A_VIEW`], an entry
/// here is a claim that the shell does *not* reach this — and that claim is
/// checked in both directions.
const HIDDEN: &[(&str, &str)] = &[
    (
        "substream",
        "a determinism primitive; the player sees its consequences, never it",
    ),
    (
        "rng_state",
        "inspection and tests only — a cursor into a stream has no player meaning",
    ),
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

/// Everything the shell and the Godot project are made of, **with comments
/// stripped**.
///
/// The stripping is not tidiness. Without it a view is "exposed" the moment
/// somebody mentions it in a doc comment, which is the exact false pass the
/// module docs above admit to — and it made the reverse check unusable, because
/// `substream` is named in one sentence of `views.rs` explaining why a forecast
/// is free, and that sentence is worth keeping.
///
/// What it still cannot do is tell a call from a string literal or an identifier
/// that merely contains the name. `preview` is satisfied today by
/// `save_preview`, which happens to be the right answer for the right reason,
/// and would not have been if it were an accident. This is a source scan; it
/// catches a view wired to nothing, which is the case that actually happens.
fn shell_sources() -> String {
    let root = repo_root();
    let mut text = String::new();
    for dir in ["crates/red-republic-shell/src", "godot"] {
        collect(&root.join(dir), &mut text);
    }
    text
}

/// Drop comments, so a view named in prose does not read as a view that is wired
/// up.
///
/// Deliberately crude: line comments for both languages and Rust block comments,
/// with no attempt to respect string literals. A `#` inside a GDScript string —
/// a colour literal — truncates that line, and a view name appearing *after* one
/// on the same line would be missed. Judged acceptable because the alternative
/// is a tokeniser for two languages inside a test, and because the failure
/// direction is a false *alarm* on the main check, which is the safe way round
/// for this one to be wrong.
fn strip_comments(text: &str, line_marker: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Rust block comments first: they can span lines, so line-by-line cannot see
    // them. Not nested-aware, which Rust permits and this codebase does not use.
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);

    out.lines()
        .map(|line| match line.find(line_marker) {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        } else {
            // GDScript and scene files comment with `#`; Rust and the shader
            // with `//`. Anything else is not scanned at all.
            let marker = match path.extension().and_then(|e| e.to_str()) {
                Some("rs") | Some("gdshader") => "//",
                Some("gd") | Some("tscn") => "#",
                _ => continue,
            };
            // This test's own source must not count: it names every view, which
            // would make it satisfy itself.
            if path.file_name().is_some_and(|n| n == "exposure.rs") {
                continue;
            }
            if let Ok(text) = fs::read_to_string(&path) {
                out.push_str(&strip_comments(&text, marker));
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
    let exempt = exemptions();

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

/// Every name in either list, for the main check.
fn exemptions() -> BTreeSet<&'static str> {
    NOT_A_VIEW
        .iter()
        .chain(HIDDEN)
        .map(|(name, _)| *name)
        .collect()
}

/// An exemption may not outlive its subject, in either of the two ways it can.
///
/// # Why this is two assertions and not one
///
/// The first is the one that was already here: an exemption naming a function
/// that no longer exists excuses nothing and hides the fact that nobody is
/// watching that slot any more.
///
/// The second is the half that was missing, and it was found by tripping over
/// it. `to_bytes` and `from_bytes` were exempted on the grounds that a save is
/// "exercised rather than displayed" — then save and load were built, the shell
/// started calling both, and the exemptions sat there excusing something that no
/// longer needed excusing. That is harmless the day it happens and not harmless
/// later: if the save UI is ever removed, the exemption keeps this test quiet
/// about it. An exemption is a claim, and a claim that has become false should
/// fail even when it has become false in the *safe* direction.
///
/// It applies only to [`HIDDEN`], which is why the two lists are separate.
/// [`NOT_A_VIEW`] makes no claim about visibility — a constructor being called
/// by the shell is not a surprise — and asserting the same thing about both
/// would have forced `new`, `tick` and `issue` out of a list they belong in.
#[test]
fn no_exemption_outlives_what_it_excuses() {
    let views = views();

    let stale: Vec<&str> = NOT_A_VIEW
        .iter()
        .chain(HIDDEN)
        .map(|(name, _)| *name)
        .filter(|name| !views.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "these exemptions name functions that no longer exist on World: {}. \
         An exemption for something gone is a hole nobody is watching.",
        stale.join(", ")
    );

    let sources = shell_sources();
    let surfaced: Vec<&str> = HIDDEN
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| sources.contains(*name))
        .collect();
    assert!(
        surfaced.is_empty(),
        "these views are listed as deliberately hidden and the shell reads them \
         anyway: {}. Either the exemption is obsolete and should be deleted so \
         the view is checked like every other one, or something is reading what \
         it was decided the player should not see.",
        surfaced.join(", ")
    );
}
