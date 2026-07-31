namespace RedRepublic.Sim.Tests;

/// <summary>
/// Roads: ordered, materialled, built, and only then drivable.
/// </summary>
/// <remarks>
/// A road was the last free thing in the republic — somebody called connect and
/// roads appeared. It is one of the largest investments a republic makes, and it
/// should cost gravel a quarry dug, lorries that drove it out, and builder-days
/// from the same crew that is not building something else meanwhile.
/// </remarks>
public sealed class RoadworksTests
{
    private static Tables T => Fixtures.Tables;

    private static int Gravel => Array.FindIndex(T.Grades, g => g.Id == "Gravel");

    /// <summary>
    /// <b>A site is deliberately not in the network.</b> Nothing routes over it
    /// and no lorry is quicker for it existing, which is what makes the moment a
    /// road opens a real event.
    /// </summary>
    [Fact]
    public void An_ordered_way_is_not_a_way_until_it_is_built()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var before = world.Roads.SegmentCount;

        var (x, y) = DryRun(world, 800.0);
        var ordered = world.Issue(Command.OrderRoad(x, y, x + 800.0, y, Gravel, false));
        Assert.True(ordered.Accepted, ordered.Refusal);

        Assert.Single(world.RoadWorks.Sites);
        Assert.Equal(before, world.Roads.SegmentCount);
        Assert.Empty(world.Roadbook);
    }

    /// <summary>
    /// <b>Junctions along the length, not only at the ends.</b> Access to the
    /// network is measured from junctions, so a long road with junctions only at
    /// its ends would serve the two buildings at those ends and nothing between.
    /// </summary>
    [Fact]
    public void A_finished_way_is_laid_with_junctions_along_its_length()
    {
        var network = new Network();
        var site = new RoadSite(1, 0.0, 0.0, 1000.0, 0.0, Gravel, false, 0);

        RoadWorks.Open(network, site, T);

        var expected = (int)Math.Ceiling(1000.0 / T.JunctionSpacing);
        Assert.Equal(expected, network.SegmentCount);
        Assert.Equal(expected + 1, network.NodeCount);
        Assert.Equal(1000.0, network.TotalLength(), 9);
    }

    /// <summary>
    /// Two runs ordered end to end are one network, not two islands — the ends
    /// merge onto whatever junction already stands there.
    /// </summary>
    [Fact]
    public void Two_runs_laid_end_to_end_are_one_network()
    {
        var network = new Network();
        RoadWorks.Open(network, new RoadSite(1, 0.0, 0.0, 400.0, 0.0, Gravel, false, 0), T);
        RoadWorks.Open(network, new RoadSite(2, 400.0, 0.0, 800.0, 0.0, Gravel, false, 0), T);

        var west = network.NearestNode(0.0, 0.0, T.JunctionMerge);
        var east = network.NearestNode(800.0, 0.0, T.JunctionMerge);
        Assert.True(west >= 0 && east >= 0);
        Assert.NotNull(network.RouteBetween(west, east));
    }

    /// <summary>
    /// The whole loop, because each half can be right while the join is not: a
    /// site nothing delivers to is never built, and a site that finishes without
    /// joining the network is gravel nobody can drive on.
    /// </summary>
    [Fact]
    public void A_way_is_ordered_materialled_and_only_then_driven_on()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        var office = Built(world, "ConstructionOffice");
        var block = Built(world, "Apartment");
        for (var i = 0; i < 40; i++)
        {
            world.Citizens.AddArrival(block, 30);
        }

        var at = world.Buildings.IndexOf(office);
        var x = world.Buildings.XAt(at);
        var y = world.Buildings.YAt(at);
        var ordered = world.Issue(Command.OrderRoad(x, y, x + 300.0, y, Gravel, false));
        Assert.True(ordered.Accepted, ordered.Refusal);

        var site = world.RoadWorks.Get(ordered.Id);
        Assert.NotNull(site);

        // Nothing is built without the gravel, however long the republic waits.
        for (var i = 0; i < SimClock.TicksPerDay; i++)
        {
            world.Tick();
        }

        Assert.Equal(0.0, site.WorkDone);

        var stock = world.RoadWorks.IndexOf(site.Id);
        for (var r = 0; r < T.Resources.Length; r++)
        {
            world.RoadWorks.Stock.Add(stock, r, site.Wants(r, T));
        }

        for (var day = 0; day < 90 && world.RoadWorks.Sites.Count > 0; day++)
        {
            for (var i = 0; i < SimClock.TicksPerDay; i++)
            {
                world.Tick();
            }
        }

        Assert.Empty(world.RoadWorks.Sites);
        Assert.Single(world.Roadbook);
        Assert.True(world.Roads.SegmentCount > 0, "the finished run never joined the network");
    }

    /// <summary>
    /// A republic's roads survive a save. The network is rebuilt by replaying the
    /// openings rather than stored, because junction merging depends on what
    /// already stood there.
    /// </summary>
    [Fact]
    public void The_roads_come_back_from_a_save()
    {
        var world = World.Found(new WorldSpec(1961, 1500.0, 0), T);
        world.Reopen(new RoadSite(1, 200.0, 200.0, 900.0, 200.0, Gravel, false, 0));
        world.Reopen(new RoadSite(2, 900.0, 200.0, 900.0, 800.0, Gravel, true, 0));

        var back = Save.Read(Save.Write(world), T);

        Assert.Equal(world.Roads.SegmentCount, back.Roads.SegmentCount);
        Assert.Equal(world.Roads.NodeCount, back.Roads.NodeCount);
        Assert.Equal(world.Roads.TotalLength(), back.Roads.TotalLength(), 9);
        Assert.Equal(world.Roads.LitLength().WithLamps, back.Roads.LitLength().WithLamps, 9);
    }

    /// <summary>
    /// Somewhere a run of this length stays out of the water.
    /// </summary>
    /// <remarks>
    /// A generated map is not a car park: a line picked by eye crosses a river as
    /// often as not, and a test that assumed otherwise would be asserting about
    /// the seed rather than about the rule under test.
    /// </remarks>
    private static (double X, double Y) DryRun(World world, double length)
    {
        for (var y = 200.0; y < world.Terrain.Extent - 200.0; y += 50.0)
        {
            for (var x = 200.0; x + length < world.Terrain.Extent - 200.0; x += 50.0)
            {
                if (!world.Terrain.CrossesWater(x, y, x + length, y))
                {
                    return (x, y);
                }
            }
        }

        throw new InvalidOperationException("no dry run of that length on this map");
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
