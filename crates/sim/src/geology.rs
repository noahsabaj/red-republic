//! What lies under the ground.
//!
//! # The model
//!
//! A **deposit is a body, not a tile and not a building.** It occupies a real
//! volume beneath the surface: a horizontal extent on the ground plane, a depth
//! at which it begins, and a stack of [`Layer`]s going down. Each layer holds a
//! finite tonnage.
//!
//! A mine is an ordinary building that **taps** a deposit. It does not own a
//! patch of it and its footprint does not bound what it can reach — one rig
//! over an oil body draws on the whole body. What decides the rate is how much
//! machinery is pointed at it, and pointing more at it depletes it faster.
//! That is the whole shape of the mechanic: siting is a question of *whether*
//! you can reach a deposit, and scale is a separate question of *how hard* you
//! work it.
//!
//! # Depth is the cost
//!
//! Extraction always works the shallowest layer that still has anything in it.
//! When a layer runs out the working depth steps down to the next one, and
//! everything about the operation gets more expensive — [`Deposit::depth_cost_multiplier`]
//! is the number the rest of the simulation multiplies by. So a deposit does
//! not fail suddenly; it gets steadily dearer to work, and at some point the
//! player decides it is no longer worth it. That decision, rather than an
//! exhaustion event, is what makes a mining town die.
//!
//! # Nothing here is visible from the surface
//!
//! There are no ore tiles. The player reads geology through the survey — see
//! [`Deposit::survey`] — and the same reading is what the founding screen's
//! candidate cards report, because Moscow surveys a posting before assigning
//! it. The overlay and the shelf must never compute this differently; there is
//! one reading and both render it.
//!
//! # The numbers are placeholders
//!
//! Extents, thicknesses and tonnages here are structure, not balance. They are
//! deliberately not tuned: the plan calls for a headless scratch runner to
//! derive a campaign trajectory, and that is what should set them.

use crate::units::{Metres, Point, Tonnes};
use serde::{Deserialize, Serialize};

/// What can be underground.
///
/// Groundwater is a peer from the start rather than a later addition, because
/// retrofitting a resource into a model that assumed all of them were ores is
/// how you end up with wells that are a special case forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mineral {
    Coal,
    IronOre,
    Oil,
    Gravel,
    Groundwater,
}

impl Mineral {
    /// Every mineral, in a fixed order.
    ///
    /// Fixed order matters: anything that iterates minerals while drawing from
    /// the simulation stream would otherwise produce a different world per run.
    pub const ALL: [Mineral; 5] = [
        Mineral::Coal,
        Mineral::IronOre,
        Mineral::Oil,
        Mineral::Gravel,
        Mineral::Groundwater,
    ];

    /// Whether the body refills. Only groundwater does — an aquifer recharges,
    /// a coal seam does not.
    pub fn recharges(self) -> bool {
        matches!(self, Mineral::Groundwater)
    }
}

/// A stable handle to a deposit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DepositId(pub u32);

/// One horizon within a deposit. Layers are ordered shallowest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// How thick this horizon is.
    pub thickness: Metres,
    /// What is left in it.
    pub remaining: Tonnes,
    /// What it held when the map was made — the denominator for "how worked
    /// out is this?", which the survey reports and the player reads.
    pub initial: Tonnes,
}

impl Layer {
    pub fn new(thickness: Metres, tonnes: Tonnes) -> Self {
        Self {
            thickness,
            remaining: tonnes,
            initial: tonnes,
        }
    }

    pub fn is_worked_out(&self) -> bool {
        !self.remaining.is_positive()
    }
}

/// A body of mineral under the ground.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deposit {
    pub id: DepositId,
    pub mineral: Mineral,
    /// Centre of the horizontal extent.
    pub centre: Point,
    /// Horizontal reach from the centre. A disc for now — real bodies are not
    /// discs, and a richer footprint is a later refinement that changes only
    /// [`Deposit::covers`].
    pub radius: Metres,
    /// Depth below the surface at which the body begins.
    pub top: Metres,
    /// Horizons, shallowest first.
    pub layers: Vec<Layer>,
}

