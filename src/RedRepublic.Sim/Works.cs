namespace RedRepublic.Sim;

/// <summary>Why a way could not be ordered.</summary>
public enum RoadError
{
    None,
    TooShort,
    Unbuildable,

    /// <summary>Lamps are refused on anything but paved road.</summary>
    NoLampsOnThisGrade,

    /// <summary>The run crosses open water, and a road is not a bridge.</summary>
    NeedsABridge,
}

/// <summary>Why a line could not be ordered.</summary>
public enum LineError
{
    None,
    TooShort,
}

/// <summary>
/// A stretch of way under construction.
/// </summary>
/// <remarks>
/// A road is a site with a bill of materials like any other, and the gravel has
/// to be <b>driven</b> to it. That is why freight can address one at all: an
/// order does not conjure a road, it puts down somewhere for lorries to take
/// gravel.
/// </remarks>
public sealed class RoadSite(
    int id, double fromX, double fromY, double toX, double toY,
    int grade, bool lamps, long ordered)
{
    public int Id { get; } = id;

    public double FromX { get; } = fromX;

    public double FromY { get; } = fromY;

    public double ToX { get; } = toX;

    public double ToY { get; } = toY;

    /// <summary>An index into the authored grade table.</summary>
    public int Grade { get; } = grade;

    /// <summary>
    /// Built with street lighting. A variant of the road rather than something
    /// you place, which is the whole design: nobody wants to site four hundred
    /// lamp posts.
    /// </summary>
    public bool Lamps { get; } = lamps;

    /// <summary>
    /// Where this sits in the republic's commissioning order — the count of
    /// things commissioned when it was ordered, not a date. It is what makes a
    /// road take its turn in the build queue like anything else, rather than
    /// jumping it or waiting behind every factory in the republic.
    /// </summary>
    public long Ordered { get; } = ordered;

    public double WorkDone { get; internal set; }

    public double Length => Units.Distance(FromX, FromY, ToX, ToY);

    public double Kilometres => Length / 1000.0;

    /// <summary>Builder-days this run needs, bill and lamps included.</summary>
    public double Labour(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var labour = t.Grades[Grade].Labour * Kilometres;
        return Lamps ? labour + (t.LampLabour * Kilometres) : labour;
    }

    public bool IsBuilt(Tables t) => WorkDone >= Labour(t);

    public double Progress(Tables t)
    {
        var labour = Labour(t);
        return labour <= 0.0 ? 1.0 : Math.Clamp(WorkDone / labour, 0.0, 1.0);
    }

    /// <summary>
    /// What this run needs of a material, by the kilometre — lamps included,
    /// because a lit road is a road plus lamps rather than a different road.
    /// </summary>
    public double Wants(int resource, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var wanted = 0.0;
        foreach (var b in t.Grades[Grade].Materials)
        {
            if (b.Resource == resource)
            {
                wanted += b.Tonnes * Kilometres;
            }
        }

        if (Lamps)
        {
            foreach (var b in t.LampMaterials)
            {
                if (b.Resource == resource)
                {
                    wanted += b.Tonnes * Kilometres;
                }
            }
        }

        return wanted;
    }
}

/// <summary>A power line or heat main under construction.</summary>
/// <remarks>
/// Same reasoning as a road site: it has a bill of materials and somebody has to
/// drive the steel out to it.
/// </remarks>
public sealed class LineSite(
    int id, int kind, double fromX, double fromY, double toX, double toY, long ordered)
{
    public int Id { get; } = id;

    /// <summary>An index into the authored utility table.</summary>
    public int Kind { get; } = kind;

    public double FromX { get; } = fromX;

    public double FromY { get; } = fromY;

    public double ToX { get; } = toX;

    public double ToY { get; } = toY;

    /// <summary>
    /// Where this sits in the republic's commissioning order — the count of
    /// things commissioned when it was ordered, not a date. It is what makes a
    /// road take its turn in the build queue like anything else, rather than
    /// jumping it or waiting behind every factory in the republic.
    /// </summary>
    public long Ordered { get; } = ordered;

    public double WorkDone { get; internal set; }

    public double Length => Units.Distance(FromX, FromY, ToX, ToY);

    public double Kilometres => Length / 1000.0;

    public double Labour(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return t.Utilities[Kind].Labour * Kilometres;
    }

    public bool IsBuilt(Tables t) => WorkDone >= Labour(t);

    public double Progress(Tables t)
    {
        var labour = Labour(t);
        return labour <= 0.0 ? 1.0 : Math.Clamp(WorkDone / labour, 0.0, 1.0);
    }

    public double Wants(int resource, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var wanted = 0.0;
        foreach (var b in t.Utilities[Kind].Materials)
        {
            if (b.Resource == resource)
            {
                wanted += b.Tonnes * Kilometres;
            }
        }

        return wanted;
    }
}

