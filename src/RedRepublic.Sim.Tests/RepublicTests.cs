namespace RedRepublic.Sim.Tests;

/// <summary>
/// A republic doing its job: people fed from a shop within reach, children
/// taught, and a firm billing for the work it does.
/// </summary>
/// <remarks>
/// These test what the systems are <i>for</i> rather than what they do
/// internally. A pass that computes the right numbers and never reaches a person
/// is a pass that has not happened.
/// </remarks>
public sealed class RepublicTests
{
    private static Tables T => Fixtures.Tables;

    private static World Found(ulong seed = 1961) =>
        World.Found(new WorldSpec(seed, 1500.0, 0), T);

    /// <summary>Somewhere this kind will stand, near a point if one can be found.</summary>
    private static (double X, double Y) Near(World world, int kind, double toX, double toY)
    {
        var best = (X: -1.0, Y: -1.0);
        var bestGap = double.PositiveInfinity;

        for (var y = 120.0; y < world.Terrain.Extent - 120.0; y += 40.0)
        {
            for (var x = 120.0; x < world.Terrain.Extent - 120.0; x += 40.0)
            {
                if (Commands.CanPlace(world, kind, x, y) is not null)
                {
                    continue;
                }

                var gap = Units.Distance(x, y, toX, toY);
                if (gap < bestGap)
                {
                    bestGap = gap;
                    best = (x, y);
                }
            }
        }

        Assert.True(best.X >= 0.0, $"nowhere will take a {T.BName[kind]}");
        return best;
    }

    private static int Raise(World world, string id, double nearX, double nearY)
    {
        var kind = T.BuildingIndex(id);
        var (x, y) = Near(world, kind, nearX, nearY);
        var placed = world.Issue(Command.ContractBuild(kind, x, y, Market.East));
        Assert.True(placed.Accepted, $"{id}: {placed.Refusal}");

        // Open it now: what is under test is what a standing republic does, not
        // how long a firm takes to build one.
        var b = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(b, T.BLabour[kind]);
        world.Buildings.SetContractor(b, -1);
        return b;
    }

    /// <summary>
    /// <b>Within reach is the whole mechanic.</b> A republic with a full warehouse
    /// and no shop near the housing is a republic whose people go hungry — goods
    /// have to be somewhere, and getting them somewhere is what lorries are for.
    /// </summary>
    [Fact]
    public void People_are_fed_from_a_shop_within_reach_and_not_from_one_beyond_it()
    {
        var world = Found();
        var food = T.ResourceIndex("Food");

        var home = Raise(world, "Apartment", 400.0, 400.0);
        var near = Raise(world, "Store", world.Buildings.XAt(home), world.Buildings.YAt(home));

        // Somebody to feed.
        for (var i = 0; i < 20; i++)
        {
            world.Citizens.AddArrival(world.Buildings.IdAt(home), 30);
        }

        Assert.True(
            Units.Distance(
                world.Buildings.XAt(near), world.Buildings.YAt(near),
                world.Buildings.XAt(home), world.Buildings.YAt(home)) <= T.ServiceRadius,
            "the store should have landed within reach of the housing");

        // A stocked shop feeds them. Clothes as well as food: both are wants,
        // so a shop with only food caps provisions at food's share of the two —
        // which is 0.79, and is the right answer rather than a bug.
        world.Buildings.Stock.Add(near, food, 500.0);
        world.Buildings.Stock.Add(near, T.ResourceIndex("Clothes"), 500.0);
        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        Assert.True(
            world.Buildings.ProvisionedAt(home) > 0.9,
            $"a stocked shop next door should feed them, got {world.Buildings.ProvisionedAt(home):F2}");
        Assert.True(world.Buildings.Stock.Get(near, food) < 500.0, "and the shelves should empty");

        // An empty shop does not, however much is in a warehouse elsewhere.
        world.Buildings.Stock.Set(near, food, 0.0);
        world.Buildings.Stock.Set(near, T.ResourceIndex("Clothes"), 0.0);
        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        Assert.Equal(0.0, world.Buildings.ProvisionedAt(home));
    }

