namespace RedRepublic.Sim;

public enum Mineral
{
    Coal,
    IronOre,
    Oil,
    Gravel,
    Groundwater,
}

public static class MineralFacts
{
    /// <summary>
    /// Whether the body refills. An aquifer does; nothing else does.
    /// </summary>
    /// <remarks>
    /// A property of the mineral rather than a check against
    /// <see cref="Mineral.Groundwater"/> inside the systems that recharge, for
    /// the reason every other authored property is one: a list of ids inside
    /// logic is a thing you must remember to edit, and what you forget lands
    /// silently in a fallback.
    /// </remarks>
    public static bool Recharges(this Mineral m) => m == Mineral.Groundwater;
}

/// <summary>One horizon within a deposit. Layers are ordered shallowest first.</summary>
public sealed class Layer(double thickness, double tonnes)
{
    /// <summary>How thick this horizon is, in metres.</summary>
    public double Thickness { get; } = thickness;

    /// <summary>What is left in it, in tonnes.</summary>
    public double Remaining { get; internal set; } = tonnes;

    /// <summary>
    /// What it held when the map was made — the denominator for "how worked out
    /// is this?", which the survey reports and the player reads.
    /// </summary>
    public double Initial { get; } = tonnes;

    public bool IsWorkedOut => Remaining <= 0.0;
}

/// <summary>What came out of the ground, and what it cost to reach.</summary>
public readonly record struct Yield(double Tonnes, double Depth, double CostMultiplier);

/// <summary>What a survey says about one body.</summary>
public readonly record struct SurveyReading(
    int Id,
    Mineral Mineral,
    double CentreX,
    double CentreY,
    double Radius,
    double Top,
    double Remaining,
    double Initial,
    double WorkingDepth)
{
    public double FractionRemaining => Initial <= 0.0 ? 0.0 : Remaining / Initial;
}

/// <summary>A body of mineral under the ground.</summary>
public sealed class Deposit
{
    /// <summary>
    /// How much dearer each metre of depth makes the work.
    /// </summary>
    /// <remarks>
    /// Shaped rather than merely scaled: cost rises with depth but never becomes
    /// infinite, so a deposit always remains <i>technically</i> workable and
    /// abandoning it stays the player's judgement rather than a wall the
    /// simulation puts up.
    /// </remarks>
    public const double DepthCostPerMetre = 0.004;

    public Deposit(
        int id, Mineral mineral, double centreX, double centreY,
        double radius, double top, Layer[] layers)
    {
        Id = id;
        Mineral = mineral;
        CentreX = centreX;
        CentreY = centreY;
        Radius = radius;
        Top = top;
        Layers = layers;
    }

    public int Id { get; }

    public Mineral Mineral { get; }

    public double CentreX { get; }

    public double CentreY { get; }

    /// <summary>
    /// Horizontal reach from the centre. A disc for now — real bodies are not
    /// discs, and a richer footprint is a later refinement that changes only
    /// <see cref="Covers"/>.
    /// </summary>
    public double Radius { get; }

    /// <summary>Depth to the top of the body, in metres.</summary>
    public double Top { get; }

    public Layer[] Layers { get; }

    /// <summary>
    /// Whether a point on the surface sits over this body — which is what
    /// decides whether a mine built there can tap it.
    /// </summary>
    public bool Covers(double x, double y) =>
        Units.Distance(CentreX, CentreY, x, y) <= Radius;

    /// <summary>Everything still in the ground.</summary>
    public double Remaining
    {
        get
        {
            var sum = 0.0;
            foreach (var l in Layers)
            {
                sum += l.Remaining;
            }

            return sum;
        }
    }

    /// <summary>Everything the body ever held.</summary>
    public double Initial
    {
        get
        {
            var sum = 0.0;
            foreach (var l in Layers)
            {
                sum += l.Initial;
            }

            return sum;
        }
    }

    public bool IsExhausted => Remaining <= 0.0;

    /// <summary>
    /// Depth work is happening at: the top of the shallowest live layer. An
    /// exhausted body reports the bottom of the whole thing, which is where work
    /// would be if there were anything left to do.
    /// </summary>
    public double WorkingDepth
    {
        get
        {
            var stop = LiveLayer() ?? Layers.Length;
            var depth = Top;
            for (var i = 0; i < stop; i++)
            {
                depth += Layers[i].Thickness;
            }

            return depth;
        }
    }

    /// <summary>What working at the current depth multiplies costs by.</summary>
    public double DepthCostMultiplier => 1.0 + (WorkingDepth * DepthCostPerMetre);

    /// <summary>
    /// Take up to <paramref name="wanted"/> tonnes, working down through the
    /// layers.
    /// </summary>
    /// <remarks>
    /// Returns what was actually available, which may be less — and the depth it
    /// came from, since that is what the operation costs. A request that spans a
    /// layer boundary reports the depth it <i>finished</i> at, so the cost of a
    /// load reflects the deepest work it required rather than the cheapest.
    /// </remarks>
    public Yield Extract(double wanted)
    {
        var got = 0.0;
        var outstanding = wanted;

        while (outstanding > 0.0)
        {
            var index = LiveLayer();
            if (index is null)
            {
                break;
            }

            var layer = Layers[index.Value];
            var taken = Math.Min(outstanding, layer.Remaining);
            layer.Remaining = Math.Max(0.0, layer.Remaining - taken);
            got += taken;
            outstanding = Math.Max(0.0, outstanding - taken);
        }

        return new Yield(got, WorkingDepth, DepthCostMultiplier);
    }