/// <summary>A finished line, carrying something between two points.</summary>
/// <param name="A">
/// The union-find node at the <c>From</c> end. Two lines that share an end share
/// a node, which is what makes them one network — see <see cref="Networks"/>.
/// </param>
/// <param name="B">The node at the <c>To</c> end.</param>
public readonly record struct Line(
    int Id, int Kind, double FromX, double FromY, double ToX, double ToY, int A, int B)
{
    public double Length => Units.Distance(FromX, FromY, ToX, ToY);

    public double Kilometres => Length / 1000.0;

    /// <summary>
    /// How far a point is from this span, measured to the segment rather than to
    /// either end — a building beside the middle of a line is beside the line.
    /// </summary>
    public double DistanceTo(double x, double y) =>
        Units.DistanceToSegment(x, y, FromX, FromY, ToX, ToY);
}

/// <summary>Every way the republic has ordered but not finished.</summary>
public sealed class RoadWorks(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<RoadSite> _sites = [];

    private int _nextId = 1;

    public IReadOnlyList<RoadSite> Sites => _sites;

    /// <summary>What each site is holding.</summary>
    public GrowableStock Stock { get; } = new(tables.Resources.Length);

    public RoadSite? Get(int id)
    {
        foreach (var s in _sites)
        {
            if (s.Id == id)
            {
                return s;
            }
        }

        return null;
    }

    public int IndexOf(int id)
    {
        for (var i = 0; i < _sites.Count; i++)
        {
            if (_sites[i].Id == id)
            {
                return i;
            }
        }

        return -1;
    }

    /// <summary>
    /// Order a way, or say why not.
    /// </summary>
    /// <remarks>
    /// The water check is what stops a gravel road being laid straight across a
    /// river at the price of gravel — before it existed, nothing asked.
    /// </remarks>
    public RoadError Order(
        double fromX, double fromY, double toX, double toY,
        int grade, bool lamps, long ordered, Terrain terrain, out RoadSite? site)
    {
        ArgumentNullException.ThrowIfNull(terrain);
        site = null;

        if (Units.Distance(fromX, fromY, toX, toY) < _t.MinRoad)
        {
            return RoadError.TooShort;
        }

        if (lamps && !_t.Grades[grade].Lamps)
        {
            return RoadError.NoLampsOnThisGrade;
        }

        var carriesWater = _t.Grades[grade].Carries == Medium.Water;
        var isBridge = _t.Grades[grade].Id is "Bridge" or "RailBridge";
        if (!isBridge && !carriesWater && terrain.CrossesWater(fromX, fromY, toX, toY))
        {
            return RoadError.NeedsABridge;
        }

        site = new RoadSite(_nextId++, fromX, fromY, toX, toY, grade, lamps, ordered);
        _sites.Add(site);
        Stock.Grow();
        return RoadError.None;
    }

    /// <summary>
    /// Whether the materials for the work still to do are on hand. Same rule as
    /// a building site: the bill falls as the work is done.
    /// </summary>
    public bool HasMaterials(RoadSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var i = IndexOf(site.Id);
        if (i < 0)
        {
            return false;
        }

        var left = 1.0 - site.Progress(_t);
        for (var r = 0; r < _t.Resources.Length; r++)
        {
            if (Stock.Get(i, r) + 1e-9 < site.Wants(r, _t) * left)
            {
                return false;
            }
        }

        return true;
    }

    /// <summary>
    /// Put a site back exactly as it was, keeping its id — what a load does.
    /// </summary>
    public RoadSite Restore(
        int id, double fromX, double fromY, double toX, double toY,
        int grade, bool lamps, long ordered, double workDone)
    {
        var site = new RoadSite(id, fromX, fromY, toX, toY, grade, lamps, ordered)
        {
            WorkDone = workDone,
        };

        _sites.Add(site);
        Stock.Grow();
        _nextId = Math.Max(_nextId, id + 1);
        return site;
    }

    public void Finish(RoadSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var i = IndexOf(site.Id);
        if (i >= 0)
        {
            _sites.RemoveAt(i);
            Stock.RemoveAt(i);
        }
    }

    /// <summary>
    /// Lay a finished road into the network.
    /// </summary>
    /// <remarks>
    /// <b>Junctions along the length, not only at the ends.</b> Access to the
    /// network is measured from junctions, so a five-kilometre road with
    /// junctions only at its ends would serve the two buildings at those ends and
    /// nothing in between. Ends merge onto whatever junction already stands
    /// there, so two roads ordered end to end become one network rather than two
    /// islands.
    /// </remarks>
    public static void Open(Network network, RoadSite site, Tables t)
    {
        ArgumentNullException.ThrowIfNull(network);
        ArgumentNullException.ThrowIfNull(site);
        ArgumentNullException.ThrowIfNull(t);

        var speed = Units.KphToMps(t.Grades[site.Grade].SpeedKph);
        var steps = Math.Max(1, (int)Math.Ceiling(site.Length / t.JunctionSpacing));
        var previous = network.JunctionAt(site.FromX, site.FromY, t.JunctionMerge);

        for (var step = 1; step <= steps; step++)
        {
            var along = (double)step / steps;
            var next = network.JunctionAt(
                site.FromX + ((site.ToX - site.FromX) * along),
                site.FromY + ((site.ToY - site.FromY) * along),
                t.JunctionMerge);

            // A junction that merged onto the one behind it adds no road.
            if (next != previous && !network.AreConnected(previous, next))
            {
                network.Connect(previous, next, speed, site.Lamps);
            }

            previous = next;
        }
    }
}

