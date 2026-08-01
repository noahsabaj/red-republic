namespace RedRepublic.Sim.Tests;

/// <summary>
/// Where the republic used to leak.
/// </summary>
/// <remarks>
/// <b>Every one of these is the same mistake in a different pass.</b> A system
/// proposes and the one writer applies, so the stock and the treasury a pass
/// reads are yesterday's until it returns — and a pass that decides twice about
/// the same tonne, or the same rouble, commits both. The result is goods and
/// money out of nothing, always in the direction that flatters the player, and
/// always invisible because the totals still balance one line at a time.
/// </remarks>
public sealed class LeakTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// Two sell rules for one resource sell it once, not twice.
    /// </summary>
    /// <remarks>
    /// <b>The invariant, not the reproduction.</b> The <c>Consume</c> a rule
    /// emits clamps at what is on hand and the <c>Money</c> beside it does not,
    /// so two rules naming one resource could be paid twice for one tonne. In
    /// practice a customs house clears thirty tonnes a day and the per-tick
    /// allowance runs out inside the first rule, which is why this sat unnoticed
    /// — and is exactly why the assertion here is "the republic was paid for
    /// what it sold" rather than a count of anything. A throughput figure is
    /// balance and could be raised tomorrow.
    /// </remarks>
    [Fact]
    public void One_tonne_is_sold_once_however_many_rules_name_it()
    {
        var world = CustomsPost(out var house, out var bloc);
        var coal = T.ResourceIndex("Coal");
        world.Buildings.Stock.Add(house, coal, 12.0);

        var before = world.Treasury.Of(bloc);
        Assert.True(world.Issue(Command.AddTradeRule(coal, bloc, TradeAction.Sell)).Accepted);
        Assert.True(world.Issue(Command.AddTradeRule(coal, bloc, TradeAction.Sell)).Accepted);

        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        var sold = 12.0 - world.Buildings.Stock.Get(house, coal);
        var price = bloc == Market.East ? T.ResourcePriceEast[coal] : T.ResourcePriceWest[coal];
        var earned = world.Treasury.Of(bloc) - before;

        Assert.True(sold > 0.0, "a customs house holding coal with a sell rule should sell some");
        Assert.Equal(sold * price, earned, 6);
    }

    /// <summary>
    /// Two buy rules cannot each spend the whole treasury.
    /// </summary>
    /// <remarks>
    /// The half that bit: each rule read the purse as it stood at the start of
    /// the pass, so on a thin treasury both bought and the republic went into
    /// the red a fraction of a tonne at a time, every tick, for as long as the
    /// rules stood.
    /// </remarks>
    [Fact]
    public void The_purse_is_spent_once_however_many_rules_draw_on_it()
    {
        var world = CustomsPost(out _, out var bloc);
        world.Treasury.Set(bloc, 400.0);

        Assert.True(world.Issue(
            Command.AddTradeRule(T.ResourceIndex("Bricks"), bloc, TradeAction.Buy)).Accepted);
        Assert.True(world.Issue(
            Command.AddTradeRule(T.ResourceIndex("Planks"), bloc, TradeAction.Buy)).Accepted);

        for (var tick = 0; tick < SimClock.TicksPerDay * 5; tick++)
        {
            world.Tick();
        }

        Assert.True(world.Treasury.Of(bloc) >= 0.0);
    }

    /// <summary>
    /// <b>A shortfall is a smaller payment, never a debt.</b>
    /// </summary>
    /// <remarks>
    /// The rule the loans table states outright and the stockpiles already
    /// followed. It was tested through <see cref="Treasury.Take"/>, which no
    /// system used: every negative amount in the republic went through
    /// <see cref="Treasury.Add"/>, which was an unclamped <c>+=</c>.
    /// </remarks>
    [Fact]
    public void A_purse_never_goes_below_nothing()
    {
        var treasury = new Treasury();
        treasury.Add(Market.East, 100.0);
        treasury.Add(Market.East, -250.0);

        Assert.Equal(0.0, treasury.Of(Market.East));
    }

    /// <summary>
    /// A trickle of ore does not sustain a works at its full rate.
    /// </summary>
    /// <remarks>
    /// Asking only whether there is <i>any</i> input let a gram of coal light a
    /// power station for ever — a works reading as healthy while the republic
    /// starved it, which hides scarcity exactly when the player most needs to
    /// see it.
    /// </remarks>
    [Fact]
    public void What_is_in_the_bin_decides_the_batch()
    {
        var world = World.Found(new WorldSpec(1961, 2000.0, 0), T);
        var kind = T.BuildingIndex("Sawmill");
        var at = Scenario.FindSite(world, kind, 1000.0, 1000.0, 900.0);
        Assert.NotNull(at);

        var placed = world.Issue(Command.Place(kind, at.Value.X, at.Value.Y));
        Assert.True(placed.Accepted);
        var b = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(b, T.BLabour[kind]);
        world.Buildings.SetStaff(b, T.BWorkers[kind]);
        world.Buildings.SetPowered(b, true);

        var wood = T.ResourceIndex("Wood");
        var planks = T.ResourceIndex("Planks");

        // A single gram, and a full day of it.
        world.Buildings.Stock.Set(b, wood, 0.001);
        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        // What came out cannot be more than what went in allowed.
        Assert.True(world.Buildings.Stock.Get(b, wood) <= 0.001 + 1e-9);
        Assert.True(
            world.Buildings.Stock.Get(b, planks) < 0.01,
            $"a gram of wood made {world.Buildings.Stock.Get(b, planks):0.###} t of planks");
    }

    /// <summary>
    /// A garage's vehicles go with it, and it is not pulled down with any of
    /// them out on the road.
    /// </summary>
    /// <remarks>
    /// Without this a republic demolished a depot and rebuilt it for a second
    /// free fleet, as often as it liked.
    /// </remarks>
    [Fact]
    public void A_garage_takes_its_vehicles_with_it()
    {
        var world = World.Found(new WorldSpec(1961, 2000.0, 0), T);
        var kind = T.BuildingIndex("MotorDepot");
        var at = Scenario.FindSite(world, kind, 1000.0, 1000.0, 900.0);
        Assert.NotNull(at);

        var placed = world.Issue(Command.Place(kind, at.Value.X, at.Value.Y));
        Assert.True(placed.Accepted);
        var b = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(b, T.BLabour[kind]);

        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        var fleet = world.Fleet.Count;
        Assert.True(fleet > 0, "a motor depot should take delivery of its establishment");

        Assert.True(world.Issue(Command.Demolish(placed.Id)).Accepted);
        Assert.Equal(0, world.Fleet.Count);
    }

    /// <summary>
    /// A run ordered by mistake can be called off.
    /// </summary>
    [Fact]
    public void A_way_ordered_by_mistake_can_be_called_off()
    {
        var world = World.Found(new WorldSpec(1961, 3000.0, 0), T);
        var dirt = T.GradeIndex("Dirt");

        Outcome ordered = default;
        for (var y = 300.0; y < 2400.0 && !ordered.Accepted; y += 100.0)
        {
            ordered = world.Issue(Command.OrderRoad(300.0, y, 1400.0, y, dirt, false));
        }

        Assert.True(ordered.Accepted, "somewhere on this map a dirt track can be ordered");
        Assert.Single(world.RoadWorks.Sites);

        var site = Destination.RoadSite(ordered.Id);
        Assert.True(world.Issue(Command.CancelWorks(site)).Accepted);
        Assert.Empty(world.RoadWorks.Sites);

        // And calling off one that is not there says so rather than pretending.
        var again = world.Issue(Command.CancelWorks(site));
        Assert.False(again.Accepted);
        Assert.NotEmpty(again.Refusal);
    }

    /// <summary>
    /// A customs house on a post, staffed, with an empty purse and a full one.
    /// </summary>
    private static World CustomsPost(out int house, out Market bloc)
    {
        var world = World.Found(new WorldSpec(1961, 3000.0, 0), T);
        var post = world.Frontier.Crossings[0];
        bloc = post.Bloc;

        var kind = T.BuildingIndex("Customs");
        var at = Scenario.FindSite(world, kind, post.X, post.Y, 400.0);
        Assert.NotNull(at);

        var placed = world.Issue(Command.Place(kind, at.Value.X, at.Value.Y));
        Assert.True(placed.Accepted);
        house = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(house, T.BLabour[kind]);

        world.Treasury.Set(bloc, 100_000.0);
        return world;
    }
}