/// What extraction actually got, and how deep it had to work for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Yield {
    pub tonnes: Tonnes,
    /// Depth the material came from — what the cost of getting it scales with.
    pub depth: Metres,
    /// The multiplier that depth implies, carried alongside so callers do not
    /// each re-derive it and drift apart.
    pub cost_multiplier: f64,
}

/// What a survey tells the player about one body.
///
/// A reading, not the body itself: it says what is there and how worked out it
/// is, and deliberately says nothing the player has no way to know.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurveyReading {
    pub id: DepositId,
    pub mineral: Mineral,
    pub centre: Point,
    pub radius: Metres,
    pub top: Metres,
    pub remaining: Tonnes,
    pub initial: Tonnes,
    /// Depth extraction is currently working at.
    pub working_depth: Metres,
}

impl SurveyReading {
    /// Fraction of the original body still in the ground, `0.0..=1.0`.
    pub fn fraction_remaining(&self) -> f64 {
        if self.initial.0 <= 0.0 {
            0.0
        } else {
            (self.remaining.0 / self.initial.0).clamp(0.0, 1.0)
        }
    }
}

/// How much dearer each metre of depth makes the work.
///
/// Placeholder, and shaped rather than merely scaled: cost rises with depth
/// but never becomes infinite, so a deposit always remains *technically*
/// workable and abandoning it stays the player's judgement rather than a wall
/// the simulation puts up.
pub const DEPTH_COST_PER_METRE: f64 = 0.004;

impl Deposit {
    pub fn new(
        id: DepositId,
        mineral: Mineral,
        centre: Point,
        radius: Metres,
        top: Metres,
        layers: Vec<Layer>,
    ) -> Self {
        Self {
            id,
            mineral,
            centre,
            radius,
            top,
            layers,
        }
    }

    /// Whether a point on the surface sits over this body — which is what
    /// decides whether a mine built there can tap it.
    pub fn covers(&self, point: Point) -> bool {
        self.centre.distance_to(point) <= self.radius
    }

    /// Everything still in the ground.
    pub fn remaining(&self) -> Tonnes {
        self.layers.iter().map(|l| l.remaining).sum()
    }

    /// Everything the body ever held.
    pub fn initial(&self) -> Tonnes {
        self.layers.iter().map(|l| l.initial).sum()
    }

    pub fn is_exhausted(&self) -> bool {
        !self.remaining().is_positive()
    }

    /// Index of the shallowest layer with anything left.
    fn live_layer(&self) -> Option<usize> {
        self.layers.iter().position(|l| !l.is_worked_out())
    }

    /// Depth work is happening at: the top of the shallowest live layer.
    ///
    /// An exhausted body reports the bottom of the whole thing, which is where
    /// work would be if there were anything left to do.
    pub fn working_depth(&self) -> Metres {
        let stop = self.live_layer().unwrap_or(self.layers.len());
        self.layers
            .iter()
            .take(stop)
            .fold(self.top, |depth, layer| depth + layer.thickness)
    }

    /// What working at the current depth multiplies costs by.
    pub fn depth_cost_multiplier(&self) -> f64 {
        1.0 + self.working_depth().0 * DEPTH_COST_PER_METRE
    }

    /// Take up to `wanted` tonnes, working down through the layers.
    ///
    /// Returns what was actually available, which may be less — and the depth
    /// it came from, since that is what the operation costs. A request that
    /// spans a layer boundary reports the depth it *finished* at, so the cost
    /// of a load reflects the deepest work it required rather than the
    /// cheapest.
    pub fn extract(&mut self, wanted: Tonnes) -> Yield {
        let mut got = Tonnes::ZERO;
        let mut outstanding = wanted;

        while outstanding.is_positive() {
            let Some(index) = self.live_layer() else {
                break;
            };
            let layer = &mut self.layers[index];
            let taken = outstanding.min(layer.remaining);
            layer.remaining = layer.remaining.saturating_sub(taken);
            got += taken;
            outstanding = outstanding.saturating_sub(taken);
        }

        let depth = self.working_depth();
        Yield {
            tonnes: got,
            depth,
            cost_multiplier: self.depth_cost_multiplier(),
        }
    }