/// <summary>
/// Every line the republic has ordered and not yet energised.
/// </summary>
/// <remarks>
/// Sites only. A finished span leaves here and joins <see cref="Networks"/>,
/// because the two answer different questions: this one is a thing lorries
/// deliver steel to, and that one is a thing current travels along.
/// </remarks>
public sealed class LineWorks(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<LineSite> _sites = [];

    private int _nextSiteId = 1;

    public IReadOnlyList<LineSite> Sites => _sites;

    public GrowableStock Stock { get; } = new(tables.Resources.Length);

    public LineSite? Get(int id)
    {
        foreach (var s in _sites)
        {
            if (s.Id == id)
            {
                return s;
            }
        }

        return null;
    }

    /// <summary>
    /// Whether the materials for the work still to do are on hand. Same rule as
    /// a building site: the bill falls as the work is done.
    /// </summary>
    public bool HasMaterials(LineSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var i = IndexOf(site.Id);
        if (i < 0)
        {
            return false;
        }

        var left = site.Kilometres * (1.0 - site.Progress(_t));
        foreach (var bill in _t.Utilities[site.Kind].Materials)
        {
            if (Stock.Get(i, bill.Resource) + 1e-9 < bill.Tonnes * left)
            {
                return false;
            }
        }

        return true;
    }

    public int IndexOf(int siteId)
    {
        for (var i = 0; i < _sites.Count; i++)
        {
            if (_sites[i].Id == siteId)
            {
                return i;
            }
        }

        return -1;
    }

    public LineError Order(
        int kind, double fromX, double fromY, double toX, double toY,
        long ordered, out LineSite? site)
    {
        site = null;
        if (Units.Distance(fromX, fromY, toX, toY) < _t.MinLine)
        {
            return LineError.TooShort;
        }

        site = new LineSite(_nextSiteId++, kind, fromX, fromY, toX, toY, ordered);
        _sites.Add(site);
        Stock.Grow();
        return LineError.None;
    }

    /// <summary>
    /// Put a site back exactly as it was, keeping its id — what a load does.
    /// </summary>
    /// <remarks>
    /// Ids come back as they were rather than being renumbered, because a save
    /// that renumbered its sites would break every journal entry that names one.
    /// </remarks>
    public LineSite Restore(
        int id, int kind, double fromX, double fromY, double toX, double toY,
        long ordered, double workDone)
    {
        var site = new LineSite(id, kind, fromX, fromY, toX, toY, ordered)
        {
            WorkDone = workDone,
        };

        _sites.Add(site);
        Stock.Grow();
        _nextSiteId = Math.Max(_nextSiteId, id + 1);
        return site;
    }

    /// <summary>
    /// Take a finished site off the books. Energising it is
    /// <see cref="Networks.Energise"/>'s business, and the two are called
    /// together — a site that left here without joining a grid would be steel
    /// nobody can find.
    /// </summary>
    public void Finish(LineSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var i = IndexOf(site.Id);
        if (i >= 0)
        {
            _sites.RemoveAt(i);
            Stock.RemoveAt(i);
        }
    }
}