    /// <summary>
    /// A shop out of reach is a shop that is not there. This is the failure a
    /// republic actually hits — the goods exist, and they are in the wrong place.
    /// </summary>
    [Fact]
    public void A_shop_beyond_reach_might_as_well_not_exist()
    {
        var world = Found();
        var food = T.ResourceIndex("Food");

        var home = Raise(world, "Apartment", 250.0, 250.0);

        // As far away as this map allows, and further than a person will go.
        var far = Raise(world, "Store", world.Terrain.Extent - 200.0, world.Terrain.Extent - 200.0);
        var gap = Units.Distance(
            world.Buildings.XAt(far), world.Buildings.YAt(far),
            world.Buildings.XAt(home), world.Buildings.YAt(home));
        Assert.True(gap > T.ServiceRadius, $"the store is {gap:F0} m away, reach is {T.ServiceRadius}");

        world.Buildings.Stock.Add(far, food, 500.0);
        for (var i = 0; i < 20; i++)
        {
            world.Citizens.AddArrival(world.Buildings.IdAt(home), 30);
        }

        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        Assert.Equal(0.0, world.Buildings.ProvisionedAt(home));
        Assert.Equal(500.0, world.Buildings.Stock.Get(far, food));
    }

    /// <summary>
    /// <b>A comfort is a way to do better, not a way to fail.</b> Drink and
    /// electronics lift a home's score and never join the wants — a republic
    /// that stocks them is doing better, and one that does not is not failing.
    /// </summary>
    [Fact]
    public void Comforts_reach_people_separately_from_what_they_need()
    {
        var world = Found();
        var home = Raise(world, "Apartment", 400.0, 400.0);
        var shop = Raise(world, "Store", world.Buildings.XAt(home), world.Buildings.YAt(home));

        for (var i = 0; i < 20; i++)
        {
            world.Citizens.AddArrival(world.Buildings.IdAt(home), 30);
        }

        world.Buildings.Stock.Add(shop, T.ResourceIndex("Food"), 500.0);
        world.Buildings.Stock.Add(shop, T.ResourceIndex("Clothes"), 500.0);

        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        // Fed and clothed, with no comforts at all.
        Assert.True(world.Buildings.ProvisionedAt(home) > 0.9);
        Assert.Equal(0.0, world.Buildings.ComfortedAt(home));
        Assert.Equal(0.0, world.Buildings.DrinkAt(home));

        world.Buildings.Stock.Add(shop, T.ResourceIndex("Alcohol"), 500.0);
        world.Buildings.Stock.Add(shop, T.ResourceIndex("Electronics"), 500.0);

        for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
        {
            world.Tick();
        }

        Assert.True(world.Buildings.ComfortedAt(home) > 0.9);
        Assert.True(world.Buildings.DrinkAt(home) > 0.9);

        // And the wants are unchanged by it — comforts never re-mark the work
        // the republic already did.
        Assert.True(world.Buildings.ProvisionedAt(home) > 0.9);
    }

    /// <summary>
    /// A contracted firm bills the treasury every day it is on site, and stops
    /// the day the building opens. That is what makes contracting a decision
    /// rather than free building.
    /// </summary>
    [Fact]
    public void A_firm_bills_while_it_builds_and_stops_when_it_is_done()
    {
        var world = Found();
        world.Treasury.Add(Market.East, 1_000_000.0);
        var before = world.Treasury.Of(Market.East);

        var kind = T.BuildingIndex("Sawmill");
        var (x, y) = Near(world, kind, 400.0, 400.0);
        var placed = world.Issue(Command.ContractBuild(kind, x, y, Market.East));
        Assert.True(placed.Accepted);
        var b = world.Buildings.IndexOf(placed.Id);

        for (var tick = 0; tick < SimClock.TicksPerDay * 3; tick++)
        {
            world.Tick();
        }

        var spent = before - world.Treasury.Of(Market.East);
        Assert.True(spent > 0.0, "a firm on site should be billing");
        Assert.Equal(T.ContractorDays * T.ContractorRate * 3.0, spent, 6);

        // Finished, and the billing stops.
        world.Buildings.AddWork(b, T.BLabour[kind]);
        var afterBuilt = world.Treasury.Of(Market.East);

        for (var tick = 0; tick < SimClock.TicksPerDay * 3; tick++)
        {
            world.Tick();
        }

        Assert.Equal(afterBuilt, world.Treasury.Of(Market.East));
    }