    /// Put water back. Only ever called for a recharging mineral — an aquifer
    /// refills, and nothing else does.
    ///
    /// Recharge fills from the shallowest layer down and never exceeds what
    /// the horizon originally held, so a well cannot manufacture an aquifer
    /// larger than the ground it sits in.
    pub fn recharge(&mut self, amount: Tonnes) {
        debug_assert!(
            self.mineral.recharges(),
            "{:?} does not recharge",
            self.mineral
        );
        let mut spare = amount;
        for layer in &mut self.layers {
            if !spare.is_positive() {
                break;
            }
            let room = layer.initial.saturating_sub(layer.remaining);
            let added = spare.min(room);
            layer.remaining += added;
            spare = spare.saturating_sub(added);
        }
    }

    pub fn survey(&self) -> SurveyReading {
        SurveyReading {
            id: self.id,
            mineral: self.mineral,
            centre: self.centre,
            radius: self.radius,
            top: self.top,
            remaining: self.remaining(),
            initial: self.initial(),
            working_depth: self.working_depth(),
        }
    }
}

/// Every deposit under a map.
///
/// A `Vec` rather than a map keyed by id, on purpose: iteration order is part
/// of the simulation's definition, and a `HashMap` would randomise it per
/// process and quietly destroy reproducibility.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Geology {
    deposits: Vec<Deposit>,
}

impl Geology {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a body.
    ///
    /// # Panics
    /// In debug builds, if the id is already taken. `get`/`get_mut` return the
    /// first match, so a duplicate id silently makes one of the two bodies
    /// unreachable and the other one edited by accident.
    pub fn insert(&mut self, deposit: Deposit) {
        debug_assert!(
            self.get(deposit.id).is_none(),
            "deposit id {:?} is already taken",
            deposit.id
        );
        self.deposits.push(deposit);
    }

    pub fn get(&self, id: DepositId) -> Option<&Deposit> {
        self.deposits.iter().find(|d| d.id == id)
    }

    pub fn get_mut(&mut self, id: DepositId) -> Option<&mut Deposit> {
        self.deposits.iter_mut().find(|d| d.id == id)
    }

    pub fn all(&self) -> &[Deposit] {
        &self.deposits
    }

