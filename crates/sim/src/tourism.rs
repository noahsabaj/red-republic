//! Visitors from abroad, and the hard currency they leave behind.
//!
//! # A second way to earn dollars, and a geographic one
//!
//! Everything else that earns the republic foreign money is a lorry full of
//! something driven to a frontier post. Tourism is the other kind of export:
//! what is sold is the republic itself, and it is bought by people who have to
//! be **fetched from the border like anybody else**.
//!
//! A [`Visit`] arrives at a post of its own bloc — Western visitors at a Western
//! post, and their money is dollars — so which stretch of the perimeter a
//! republic can reach decides what its tourism is worth, exactly as it decides
//! what its coal is worth. That is the same geography the trade rules already
//! answer to, and it needed no new mechanism.
//!
//! # What they pay for is what is near where they sleep
//!
//! A party takes beds in a [`crate::building::BuildingKind::Hotel`] and spends
//! every day it stays. How much depends on what is within walking distance of
//! that hotel: culture the republic built, and air it has kept clean. A hotel in
//! a smog beside nothing earns a fraction of one in a town worth visiting, and
//! neither of those inputs is new — the same `serves` cover the contentment pass
//! reads, and the same pollution lattice.
//!
//! # The bounds are the same two migration already has
//!
//! **Nobody comes to a republic with nowhere to sleep**, so arrivals are capped
//! by free beds: a republic without a hotel has no tourism at all, and that is a
//! consequence rather than a lock. And a party nobody comes for gives up, on the
//! same [`crate::migration::PATIENCE`] a settler group does, which is what stops
//! a republic with no coaches hoarding a crowd at its border.
//!
//! # Leaving has no journey, for the reason emigration has none
//!
//! A visitor whose fortnight is up goes home. Making them queue for a coach out
//! would mean a republic that failed to collect them *kept* them, and their beds
//! with them — a failure that rewards itself.

use crate::fleet::VehicleId;
use crate::trade::Market;
use crate::units::Point;
use serde::{Deserialize, Serialize};

/// A stable handle to a party of visitors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct VisitId(pub u32);

/// How long a party stays once it has a bed, in days.
///
/// A fortnight. Long enough that fetching one is worth the diesel, short enough
/// that a republic's beds turn over and a hotel is a recurring earner rather
/// than a building that fills once.
pub const STAY: u64 = 14;

/// The largest party that will turn up at a post at once.
///
/// A coachload, and bounded for the reason [`crate::wellbeing::ARRIVAL_PARTY`]
/// is: a republic that has just opened its first hotel should get visitors as a
/// stream it can collect rather than a crowd standing at the border.
pub const PARTY: u32 = 20;

/// What one visitor spends in a day, in their own bloc's money, at a republic
/// that has given them everything to look at.
///
/// **Deliberately worth a lorry-load of coal and not a mine.** Twenty visitors
/// for a fortnight at full appeal is a couple of hundred dollars — real money
/// against a coal price of one dollar a tonne, and nothing like enough to be
/// the only thing a republic does. Tourism is a second income, not a bypass.
pub const SPEND_PER_HEAD_PER_DAY: f64 = 0.9;

/// How little a republic can be worth visiting and still be visited at all.
///
/// A floor rather than zero, for the reason the drought curve has one: a
/// multiplier that reaches zero makes the whole mechanic unreachable until some
/// other thing is built, and an empty steppe posting with a hotel should earn
/// *something* for the trouble of having built one.
pub const APPEAL_FLOOR: f64 = 0.15;

/// People from abroad, on their way in or already here.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Visit {
    pub id: VisitId,
    /// Where they are: a frontier post until a coach sets them down.
    pub at: Point,
    pub heads: u32,
    /// Which bloc they came from, and therefore which currency they spend.
    pub market: Market,
    /// The coach carrying them, if one has picked them up.
    pub riding: Option<VehicleId>,
    /// The hotel they are staying in, once they are in one.
    pub staying_at: Option<crate::building::BuildingId>,
    /// The day they turned up at the post — what patience is measured against.
    pub since: u64,
    /// The day they go home. Only meaningful once `staying_at` is set.
    pub until: u64,
}

