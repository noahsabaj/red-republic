//! Choosing your land: the shelf of candidate postings.
//!
//! Founding is not a dialog with a seed box. It is **choosing your land**, and
//! the first beat of it is a shelf of candidate maps derived from one master
//! seed, each shown with the few facts that actually decide a start. Moscow
//! surveyed the ground before assigning the posting, so the player sees what
//! the survey saw — no more, and no less.
//!
//! # Filters transform the seeds, they do not replace them
//!
//! Changing size or climate keeps the **same candidate seeds** and re-derives
//! them under the new filter. That is the whole reason the shelf is worth
//! building: flipping from plains to taiga shows what that does to land you
//! were already considering, rather than dealing you six strangers. Candidate
//! `n` is always candidate `n`.
//!
//! # The stats come from the engine, never from here
//!
//! Every figure on a card is read from [`crate::geology::Geology::survey`] and
//! the terrain, which are the same sources the in-game survey overlay reads.
//! A shelf that computed its own idea of "coal-rich" would eventually disagree
//! with the game it is advertising, and the player would be right to call that
//! a lie.
//!
//! # It is not free, and the number is measured
//!
//! Generating a candidate costs a full worldgen — **36 ms for a 10 km map**,
//! measured by `worldgen_cost` in the baselines. A shelf of six is about 220 ms,
//! which is fine for a screen the player opens once, and would not be fine for
//! something that regenerated on every keystroke. [`Shelf::derive`] therefore
//! generates on demand and hands back owned worlds the caller can keep.

use crate::climate::ClimateId;
use crate::geology::Mineral;
use crate::terrain::Surface;
use crate::trade::Market;
use crate::units::{Metres, Point, Tonnes};
use crate::world::{World, WorldSpec, derive};

/// Substream identifier for the candidate shelf.
///
/// Its own stream so that the shelf a master seed produces never shifts because
/// something else about worldgen changed.
pub const SHELF_STREAM: u64 = 11;

/// How many candidates a shelf offers.
///
/// Six: enough that the choice is real, few enough that every card can carry
/// its picture and its numbers without a scrollbar.
pub const SHELF_SIZE: usize = 6;

/// The map sizes on offer, smallest first.
///
/// Named in kilometres because that is what they are — the archived build's
/// sizes were tile counts, and "128x128" told a player nothing about how far
/// a lorry would have to drive.
pub const SIZES: [(&str, Metres); 3] = [
    ("Small", Metres(6_000.0)),
    ("Standard", Metres(10_000.0)),
    ("Large", Metres(16_000.0)),
];

/// What the player has narrowed the shelf to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShelfFilter {
    pub extent: Metres,
    pub climate: ClimateId,
}

impl Default for ShelfFilter {
    fn default() -> Self {
        Self {
            extent: SIZES[1].1,
            climate: ClimateId::Plains,
        }
    }
}

/// The facts a candidate card carries, beside its picture.
///
/// Deliberately few. These are the things that decide a start, and a card that
/// listed everything would be a spreadsheet the player has to study rather than
/// a choice they can feel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandidateStats {
    /// Share of the map a building could stand on.
    pub buildable: f64,
    pub water: f64,
    pub forest: f64,
    /// What the survey found, still in the ground.
    pub coal: Tonnes,
    pub iron: Tonnes,
    pub oil: Tonnes,
    pub groundwater: Tonnes,
    /// How far the middle of the map is from workable coal. The single most
    /// decisive fact about a posting: coal under your feet is a republic, coal
    /// twelve kilometres away is a haulage problem.
    pub coal_reach: Option<Metres>,
    /// Which edge is foreign soil.
    /// Frontier posts of each bloc. Which blocs a posting can reach, and how
    /// many ways, is a decisive fact about it: a republic with one Western post
    /// on the far side of a river trades in dollars only when it has built a
    /// road most of the way across the map.
    pub crossings_east: u32,
    pub crossings_west: u32,
    /// The coldest month's mean, in degrees Celsius — what a winter costs.
    pub coldest_month_c: f64,
}

