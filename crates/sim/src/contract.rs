//! Foreign tenders: bulk orders from the two blocs, at a premium, on a clock.
//!
//! # What a contract is for
//!
//! Spot trade at the border is a flat, patient business — whatever reaches the
//! customs house sells at today's price and nothing is ever urgent. A contract
//! is the opposite: a fixed tonnage, by a fixed day, at a price locked when it
//! was offered. Accepting one is a bet that the republic's output will hold up,
//! and it is the only thing in the economy that punishes a plan that quietly
//! stopped working.
//!
//! # Offers draw from their own stream
//!
//! Tenders come from a substream keyed by the month, never from the simulation's
//! generator. This is the exact mistake the archived build documented and then
//! fixed: drawing offers from the economy stream meant that merely *looking* at
//! what was on offer shifted every later economic roll. A per-month stream is a
//! pure function of `(seed, month)`, so it can be recomputed at any time and in
//! any order.
//!
//! # Failure is priced, not fatal
//!
//! A missed deadline costs a fine on the undelivered share and sours relations
//! with that bloc, which makes its prices worse for a while. Relations decay
//! back on their own, so a bad year is a scar rather than a permanent state —
//! and the fine can never overdraw a treasury, because the republic does not
//! run an overdraft it never agreed to.

use crate::resource::Resource;
use crate::trade::Market;
use crate::units::Tonnes;
use serde::{Deserialize, Serialize};

/// A new tender lands every other month, if there is a crossing to land at.
pub const OFFER_EVERY_MONTHS: u64 = 2;

/// How many unaccepted offers may be on the table at once.
pub const MAX_OPEN_OFFERS: usize = 2;

/// Value band of an eastern tender, in roubles. Orders are value-banded rather
/// than tonnage-banded: a machinery tender is a few tonnes and a coal tender is
/// a trainload, but both are worth comparable money.
pub const VALUE_BAND_EAST: (f64, f64) = (250.0, 1_250.0);

/// The same for the west, in dollars.
pub const VALUE_BAND_WEST: (f64, f64) = (125.0, 625.0);

pub const MIN_TONNES: f64 = 2.0;
pub const MAX_TONNES: f64 = 200.0;

/// Premium over the market price, locked at offer time.
pub const PREMIUM: (f64, f64) = (0.15, 0.25);

/// Days to deliver, drawn per tender.
pub const DEADLINE_DAYS: (u64, u64) = (60, 90);

/// How long an unaccepted offer stands.
pub const OFFER_DAYS: u64 = 30;

/// Share of the undelivered value charged as a fine on failure.
pub const FINE_SHARE: f64 = 0.25;

/// Price penalty added per failed contract.
pub const RELATIONS_HIT: f64 = 0.12;

/// The worst relations can get.
pub const RELATIONS_CAP: f64 = 0.25;

/// How fast a bloc forgets. At 0.002 a day a single failure haunts prices for
/// about two months.
pub const RELATIONS_DECAY_PER_DAY: f64 = 0.002;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContractId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractState {
    /// On the table, not yet accepted.
    Offer,
    /// Accepted and running.
    Active,
    /// Delivered in full.
    Done,
    /// The deadline passed with tonnage outstanding.
    Failed,
}

/// One tender.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub resource: Resource,
    pub market: Market,
    pub amount: Tonnes,
    pub delivered: Tonnes,
    /// Locked when the offer was made, so a price collapse cannot rescue a
    /// republic that took a bad deal, and a boom cannot rob it of a good one.
    pub price_per_tonne: f64,
    /// Absolute day index the goods are due by.
    pub deadline_day: u64,
    /// Absolute day index after which an unaccepted offer is withdrawn.
    pub offer_expires_day: u64,
    pub state: ContractState,
    /// When it reached `Done` or `Failed`, for pruning the history.
    pub closed_day: Option<u64>,
}

impl Contract {
    pub fn outstanding(&self) -> Tonnes {
        self.amount.saturating_sub(self.delivered)
    }

    pub fn is_live(&self) -> bool {
        self.state == ContractState::Active
    }

    /// What failing right now would cost.
    pub fn fine(&self) -> f64 {
        FINE_SHARE * self.outstanding().0 * self.price_per_tonne
    }
}

/// Every tender, and how each bloc currently feels about the republic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Contracts {
    list: Vec<Contract>,
    next_id: u32,
    /// Price penalty per bloc, `0.0..=RELATIONS_CAP`.
    east_penalty: f64,
    west_penalty: f64,
}