    /// Bodies a mine at this point could tap, shallowest first — so the
    /// obvious thing to reach for is the cheapest one to work.
    pub fn tappable_at(&self, point: Point) -> Vec<DepositId> {
        let mut hits: Vec<&Deposit> = self.deposits.iter().filter(|d| d.covers(point)).collect();
        hits.sort_by(|a, b| {
            a.top
                .0
                .partial_cmp(&b.top.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        hits.into_iter().map(|d| d.id).collect()
    }

    /// The survey the player reads. One reading, rendered by both the overlay
    /// and the founding screen's candidate cards — never recomputed by either.
    pub fn survey(&self) -> Vec<SurveyReading> {
        self.deposits.iter().map(Deposit::survey).collect()
    }

    /// Total still in the ground, by mineral — the summary a candidate card
    /// needs to say whether a posting is coal-rich or coal-poor.
    pub fn remaining_of(&self, mineral: Mineral) -> Tonnes {
        self.deposits
            .iter()
            .filter(|d| d.mineral == mineral)
            .map(Deposit::remaining)
            .sum()
    }

    /// Distance from a point to the nearest body of a mineral, measured to the
    /// edge of its extent — zero when standing over one.
    ///
    /// This is the single most decisive fact about a starting site, which is
    /// why it is engine-owned: the founding shelf must not compute its own
    /// version and drift from what the survey overlay shows in game.
    pub fn distance_to_nearest(&self, point: Point, mineral: Mineral) -> Option<Metres> {
        self.deposits
            .iter()
            .filter(|d| d.mineral == mineral && !d.is_exhausted())
            .map(|d| Metres((d.centre.distance_to(point) - d.radius).0.max(0.0)))
            .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seam(top_m: f64, layers: &[(f64, f64)]) -> Deposit {
        Deposit::new(
            DepositId(1),
            Mineral::Coal,
            Point::ORIGIN,
            Metres(400.0),
            Metres(top_m),
            layers
                .iter()
                .map(|&(thickness, tonnes)| Layer::new(Metres(thickness), Tonnes(tonnes)))
                .collect(),
        )
    }

    #[test]
    fn a_mine_over_the_body_taps_it_and_one_outside_does_not() {
        let d = seam(20.0, &[(10.0, 1_000.0)]);
        assert!(d.covers(Point::new(Metres(399.0), Metres(0.0))));
        assert!(!d.covers(Point::new(Metres(401.0), Metres(0.0))));
    }

    /// The mechanic in one test: a single small rig and a big one draw on the
    /// same body, and the only difference is how fast it goes.
    #[test]
    fn extraction_rate_is_the_only_difference_between_a_small_mine_and_a_large_one() {
        let mut small = seam(20.0, &[(10.0, 1_000.0)]);
        let mut large = seam(20.0, &[(10.0, 1_000.0)]);
        for _ in 0..10 {
            small.extract(Tonnes(1.0));
        }
        large.extract(Tonnes(10.0));
        assert_eq!(small.remaining(), large.remaining());
    }

    #[test]
    fn extraction_cannot_take_more_than_is_there() {
        let mut d = seam(20.0, &[(10.0, 50.0)]);
        let got = d.extract(Tonnes(500.0));
        assert_eq!(got.tonnes, Tonnes(50.0));
        assert!(d.is_exhausted());
        // And an exhausted body yields nothing rather than going negative.
        assert_eq!(d.extract(Tonnes(10.0)).tonnes, Tonnes::ZERO);
        assert_eq!(d.remaining(), Tonnes::ZERO);
    }

    #[test]
    fn working_out_a_layer_pushes_the_work_deeper_and_raises_the_cost() {
        let mut d = seam(20.0, &[(30.0, 100.0), (30.0, 100.0)]);
        assert_eq!(d.working_depth(), Metres(20.0));
        let shallow_cost = d.depth_cost_multiplier();

        d.extract(Tonnes(100.0)); // exactly finishes the first horizon
        assert_eq!(d.working_depth(), Metres(50.0), "top + first thickness");
        assert!(
            d.depth_cost_multiplier() > shallow_cost,
            "deeper work must cost more"
        );
    }

    #[test]
    fn a_draw_spanning_two_layers_is_priced_at_the_depth_it_finished_at() {
        let mut d = seam(0.0, &[(40.0, 10.0), (40.0, 100.0)]);
        let got = d.extract(Tonnes(30.0));
        assert_eq!(got.tonnes, Tonnes(30.0));
        // It had to break into the second horizon, so it is priced there.
        assert_eq!(got.depth, Metres(40.0));
        assert!((got.cost_multiplier - (1.0 + 40.0 * DEPTH_COST_PER_METRE)).abs() < 1e-12);
    }

    /// A deposit must never become unworkable on its own — the decision to
    /// walk away from a mining town belongs to the player, not to a wall the
    /// simulation puts up.
    #[test]
    fn depth_makes_work_dearer_but_never_impossible() {
        let deep = seam(2_000.0, &[(10.0, 1.0)]);
        let cost = deep.depth_cost_multiplier();
        assert!(cost > 1.0);
        assert!(cost.is_finite());
    }

    #[test]
    fn an_aquifer_refills_but_never_past_what_the_ground_held() {
        let mut water = Deposit::new(
            DepositId(2),
            Mineral::Groundwater,
            Point::ORIGIN,
            Metres(500.0),
            Metres(5.0),
            vec![Layer::new(Metres(20.0), Tonnes(100.0))],
        );
        water.extract(Tonnes(80.0));
        assert_eq!(water.remaining(), Tonnes(20.0));

        water.recharge(Tonnes(30.0));
        assert_eq!(water.remaining(), Tonnes(50.0));

        water.recharge(Tonnes(1_000.0));
        assert_eq!(
            water.remaining(),
            Tonnes(100.0),
            "recharge must not invent an aquifer bigger than the ground"
        );
    }

    #[test]
    fn only_groundwater_recharges() {
        assert!(Mineral::Groundwater.recharges());
        for m in Mineral::ALL {
            if m != Mineral::Groundwater {
                assert!(!m.recharges(), "{m:?} should not recharge");
            }
        }
    }

    #[test]
    fn tappable_bodies_come_back_shallowest_first() {
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::Coal,
            Point::ORIGIN,
            Metres(300.0),
            Metres(120.0),
            vec![Layer::new(Metres(10.0), Tonnes(500.0))],
        ));
        g.insert(Deposit::new(
            DepositId(2),
            Mineral::Gravel,
            Point::ORIGIN,
            Metres(300.0),
            Metres(3.0),
            vec![Layer::new(Metres(4.0), Tonnes(500.0))],
        ));
        assert_eq!(
            g.tappable_at(Point::ORIGIN),
            vec![DepositId(2), DepositId(1)],
            "the shallow gravel is the cheaper thing to reach"
        );
        assert!(
            g.tappable_at(Point::new(Metres(5_000.0), Metres(0.0)))
                .is_empty()
        );
    }

    #[test]
    fn distance_to_the_nearest_body_is_measured_to_its_edge() {
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::Coal,
            Point::new(Metres(1_000.0), Metres(0.0)),
            Metres(200.0),
            Metres(30.0),
            vec![Layer::new(Metres(10.0), Tonnes(500.0))],
        ));
        // 1000 m to the centre, 200 m of body — 800 m of walking.
        assert_eq!(
            g.distance_to_nearest(Point::ORIGIN, Mineral::Coal),
            Some(Metres(800.0))
        );
        // Standing over it is zero, not negative.
        assert_eq!(
            g.distance_to_nearest(Point::new(Metres(1_000.0), Metres(0.0)), Mineral::Coal),
            Some(Metres(0.0))
        );
        assert_eq!(g.distance_to_nearest(Point::ORIGIN, Mineral::Oil), None);
    }

