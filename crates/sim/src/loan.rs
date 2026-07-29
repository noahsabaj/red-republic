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

/// Three rungs. The smallest is a bridge, the largest is a bet.
pub const TIERS: [Tier; 3] = [
    Tier {
        principal: 2_000.0,
        interest: 0.08,
        term_days: 360,
    },
    Tier {
        principal: 6_000.0,
        interest: 0.14,
        term_days: 540,
    },
    Tier {
        principal: 15_000.0,
        interest: 0.22,
        term_days: 720,
    },
];

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
        let terms = *TIERS.get(tier).ok_or(LoanError::NoSuchTier)?;
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

    #[test]
    fn the_terms_are_fixed_when_the_money_is_taken() {
        let mut loans = Loans::new();
        let got = loans.take(Market::East, 0, 100).expect("first advance");
        assert_eq!(got, TIERS[0].principal);
        let loan = loans.of(Market::East).expect("carrying it");
        assert_eq!(loan.owed, TIERS[0].principal * (1.0 + TIERS[0].interest));
        assert_eq!(loan.due_day, 100 + TIERS[0].term_days);
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
        let due = 10 + TIERS[0].term_days;
        assert_eq!(loans.overdue(due - 1).count(), 0, "not yet");
        assert_eq!(loans.overdue(due).count(), 1, "the day has come");

        let lost = loans.default_on(Market::East).expect("there was one");
        assert!(lost > TIERS[0].principal, "the interest was owed too");
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
