//! People arriving, and people leaving.
//!
//! # An immigrant is not a number that goes up
//!
//! A republic that is worth living in attracts people, and they turn up **at a
//! frontier post** — the same way foreign builders do, and for the same reason:
//! this game's standing rule is that if a thing is physical then its solution is
//! a physical thing that exists in the world. A settler who materialised in an
//! apartment block would be the "click a button and it happens" shape the whole
//! build exists to refuse.
//!
//! So a [`Group`] stands at a post. It has to be fetched by a coach, over the
//! roads the republic has built, and set down at housing with room in it. Until
//! then it is people at the border, visible on the map, doing nothing for you.
//!
//! # And a group that is never fetched goes home
//!
//! [`PATIENCE`] is why this cannot pile up. A republic with no bus depot, or no
//! road to the post its settlers arrived at, does not accumulate an unbounded
//! crowd at the border — the crowd gives up. That is a consequence rather than
//! a tidy-up: the republic was offered people and could not reach them.
//!
//! # Leaving is not the same shape
//!
//! Emigration deliberately has no journey. Somebody who has decided to go does
//! not need the republic's permission or its transport, and modelling a queue of
//! people waiting for a coach out would make a failing republic *retain* people
//! by failing harder. They are simply gone, and the count is reported.

use crate::fleet::VehicleId;
use crate::units::Point;
use serde::{Deserialize, Serialize};

/// A stable handle to a group of arrivals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GroupId(pub u32);

/// How long a group will stand at a post waiting to be collected, in days.
///
/// A season. Long enough that a republic which is building a road to the post
/// can still get there, short enough that a republic which never will is told
/// so rather than quietly hoarding a queue.
pub const PATIENCE: u64 = 90;

/// People standing at a frontier post, waiting to be carried in.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    /// The post they walked up to.
    pub at: Point,
    pub heads: u32,
    /// The coach carrying them, if one has picked them up.
    pub riding: Option<VehicleId>,
    /// The day they arrived — what [`PATIENCE`] is measured against.
    pub since: u64,
}

impl Group {
    /// Standing at the border with no lift coming: what asks a bus depot for a
    /// coach.
    pub fn is_waiting(&self) -> bool {
        self.riding.is_none()
    }

    /// How long they have been standing there, in days.
    pub fn waited(&self, today: u64) -> u64 {
        today.saturating_sub(self.since)
    }

    /// Whether they have given up.
    pub fn has_given_up(&self, today: u64) -> bool {
        self.riding.is_none() && self.waited(today) >= PATIENCE
    }
}

/// Everybody trying to get in, and the tally of everybody who has gone.
///
/// The tallies are cumulative and they are simulation state rather than a
/// statistic: "forty-one people left last year" is the only way a player can
/// see a slow bleed, and a number that is not stored cannot be shown.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Migration {
    list: Vec<Group>,
    next_id: u32,
    /// Everyone who has ever been carried in and settled.
    settled: u32,
    /// Everyone who has ever left.
    left: u32,
    /// Everyone who stood at a post until [`PATIENCE`] ran out.
    gave_up: u32,
}

impl Migration {
    pub fn new() -> Self {
        Self {
            list: Vec::new(),
            next_id: 1,
            settled: 0,
            left: 0,
            gave_up: 0,
        }
    }

    pub fn all(&self) -> &[Group] {
        &self.list
    }

