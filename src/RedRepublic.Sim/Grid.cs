namespace RedRepublic.Sim;

/// <summary>
/// The republic's energised networks, and who is plugged into them.
/// </summary>
/// <remarks>
/// <para>
/// <b>This is what makes power and heat physical.</b> Without it a station
/// anywhere on the map lights every building on it and a boiler house anywhere
/// warms every block — quantities with no geography, the same shape freight had
/// before lorries and construction had before crews. A republic could site its
/// plant on the far side of a mountain and lose nothing by it.
/// </para>
/// <para>
/// <b>Connection is stored, not searched.</b> Testing every building against
/// every line on every tick is <c>buildings × lines</c> distance tests, 1,440
/// times a simulated day. A building is attached once — when it is placed, or
/// when a line opens near it — and the attachment is a node index in a
/// union-find. Reading it back is a find, and two grids that later meet merge in
/// one union rather than by rewiring anybody. The same discipline the traversal
/// lattice uses: derive at the event that invalidates it, not per tick.
/// </para>
/// <para>
/// <b>A pylon is not a manhole.</b> Nodes carry the kind of line that made them,
/// so a power line and a heat main whose ends fall in the same place stay two
/// networks. Without that, <see cref="NetworkOf"/> answers the same number for
/// both, which is a pipe carrying electricity.
/// </para>
/// </remarks>
public sealed class Networks(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<Line> _lines = [];

    // Merged line ends, in creation order: shared pylons and junction chambers.
    private readonly List<double> _nodeX = [];
    private readonly List<double> _nodeY = [];
    private readonly List<int> _nodeKind = [];
    private readonly List<int> _parent = [];

    // Where each building is plugged in, by (building id, utility kind).
    private readonly Dictionary<(int Building, int Kind), int> _attached = [];

    private int _nextId = 1;

    public IReadOnlyList<Line> Lines => _lines;

    public int Count => _lines.Count;

    /// <summary>Every span of one kind — what a power or heat pass walks.</summary>
    public List<Line> OfKind(int kind)
    {
        var found = new List<Line>();
        foreach (var l in _lines)
        {
            if (l.Kind == kind)
            {
                found.Add(l);
            }
        }

        return found;
    }

    public double LengthOf(int kind)
    {
        var total = 0.0;
        foreach (var l in _lines)
        {
            if (l.Kind == kind)
            {
                total += l.Length;
            }
        }

        return total;
    }

    /// <summary>
    /// Which network a building is on, or -1 if it is on none of that kind.
    /// </summary>
    /// <remarks>
    /// The stored attachment resolved through the union-find, so two grids since
    /// joined by a new span answer with the same number without anybody being
    /// rewired.
    /// </remarks>
    public int NetworkOf(int building, int kind) =>
        _attached.TryGetValue((building, kind), out var node) ? Root(node) : -1;

    /// <summary>Whether two buildings are on the same network of a kind.</summary>
    public bool Together(int a, int b, int kind)
    {
        var x = NetworkOf(a, kind);
        return x >= 0 && x == NetworkOf(b, kind);
    }

    /// <summary>
    /// The span of one network, end to end — what the losses are charged on.
    /// </summary>
    /// <remarks>
    /// The sum of its spans rather than the distance between its extremes,
    /// because current and hot water travel the wire and the pipe rather than the
    /// straight line, and a network that doubles back on itself really does lose
    /// more.
    /// </remarks>
    public double SpanOf(int network, int kind)
    {
        var total = 0.0;
        foreach (var l in _lines)
        {
            if (l.Kind == kind && Root(l.A) == network)
            {
                total += l.Length;
            }
        }

        return total;
    }

    /// <summary>
    /// What fraction of what enters a network comes out the far end of it.
    /// </summary>
    /// <remarks>
    /// Charged on the whole network rather than on one span, which is what makes
    /// a sprawling grid worse than a compact one and is the whole argument for
    /// siting a plant near what it serves.
    /// </remarks>
    public double Efficiency(int network, int kind) =>
        Math.Clamp(1.0 - (_t.Utilities[kind].LossPerKm * (SpanOf(network, kind) / 1000.0)), 0.0, 1.0);

    /// <summary>How many buildings are plugged into a kind of network.</summary>
    public int ConnectedCount(int kind)
    {
        var n = 0;
        foreach (var key in _attached.Keys)
        {
            if (key.Kind == kind)
            {
                n++;
            }
        }

        return n;
    }

    /// <summary>
    /// Energise a finished span.
    /// </summary>
    /// <remarks>
    /// Nothing is subdivided the way a road is: a road needs junctions along its
    /// length because access is measured from junctions, and a connection here is
    /// measured from the <i>span</i>, so the two ends are all the structure it
    /// needs.
    /// </remarks>
    public Line Energise(LineSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var a = NodeAt(site.FromX, site.FromY, site.Kind);
        var b = NodeAt(site.ToX, site.ToY, site.Kind);
        Union(a, b);

        var line = new Line(
            _nextId++, site.Kind, site.FromX, site.FromY, site.ToX, site.ToY, a, b);
        _lines.Add(line);
        return line;
    }

    /// <summary>
    /// Plug a building in, if anything of that kind runs close enough to it.
    /// </summary>
    /// <remarks>
    /// Nearest span wins, ties on line id, so the answer does not depend on the
    /// order the lines happen to sit in the list — a determinism requirement
    /// rather than tidiness, because a save that replayed its journal could
    /// otherwise come back on a different grid.
    /// </remarks>
    public bool Attach(int building, double x, double y, int kind)
    {
        var reach = _t.Utilities[kind].Reach;
        var node = -1;
        var bestGap = 0.0;
        var bestId = 0;

        foreach (var l in _lines)
        {
            if (l.Kind != kind)
            {
                continue;
            }

            var gap = l.DistanceTo(x, y);
            if (gap > reach)
            {
                continue;
            }

            if (node < 0 || gap < bestGap || (gap == bestGap && l.Id < bestId))
            {
                node = l.A;
                bestGap = gap;
                bestId = l.Id;
            }
        }

        if (node < 0)
        {
            return false;
        }

        _attached[(building, kind)] = node;
        return true;
    }

    /// <summary>Plug a building into everything within reach of it — what a placement does.</summary>
    public void AttachAll(int building, double x, double y)
    {
        for (var kind = 0; kind < _t.Utilities.Length; kind++)
        {
            Attach(building, x, y, kind);
        }
    }

    /// <summary>Unplug a building from everything — what a demolition does.</summary>
    public void Detach(int building)
    {
        for (var kind = 0; kind < _t.Utilities.Length; kind++)
        {
            _attached.Remove((building, kind));
        }
    }

    /// <summary>
    /// Plug in everything that now stands beside a newly energised span.
    /// </summary>
    /// <remarks>
    /// The other half of <see cref="Energise"/>: a line opening near a building
    /// connects it just as surely as a building going up near a line does, and
    /// nothing else would ever notice.
    /// </remarks>
    public void AttachAlong(Buildings buildings, int kind)
    {
        ArgumentNullException.ThrowIfNull(buildings);
        for (var b = 0; b < buildings.Count; b++)
        {
            Attach(buildings.IdAt(b), buildings.XAt(b), buildings.YAt(b), kind);
        }
    }

    /// <summary>
    /// Root of a node. No path compression, so this stays a read: union by lower
    /// index keeps the trees shallow enough that walking them is cheaper than the
    /// mutability compression would need.
    /// </summary>
    private int Root(int node)
    {
        while (_parent[node] != node)
        {
            node = _parent[node];
        }

        return node;
    }

    private void Union(int a, int b)
    {
        var ra = Root(a);
        var rb = Root(b);
        if (ra == rb)
        {
            return;
        }

        // Lower index wins, which keeps the answer independent of the order lines
        // happened to be built in — a determinism requirement, not a performance
        // one.
        if (ra < rb)
        {
            _parent[rb] = ra;
        }
        else
        {
            _parent[ra] = rb;
        }
    }

    /// <summary>
    /// Find or create the node at a point, merging onto anything of the same kind
    /// within the join distance. Without that merge every line is an island.
    /// </summary>
    private int NodeAt(double x, double y, int kind)
    {
        var found = -1;
        var bestGap = 0.0;

        for (var i = 0; i < _nodeX.Count; i++)
        {
            if (_nodeKind[i] != kind)
            {
                continue;
            }

            var gap = Units.Distance(_nodeX[i], _nodeY[i], x, y);
            if (gap > _t.UtilityJoin)
            {
                continue;
            }

            // Strictly nearer, so the lowest index takes a tie.
            if (found < 0 || gap < bestGap)
            {
                found = i;
                bestGap = gap;
            }
        }

        if (found >= 0)
        {
            return found;
        }

        var index = _nodeX.Count;
        _nodeX.Add(x);
        _nodeY.Add(y);
        _nodeKind.Add(kind);
        _parent.Add(index);
        return index;
    }
}
