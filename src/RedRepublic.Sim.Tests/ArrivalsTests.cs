namespace RedRepublic.Sim.Tests;

/// <summary>
/// Belts, settlers and visitors: the three passes that were declared and never
/// ran.
/// </summary>
/// <remarks>
/// <c>Job.Settle</c> and <c>Job.Tour</c> existed in the fleet and nothing ever
/// issued one, so a party could arrive at the frontier and wait there for ever
/// while the republic advertised for more.
/// </remarks>
public sealed class ArrivalsTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// <b>Anybody who can walk to a bed walks.</b> The coaches only ever go out
    /// for the people who genuinely need fetching, so a republic whose estates
    /// are near its border needs fewer of them.
    /// </summary>
    [Fact]
    public void A_party_within_walking_distance_walks_in()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var block = Built(world, "Apartment");
        var at = world.Buildings.IndexOf(block);

        world.Migration.Arrive(
            world.Buildings.XAt(at) + 50.0, world.Buildings.YAt(at), 4, world.Clock.DayIndex);
        Assert.Equal(4, world.Migration.HeadsWaiting());

        world.Tick();

        Assert.Equal(0, world.Migration.HeadsWaiting());
        Assert.Equal(4, world.Citizens.ResidentsOf(block));
    }

    /// <summary>
    /// A party too far to walk waits for a coach — and a republic with none loses
    /// them, which is what makes a coach and a road to the post part of the cost
    /// of growing.
    /// </summary>
    [Fact]
    public void A_party_too_far_to_walk_needs_fetching()
    {
        var world = World.Found(new WorldSpec(1961, 6000.0, 0), T);
        var block = Built(world, "Apartment");
        var at = world.Buildings.IndexOf(block);

        var far = world.Buildings.XAt(at) + (T.MaxWalkM * 3.0);
        world.Migration.Arrive(far, world.Buildings.YAt(at), 4, world.Clock.DayIndex);

        world.Tick();

        Assert.Equal(4, world.Migration.HeadsWaiting());
        Assert.Equal(0, world.Citizens.ResidentsOf(block));
    }

    /// <summary>
    /// A coach fetches them, and they are people in one place throughout: aboard
    /// the coach, then in the block, never in both and never lost between.
    /// </summary>
    [Fact]
    public void A_coach_fetches_a_party_and_houses_it()
    {
        var world = World.Found(new WorldSpec(1961, 6000.0, 0), T);
        var block = Built(world, "Apartment");
        var depot = Built(world, "BusDepot");
        var at = world.Buildings.IndexOf(block);
        var yard = world.Buildings.IndexOf(depot);

        var coach = Coach(world, yard);
        Assert.True(coach >= 0, "the depot's establishment holds no passenger vehicle");

        var far = world.Buildings.XAt(at) + (T.MaxWalkM * 2.0);
        world.Migration.Arrive(far, world.Buildings.YAt(at), 4, world.Clock.DayIndex);

        for (var i = 0; i < SimClock.TicksPerDay * 3 && world.Migration.HeadsWaiting() > 0; i++)
        {
            world.Tick();
        }

        Assert.Equal(0, world.Migration.HeadsWaiting());
        Assert.Equal(4, world.Citizens.ResidentsOf(block));
    }

    /// <summary>
    /// <b>A belt is a haul that needs no lorry, no driver and no diesel</b> — and
    /// it goes exactly where it was built and nowhere else. Nothing moves until
    /// both ends are on the same belt.
    /// </summary>
    [Fact]
    public void A_belt_moves_goods_between_things_standing_on_it()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var conveyor = T.UtilityIndex("Conveyor");
        var coal = T.ResourceIndex("Coal");

        var depot = Built(world, "Depot");
        var plant = Built(world, "PowerPlant");
        var yard = world.Buildings.IndexOf(depot);
        var works = world.Buildings.IndexOf(plant);

        world.Buildings.Stock.Add(yard, coal, 200.0);
        var held = world.Buildings.Stock.Get(works, coal);

        // Not on a belt: the coal stays where it is however hungry the plant is.
        world.Tick();
        Assert.Equal(held, world.Buildings.Stock.Get(works, coal));

        // Strung between them, and it moves without anybody driving.
        var span = new LineSite(
            1, conveyor,
            world.Buildings.XAt(yard), world.Buildings.YAt(yard),
            world.Buildings.XAt(works), world.Buildings.YAt(works), 0);
        world.Grid.Energise(span);
        world.Grid.AttachAll(depot, world.Buildings.XAt(yard), world.Buildings.YAt(yard));
        world.Grid.AttachAll(plant, world.Buildings.XAt(works), world.Buildings.YAt(works));
        Assert.True(world.Grid.Together(depot, plant, conveyor));

        world.Tick();
        Assert.True(
            world.Buildings.Stock.Get(works, coal) > held,
            "the belt carried nothing between two things standing on it");
        Assert.True(
            world.Buildings.Stock.Get(yard, coal) < 200.0,
            "the coal was conjured rather than carried");
    }

    /// <summary>
    /// <b>An empty republic is failing nobody, and must not be scored as if it
    /// were failing everybody.</b>
    /// </summary>
    /// <remarks>
    /// The average of what a republic offers its residents is, on a blank map, an
    /// average over an empty set — and calling that zero says "you are failing
    /// people you do not have", which leaves the map permanently uninhabited and
    /// the opening unable to start at all. It is the same rule contentment
    /// already applies twice: a republic is not marked down for a demand that
    /// does not exist.
    /// </remarks>
    [Fact]
    public void A_blank_map_with_housing_on_it_can_attract_its_first_settler()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        Built(world, "Apartment");

        Assert.Equal(0, world.Citizens.Count);

        for (var day = 0; day < 400 && world.Citizens.Count == 0
            && world.Migration.HeadsWaiting() == 0; day++)
        {
            for (var i = 0; i < SimClock.TicksPerDay; i++)
            {
                world.Tick();
            }
        }

        Assert.True(
            world.Citizens.Count > 0 || world.Migration.HeadsWaiting() > 0,
            "a blank map with a block standing empty on it drew nobody in a year");
    }

    /// <summary>
    /// <b>A site is served whatever the quantity.</b>
    /// </summary>
    /// <remarks>
    /// The minimum load exists to stop a lorry being sent for four kilograms of
    /// something a farm produces continuously. A site's outstanding bill is
    /// finite and terminal, so refusing to deliver it does not defer the trip —
    /// it cancels the building. The trajectory runner found the difference: a
    /// span wanting seven hundred kilograms of steel, two hundred tonnes of it
    /// standing at the customs house, six idle lorries, and a republic dark for
    /// three years because nothing would roll for less than a load.
    /// </remarks>
    [Fact]
    public void A_site_wanting_less_than_a_load_still_gets_its_delivery()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var steel = T.ResourceIndex("Steel");

        var depot = Built(world, "Depot");
        var garage = Built(world, "MotorDepot");
        world.Buildings.Stock.Add(world.Buildings.IndexOf(depot), steel, 200.0);
        world.Buildings.Stock.Add(
            world.Buildings.IndexOf(garage), T.ResourceIndex("Fuel"), 50.0);

        var at = world.Buildings.IndexOf(depot);
        var ordered = world.Issue(Command.OrderLine(
            T.UtilityIndex("Power"),
            world.Buildings.XAt(at), world.Buildings.YAt(at),
            world.Buildings.XAt(at) + T.MinLine + 1.0, world.Buildings.YAt(at)));
        Assert.True(ordered.Accepted, ordered.Refusal);

        var site = world.LineWorks.Get(ordered.Id);
        Assert.NotNull(site);
        Assert.True(
            site.Wants(steel, T) < T.MinLoad,
            "the shortest span the republic will survey still wants a full load");

        for (var day = 0; day < 30; day++)
        {
            for (var i = 0; i < SimClock.TicksPerDay; i++)
            {
                world.Tick();
            }

            if (world.LineWorks.Stock.Get(world.LineWorks.IndexOf(site.Id), steel) > 0.0)
            {
                return;
            }
        }

        Assert.Fail("no lorry ever rolled for a site wanting less than a load");
    }

    /// <summary>The first idle passenger vehicle in a garage, or -1.</summary>
    private static int Coach(World world, int garage)
    {
        for (var i = 0; i < SimClock.TicksPerDay; i++)
        {
            world.Tick();
            for (var v = 0; v < world.Fleet.Count; v++)
            {
                if (T.VRole[world.Fleet.KindAt(v)] == "Passenger"
                    && world.Fleet.HomeAt(v) == world.Buildings.IdAt(garage))
                {
                    return v;
                }
            }
        }

        return -1;
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