    /// <summary>
    /// Put water back. Only ever called for a recharging mineral — an aquifer
    /// refills, and nothing else does.
    /// </summary>
    /// <remarks>
    /// Recharge fills from the shallowest layer down and never exceeds what the
    /// horizon originally held, so a well cannot manufacture an aquifer larger
    /// than the ground it sits in.
    /// </remarks>
    public void Recharge(double amount)
    {
        var spare = amount;
        foreach (var layer in Layers)
        {
            if (spare <= 0.0)
            {
                break;
            }

            var room = Math.Max(0.0, layer.Initial - layer.Remaining);
            var added = Math.Min(spare, room);
            layer.Remaining += added;
            spare = Math.Max(0.0, spare - added);
        }
    }

    public SurveyReading Survey() => new(
        Id, Mineral, CentreX, CentreY, Radius, Top, Remaining, Initial, WorkingDepth);

    /// <summary>Index of the shallowest layer with anything left.</summary>
    private int? LiveLayer()
    {
        for (var i = 0; i < Layers.Length; i++)
        {
            if (!Layers[i].IsWorkedOut)
            {
                return i;
            }
        }

        return null;
    }
}

/// <summary>
/// Every deposit under a map.
/// </summary>
/// <remarks>
/// A list rather than a map keyed by id, on purpose: iteration order is part of
/// the simulation's definition wherever an accumulation follows it, and a hash
/// map's order is not a thing a shared seed can promise.
/// </remarks>
public sealed class Geology
{
    private readonly List<Deposit> _deposits = [];

    public IReadOnlyList<Deposit> All => _deposits;

    /// <summary>
    /// Generate the geology of a square map.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Bodies are centred anywhere within the extent — including near an edge,
    /// where part of the body simply runs off the map. That is deliberate: a
    /// seam that continues past the border is more honest than one that politely
    /// stops, and it means the edge of the map is not quietly the richest place
    /// to build.
    /// </para>
    /// <para>
    /// <b>Plan order, then body order, then layer order — fixed, because the
    /// draw order IS the map.</b> Every value comes from
    /// <see cref="Rng.NextRange"/>, which is a multiply and an add: no
    /// transcendentals, nothing that could differ in its last bit between
    /// machines.
    /// </para>
    /// </remarks>
    public static Geology Generate(ulong seed, double extent, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        var rng = Rng.FromSeed(seed);
        var geology = new Geology();
        var nextId = 1;

        foreach (var entry in tables.MineralPlan)
        {
            for (var b = 0; b < entry.Bodies; b++)
            {
                var cx = rng.NextRange(0.0, extent);
                var cy = rng.NextRange(0.0, extent);
                var radius = rng.NextRange(entry.RadiusLo, entry.RadiusHi);
                var top = rng.NextRange(entry.TopLo, entry.TopHi);

                var layers = new Layer[entry.Layers];
                for (var l = 0; l < entry.Layers; l++)
                {
                    var thickness = rng.NextRange(entry.ThicknessLo, entry.ThicknessHi);
                    var tonnes = rng.NextRange(entry.TonnesLo, entry.TonnesHi);
                    layers[l] = new Layer(thickness, tonnes);
                }

                geology._deposits.Add(new Deposit(
                    nextId, (Mineral)entry.Mineral, cx, cy, radius, top, layers));
                nextId++;
            }
        }

        return geology;
    }

    public Deposit? Get(int id)
    {
        foreach (var d in _deposits)
        {
            if (d.Id == id)
            {
                return d;
            }
        }

        return null;
    }

    /// <summary>Every body a mine at this point could work.</summary>
    public List<int> TappableAt(double x, double y)
    {
        var found = new List<int>();
        foreach (var d in _deposits)
        {
            if (d.Covers(x, y) && !d.IsExhausted)
            {
                found.Add(d.Id);
            }
        }

        return found;
    }

    public List<SurveyReading> Survey()
    {
        var readings = new List<SurveyReading>(_deposits.Count);
        foreach (var d in _deposits)
        {
            readings.Add(d.Survey());
        }

        return readings;
    }

    public double RemainingOf(Mineral mineral)
    {
        var sum = 0.0;
        foreach (var d in _deposits)
        {
            if (d.Mineral == mineral)
            {
                sum += d.Remaining;
            }
        }

        return sum;
    }

    /// <summary>
    /// How far the nearest body of a mineral is, or null if there is none left.
    /// What a founding candidate card reports about a posting.
    /// </summary>
    public double? DistanceToNearest(double x, double y, Mineral mineral)
    {
        double? best = null;
        foreach (var d in _deposits)
        {
            if (d.Mineral != mineral || d.IsExhausted)
            {
                continue;
            }

            // The distance to the body, not to its centre: standing on the edge
            // of a seam is standing on the seam.
            var gap = Math.Max(0.0, Units.Distance(d.CentreX, d.CentreY, x, y) - d.Radius);
            if (best is null || gap < best)
            {
                best = gap;
            }
        }

        return best;
    }
}