impl Contracts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[Contract] {
        &self.list
    }

    pub fn get(&self, id: ContractId) -> Option<&Contract> {
        self.list.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: ContractId) -> Option<&mut Contract> {
        self.list.iter_mut().find(|c| c.id == id)
    }

    pub fn offers(&self) -> impl Iterator<Item = &Contract> {
        self.list.iter().filter(|c| c.state == ContractState::Offer)
    }

    pub fn active(&self) -> impl Iterator<Item = &Contract> {
        self.list.iter().filter(|c| c.is_live())
    }

    /// The id the next tender will carry. Reserved by the offer system so ids
    /// stay unique across a save.
    pub fn reserve_id(&mut self) -> ContractId {
        self.next_id += 1;
        ContractId(self.next_id)
    }

    pub fn insert(&mut self, contract: Contract) {
        debug_assert!(
            self.get(contract.id).is_none(),
            "contract id {:?} is already taken",
            contract.id
        );
        // Keep `next_id` ahead of anything inserted directly, so a contract
        // added by a scenario cannot collide with one the system later offers.
        self.next_id = self.next_id.max(contract.id.0);
        self.list.push(contract);
    }

    pub fn remove(&mut self, id: ContractId) -> bool {
        let before = self.list.len();
        self.list.retain(|c| c.id != id);
        self.list.len() != before
    }

    /// How sour a bloc is on the republic.
    pub fn penalty(&self, market: Market) -> f64 {
        match market {
            Market::East => self.east_penalty,
            Market::West => self.west_penalty,
        }
    }

    pub fn set_penalty(&mut self, market: Market, value: f64) {
        let clamped = value.clamp(0.0, RELATIONS_CAP);
        match market {
            Market::East => self.east_penalty = clamped,
            Market::West => self.west_penalty = clamped,
        }
    }

    /// What a tonne fetches at the border today, after relations.
    ///
    /// A bloc that has been let down pays less and charges more. Both directions
    /// move, deliberately: a penalty that only touched exports would be a mild
    /// inconvenience to a republic that mostly imports.
    pub fn sell_price(&self, market: Market, resource: Resource) -> f64 {
        market.sell_price(resource) * (1.0 - self.penalty(market))
    }

    pub fn buy_price(&self, market: Market, resource: Resource) -> f64 {
        market.buy_price(resource) * (1.0 + self.penalty(market))
    }

    /// Accept an offer. A player action, not something a system decides.
    ///
    /// Returns false if there is no such offer — including one that has already
    /// been accepted, so accepting twice is not a second success.
    pub fn accept(&mut self, id: ContractId) -> bool {
        match self.get_mut(id) {
            Some(c) if c.state == ContractState::Offer => {
                c.state = ContractState::Active;
                true
            }
            _ => false,
        }
    }

    /// Turn an offer down. It leaves the table immediately.
    pub fn decline(&mut self, id: ContractId) -> bool {
        match self.get(id) {
            Some(c) if c.state == ContractState::Offer => self.remove(id),
            _ => false,
        }
    }

    /// Live contracts an export of this good could count against, **soonest
    /// deadline first**.
    ///
    /// A list rather than a single answer because a caller filling several in
    /// one pass has to be able to move on once one is satisfied — see the
    /// scratch ledger in the trade system. Soonest-first means a republic works
    /// off the obligation that is about to bite rather than cherry-picking the
    /// profitable one; ties break on id so the answer is reproducible when two
    /// tenders fall due on the same day.
    pub fn claimants(&self, resource: Resource, market: Market) -> Vec<ContractId> {
        let mut candidates: Vec<&Contract> = self
            .list
            .iter()
            .filter(|c| c.is_live() && c.resource == resource && c.market == market)
            .filter(|c| c.outstanding().is_positive())
            .collect();
        candidates.sort_by_key(|c| (c.deadline_day, c.id));
        candidates.into_iter().map(|c| c.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tender(id: u32, market: Market, deadline: u64) -> Contract {
        Contract {
            id: ContractId(id),
            resource: Resource::Coal,
            market,
            amount: Tonnes(100.0),
            delivered: Tonnes::ZERO,
            price_per_tonne: 3.0,
            deadline_day: deadline,
            offer_expires_day: 90,
            state: ContractState::Active,
            closed_day: None,
        }
    }

    #[test]
    fn accepting_an_offer_makes_it_live_and_only_once() {
        let mut c = Contracts::new();
        let mut offer = tender(1, Market::East, 120);
        offer.state = ContractState::Offer;
        c.insert(offer);

        assert!(c.accept(ContractId(1)));
        assert!(c.get(ContractId(1)).unwrap().is_live());
        assert!(
            !c.accept(ContractId(1)),
            "accepting twice is not a second success"
        );
        assert!(!c.accept(ContractId(9)), "no such tender");
    }

    #[test]
    fn declining_takes_it_off_the_table() {
        let mut c = Contracts::new();
        let mut offer = tender(1, Market::East, 120);
        offer.state = ContractState::Offer;
        c.insert(offer);
        assert!(c.decline(ContractId(1)));
        assert!(c.get(ContractId(1)).is_none());
        assert!(!c.decline(ContractId(1)));
    }

    /// A live contract cannot be declined — that is what taking one on means.
    #[test]
    fn an_accepted_contract_cannot_be_walked_away_from() {
        let mut c = Contracts::new();
        c.insert(tender(1, Market::East, 120));
        assert!(!c.decline(ContractId(1)));
        assert!(c.get(ContractId(1)).is_some());
    }

    #[test]
    fn the_soonest_deadline_is_served_first() {
        let mut c = Contracts::new();
        c.insert(tender(1, Market::East, 200));
        c.insert(tender(2, Market::East, 100));
        c.insert(tender(3, Market::West, 50));
        assert_eq!(
            c.claimants(Resource::Coal, Market::East),
            vec![ContractId(2), ContractId(1)],
            "the nearer deadline should claim the load first"
        );
        // A market with no live tender claims nothing, and neither does a
        // resource nobody ordered.
        assert!(c.claimants(Resource::Steel, Market::East).is_empty());
    }

    #[test]
    fn a_filled_contract_stops_claiming() {
        let mut c = Contracts::new();
        c.insert(tender(1, Market::East, 100));
        c.get_mut(ContractId(1)).unwrap().delivered = Tonnes(100.0);
        assert!(c.claimants(Resource::Coal, Market::East).is_empty());
    }

    /// A soured bloc pays less and charges more — both directions, or a
    /// republic that mostly imports would barely feel a failure.
    #[test]
    fn relations_move_prices_both_ways() {
        let mut c = Contracts::new();
        let clean_sell = c.sell_price(Market::East, Resource::Coal);
        let clean_buy = c.buy_price(Market::East, Resource::Coal);

        c.set_penalty(Market::East, RELATIONS_HIT);
        assert!(c.sell_price(Market::East, Resource::Coal) < clean_sell);
        assert!(c.buy_price(Market::East, Resource::Coal) > clean_buy);
        // And the other bloc does not care.
        assert_eq!(
            c.sell_price(Market::West, Resource::Coal),
            Market::West.sell_price(Resource::Coal)
        );
    }

    #[test]
    fn relations_cannot_be_driven_past_the_cap() {
        let mut c = Contracts::new();
        c.set_penalty(Market::East, 5.0);
        assert_eq!(c.penalty(Market::East), RELATIONS_CAP);
        c.set_penalty(Market::East, -1.0);
        assert_eq!(c.penalty(Market::East), 0.0);
    }

    /// Trading at the border must stay a loss even at the worst relations, or a
    /// republic could arbitrage its own disgrace.
    #[test]
    fn even_soured_relations_never_make_a_round_trip_profitable() {
        let mut c = Contracts::new();
        for penalty in [0.0, RELATIONS_HIT, RELATIONS_CAP] {
            c.set_penalty(Market::East, penalty);
            c.set_penalty(Market::West, penalty);
            for market in [Market::East, Market::West] {
                for resource in Resource::ALL {
                    assert!(
                        c.sell_price(market, resource) < c.buy_price(market, resource),
                        "{resource:?} on {market:?} at penalty {penalty}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_fine_is_a_share_of_what_was_not_delivered() {
        let mut c = tender(1, Market::East, 100);
        c.delivered = Tonnes(40.0);
        assert_eq!(c.outstanding(), Tonnes(60.0));
        assert!((c.fine() - FINE_SHARE * 60.0 * 3.0).abs() < 1e-12);
        // Delivered in full, nothing owed.
        c.delivered = Tonnes(100.0);
        assert_eq!(c.fine(), 0.0);
    }

    #[test]
    fn ids_never_collide_with_one_inserted_directly() {
        let mut c = Contracts::new();
        c.insert(tender(7, Market::East, 100));
        let next = c.reserve_id();
        assert!(next.0 > 7, "the system reissued a live id");
    }
}
