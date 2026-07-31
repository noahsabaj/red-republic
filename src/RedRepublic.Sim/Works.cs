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
    int grade, bool lamps, long orderedDay)
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

    public long OrderedDay { get; } = orderedDay;

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
    int id, int kind, double fromX, double fromY, double toX, double toY, long orderedDay)
{
    public int Id { get; } = id;

    /// <summary>An index into the authored utility table.</summary>
    public int Kind { get; } = kind;

    public double FromX { get; } = fromX;

    public double FromY { get; } = fromY;

    public double ToX { get; } = toX;

    public double ToY { get; } = toY;

    public long OrderedDay { get; } = orderedDay;

    public double WorkDone { get; internal set; }

    public double Length => Units.Distance(FromX, FromY, ToX, ToY);

    public double Kilometres => Length / 1000.0;

    public double Labour(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return t.Utilities[Kind].Labour * Kilometres;
    }

    public bool IsBuilt(Tables t) => WorkDone >= Labour(t);

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
public readonly record struct Line(int Id, int Kind, double FromX, double FromY, double ToX, double ToY)
{
    public double Length => Units.Distance(FromX, FromY, ToX, ToY);

    public double Kilometres => Length / 1000.0;

    /// <summary>
    /// What fraction of what enters this line comes out the other end.
    /// </summary>
    /// <remarks>
    /// Loss by the kilometre is what makes <i>where you put the power station</i>
    /// matter. Without it a plant anywhere lights everything, which is the
    /// abstraction lines exist to replace.
    /// </remarks>
    public double Efficiency(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return Math.Clamp(1.0 - (t.Utilities[Kind].LossPerKm * Kilometres), 0.0, 1.0);
    }
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
        int grade, bool lamps, long today, Terrain terrain, out RoadSite? site)
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

        site = new RoadSite(_nextId++, fromX, fromY, toX, toY, grade, lamps, today);
        _sites.Add(site);
        Stock.Grow();
        return RoadError.None;
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
}

/// <summary>Every line ordered, and every line finished.</summary>
public sealed class LineWorks(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<LineSite> _sites = [];
    private readonly List<Line> _lines = [];

    private int _nextSiteId = 1;
    private int _nextLineId = 1;

    public IReadOnlyList<LineSite> Sites => _sites;

    public IReadOnlyList<Line> Lines => _lines;

    public GrowableStock Stock { get; } = new(tables.Resources.Length);

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
        long today, out LineSite? site)
    {
        site = null;
        if (Units.Distance(fromX, fromY, toX, toY) < _t.MinLine)
        {
            return LineError.TooShort;
        }

        site = new LineSite(_nextSiteId++, kind, fromX, fromY, toX, toY, today);
        _sites.Add(site);
        Stock.Grow();
        return LineError.None;
    }

    /// <summary>The site is done: it becomes a line and stops being a site.</summary>
    public Line Finish(LineSite site)
    {
        ArgumentNullException.ThrowIfNull(site);
        var i = IndexOf(site.Id);
        if (i >= 0)
        {
            _sites.RemoveAt(i);
            Stock.RemoveAt(i);
        }

        var line = new Line(
            _nextLineId++, site.Kind, site.FromX, site.FromY, site.ToX, site.ToY);
        _lines.Add(line);
        return line;
    }

    /// <summary>Every line of one kind — what the power or heat pass walks.</summary>
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

    public double TotalLength(int kind)
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
}
