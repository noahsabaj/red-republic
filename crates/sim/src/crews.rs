//! Building crews: the people a Construction Office sends out, and where they
//! are standing right now.
//!
//! # Why a crew is a thing rather than a number
//!
//! Construction used to work like freight did before lorries. Every Construction
//! Office in the republic contributed its staff to one pool of builder-days, and
//! that pool was spent on whatever site was next in the queue — a site on the far
//! side of the map cost exactly what a site next door cost, and a crew arrived
//! the instant a foundation was ordered. Nothing about it was visible and nothing
//! about it could be planned around.
//!
//! Noah's own rule for this: *"The office employs them and they commute
//! office→site. The construction office should have buses to shuttle the crew to
//! the worksite. No local crew. Construction offices own the labor and
//! machinery."* A [`Party`] is what that makes true. Builders are employed at an
//! office, they are carried to a site by a bus that burns diesel and takes time,
//! they stand there until the work is done, and they have to be fetched back.
//!
//! That is a second physical transport hop and it is deliberate: it means a
//! remote site is expensive in the currency everything else in this republic is
//! expensive in, which is vehicles, roads and hours.
//!
//! # Heads are conserved
//!
//! A party is somewhere: standing at a site, standing beside a finished one
//! waiting for a lift, or aboard a bus. It is never in two of those at once —
//! the type cannot express it, because `working` and `riding` are set and cleared
//! by the same mutations. What an office has *left* to send is its staff less
//! everyone already out ([`Crews::posted`]), so a head that is out cannot also be
//! posted somewhere else. `nobody_is_in_two_places_at_once` is the guard.

use crate::building::BuildingId;
use crate::fleet::{Destination, VehicleId};
use crate::trade::Market;
use crate::units::Point;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A stable handle to a crew.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PartyId(pub u32);

/// A gang of builders out from their office.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Party {
    pub id: PartyId,
    /// The office that employs them. They are drawn from its staff and they go
    /// back to it; an office cannot be pulled down while any of its crews are
    /// out, so this is never dangling.
    pub office: BuildingId,
    pub heads: u32,
    /// Where they are standing. While they are aboard a bus this is where the
    /// bus was when they got on — the bus is the thing to draw.
    pub at: Point,
    /// The site they are working, if they are working one.
    pub working: Option<Destination>,
    /// The bus they are aboard, if they are aboard one.
    pub riding: Option<VehicleId>,
    /// The bloc these builders were hired from, if they are foreign labour on
    /// its way in.
    ///
    /// **Set only until they reach the office.** Foreign workers arrive at a
    /// frontier post — they do not appear in the yard — so they are a gang
    /// standing at the border needing a lift, which is a state this module
    /// already models. Once the bus brings them in they join the office's books
    /// and stop being distinguishable from anybody else it employs; what stays
    /// different is that the republic pays them, daily, in their own bloc's
    /// money.
    ///
    /// It is what keeps [`Crews::posted`] honest while they travel: they are not
    /// yet part of the establishment, so they must not be subtracted from it.
    pub hired_from: Option<Market>,
}

impl Party {
    /// Standing somewhere with no site to work and no lift coming: the state
    /// that asks the office to send a bus.
    pub fn is_stranded(&self) -> bool {
        self.working.is_none() && self.riding.is_none()
    }
}

/// Every crew the republic has out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Crews {
    list: Vec<Party>,
    next_id: u32,
    /// Foreign builders on an office's books, by the bloc that supplied them.
    ///
    /// Kept by bloc because the republic pays them in **their** currency, every
    /// day, for as long as it keeps them — that ongoing cost in hard money is
    /// the whole difference between hiring abroad and training at home, and a
    /// single total could not say which purse it comes out of.
    hired: BTreeMap<(BuildingId, Market), u32>,
}

/// What a bloc charges to place one worker with you, in its own currency.
///
/// A one-off fee on top of the wage, and deliberately steep next to the wage:
/// hiring abroad is a decision you make once and then live with, not a dial you
/// turn up and down by the week. First-pass balance.
pub const HIRING_FEE: f64 = 40.0;

/// What one foreign builder is paid a day, in their own bloc's currency.
///
/// The ongoing half, and the reason this is never simply better than training
/// your own: domestic labour costs the republic nothing in money, and every day
/// a foreign gang stays is hard currency the border earned being spent again.
pub const FOREIGN_WAGE: f64 = 1.5;

