//! The border, the treasury, and what crosses.
//!
//! # Money is foreign currency only
//!
//! The archived build's rule, carried forward whole: **nothing domestic costs
//! money.** Buildings cost materials and the labour of citizens; roads cost
//! gravel. Currency enters the republic at the border, through exports, and
//! leaves at the border, through imports. A treasury that can be spent on
//! domestic work would quietly turn a planned economy into a market one.
//!
//! # Two markets, and a spread
//!
//! The eastern bloc pays in roubles and the west in dollars, at different
//! prices — that asymmetry is the whole trade game. Selling is worth less per
//! tonne than buying costs, by [`BORDER_SPREAD`], so shuttling goods back and
//! forth across the border is a loss rather than an arbitrage. Without that,
//! the optimal strategy is a loop that produces nothing.
//!
//! # Trade is physical
//!
//! Goods do not teleport. An export has to be trucked to a customs house
//! first, an import arrives there and has to be trucked onward, and a customs
//! house clears only so much a day — [`CUSTOMS_THROUGHPUT_PER_DAY`], scaled by
//! how well it is staffed. A republic with no customs house cannot trade at
//! all, however much currency it has.

use crate::resource::Resource;
use crate::units::{Metres, Point, Tonnes};
use serde::{Deserialize, Serialize};

/// A stretch of frontier, and the bloc on the other side of it.
///
/// Position runs around the perimeter as a single parameter in `0.0..4.0`:
/// 0 at the map's origin corner, 1 at the next corner clockwise, and so on. One
/// number rather than an edge plus an offset, because every question anyone
/// asks of the frontier — which bloc is this, where is the nearest crossing,
/// how far along is that post — is a question about a loop.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrontierArc {
    /// Where this stretch begins on the perimeter. It runs to the next arc's
    /// start, and the last wraps to the first.
    pub start: f64,
    pub bloc: Market,
}

/// A crossing point, placed by worldgen rather than by the player.
///
/// **You do not build these.** The frontier and its posts are there when you
/// arrive; what you decide is which one to build road out to, and that is a
/// siting and haulage decision rather than a dropdown. It is also what makes
/// the two currencies geographic: a crossing settles in its own bloc's money,
/// so trading west means hauling west.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BorderCrossing {
    pub id: CrossingId,
    /// Where the post stands, inset from the frontier onto workable ground.
    pub at: Point,
    /// Where it sits on the perimeter, so the frontier can be drawn with its
    /// posts in the right places.
    pub along: f64,
    pub bloc: Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CrossingId(pub u32);

/// The whole perimeter, and who is on the other side of each part of it.
///
/// Replaces `BorderEdge`, which named **one** edge of the map as foreign and
/// said nothing about the other three — so a republic had a single frontier and
/// three impassable nothings. A republic is surrounded. The map Noah pointed at
/// has blue Western Alliance and red Eastern Bloc frontier on the same edge,
/// which is the shape this models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontier {
    extent: Metres,
    /// In perimeter order, always covering the whole loop and never empty.
    arcs: Vec<FrontierArc>,
    crossings: Vec<BorderCrossing>,
}

/// How many crossings a frontier gets. Enough that "which one" is a real
/// choice and few enough that one is always a haul away.
pub const CROSSINGS: usize = 4;

/// How far inside the frontier a post stands.
pub const CROSSING_INSET: Metres = Metres(120.0);

impl Frontier {
    /// The perimeter as one parameter: `0.0..4.0`, one unit per side.
    pub const TURNS: f64 = 4.0;

