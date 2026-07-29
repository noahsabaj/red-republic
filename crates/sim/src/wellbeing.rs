//! How well the republic serves the people living in it, and what that costs
//! when it fails.
//!
//! # Why this module exists at all
//!
//! `Building::provisioned` and `Building::heated` have been computed every tick
//! since the households and heating systems landed, and **nothing read either
//! one**. A republic could starve its estates and freeze them and lose nothing
//! by it. Contentment is what those two were waiting for: it is the first thing
//! in this simulation that pushes back on the player, because a republic that
//! fails its people loses them and one that serves them attracts more.
//!
//! # A score nobody can argue with is a score nobody can act on
//!
//! [`Contentment`] is a **breakdown, not a number**. Every component is named,
//! every one is `0.0..=1.0`, and [`Contentment::overall`] is the weighted mean
//! of them. That is the project's standing rule about systems explaining
//! themselves applied to the one number the player will look at most: "your
//! people are at 61%" is useless, and "fed, warm, no doctor, no work" is a
//! decision.
//!
//! The weights are authored here rather than folded into the arithmetic, so
//! changing what the republic is judged on is a data edit and a test can read
//! them.

use serde::{Deserialize, Serialize};

/// How far people will walk to a service.
///
/// The same reach the shops already use: a clinic or a school you cannot walk
/// to is a clinic or a school somebody else's estate has.
pub use crate::systems::SERVICE_RADIUS;

/// What a home offers the people in it, component by component.
///
/// Each field is `0.0..=1.0`. They are deliberately not folded together on the
/// way in — the whole point is that a panel can say *which* one is short.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Contentment {
    /// Food and clothes actually taken off a shelf within reach. Read straight
    /// off `Building::provisioned`, which the households system already writes.
    pub provisions: f64,
    /// Warmth, on a day cold enough for it to mean anything.
    ///
    /// **One on a warm day, always.** Heating demand follows today's
    /// temperature and never the month, so a July estate is not unhappy about
    /// a boiler nobody has lit — it simply is not being asked.
    pub warmth: f64,
    /// A staffed clinic within reach.
    pub health: f64,
    /// Somewhere to go that is not work.
    pub culture: f64,
    /// A school within reach, weighted by how many children live here.
    ///
    /// A block of pensioners is not unhappy about the lack of a school, and a
    /// block full of children very much is.
    pub schooling: f64,
    /// The share of working-age residents who hold a job.
    pub work: f64,
}

impl Contentment {
    /// A home nobody has done anything for.
    pub const NOTHING: Self = Self {
        provisions: 0.0,
        warmth: 0.0,
        health: 0.0,
        culture: 0.0,
        schooling: 0.0,
        work: 0.0,
    };

    /// What each component is worth, in the same order as [`Contentment::parts`].
    ///
    /// Food and warmth dominate because they are survival and the rest are
    /// quality of life. They are authored rather than buried in `overall` so
    /// that rebalancing what the republic is judged on is a data edit.
    pub const WEIGHTS: [f64; 6] = [3.0, 2.0, 1.0, 0.75, 0.75, 1.5];

    /// The components in a fixed order, each with the name a panel prints.
    ///
    /// Iteration order is part of the simulation's definition here: the shell
    /// reads these into a packed array and labels them by index.
    pub const NAMES: [&'static str; 6] = [
        "Provisions",
        "Warmth",
        "Health",
        "Culture",
        "Schooling",
        "Work",
    ];

    pub fn parts(&self) -> [f64; 6] {
        [
            self.provisions,
            self.warmth,
            self.health,
            self.culture,
            self.schooling,
            self.work,
        ]
    }

    /// The weighted mean, `0.0..=1.0`.
    pub fn overall(&self) -> f64 {
        let total: f64 = Self::WEIGHTS.iter().sum();
        let scored: f64 = self
            .parts()
            .iter()
            .zip(Self::WEIGHTS)
            .map(|(v, w)| v.clamp(0.0, 1.0) * w)
            .sum();
        (scored / total).clamp(0.0, 1.0)
    }