    /// <summary>
    /// A republic that never builds a school raises a generation that cannot run
    /// its own mines. Attendance is what makes the building worth putting up.
    /// </summary>
    [Fact]
    public void Children_are_taught_only_where_there_is_a_school()
    {
        var world = Found();
        var home = Raise(world, "Apartment", 400.0, 400.0);
        var homeId = world.Buildings.IdAt(home);

        var child = world.Citizens.Add(homeId, 7, 0, 1.0, 0.6);
        Assert.Equal(Education.Unschooled, world.Citizens.EducationAt(child));

        // Teachers. A school is a workplace like any other: the labour pass
        // staffs it out of the people living in reach, and a school nobody works
        // at teaches nobody — which is the rule, not an oversight.
        for (var i = 0; i < 15; i++)
        {
            world.Citizens.AddArrival(homeId, 30);
        }

        // No school: a fortnight passes and nothing is learnt.
        for (var tick = 0; tick < SimClock.TicksPerDay * 14; tick++)
        {
            world.Tick();
        }

        Assert.Equal(0, world.Citizens.SchoolDaysAt(child));

        // A school within reach, staffed by the republic's own labour pass.
        var school = Raise(world, "School", world.Buildings.XAt(home), world.Buildings.YAt(home));

        for (var tick = 0; tick < SimClock.TicksPerDay * 10; tick++)
        {
            world.Tick();
        }

        Assert.True(
            world.Buildings.StaffAt(school) > 0,
            "the labour pass should have staffed the school");
        Assert.True(
            world.Citizens.SchoolDaysAt(child) > 0,
            "a staffed school within reach should be teaching");
    }

    /// <summary>
    /// The whole determinism claim, over a month with people in it: the same
    /// seed produces the same republic, down to who is alive and what is in the
    /// bins.
    /// </summary>
    [Fact]
    public void A_month_of_republic_replays_exactly()
    {
        static (int People, double Provisioned, double Purse, double Snow, int Schooled) RunAMonth()
        {
            var world = World.Found(new WorldSpec(1961, 1500.0, 1), Fixtures.Tables);
            var t = Fixtures.Tables;

            var kind = t.BuildingIndex("Apartment");
            var (x, y) = (0.0, 0.0);
            for (var py = 120.0; py < world.Terrain.Extent - 120.0 && x == 0.0; py += 40.0)
            {
                for (var px = 120.0; px < world.Terrain.Extent - 120.0; px += 40.0)
                {
                    if (Commands.CanPlace(world, kind, px, py) is null)
                    {
                        (x, y) = (px, py);
                        break;
                    }
                }
            }

            var placed = world.Issue(Command.ContractBuild(kind, x, y, Market.East));
            var home = world.Buildings.IndexOf(placed.Id);
            world.Buildings.AddWork(home, t.BLabour[kind]);
            world.Buildings.SetContractor(home, -1);

            for (var i = 0; i < 30; i++)
            {
                world.Citizens.AddArrival(world.Buildings.IdAt(home), 20 + i);
            }

            for (var tick = 0; tick < SimClock.TicksPerDay * 30; tick++)
            {
                world.Tick();
            }

            return (
                world.Citizens.Count,
                world.Buildings.ProvisionedAt(home),
                world.Treasury.Of(Market.East),
                world.Ground.Snow,
                world.Citizens.ByEducation(Education.Schooled));
        }

        Assert.Equal(RunAMonth(), RunAMonth());
    }
}
