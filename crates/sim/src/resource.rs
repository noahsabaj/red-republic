//! What the republic makes, moves and sells.
//!
//! Ported from the archived build's resource table — the ids, the chains they
//! form, and the border prices, all of which were balanced against each other
//! over a long time and are worth far more than they cost to keep.
//!
//! # Bulk, not units
//!
//! Every quantity is [`Tonnes`] and every quantity is continuous. Production,
//! wear and consumption are rates, so a fractional tonne is a real amount and
//! not a rounding artefact. **Wholeness is a property of the edges** — what
//! crosses the border, what a contract owes, what the player is shown — and
//! never of the simulation. The archived build learned this the hard way when
//! a float tail leaked into a toast as "42.1423… coal undelivered".

use crate::units::Tonnes;
use serde::{Deserialize, Serialize};

/// Everything the economy handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Resource {
    Coal,
    IronOre,
    Steel,
    Oil,
    Fuel,
    Wood,
    Planks,
    Gravel,
    Bricks,
    Crops,
    Food,
    Clothes,
    Machinery,
}

impl Resource {
    /// Every resource, in a fixed order — iteration order is part of the
    /// simulation's definition wherever a draw or an accumulation follows it.
    pub const ALL: [Resource; 13] = [
        Resource::Coal,
        Resource::IronOre,
        Resource::Steel,
        Resource::Oil,
        Resource::Fuel,
        Resource::Wood,
        Resource::Planks,
        Resource::Gravel,
        Resource::Bricks,
        Resource::Crops,
        Resource::Food,
        Resource::Clothes,
        Resource::Machinery,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Resource::Coal => "Coal",
            Resource::IronOre => "Iron Ore",
            Resource::Steel => "Steel",
            Resource::Oil => "Oil",
            Resource::Fuel => "Fuel",
            Resource::Wood => "Wood",
            Resource::Planks => "Planks",
            Resource::Gravel => "Gravel",
            Resource::Bricks => "Bricks",
            Resource::Crops => "Crops",
            Resource::Food => "Food",
            Resource::Clothes => "Clothes",
            Resource::Machinery => "Machinery",
        }
    }

    /// Price per tonne buying from the eastern bloc, in roubles.
    pub fn price_east(self) -> f64 {
        match self {
            Resource::Coal => 2.5,
            Resource::IronOre => 3.0,
            Resource::Steel => 14.0,
            Resource::Oil => 5.0,
            Resource::Fuel => 10.0,
            Resource::Wood => 2.0,
            Resource::Planks => 5.0,
            Resource::Gravel => 1.5,
            Resource::Bricks => 6.0,
            Resource::Crops => 2.0,
            Resource::Food => 4.5,
            Resource::Clothes => 9.0,
            Resource::Machinery => 80.0,
        }
    }

    /// Price per tonne buying from the west, in dollars.
    pub fn price_west(self) -> f64 {
        match self {
            Resource::Coal => 1.0,
            Resource::IronOre => 1.5,
            Resource::Steel => 8.0,
            Resource::Oil => 3.0,
            Resource::Fuel => 6.0,
            Resource::Wood => 1.0,
            Resource::Planks => 2.5,
            Resource::Gravel => 0.7,
            Resource::Bricks => 3.0,
            Resource::Crops => 1.0,
            Resource::Food => 2.0,
            Resource::Clothes => 5.0,
            Resource::Machinery => 50.0,
        }
    }

    /// The mineral this resource is dug out of, if any.
    pub fn from_mineral(self) -> Option<crate::geology::Mineral> {
        use crate::geology::Mineral;
        Some(match self {
            Resource::Coal => Mineral::Coal,
            Resource::IronOre => Mineral::IronOre,
            Resource::Oil => Mineral::Oil,
            Resource::Gravel => Mineral::Gravel,
            _ => return None,
        })
    }
}

