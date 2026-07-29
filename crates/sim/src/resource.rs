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
///
/// # The order is logical, and it is part of the definition
///
/// Chains run downwards: what a thing is made from sits above it. That is for
/// the reader and for the stockpile table, both of which are easier to follow
/// when steel stands next to the ore it came out of. Iteration order is part of
/// the simulation's definition wherever an accumulation follows [`Resource::ALL`],
/// so this list is not to be reshuffled casually.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Resource {
    Coal,
    IronOre,
    Steel,
    Oil,
    Fuel,
    /// The heavy end of a barrel. A refinery makes it whether anybody wants it
    /// or not — which is the point of it being a second output rather than a
    /// second recipe: a republic that refines for diesel has bitumen to do
    /// something with, and asphalt is what that something is.
    Bitumen,
    Chemicals,
    Wood,
    Planks,
    Gravel,
    Bricks,
    Cement,
    Concrete,
    /// A wall, a floor and a ceiling, cast at a works and driven to the site.
    ///
    /// This is how Soviet housing was actually built, and it is the one
    /// construction material that buys a *different building* rather than a
    /// cheaper version of the same one — see `BuildingKind::PanelBlock`.
    PrefabPanel,
    Asphalt,
    Crops,
    Food,
    Clothes,
    /// A hard-currency export made from the republic's own fields, and
    /// deliberately **not** something the people are judged on having.
    ///
    /// See [`Resource::is_luxury`] for why that distinction is drawn at all.
    Alcohol,
    Machinery,
    Electronics,
    /// What a republic throws away. A resource like any other, which is the
    /// point: it accumulates in a bin, it has to be *driven* somewhere, and a
    /// republic that has nowhere to drive it watches it pile up where people
    /// live.
    Waste,
}