impl CandidateStats {
    /// A single rough measure of how kind a posting is, `0.0..=1.0`.
    ///
    /// For **sorting and colour**, not for hiding anything: every underlying
    /// figure is on the card and the player is free to disagree with the
    /// weighting. Land quality genuinely varies and nothing here guarantees a
    /// rich start — a bad hand is a legitimate hand, and papering over one
    /// would make the shelf dishonest.
    pub fn promise(&self) -> f64 {
        let coal = (self.coal.0 / 5_000_000.0).clamp(0.0, 1.0);
        let iron = (self.iron.0 / 3_000_000.0).clamp(0.0, 1.0);
        let near = match self.coal_reach {
            Some(d) => (1.0 - d.0 / 5_000.0).clamp(0.0, 1.0),
            None => 0.0,
        };
        let warmth = ((self.coldest_month_c + 25.0) / 30.0).clamp(0.0, 1.0);
        0.3 * self.buildable + 0.2 * coal + 0.15 * iron + 0.2 * near + 0.15 * warmth
    }
}

/// One posting on offer.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Position on the shelf, stable across filter changes.
    pub index: usize,
    pub spec: WorldSpec,
    pub stats: CandidateStats,
}

/// The shelf itself: candidate seeds, and the worlds they make.
#[derive(Debug, Clone, PartialEq)]
pub struct Shelf {
    pub master_seed: u64,
    pub filter: ShelfFilter,
    pub candidates: Vec<Candidate>,
}