    /// Draw a frontier for a map: where the blocs meet, and where the posts are.
    ///
    /// Deterministic from its own substream, so the frontier is a property of
    /// the seed rather than of how much of the world has been generated —
    /// which is what lets a founding shelf show one candidate's frontier
    /// without disturbing another's.
    ///
    /// The two blocs each hold a contiguous stretch. A frontier chopped into
    /// many alternating pieces would make "haul west" meaningless: the point of
    /// two currencies being geographic is that one direction is the west.
    pub fn generate(extent: Metres, rng: &mut crate::rng::Rng) -> Self {
        // Where the two blocs meet, as two points on the loop. Kept at least
        // a side apart so neither ends up with a sliver nobody can reach.
        let first = rng.next_bounded(4_000) as f64 / 1_000.0;
        let span = 1.0 + rng.next_bounded(2_000) as f64 / 1_000.0;
        let second = (first + span).rem_euclid(Self::TURNS);

        let (a, b) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        let east_first = rng.next_bounded(2) == 0;
        let (one, two) = if east_first {
            (Market::East, Market::West)
        } else {
            (Market::West, Market::East)
        };
        let arcs = vec![
            FrontierArc {
                start: 0.0,
                bloc: two,
            },
            FrontierArc {
                start: a,
                bloc: one,
            },
            FrontierArc {
                start: b,
                bloc: two,
            },
        ];

        let mut frontier = Self::new(extent, arcs, Vec::new());
        // Posts spread evenly around the loop, offset so they never land
        // exactly on a corner or on the seam between blocs.
        let offset = rng.next_bounded(1_000) as f64 / 1_000.0 * Self::TURNS / CROSSINGS as f64;
        let mut crossings = Vec::with_capacity(CROSSINGS);
        for i in 0..CROSSINGS {
            let along = (offset + Self::TURNS * i as f64 / CROSSINGS as f64) % Self::TURNS;
            crossings.push(BorderCrossing {
                id: CrossingId(i as u32 + 1),
                at: frontier.inset_point(along),
                along,
                bloc: frontier.bloc_at(along),
            });
        }
        frontier.crossings = crossings;
        frontier
    }

    /// A point on the frontier, moved inward onto the republic's own ground.
    fn inset_point(&self, along: f64) -> Point {
        let e = self.extent.0;
        let on = self.point_at(along);
        let inset = CROSSING_INSET.0.min(e * 0.25);
        Point::new(
            Metres(on.x.0.clamp(inset, e - inset)),
            Metres(on.y.0.clamp(inset, e - inset)),
        )
    }

    pub fn new(extent: Metres, arcs: Vec<FrontierArc>, crossings: Vec<BorderCrossing>) -> Self {
        assert!(!arcs.is_empty(), "a frontier needs at least one stretch");
        Self {
            extent,
            arcs,
            crossings,
        }
    }

    pub fn arcs(&self) -> &[FrontierArc] {
        &self.arcs
    }

    pub fn crossings(&self) -> &[BorderCrossing] {
        &self.crossings
    }

    pub fn get(&self, id: CrossingId) -> Option<&BorderCrossing> {
        self.crossings.iter().find(|c| c.id == id)
    }

    /// Where a perimeter parameter lands on the map.
    pub fn point_at(&self, along: f64) -> Point {
        let e = self.extent.0;
        let t = along.rem_euclid(Self::TURNS);
        let (side, f) = (t.floor(), t.fract());
        match side as u32 {
            0 => Point::new(Metres(e * f), Metres(0.0)),
            1 => Point::new(Metres(e), Metres(e * f)),
            2 => Point::new(Metres(e * (1.0 - f)), Metres(e)),
            _ => Point::new(Metres(0.0), Metres(e * (1.0 - f))),
        }
    }

    /// The nearest point on the frontier to somewhere inside the republic,
    /// as a perimeter parameter.
    pub fn nearest_along(&self, point: Point) -> f64 {
        let e = self.extent.0;
        let (x, y) = (point.x.0.clamp(0.0, e), point.y.0.clamp(0.0, e));
        // Distance to each side, then the parameter on whichever is closest.
        // Ties break toward the lower side index so the answer is stable.
        let candidates = [
            (y, x / e),                 // north edge, y = 0
            (e - x, 1.0 + y / e),       // east edge
            (e - y, 2.0 + (e - x) / e), // south edge
            (x, 3.0 + (e - y) / e),     // west edge
        ];
        candidates
            .iter()
            .enumerate()
            .min_by(|(ia, (da, _)), (ib, (db, _))| da.total_cmp(db).then_with(|| ia.cmp(ib)))
            .map(|(_, (_, t))| *t)
            .unwrap_or(0.0)
    }

    /// How far a point is from foreign soil.
    pub fn distance_from(&self, point: Point) -> Metres {
        let e = self.extent.0;
        let (x, y) = (point.x.0, point.y.0);
        Metres(x.min(y).min(e - x).min(e - y).max(0.0))
    }

    /// Which bloc holds the frontier at a perimeter parameter.
    pub fn bloc_at(&self, along: f64) -> Market {
        let t = along.rem_euclid(Self::TURNS);
        // The arc whose start is the last one at or before `t`; if `t` comes
        // before every start it belongs to the arc that wraps around.
        let mut held = self.arcs.last().expect("never empty").bloc;
        for arc in &self.arcs {
            if arc.start <= t {
                held = arc.bloc;
            }
        }
        held
    }

