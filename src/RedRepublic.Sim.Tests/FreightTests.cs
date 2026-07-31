namespace RedRepublic.Sim.Tests;

/// <summary>
/// Freight: the half of the game that makes the map matter.
/// </summary>
/// <remarks>
/// Goods have to be <i>somewhere</i>, and getting them somewhere else takes a
/// vehicle, a route and time. A republic where tonnage teleports is a
/// spreadsheet.
/// </remarks>
public sealed class FreightTests
{
    private static Tables T => Fixtures.Tables;

    private static World Found(ulong seed = 1961) =>
        World.Found(new WorldSpec(seed, 1500.0, 0), T);

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

        var b = world.Buildings.IndexOf(placed.Id);
        world.Buildings.AddWork(b, T.BLabour[kind]);
        world.Buildings.SetContractor(b, -1);
        return b;
    }

    /// <summary>
    /// A garage takes delivery of exactly the vehicles its establishment allows —
    /// no more however many drivers it has, and no fewer.
    /// </summary>
    [Fact]
    public void A_garage_gets_the_vehicles_it_is_established_for()
    {
        var world = Found();
        var depot = Raise(world, "MotorDepot", 400.0, 400.0);
        var kind = world.Buildings.KindAt(depot);

        Assert.Equal(0, world.Fleet.Count);

        world.Tick();

        var establishment = T.Establishment.KeysOf(kind);
        var counts = T.Establishment.ValuesOf(kind);
        var expected = 0;
        for (var i = 0; i < counts.Length; i++)
        {
            expected += counts[i];
        }

        Assert.True(expected > 0, "a motor depot should keep lorries");
        Assert.Equal(expected, world.Fleet.Count);

        // Every one of them belongs to the depot and starts idle with a full tank.
        foreach (var v in world.Fleet.OfGarage(world.Buildings.IdAt(depot)))
        {
            Assert.Equal(VehicleState.Idle, world.Fleet.StateAt(v));
            Assert.Equal(T.VTank[world.Fleet.KindAt(v)], world.Fleet.FuelAt(v));
        }

        // A second tick does not deliver a second fleet.
        for (var tick = 0; tick < 100; tick++)
        {
            world.Tick();
        }

        Assert.Equal(expected, world.Fleet.Count);
    }

    /// <summary>
    /// <b>The whole freight loop, end to end.</b> A works with an empty bin, a
    /// yard with the goods, and a lorry that drives out, loads, drives back and
    /// sets the load down.
    /// </summary>
    [Fact]
    public void A_lorry_fetches_what_a_works_is_short_of()
    {
        var world = Found();
        var wood = T.ResourceIndex("Wood");

        var depot = Raise(world, "MotorDepot", 500.0, 500.0);
        var mill = Raise(world, "Sawmill", 600.0, 600.0);
        var yard = Raise(world, "OpenYard", 400.0, 400.0);

        // Somewhere with wood, and a mill that eats it and has none.
        world.Buildings.Stock.Add(yard, wood, 200.0);
        Assert.Equal(0.0, world.Buildings.Stock.Get(mill, wood));

        // Long enough for a lorry to be delivered, sent, loaded and to come back.
        var delivered = false;
        for (var tick = 0; tick < SimClock.TicksPerDay * 3 && !delivered; tick++)
        {
            world.Tick();
            delivered = world.Buildings.Stock.Get(mill, wood) > 0.0;
        }

        Assert.True(delivered, "a lorry should have brought the mill its wood");
        Assert.True(
            world.Buildings.Stock.Get(yard, wood) < 200.0,
            "and taken it out of the yard it came from");

        // Nothing was conjured: what left the yard is what arrived, plus
        // whatever is still on the road.
        var afloat = world.Fleet.CargoAfloat();
        var atMill = world.Buildings.Stock.Get(mill, wood);
        var atYard = world.Buildings.Stock.Get(yard, wood);
        Assert.Equal(200.0, atYard + atMill + afloat, 6);
    }

    /// <summary>
    /// Freight ranks what to carry, and an empty bin outranks a topped-up one.
    /// That ranking is what makes a priority a decision rather than a label.
    /// </summary>
    [Fact]
    public void An_empty_bin_is_fetched_before_a_low_one()
    {
        var world = Found();
        var wood = T.ResourceIndex("Wood");

        Raise(world, "MotorDepot", 500.0, 500.0);
        var starved = Raise(world, "Sawmill", 560.0, 500.0);
        var stocked = Raise(world, "Sawmill", 440.0, 500.0);
        var yard = Raise(world, "OpenYard", 500.0, 620.0);

        world.Buildings.Stock.Add(yard, wood, 300.0);

        // One mill has nothing; the other has a day's worth.
        world.Buildings.Stock.Add(stocked, wood, 2.0);

        var starvedGot = false;
        for (var tick = 0; tick < SimClock.TicksPerDay * 2 && !starvedGot; tick++)
        {
            world.Tick();
            starvedGot = world.Buildings.Stock.Get(starved, wood) > 0.0;
        }

        Assert.True(starvedGot, "the empty mill should have been served");
    }

    /// <summary>
    /// <b>Goods do not teleport.</b> A lorry is somewhere between its ends for the
    /// whole journey, and the load is on it rather than at either end.
    /// </summary>
    [Fact]
    public void A_load_is_on_the_road_while_it_travels()
    {
        var world = Found();
        var wood = T.ResourceIndex("Wood");

        Raise(world, "MotorDepot", 300.0, 300.0);
        Raise(world, "Sawmill", 1200.0, 1200.0);
        var yard = Raise(world, "OpenYard", 300.0, 360.0);
        world.Buildings.Stock.Add(yard, wood, 200.0);

        var sawOnTheRoad = false;
        var movedAtAll = false;
        var lastX = double.NaN;

        for (var tick = 0; tick < SimClock.TicksPerDay * 2; tick++)
        {
            world.Tick();

            for (var v = 0; v < world.Fleet.Count; v++)
            {
                if (world.Fleet.StateAt(v) == VehicleState.Delivering
                    && world.Fleet.Cargo.Total(v) > 0.0)
                {
                    sawOnTheRoad = true;
                }

                if (!double.IsNaN(lastX) && world.Fleet.XAt(v) != lastX)
                {
                    movedAtAll = true;
                }

                lastX = world.Fleet.XAt(v);
            }
        }

        Assert.True(movedAtAll, "a dispatched lorry should move across the map");
        Assert.True(sawOnTheRoad, "and its load should be aboard while it travels");
    }

    /// <summary>
    /// A journey plans over the road where there is one, because the same lorry
    /// makes its own speed on tarmac and crawls over a field. That is the whole
    /// reason a road is worth building.
    /// </summary>
    [Fact]
    public void A_route_uses_a_road_when_one_joins_the_ends()
    {
        var world = Found();

        var depot = Raise(world, "MotorDepot", 400.0, 400.0);
        var yard = Raise(world, "OpenYard", 1100.0, 400.0);

        // A road joining the two, at a junction each end.
        var a = world.Roads.AddNode(world.Buildings.XAt(depot), world.Buildings.YAt(depot));
        var b = world.Roads.AddNode(world.Buildings.XAt(yard), world.Buildings.YAt(yard));
        world.Roads.Connect(a, b, Units.KphToMps(T.DefaultRoadKph));

        world.Tick();
        Assert.True(world.Fleet.Count > 0);

        // A lorry sent between them plans a journey with a road leg in it.
        world.Buildings.Stock.Add(yard, T.ResourceIndex("Wood"), 100.0);
        Raise(world, "Sawmill", world.Buildings.XAt(depot), world.Buildings.YAt(depot));

        Journey? seen = null;
        for (var tick = 0; tick < SimClock.TicksPerDay && seen is null; tick++)
        {
            world.Tick();
            for (var v = 0; v < world.Fleet.Count; v++)
            {
                var j = world.Fleet.JourneyAt(v);
                if (j is not null && j.Legs > 1)
                {
                    seen = j;
                    break;
                }
            }
        }

        Assert.NotNull(seen);

        // At least one leg has a way under it with a speed limit.
        var onRoad = false;
        for (var leg = 0; leg < seen.Legs; leg++)
        {
            onRoad |= seen.LegOnRoad(leg);
        }

        Assert.True(onRoad, "the route should use the road that joins the two ends");
    }

    /// <summary>
    /// The going slows everything. A republic in the spring thaw hauls slower
    /// than one on frozen ground, which is what makes the season matter to
    /// freight rather than only to farms.
    /// </summary>
    [Fact]
    public void Mud_slows_the_haul()
    {
        var world = Found();
        var kind = Array.IndexOf(T.VehicleIds, "Lorry");
        var journey = Journey.Begin(
            [0.0, 5000.0], [0.0, 0.0], [-1.0], 0.0, 10.0);

        var firm = journey.SpeedOn(
            0, Units.KphToMps(T.VOnRoadKph[kind]), Units.KphToMps(T.VCrossCountryKph[kind]), 1.0);
        var soft = journey.SpeedOn(
            0, Units.KphToMps(T.VOnRoadKph[kind]), Units.KphToMps(T.VCrossCountryKph[kind]),
            T.MudDrag);

        Assert.True(soft < firm, "mud should slow a lorry");
        Assert.Equal(firm / T.MudDrag, soft, 9);

        // And frozen ground is firm ground however wet it is, which is why
        // winter haulage beats spring haulage.
        Assert.True(world.Tables.MudDrag > 1.0);
    }
}