impl Crews {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
            hired: BTreeMap::new(),
        }
    }

    pub fn all(&self) -> &[Party] {
        &self.list
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn get(&self, id: PartyId) -> Option<&Party> {
        self.list.iter().find(|p| p.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: PartyId) -> Option<&mut Party> {
        self.list.iter_mut().find(|p| p.id == id)
    }

    /// How many of an office's people are away from it — riding, working, or
    /// standing at a finished site waiting to be collected.
    ///
    /// This is what an office has to subtract from its staff before it can post
    /// anybody else, and it is why a head cannot be counted twice.
    pub fn posted(&self, office: BuildingId) -> u32 {
        self.list
            .iter()
            .filter(|p| p.office == office && p.hired_from.is_none())
            .map(|p| p.heads)
            .sum()
    }

    /// Foreign builders on this office's books, by bloc.
    pub fn hired(&self, office: BuildingId, market: Market) -> u32 {
        self.hired.get(&(office, market)).copied().unwrap_or(0)
    }

    /// Everybody this office is paying who is not a citizen — on its books or
    /// still standing at the post it hired them to.
    ///
    /// **The second half is the one that matters.** A gang joins `hired` only
    /// when a bus brings it home, which is right for posting them to a site and
    /// wrong for deciding whether the office can move that bus: an office with
    /// no citizens and twenty paid builders at the border could not send one,
    /// so the whole opening was a circle. See [`crate::fleet::crewed`].
    pub fn on_payroll(&self, office: BuildingId) -> u32 {
        self.hired_total(office)
            + self
                .list
                .iter()
                .filter(|p| p.office == office && p.hired_from.is_some())
                .map(|p| p.heads)
                .sum::<u32>()
    }

    /// Every foreign builder this office employs, whichever bloc sent them.
    pub fn hired_total(&self, office: BuildingId) -> u32 {
        Market::ALL.iter().map(|&m| self.hired(office, m)).sum()
    }

    /// The whole republic's foreign wage bill in one bloc's money, in heads.
    pub fn hired_from_bloc(&self, market: Market) -> u32 {
        self.hired
            .iter()
            .filter(|((_, m), _)| *m == market)
            .map(|(_, n)| n)
            .sum()
    }

    /// Every office that employs anybody from this bloc, in commissioning order.
    pub fn employers(&self, market: Market) -> Vec<(BuildingId, u32)> {
        self.hired
            .iter()
            .filter(|((_, m), n)| *m == market && **n > 0)
            .map(|((office, _), n)| (*office, *n))
            .collect()
    }

    pub(crate) fn take_on(&mut self, office: BuildingId, market: Market, heads: u32) {
        *self.hired.entry((office, market)).or_default() += heads;
    }

    /// Let foreign workers go. Returns how many actually left.
    pub(crate) fn let_go(&mut self, office: BuildingId, market: Market, heads: u32) -> u32 {
        let on_books = self.hired.entry((office, market)).or_default();
        let gone = heads.min(*on_books);
        *on_books -= gone;
        if *on_books == 0 {
            self.hired.remove(&(office, market));
        }
        gone
    }

    /// The crew working a site, if one is.
    ///
    /// One party per site, deliberately: two offices sharing a foundation would
    /// make "who is building this" a question with two answers, and the site
    /// panel has to be able to name the office that owes it a bus.
    pub fn working_at(&self, site: Destination) -> Option<&Party> {
        self.list.iter().find(|p| p.working == Some(site))
    }

    /// How many builders are standing on a site.
    pub fn at_site(&self, site: Destination) -> u32 {
        self.working_at(site).map_or(0, |p| p.heads)
    }

    /// The crew aboard a bus, if it is carrying one.
    pub fn riding(&self, vehicle: VehicleId) -> Option<&Party> {
        self.list.iter().find(|p| p.riding == Some(vehicle))
    }

    /// Crews with nothing to work and no lift coming, oldest first.
    pub fn stranded(&self) -> impl Iterator<Item = &Party> {
        self.list.iter().filter(|p| p.is_stranded())
    }

    /// Every crew an office has out, in the order they were sent.
    pub fn of_office(&self, office: BuildingId) -> impl Iterator<Item = &Party> {
        self.list.iter().filter(move |p| p.office == office)
    }

    /// Send builders out from an office, aboard a bus.
    ///
    /// They board at the office and they are on the bus from that moment: there
    /// is no state in which a party has left its office and is nowhere.
    pub(crate) fn send(
        &mut self,
        office: BuildingId,
        heads: u32,
        at: Point,
        riding: VehicleId,
    ) -> PartyId {
        self.add(office, heads, at, Some(riding), None)
    }

    /// Foreign labour, set down at the frontier post it arrived through.
    ///
    /// They start **stranded**, which is not a special case: a gang with
    /// nowhere to be and no lift is exactly what asks an office for a bus, and
    /// it is the same path a crew standing beside a finished building takes. A
    /// hired worker who could not be collected would be a hired worker standing
    /// at the border being paid, which is a consequence rather than a bug.
    pub(crate) fn arrive(
        &mut self,
        office: BuildingId,
        market: Market,
        heads: u32,
        at: Point,
    ) -> PartyId {
        self.add(office, heads, at, None, Some(market))
    }

    fn add(
        &mut self,
        office: BuildingId,
        heads: u32,
        at: Point,
        riding: Option<VehicleId>,
        hired_from: Option<Market>,
    ) -> PartyId {
        let id = PartyId(self.next_id);
        self.next_id += 1;
        self.list.push(Party {
            id,
            office,
            heads,
            at,
            working: None,
            riding,
            hired_from,
        });
        id
    }

    /// They are home. The heads go back into the office's establishment.
    pub(crate) fn dissolve(&mut self, id: PartyId) -> Option<Party> {
        let index = self.list.iter().position(|p| p.id == id)?;
        Some(self.list.remove(index))
    }

    /// Down tools: whatever they were working, they are now waiting for a lift.
    ///
    /// Called when a site finishes under them, when the player recalls them, and
    /// when a road site stops existing because it became a road.
    pub(crate) fn release(&mut self, site: Destination, at: Point) {
        for party in self.list.iter_mut().filter(|p| p.working == Some(site)) {
            party.working = None;
            party.at = at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Metres;

    fn at(x: f64, y: f64) -> Point {
        Point::new(Metres(x), Metres(y))
    }

    #[test]
    fn an_office_counts_everyone_it_has_out_once() {
        let mut crews = Crews::new();
        let office = BuildingId(1);
        let a = crews.send(office, 6, at(0.0, 0.0), VehicleId(1));
        crews.send(office, 4, at(0.0, 0.0), VehicleId(2));
        crews.send(BuildingId(2), 9, at(0.0, 0.0), VehicleId(3));

        assert_eq!(crews.posted(office), 10);
        assert_eq!(crews.posted(BuildingId(2)), 9);
        assert_eq!(crews.of_office(office).count(), 2);

        // Landing them at a site moves them; it does not duplicate them.
        let site = Destination::Building(BuildingId(7));
        let party = crews.get_mut(a).unwrap();
        party.riding = None;
        party.working = Some(site);
        assert_eq!(crews.posted(office), 10);
        assert_eq!(crews.at_site(site), 6);
        assert!(crews.riding(VehicleId(1)).is_none());
        assert!(crews.riding(VehicleId(2)).is_some());
    }

    /// A crew that has nowhere to be is what asks the office for a bus. Getting
    /// this wrong in either direction is a deadlock: never stranded and they are
    /// never fetched, always stranded and the office sends buses for ever.
    #[test]
    fn a_crew_is_stranded_only_when_it_has_no_site_and_no_lift() {
        let mut crews = Crews::new();
        let site = Destination::Building(BuildingId(7));
        let id = crews.send(BuildingId(1), 5, at(0.0, 0.0), VehicleId(1));
        assert_eq!(crews.stranded().count(), 0, "aboard a bus");

        let party = crews.get_mut(id).unwrap();
        party.riding = None;
        party.working = Some(site);
        assert_eq!(crews.stranded().count(), 0, "working");

        crews.release(site, at(100.0, 100.0));
        assert_eq!(crews.stranded().count(), 1, "the site finished under them");
        assert_eq!(crews.get(id).unwrap().at, at(100.0, 100.0));
        assert_eq!(crews.at_site(site), 0);

        crews.dissolve(id);
        assert!(crews.is_empty());
        assert_eq!(crews.posted(BuildingId(1)), 0, "they are back at work");
    }
}
