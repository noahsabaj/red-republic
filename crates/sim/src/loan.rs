//! Bloc advances: borrowing hard currency, and what happens when you cannot
//! pay it back.
//!
//! # Why a republic borrows
//!
//! Currency only enters at the border, through exports. That makes the opening
//! of a republic a chicken-and-egg problem: the industry that earns dollars is
//! built from materials that have to be imported with dollars. A loan is the
//! way out, and the cost of taking it is that the deadline arrives whether or
//! not the industry did.
//!
//! # The terms are locked when the money is taken
//!
//! Simple interest, fixed at the moment of borrowing, exactly as a contract's
//! price is locked when the tender is accepted. The reason is the same: a
//! republic that took a bad deal should not be rescued by the rate moving, and
//! one that took a good deal should not be robbed of it.
//!
//! # One advance per bloc
//!
//! You may owe the East and the West at once, but not the East twice. Stacking
//! advances from one bloc turns the mechanic into a money printer with a
//! deadline nobody has to meet — the second loan pays the first.

use crate::trade::Market;
use serde::{Deserialize, Serialize};

/// What the blocs will advance, and on what terms.
///
/// Authored as a table rather than computed, so the middle rung being the
/// obvious one and the top rung being a gamble are decisions somebody made.
/// Interest rises with size because a bigger advance is a bigger bet on a
/// republic that has not yet proved it can export anything.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tier {
    pub principal: f64,
    /// Simple interest over the whole term, as a fraction of the principal.
    pub interest: f64,
    /// Days from taking it to when the whole sum is due.
    pub term_days: u64,
}

/// The Eastern Bloc's ladder, in roubles: large, long and cheap.
///
/// # Sized against what a rouble buys, which is buildings
///
/// A republic opens with [`crate::scenario::GRANT_ROUBLES`] and a contracted
/// building costs `labour * CONTRACTOR_RATE` — 17,000 for the cheapest on the
/// roster, 68,000 for the median, 210,800 for the dearest. So the rungs are a
/// bridge, a programme and a bet, measured in buildings:
///
/// | rung | roubles | buys | of the grant |
/// |---|---|---|---|
/// | bridge | 60,000 | about one median works | 2% |
/// | programme | 300,000 | a small chain of four or five | 12% |
/// | bet | 1,000,000 | fifteen, most of a second founding | 40% |
///
/// **One ladder used to serve both currencies and it was a dollar ladder.** Its
/// top rung was 15,000, which is less than the cheapest building on the roster —
/// so the largest advance the Eastern Bloc offered could not raise a Woodcutter
/// Post, against a grant that raises thirty-seven Coal Mines. Nobody had ever
/// had a reason to compare the two numbers.
///
/// # The terms are long because roubles are earned slowly
///
/// A customs house clears [`crate::trade::CUSTOMS_THROUGHPUT_PER_DAY`] tonnes a
/// day and coal sells east at 2 roubles the tonne, so a republic exporting flat
/// out through one post earns about 60 roubles a day until it is making
/// something worth more than coal. The bottom rung is repayable on exactly that
/// with room to spare — one post, at the ceiling, over five years — and each rung
/// above it needs a republic that has industrialised past coal. Five years on the
/// bridge and ten on the bet is what makes them different instruments rather than
/// the same bet at three sizes.
///
/// The bridge was three years first, which made it *exactly* break-even against
/// perfect coal export for the whole term — a republic that spent three years
/// achieving nothing but the loan. `the_bridge_is_repayable_on_coal_alone`
/// wants half again on top, and found it.
pub const TIERS_EAST: [Tier; 3] = [
    Tier {
        principal: 60_000.0,
        interest: 0.08,
        term_days: 1_800,
    },
    Tier {
        principal: 300_000.0,
        interest: 0.14,
        term_days: 2_700,
    },
    Tier {
        principal: 1_000_000.0,
        interest: 0.22,
        term_days: 3_600,
    },
];

