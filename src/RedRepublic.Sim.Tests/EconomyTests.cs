namespace RedRepublic.Sim.Tests;

/// <summary>The frontier: who is on the other side, and where you can reach them.</summary>
public sealed class FrontierTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// A republic is <i>surrounded</i>. An earlier model named one edge as
    /// foreign and said nothing about the other three, so a republic had a
    /// single frontier and three impassable nothings.
    /// </summary>
    [Fact]
    public void The_whole_perimeter_is_somebody_elses()
    {
        var f = Frontier.Generate(6000.0, Rng.FromSeed(1961), T);

        // Every point on the loop belongs to a bloc, all the way round.
        var seen = new HashSet<Market>();
        for (var along = 0.0; along < Frontier.Turns; along += 0.05)
        {
            seen.Add(f.BlocAt(along));
        }

        Assert.Equal(2, seen.Count);

        // And it wraps: past the last turn is the first side again.
        Assert.Equal(f.BlocAt(0.25), f.BlocAt(Frontier.Turns + 0.25));
        Assert.Equal(f.BlocAt(0.25), f.BlocAt(-Frontier.Turns + 0.25));
    }

    /// <summary>The perimeter walks the four sides and comes home.</summary>
    [Fact]
    public void The_perimeter_walks_the_edges_of_the_map()
    {
        var f = Frontier.Generate(6000.0, Rng.FromSeed(7), T);

        Assert.Equal((0.0, 0.0), f.PointAt(0.0));
        Assert.Equal((6000.0, 0.0), f.PointAt(1.0));
        Assert.Equal((6000.0, 6000.0), f.PointAt(2.0));
        Assert.Equal((0.0, 6000.0), f.PointAt(3.0));

        // Four turns is home again.
        Assert.Equal(f.PointAt(0.0), f.PointAt(Frontier.Turns));

        // The middle of the map is as far from the frontier as you can get.
        Assert.Equal(3000.0, f.DistanceFrom(3000.0, 3000.0));
        Assert.Equal(0.0, f.DistanceFrom(0.0, 3000.0));
    }

    /// <summary>
    /// Posts are spaced evenly so one is always a haul away and "which one"
    /// stays a real choice — and inset onto workable ground, because a post on
    /// the map edge has nowhere to put a road.
    /// </summary>
    [Fact]
    public void Posts_are_spread_around_and_stand_on_workable_ground()
    {
        var f = Frontier.Generate(6000.0, Rng.FromSeed(1961), T);

        Assert.Equal(T.Crossings, f.Crossings.Count);

        foreach (var c in f.Crossings)
        {
            Assert.InRange(c.X, 0.0, 6000.0);
            Assert.InRange(c.Y, 0.0, 6000.0);

            // Inset from the edge it sits on, so there is ground to build from.
            Assert.True(
                f.DistanceFrom(c.X, c.Y) >= T.CrossingInset - 1e-9,
                $"post {c.Id} stands {f.DistanceFrom(c.X, c.Y)} m in, expected {T.CrossingInset}");

            Assert.Equal(c.Bloc, f.BlocAt(c.Along));
        }

        // Evenly spaced around the loop rather than clustered.
        var alongs = f.Crossings.Select(c => c.Along).Order().ToList();
        for (var i = 1; i < alongs.Count; i++)
        {
            Assert.Equal(Frontier.Turns / T.Crossings, alongs[i] - alongs[i - 1], 9);
        }
    }

    [Fact]
    public void The_same_seed_lays_the_same_frontier()
    {
        var a = Frontier.Generate(6000.0, Rng.FromSeed(1961), T);
        var b = Frontier.Generate(6000.0, Rng.FromSeed(1961), T);
        var c = Frontier.Generate(6000.0, Rng.FromSeed(1962), T);

        Assert.Equal(a.Arcs, b.Arcs);
        Assert.Equal(a.Crossings, b.Crossings);
        Assert.NotEqual(a.Crossings[0].Along, c.Crossings[0].Along);
    }

    /// <summary>
    /// Trading west means hauling west. The nearest post of a bloc is what an
    /// import policy resolves to when the player has not named one.
    /// </summary>
    [Fact]
    public void The_nearest_post_of_a_bloc_is_findable()
    {
        var f = Frontier.Generate(6000.0, Rng.FromSeed(1961), T);

        var any = f.NearestCrossing(3000.0, 3000.0, null);
        Assert.NotNull(any);

        foreach (var bloc in new[] { Market.East, Market.West })
        {
            var post = f.NearestCrossing(3000.0, 3000.0, bloc);
            if (post is not null)
            {
                Assert.Equal(bloc, post.Value.Bloc);
            }
        }

        Assert.NotNull(f.Get(any.Value.Id));
        Assert.Null(f.Get(999));
    }
}

