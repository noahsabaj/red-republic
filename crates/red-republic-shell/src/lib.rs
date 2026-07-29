//! The Godot shell: the only Rust in this repository that knows an engine
//! exists.
//!
//! # What this crate may and may not do
//!
//! It may render, take input, and translate between Godot's types and the
//! simulation's. It may **not** contain simulation. The test is blunt: if a
//! number here would change how a republic turns out, it is in the wrong crate.
//!
//! It depends on `red-republic-sim` with no features, which is not incidental.
//! The `fixtures` feature is what lets the benchmark harness stand buildings up
//! and write to the population directly; a shell that enabled it would be
//! outside the single-writer rule, and the dependency line in `Cargo.toml` is
//! what stops that rather than a comment asking nicely.
//!
//! # The marshalling rule, which was measured
//!
//! At 1,205 buildings, handing the UI one `Dictionary` per entity cost
//! **8,640 µs** — 52% of a 16.7 ms frame. The same data as a flat
//! `PackedFloat32Array` cost **27 µs**. That is 316× apart, and it is why every
//! bulk read in [`marshal`] returns a packed array with a documented stride
//! rather than the idiomatic Godot shape.
//!
//! Raw FFI overhead is 0.21 µs, so a chatty *small* interface is free. The rule
//! is about bulk, not about call count: many small calls are fine, one fat
//! structured call is not.

use godot::prelude::*;

pub mod marshal;
pub mod republic;
pub mod shelf;
pub mod terrain_mesh;
pub mod views;

struct RedRepublicShell;

#[gdextension]
unsafe impl ExtensionLibrary for RedRepublicShell {}
