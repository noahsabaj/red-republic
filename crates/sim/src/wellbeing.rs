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
    /// Whether the bins are emptied and the air is breathable.
    ///
    /// One component for two things that a resident cannot tell apart: rubbish
    /// piling up in the yard because nobody drove it to a landfill, and smoke
    /// from a works upwind. Both are "this is not a pleasant place to live",
    /// both are the player's to fix, and splitting them would make the panel
    /// longer without making a decision clearer.
    pub cleanliness: f64,
    /// Fire, police and courts within reach. See [`crate::building::Need`].
    pub safety: f64,
    /// Drink and household electrics off a shelf within reach, `0.0..=1.0`.
    ///
    /// **Deliberately not one of the weighted components above, and that is the
    /// whole design.** Everything in [`Contentment::parts`] is a way for a
    /// republic to *fail* its people: absent, it costs. Comforts are the
    /// opposite — they are worth having and nobody's life is ruined without
    /// them — so they are applied as a **lift on top** in
    /// [`Contentment::overall`] rather than as a ninth thing to be short of.
    ///
    /// That distinction is what makes them addable at all. Modelled as a want
    /// they would have dropped the score of every republic already standing, on
    /// the day the goods were invented, for a shortfall that did not exist the
    /// day before — and a republic must never be re-marked for work it did
    /// before the rules changed.
    ///
    /// It is also why [`Contentment::worst`] cannot name this: "your people's
    /// biggest problem is no television" is not something a panel should ever
    /// say to somebody whose estate is cold.
    pub comforts: f64,
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
        cleanliness: 0.0,
        safety: 0.0,
        comforts: 0.0,
    };

    /// The most that fully-stocked comforts add to a home's score.
    ///
    /// **It certainly helps and it is not a dealbreaker**, which is the whole
    /// brief. Twelve points is enough to carry a republic sitting just under
    /// [`CONTENT_ATTRACTS`] over it — so a distillery and an electronics works
    /// are a real way to start attracting people — and nowhere near enough to
    /// rescue one that is cold, hungry or out of work.
    pub const COMFORT_LIFT: f64 = 0.12;

    /// What each component is worth, in the same order as [`Contentment::parts`].
    ///
    /// Food and warmth dominate because they are survival and the rest are
    /// quality of life. They are authored rather than buried in `overall` so
    /// that rebalancing what the republic is judged on is a data edit.
    pub const WEIGHTS: [f64; 8] = [3.0, 2.0, 1.0, 0.75, 0.75, 1.5, 1.0, 0.9];

    /// The components in a fixed order, each with the name a panel prints.
    ///
    /// Iteration order is part of the simulation's definition here: the shell
    /// reads these into a packed array and labels them by index.
    pub const NAMES: [&'static str; 8] = [
        "Provisions",
        "Warmth",
        "Health",
        "Culture",
        "Schooling",
        "Work",
        "Cleanliness",
        "Safety",
    ];

    pub fn parts(&self) -> [f64; Self::NAMES.len()] {
        [
            self.provisions,
            self.warmth,
            self.health,
            self.culture,
            self.schooling,
            self.work,
            self.cleanliness,
            self.safety,
        ]
    }

    /// The weighted mean of the needs, before comforts are added.
    ///
    /// Kept separate from [`Contentment::overall`] so a panel can show what the
    /// republic is doing about the things that matter and what the extras are
    /// worth on top, rather than one number that mixes the two.
    pub fn needs_met(&self) -> f64 {
        let total: f64 = Self::WEIGHTS.iter().sum();
        let scored: f64 = self
            .parts()
            .iter()
            .zip(Self::WEIGHTS)
            .map(|(v, w)| v.clamp(0.0, 1.0) * w)
            .sum();
        (scored / total).clamp(0.0, 1.0)
    }

    /// What the comforts are adding today, `0.0..=COMFORT_LIFT`.
    pub fn lift(&self) -> f64 {
        Self::COMFORT_LIFT * self.comforts.clamp(0.0, 1.0)
    }

    /// The weighted mean of the needs, lifted by whatever comforts reached the
    /// shelves, `0.0..=1.0`.
    ///
    /// **Additive rather than weighted**, so a republic that has never heard of
    /// vodka scores exactly what it scored before either good existed. See
    /// [`Contentment::comforts`] for why that is load-bearing rather than tidy.
    pub fn overall(&self) -> f64 {
        (self.needs_met() + self.lift()).clamp(0.0, 1.0)
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

/// What a fully-stocked republic's drinking takes off its people's health.
///
/// **The trade the player is being asked to think about.** Drink lifts a home's
/// contentment and it costs the people in it something, and both halves are
/// real: at full supply a citizen with a clinic next door targets 0.90 health
/// rather than 1.00, which tells on mortality without being a catastrophe.
///
/// It is scaled by what the shops in reach actually *had*, so a republic that
/// makes no alcohol pays nothing and one that supplies it everywhere pays in
/// full — and a player who decides the contentment is worth the health is
/// making exactly the decision this is for.
///
/// Electronics have no such cost, which is authored here by their absence and
/// worth stating: a television is not bad for you.
pub const ALCOHOL_HEALTH_COST: f64 = 0.10;

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
            cleanliness: 1.0,
            safety: 1.0,
            comforts: 0.0,
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
            cleanliness: 1.0,
            safety: 1.0,
            comforts: 0.0,
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

    /// **Comforts may only ever add.** This is the guard the whole design exists
    /// to make true: a republic that has never built a distillery must score
    /// exactly what it scored before drink and electrics were invented, or the
    /// act of deepening the economy silently re-marks work the player already
    /// did.
    ///
    /// Checked across the whole range rather than at a point, because a
    /// weighted-component implementation would pass a spot check at zero and
    /// fail everywhere else.
    #[test]
    fn comforts_never_lower_a_score_and_a_full_shelf_lifts_it_by_the_stated_amount() {
        for tenth in 0..=10 {
            let base = Contentment {
                provisions: f64::from(tenth) / 10.0,
                warmth: 0.8,
                health: 0.7,
                culture: 0.3,
                schooling: 1.0,
                work: 0.9,
                cleanliness: 0.6,
                safety: 0.5,
                comforts: 0.0,
            };
            let comforted = Contentment {
                comforts: 1.0,
                ..base
            };
            assert!(
                comforted.overall() >= base.overall(),
                "comforts made a republic worse off at provisions {tenth}/10"
            );
            // And the needs half is untouched by them, which is what lets a
            // panel show the two apart.
            assert_eq!(base.needs_met(), comforted.needs_met());
            let expected = (base.overall() + Contentment::COMFORT_LIFT).min(1.0);
            assert!(
                (comforted.overall() - expected).abs() < 1e-12,
                "a full shelf lifted {:.3} rather than {:.3}",
                comforted.overall() - base.overall(),
                Contentment::COMFORT_LIFT
            );
        }
    }

    /// A comfort is never the thing a panel tells you to go and fix.
    ///
    /// "Your people's biggest problem is no television" is not something to say
    /// to somebody whose estate is cold, and `worst` reads `parts` — so this
    /// also pins comforts out of that roster.
    #[test]
    fn no_estate_is_ever_told_its_worst_problem_is_a_missing_luxury() {
        let cold_and_dry = Contentment {
            provisions: 1.0,
            warmth: 0.2,
            health: 1.0,
            culture: 1.0,
            schooling: 1.0,
            work: 1.0,
            cleanliness: 1.0,
            safety: 1.0,
            comforts: 0.0,
        };
        assert_eq!(cold_and_dry.worst(), Some("Warmth"));
        assert!(
            !Contentment::NAMES.contains(&"Comforts"),
            "comforts joined the weighted roster, which makes them a way to fail"
        );
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
            cleanliness: 1.0,
            safety: 1.0,
            comforts: 0.0,
        };
        assert!((over.overall() - 1.0).abs() < 1e-12);
    }
}
