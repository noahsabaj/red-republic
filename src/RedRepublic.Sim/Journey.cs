namespace RedRepublic.Sim;

/// <summary>What a vehicle travels on.</summary>
/// <remarks>
/// The authored property that says which network a vehicle is allowed to look
/// at. A train routes over the rails and finds no road there.
/// </remarks>
public enum Medium
{
    Road,
    Rail,
    Tram,
    Metro,
    Water,
    Air,
}

/// <summary>
/// A journey under way: where it goes, what the way allows, and when each leg
/// ends.
/// </summary>
/// <remarks>
/// <para>
/// <b>Leg ends are absolute fractional ticks, so nothing drifts.</b> Storing a
/// remaining duration and decrementing it would accumulate a rounding error over
/// a long haul, and a lorry that arrives a few ticks late every trip is a
/// republic that quietly runs slower than its own timetable.
/// </para>
/// <para>
/// The position between leg ends is interpolated rather than stepped, so a
/// vehicle can be drawn at any fractional tick without the simulation having to
/// run at the frame rate.
/// </para>
/// </remarks>
public sealed class Journey
{
    /// <summary>Waypoints. Leg <c>i</c> runs from <c>Path[i]</c> to <c>Path[i + 1]</c>.</summary>
    private readonly double[] _x;

    private readonly double[] _y;

    /// <summary>
    /// What the way allows on each leg in metres per second, or a negative
    /// number where the leg crosses open ground.
    /// </summary>
    /// <remarks>
    /// A limit rather than a flag, because a road's whole point is that a better
    /// surface carries faster: a lorry on a dirt track makes 25 km/h whatever it
    /// is capable of, and the same lorry on tarmac makes its own fifty.
    /// </remarks>
    private readonly double[] _limit;

    private Journey(double[] x, double[] y, double[] limit, double now, double firstLeg)
    {
        _x = x;
        _y = y;
        _limit = limit;
        Leg = 0;
        LegStart = now;
        LegEnd = now + firstLeg;
    }

    /// <summary>Which leg is under way.</summary>
    public int Leg { get; private set; }

    /// <summary>When this leg began, as an absolute fractional tick.</summary>
    public double LegStart { get; private set; }

    /// <summary>When this leg is due to end. Absolute, so nothing drifts.</summary>
    public double LegEnd { get; private set; }

    public int Legs => _x.Length - 1;

    public bool OnLastLeg => Leg + 1 >= Legs;

    public static Journey Begin(
        IReadOnlyList<double> x, IReadOnlyList<double> y, IReadOnlyList<double> limit,
        double now, double firstLeg)
    {
        ArgumentNullException.ThrowIfNull(x);
        ArgumentNullException.ThrowIfNull(y);
        ArgumentNullException.ThrowIfNull(limit);
        if (x.Count < 2)
        {
            throw new ArgumentException("a journey needs somewhere to go", nameof(x));
        }

        if (limit.Count + 1 != x.Count || y.Count != x.Count)
        {
            throw new ArgumentException(
                "every leg needs to say what the way allows on it", nameof(limit));
        }

        return new Journey([.. x], [.. y], [.. limit], now, firstLeg);
    }

    public double LegFromX => _x[Leg];

    public double LegFromY => _y[Leg];

    public double LegToX => _x[Leg + 1];

    public double LegToY => _y[Leg + 1];

    public double DestinationX => _x[^1];

    public double DestinationY => _y[^1];

    public double LegDistance => Units.Distance(LegFromX, LegFromY, LegToX, LegToY);

    public double Distance
    {
        get
        {
            var total = 0.0;
            for (var i = 0; i + 1 < _x.Length; i++)
            {
                total += Units.Distance(_x[i], _y[i], _x[i + 1], _y[i + 1]);
            }

            return total;
        }
    }

    /// <summary>Whether this leg has a made way under it.</summary>
    public bool LegOnRoad(int leg) => _limit[leg] >= 0.0;

    /// <summary>
    /// How fast a vehicle goes on a leg: its own pace, held down by what the way
    /// allows and by how bad the going is.
    /// </summary>
    public double SpeedOn(int leg, double onRoad, double crossCountry, double drag)
    {
        var slow = Math.Max(1.0, drag);
        return _limit[leg] >= 0.0
            ? Math.Min(onRoad, _limit[leg]) / slow
            : crossCountry / slow;
    }

    public double LegSpeed(double onRoad, double crossCountry, double drag) =>
        SpeedOn(Leg, onRoad, crossCountry, drag);

    public bool LegDoneBy(double now) => now >= LegEnd;

    /// <summary>Move on to the next leg, at the speed it will be run at.</summary>
    public void Advance(double speed, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        if (OnLastLeg)
        {
            throw new InvalidOperationException("there is no leg after the last one");
        }

        Leg++;
        LegStart = LegEnd;
        LegEnd = LegStart + LegTicks(LegDistance, speed, t);
    }

    /// <summary>
    /// How many ticks a leg takes, never less than one.
    /// </summary>
    /// <remarks>
    /// A leg that takes no time at all would let a vehicle cross the whole
    /// republic inside one tick if its waypoints happened to be close together,
    /// which is how a fleet ends up teleporting.
    /// </remarks>
    public static double LegTicks(double metres, double speedMps, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        if (speedMps <= 0.0)
        {
            return t.MinLegTicks;
        }

        return Math.Max(t.MinLegTicks, metres / speedMps / SimClock.Tick);
    }

    /// <summary>
    /// Where it is at a fractional tick — interpolated along the leg, so a
    /// vehicle can be drawn between simulation steps.
    /// </summary>
    public (double X, double Y) PositionAt(double now)
    {
        var span = LegEnd - LegStart;
        var t = span > 0.0 ? Math.Clamp((now - LegStart) / span, 0.0, 1.0) : 1.0;
        return (LegFromX + ((LegToX - LegFromX) * t), LegFromY + ((LegToY - LegFromY) * t));
    }
}
