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
use crate::fleet::Destination;
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

    /// Pay a foreign firm to put a building up, instead of building it yourself.
    ///
    /// **This is how a republic that owns nothing starts.** A blank map has no
    /// Construction Office, no crews and no materials, so [`Command::Place`]
    /// puts down a site nothing can ever work. A contracted site needs none of
    /// them: it advances on its own and bills the treasury daily in `market`'s
    /// own currency, exactly as foreign labour does.
    ///
    /// It is deliberately expensive. Contractors cost several times what your
    /// own crews do, which is the whole argument for a Construction Office and
    /// the reason the opening is a question of what to buy rather than a free
    /// hand.
    ContractBuild {
        kind: BuildingKind,
        at: Point,
        market: Market,
    },

    /// Pull a building down.
    Demolish { building: BuildingId },

    /// Order a road. It is a site until the crew and the gravel reach it.
    OrderRoad {
        from: Point,
        to: Point,
        grade: Grade,
    },

    /// Order a power line or a heat main.
    ///
    /// A site until the crew and the steel reach it, and it carries nothing
    /// until then. Power and heat used to be quantities with no geography at
    /// all — a plant anywhere lit everything — and this is the command that
    /// makes both physical.
    OrderLine {
        kind: crate::utility::Utility,
        from: Point,
        to: Point,
    },

    /// Call a building crew off a site.
    ///
    /// They down tools where they stand and wait for their office to send a bus.
    /// It exists because a site whose materials never arrive would otherwise
    /// hold its gang for ever — and because the alternative, letting the
    /// simulation decide when a crew has waited long enough, is a judgment about
    /// the player's plan that the player should be making.
    RecallCrew { site: Destination },

    /// Say where sites buy materials the republic has not made.
    ///
    /// `site` names one site, or `None` for the republic's default. `crossing`
    /// names the post to import through, or `None` to import nothing — and the
    /// two `None`s mean different things, which is why neither is a bare id: a
    /// site set to `None` while the default is set has been opted *out*, and a
    /// site with no instruction at all follows the default.
    ///
    /// **This shortens no build.** It answers where a tonne of brick comes from
    /// in a republic with no brickworks; the goods land at the post and your
    /// lorries still have to fetch them.
    SetImportPolicy {
        site: Option<Destination>,
        crossing: Option<crate::trade::CrossingId>,
    },

    /// Put a site back under the republic's default policy.
    ClearImportPolicy { site: Destination },

    /// Tell a terminal or a distribution office what to keep on hand.
    ///
    /// **The command that makes a station worth building.** Nothing in the
    /// republic wants to deliver to one otherwise: a station consumes nothing
    /// and sells nothing, so the freight ranking has no reason to look at it.
    /// A standing order gives it a demand of its own, and the ordinary ranking
    /// does the rest — lorries or trains bring the tonnage in, and whatever
    /// needs it nearby draws on the yard the way it draws on any other.
    ///
    /// Setting `tonnes` to zero cancels the order.
    SetStandingOrder {
        building: BuildingId,
        resource: Resource,
        tonnes: crate::units::Tonnes,
    },

    /// Hire builders from a bloc, for one Construction Office.
    ///
    /// They cost a placement fee now and a wage every day thereafter, both in
    /// that bloc's own currency — which is the whole trade against training
    /// your own, who cost the republic no money at all.
    ///
    /// **They arrive at a frontier post, not in the yard.** A bus has to go and
    /// fetch them, over the roads the republic has built, exactly as a crew
    /// coming off a finished site does.
    HireForeign {
        market: Market,
        office: BuildingId,
        heads: u32,
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

    /// Set the national working day, in hours per shift.
    ///
    /// Its own command rather than a scope on [`Command::SetShiftHours`],
    /// because the national standard is the thing everything else falls back to
    /// and therefore is the one rule that cannot be cleared. Folding it in would
    /// have made "clear the national standard" a state the type allowed and the
    /// handler had to refuse.
    SetNationalShiftHours { hours: f64 },

    /// Set — or with `None` clear — an exception to the national working day.
    ///
    /// *"Doctors work twelve, but at this hospital fourteen."* A rule about a
    /// kind covers every such building including ones not yet built; a rule
    /// about one building beats it. Clearing falls back to the rule above
    /// rather than freezing whatever number was in force.
    SetShiftHours {
        scope: ShiftScope,
        hours: Option<f64>,
    },

    /// How many crews a workplace runs, `0..=`[`crate::shifts::MAX_SHIFTS`].
    ///
    /// **The decision night is made of.** One crew is a day shift and is what
    /// every authored output rate describes; three is round the clock, three
    /// times the goods and three times the people — who have to be housed, fed
    /// and carried to work in the dark. Zero mothballs the place.
    SetShifts { building: BuildingId, shifts: u8 },

    /// Name the republic. The second beat of founding.
    ///
    /// A command rather than a field on [`crate::world::WorldSpec`] for two
    /// reasons, and the second is the load-bearing one. The shelf generates six
    /// candidate worlds before the player has named anything, so a spec with a
    /// required name would have made it invent five names nobody chose. And a
    /// name is an **input**: it is something a person decided, so it belongs in
    /// the journal beside every other decision, which is what makes a replayed
    /// republic come back called what the player called it rather than nothing.
    ///
    /// It changes nothing the simulation computes, and it is still not the
    /// shell's to hold. A name kept beside the save rather than inside it is a
    /// second file that can be lost, renamed or copied apart from the republic
    /// it belongs to.
    NameRepublic { name: String },
}

