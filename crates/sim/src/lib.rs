//! The Red Republic simulation.
//!
//! Headless and engine-independent by construction: this crate has no
//! renderer, no windowing, and no engine dependency, and it must stay that way.
//! That independence is the whole point — it is what lets the shell be chosen
//! on measured evidence once the simulation exists, instead of guessed at now.
//!
//! # Determinism
//!
//! Same seed and same inputs must produce the same world, and a save must
//! reload bit-exact. The archived 1.x build proved this discipline is holdable
//! and that a round-trip test is what catches every missed field; this crate
//! inherits both the requirement and the tripwire.
//!
//! Two different bars, deliberately:
//!
//! - **Map generation** must reproduce across machines, because a shared seed
//!   is a promise between players.
//! - **The running simulation** need only reproduce for the same binary, which
//!   is the easy case: Rust does not reassociate floats, so one compiler and
//!   one instruction set is enough.

#![forbid(unsafe_code)]

pub mod rng;
pub mod time;
pub mod units;
pub mod world;

pub use rng::{Rng, RngState};
pub use time::{Date, Season, SimClock};
pub use units::{Metres, Point, Seconds, Speed};
pub use world::{SAVE_VERSION, Save, SaveError, World};

#[cfg(test)]
mod tests {
    /// Guards the float half of the determinism claim above.
    ///
    /// IEEE-754 addition is not associative, and Rust must not pretend it is.
    /// If anything ever enables fast-math-style reassociation — a compiler
    /// flag, a codegen option, a `_fast` intrinsic — these two expressions
    /// collapse to the same value and this test fails. That is the point: the
    /// invariant is stated in the module docs, and this is what stops the
    /// statement from quietly becoming untrue.
    ///
    /// 1e16 has a ulp of 2, so `1e16 + 1.0` rounds back to `1e16`, and the
    /// order the three terms are grouped in decides whether the 1.0 survives.
    #[test]
    fn float_addition_is_not_reassociated() {
        let (a, b, c) = (1e16_f64, -1e16_f64, 1.0_f64);
        assert_eq!((a + b) + c, 1.0, "left grouping must keep the 1.0");
        assert_eq!(a + (b + c), 0.0, "right grouping must round the 1.0 away");
    }
}
