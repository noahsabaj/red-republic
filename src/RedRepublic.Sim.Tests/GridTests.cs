namespace RedRepublic.Sim.Tests;

/// <summary>
/// The energised grid: who is on it, who is not, and what that costs.
/// </summary>
/// <remarks>
/// The claim this whole structure exists to make true is that a power station
/// stands <i>somewhere</i>. Every test here is a way that could stop being true
/// without anything else noticing.
/// </remarks>
public sealed class GridTests
{
    private static Tables T => Fixtures.Tables;

    private static int Power => T.UtilityIndex("Power");

    private static int Heat => T.UtilityIndex("Heat");

    /// <summary>A span, ordered and finished, ready to energise.</summary>
    private static LineSite Span(int kind, double fromX, double fromY, double toX, double toY)
    {
        var works = new LineWorks(T);
        works.Order(kind, fromX, fromY, toX, toY, 0, out var site);
        Assert.NotNull(site);
        return site;
    }

    /// <summary>
    /// The distance that decides a connection is to the <b>span</b>, not to its
    /// ends: a building beside the middle of a line is beside the line.
    /// </summary>
    [Fact]
    public void A_building_beside_the_middle_of_a_span_is_beside_it()
    {
        var grid = new Networks(T);
        grid.Energise(Span(Power, 0.0, 0.0, 1000.0, 0.0));
        var reach = T.Utilities[Power].Reach;

        Assert.True(grid.Attach(1, 500.0, reach - 1.0, Power));
        Assert.False(
            grid.Attach(2, 500.0, reach + 100.0, Power),
            "the reach stretched further than it is authored to");

        // And past the end is measured to the end, not to the infinite line.
        Assert.False(grid.Attach(3, 1000.0 + reach + 100.0, 0.0, Power));
        Assert.True(grid.Attach(4, 1000.0 + reach - 1.0, 0.0, Power));
    }

    /// <summary>
    /// A pipe never carries electricity. Nodes carry the kind of line that made
    /// them, so two spans whose ends fall in the same place stay two networks —
    /// otherwise the grid answers the same number for both.
    /// </summary>
    [Fact]
    public void The_two_networks_never_touch()
    {
        var grid = new Networks(T);
        grid.Energise(Span(Power, 0.0, 0.0, 1000.0, 0.0));
        grid.Energise(Span(Heat, 0.0, 0.0, 1000.0, 0.0));

        Assert.True(grid.Attach(1, 500.0, 50.0, Power));
        Assert.True(grid.Attach(1, 500.0, 50.0, Heat));

        var power = grid.NetworkOf(1, Power);
        var heat = grid.NetworkOf(1, Heat);
        Assert.True(power >= 0 && heat >= 0);
        Assert.True(power != heat, "the pylon and the pipe are the same network");
    }

    /// <summary>
    /// Two spans that meet are one network; two that do not are two — and joining
    /// them later merges them without rewiring anybody.
    /// </summary>
    [Fact]
    public void Spans_that_meet_are_one_network_and_a_later_span_joins_two()
    {
        var grid = new Networks(T);
        grid.Energise(Span(Power, 0.0, 0.0, 800.0, 0.0));
        grid.Energise(Span(Power, 3000.0, 0.0, 3800.0, 0.0));

        Assert.True(grid.Attach(1, 100.0, 50.0, Power));
        Assert.True(grid.Attach(2, 3100.0, 50.0, Power));
        Assert.False(grid.Together(1, 2, Power), "two islands are not one grid");

        // The span that closes the gap. Neither building is touched.
        grid.Energise(Span(Power, 800.0, 0.0, 3000.0, 0.0));
        Assert.True(grid.Together(1, 2, Power), "the grids did not merge when joined");

        // The span is the sum of what was built, not the distance between ends.
        Assert.Equal(3800.0, grid.SpanOf(grid.NetworkOf(1, Power), Power), 9);
    }

    /// <summary>
    /// A building with nothing near it is on no network at all, and that is the
    /// state the whole structure exists to make representable.
    /// </summary>
    [Fact]
    public void A_building_with_no_line_near_it_is_on_nothing()
    {
        var grid = new Networks(T);
        grid.Energise(Span(Power, 0.0, 0.0, 800.0, 0.0));

        Assert.False(grid.Attach(9, 5000.0, 5000.0, Power));
        Assert.Equal(-1, grid.NetworkOf(9, Power));
        Assert.False(grid.Together(9, 9, Power));
    }

