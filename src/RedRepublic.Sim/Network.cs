namespace RedRepublic.Sim;

/// <summary>A stretch of way between two junctions.</summary>
/// <param name="From">The junction it leaves.</param>
/// <param name="To">The junction it reaches.</param>
/// <param name="Length">Metres.</param>
/// <param name="Speed">Metres per second the way itself allows.</param>
/// <param name="Lamps">
/// Whether this stretch was built with street lighting. Authored when the road
/// is ordered and never changed after, which is what separates it from
/// <paramref name="Alight"/>: lamps are a thing you paid for, and whether they
/// are burning tonight is a question for the grid.
/// </param>
/// <param name="Alight">
/// Whether those lamps are on. Set by the power system, never authored. A lit
/// road is a real consumer: it draws through a transformer station in reach
/// exactly as a building does, and a republic short of generation puts its
/// streets out along with everything else. That is the point — lighting the
/// town is an infrastructure project rather than a checkbox.
/// </param>
public readonly record struct Segment(
    int From,
    int To,
    double Length,
    double Speed,
    bool Lamps,
    bool Alight)
{
    public double TravelTime => Units.TimeToCover(Speed, Length);

    /// <summary>Whether somebody could walk this in the dark: lamps that are burning.</summary>
    public bool IsLit => Lamps && Alight;
}

/// <summary>A route through the network.</summary>
public sealed record Route(IReadOnlyList<int> Nodes, double Distance, double Time);

/// <summary>
/// A way through the republic, as a graph. Roads, rails and navigable water.
/// </summary>
/// <remarks>
/// <para>
/// <b>One graph type, three networks.</b> A railway is the same object as a
/// road: junctions, two-way spans between them, a speed limit per span, and
/// shortest route by travel time. So is navigable water. They differ in what
/// they cost, what may ride them and — for water — in not being built at all,
/// none of which is a property of the graph. What keeps them apart is not three
/// types but three <i>instances</i>: a train routes over the rails and finds no
/// road there, because the road is in a different network.
/// </para>
/// <para>
/// <b>A graph and not a grid.</b> The archived build routed by flooding a tile
/// grid, which was free at its map sizes. At metric scale a 10 km republic is a
/// million cells and freight runs many queries a simulated day, so the same
/// approach would spend the whole frame budget pathfinding empty ground. A
/// network is a few thousand junctions, and its size tracks what the player has
/// actually built.
/// </para>
/// </remarks>
public sealed class Network
{
    private readonly List<double> _nodeX = [];
    private readonly List<double> _nodeY = [];
    private readonly List<Segment> _segments = [];

    /// <summary>Neighbours of each node, as (node, segment) pairs.</summary>
    private readonly List<List<(int Node, int Segment)>> _adjacency = [];

    public int NodeCount => _nodeX.Count;

    public int SegmentCount => _segments.Count;

    public IReadOnlyList<Segment> Segments => _segments;

    public double NodeX(int node) => _nodeX[node];

    public double NodeY(int node) => _nodeY[node];

    public int AddNode(double x, double y)
    {
        _nodeX.Add(x);
        _nodeY.Add(y);
        _adjacency.Add([]);
        return _nodeX.Count - 1;
    }

    /// <summary>
    /// Lay a way between two junctions. Two-way: freight and people use it in
    /// both directions, and a one-way system is not a thing the republic models.
    /// </summary>
    public int Connect(int a, int b, double speedMps, bool lamps = false)
    {
        if (a == b)
        {
            throw new ArgumentException("a segment needs two different junctions", nameof(b));
        }

        var index = _segments.Count;
        _segments.Add(new Segment(
            a,
            b,
            Units.Distance(_nodeX[a], _nodeY[a], _nodeX[b], _nodeY[b]),
            speedMps,
            lamps,
            false));
        _adjacency[a].Add((b, index));
        _adjacency[b].Add((a, index));
        return index;
    }

    public void SetAlight(int segment, bool alight) =>
        _segments[segment] = _segments[segment] with { Alight = alight };

    public double MidpointX(int segment) =>
        (_nodeX[_segments[segment].From] + _nodeX[_segments[segment].To]) / 2.0;

    public double MidpointY(int segment) =>
        (_nodeY[_segments[segment].From] + _nodeY[_segments[segment].To]) / 2.0;

    public double TotalLength()
    {
        var total = 0.0;
        foreach (var s in _segments)
        {
            total += s.Length;
        }

        return total;
    }

    /// <summary>How much way carries lamps, and how much of that is burning.</summary>
    public (double WithLamps, double Burning) LitLength()
    {
        var withLamps = 0.0;
        var burning = 0.0;
        foreach (var s in _segments)
        {
            if (s.Lamps)
            {
                withLamps += s.Length;
                if (s.Alight)
                {
                    burning += s.Length;
                }
            }
        }

        return (withLamps, burning);
    }

    /// <summary>
    /// The junction at a point, reusing one already within <paramref name="merge"/>
    /// so that two roads ordered to the same place meet rather than crossing
    /// without touching.
    /// </summary>
    public int JunctionAt(double x, double y, double merge)
    {
        var near = NearestNode(x, y, merge);
        return near >= 0 ? near : AddNode(x, y);
    }