/// The Western Alliance's ladder, in dollars: small, short and dear.
///
/// # A different instrument, not the same one converted
///
/// The two blocs are not two spellings of the same money. A republic **starts
/// with no dollars at all** and earns them only by hauling goods to a western
/// post, where prices are about half the eastern ones — so a western advance is
/// measured in what it imports rather than in what it builds. Twenty-five
/// thousand dollars is five hundred tonnes of machinery, which is the industry
/// that earns the dollars back, and that circle is the whole reason this
/// mechanic exists.
///
/// Dearer and shorter than the east's, deliberately. The fraternal creditor
/// lends patiently and the hard-currency one does not, and that asymmetry is the
/// same one [`crate::trade::BORDER_SPREAD`] draws: which bloc you deal with is a
/// decision, not a detail.
///
/// The top rung is 25,000 rather than the 15,000 it was, for one reason worth
/// stating: it is the least that can raise the cheapest building on the roster.
/// A ladder whose largest rung cannot buy the smallest thing in the game is a
/// ladder that only ever buys goods, and `the_top_rung_can_raise_a_building`
/// holds both ladders to it.
pub const TIERS_WEST: [Tier; 3] = [
    Tier {
        principal: 3_000.0,
        interest: 0.10,
        term_days: 360,
    },
    Tier {
        principal: 9_000.0,
        interest: 0.16,
        term_days: 540,
    },
    Tier {
        principal: 25_000.0,
        interest: 0.25,
        term_days: 720,
    },
];

/// What a bloc will advance.
///
/// A function rather than a field on [`Market`], because the ladder is loan data
/// and belongs beside the tables above — a market knows what it pays for a tonne
/// and has no business knowing what it lends.
pub fn ladder(market: Market) -> &'static [Tier] {
    match market {
        Market::East => &TIERS_EAST,
        Market::West => &TIERS_WEST,
    }
}

/// How many rungs every ladder has.
///
/// Both are the same length on purpose — bridge, programme, bet is the shape of
/// the decision, and a bloc offering four rungs where the other offers three
/// would be a difference that means nothing. `every_ladder_is_the_same_shape`
/// holds it.
pub const RUNGS: usize = 3;

/// What a default costs, as a fraction of what was still owed.
///
/// On top of losing the advance itself: the bloc writes the debt off and sours
/// on you, which raises every price it quotes thereafter through the same
/// relations penalty a missed tender uses.
pub const DEFAULT_FINE: f64 = 0.25;

/// How far relations sour on a default.
///
/// Deliberately worse than a missed tender. Failing to deliver goods is a bad
/// month; failing to repay an advance is a bad republic.
pub const DEFAULT_RELATIONS: f64 = 0.15;

/// An advance the republic is carrying.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Loan {
    pub market: Market,
    /// What was advanced.
    pub principal: f64,
    /// Principal plus interest — the whole sum owed, fixed at taking.
    pub owed: f64,
    /// How much of `owed` has been paid back.
    pub repaid: f64,
    pub taken_day: u64,
    pub due_day: u64,
}

impl Loan {
    pub fn outstanding(&self) -> f64 {
        (self.owed - self.repaid).max(0.0)
    }

    pub fn is_cleared(&self) -> bool {
        self.outstanding() <= f64::EPSILON
    }

    /// Days left to pay, or zero once the day has come.
    pub fn days_left(&self, today: u64) -> u64 {
        self.due_day.saturating_sub(today)
    }
}

/// Every advance the republic is carrying, at most one per bloc.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Loans {
    live: Vec<Loan>,
    /// Advances that were repaid, and advances that were not. Kept because
    /// "this republic has defaulted before" is a thing a player should be able
    /// to see about themselves.
    pub defaulted: u32,
    pub cleared: u32,
    /// Blocs that will not advance again.
    ///
    /// **This is what makes a default cost anything at all.** The treasury
    /// deliberately refuses to go negative — a republic does not run an
    /// overdraft it never agreed to — so a fine levied on an empty purse takes
    /// nothing, and without this a default is free: borrow, spend it, default,
    /// borrow again. A test found that within a minute of the mechanic
    /// existing.
    ///
    /// Losing a creditor for good is a consequence that does not need money to
    /// bite, which is the right shape for a penalty aimed at a republic that
    /// has none.
    burnt: Vec<Market>,
}

/// Why an advance could not be taken or repaid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoanError {
    /// There is no such rung on the ladder.
    NoSuchTier,
    /// This bloc has already advanced you money you have not paid back.
    AlreadyOwing,
    /// This bloc has been defaulted on and will not advance again.
    Defaulted,
    /// There is nothing outstanding to this bloc.
    NothingOwed,
    /// The treasury does not hold that much of this bloc's currency.
    CannotAfford,
}

