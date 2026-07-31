namespace RedRepublic.Sim.Tests;

/// <summary>
/// The player's surface: every verb reaches a handler, and every refusal carries
/// a sentence a screen can show.
/// </summary>
public sealed class CommandsTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// <b>Every verb in the vocabulary is carried out by something.</b>
    /// </summary>
    /// <remarks>
    /// Eleven of these fell through to "that is not something the republic can do
    /// yet" and nothing said so: a refusal is a legitimate answer, so a command
    /// nobody had written a handler for looked exactly like a command being
    /// properly declined. The catch-all is what this asserts is unreachable —
    /// it is the string a player would have seen, and it may never be the answer
    /// to a verb the enum admits.
    /// </remarks>
    [Fact]
    public void No_verb_falls_through_to_the_catch_all()
    {
        var world = Fixtures.Flat();
        var unhandled = new List<CommandKind>();

        foreach (var kind in Enum.GetValues<CommandKind>())
        {
            // Deliberately nonsense arguments: what is under test is whether a
            // handler exists at all, and a handler that refuses has answered.
            var outcome = Commands.CarryOut(world, new Command(kind));
            if (outcome.Refusal == "that is not something the republic can do yet")
            {
                unhandled.Add(kind);
            }
        }

        Assert.Empty(unhandled);
    }

    /// <summary>
    /// A refusal is a sentence, not a code. It is what a panel prints in a toast
    /// and what greys out a button with a tooltip explaining itself.
    /// </summary>
    [Fact]
    public void Every_refusal_carries_a_sentence()
    {
        var world = Fixtures.Flat();

        foreach (var kind in Enum.GetValues<CommandKind>())
        {
            var outcome = Commands.CarryOut(world, new Command(kind));
            if (!outcome.Accepted)
            {
                Assert.False(
                    string.IsNullOrWhiteSpace(outcome.Refusal),
                    $"{kind} refused without saying why");
            }
        }
    }

    /// <summary>
    /// <b>The order is the decision</b>: when throughput or money runs short the
    /// first rule is served first, so moving one is its own verb rather than a
    /// re-send of the whole policy.
    /// </summary>
    [Fact]
    public void Trade_rules_are_added_reordered_and_withdrawn()
    {
        var world = Fixtures.Flat();
        var coal = T.ResourceIndex("Coal");
        var steel = T.ResourceIndex("Steel");

        Assert.True(world.Issue(
            Command.AddTradeRule(coal, Market.East, TradeAction.Sell)).Accepted);
        Assert.True(world.Issue(
            Command.AddTradeRule(steel, Market.West, TradeAction.Buy)).Accepted);
        Assert.Equal(2, world.TradeRules.Count);

        Assert.True(world.Issue(Command.MoveTradeRule(1, 0)).Accepted);
        Assert.Equal(steel, world.TradeRules[0].Resource);

        Assert.True(world.Issue(Command.RemoveTradeRule(0)).Accepted);
        Assert.Single(world.TradeRules);
        Assert.Equal(coal, world.TradeRules[0].Resource);

        var gone = world.Issue(Command.RemoveTradeRule(7));
        Assert.False(gone.Accepted);
        Assert.Contains("7", gone.Refusal, StringComparison.Ordinal);
    }

    /// <summary>
    /// A tank is not a coal bunker, and the player is told so at the door rather
    /// than left with an order that stands for ever and never fills a tonne.
    /// </summary>
    [Fact]
    public void A_standing_order_is_refused_where_it_could_never_fill()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var coal = T.ResourceIndex("Coal");

        var house = Place(world, "Apartment");
        var nowhere = world.Issue(Command.SetStandingOrder(house, coal, 10.0));
        Assert.False(nowhere.Accepted);
        Assert.Contains("order", nowhere.Refusal, StringComparison.Ordinal);

        var depot = Place(world, "Depot");
        Assert.True(world.Issue(Command.SetStandingOrder(depot, coal, 50.0)).Accepted);
        Assert.Equal(50.0, world.Buildings.Orders.Get(world.Buildings.IndexOf(depot), coal));

        var toomuch = world.Issue(Command.SetStandingOrder(depot, coal, 1_000_000.0));
        Assert.False(toomuch.Accepted);
        Assert.Contains("fit", toomuch.Refusal, StringComparison.Ordinal);
    }

    /// <summary>
    /// A working day the republic will not roster is refused rather than clamped:
    /// a player who asked for twenty hours and got sixteen has been silently
    /// overruled, and the panel would show a number they did not choose.
    /// </summary>
    [Fact]
    public void A_shift_outside_what_the_republic_rosters_is_refused_not_clamped()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var mine = T.BuildingIndex("CoalMine");

        var absurd = world.Issue(Command.SetKindHours(mine, T.MaxHours + 4.0));
        Assert.False(absurd.Accepted);
        Assert.Null(world.Buildings.Shifts.OfKind(mine));

        Assert.True(world.Issue(Command.SetKindHours(mine, T.MaxHours)).Accepted);
        Assert.Equal(T.MaxHours, world.Buildings.Shifts.OfKind(mine));

        // Clearing falls back to the rule above rather than freezing the number.
        Assert.True(world.Issue(Command.SetKindHours(mine, null)).Accepted);
        Assert.Null(world.Buildings.Shifts.OfKind(mine));

        // And clearing what was never set says so rather than pretending.
        Assert.False(world.Issue(Command.SetKindHours(mine, null)).Accepted);
    }

    /// <summary>
    /// A policy in the world always names a post that exists. A typo would
    /// otherwise sit in a save quietly importing nothing, which looks exactly
    /// like a republic that cannot afford anything.
    /// </summary>
    [Fact]
    public void An_import_policy_naming_no_post_is_refused()
    {
        var world = Fixtures.Flat();

        var nonsense = world.Issue(Command.SetImportPolicy(null, 9999));
        Assert.False(nonsense.Accepted);

        var post = world.Frontier.Crossings[0].Id;
        Assert.True(world.Issue(Command.SetImportPolicy(null, post)).Accepted);
        Assert.Equal(post, world.BuildPolicy.Global);

        // A site with no instruction of its own has nothing to clear.
        var site = Destination.RoadSite(3);
        Assert.False(world.Issue(Command.ClearImportPolicy(site)).Accepted);

        Assert.True(world.Issue(Command.SetImportPolicy(site, null)).Accepted);
        Assert.True(world.BuildPolicy.IsOverridden(site));
        Assert.True(world.Issue(Command.ClearImportPolicy(site)).Accepted);
    }

    /// <summary>
    /// Hiring is one transaction at a genuine boundary: the fee leaves and the
    /// workers set out, and neither happens without the other. They arrive at a
    /// frontier post, not in the yard.
    /// </summary>
    [Fact]
    public void Foreign_builders_cost_money_and_arrive_at_a_post()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var office = Place(world, "ConstructionOffice");
        var at = world.Buildings.IndexOf(office);
        world.Buildings.AddWork(at, T.BLabour[world.Buildings.KindAt(at)]);

        var broke = world.Issue(Command.HireForeign(Market.East, office, 5));
        Assert.False(broke.Accepted);
        Assert.Contains("treasury holds", broke.Refusal, StringComparison.Ordinal);

        world.Treasury.Add(Market.East, 1_000_000.0);
        var before = world.Treasury.Of(Market.East);
        Assert.True(world.Issue(Command.HireForeign(Market.East, office, 5)).Accepted);

        Assert.Equal(5, world.Crews.HiredAt(office));
        Assert.True(world.Treasury.Of(Market.East) < before, "hiring was free");
        Assert.Equal(5, world.Migration.HeadsWaiting());

        // Hiring nobody, and hiring to something that is not an office.
        Assert.False(world.Issue(Command.HireForeign(Market.East, office, 0)).Accepted);
        var house = Place(world, "Apartment");
        Assert.False(world.Issue(Command.HireForeign(Market.East, house, 3)).Accepted);
    }

    private static int Place(World world, string id)
    {
        var kind = T.BuildingIndex(id);
        for (var y = 200.0; y < world.Terrain.Extent - 200.0; y += 60.0)
        {
            for (var x = 200.0; x < world.Terrain.Extent - 200.0; x += 60.0)
            {
                if (Commands.CanPlace(world, kind, x, y) is not null)
                {
                    continue;
                }

                var outcome = world.Issue(Command.Place(kind, x, y));
                Assert.True(outcome.Accepted, outcome.Refusal);
                return outcome.Id;
            }
        }

        throw new InvalidOperationException($"nowhere on this map will take a {id}");
    }
}