    /// Which bloc's frontier is nearest to a point.
    pub fn bloc_near(&self, point: Point) -> Market {
        self.bloc_at(self.nearest_along(point))
    }

    /// The nearest crossing to a point, optionally of one bloc only.
    ///
    /// Straight-line distance rather than road distance on purpose: this
    /// answers "which post is this near", and how long it actually takes to
    /// reach one is the fleet's question, priced by the same planner that will
    /// drive it.
    pub fn nearest_crossing(&self, to: Point, bloc: Option<Market>) -> Option<&BorderCrossing> {
        self.crossings
            .iter()
            .filter(|c| bloc.is_none_or(|b| c.bloc == b))
            .min_by(|a, b| {
                a.at.distance_to(to)
                    .0
                    .total_cmp(&b.at.distance_to(to).0)
                    .then_with(|| a.id.cmp(&b.id))
            })
    }
}

/// How near the border a customs house must stand to clear anything.
pub const CUSTOMS_RANGE: Metres = Metres(400.0);

/// Tonnes a fully staffed customs house clears in a day. Ported from the
/// archived balance.
pub const CUSTOMS_THROUGHPUT_PER_DAY: f64 = 30.0;

/// What the border pays for a tonne, as a fraction of what it charges.
///
/// The reason trading with yourself is not a business model.
pub const BORDER_SPREAD: f64 = 0.8;

/// Which bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Market {
    East,
    West,
}

impl Market {
    /// What one tonne costs to buy here.
    pub fn buy_price(self, resource: Resource) -> f64 {
        match self {
            Market::East => resource.price_east(),
            Market::West => resource.price_west(),
        }
    }

    /// What one tonne fetches when sold here.
    pub fn sell_price(self, resource: Resource) -> f64 {
        self.buy_price(resource) * BORDER_SPREAD
    }
}

/// The republic's hard currency.
///
/// Two separate purses, deliberately: roubles cannot buy from the west and
/// dollars cannot buy from the east, which is what makes *which* market you
/// trade with a decision rather than a detail.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Treasury {
    pub rubles: f64,
    pub dollars: f64,
}

impl Treasury {
    pub fn of(&self, market: Market) -> f64 {
        match market {
            Market::East => self.rubles,
            Market::West => self.dollars,
        }
    }

    pub fn credit(&mut self, market: Market, amount: f64) {
        match market {
            Market::East => self.rubles += amount,
            Market::West => self.dollars += amount,
        }
    }

    /// Spend, refusing to go negative. Returns what was actually spent — the
    /// republic does not run an overdraft it never agreed to.
    pub fn debit(&mut self, market: Market, amount: f64) -> f64 {
        let available = self.of(market);
        let spent = amount.min(available).max(0.0);
        match market {
            Market::East => self.rubles -= spent,
            Market::West => self.dollars -= spent,
        }
        spent
    }
}

/// A standing instruction to the customs house.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TradeRule {
    pub resource: Resource,
    pub market: Market,
    pub action: TradeAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TradeAction {
    /// Sell whatever reaches the customs house.
    Sell,
    /// Keep the customs house topped up to this much, buying the shortfall.
    Buy { up_to: Tonnes },
}

/// The republic's standing trade policy, in the order it is applied.
///
/// Order is the player's, and it matters when throughput or money runs short:
/// the first rule gets served first. That is a decision the player makes, not
/// one the simulation should make for them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TradePolicy {
    pub rules: Vec<TradeRule>,
}

