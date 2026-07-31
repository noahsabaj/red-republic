namespace RedRepublic.Sim;

/// <summary>Who is on the other side of the frontier.</summary>
/// <remarks>
/// The two blocs are <b>not two spellings of the same money</b>. A republic
/// starts with roubles and no dollars at all, and earns dollars only by hauling
/// goods to a western post. Which bloc you trade with is a geographic decision
/// because a crossing settles in its own bloc's money.
/// </remarks>
public enum Market
{
    East,
    West,
}

/// <summary>A stretch of the perimeter, and who holds it.</summary>
/// <param name="Start">
/// Where this stretch begins on the perimeter. It runs to the next arc's start,
/// and the last wraps to the first.
/// </param>
public readonly record struct FrontierArc(double Start, Market Bloc);

/// <summary>
/// A crossing point, placed by worldgen rather than by the player.
/// </summary>
/// <remarks>
/// <b>You do not build these.</b> The frontier and its posts are there when you
/// arrive; what you decide is which one to build road out to, and that is a
/// siting and haulage decision rather than a dropdown.
/// </remarks>
public readonly record struct BorderCrossing(int Id, double X, double Y, double Along, Market Bloc);

/// <summary>
/// The whole perimeter, and who is on the other side of each part of it.
/// </summary>
/// <remarks>
/// A republic is <i>surrounded</i>. An earlier model named one edge of the map
/// as foreign and said nothing about the other three, so a republic had a single
/// frontier and three impassable nothings.
/// </remarks>
public sealed class Frontier
{
    /// <summary>The perimeter, measured in sides: four turns brings you home.</summary>
    public const double Turns = 4.0;

    private readonly List<FrontierArc> _arcs;
    private readonly List<BorderCrossing> _crossings;

    private Frontier(double extent, List<FrontierArc> arcs, List<BorderCrossing> crossings)
    {
        Extent = extent;
        _arcs = arcs;
        _crossings = crossings;
    }

    public double Extent { get; }

    public IReadOnlyList<FrontierArc> Arcs => _arcs;

    public IReadOnlyList<BorderCrossing> Crossings => _crossings;

    /// <summary>
    /// Lay a frontier around a map: two blocs, split at two points drawn from
    /// the seed, with posts spaced evenly around the whole loop.
    /// </summary>
    /// <remarks>
    /// The posts are evenly spaced rather than scattered so that one is always a
    /// haul away and "which one" stays a real choice — the offset is what stops
    /// every republic having a post in the same corner.
    /// </remarks>
    public static Frontier Generate(double extent, Rng rng, Tables t)
    {
        ArgumentNullException.ThrowIfNull(rng);
        ArgumentNullException.ThrowIfNull(t);

        var first = rng.NextBounded(4_000) / 1_000.0;
        var span = 1.0 + (rng.NextBounded(2_000) / 1_000.0);
        var second = Rem(first + span, Turns);

        var (a, b) = first <= second ? (first, second) : (second, first);
        var eastFirst = rng.NextBounded(2) == 0;
        var (one, two) = eastFirst ? (Market.East, Market.West) : (Market.West, Market.East);

        var arcs = new List<FrontierArc>
        {
            new(0.0, two),
            new(a, one),
            new(b, two),
        };

        var frontier = new Frontier(extent, arcs, []);
        var offset = rng.NextBounded(1_000) / 1_000.0 * Turns / t.Crossings;
        for (var i = 0; i < t.Crossings; i++)
        {
            var along = (offset + (Turns * i / t.Crossings)) % Turns;
            var (x, y) = frontier.InsetPoint(along, t.CrossingInset);
            frontier._crossings.Add(
                new BorderCrossing(i + 1, x, y, along, frontier.BlocAt(along)));
        }

        return frontier;
    }

    public BorderCrossing? Get(int id)
    {
        foreach (var c in _crossings)
        {
            if (c.Id == id)
            {
                return c;
            }
        }

        return null;
    }

    /// <summary>Where a point on the perimeter is, walking clockwise from the origin.</summary>
    public (double X, double Y) PointAt(double along)
    {
        var e = Extent;
        var t = Rem(along, Turns);
        var side = Math.Floor(t);
        var f = t - side;
        return (int)side switch
        {
            0 => (e * f, 0.0),
            1 => (e, e * f),
            2 => (e * (1.0 - f), e),
            _ => (0.0, e * (1.0 - f)),
        };
    }

    /// <summary>Which bloc holds the frontier at a point along it.</summary>
    public Market BlocAt(double along)
    {
        var t = Rem(along, Turns);
        var held = _arcs[^1].Bloc;
        foreach (var arc in _arcs)
        {
            if (arc.Start <= t)
            {
                held = arc.Bloc;
            }
        }

        return held;
    }

