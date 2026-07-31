namespace RedRepublic.Sim.Tests;

/// <summary>Roads, rails and water, as one graph type in three instances.</summary>
public sealed class NetworkTests
{
    private static Tables T => Fixtures.Tables;

    private static double Road => Units.KphToMps(T.DefaultRoadKph);

    [Fact]
    public void A_segment_joins_two_junctions_both_ways()
    {
        var n = new Network();
        var a = n.AddNode(0.0, 0.0);
        var b = n.AddNode(300.0, 400.0);
        n.Connect(a, b, Road);

        Assert.Equal(2, n.NodeCount);
        Assert.Equal(1, n.SegmentCount);
        Assert.Equal(500.0, n.Segments[0].Length);
        Assert.True(n.AreConnected(a, b));
        Assert.True(n.AreConnected(b, a));
        Assert.Equal(500.0, n.TotalLength());
        Assert.Equal(150.0, n.MidpointX(0));
        Assert.Equal(200.0, n.MidpointY(0));

        Assert.Throws<ArgumentException>(() => n.Connect(a, a, Road));
    }

    /// <summary>
    /// Routing is by <b>travel time</b>, not distance — which is what makes a
    /// paved detour beat a direct track, and what makes a road worth upgrading.
    /// </summary>
    [Fact]
    public void The_fastest_route_wins_not_the_shortest()
    {
        var n = new Network();
        var start = n.AddNode(0.0, 0.0);
        var end = n.AddNode(2000.0, 0.0);
        var detour = n.AddNode(1000.0, 900.0);

        // Straight but slow.
        n.Connect(start, end, Units.KphToMps(10.0));

        // Longer but quick.
        n.Connect(start, detour, Units.KphToMps(80.0));
        n.Connect(detour, end, Units.KphToMps(80.0));

        var route = n.RouteWhere(start, end, static _ => true);
        Assert.NotNull(route);
        Assert.Equal([start, detour, end], route.Nodes);
        Assert.True(route.Distance > 2000.0);
        Assert.True(route.Time < Units.TimeToCover(Units.KphToMps(10.0), 2000.0));
    }

    /// <summary>
    /// <b>The tie-break is load-bearing.</b> Two routes of exactly equal time are
    /// common on a grid of roads, and a frontier left to break the tie however it
    /// happens to would answer one way today and another after an unrelated edit.
    /// The same republic must behave the same way twice.
    /// </summary>
    [Fact]
    public void Equal_time_routes_resolve_the_same_way_every_run()
    {
        // A perfect grid, where every route across a square costs the same.
        static Network Grid()
        {
            var n = new Network();
            const int side = 6;
            var node = new int[side, side];
            for (var y = 0; y < side; y++)
            {
                for (var x = 0; x < side; x++)
                {
                    node[x, y] = n.AddNode(x * 500.0, y * 500.0);
                }
            }

            for (var y = 0; y < side; y++)
            {
                for (var x = 0; x < side; x++)
                {
                    if (x + 1 < side)
                    {
                        n.Connect(node[x, y], node[x + 1, y], Units.KphToMps(50.0));
                    }

                    if (y + 1 < side)
                    {
                        n.Connect(node[x, y], node[x, y + 1], Units.KphToMps(50.0));
                    }
                }
            }

            return n;
        }

        var first = Grid().RouteWhere(0, 35, static _ => true);
        var second = Grid().RouteWhere(0, 35, static _ => true);

        Assert.NotNull(first);
        Assert.NotNull(second);
        Assert.Equal(first.Nodes, second.Nodes);
        Assert.Equal(first.Time, second.Time);
        Assert.Equal(first.Distance, second.Distance);
    }

    [Fact]
    public void A_route_to_nowhere_is_no_route()
    {
        var n = new Network();
        var a = n.AddNode(0.0, 0.0);
        var b = n.AddNode(100.0, 0.0);
        var island = n.AddNode(5000.0, 5000.0);
        n.Connect(a, b, Road);

        Assert.Null(n.RouteWhere(a, island, static _ => true));
        Assert.Null(n.RouteWhere(a, 99, static _ => true));

        // Somewhere is always reachable from itself, at no cost.
        var here = n.RouteWhere(a, a, static _ => true);
        Assert.NotNull(here);
        Assert.Equal(0.0, here.Time);
        Assert.Equal([a], here.Nodes);
    }