/// The seed of candidate `index` under a master seed.
///
/// A pure function of the two, and deliberately independent of the filter: this
/// is what makes "the same land under a different sky" possible.
pub fn candidate_seed(master_seed: u64, index: usize) -> u64 {
    derive(master_seed, SHELF_STREAM) ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

impl Shelf {
    /// Generate a shelf. Costs one full worldgen per candidate — see the module
    /// docs for what that is measured at.
    pub fn derive(master_seed: u64, filter: ShelfFilter) -> Self {
        let candidates = (0..SHELF_SIZE)
            .map(|index| {
                let spec = WorldSpec {
                    seed: candidate_seed(master_seed, index),
                    extent: filter.extent,
                    climate: filter.climate,
                };
                let world = World::new(spec);
                Candidate {
                    index,
                    spec,
                    stats: survey(&world),
                }
            })
            .collect();
        Self {
            master_seed,
            filter,
            candidates,
        }
    }

    /// The same shelf under a different filter — same seeds, new land.
    pub fn refilter(&self, filter: ShelfFilter) -> Self {
        Self::derive(self.master_seed, filter)
    }

    pub fn get(&self, index: usize) -> Option<&Candidate> {
        self.candidates.iter().find(|c| c.index == index)
    }

    /// Build the world for a candidate. The one the player actually plays.
    pub fn found(&self, index: usize) -> Option<World> {
        self.get(index).map(|c| World::new(c.spec))
    }
}

/// Read a generated world the way the in-game survey overlay would.
pub fn survey(world: &World) -> CandidateStats {
    let terrain = &world.terrain;
    let water = terrain.fraction_of(Surface::Water);
    let forest = terrain.fraction_of(Surface::Forest);
    let middle = Point::new(terrain.extent() / 2.0, terrain.extent() / 2.0);

    CandidateStats {
        buildable: 1.0 - water,
        water,
        forest,
        coal: world.geology.remaining_of(Mineral::Coal),
        iron: world.geology.remaining_of(Mineral::IronOre),
        oil: world.geology.remaining_of(Mineral::Oil),
        groundwater: world.geology.remaining_of(Mineral::Groundwater),
        coal_reach: world.geology.distance_to_nearest(middle, Mineral::Coal),
        crossings_east: world
            .frontier
            .crossings()
            .iter()
            .filter(|c| c.bloc == Market::East)
            .count() as u32,
        crossings_west: world
            .frontier
            .crossings()
            .iter()
            .filter(|c| c.bloc == Market::West)
            .count() as u32,
        coldest_month_c: world.climate.def().coldest_mean_c(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> ShelfFilter {
        ShelfFilter {
            extent: Metres(2_000.0),
            climate: ClimateId::Plains,
        }
    }

    #[test]
    fn a_shelf_offers_distinct_postings_from_one_master_seed() {
        let shelf = Shelf::derive(1961, small());
        assert_eq!(shelf.candidates.len(), SHELF_SIZE);
        let mut seeds: Vec<u64> = shelf.candidates.iter().map(|c| c.spec.seed).collect();
        seeds.sort();
        seeds.dedup();
        assert_eq!(seeds.len(), SHELF_SIZE, "two candidates share a seed");
        // And positions are stable, which is what lets a card be re-rendered.
        for (i, c) in shelf.candidates.iter().enumerate() {
            assert_eq!(c.index, i);
        }
    }

    #[test]
    fn the_same_master_seed_deals_the_same_shelf() {
        assert_eq!(Shelf::derive(1961, small()), Shelf::derive(1961, small()));
        assert_ne!(Shelf::derive(1961, small()), Shelf::derive(1962, small()));
    }

    /// The design decision this module exists to hold: a filter **transforms**
    /// the shelf rather than replacing it. Flipping the climate has to show the
    /// player what that does to land they were already considering.
    #[test]
    fn changing_a_filter_keeps_the_same_seeds() {
        let plains = Shelf::derive(1961, small());
        let taiga = plains.refilter(ShelfFilter {
            climate: ClimateId::Taiga,
            ..small()
        });

        let before: Vec<u64> = plains.candidates.iter().map(|c| c.spec.seed).collect();
        let after: Vec<u64> = taiga.candidates.iter().map(|c| c.spec.seed).collect();
        assert_eq!(before, after, "the shelf dealt a fresh hand");

        // Same ground, colder sky.
        for i in 0..SHELF_SIZE {
            let a = plains.get(i).unwrap();
            let b = taiga.get(i).unwrap();
            assert_eq!(a.stats.buildable, b.stats.buildable);
            assert_eq!(a.stats.coal, b.stats.coal);
            assert!(b.stats.coldest_month_c < a.stats.coldest_month_c);
        }
    }

    /// Resizing is the other filter, and it genuinely changes the land — a
    /// bigger map is more ground, not the same ground drawn larger.
    #[test]
    fn resizing_keeps_the_seeds_and_changes_the_ground() {
        let small_shelf = Shelf::derive(7, small());
        let big = small_shelf.refilter(ShelfFilter {
            extent: Metres(4_000.0),
            ..small()
        });
        assert_eq!(
            small_shelf.candidates[0].spec.seed,
            big.candidates[0].spec.seed
        );
        assert_ne!(small_shelf.candidates[0].stats, big.candidates[0].stats);
    }

    /// The card must read what the game reads. If these ever diverge, the shelf
    /// is advertising a republic the player will not be given.
    #[test]
    fn a_card_reports_what_the_world_actually_holds() {
        let shelf = Shelf::derive(1961, small());
        let candidate = shelf.get(2).expect("six candidates");
        let world = shelf.found(2).expect("six candidates");

        assert_eq!(
            world.geology.remaining_of(Mineral::Coal),
            candidate.stats.coal
        );
        assert_eq!(
            world.frontier.crossings().len() as u32,
            candidate.stats.crossings_east + candidate.stats.crossings_west,
        );
        assert_eq!(
            world.terrain.fraction_of(Surface::Water),
            candidate.stats.water
        );
        // And the survey the overlay renders sees the same bodies the card
        // counted — one reading, two renderings.
        let surveyed: Tonnes = world
            .geology
            .survey()
            .iter()
            .filter(|r| r.mineral == Mineral::Coal)
            .map(|r| r.remaining)
            .sum();
        assert_eq!(surveyed, candidate.stats.coal);
    }

    /// Land quality genuinely varies. A shelf where every posting were equally
    /// good would make the choice decorative.
    #[test]
    fn postings_are_not_all_equally_good() {
        let shelf = Shelf::derive(1961, ShelfFilter::default());
        let scores: Vec<f64> = shelf.candidates.iter().map(|c| c.stats.promise()).collect();
        let best = scores.iter().copied().fold(f64::MIN, f64::max);
        let worst = scores.iter().copied().fold(f64::MAX, f64::min);
        assert!(
            best - worst > 0.02,
            "every posting scored the same: {scores:?}"
        );
        assert!(scores.iter().all(|s| (0.0..=1.0).contains(s)));
    }

    #[test]
    fn the_world_a_card_promises_is_the_world_it_hands_over() {
        let shelf = Shelf::derive(99, small());
        let played = shelf.found(0).expect("a first candidate");
        assert_eq!(played.seed(), shelf.candidates[0].spec.seed);
        assert_eq!(played.climate, shelf.filter.climate);
        assert_eq!(played.terrain.extent(), shelf.filter.extent);
    }

    #[test]
    fn asking_for_a_posting_that_is_not_on_the_shelf_gets_nothing() {
        let shelf = Shelf::derive(1, small());
        assert!(shelf.get(SHELF_SIZE).is_none());
        assert!(shelf.found(SHELF_SIZE).is_none());
    }
}