impl std::fmt::Display for LoanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoanError::NoSuchTier => write!(f, "no advance of that size is offered"),
            LoanError::AlreadyOwing => {
                write!(f, "this bloc has already advanced what you have not repaid")
            }
            LoanError::Defaulted => write!(
                f,
                "this bloc has been defaulted on and will not advance again"
            ),
            LoanError::NothingOwed => write!(f, "nothing is owed to this bloc"),
            LoanError::CannotAfford => write!(f, "the treasury cannot cover that"),
        }
    }
}

impl std::error::Error for LoanError {}

impl Loans {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn all(&self) -> &[Loan] {
        &self.live
    }

    pub fn of(&self, market: Market) -> Option<&Loan> {
        self.live.iter().find(|l| l.market == market)
    }

    /// Everything still owed to a bloc.
    pub fn outstanding(&self, market: Market) -> f64 {
        self.of(market).map_or(0.0, Loan::outstanding)
    }

    /// Whether an advance could be taken, and why not if not.
    ///
    /// A pre-flight the UI reads to grey a button out *with its reason*, which
    /// is the whole argument for refusals carrying words.
    pub fn can_take(&self, market: Market, tier: usize) -> Result<Tier, LoanError> {
        let terms = *ladder(market).get(tier).ok_or(LoanError::NoSuchTier)?;
        if self.burnt.contains(&market) {
            return Err(LoanError::Defaulted);
        }
        if self.of(market).is_some() {
            return Err(LoanError::AlreadyOwing);
        }
        Ok(terms)
    }

    pub(crate) fn take(
        &mut self,
        market: Market,
        tier: usize,
        today: u64,
    ) -> Result<f64, LoanError> {
        let terms = self.can_take(market, tier)?;
        self.live.push(Loan {
            market,
            principal: terms.principal,
            owed: terms.principal * (1.0 + terms.interest),
            repaid: 0.0,
            taken_day: today,
            due_day: today + terms.term_days,
        });
        Ok(terms.principal)
    }

    /// Pay some of an advance back. Returns what was actually paid, which is
    /// never more than is owed — overpaying a loan is not a way to lose money.
    pub(crate) fn repay(&mut self, market: Market, amount: f64) -> Result<f64, LoanError> {
        let loan = self
            .live
            .iter_mut()
            .find(|l| l.market == market)
            .ok_or(LoanError::NothingOwed)?;
        let paid = amount.min(loan.outstanding()).max(0.0);
        loan.repaid += paid;
        if loan.is_cleared() {
            self.live.retain(|l| l.market != market);
            self.cleared += 1;
        }
        Ok(paid)
    }

    /// Write an advance off as unpaid.
    pub(crate) fn default_on(&mut self, market: Market) -> Option<f64> {
        let lost = self.of(market)?.outstanding();
        self.live.retain(|l| l.market != market);
        self.defaulted += 1;
        if !self.burnt.contains(&market) {
            self.burnt.push(market);
        }
        Some(lost)
    }

    /// Whether a bloc will lend at all.
    pub fn will_lend(&self, market: Market) -> bool {
        !self.burnt.contains(&market)
    }

