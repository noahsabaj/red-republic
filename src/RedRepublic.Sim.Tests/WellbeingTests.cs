namespace RedRepublic.Sim.Tests;

/// <summary>How well the republic is serving the people in one home.</summary>
public sealed class WellbeingTests
{
    private static Tables T => Fixtures.Tables;

    private static Contentment Met(double all) =>
        new(all, all, all, all, all, all, all, all, 0.0);

    /// <summary>
    /// The weights and the names have to stay the same length and the same
    /// order. They are read together — a component with no name cannot be the
    /// answer to "what is worst", and a name with no weight would be scored as
    /// nothing.
    /// </summary>
    [Fact]
    public void Every_want_has_a_name_and_a_weight()
    {
        Assert.Equal(Contentment.Names.Length, T.ContentmentWeights.Count);
        Assert.Equal(8, Contentment.Names.Length);
        Assert.Equal(8, Met(1.0).Parts.Length);

        foreach (var w in T.ContentmentWeights)
        {
            Assert.True(w > 0.0, "a want with no weight is a want that is not scored");
        }
    }

    /// <summary>
    /// Food and warmth dominate because they are the two a person cannot do
    /// without. That ordering is the balance decision, and flattening it would
    /// leave every other test here green.
    /// </summary>
    [Fact]
    public void Food_and_warmth_outweigh_everything_else()
    {
        var w = T.ContentmentWeights;
        var provisions = w[0];
        var warmth = w[1];

        for (var i = 2; i < w.Count; i++)
        {
            Assert.True(provisions > w[i], $"provisions should outweigh {Contentment.Names[i]}");
            Assert.True(warmth >= w[i], $"warmth should not be outweighed by {Contentment.Names[i]}");
        }
    }

    [Fact]
    public void A_home_that_wants_for_nothing_scores_full_marks()
    {
        Assert.Equal(0.0, Contentment.Nothing.NeedsMet(T));
        Assert.Equal(1.0, Met(1.0).NeedsMet(T));
        Assert.Equal(0.5, Met(0.5).NeedsMet(T), 12);

        // Out-of-range components are clamped rather than trusted, so a system
        // that writes 1.4 cannot inflate a score.
        Assert.Equal(1.0, Met(1.4).NeedsMet(T));
        Assert.Equal(0.0, Met(-2.0).NeedsMet(T));
    }

    /// <summary>
    /// <b>A want is a way to fail; a comfort is a way to do better.</b> Comforts
    /// lift the score and are never one of the things to be short of — modelled
    /// as a want they would have dropped every standing republic's score on the
    /// day the goods were invented, for a shortfall that did not exist the day
    /// before.
    /// </summary>
    [Fact]
    public void Comforts_lift_a_score_and_never_lower_one()
    {
        var bare = Met(0.5);
        var comforted = bare with { Comforts = 1.0 };

        // The needs are identical: adding comforts cannot re-mark work already
        // done.
        Assert.Equal(bare.NeedsMet(T), comforted.NeedsMet(T));

        Assert.Equal(0.0, bare.Lift(T));
        Assert.Equal(T.ComfortLift, comforted.Lift(T));
        Assert.Equal(bare.Overall(T) + T.ComfortLift, comforted.Overall(T), 12);

        // It certainly helps and it is not a dealbreaker: a republic that meets
        // nothing but stocks televisions is still failing.
        var onlyTelevisions = Contentment.Nothing with { Comforts = 1.0 };
        Assert.Equal(T.ComfortLift, onlyTelevisions.Overall(T));
        Assert.True(onlyTelevisions.Overall(T) < 0.25);

        // And a full house cannot exceed one.
        Assert.Equal(1.0, (Met(1.0) with { Comforts = 1.0 }).Overall(T));
    }

    /// <summary>
    /// <b>Tell the player which thing is worst, not what their score is.</b>
    /// Ranked by weighted loss, so a small shortfall in food outranks a total
    /// absence of cinemas.
    /// </summary>
    [Fact]
    public void The_worst_want_is_the_one_costing_the_most()
    {
        Assert.Null(Met(1.0).Worst(T));

        var cold = Met(1.0) with { Warmth = 0.0 };
        Assert.Equal("Warmth", cold.Worst(T));

        var hungry = Met(1.0) with { Provisions = 0.0 };
        Assert.Equal("Provisions", hungry.Worst(T));

        // A small shortfall in food beats a total absence of culture, because
        // the weights say food matters more.
        var both = Met(1.0) with { Provisions = 0.7, Culture = 0.0 };
        Assert.Equal("Provisions", both.Worst(T));

        // A home that wants for everything leads with the heaviest want.
        Assert.Equal("Provisions", Contentment.Nothing.Worst(T));
    }

    /// <summary>
    /// Comforts can never be the answer: "your people's biggest problem is no
    /// television" is not something a panel should say to somebody whose estate
    /// is cold.
    /// </summary>
    [Fact]
    public void No_television_is_never_the_biggest_problem()
    {
        var everythingButComforts = Met(1.0) with { Comforts = 0.0 };
        Assert.Null(everythingButComforts.Worst(T));

        foreach (var name in Contentment.Names)
        {
            Assert.NotEqual("Comforts", name);
        }
    }

    /// <summary>
    /// Loyalty follows contentment slowly, so one bad winter does not empty a
    /// town and one good month does not fill it.
    /// </summary>
    [Fact]
    public void Loyalty_drifts_rather_than_tracking()
    {
        Assert.True(T.LoyaltyDrift > 0.0 && T.LoyaltyDrift < 0.2,
            "loyalty should follow contentment slowly, not track it");

        // Somebody arriving is above the leaving threshold, so a republic does
        // not lose people the day they turn up.
        Assert.True(Citizens.ArrivingLoyalty > T.LoyaltyLeaves);

        // And a republic has to look better than the middling to draw anyone.
        Assert.True(T.ContentAttracts > 0.5);
    }
}