    [Fact]
    public void A_demolished_building_is_unplugged()
    {
        var grid = new Networks(T);
        grid.Energise(Span(Power, 0.0, 0.0, 800.0, 0.0));
        grid.AttachAll(1, 400.0, 40.0);
        Assert.True(grid.NetworkOf(1, Power) >= 0);

        grid.Detach(1);
        Assert.Equal(-1, grid.NetworkOf(1, Power));
        Assert.Equal(0, grid.ConnectedCount(Power));
    }

    /// <summary>
    /// Which network a thing is on may not depend on the order the lines were
    /// built in, or a save that replayed its journal could come back different.
    /// </summary>
    [Fact]
    public void The_answer_does_not_depend_on_build_order()
    {
        (double FromX, double ToX)[] spans =
        [
            (0.0, 800.0),
            (800.0, 1600.0),
            (1600.0, 2400.0),
        ];

        Assert.Equal(NetworkUnder([0, 1, 2]), NetworkUnder([2, 1, 0]));
        Assert.Equal(NetworkUnder([0, 1, 2]), NetworkUnder([1, 2, 0]));

        int NetworkUnder(int[] order)
        {
            var grid = new Networks(T);
            foreach (var i in order)
            {
                grid.Energise(Span(Power, spans[i].FromX, 0.0, spans[i].ToX, 0.0));
            }

            grid.Attach(1, 2300.0, 40.0, Power);
            return grid.NetworkOf(1, Power);
        }
    }

    /// <summary>
    /// <b>An isolated power station lights nothing, including itself.</b> That is
    /// the abstraction this whole module replaces: before it, a plant anywhere on
    /// the map lit every building on it.
    /// </summary>
    [Fact]
    public void A_plant_strung_to_nothing_lights_nothing()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var plant = Built(world, "PowerPlant");
        var works = Built(world, "FoodFactory");
        world.Buildings.Stock.Add(
            world.Buildings.IndexOf(plant), T.ResourceIndex("Coal"), 50.0);

        world.Tick();

        Assert.False(
            world.Buildings.PoweredAt(world.Buildings.IndexOf(works)),
            "a works drew current from a station with no wire out of it");
    }

    /// <summary>
    /// <b>A span is ordered, materialled, built, and only then carries anything.</b>
    /// The whole loop, because each half of it can be right while the join is not:
    /// a site nothing delivers to is never built, and a site that finishes without
    /// joining a grid is steel nobody can find.
    /// </summary>
    [Fact]
    public void A_span_is_ordered_materialled_and_only_then_carries()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var steel = T.ResourceIndex("Steel");
        var office = Built(world, "ConstructionOffice");

        // Real people, because the labour pass rewrites the staff of every
        // workplace each morning and an office nobody works at sends nobody.
        var block = Built(world, "Apartment");
        for (var i = 0; i < 40; i++)
        {
            world.Citizens.AddArrival(block, 30);
        }

        var from = (X: world.Buildings.XAt(world.Buildings.IndexOf(office)),
                    Y: world.Buildings.YAt(world.Buildings.IndexOf(office)));
        var ordered = world.Issue(Command.OrderLine(
            Power, from.X, from.Y, from.X + 400.0, from.Y));
        Assert.True(ordered.Accepted, ordered.Refusal);

        var site = world.LineWorks.Get(ordered.Id);
        Assert.NotNull(site);
        Assert.Empty(world.Grid.Lines);

        // Nothing is built without the steel, however long the republic waits.
        for (var i = 0; i < SimClock.TicksPerDay; i++)
        {
            world.Tick();
        }

        Assert.Equal(0.0, site.WorkDone);
        Assert.Single(world.LineWorks.Sites);

        // Deliver it, and the same crews that build a factory string the span.
        world.LineWorks.Stock.Add(
            world.LineWorks.IndexOf(site.Id), steel, site.Wants(steel, T));

        for (var day = 0; day < 60 && world.LineWorks.Sites.Count > 0; day++)
        {
            for (var i = 0; i < SimClock.TicksPerDay; i++)
            {
                world.Tick();
            }
        }

        Assert.Empty(world.LineWorks.Sites);
        Assert.Single(world.Grid.Lines);
        Assert.True(
            world.Grid.NetworkOf(office, Power) >= 0,
            "the office the span was strung from is not on the grid it made");
    }

    /// <summary>A staffed, finished building of a kind, somewhere it will stand.</summary>
    private static int Built(World world, string id)
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
                var i = world.Buildings.IndexOf(outcome.Id);
                world.Buildings.AddWork(i, T.BLabour[kind]);
                world.Buildings.SetStaff(i, world.Buildings.Jobs(i));
                return outcome.Id;
            }
        }

        throw new InvalidOperationException($"nowhere on this map will take a {id}");
    }
}
