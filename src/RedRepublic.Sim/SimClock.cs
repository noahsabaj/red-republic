namespace RedRepublic.Sim;

public enum Season
{
    Winter,
    Spring,
    Summer,
    Autumn,
}

/// <summary>A calendar date in the republic's 360-day year.</summary>
public readonly record struct Date(int Year, int Month, int Day)
{
    /// <summary>Days since 1 January 1960.</summary>
    public long DayIndex =>
        ((long)(Year - SimClock.EpochYear) * SimClock.DaysPerYear)
        + ((long)(Month - 1) * SimClock.DaysPerMonth)
        + (Day - 1);

    /// <summary>The inverse of <see cref="DayIndex"/>.</summary>
    public static Date FromDayIndex(long index)
    {
        var year = SimClock.EpochYear + (int)(index / SimClock.DaysPerYear);
        var withinYear = (int)(index % SimClock.DaysPerYear);
        return new Date(
            year,
            (withinYear / SimClock.DaysPerMonth) + 1,
            (withinYear % SimClock.DaysPerMonth) + 1);
    }

    public Season Season => Month switch
    {
        12 or 1 or 2 => Sim.Season.Winter,
        >= 3 and <= 5 => Sim.Season.Spring,
        >= 6 and <= 8 => Sim.Season.Summer,
        _ => Sim.Season.Autumn,
    };
}

/// <summary>
/// Simulated time: a fixed timestep, and the calendar derived from it.
/// </summary>
/// <remarks>
/// <para>
/// The simulation advances in whole ticks of <see cref="Tick"/> and never in
/// variable real-time deltas. A fixed timestep is not a style preference — it is
/// what makes a run reproducible, because a variable step makes the result a
/// function of how busy the machine was.
/// </para>
/// <para>
/// <b>How many ticks pass per real second is not this class's business.</b> That
/// is a game-speed control and it belongs to the shell. The simulation only
/// knows how to take one step.
/// </para>
/// <para>
/// Twelve thirty-day months, a 360-day year, founding on 1 March 1960. A
/// simplification, deliberately: equal months make seasonal balance legible, and
/// every balance figure in the manifest was tuned against this calendar.
/// Changing it would silently invalidate all of them.
/// </para>
/// </remarks>
public sealed class SimClock
{
    /// <summary>
    /// The fixed simulation timestep, in seconds — one simulated minute.
    /// </summary>
    /// <remarks>
    /// Fine enough that a commute is felt (a lorry at 54 km/h covers 900 m a
    /// step) and coarse enough that a day is 1,440 steps rather than tens of
    /// thousands.
    /// </remarks>
    public const double Tick = 60.0;

    /// <summary>Ticks in one simulated day.</summary>
    public const long TicksPerDay = 1_440;

    public const int DaysPerMonth = 30;
    public const int MonthsPerYear = 12;
    public const int DaysPerYear = DaysPerMonth * MonthsPerYear;

    /// <summary>The year day indices count from. Day index 0 is 1 January 1960.</summary>
    public const int EpochYear = 1960;

    /// <summary>
    /// Founding is 1 March 1960 — day index 60, not 0. The archived build's
    /// contract deadlines were absolute indices against this origin, and the
    /// ported balance figures assume it.
    /// </summary>
    public const long FoundingDayIndex = 60;

    /// <summary>
    /// Ticks elapsed since founding. Monotonic, and the single source of "when
    /// it is" — nothing else may keep its own notion of elapsed time.
    /// </summary>
    public long Ticks { get; private set; }

    /// <summary>Take one step. The only way time moves.</summary>
    public void Advance() => Ticks++;

    /// <summary>
    /// Take several steps at once. Identical in effect to calling
    /// <see cref="Advance"/> that many times — it must stay that way, or
    /// fast-forwarding would diverge from playing.
    /// </summary>
    public void AdvanceBy(long ticks) => Ticks += ticks;

    /// <summary>Simulated seconds since founding.</summary>
    public double Elapsed => Tick * Ticks;

    /// <summary>Whole days since founding.</summary>
    public long DaysElapsed => Ticks / TicksPerDay;

    /// <summary>Days since 1 January 1960 — the index balance tables and deadlines use.</summary>
    public long DayIndex => FoundingDayIndex + DaysElapsed;

    public Date Date => Date.FromDayIndex(DayIndex);

    /// <summary>
    /// Days since 1 January of the current year — what the climate curve is a
    /// function of, so it counts from January and not from founding.
    /// </summary>
    public int DayOfYear => (int)(DayIndex % DaysPerYear);

    public Season Season => Date.Season;

    /// <summary>
    /// Seconds since midnight. Sub-day resolution is the reason the tick is a
    /// minute rather than a day: a commute that happens at a time of day is the
    /// whole point of simulating citizens individually.
    /// </summary>
    public double TimeOfDay => Tick * (Ticks % TicksPerDay);

    /// <summary>True on the tick that starts a new day — where daily bookkeeping hangs.</summary>
    public bool IsDayBoundary => Ticks % TicksPerDay == 0;
}