impl Visit {
    /// Standing at the border with no lift coming: what asks a bus depot for a
    /// coach.
    pub fn is_waiting(&self) -> bool {
        self.riding.is_none() && self.staying_at.is_none()
    }

    /// Whether they are in a hotel, spending.
    pub fn is_staying(&self) -> bool {
        self.staying_at.is_some()
    }

    pub fn waited(&self, today: u64) -> u64 {
        today.saturating_sub(self.since)
    }

    /// Whether they gave up waiting to be collected.
    pub fn has_given_up(&self, today: u64) -> bool {
        self.is_waiting() && self.waited(today) >= crate::migration::PATIENCE
    }

    /// Whether their stay is over.
    pub fn is_done(&self, today: u64) -> bool {
        self.is_staying() && today >= self.until
    }
}

/// Everybody visiting, and what they have been worth.
///
/// The tallies are cumulative and they are simulation state rather than a
/// statistic, for the reason [`crate::migration::Migration`]'s are: "your hotels
/// earned nine hundred dollars last year" is the only way the whole mechanic is
/// visible at all, and a number that is not stored cannot be shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tourism {
    list: Vec<Visit>,
    next_id: u32,
    /// Everyone who has ever stayed.
    visited: u32,
    /// Everyone who stood at a post until their patience ran out.
    turned_away: u32,
    /// What they have spent, per bloc, in that bloc's own money.
    earned_east: f64,
    earned_west: f64,
}