    /// The component that is dragging this home down hardest — value times
    /// weight, so a small shortfall in food outranks a total absence of cinema.
    ///
    /// This is what a panel names when the player asks why an estate is
    /// unhappy, and it is why the breakdown is stored rather than a score.
    pub fn worst(&self) -> Option<&'static str> {
        let mut ranked: Vec<(f64, usize)> = self
            .parts()
            .iter()
            .zip(Self::WEIGHTS)
            .enumerate()
            .map(|(i, (v, w))| ((1.0 - v.clamp(0.0, 1.0)) * w, i))
            .filter(|(loss, _)| *loss > 1e-9)
            .collect();
        ranked.sort_by(|(la, ia), (lb, ib)| lb.total_cmp(la).then_with(|| ia.cmp(ib)));
        ranked.first().map(|&(_, i)| Self::NAMES[i])
    }
}

impl Default for Contentment {
    fn default() -> Self {
        Self::NOTHING
    }
}

/// How fast a citizen's own feeling about the republic follows the home they
/// live in, per day.
///
/// Slow on purpose. Loyalty is what decides whether somebody packs up and
/// leaves, and a republic that could lose a third of its people to one bad week
/// of coal supply would be a republic nobody could take a risk in. A month of
/// failure should cost you people; a Tuesday should not.
pub const LOYALTY_DRIFT: f64 = 0.02;

/// Below this, people start leaving.
pub const LOYALTY_LEAVES: f64 = 0.35;

/// The chance per day that a citizen at zero loyalty emigrates.
///
/// Scaled by how far below [`LOYALTY_LEAVES`] they are, so the first person
/// out is a trickle and a collapse is a collapse.
pub const EMIGRATION_ODDS: f64 = 0.02;

/// How fast health follows the medical care available, per day.
pub const HEALTH_DRIFT: f64 = 0.01;

/// The health a citizen tends toward with no clinic in reach.
///
/// Not zero: people do not die of having no polyclinic, they are simply less
/// robust when they are old. This is the floor the drift pulls toward when
/// nothing is provided, and it is what makes a clinic worth building rather
/// than mandatory.
pub const HEALTH_UNSERVED: f64 = 0.55;

/// Above this republic-wide contentment, people want to come.
pub const CONTENT_ATTRACTS: f64 = 0.6;

/// The largest group that will arrive at a post at once.
///
/// A busload, near enough, and deliberately bounded: a republic that has just
/// built its first estate should get settlers as a stream it can absorb rather
/// than a crowd standing at the border with nothing to fetch them.
pub const ARRIVAL_PARTY: u32 = 24;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_republic_that_does_everything_scores_one_and_one_that_does_nothing_scores_zero() {
        let all = Contentment {
            provisions: 1.0,
            warmth: 1.0,
            health: 1.0,
            culture: 1.0,
            schooling: 1.0,
            work: 1.0,
        };
        assert!((all.overall() - 1.0).abs() < 1e-12);
        assert!(all.worst().is_none(), "nothing is short");
        assert_eq!(Contentment::NOTHING.overall(), 0.0);
    }

    /// The reason the breakdown is stored rather than a score: the answer to
    /// "why are they unhappy" has to be the thing that actually costs most,
    /// not the smallest number.
    #[test]
    fn the_worst_part_is_weighted_and_not_merely_the_lowest_number() {
        let hungry = Contentment {
            provisions: 0.8,
            warmth: 1.0,
            health: 1.0,
            // Absent entirely, but it is worth a quarter of what food is.
            culture: 0.0,
            schooling: 1.0,
            work: 1.0,
        };
        // 0.2 x 3.0 = 0.6 against 1.0 x 0.75 = 0.75, so culture wins here...
        assert_eq!(hungry.worst(), Some("Culture"));

        let hungrier = Contentment {
            provisions: 0.5,
            ..hungry
        };
        // ...and at half rations food does: 0.5 x 3.0 = 1.5.
        assert_eq!(hungrier.worst(), Some("Provisions"));
    }

    #[test]
    fn every_component_is_named_and_weighted() {
        assert_eq!(Contentment::NAMES.len(), Contentment::WEIGHTS.len());
        assert_eq!(Contentment::NAMES.len(), Contentment::NOTHING.parts().len());
        assert!(
            Contentment::WEIGHTS.iter().all(|w| *w > 0.0),
            "a component worth nothing is a component that should not be here"
        );
    }

    /// Clamping, because `provisioned` is a ratio that a rounding tail can push
    /// a hair over one and a score above 100% would print as one.
    #[test]
    fn a_component_over_one_does_not_push_the_score_over_one() {
        let over = Contentment {
            provisions: 1.5,
            warmth: 1.0,
            health: 1.0,
            culture: 1.0,
            schooling: 1.0,
            work: 1.0,
        };
        assert!((over.overall() - 1.0).abs() < 1e-12);
    }
}
