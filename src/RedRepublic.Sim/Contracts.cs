namespace RedRepublic.Sim;

/// <summary>Where a tender stands.</summary>
public enum ContractState
{
    /// <summary>On the table, waiting to be taken or turned down.</summary>
    Offered,

    /// <summary>Taken, and being delivered against.</summary>
    Accepted,

    Delivered,

    /// <summary>The deadline passed with tonnage outstanding.</summary>
    Failed,

    /// <summary>Turned down, or left on the table until it lapsed.</summary>
    Declined,
}

/// <summary>
/// A standing order from a foreign trade directorate: this much of this, by
/// then, at a premium.
/// </summary>
/// <remarks>
/// The premium is what makes a tender worth taking over selling at the counter,
/// and the deadline is what makes it a decision rather than free money.
/// </remarks>
public sealed class Contract(
    int id, Market market, int resource, double tonnes, double pricePerTonne,
    long offeredDay, long expiresDay, long deadlineDay)
{
    public int Id { get; } = id;

    public Market Market { get; } = market;

    public int Resource { get; } = resource;

    public double Tonnes { get; } = tonnes;

    /// <summary>What the bloc pays a tonne, already including the premium.</summary>
    public double PricePerTonne { get; } = pricePerTonne;

    public double Delivered { get; private set; }

    public long OfferedDay { get; } = offeredDay;

    /// <summary>The day the offer leaves the table if it has not been taken.</summary>
    public long ExpiresDay { get; } = expiresDay;

    /// <summary>The day the goods are due, once it has been taken.</summary>
    public long DeadlineDay { get; } = deadlineDay;

    public ContractState State { get; internal set; } = ContractState.Offered;

    public double Outstanding => Math.Max(0.0, Tonnes - Delivered);

    public bool IsLive => State == ContractState.Accepted;

    public double Value => Tonnes * PricePerTonne;

    /// <summary>
    /// Deliver against this tender. Returns what the bloc paid for the
    /// consignment — nothing beyond the tonnage ordered, because a tender is an
    /// order and not a standing buyer.
    /// </summary>
    public double Deliver(double tonnes)
    {
        if (!IsLive)
        {
            return 0.0;
        }

        var taken = Math.Min(tonnes, Outstanding);
        Delivered += taken;
        if (Outstanding <= 0.0)
        {
            State = ContractState.Delivered;
        }

        return taken * PricePerTonne;
    }
}