impl Resource {
    /// Every resource, in a fixed order — iteration order is part of the
    /// simulation's definition wherever a draw or an accumulation follows it.
    pub const ALL: [Resource; 22] = [
        Resource::Coal,
        Resource::IronOre,
        Resource::Steel,
        Resource::Oil,
        Resource::Fuel,
        Resource::Bitumen,
        Resource::Chemicals,
        Resource::Wood,
        Resource::Planks,
        Resource::Gravel,
        Resource::Bricks,
        Resource::Cement,
        Resource::Concrete,
        Resource::PrefabPanel,
        Resource::Asphalt,
        Resource::Crops,
        Resource::Food,
        Resource::Clothes,
        Resource::Alcohol,
        Resource::Machinery,
        Resource::Electronics,
        Resource::Waste,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Resource::Coal => "Coal",
            Resource::IronOre => "Iron Ore",
            Resource::Steel => "Steel",
            Resource::Oil => "Oil",
            Resource::Fuel => "Fuel",
            Resource::Bitumen => "Bitumen",
            Resource::Chemicals => "Chemicals",
            Resource::Wood => "Wood",
            Resource::Planks => "Planks",
            Resource::Gravel => "Gravel",
            Resource::Bricks => "Bricks",
            Resource::Cement => "Cement",
            Resource::Concrete => "Concrete",
            Resource::PrefabPanel => "Prefab Panels",
            Resource::Asphalt => "Asphalt",
            Resource::Crops => "Crops",
            Resource::Food => "Food",
            Resource::Clothes => "Clothes",
            Resource::Alcohol => "Alcohol",
            Resource::Machinery => "Machinery",
            Resource::Electronics => "Electronics",
            Resource::Waste => "Waste",
        }
    }

    /// Whether this is something people are glad of rather than something they
    /// need.
    ///
    /// Drink and household electrics — radios, televisions, the things that
    /// make a flat somewhere to live rather than somewhere to sleep. **A
    /// comfort is worth having and nobody's life is ruined without it**, and
    /// that sentence is the whole model: [`crate::wellbeing::Contentment`]
    /// applies them as a lift on top of the needs rather than as one more thing
    /// to be short of.
    ///
    /// **The first version of this made them exports only, and that was wrong.**
    /// The reasoning was sound as far as it went — modelling them as ordinary
    /// wants would have dropped every standing republic's score the day the
    /// goods were invented — but the fix for that is to make them *additive*,
    /// not to keep them out of people's hands. They are both now: worth a great
    /// deal per tonne at a frontier post, and worth something to the people who
    /// live here, which is a genuine decision about where the lorry goes.
    ///
    /// Alcohol carries a cost with it — see
    /// [`crate::wellbeing::ALCOHOL_HEALTH_COST`] — and electronics do not.
    ///
    /// Authored as a property of the resource rather than as a list inside the
    /// trade or households systems, for the reason every other property here is.
    pub fn is_comfort(self) -> bool {
        matches!(self, Resource::Alcohol | Resource::Electronics)
    }

    /// What shape this stuff is, and therefore what will hold it.
    ///
    /// **This is what makes storage a decision rather than a number.** A
    /// republic used to be able to keep two hundred tonnes of anything in one
    /// shed; now oil goes in a tank, grain goes in a silo, and gravel goes in a
    /// heap — which is the whole of W&R's storage roster expressed as one
    /// authored fact per resource rather than as five special cases.
    ///
    /// Authored here beside the prices rather than as a list of resources
    /// inside the storage buildings, for the usual reason: a list in logic is a
    /// thing you must remember to edit.
    pub fn form(self) -> Form {
        match self {
            // Tipped in heaps and left in the rain.
            Resource::Coal
            | Resource::IronOre
            | Resource::Gravel
            | Resource::Asphalt
            | Resource::Waste => Form::Aggregate,
            // Pumped, and it needs something that holds pressure.
            Resource::Oil
            | Resource::Fuel
            | Resource::Bitumen
            | Resource::Chemicals
            | Resource::Alcohol => Form::Liquid,
            // Powder and grain: it has to stay dry and it flows, which is what
            // a silo is for and why a warehouse full of sacks is not one.
            Resource::Cement | Resource::Crops => Form::Bulk,
            // Weather does not hurt it and a yard will do.
            Resource::Wood
            | Resource::Planks
            | Resource::Bricks
            | Resource::Concrete
            | Resource::PrefabPanel
            | Resource::Steel
            | Resource::Machinery => Form::Open,
            // Wants a roof and a floor.
            Resource::Food | Resource::Clothes | Resource::Electronics => Form::Covered,
        }
    }

    /// Price per tonne buying from the eastern bloc, in roubles.
    ///
    /// The manufactured end of the table is dear on purpose: a tonne of
    /// electronics is worth twenty tonnes of steel and two hundred of coal, so
    /// a republic that gets a chain running earns in a lorry what a mine earns
    /// in a month. That gap **is** the industrialisation incentive, and pricing
    /// the new goods near their inputs would have made the chains decoration.
    pub fn price_east(self) -> f64 {
        match self {
            Resource::Coal => 2.5,
            Resource::IronOre => 3.0,
            Resource::Steel => 14.0,
            Resource::Oil => 5.0,
            Resource::Fuel => 10.0,
            Resource::Bitumen => 4.0,
            Resource::Chemicals => 26.0,
            Resource::Wood => 2.0,
            Resource::Planks => 5.0,
            Resource::Gravel => 1.5,
            Resource::Bricks => 6.0,
            Resource::Cement => 8.0,
            Resource::Concrete => 12.0,
            Resource::PrefabPanel => 30.0,
            Resource::Asphalt => 13.0,
            Resource::Crops => 2.0,
            Resource::Food => 4.5,
            Resource::Clothes => 9.0,
            Resource::Alcohol => 45.0,
            Resource::Machinery => 80.0,
            Resource::Electronics => 140.0,
            // Nobody buys your rubbish, and shipping it abroad costs. A price
            // is authored anyway because every resource is priced on both
            // sides — an unpriced one would be a hole in the trade table
            // rather than a decision.
            Resource::Waste => 0.2,
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
            Resource::Bitumen => 2.2,
            Resource::Chemicals => 16.0,
            Resource::Wood => 1.0,
            Resource::Planks => 2.5,
            Resource::Gravel => 0.7,
            Resource::Bricks => 3.0,
            Resource::Cement => 4.5,
            Resource::Concrete => 7.0,
            Resource::PrefabPanel => 18.0,
            Resource::Asphalt => 7.5,
            Resource::Crops => 1.0,
            Resource::Food => 2.0,
            Resource::Clothes => 5.0,
            Resource::Alcohol => 30.0,
            Resource::Machinery => 50.0,
            Resource::Electronics => 95.0,
            Resource::Waste => 0.1,
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

/// What shape a resource comes in, and therefore what will hold it.
///
/// Five forms and five kinds of store, which is not a coincidence: the forms
/// exist so that W&R's storage roster — open, warehouse, aggregate, tank, silo
/// — is a property of the *goods* rather than five lists of resource ids inside
/// five buildings. A new resource declares its form in
/// [`Resource::form`] and every store already knows whether it will take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Form {
    /// Heaps of loose stuff. Tipped, scraped up, and none the worse for rain.
    Aggregate,
    /// Anything that has to be pumped and will run away if it is not contained.
    Liquid,
    /// Powder and grain: it flows, and it spoils wet.
    Bulk,
    /// Solid goods that survive outdoors.
    Open,
    /// Solid goods that want a roof.
    Covered,
}

impl Form {
    pub const ALL: [Form; 5] = [
        Form::Aggregate,
        Form::Liquid,
        Form::Bulk,
        Form::Open,
        Form::Covered,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Form::Aggregate => "Loose",
            Form::Liquid => "Liquid",
            Form::Bulk => "Granular",
            Form::Open => "Open",
            Form::Covered => "Covered",
        }
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

    /// The roster's own size, so `ALL` and the enum cannot drift apart. Rust
    /// will not catch a resource added to the enum and forgotten here — it is
    /// an array literal, not a match — and a resource missing from `ALL` is
    /// invisible to every stockpile, every panel and every trade rule.
    #[test]
    fn every_resource_is_in_the_roster_exactly_once() {
        let mut sorted = Resource::ALL;
        sorted.sort();
        sorted.windows(2).for_each(|pair| {
            assert_ne!(pair[0], pair[1], "{:?} is listed twice", pair[0]);
        });
        // Discriminants run 0..ALL.len() with none missing, which is what
        // `Stock`'s indexing by `r as usize` silently depends on.
        for (index, resource) in Resource::ALL.into_iter().enumerate() {
            assert_eq!(resource as usize, index, "{resource:?} is out of place");
        }
    }

    #[test]
    fn every_resource_has_a_name_and_a_form() {
        for r in Resource::ALL {
            assert!(!r.name().is_empty(), "{r:?}");
            assert!(Form::ALL.contains(&r.form()), "{r:?}");
        }
    }

    /// A comfort is worth both things at once, and that is the decision.
    ///
    /// Dear enough abroad that carrying it to a post is a real use of a lorry,
    /// and wanted at home — so where the tonnage goes is a choice rather than a
    /// foregone conclusion. A comfort that was only worth one of the two would
    /// collapse back into an export or into a chore.
    #[test]
    fn a_comfort_is_worth_selling_and_worth_keeping() {
        let comforts: Vec<_> = Resource::ALL
            .into_iter()
            .filter(|r| r.is_comfort())
            .collect();
        assert!(!comforts.is_empty(), "nothing is a comfort");
        for r in comforts {
            assert!(
                r.price_west() > Resource::Steel.price_west(),
                "{r:?} is a comfort nobody would bother exporting"
            );
        }
        assert!(!Resource::Food.is_comfort(), "food is a need, not a treat");
        assert!(!Resource::Clothes.is_comfort(), "clothes are a need");
    }
}