impl TradePolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sell(mut self, resource: Resource, market: Market) -> Self {
        self.rules.push(TradeRule {
            resource,
            market,
            action: TradeAction::Sell,
        });
        self
    }

    pub fn buy(mut self, resource: Resource, market: Market, up_to: Tonnes) -> Self {
        self.rules.push(TradeRule {
            resource,
            market,
            action: TradeAction::Buy { up_to },
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(extent: f64) -> Frontier {
        Frontier::new(
            Metres(extent),
            vec![
                FrontierArc {
                    start: 0.0,
                    bloc: Market::East,
                },
                FrontierArc {
                    start: 2.0,
                    bloc: Market::West,
                },
            ],
            Vec::new(),
        )
    }

    /// The frontier is the whole perimeter, so the distance to it is the
    /// distance to the *nearest* side.
    ///
    /// This replaces a test that measured from one chosen edge, which was
    /// right about the old model and is the wrong question now: under
    /// `BorderEdge` a republic had one frontier and three impassable nothings.
    #[test]
    fn the_frontier_is_measured_to_whichever_side_is_nearest() {
        let f = plain(1_000.0);
        assert_eq!(
            f.distance_from(Point::new(Metres(100.0), Metres(250.0))),
            Metres(100.0),
        );
        assert_eq!(
            f.distance_from(Point::new(Metres(500.0), Metres(250.0))),
            Metres(250.0),
        );
        assert_eq!(
            f.distance_from(Point::new(Metres(900.0), Metres(500.0))),
            Metres(100.0),
        );
        // Dead centre is as far from foreign soil as it is possible to get.
        assert_eq!(
            f.distance_from(Point::new(Metres(500.0), Metres(500.0))),
            Metres(500.0),
        );
    }

    /// Which bloc you are near depends on which way you look, which is the
    /// whole point of the frontier having sides.
    #[test]
    fn the_two_blocs_hold_different_stretches_of_the_same_perimeter() {
        let f = plain(1_000.0);
        // North edge (t in 0..1) and east (1..2) are the East's; south (2..3)
        // and west (3..4) are the West's.
        assert_eq!(f.bloc_at(0.5), Market::East);
        assert_eq!(f.bloc_at(1.5), Market::East);
        assert_eq!(f.bloc_at(2.5), Market::West);
        assert_eq!(f.bloc_at(3.5), Market::West);

        // And from inside: near the north edge is East, near the south is West.
        assert_eq!(
            f.bloc_near(Point::new(Metres(500.0), Metres(50.0))),
            Market::East
        );
        assert_eq!(
            f.bloc_near(Point::new(Metres(500.0), Metres(950.0))),
            Market::West
        );
    }

    /// A generated frontier always has posts of both blocs to trade through,
    /// and they stand on the republic's own ground rather than on the line.
    #[test]
    fn a_generated_frontier_has_posts_inside_the_republic() {
        for seed in [1u64, 1961, 7, 40_000] {
            let extent = Metres(6_000.0);
            let f = Frontier::generate(extent, &mut crate::rng::Rng::from_seed(seed));
            assert_eq!(f.crossings().len(), CROSSINGS, "seed {seed}");
            for crossing in f.crossings() {
                let d = f.distance_from(crossing.at);
                assert!(
                    d.0 > 0.0 && d.0 <= CROSSING_INSET.0 + 1.0,
                    "seed {seed}: a post sat {d:?} from the frontier"
                );
            }
            let east = f
                .crossings()
                .iter()
                .filter(|c| c.bloc == Market::East)
                .count();
            assert!(
                east > 0 && east < CROSSINGS,
                "seed {seed}: every post belonged to one bloc, so there is                  nothing to choose between"
            );
        }
    }

    /// Selling and buying the same tonne must lose money, or the optimal
    /// republic is a loop that produces nothing.
    #[test]
    fn trading_with_yourself_is_a_loss() {
        for market in [Market::East, Market::West] {
            for resource in Resource::ALL {
                assert!(
                    market.sell_price(resource) < market.buy_price(resource),
                    "{resource:?} on {market:?} is free money"
                );
            }
        }
    }

    #[test]
    fn the_two_purses_are_separate() {
        let mut t = Treasury::default();
        t.credit(Market::East, 100.0);
        assert_eq!(t.rubles, 100.0);
        assert_eq!(t.dollars, 0.0);
        assert_eq!(t.of(Market::West), 0.0);
    }

    /// The republic does not run an overdraft it never agreed to.
    #[test]
    fn spending_stops_at_what_is_there() {
        let mut t = Treasury::default();
        t.credit(Market::East, 50.0);
        assert_eq!(t.debit(Market::East, 80.0), 50.0);
        assert_eq!(t.rubles, 0.0);
        assert_eq!(t.debit(Market::East, 10.0), 0.0);
        assert_eq!(t.rubles, 0.0);
    }

    #[test]
    fn policy_reads_in_the_order_it_was_written() {
        let policy = TradePolicy::new().sell(Resource::Coal, Market::East).buy(
            Resource::Machinery,
            Market::West,
            Tonnes(5.0),
        );
        assert_eq!(policy.rules.len(), 2);
        assert_eq!(policy.rules[0].resource, Resource::Coal);
        assert_eq!(
            policy.rules[1].action,
            TradeAction::Buy { up_to: Tonnes(5.0) }
        );
    }
}
