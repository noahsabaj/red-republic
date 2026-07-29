//! Commands: the only way anything outside this crate changes the republic.
//!
//! # A command is not a mutation
//!
//! [`crate::systems::Mutation`] is what a *system* proposes to change, and it
//! is the internal write-set the single writer applies. A [`Command`] is what a
//! *person* asked for. The archived build kept these apart deliberately and so
//! does this one: conflating them would make "record the inputs" quietly mean
//! "record the outputs", which is a different and much less useful thing. A
//! replay needs to know that someone ordered a road, not that eleven tonnes of
//! gravel moved.
//!
//! # Why the surface is sealed at all
//!
//! Until this module existed, every field on [`crate::World`] was reachable
//! from outside the crate, and so was `get_mut` on every structure under it —
//! about twenty-five entry points in total. A shell could write what no system
//! was allowed to write: the exact `{field, value}` escape hatch the
//! single-writer rule refuses, left open to the UI by accident rather than by
//! decision.
//!
//! It also meant the determinism rule was only half true. "Same seed and same
//! inputs produce the same world" had no *inputs* — there was no such thing —
//! so `a_reloaded_world_resumes_the_same_future` proved replay for a world
//! nobody was playing.
//!
//! Everything below [`crate::World`] is now `pub(crate)`. Systems are inside
//! the crate and are unaffected; anything outside it reads through views and
//! writes through here. **The enforcement is the compiler, not a convention** —
//! and `src/bin/trajectory.rs` and `tests/baselines.rs` are separate crates, so
//! they are held to exactly the boundary the shell will be.
//!
//! # Refusals carry their reason
//!
//! A command returns why it failed, not merely that it did. Those strings are
//! what a panel prints in a toast and what greys out a button with a tooltip
//! explaining itself. The archived build had this from the start and it is
//! cheaper to build in than to retrofit.

use crate::building::{BuildingId, BuildingKind, PlacementError};
use crate::contract::ContractId;
use crate::resource::Resource;
use crate::roadworks::{Grade, RoadError, RoadSiteId};
use crate::trade::{Market, TradeAction, TradeRule};
use crate::units::Point;
use serde::{Deserialize, Serialize};

/// One thing the player asked the republic to do.
///
/// Every variant is a decision a person makes. Consequences are systems' work
/// and never appear here — there is no `Command::Produce`, and there will not
/// be one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Commission a building. It goes up as a site and is built by the crew.
    Place { kind: BuildingKind, at: Point },

    /// Pull a building down.
    Demolish { building: BuildingId },

    /// Order a road. It is a site until the crew and the gravel reach it.
    OrderRoad {
        from: Point,
        to: Point,
        grade: Grade,
    },

    /// Take a tender the Foreign Trade Directorate has offered.
    AcceptContract { contract: ContractId },

    /// Turn one down. It leaves the table immediately.
    DeclineContract { contract: ContractId },

    /// Add a standing instruction to the customs houses.
    AddTradeRule {
        resource: Resource,
        market: Market,
        action: TradeAction,
    },

    /// Withdraw one.
    RemoveTradeRule { index: u32 },

    /// Take an advance from a bloc. `tier` indexes `loan::TIERS`.
    TakeLoan { market: Market, tier: u32 },

    /// Pay some of an advance back. More than is owed pays off what is owed.
    RepayLoan { market: Market, amount: f64 },

    /// Move one up or down the running order.
    ///
    /// Its own command rather than a re-send of the whole policy, because the
    /// order **is** the decision: when throughput or money runs short the first
    /// rule is served first, and that ranking is the player's to make.
    MoveTradeRule { from: u32, to: u32 },
}

/// What an accepted command produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Done {
    /// It was carried out and there is nothing to hand back.
    Nothing,
    /// A building was commissioned.
    Commissioned(BuildingId),
    /// A road was ordered.
    Ordered(RoadSiteId),
}