/// <summary>Two purses, and the credit that fills them.</summary>
public sealed class LoansTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// <b>Two currencies, not one converted.</b> A republic can be rich in
    /// roubles and unable to buy a single western machine.
    /// </summary>
    [Fact]
    public void The_two_purses_are_separate()
    {
        var t = new Treasury();
        t.Add(Market.East, 100_000.0);

        Assert.Equal(100_000.0, t.Of(Market.East));
        Assert.Equal(0.0, t.Of(Market.West));
        Assert.False(t.CanAfford(Market.West, 1.0));

        // A shortfall is a smaller payment, never a debt: the treasury refuses
        // to go negative, which is why a fine on an empty purse takes nothing.
        Assert.Equal(0.0, t.Take(Market.West, 500.0));
        Assert.Equal(0.0, t.Of(Market.West));
    }

    /// <summary>
    /// The east lends by the hundred thousand over years; the west by the
    /// thousand over months, dearer. They are different instruments, and the
    /// sizes are what say so.
    /// </summary>
    [Fact]
    public void The_two_ladders_are_different_instruments()
    {
        var east = T.Ladder(Market.East);
        var west = T.Ladder(Market.West);

        Assert.Equal(3, east.Count);
        Assert.Equal(3, west.Count);

        // Large, long and cheap against small, short and dear. Rung for rung:
        // the east's smallest advance is twenty times the west's, and its
        // shortest term is longer than the west's longest.
        for (var i = 0; i < east.Count; i++)
        {
            Assert.True(
                east[i].Principal > west[i].Principal * 10.0,
                $"rung {i}: east {east[i].Principal} against west {west[i].Principal}");
            Assert.True(east[i].Interest < west[i].Interest);
        }

        Assert.True(east[0].TermDays > west[^1].TermDays);

        // Each ladder climbs.
        for (var i = 1; i < east.Count; i++)
        {
            Assert.True(east[i].Principal > east[i - 1].Principal);
            Assert.True(east[i].Interest > east[i - 1].Interest);
        }
    }

    [Fact]
    public void An_advance_is_owed_in_full_from_the_day_it_is_taken()
    {
        var loans = new Loans(T);
        var terms = T.Ladder(Market.East)[0];

        Assert.Equal(LoanError.None, loans.Take(Market.East, 0, 100, out var loan));
        Assert.NotNull(loan);

        Assert.Equal(terms.Principal, loan.Principal);
        Assert.Equal(terms.Principal * (1.0 + terms.Interest), loan.Owed);
        Assert.Equal(100 + terms.TermDays, loan.DueDay);
        Assert.Equal(terms.TermDays, loan.DaysLeft(100));
        Assert.Equal(0, loan.DaysLeft(loan.DueDay + 50));

        // One advance per bloc, and no such rung is refused rather than clamped.
        Assert.Equal(LoanError.AlreadyOwing, loans.Take(Market.East, 1, 100, out _));
        Assert.Equal(LoanError.NoSuchTier, loans.Take(Market.West, 9, 100, out _));
        Assert.Equal(LoanError.None, loans.Take(Market.West, 0, 100, out _));
    }

    [Fact]
    public void Repaying_more_than_is_owed_pays_off_what_is_owed()
    {
        var loans = new Loans(T);
        var treasury = new Treasury();
        loans.Take(Market.East, 0, 0, out var loan);
        Assert.NotNull(loan);
        treasury.Add(Market.East, loan.Owed * 2.0);

        Assert.Equal(LoanError.None, loans.Repay(Market.East, 1000.0, treasury, out var first));
        Assert.Equal(1000.0, first);
        Assert.Equal(loan.Owed - 1000.0, loans.Outstanding(Market.East));

        Assert.Equal(LoanError.None, loans.Repay(Market.East, 1e9, treasury, out var rest));
        Assert.Equal(loan.Owed - 1000.0, rest);
        Assert.Equal(0.0, loans.Outstanding(Market.East));
        Assert.Equal(1, loans.Cleared);

        // Cleared, the bloc will lend again.
        Assert.Equal(LoanError.NothingOwed, loans.Repay(Market.East, 10.0, treasury, out _));
        Assert.Equal(LoanError.None, loans.CanTake(Market.East, 0, out _));
    }

    [Fact]
    public void A_repayment_the_purse_cannot_cover_is_refused_rather_than_part_paid()
    {
        var loans = new Loans(T);
        var treasury = new Treasury();
        loans.Take(Market.East, 0, 0, out _);
        treasury.Add(Market.East, 100.0);

        Assert.Equal(LoanError.CannotAfford, loans.Repay(Market.East, 500.0, treasury, out var paid));
        Assert.Equal(0.0, paid);
        Assert.Equal(100.0, treasury.Of(Market.East));
    }

    /// <summary>
    /// <b>Losing a creditor is what makes a default cost anything.</b> The
    /// treasury refuses to go negative, so a fine on an empty purse takes
    /// nothing — and without a burnt creditor a default is free: borrow, spend
    /// it, default, borrow again.
    /// </summary>
    [Fact]
    public void A_default_costs_the_creditor_even_when_the_purse_is_empty()
    {
        var loans = new Loans(T);
        var treasury = new Treasury();
        loans.Take(Market.East, 0, 0, out var loan);
        Assert.NotNull(loan);

        Assert.Empty(loans.Overdue(loan.DueDay - 1));
        var late = loans.Overdue(loan.DueDay);
        Assert.Single(late);

        var fine = loans.Default(late[0]);
        Assert.True(fine > 0.0);

        // The purse is empty, so the fine takes nothing at all...
        Assert.Equal(0.0, treasury.Take(Market.East, fine));

        // ...and the creditor is gone, which is the punishment that bites.
        Assert.Equal(1, loans.Defaulted);
        Assert.False(loans.WillLend(Market.East));
        Assert.Equal(LoanError.Defaulted, loans.CanTake(Market.East, 0, out _));

        // The other bloc is unaffected — they are separate relationships.
        Assert.True(loans.WillLend(Market.West));
        Assert.Equal(LoanError.None, loans.CanTake(Market.West, 0, out _));
    }
}