/// A per-resource quantity, indexed by [`Resource`].
///
/// A fixed-size array rather than a map: no allocation, no iteration-order
/// question, and adding a resource is a compile error everywhere it matters
/// rather than a silent zero.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Stock([f64; Resource::ALL.len()]);

impl Stock {
    pub const EMPTY: Self = Self([0.0; Resource::ALL.len()]);

    pub fn get(&self, r: Resource) -> Tonnes {
        Tonnes(self.0[r as usize])
    }

    pub fn set(&mut self, r: Resource, amount: Tonnes) {
        self.0[r as usize] = amount.0.max(0.0);
    }

    pub fn add(&mut self, r: Resource, amount: Tonnes) {
        self.set(r, self.get(r) + amount);
    }

    /// Take up to `amount`, returning what was actually there. Never goes
    /// negative — a shortfall is a smaller delivery, not a debt.
    pub fn take(&mut self, r: Resource, amount: Tonnes) -> Tonnes {
        let taken = amount.min(self.get(r));
        self.set(r, self.get(r) - taken);
        taken
    }

    pub fn total(&self) -> Tonnes {
        Tonnes(self.0.iter().sum())
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&v| v <= 0.0)
    }

    /// Every resource with something in it, in [`Resource::ALL`] order.
    pub fn iter(&self) -> impl Iterator<Item = (Resource, Tonnes)> + '_ {
        Resource::ALL
            .into_iter()
            .map(move |r| (r, self.get(r)))
            .filter(|(_, t)| t.is_positive())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_holds_and_returns_what_was_put_in() {
        let mut s = Stock::EMPTY;
        s.add(Resource::Coal, Tonnes(10.0));
        s.add(Resource::Coal, Tonnes(5.0));
        assert_eq!(s.get(Resource::Coal), Tonnes(15.0));
        assert_eq!(s.get(Resource::Steel), Tonnes::ZERO);
    }

    /// A shortfall is a smaller delivery, never a debt — the archived build's
    /// rule, and the reason stock is clamped rather than allowed to go
    /// negative and quietly poison every downstream rate.
    #[test]
    fn taking_more_than_is_there_returns_what_was_there() {
        let mut s = Stock::EMPTY;
        s.add(Resource::Coal, Tonnes(3.0));
        assert_eq!(s.take(Resource::Coal, Tonnes(10.0)), Tonnes(3.0));
        assert_eq!(s.get(Resource::Coal), Tonnes::ZERO);
        assert_eq!(s.take(Resource::Coal, Tonnes(1.0)), Tonnes::ZERO);
    }

    #[test]
    fn iteration_skips_empties_and_keeps_the_declared_order() {
        let mut s = Stock::EMPTY;
        s.add(Resource::Machinery, Tonnes(1.0));
        s.add(Resource::Coal, Tonnes(2.0));
        let seen: Vec<_> = s.iter().map(|(r, _)| r).collect();
        assert_eq!(seen, vec![Resource::Coal, Resource::Machinery]);
    }

    #[test]
    fn every_resource_is_priced_on_both_sides_of_the_border() {
        for r in Resource::ALL {
            assert!(r.price_east() > 0.0, "{r:?}");
            assert!(r.price_west() > 0.0, "{r:?}");
        }
    }

    /// The west is the hard-currency market and sells cheaper in its own
    /// money — the price asymmetry the whole trade game rests on.
    #[test]
    fn the_two_markets_price_differently() {
        for r in Resource::ALL {
            assert_ne!(r.price_east(), r.price_west(), "{r:?}");
        }
    }

    #[test]
    fn extractable_resources_name_their_mineral() {
        use crate::geology::Mineral;
        assert_eq!(Resource::Coal.from_mineral(), Some(Mineral::Coal));
        assert_eq!(Resource::Gravel.from_mineral(), Some(Mineral::Gravel));
        assert_eq!(Resource::Steel.from_mineral(), None);
    }
}
