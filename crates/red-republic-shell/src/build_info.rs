//! Which build this is.
//!
//! # There is one version number in this repository
//!
//! It is `workspace.package.version` in the root `Cargo.toml`, and everything
//! that needs to state a version derives it from there:
//!
//! * the main menu, through [`Build::version`] below — so what a player reads in
//!   the corner is the version of the simulation binary actually loaded, not a
//!   string somebody typed near it;
//! * the Windows executable's own version resource and the installer, both of
//!   which `tools/package.ps1` renders from `cargo metadata`.
//!
//! That is deliberate rather than tidy. A version typed in two places is a
//! version that will disagree with itself, and the copy that goes stale is
//! always the one nobody looks at until a player quotes it in a bug report. The
//! export preset and the installer script are **generated** for the same
//! reason — see `tools/package.ps1`, which is the only thing that ever writes
//! them.
//!
//! # Why a debug build says so
//!
//! `debug_assertions` is on in every build except a release one, and a
//! development build behaves differently in ways a bug report needs to know
//! about: `opt-level = 1`, assertions live, and vsync off. A screenshot of the
//! menu is often all there is to go on, so the marker goes where the screenshot
//! will catch it.

use godot::prelude::*;

/// What build this is. Static-only; there is nothing to instantiate.
#[derive(GodotClass)]
#[class(no_init)]
pub struct Build;

#[godot_api]
impl Build {
    /// The version of the loaded simulation binary, as the menu shows it.
    ///
    /// A release build gives the bare CalVer, `2026.7.0`; anything else appends
    /// the marker, because a development build is a different artifact and a bug
    /// report that cannot tell them apart is a bug report that wastes a round
    /// trip.
    #[func]
    fn version() -> GString {
        let mut line = String::from(env!("CARGO_PKG_VERSION"));
        if cfg!(debug_assertions) {
            line.push_str(" development build");
        }
        GString::from(&line)
    }

    /// The bare version, with no build marker — for anything that has to parse
    /// it rather than read it.
    #[func]
    fn semver() -> GString {
        GString::from(env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    /// The version is a version.
    ///
    /// Thin, and it earns its place by direction rather than by depth: this
    /// asserts the *shape* the installer and the executable's version resource
    /// both require. Windows `FILEVERSION` is four comma-separated integers, so
    /// `tools/package.ps1` appends a `.0` to the three-part CalVer — and a
    /// version like `2026.7.0-rc1` would render an invalid resource and fail the
    /// export with a message about the preset rather than about the version.
    /// Failing here instead names the actual cause.
    ///
    /// It does **not** catch the natural CalVer slip of writing the month as
    /// `2026.07.0`, and it does not need to: Cargo rejects that at manifest
    /// parse with `invalid leading zero in minor version number`, before this or
    /// anything else in the workspace gets to run. Checked, because the first
    /// version of this comment claimed the test covered it and it does not —
    /// `"07"` is non-empty and all digits, so it passes every assertion here.
    #[test]
    fn the_version_is_three_plain_numbers() {
        let version = env!("CARGO_PKG_VERSION");
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "the workspace version is {version:?}; the Windows version resource \
             needs three numeric parts, so a pre-release suffix needs \
             tools/package.ps1 taught how to render it first"
        );
        for part in parts {
            assert!(
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()),
                "the workspace version is {version:?}; {part:?} is not a number"
            );
        }
    }
}