/// <summary>Tenders from the foreign trade directorates.</summary>
public sealed class ContractsTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// A tender is drawn against a <i>value</i> band rather than a tonnage band,
    /// so an order for coal and one for electronics are comparably worth taking.
    /// Tonnage falls out of the value and the price.
    /// </summary>
    [Fact]
    public void A_tender_is_worth_taking_whatever_it_is_for()
    {
        var c = new Contracts(T);
        var rng = Rng.FromSeed(1961);
        var coal = T.ResourceIndex("Coal");
        var electronics = T.ResourceIndex("Electronics");

        var bulk = c.Offer(Market.East, coal, 0, rng);
        var fine = c.Offer(Market.East, electronics, 0, rng);

        // Both within the band, and the cheap one is for far more tonnes.
        foreach (var offer in new[] { bulk, fine })
        {
            Assert.InRange(offer.Tonnes, T.MinTonnes, T.MaxTonnes);
            Assert.Equal(ContractState.Offered, offer.State);
            Assert.True(offer.Value > 0.0);
        }

        Assert.True(bulk.Tonnes > fine.Tonnes);

        // The premium is what makes it worth taking over the counter price.
        Assert.True(fine.PricePerTonne > T.ResourcePriceEast[electronics]);
    }

    [Fact]
    public void A_tender_is_taken_or_turned_down_and_then_delivered_against()
    {
        var c = new Contracts(T);
        var offer = c.Offer(Market.East, T.ResourceIndex("Coal"), 0, Rng.FromSeed(7));

        Assert.False(c.Accept(999));
        Assert.True(c.Accept(offer.Id));
        Assert.True(offer.IsLive);

        // It cannot be taken twice, nor turned down once taken.
        Assert.False(c.Accept(offer.Id));
        Assert.False(c.Decline(offer.Id));

        var half = offer.Deliver(offer.Tonnes / 2.0);
        Assert.Equal(offer.Tonnes / 2.0 * offer.PricePerTonne, half, 9);
        Assert.True(offer.IsLive);

        // Over-delivering pays for the tonnage ordered and no more: a tender is
        // an order, not a standing buyer.
        var rest = offer.Deliver(offer.Tonnes * 10.0);
        Assert.Equal(offer.Tonnes / 2.0 * offer.PricePerTonne, rest, 9);
        Assert.Equal(ContractState.Delivered, offer.State);
        Assert.Equal(0.0, offer.Outstanding);
        Assert.Equal(0.0, offer.Deliver(100.0));
    }

    /// <summary>
    /// Not taking an offer is not a broken promise: a lapsed tender costs
    /// nothing. Missing a deadline you agreed to does.
    /// </summary>
    [Fact]
    public void Lapsing_is_free_and_failing_is_not()
    {
        var c = new Contracts(T);
        var rng = Rng.FromSeed(1961);

        var ignored = c.Offer(Market.East, T.ResourceIndex("Coal"), 0, rng);
        var fines = c.Settle(ignored.ExpiresDay);

        Assert.Empty(fines);
        Assert.Equal(ContractState.Declined, ignored.State);
        Assert.Equal(0.0, c.Relations(Market.East));

        var taken = c.Offer(Market.East, T.ResourceIndex("Coal"), 0, rng);
        c.Accept(taken.Id);
        fines = c.Settle(taken.DeadlineDay);

        Assert.Single(fines);
        Assert.Equal(Market.East, fines[0].Market);
        Assert.Equal(taken.Value * T.FineShare, fines[0].Fine, 9);
        Assert.Equal(ContractState.Failed, taken.State);
        Assert.True(c.Relations(Market.East) > 0.0);
        Assert.Equal(0.0, c.Relations(Market.West));
    }

    /// <summary>
    /// Bad feeling is capped and mends with time — so a republic that failed one
    /// tender in its first year is not locked out for ever, and one that failed
    /// ten is not arbitrarily worse off than one that failed three.
    /// </summary>
    [Fact]
    public void Relations_are_capped_and_mend()
    {
        var c = new Contracts(T);

        for (var i = 0; i < 50; i++)
        {
            c.Sour(Market.East, T.RelationsHit);
        }

        Assert.Equal(T.RelationsCap, c.Relations(Market.East));

        // Time mends, all the way back to nothing rather than approaching it.
        for (var day = 0; day < 10_000; day++)
        {
            c.Settle(day);
        }

        Assert.Equal(0.0, c.Relations(Market.East));
    }

    [Fact]
    public void The_directorate_will_only_have_so_many_on_the_table()
    {
        var c = new Contracts(T);
        var rng = Rng.FromSeed(1961);
        var coal = T.ResourceIndex("Coal");

        Assert.True(c.WillOffer(Market.East));
        for (var i = 0; i < T.MaxOpenOffers; i++)
        {
            c.Offer(Market.East, coal, 0, rng);
        }

        Assert.False(c.WillOffer(Market.East));

        // Taking one clears the table for another.
        c.Accept(c.Offers()[0].Id);
        Assert.True(c.WillOffer(Market.East));
    }
}
