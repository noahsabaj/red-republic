namespace RedRepublic.Sim.Tests;

/// <summary>
/// Getting to work, and what extends how far one can reach.
/// </summary>
/// <remarks>
/// Walking alone is a complete model of a village and a useless model of a city:
/// it means a republic can never be larger than the distance a person will walk.
/// Everything here is about the ceiling that lifts, and what it costs.
/// </remarks>
public sealed class TransportTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>A road from end to end, with junctions every 500 m.</summary>
    private static Network Highway(double length)
    {
        var roads = new Network();
        var steps = Math.Max(1, (int)(length / 500.0));
        var previous = roads.AddNode(0.0, 0.0);
        for (var i = 1; i <= steps; i++)
        {
            var next = roads.AddNode(length * i / steps, 0.0);
            roads.Connect(previous, next, Units.KphToMps(T.CommercialKph((int)Medium.Road)) * 2.5);
            previous = next;
        }

        return roads;
    }

    /// <summary>
    /// <b>Walking round a lake on a road is a real route; the straight line
    /// through it is not.</b> But the road never claims to be longer than
    /// walking directly, because nobody walks the long way round on purpose.
    /// </summary>
    [Fact]
    public void The_walk_takes_the_road_only_when_the_road_is_shorter()
    {
        var roads = new Network();
        var a = roads.AddNode(0.0, 0.0);
        var b = roads.AddNode(500.0, 400.0);
        var c = roads.AddNode(1000.0, 0.0);
        roads.Connect(a, b, 12.0);
        roads.Connect(b, c, 12.0);

        // The dog-leg is longer than the straight line, so the straight line wins.
        Assert.Equal(1000.0, Transport.CommuteDistance(roads, 0.0, 0.0, 1000.0, 0.0, T), 6);

        // With nothing near either end, it is the straight line by default.
        var empty = new Network();
        Assert.Equal(
            1000.0, Transport.CommuteDistance(empty, 0.0, 0.0, 1000.0, 0.0, T), 6);
    }

    /// <summary>
    /// <b>The bound is on time, not distance.</b> The same three quarters of an
    /// hour is a short walk or a long ride, and that difference is the whole
    /// mechanic — a job beyond walking range is a job somebody can hold if there
    /// is a seat.
    /// </summary>
    [Fact]
    public void A_seat_reaches_further_than_a_walk()
    {
        var world = Fixtures.Flat();
        var far = T.MaxWalkM * 2.0;
        var road = Highway(far + 1000.0);

        Assert.False(Transport.IsReachable(road, 100.0, 0.0, 100.0 + far, 0.0, T));

        var services = new List<Service> { new(Medium.Road, 20) };
        var walked = Transport.ReachAt(world, 0.0, 0.0, 200.0, 0.0, services, false);
        Assert.NotNull(walked);
        Assert.False(walked.Value.IsCarried, "a two-hundred-metre journey wanted a bus");
    }

    /// <summary>
    /// A service with no seats left carries nobody, however good the road is.
    /// Extending reach is a capacity you fund rather than a rule that changes.
    /// </summary>
    [Fact]
    public void A_full_service_carries_nobody()
    {
        var world = Fixtures.Flat();
        var far = T.MaxWalkM * 2.0;

        Assert.Null(Transport.ReachAt(world, 0.0, 0.0, far, 0.0, [new Service(Medium.Road, 0)], false));
    }

    /// <summary>
    /// <b>What makes street lighting a mechanic rather than a decoration.</b> A
    /// short walk is always fine — nobody needs a lamp to cross the yard, and a
    /// rule that said otherwise would make the first night shift impossible
    /// rather than expensive.
    /// </summary>
    [Fact]
    public void A_short_walk_is_fine_in_the_dark_and_a_long_unlit_one_is_not()
    {
        var world = Fixtures.Flat();
        var near = T.NightWalkM / 2.0;
        var far = T.NightWalkM * 3.0;

        var across = Transport.ReachAt(world, 0.0, 0.0, near, 0.0, [], true);
        Assert.NotNull(across);
        Assert.False(across.Value.IsCarried);

        Assert.Null(
            Transport.ReachAt(world, 0.0, 0.0, far, 0.0, [], true));
    }

    /// <summary>
    /// <b>A ride is a ride whatever the hour.</b> The passenger is not the one
    /// out in the dark, so a republic can answer the night with lamps or with a
    /// service — and both cost.
    /// </summary>
    [Fact]
    public void A_night_shift_can_be_answered_with_a_service_instead_of_lamps()
    {
        var world = Fixtures.Flat();
        var far = T.NightWalkM * 3.0;
        Lay(world.Roads, 0.0, far);

        var carried = Transport.ReachAt(
            world, 0.0, 0.0, far, 0.0, [new Service(Medium.Road, 5)], true);

        Assert.NotNull(carried);
        Assert.True(carried.Value.IsCarried, "the night shift walked an unlit road");
    }

    /// <summary>
    /// A lit way in is the other answer, and the search only ever traverses lit
    /// road — so a republic that has just lit its main street can staff the night
    /// shift on foot.
    /// </summary>
    [Fact]
    public void A_lit_road_lets_the_night_shift_walk()
    {
        var world = Fixtures.Flat();
        var far = T.NightWalkM * 3.0;
        Lay(world.Roads, 0.0, far, lamps: true);

        for (var s = 0; s < world.Roads.SegmentCount; s++)
        {
            world.Roads.SetAlight(s, true);
        }

        var walked = Transport.ReachAt(world, 0.0, 0.0, far, 0.0, [], true);
        Assert.NotNull(walked);
        Assert.False(walked.Value.IsCarried, "a lit walk was not found");

        // Lamps that are not burning are lamps that light nothing: a republic
        // short of generation puts its night shift out with its streets.
        for (var s = 0; s < world.Roads.SegmentCount; s++)
        {
            world.Roads.SetAlight(s, false);
        }

        Assert.Null(Transport.ReachAt(world, 0.0, 0.0, far, 0.0, [], true));
    }

    /// <summary>
    /// A depot's seats are what it can actually run, and a depot with a dry tank
    /// runs nothing — the same shape as every other input limiter, so a shortage
    /// degrades a service rather than switching it off at an invisible threshold.
    /// </summary>
    [Fact]
    public void A_depot_with_no_fuel_runs_no_service()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var depot = Built(world, "BusDepot");
        var at = world.Buildings.IndexOf(depot);
        var fuel = T.ResourceIndex("Fuel");

        Assert.Empty(Transport.Services(world));

        world.Buildings.Stock.Add(at, fuel, 50.0);
        var running = Transport.Services(world);
        Assert.Single(running);
        Assert.True(running[0].Seats > 0);

        // And what it carries, it burns.
        var burned = Transport.FuelBurn(world, running[0].Seats);
        Assert.Single(burned);
        Assert.Equal(at, burned[0].Depot);
        Assert.True(burned[0].Tonnes > 0.0);
    }

    private static void Lay(Network roads, double from, double to, bool lamps = false)
    {
        var previous = roads.JunctionAt(from, 0.0, T.JunctionMerge);
        var steps = Math.Max(1, (int)Math.Ceiling((to - from) / T.JunctionSpacing));
        for (var i = 1; i <= steps; i++)
        {
            var next = roads.JunctionAt(from + ((to - from) * i / steps), 0.0, T.JunctionMerge);
            roads.Connect(previous, next, Units.KphToMps(40.0), lamps);
            previous = next;
        }
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
                world.Buildings.SetPowered(i, true);
                return outcome.Id;
            }
        }

        throw new InvalidOperationException($"nowhere on this map will take a {id}");
    }
}