    public bool AreConnected(int a, int b)
    {
        foreach (var (node, _) in _adjacency[a])
        {
            if (node == b)
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>The nearest junction to a point, or -1 if none is within reach.</summary>
    public int NearestNode(double x, double y, double within)
    {
        var best = -1;
        var bestGap = within;
        for (var i = 0; i < _nodeX.Count; i++)
        {
            var gap = Units.Distance(_nodeX[i], _nodeY[i], x, y);

            // Strictly closer, so the lowest index wins a tie and two runs of
            // the same republic pick the same junction.
            if (gap <= bestGap && (best < 0 || gap < bestGap))
            {
                best = i;
                bestGap = gap;
            }
        }

        return best;
    }

    public Route? RouteBetween(int from, int to) => RouteWhere(from, to, static _ => true);

    /// <summary>
    /// Shortest route by <b>travel time</b>, over segments the caller will
    /// accept.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Dijkstra, and the tie-break is load-bearing. The frontier is ordered by
    /// <c>(cost, node)</c> rather than by cost alone: two routes of exactly
    /// equal time are common on a grid of roads, and a heap left to break the
    /// tie however it happens to would give one answer today and another after
    /// an unrelated edit. Same bar as worldgen — the same republic must behave
    /// the same way twice.
    /// </para>
    /// <para>
    /// Time rather than distance, because that is what a lorry actually spends
    /// and it is what makes a paved detour beat a direct track.
    /// </para>
    /// </remarks>
    public Route? RouteWhere(int from, int to, Func<Segment, bool> allow)
    {
        ArgumentNullException.ThrowIfNull(allow);
        var n = _nodeX.Count;
        if (from < 0 || to < 0 || from >= n || to >= n)
        {
            return null;
        }

        if (from == to)
        {
            return new Route([from], 0.0, 0.0);
        }

        var best = new double[n];
        Array.Fill(best, double.PositiveInfinity);
        var cameFrom = new int[n];
        Array.Fill(cameFrom, -1);
        var settled = new bool[n];

        // Ordered by cost then node — the total order that makes an equal-time
        // tie resolve the same way on any machine and in any build.
        var heap = new PriorityQueue<int, (double Cost, int Node)>();
        best[from] = 0.0;
        heap.Enqueue(from, (0.0, from));

        while (heap.TryDequeue(out var node, out var entry))
        {
            if (settled[node])
            {
                continue;
            }

            settled[node] = true;
            if (node == to)
            {
                break;
            }

            foreach (var (neighbour, segment) in _adjacency[node])
            {
                if (settled[neighbour] || !allow(_segments[segment]))
                {
                    continue;
                }

                var candidate = entry.Cost + _segments[segment].TravelTime;
                if (candidate < best[neighbour])
                {
                    best[neighbour] = candidate;
                    cameFrom[neighbour] = node;
                    heap.Enqueue(neighbour, (candidate, neighbour));
                }
            }
        }

        if (!settled[to])
        {
            return null;
        }

        var nodes = new List<int> { to };
        var cursor = to;
        while (cameFrom[cursor] >= 0)
        {
            cursor = cameFrom[cursor];
            nodes.Add(cursor);
        }

        nodes.Reverse();

        var distance = 0.0;
        for (var i = 0; i + 1 < nodes.Count; i++)
        {
            distance += SegmentBetween(nodes[i], nodes[i + 1])?.Length ?? 0.0;
        }

        return new Route(nodes, distance, best[to]);
    }

    private Segment? SegmentBetween(int a, int b)
    {
        foreach (var (node, segment) in _adjacency[a])
        {
            if (node == b)
            {
                return _segments[segment];
            }
        }

        return null;
    }

    /// <summary>
    /// The water a barge can actually use, sampled off the terrain.
    /// </summary>
    /// <remarks>
    /// Built rather than ordered: nobody digs a river. A fairway is laid where
    /// the water is wide enough to turn a barge in — a stream a metre across is
    /// water on the map and is not a waterway.
    /// </remarks>
    public static Network Navigable(Terrain terrain, Tables t)
    {
        ArgumentNullException.ThrowIfNull(terrain);
        ArgumentNullException.ThrowIfNull(t);

        var network = new Network();
        var spacing = t.FairwaySpacing;
        var beam = t.NavigableBeam;
        var speed = Units.KphToMps(t.NavigableKph);
        var across = Math.Max(1, (int)(terrain.Extent / spacing));

        // A lattice of candidate points over the water, joined to their
        // neighbours where both ends and the span between them float.
        var node = new int[across * across];
        Array.Fill(node, -1);

        for (var gy = 0; gy < across; gy++)
        {
            for (var gx = 0; gx < across; gx++)
            {
                var x = (gx + 0.5) * spacing;
                var y = (gy + 0.5) * spacing;
                if (IsFairway(terrain, x, y, beam))
                {
                    node[(gy * across) + gx] = network.AddNode(x, y);
                }
            }
        }

        for (var gy = 0; gy < across; gy++)
        {
            for (var gx = 0; gx < across; gx++)
            {
                var here = node[(gy * across) + gx];
                if (here < 0)
                {
                    continue;
                }

                // East and south only, so each pair is joined once.
                foreach (var (dx, dy) in new[] { (1, 0), (0, 1) })
                {
                    var nx = gx + dx;
                    var ny = gy + dy;
                    if (nx >= across || ny >= across)
                    {
                        continue;
                    }

                    var there = node[(ny * across) + nx];
                    if (there >= 0)
                    {
                        network.Connect(here, there, speed);
                    }
                }
            }
        }

        return network;
    }

    /// <summary>
    /// Whether a barge could sit here: water, and wide enough to turn in.
    /// </summary>
    private static bool IsFairway(Terrain terrain, double x, double y, double beam)
    {
        if (terrain.SurfaceAt(x, y) != Surface.Water)
        {
            return false;
        }

        var half = beam / 2.0;
        foreach (var (dx, dy) in new[] { (-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0) })
        {
            if (terrain.SurfaceAt(x + (dx * half), y + (dy * half)) != Surface.Water)
            {
                return false;
            }
        }

        return true;
    }
}