    /// <summary>Where on the perimeter a point is closest to.</summary>
    public double NearestAlong(double x, double y)
    {
        var e = Extent;
        var cx = Math.Clamp(x, 0.0, e);
        var cy = Math.Clamp(y, 0.0, e);

        // Distance to each of the four sides; the nearest wins, ties to the
        // lowest side so the answer is stable.
        Span<double> gap =
        [
            cy,
            e - cx,
            e - cy,
            cx,
        ];

        var best = 0;
        for (var i = 1; i < 4; i++)
        {
            if (gap[i] < gap[best])
            {
                best = i;
            }
        }

        return best switch
        {
            0 => cx / e,
            1 => 1.0 + (cy / e),
            2 => 2.0 + ((e - cx) / e),
            _ => 3.0 + ((e - cy) / e),
        };
    }

    /// <summary>How far a point is from the frontier — the distance to the nearest edge.</summary>
    public double DistanceFrom(double x, double y)
    {
        var e = Extent;
        return Math.Min(Math.Min(x, e - x), Math.Min(y, e - y));
    }

    public Market BlocNear(double x, double y) => BlocAt(NearestAlong(x, y));

    /// <summary>
    /// The nearest post, optionally only one of a given bloc — what an import
    /// policy resolves to when the player has not named one.
    /// </summary>
    public BorderCrossing? NearestCrossing(double x, double y, Market? bloc)
    {
        BorderCrossing? best = null;
        var bestGap = double.PositiveInfinity;
        foreach (var c in _crossings)
        {
            if (bloc is not null && c.Bloc != bloc)
            {
                continue;
            }

            var gap = Units.Distance(c.X, c.Y, x, y);
            if (gap < bestGap)
            {
                bestGap = gap;
                best = c;
            }
        }

        return best;
    }

    /// <summary>A point on the perimeter, pulled inside onto workable ground.</summary>
    private (double X, double Y) InsetPoint(double along, double inset)
    {
        var (x, y) = PointAt(along);
        var e = Extent;

        // Toward the middle, by the inset, on whichever axis the edge runs.
        var towardX = x <= 0.0 ? inset : x >= e ? -inset : 0.0;
        var towardY = y <= 0.0 ? inset : y >= e ? -inset : 0.0;
        return (Math.Clamp(x + towardX, 0.0, e), Math.Clamp(y + towardY, 0.0, e));
    }

    /// <summary>
    /// Euclidean remainder. C#'s <c>%</c> keeps the sign of the dividend, so a
    /// negative position would run off the start of the loop rather than round
    /// it.
    /// </summary>
    private static double Rem(double v, double m) => v - (m * Math.Floor(v / m));
}

/// <summary>
/// What the republic holds in each currency.
/// </summary>
/// <remarks>
/// Two purses, not one converted. A republic can be rich in roubles and unable
/// to buy a single western machine, and that is the shape of the trade game.
/// </remarks>
public sealed class Treasury
{
    private readonly double[] _held = new double[2];

    public double Of(Market market) => _held[(int)market];

    public void Add(Market market, double amount) => _held[(int)market] += amount;

    /// <summary>
    /// Take what is there, up to what was asked. Returns what was actually
    /// taken — a shortfall is a smaller payment, never a debt, the same rule the
    /// stockpiles follow.
    /// </summary>
    public double Take(Market market, double amount)
    {
        var got = Math.Min(_held[(int)market], amount);
        _held[(int)market] -= got;
        return got;
    }

    public bool CanAfford(Market market, double amount) => _held[(int)market] >= amount;

    public void Set(Market market, double amount) => _held[(int)market] = amount;
}

/// <summary>What a standing instruction to the customs houses says to do.</summary>
public enum TradeAction
{
    Buy,
    Sell,
}

/// <summary>
/// One standing instruction: buy or sell this, with this bloc.
/// </summary>
/// <remarks>
/// The <b>order</b> of the rules is the decision. When throughput or money runs
/// short the first rule is served first, which is why moving one up or down is a
/// command of its own rather than a re-send of the whole policy.
/// </remarks>
public readonly record struct TradeRule(int Resource, Market Market, TradeAction Action);

/// <summary>So much of a resource — a line in a bill of materials.</summary>
public readonly record struct Bill(int Resource, double Tonnes);

/// <summary>
/// One grade of way: what it carries, how fast, and what it costs by the
/// kilometre.
/// </summary>
/// <remarks>
/// A dirt track and a railway are the same kind of thing here, which is what
/// lets one order-a-way command serve both. <paramref name="Lamps"/> is refused
/// on anything but paved road — nobody hangs a street light over a railway.
/// </remarks>
public readonly record struct GradeDef(
    string Id,
    string Name,
    Medium Carries,
    double SpeedKph,
    double Labour,
    bool Lamps,
    Bill[] Materials);

/// <summary>
/// One kind of line: what it carries, how far, and what it loses on the way.
/// </summary>
/// <remarks>
/// Power and heat used to be quantities with no geography at all — a plant
/// anywhere lit everything. A line is what makes both physical, and
/// <paramref name="LossPerKm"/> is what makes where you put the plant matter.
/// </remarks>
public readonly record struct UtilityDef(
    string Id,
    string Name,
    double Labour,
    double Reach,
    double LossPerKm,
    double Throughput,
    int[] Carries,
    Bill[] Materials);