/// Which buildings a working-hours exception covers.
///
/// The national standard is deliberately absent: it is what these fall back to,
/// so it is the one rule with nothing above it and the one that cannot be
/// cleared. It has its own command instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftScope {
    /// Every building of a kind, including ones not yet built. A rule, rather
    /// than a batch edit over what happens to stand today.
    Kind(BuildingKind),
    /// One building, beating any rule about its kind.
    Building(BuildingId),
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
    /// A line was ordered.
    Strung(crate::utility::LineSiteId),
}

/// Why a command was refused, in words a panel can print.
///
/// `PartialEq` but not `Eq`, because a refusal carries the numbers that make it
/// actionable — what a thing costs against what the treasury holds — and money
/// is a float. A refusal a player cannot act on is the failure this whole type
/// exists to avoid, so the numbers win.
#[derive(Debug, Clone, PartialEq)]
pub enum Refused {
    /// The ground, the geology or the border said no.
    Placement(PlacementError),
    /// The road could not be ordered.
    Road(RoadError),
    /// The line could not be ordered.
    Line(crate::utility::LineError),
    /// There is no building with that id — it may have been demolished.
    NoSuchBuilding(BuildingId),
    /// There is no offer with that id. Includes one already accepted, so
    /// accepting twice is not a second success.
    NoSuchOffer(ContractId),
    /// There is no trade rule at that position.
    NoSuchRule { index: u32, rules: u32 },
    /// The advance could not be taken or repaid.
    Loan(crate::loan::LoanError),
    /// A site with builders standing on it. Recall them first.
    CrewOnSite,
    /// A Construction Office with crews still out at sites.
    CrewsOut,
    /// Nobody is working that site, so there is nobody to call off it.
    NoCrewThere,
    /// There is no frontier post with that number.
    NoSuchCrossing(crate::trade::CrossingId),
    /// That site had no instruction of its own to clear.
    NoSuchPolicy,
    /// Builders are hired to a Construction Office, and that is not one.
    NotAConstructionOffice,
    /// The treasury cannot cover the placement fee in that bloc's currency.
    CannotAffordHiring { needed: f64, held: f64 },
    /// That bloc holds no frontier post, so its workers have nowhere to arrive.
    NoPostOfThatBloc(Market),
    /// Hiring nobody.
    NobodyToHire,
    /// A standing order somewhere that does not hold goods to order.
    ///
    /// A coal mine told to keep fifty tonnes of coal on hand would be an
    /// instruction to fetch coal to the coal, and a house told to keep steel
    /// would send lorries to a doorstep for ever.
    NotAStore,
    /// A standing order for goods of a shape the place will not take.
    ///
    /// A storage tank told to keep coal, a grain silo told to keep steel. The
    /// order would stand for ever and never fill a tonne, because
    /// `Building::intake_capacity` returns zero for a form the building does not
    /// admit — so this is refused at the door rather than left to be a mystery.
    WillNotHold {
        place: &'static str,
        goods: &'static str,
    },
    /// A standing order larger than the place could hold.
    OverCapacity { asked: f64, holds: f64 },
    /// A republic named nothing at all.
    ///
    /// Refused rather than defaulted. A build held to a release standard has no
    /// placeholder strings in it, and "Republic 1" is exactly one — it would
    /// reach the title bar, the save list and the pause menu before anybody
    /// noticed the player had never been made to answer.
    Nameless,
    /// A name longer than [`crate::world::NAME_LIMIT`] characters.
    ///
    /// Refused rather than truncated, because a name the player typed and the
    /// game silently shortened is not the name they chose.
    NameTooLong { asked: usize, limit: usize },
    /// A working day outside what the republic will roster.
    ///
    /// Refused rather than clamped for the reason a truncated name is: a player
    /// who asked for a twenty-hour shift and got sixteen has been silently
    /// overruled, and the panel would show a number they did not choose.
    ShiftOutOfRange { asked: f64, min: f64, max: f64 },
    /// More crews than a building may run.
    TooManyShifts { asked: u8, limit: u8 },
    /// A roster set on something with no jobs to roster — a house, a store, a
    /// pylon. Refused rather than ignored, because a shift count sitting on a
    /// building nothing will ever read is a control that lies.
    NotAWorkplace,
    /// That building had no working-hours exception of its own to clear.
    NoSuchShiftRule,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::Placement(why) => write!(f, "{why}"),
            Refused::Road(why) => write!(f, "{why}"),
            Refused::Line(why) => write!(f, "{why}"),
            Refused::NoSuchBuilding(id) => write!(f, "there is no building {}", id.0),
            // The id is carried for the caller, never shown: a contract number
            // is not something a player has ever seen or could act on.
            Refused::NoSuchOffer(_) => write!(f, "that tender is no longer on the table"),
            Refused::Loan(why) => write!(f, "{why}"),
            Refused::CrewOnSite => write!(f, "a crew is still working this site"),
            Refused::CrewsOut => write!(
                f,
                "this office still has crews out at sites; bring them in first"
            ),
            Refused::NoCrewThere => write!(f, "nobody is working that site"),
            Refused::NoSuchCrossing(id) => {
                write!(f, "there is no frontier post {}", id.0)
            }
            Refused::NoSuchPolicy => {
                write!(f, "this site already follows the republic's import policy")
            }
            Refused::NotAConstructionOffice => {
                write!(f, "builders are hired to a Construction Office")
            }
            Refused::CannotAffordHiring { needed, held } => write!(
                f,
                "placing them costs {needed:.0} and the treasury holds {held:.0}"
            ),
            Refused::NoPostOfThatBloc(market) => write!(
                f,
                "the {} holds no frontier post here, so its workers have nowhere to arrive",
                market.name()
            ),
            Refused::NotAStore => write!(
                f,
                "only a terminal, a store or a distribution office keeps goods to order"
            ),
            Refused::WillNotHold { place, goods } => {
                write!(f, "{goods} will not go in a {place}")
            }
            Refused::OverCapacity { asked, holds } => {
                write!(f, "asked for {asked:.0} t where only {holds:.0} t will fit")
            }
            Refused::NobodyToHire => write!(f, "no workers were asked for"),
            Refused::Nameless => write!(f, "a republic needs a name"),
            Refused::NameTooLong { asked, limit } => write!(
                f,
                "that name is {asked} characters and {limit} is the most that will fit"
            ),
            Refused::NoSuchRule { index, rules } => match rules {
                0 => write!(f, "there are no trade rules to change"),
                1 => write!(f, "there is only one trade rule, and it is not {index}"),
                n => write!(f, "there is no trade rule {index}; there are {n}"),
            },
            Refused::ShiftOutOfRange { asked, min, max } => write!(
                f,
                "a shift of {asked:.0} hours is not one the republic will roster; \
                 between {min:.0} and {max:.0}"
            ),
            Refused::TooManyShifts { asked, limit } => write!(
                f,
                "{asked} shifts is more than the {limit} that fit in a day"
            ),
            Refused::NotAWorkplace => write!(f, "nobody works there, so there is no shift to set"),
            Refused::NoSuchShiftRule => {
                write!(f, "that building keeps the standard working day already")
            }
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

impl From<crate::utility::LineError> for Refused {
    fn from(why: crate::utility::LineError) -> Self {
        Refused::Line(why)
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