impl Tourism {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
            visited: 0,
            turned_away: 0,
            earned_east: 0.0,
            earned_west: 0.0,
        }
    }

    pub fn all(&self) -> &[Visit] {
        &self.list
    }

    pub fn get(&self, id: VisitId) -> Option<&Visit> {
        self.list.iter().find(|v| v.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: VisitId) -> Option<&mut Visit> {
        self.list.iter_mut().find(|v| v.id == id)
    }

    /// Parties nobody has been sent for.
    pub fn unfetched(&self) -> impl Iterator<Item = &Visit> {
        self.list.iter().filter(|v| v.is_waiting())
    }

    /// The party aboard a coach, if it is carrying one.
    pub fn riding(&self, vehicle: VehicleId) -> Option<&Visit> {
        self.list.iter().find(|v| v.riding == Some(vehicle))
    }

    /// How many visitors are asleep in the republic's hotels tonight.
    pub fn staying_heads(&self) -> u32 {
        self.list
            .iter()
            .filter(|v| v.is_staying())
            .map(|v| v.heads)
            .sum()
    }

    /// How many are standing at a post waiting to be collected.
    pub fn waiting_heads(&self) -> u32 {
        self.list
            .iter()
            .filter(|v| !v.is_staying())
            .map(|v| v.heads)
            .sum()
    }

    /// Beds a party already in or heading to this hotel has taken.
    pub fn booked_at(&self, hotel: crate::building::BuildingId) -> u32 {
        self.list
            .iter()
            .filter(|v| v.staying_at == Some(hotel))
            .map(|v| v.heads)
            .sum()
    }

    pub fn visited(&self) -> u32 {
        self.visited
    }

    pub fn turned_away(&self) -> u32 {
        self.turned_away
    }

    /// What visitors from a bloc have spent, in that bloc's money.
    pub fn earned(&self, market: Market) -> f64 {
        match market {
            Market::East => self.earned_east,
            Market::West => self.earned_west,
        }
    }

    pub(crate) fn arrive(&mut self, at: Point, heads: u32, market: Market, day: u64) -> VisitId {
        let id = VisitId(self.next_id);
        self.next_id += 1;
        self.list.push(Visit {
            id,
            at,
            heads,
            market,
            riding: None,
            staying_at: None,
            since: day,
            until: 0,
        });
        id
    }

    /// They are in a hotel and the clock on their stay starts.
    pub(crate) fn check_in(
        &mut self,
        id: VisitId,
        hotel: crate::building::BuildingId,
        at: Point,
        day: u64,
    ) {
        if let Some(v) = self.get_mut(id) {
            v.riding = None;
            v.staying_at = Some(hotel);
            v.at = at;
            v.until = day + STAY;
        }
        self.visited += self.get(id).map_or(0, |v| v.heads);
    }

    pub(crate) fn take(&mut self, market: Market, amount: f64) {
        match market {
            Market::East => self.earned_east += amount,
            Market::West => self.earned_west += amount,
        }
    }

    /// A party stops existing — either its stay ended or nobody ever came.
    ///
    /// One method for both, and the ledger tells them apart by whether they ever
    /// reached a hotel. A party that gave up at the border is a republic's
    /// failure to reach it; a party that went home after a fortnight is the
    /// mechanic working.
    pub(crate) fn end(&mut self, id: VisitId) -> Option<Visit> {
        let index = self.list.iter().position(|v| v.id == id)?;
        let visit = self.list.remove(index);
        if !visit.is_staying() {
            self.turned_away += visit.heads;
        }
        Some(visit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::building::BuildingId;
    use crate::units::Metres;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    #[test]
    fn a_party_waits_then_stays_then_goes_home() {
        let mut t = Tourism::new();
        let id = t.arrive(at(0.0, 0.0), 12, Market::West, 100);
        assert_eq!(t.waiting_heads(), 12);
        assert_eq!(t.staying_heads(), 0);
        assert_eq!(t.unfetched().count(), 1);

        t.get_mut(id).unwrap().riding = Some(VehicleId(3));
        assert_eq!(t.unfetched().count(), 0, "a coach is on its way");
        assert!(t.riding(VehicleId(3)).is_some());

        t.check_in(id, BuildingId(7), at(500.0, 500.0), 100);
        assert_eq!(t.staying_heads(), 12);
        assert_eq!(t.waiting_heads(), 0);
        assert_eq!(t.visited(), 12);
        assert_eq!(t.booked_at(BuildingId(7)), 12);

        assert!(!t.get(id).unwrap().is_done(100 + STAY - 1));
        assert!(t.get(id).unwrap().is_done(100 + STAY));

        t.end(id);
        assert_eq!(t.staying_heads(), 0);
        assert_eq!(
            t.turned_away(),
            0,
            "somebody who stayed a fortnight was not turned away"
        );
    }

    /// The bound that stops a republic with no coaches hoarding a crowd, and
    /// the ledger telling the two kinds of departure apart.
    #[test]
    fn a_party_nobody_fetches_gives_up_and_is_counted_as_such() {
        let mut t = Tourism::new();
        let ignored = t.arrive(at(0.0, 0.0), 10, Market::East, 0);
        let collected = t.arrive(at(0.0, 0.0), 10, Market::East, 0);
        t.check_in(collected, BuildingId(1), at(10.0, 10.0), 0);

        assert!(
            !t.get(ignored)
                .unwrap()
                .has_given_up(crate::migration::PATIENCE - 1)
        );
        assert!(
            t.get(ignored)
                .unwrap()
                .has_given_up(crate::migration::PATIENCE)
        );
        assert!(
            !t.get(collected)
                .unwrap()
                .has_given_up(crate::migration::PATIENCE * 10),
            "a party asleep in a hotel is not waiting for anything"
        );

        t.end(ignored);
        assert_eq!(t.turned_away(), 10);
        assert!(t.end(ignored).is_none(), "and only once");
    }

    /// Money is kept per bloc, because a dollar and a rouble are not the same
    /// thing anywhere else in this simulation either.
    #[test]
    fn takings_are_kept_in_the_currency_they_were_spent_in() {
        let mut t = Tourism::new();
        t.take(Market::West, 40.0);
        t.take(Market::East, 15.0);
        t.take(Market::West, 2.5);
        assert_eq!(t.earned(Market::West), 42.5);
        assert_eq!(t.earned(Market::East), 15.0);
    }

    #[test]
    fn a_tourism_ledger_survives_a_save() {
        let mut t = Tourism::new();
        t.arrive(at(100.0, 200.0), 8, Market::West, 5);
        t.take(Market::West, 12.0);
        let wire = postcard::to_stdvec(&t).expect("serializes");
        let back: Tourism = postcard::from_bytes(&wire).expect("parses");
        assert_eq!(back, t);
        assert_eq!(back.earned(Market::West), 12.0);
    }
}