    pub fn get(&self, id: GroupId) -> Option<&Group> {
        self.list.iter().find(|g| g.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.list.iter_mut().find(|g| g.id == id)
    }

    /// How many people are standing at the border right now, fetched or not.
    pub fn waiting_heads(&self) -> u32 {
        self.list.iter().map(|g| g.heads).sum()
    }

    /// Groups nobody has been sent for, oldest first.
    pub fn unfetched(&self) -> impl Iterator<Item = &Group> {
        self.list.iter().filter(|g| g.is_waiting())
    }

    /// The group aboard a coach, if it is carrying one.
    pub fn riding(&self, vehicle: VehicleId) -> Option<&Group> {
        self.list.iter().find(|g| g.riding == Some(vehicle))
    }

    pub fn settled(&self) -> u32 {
        self.settled
    }

    pub fn left(&self) -> u32 {
        self.left
    }

    pub fn gave_up(&self) -> u32 {
        self.gave_up
    }

    pub(crate) fn arrive(&mut self, at: Point, heads: u32, day: u64) -> GroupId {
        let id = GroupId(self.next_id);
        self.next_id += 1;
        self.list.push(Group {
            id,
            at,
            heads,
            riding: None,
            since: day,
        });
        id
    }

    /// They are in. The heads become citizens; the group stops existing.
    pub(crate) fn settle(&mut self, id: GroupId) -> Option<Group> {
        let index = self.list.iter().position(|g| g.id == id)?;
        let group = self.list.remove(index);
        self.settled += group.heads;
        Some(group)
    }

    /// Nobody came for them.
    pub(crate) fn give_up(&mut self, id: GroupId) -> Option<Group> {
        let index = self.list.iter().position(|g| g.id == id)?;
        let group = self.list.remove(index);
        self.gave_up += group.heads;
        Some(group)
    }

    pub(crate) fn record_departures(&mut self, heads: u32) {
        self.left += heads;
    }

    /// Settlers a coach carried to a block that could not take them.
    ///
    /// Counted with the ones who gave up at the border, because from the
    /// republic's side they are the same thing: people it was offered and did
    /// not house. It is a consequence of pulling housing down with a coach in
    /// the air, and it is recorded rather than swallowed so the number a panel
    /// shows is the truth.
    pub(crate) fn record_turned_away(&mut self, heads: u32) {
        self.gave_up += heads;
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
    fn a_group_waits_until_something_comes_for_it() {
        let mut m = Migration::new();
        let id = m.arrive(at(0.0, 0.0), 12, 100);
        assert_eq!(m.waiting_heads(), 12);
        assert_eq!(m.unfetched().count(), 1);

        m.get_mut(id).unwrap().riding = Some(VehicleId(3));
        assert_eq!(m.unfetched().count(), 0, "a coach is on its way");
        assert!(m.riding(VehicleId(3)).is_some());
        assert_eq!(m.waiting_heads(), 12, "still at the border until set down");

        m.settle(id);
        assert_eq!(m.settled(), 12);
        assert_eq!(m.waiting_heads(), 0);
    }

    /// The bound that stops a republic with no transport hoarding a crowd.
    /// Both halves matter: a fetched group never gives up, and an unfetched one
    /// does so exactly once.
    #[test]
    fn a_group_nobody_fetches_gives_up_and_a_fetched_one_never_does() {
        let mut m = Migration::new();
        let ignored = m.arrive(at(0.0, 0.0), 10, 0);
        let collected = m.arrive(at(0.0, 0.0), 10, 0);
        m.get_mut(collected).unwrap().riding = Some(VehicleId(1));

        assert!(!m.get(ignored).unwrap().has_given_up(PATIENCE - 1));
        assert!(m.get(ignored).unwrap().has_given_up(PATIENCE));
        assert!(
            !m.get(collected).unwrap().has_given_up(PATIENCE * 10),
            "a group aboard a coach is not waiting for anything"
        );

        m.give_up(ignored);
        assert_eq!(m.gave_up(), 10);
        assert!(m.give_up(ignored).is_none(), "and only once");
    }

    #[test]
    fn a_migration_ledger_survives_a_save() {
        let mut m = Migration::new();
        m.arrive(at(100.0, 200.0), 8, 5);
        m.record_departures(3);
        let wire = postcard::to_stdvec(&m).expect("serializes");
        let back: Migration = postcard::from_bytes(&wire).expect("parses");
        assert_eq!(back, m);
        assert_eq!(back.left(), 3);
    }
}