    /// An exhausted body stops counting as somewhere to build — otherwise the
    /// founding shelf would advertise coal that is not there any more.
    #[test]
    fn a_worked_out_body_no_longer_counts_as_the_nearest() {
        let mut g = Geology::new();
        g.insert(Deposit::new(
            DepositId(1),
            Mineral::Coal,
            Point::ORIGIN,
            Metres(100.0),
            Metres(10.0),
            vec![Layer::new(Metres(5.0), Tonnes(10.0))],
        ));
        assert!(
            g.distance_to_nearest(Point::ORIGIN, Mineral::Coal)
                .is_some()
        );
        g.get_mut(DepositId(1)).unwrap().extract(Tonnes(10.0));
        assert!(
            g.distance_to_nearest(Point::ORIGIN, Mineral::Coal)
                .is_none()
        );
    }

    #[test]
    fn the_survey_reports_how_worked_out_a_body_is() {
        let mut d = seam(20.0, &[(30.0, 100.0), (30.0, 100.0)]);
        assert!((d.survey().fraction_remaining() - 1.0).abs() < 1e-12);
        d.extract(Tonnes(150.0));
        assert!((d.survey().fraction_remaining() - 0.25).abs() < 1e-12);
        assert_eq!(d.survey().working_depth, Metres(50.0));
    }

    #[test]
    fn totals_by_mineral_answer_whether_a_posting_is_rich() {
        let mut g = Geology::new();
        for (id, mineral, tonnes) in [
            (1u32, Mineral::Coal, 500.0),
            (2, Mineral::Coal, 300.0),
            (3, Mineral::IronOre, 200.0),
        ] {
            g.insert(Deposit::new(
                DepositId(id),
                mineral,
                Point::ORIGIN,
                Metres(100.0),
                Metres(10.0),
                vec![Layer::new(Metres(5.0), Tonnes(tonnes))],
            ));
        }
        assert_eq!(g.remaining_of(Mineral::Coal), Tonnes(800.0));
        assert_eq!(g.remaining_of(Mineral::IronOre), Tonnes(200.0));
        assert_eq!(g.remaining_of(Mineral::Oil), Tonnes::ZERO);
    }
}
