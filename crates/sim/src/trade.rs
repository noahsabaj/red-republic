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

/// Which edge of the map is foreign soil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderEdge {
    North,
    South,
    East,
    West,
}

impl BorderEdge {
    pub const ALL: [BorderEdge; 4] = [
        BorderEdge::North,
        BorderEdge::South,
        BorderEdge::East,
        BorderEdge::West,
    ];

    /// How far a point is from this edge of a square map.
    pub fn distance_from(self, point: Point, extent: Metres) -> Metres {
        Metres(match self {
            BorderEdge::North => point.y.0,
            BorderEdge::South => extent.0 - point.y.0,
            BorderEdge::West => point.x.0,
            BorderEdge::East => extent.0 - point.x.0,
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

    #[test]
    fn the_border_is_measured_from_the_right_edge() {
        let extent = Metres(1_000.0);
        let p = Point::new(Metres(100.0), Metres(250.0));
        assert_eq!(BorderEdge::North.distance_from(p, extent), Metres(250.0));
        assert_eq!(BorderEdge::South.distance_from(p, extent), Metres(750.0));
        assert_eq!(BorderEdge::West.distance_from(p, extent), Metres(100.0));
        assert_eq!(BorderEdge::East.distance_from(p, extent), Metres(900.0));
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