    /// <summary>
    /// A filter is what keeps a train off a road: the caller says which segments
    /// it will accept, and the router never sees the rest.
    /// </summary>
    [Fact]
    public void A_router_only_sees_the_segments_it_will_accept()
    {
        var n = new Network();
        var a = n.AddNode(0.0, 0.0);
        var b = n.AddNode(1000.0, 0.0);
        var c = n.AddNode(2000.0, 0.0);
        n.Connect(a, b, Road, lamps: true);
        n.Connect(b, c, Road);

        Assert.NotNull(n.RouteWhere(a, c, static _ => true));

        // Only lamp-lit stretches: the far leg is refused, so there is no way on.
        Assert.Null(n.RouteWhere(a, c, static s => s.Lamps));
        Assert.NotNull(n.RouteWhere(a, b, static s => s.Lamps));
    }

    /// <summary>
    /// Lamps are a thing you paid for; whether they are burning tonight is a
    /// question for the grid. A republic short of generation puts its streets
    /// out along with everything else.
    /// </summary>
    [Fact]
    public void Lamps_are_built_and_lighting_is_supplied()
    {
        var n = new Network();
        var a = n.AddNode(0.0, 0.0);
        var b = n.AddNode(1000.0, 0.0);
        var c = n.AddNode(2000.0, 0.0);
        n.Connect(a, b, Road, lamps: true);
        n.Connect(b, c, Road);

        var (withLamps, burning) = n.LitLength();
        Assert.Equal(1000.0, withLamps);
        Assert.Equal(0.0, burning);
        Assert.False(n.Segments[0].IsLit);

        n.SetAlight(0, true);
        Assert.True(n.Segments[0].IsLit);
        Assert.Equal(1000.0, n.LitLength().Burning);

        // An unlit stretch cannot be lit by the grid: there are no lamps on it.
        n.SetAlight(1, true);
        Assert.False(n.Segments[1].IsLit);
        Assert.Equal(1000.0, n.LitLength().Burning);
    }

    /// <summary>
    /// Two roads ordered to the same place meet rather than crossing without
    /// touching — which is the difference between a network and a drawing.
    /// </summary>
    [Fact]
    public void Roads_ordered_to_the_same_place_share_a_junction()
    {
        var n = new Network();
        var first = n.JunctionAt(500.0, 500.0, 20.0);
        var again = n.JunctionAt(505.0, 500.0, 20.0);
        var elsewhere = n.JunctionAt(900.0, 500.0, 20.0);

        Assert.Equal(first, again);
        Assert.NotEqual(first, elsewhere);
        Assert.Equal(2, n.NodeCount);

        Assert.Equal(first, n.NearestNode(502.0, 500.0, 50.0));
        Assert.Equal(-1, n.NearestNode(5000.0, 5000.0, 50.0));
    }

    /// <summary>
    /// Nobody digs a river: the waterway is sampled off the terrain, and only
    /// where the water is wide enough to turn a barge in. A stream a metre across
    /// is water on the map and is not a waterway.
    /// </summary>
    [Fact]
    public void A_fairway_is_only_laid_where_a_barge_would_fit()
    {
        var terrain = Terrain.Flat(1000.0, 10.0);

        // Dry land carries no fairway at all.
        Assert.Equal(0, Network.Navigable(terrain, T).NodeCount);

        // A broad lake does.
        for (var y = 200.0; y < 800.0; y += 10.0)
        {
            for (var x = 200.0; x < 800.0; x += 10.0)
            {
                terrain.SetSurfaceAt(x, y, Surface.Water);
            }
        }

        var water = Network.Navigable(terrain, T);
        Assert.True(water.NodeCount > 0, "a 600 m lake should carry a fairway");
        Assert.True(water.SegmentCount > 0, "and the fairway should join up");

        // A one-cell channel is not navigable, however long it is.
        var narrow = Terrain.Flat(1000.0, 10.0);
        for (var y = 0.0; y < 1000.0; y += 10.0)
        {
            narrow.SetSurfaceAt(500.0, y, Surface.Water);
        }

        Assert.Equal(0, Network.Navigable(narrow, T).NodeCount);
    }
}
