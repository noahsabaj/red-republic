namespace RedRepublic.Sim;

/// <summary>One rung of a bloc's lending ladder.</summary>
/// <param name="Principal">What is advanced.</param>
/// <param name="Interest">Simple interest over the whole term, as a fraction of the principal.</param>
/// <param name="TermDays">Days from taking it to when the whole sum is due.</param>
public readonly record struct Tier(double Principal, double Interest, long TermDays);

/// <summary>An advance the republic is carrying.</summary>
public sealed class Loan(Market market, double principal, double owed, long takenDay, long dueDay)
{
    public Market Market { get; } = market;

    public double Principal { get; } = principal;

    /// <summary>Principal plus interest — the whole sum owed, fixed at taking.</summary>
    public double Owed { get; } = owed;

    public double Repaid { get; internal set; }

    public long TakenDay { get; } = takenDay;

    public long DueDay { get; } = dueDay;

    public double Outstanding => Math.Max(0.0, Owed - Repaid);

    public bool IsCleared => Outstanding <= double.Epsilon;

    /// <summary>Days left to pay, or zero once the day has come.</summary>
    public long DaysLeft(long today) => Math.Max(0, DueDay - today);
}

/// <summary>Why an advance could not be taken or repaid.</summary>
public enum LoanError
{
    None,

    /// <summary>There is no such rung on the ladder.</summary>
    NoSuchTier,

    /// <summary>This bloc has already advanced money that has not been paid back.</summary>
    AlreadyOwing,

    /// <summary>This bloc has been defaulted on and will not advance again.</summary>
    Defaulted,

    /// <summary>There is nothing outstanding to this bloc.</summary>
    NothingOwed,

    /// <summary>The treasury does not hold that much of this bloc's currency.</summary>
    CannotAfford,
}

/// <summary>
/// Every advance the republic is carrying — at most one per bloc.
/// </summary>
/// <remarks>
/// <para>
/// <b>The two ladders are different instruments, not one converted.</b> The east
/// lends roubles by the hundred thousand over years; the west lends dollars by
/// the thousand over months, dearer. Which bloc you ask is the decision, and the
/// rung is only how far you are willing to go with it.
/// </para>
/// <para>
/// <b>Losing a creditor is what makes a default cost anything.</b> The treasury
/// deliberately refuses to go negative — a republic does not run an overdraft it
/// never agreed to — so a fine levied on an empty purse takes nothing, and
/// without a burnt creditor a default is free: borrow, spend it, default, borrow
/// again. A consequence that does not need money to bite is the right shape for
/// a penalty aimed at a republic that has none.
/// </para>
/// </remarks>
public sealed class Loans(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<Loan> _live = [];
    private readonly List<Market> _burnt = [];

    public IReadOnlyList<Loan> All => _live;

    /// <summary>Advances that were repaid, and advances that were not.</summary>
    /// <remarks>
    /// Kept because "this republic has defaulted before" is a thing a player
    /// should be able to see about themselves.
    /// </remarks>
    public int Defaulted { get; private set; }

    public int Cleared { get; private set; }

    public Loan? Of(Market market)
    {
        foreach (var l in _live)
        {
            if (l.Market == market)
            {
                return l;
            }
        }

        return null;
    }

    /// <summary>Put an advance back exactly as it was — what a load does.</summary>
    public void Restore(
        Market market, double principal, double owed, double repaid, long taken, long due)
    {
        _live.Add(new Loan(market, principal, owed, taken, due) { Repaid = repaid });
    }

    /// <summary>Restore the record of who has been defaulted on and how often.</summary>
    public void RestoreHistory(IReadOnlyList<Market> burnt, int defaulted, int cleared)
    {
        ArgumentNullException.ThrowIfNull(burnt);
        _burnt.Clear();
        _burnt.AddRange(burnt);
        Defaulted = defaulted;
        Cleared = cleared;
    }

    /// <summary>Blocs that will not lend again.</summary>
    public IReadOnlyList<Market> Burnt => _burnt;

    public double Outstanding(Market market) => Of(market)?.Outstanding ?? 0.0;

    public bool WillLend(Market market) => !_burnt.Contains(market);

    public IReadOnlyList<Tier> Ladder(Market market) => _t.Ladder(market);

    /// <summary>Whether an advance can be taken, and on what terms.</summary>
    public LoanError CanTake(Market market, int tier, out Tier terms)
    {
        terms = default;
        var ladder = _t.Ladder(market);
        if (tier < 0 || tier >= ladder.Count)
        {
            return LoanError.NoSuchTier;
        }

        terms = ladder[tier];
        if (_burnt.Contains(market))
        {
            return LoanError.Defaulted;
        }

        return Of(market) is not null ? LoanError.AlreadyOwing : LoanError.None;
    }

    /// <summary>Take an advance. The whole sum owed is fixed at taking.</summary>
    public LoanError Take(Market market, int tier, long today, out Loan? loan)
    {
        loan = null;
        var refusal = CanTake(market, tier, out var terms);
        if (refusal != LoanError.None)
        {
            return refusal;
        }

        loan = new Loan(
            market,
            terms.Principal,
            terms.Principal * (1.0 + terms.Interest),
            today,
            today + terms.TermDays);
        _live.Add(loan);
        return LoanError.None;
    }

    /// <summary>
    /// Pay some of an advance back. More than is owed pays off what is owed —
    /// a republic cannot overpay itself into credit.
    /// </summary>
    public LoanError Repay(Market market, double amount, Treasury treasury, out double paid)
    {
        ArgumentNullException.ThrowIfNull(treasury);
        paid = 0.0;

        var loan = Of(market);
        if (loan is null)
        {
            return LoanError.NothingOwed;
        }

        var wanted = Math.Min(Math.Max(0.0, amount), loan.Outstanding);
        if (!treasury.CanAfford(market, wanted))
        {
            return LoanError.CannotAfford;
        }

        paid = treasury.Take(market, wanted);
        loan.Repaid += paid;

        if (loan.IsCleared)
        {
            _live.Remove(loan);
            Cleared++;
        }

        return LoanError.None;
    }

    /// <summary>Advances whose day has come and which are still outstanding.</summary>
    public List<Loan> Overdue(long today)
    {
        var late = new List<Loan>();
        foreach (var l in _live)
        {
            if (today >= l.DueDay && !l.IsCleared)
            {
                late.Add(l);
            }
        }

        return late;
    }

    /// <summary>
    /// The bloc calls it in and is not asked again. Returns the fine, which the
    /// caller levies on a purse that may well be empty — see the class note on
    /// why that is not the punishment.
    /// </summary>
    public double Default(Loan loan)
    {
        ArgumentNullException.ThrowIfNull(loan);
        _live.Remove(loan);
        if (!_burnt.Contains(loan.Market))
        {
            _burnt.Add(loan.Market);
        }

        Defaulted++;
        return loan.Outstanding * _t.DefaultFine;
    }
}