    /// Advances whose day has come with money still owed.
    pub fn overdue(&self, today: u64) -> impl Iterator<Item = &Loan> {
        self.live.iter().filter(move |l| today >= l.due_day)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The largest advance a bloc offers must be able to raise the cheapest
    /// building on the roster.
    ///
    /// **This is the check that was missing, and the bug it would have caught
    /// shipped.** One ladder served both currencies, and its top rung was
    /// 15,000 against a cheapest building of 17,000 — so the largest advance
    /// the Eastern Bloc would make could not buy the smallest thing in the game,
    /// beside a founding grant that buys thirty-seven of the median one.
    ///
    /// Stated as a relationship rather than as a number, so it goes on holding
    /// when the roster grows a cheaper building or `CONTRACTOR_RATE` moves. What
    /// it asserts is the *claim this module opens with*: an advance is the way
    /// out of the circle where the industry that earns money is built with
    /// money. A ladder that cannot buy a building is not a way out of anything.
    #[test]
    fn the_top_rung_can_raise_a_building() {
        let cheapest = crate::building::BUILDINGS
            .iter()
            .map(|d| d.labour * crate::systems::CONTRACTOR_RATE)
            .fold(f64::INFINITY, f64::min);
        assert!(
            cheapest.is_finite() && cheapest > 0.0,
            "the roster priced nothing, so this proves nothing"
        );
        for market in Market::ALL {
            let top = ladder(market)
                .last()
                .unwrap_or_else(|| panic!("{market:?} offers no advances at all"));
            assert!(
                top.principal >= cheapest,
                "the largest advance {market:?} offers is {:.0} and the cheapest \
                 building on the roster costs {cheapest:.0} to contract. An \
                 advance is supposed to be the way out of the circle where the \
                 industry that earns money is built with money.",
                top.principal
            );
        }
    }

    /// The eastern ladder is sized against the grant a republic opens with.
    ///
    /// Money is not comparable between blocs — a dollar and a rouble buy
    /// different amounts of different things — but roubles and the grant are the
    /// same money, so this one comparison is meaningful and it is the one that
    /// was never made. A top rung worth a rounding error against the opening
    /// treasury is a rung nobody would ever press.
    ///
    /// The upper bound matters as much as the lower: an advance larger than the
    /// founding grant would make borrowing a better start than being posted,
    /// which is the shape this game refuses everywhere else.
    #[test]
    fn an_eastern_advance_is_worth_taking_and_is_not_a_second_founding() {
        let grant = crate::scenario::GRANT_ROUBLES;
        let top = TIERS_EAST.last().expect("the east lends").principal;
        let bottom = TIERS_EAST.first().expect("the east lends").principal;
        assert!(
            top >= grant * 0.2,
            "the largest rouble advance is {top:.0} against a grant of \
             {grant:.0} — {:.1}% of what a republic opens with, which is not a \
             decision anybody would weigh",
            100.0 * top / grant
        );
        assert!(
            top < grant,
            "an advance of {top:.0} is more than the {grant:.0} grant, so \
             borrowing would be a better start than being posted"
        );
        assert!(
            bottom < top / 4.0,
            "the rungs are {bottom:.0} and {top:.0}, close enough together that \
             there is nothing to choose between them"
        );
    }

    /// Both ladders are the same shape, and each climbs.
    ///
    /// Bridge, programme, bet is the decision; a bloc offering four rungs where
    /// the other offers three would be a difference that means nothing. And a
    /// rung that is larger *and* cheaper than the one below it would make every
    /// smaller rung dead — the interest is what prices the size.
    #[test]
    fn every_ladder_is_the_same_shape_and_climbs() {
        for market in Market::ALL {
            let rungs = ladder(market);
            assert_eq!(
                rungs.len(),
                RUNGS,
                "{market:?} offers {} rungs and RUNGS says {RUNGS}",
                rungs.len()
            );
            for pair in rungs.windows(2) {
                let (lower, higher) = (pair[0], pair[1]);
                assert!(
                    higher.principal > lower.principal,
                    "{market:?}: a rung of {:.0} sits above one of {:.0}",
                    higher.principal,
                    lower.principal
                );
                assert!(
                    higher.interest > lower.interest,
                    "{market:?}: {:.0} costs {:.0}% and {:.0} costs {:.0}%, so \
                     the smaller rung is dead",
                    higher.principal,
                    100.0 * higher.interest,
                    lower.principal,
                    100.0 * lower.interest
                );
                assert!(
                    higher.term_days >= lower.term_days,
                    "{market:?}: the larger advance is due sooner than the \
                     smaller one"
                );
            }
        }
    }

    /// The bottom rung of each ladder is repayable by a republic exporting
    /// through one customs house at its throughput ceiling.
    ///
    /// **The bridge has to be a bridge.** A smallest advance nobody can pay back
    /// is not a way to get started, it is a way to lose a creditor for good —
    /// and losing one is permanent, which is the whole thing that makes a
    /// default cost anything to a republic with an empty purse.
    ///
    /// Priced on coal, which is the cheapest thing a young republic exports and
    /// therefore the worst case. Anything it learns to make instead is worth
    /// more per tonne, so a ladder that passes on coal passes on everything.
    #[test]
    fn the_bridge_is_repayable_on_coal_alone() {
        for market in Market::ALL {
            let rung = ladder(market).first().expect("every bloc lends");
            let owed = rung.principal * (1.0 + rung.interest);
            let a_day = crate::trade::CUSTOMS_THROUGHPUT_PER_DAY
                * market.sell_price(crate::resource::Resource::Coal);
            let earned = a_day * rung.term_days as f64;
            // Half again on top of the debt, not merely enough. A bridge that is
            // exactly break-even against three years of flawless coal export is
            // a republic that spent three years standing still — which is what
            // the first draft of this table did, and what this margin caught.
            assert!(
                earned >= owed * 1.5,
                "{market:?}'s smallest advance owes {owed:.0} over {} days, and \
                 one customs house running flat out on coal earns {earned:.0} in \
                 that time. The bridge cannot be crossed.",
                rung.term_days
            );
        }
    }

    #[test]
    fn the_terms_are_fixed_when_the_money_is_taken() {
        let mut loans = Loans::new();
        let got = loans.take(Market::East, 0, 100).expect("first advance");
        assert_eq!(got, TIERS_EAST[0].principal);
        let loan = loans.of(Market::East).expect("carrying it");
        assert_eq!(
            loan.owed,
            TIERS_EAST[0].principal * (1.0 + TIERS_EAST[0].interest)
        );
        assert_eq!(loan.due_day, 100 + TIERS_EAST[0].term_days);
        assert!(loan.owed > loan.principal, "an advance costs something");
    }

    /// Stacking advances from one bloc turns the mechanic into a money printer
    /// with a deadline nobody has to meet: the second loan pays the first.
    #[test]
    fn one_bloc_will_not_advance_twice() {
        let mut loans = Loans::new();
        loans.take(Market::East, 1, 0).expect("first");
        assert_eq!(loans.take(Market::East, 1, 0), Err(LoanError::AlreadyOwing));
        // But the other bloc is a separate purse and a separate creditor.
        assert!(loans.take(Market::West, 1, 0).is_ok());
    }

    #[test]
    fn repaying_it_all_clears_it_and_overpaying_costs_nothing_extra() {
        let mut loans = Loans::new();
        loans.take(Market::West, 0, 0).expect("advance");
        let owed = loans.outstanding(Market::West);
        let paid = loans.repay(Market::West, owed * 2.0).expect("repayable");
        assert_eq!(paid, owed, "a repayment is capped at what is owed");
        assert!(loans.of(Market::West).is_none(), "it is cleared");
        assert_eq!(loans.cleared, 1);
        assert_eq!(loans.repay(Market::West, 1.0), Err(LoanError::NothingOwed));
    }

    #[test]
    fn an_advance_comes_due_and_can_be_defaulted_on() {
        let mut loans = Loans::new();
        loans.take(Market::East, 0, 10).expect("advance");
        let due = 10 + TIERS_EAST[0].term_days;
        assert_eq!(loans.overdue(due - 1).count(), 0, "not yet");
        assert_eq!(loans.overdue(due).count(), 1, "the day has come");

        let lost = loans.default_on(Market::East).expect("there was one");
        assert!(lost > TIERS_EAST[0].principal, "the interest was owed too");
        assert_eq!(loans.defaulted, 1);
        assert!(loans.of(Market::East).is_none());

        // And the bloc is done with you. Without this a default is FREE for a
        // republic with an empty purse -- the treasury refuses to go negative,
        // so the fine takes nothing -- and the mechanic becomes borrow, spend,
        // default, borrow again.
        assert!(!loans.will_lend(Market::East));
        assert_eq!(loans.take(Market::East, 0, 500), Err(LoanError::Defaulted));
        // The other bloc has no reason to care.
        assert!(loans.will_lend(Market::West));
        assert!(loans.take(Market::West, 0, 500).is_ok());
    }

    /// Part-paying moves the debt without clearing it, which is what makes a
    /// deadline something you can work toward rather than a cliff.
    #[test]
    fn part_payment_reduces_what_is_owed_without_clearing_it() {
        let mut loans = Loans::new();
        loans.take(Market::East, 2, 0).expect("advance");
        let before = loans.outstanding(Market::East);
        loans.repay(Market::East, before / 3.0).expect("part");
        let after = loans.outstanding(Market::East);
        assert!(after < before && after > 0.0);
        assert!(loans.of(Market::East).is_some(), "still carrying it");
    }
}