/// Why a command was refused, in words a panel can print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The ground, the geology or the border said no.
    Placement(PlacementError),
    /// The road could not be ordered.
    Road(RoadError),
    /// There is no building with that id — it may have been demolished.
    NoSuchBuilding(BuildingId),
    /// There is no offer with that id. Includes one already accepted, so
    /// accepting twice is not a second success.
    NoSuchOffer(ContractId),
    /// There is no trade rule at that position.
    NoSuchRule { index: u32, rules: u32 },
    /// The advance could not be taken or repaid.
    Loan(crate::loan::LoanError),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Placement(why) => write!(f, "{why}"),
            Refused::Road(why) => write!(f, "{why}"),
            Refused::NoSuchBuilding(id) => write!(f, "there is no building {}", id.0),
            // The id is carried for the caller, never shown: a contract number
            // is not something a player has ever seen or could act on.
            Refused::NoSuchOffer(_) => write!(f, "that tender is no longer on the table"),
            Refused::Loan(why) => write!(f, "{why}"),
            Refused::NoSuchRule { index, rules } => match rules {
                0 => write!(f, "there are no trade rules to change"),
                1 => write!(f, "there is only one trade rule, and it is not {index}"),
                n => write!(f, "there is no trade rule {index}; there are {n}"),
            },
        }
    }
}

impl std::error::Error for Refused {}

impl From<PlacementError> for Refused {
    fn from(why: PlacementError) -> Self {
        Refused::Placement(why)
    }
}

impl From<RoadError> for Refused {
    fn from(why: RoadError) -> Self {
        Refused::Road(why)
    }
}

impl From<crate::loan::LoanError> for Refused {
    fn from(why: crate::loan::LoanError) -> Self {
        Refused::Loan(why)
    }
}

/// What a command call answers.
pub type Outcome = Result<Done, Refused>;

/// One command, and when it was carried out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issued {
    /// The tick the republic was on when this was done.
    pub tick: u64,
    pub command: Command,
}

/// Everything the player has actually done, in order.
///
/// **Refused commands are not recorded**, and that is not an omission: a
/// refusal changes nothing, so replaying it would be replaying a no-op. What
/// the journal has to contain is exactly the set of things that moved the
/// world, or a replay is not a replay.
///
/// It travels inside the save, which makes a save a record of how its republic
/// came to be rather than only what it currently is. That is worth the bytes:
/// player commands arrive at human speed, so a decade of play is tens of
/// thousands of small entries, and being able to reproduce any reported bug
/// from the save alone is exactly what a release standard needs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Journal {
    entries: Vec<Issued>,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything carried out, oldest first.
    pub fn entries(&self) -> &[Issued] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Everything carried out on a given tick, in the order it was carried out.
    pub fn on_tick(&self, tick: u64) -> impl Iterator<Item = &Command> {
        self.entries
            .iter()
            .filter(move |e| e.tick == tick)
            .map(|e| &e.command)
    }

    /// The last tick anything happened on, if anything has.
    pub fn last_tick(&self) -> Option<u64> {
        self.entries.last().map(|e| e.tick)
    }

    pub(crate) fn record(&mut self, tick: u64, command: Command) {
        self.entries.push(Issued { tick, command });
    }
}

/// Apply a policy edit to a rule list, or say why not.
///
/// Split out of `World` because it is pure list surgery over the player's
/// running order and nothing about it needs a world.
pub(crate) fn edit_rules(rules: &mut Vec<TradeRule>, command: &Command) -> Outcome {
    let count = rules.len() as u32;
    let at = |index: u32| -> Result<usize, Refused> {
        if (index as usize) < rules.len() {
            Ok(index as usize)
        } else {
            Err(Refused::NoSuchRule {
                index,
                rules: count,
            })
        }
    };

    match *command {
        Command::AddTradeRule {
            resource,
            market,
            action,
        } => {
            rules.push(TradeRule {
                resource,
                market,
                action,
            });
            Ok(Done::Nothing)
        }
        Command::RemoveTradeRule { index } => {
            let i = at(index)?;
            rules.remove(i);
            Ok(Done::Nothing)
        }
        Command::MoveTradeRule { from, to } => {
            let (a, b) = (at(from)?, at(to)?);
            let rule = rules.remove(a);
            rules.insert(b, rule);
            Ok(Done::Nothing)
        }
        _ => unreachable!("edit_rules is only called for the trade-policy commands"),
    }
}