/// <summary>
/// Every tender the Foreign Trade Directorate has put up, taken or settled.
/// </summary>
/// <remarks>
/// <para>
/// A list rather than a map keyed by id, because iteration order is part of the
/// simulation's definition wherever an accumulation follows it.
/// </para>
/// <para>
/// <b>Relations are per bloc and decay.</b> A missed delivery sours them and time
/// mends them, which is what makes a default a setback rather than a permanent
/// state — and what stops a republic that failed one tender in its first year
/// being locked out for ever.
/// </para>
/// </remarks>
public sealed class Contracts(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<Contract> _all = [];
    private readonly double[] _relations = new double[2];

    private int _nextId = 1;

    public IReadOnlyList<Contract> All => _all;

    /// <summary>
    /// Put a tender back exactly as it was, keeping its id — what a load does.
    /// </summary>
    /// <remarks>
    /// Ids come back as they were because a renumbered tender is one the journal
    /// no longer names.
    /// </remarks>
    public void Restore(
        int id, Market market, int resource, double tonnes, double pricePerTonne,
        long offered, long expires, long deadline, ContractState state, double delivered)
    {
        var c = new Contract(id, market, resource, tonnes, pricePerTonne, offered, expires, deadline)
        {
            State = state,
        };

        c.Deliver(delivered);
        c.State = state;
        _all.Add(c);
        _nextId = Math.Max(_nextId, id + 1);
    }

    public void RestoreRelations(Market market, double penalty) =>
        _relations[(int)market] = penalty;

    /// <summary>
    /// How badly a bloc is thinking of the republic, 0 fine to the cap.
    /// </summary>
    /// <remarks>
    /// Capped, because past a point more bad feeling changes nothing — the same
    /// reason pollution saturates. Without a cap a republic that failed ten
    /// tenders would be arbitrarily worse off than one that failed three, with
    /// no way back from either.
    /// </remarks>
    public double Relations(Market market) => _relations[(int)market];

    public Contract? Get(int id)
    {
        foreach (var c in _all)
        {
            if (c.Id == id)
            {
                return c;
            }
        }

        return null;
    }

    public List<Contract> Offers()
    {
        var open = new List<Contract>();
        foreach (var c in _all)
        {
            if (c.State == ContractState.Offered)
            {
                open.Add(c);
            }
        }

        return open;
    }

    public List<Contract> Active()
    {
        var live = new List<Contract>();
        foreach (var c in _all)
        {
            if (c.IsLive)
            {
                live.Add(c);
            }
        }

        return live;
    }

    /// <summary>
    /// Put a tender on the table, drawn against the bloc's value band.
    /// </summary>
    /// <remarks>
    /// The value band rather than a tonnage band, because a tender for two
    /// hundred tonnes of coal and one for two tonnes of electronics should be
    /// comparably worth taking. Tonnage falls out of the value and the price,
    /// clamped so neither end is absurd.
    /// </remarks>
    public Contract Offer(Market market, int resource, long today, Rng rng)
    {
        ArgumentNullException.ThrowIfNull(rng);

        var (lo, hi) = market == Market.East
            ? (_t.ValueBandEastLo, _t.ValueBandEastHi)
            : (_t.ValueBandWestLo, _t.ValueBandWestHi);

        var value = rng.NextRange(lo, hi);
        var premium = 1.0 + rng.NextRange(_t.PremiumLo, _t.PremiumHi);
        var counter = market == Market.East
            ? _t.ResourcePriceEast[resource]
            : _t.ResourcePriceWest[resource];
        var price = counter * premium;
        var tonnes = Math.Clamp(value / price, _t.MinTonnes, _t.MaxTonnes);

        var deadline = today + (long)rng.NextRange(_t.DeadlineDaysLo, _t.DeadlineDaysHi);
        var contract = new Contract(
            _nextId++, market, resource, tonnes, price, today, today + _t.OfferDays, deadline);
        _all.Add(contract);
        return contract;
    }

    /// <summary>Take a tender. Returns false if it was not on the table.</summary>
    public bool Accept(int id)
    {
        var c = Get(id);
        if (c is null || c.State != ContractState.Offered)
        {
            return false;
        }

        c.State = ContractState.Accepted;
        return true;
    }

    /// <summary>Turn one down. It leaves the table immediately.</summary>
    public bool Decline(int id)
    {
        var c = Get(id);
        if (c is null || c.State != ContractState.Offered)
        {
            return false;
        }

        c.State = ContractState.Declined;
        return true;
    }

    /// <summary>
    /// A day's bookkeeping: offers lapse, deadlines fall due, and relations
    /// mend.
    /// </summary>
    /// <remarks>
    /// Returns the fines to levy. The caller settles them, because the treasury
    /// deliberately refuses to go negative and what happens to an unpayable fine
    /// is a decision for whoever holds the purse.
    /// </remarks>
    public List<(Market Market, double Fine)> Settle(long today)
    {
        var fines = new List<(Market, double)>();

        foreach (var c in _all)
        {
            if (c.State == ContractState.Offered && today >= c.ExpiresDay)
            {
                // Left on the table long enough is a refusal. It costs nothing:
                // not taking an offer is not a broken promise.
                c.State = ContractState.Declined;
                continue;
            }

            if (c.IsLive && today >= c.DeadlineDay)
            {
                c.State = ContractState.Failed;
                fines.Add((c.Market, c.Value * _t.FineShare));
                Sour(c.Market, _t.RelationsHit);
            }
        }

        // Time mends. Applied after the day's failures so a tender missed today
        // is not softened by the same day's decay.
        for (var m = 0; m < _relations.Length; m++)
        {
            _relations[m] = Math.Max(0.0, _relations[m] - _t.RelationsDecayPerDay);
        }

        return fines;
    }

    /// <summary>A default sours relations the same way a missed delivery does.</summary>
    public void Sour(Market market, double by) =>
        _relations[(int)market] = Math.Min(_t.RelationsCap, _relations[(int)market] + by);

    /// <summary>Whether the directorate is willing to put more on the table.</summary>
    public bool WillOffer(Market market) => Offers().Count < _t.MaxOpenOffers;
}
