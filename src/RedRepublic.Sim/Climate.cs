namespace RedRepublic.Sim;

/// <summary>
/// One posting's authored year: what the weather is like here.
/// </summary>
/// <remarks>
/// Temperature and rainfall are authored together rather than as two dials,
/// because the two together are what make a climate <i>somewhere</i>: the taiga
/// is cold and dry, the maritime posting is mild and wet, and those are
/// different problems.
/// </remarks>
public sealed class Climate
{
    internal Climate(string id, string name, double[] monthlyMeanC, double dailySwingC, double[] monthlyRainMm)
    {
        Id = id;
        Name = name;
        MonthlyMeanC = monthlyMeanC;
        DailySwingC = dailySwingC;
        MonthlyRainMm = monthlyRainMm;
    }

    public string Id { get; }

    public string Name { get; }

    /// <summary>Mean temperature in each month, January first, in Celsius.</summary>
    public double[] MonthlyMeanC { get; }

    /// <summary>How far a single day may stray from its month's mean, either way.</summary>
    public double DailySwingC { get; }

    /// <summary>Mean precipitation in each month, January first, in mm per day.</summary>
    public double[] MonthlyRainMm { get; }

    /// <summary>
    /// The coldest month's mean — what a founding briefing quotes, because it is
    /// the number that decides how much coal a winter costs.
    /// </summary>
    public double ColdestMeanC => MonthlyMeanC.Min();

    public double WarmestMeanC => MonthlyMeanC.Max();

    /// <summary>
    /// The seasonal mean on a given day of the year, interpolated between the
    /// two months either side of it.
    /// </summary>
    public double MeanOn(int dayOfYear) => ReadMonthly(MonthlyMeanC, dayOfYear);

    /// <summary>The seasonal mean rainfall on a given day, in millimetres.</summary>
    public double RainOn(int dayOfYear) => ReadMonthly(MonthlyRainMm, dayOfYear);

    /// <summary>
    /// Read a twelve-month table on a given day of the year, interpolating
    /// between the two months either side of it.
    /// </summary>
    /// <remarks>
    /// Each authored figure sits at the <b>middle</b> of its month, so the curve
    /// passes through the table rather than stepping between plateaus. A 1
    /// January a whole degree different from 31 December would be a calendar
    /// artefact, and those are what this exists to avoid — which is why the
    /// position wraps rather than clamping at the ends of the table.
    /// </remarks>
    internal static double ReadMonthly(double[] table, int dayOfYear)
    {
        var day = (double)(dayOfYear % SimClock.DaysPerYear);
        var position = (day / SimClock.DaysPerMonth) - 0.5;
        const double months = SimClock.MonthsPerYear;

        // Euclidean remainder, so late December interpolates into January rather
        // than falling off the end. C#'s `%` keeps the sign of the dividend, so
        // a plain `position % months` would go negative for the first fortnight
        // of January and index backwards.
        var wrapped = position - (months * Math.Floor(position / months));
        var index = (int)Math.Floor(wrapped);
        var frac = wrapped - index;
        var a = table[index % 12];
        var b = table[(index + 1) % 12];
        return a + ((b - a) * frac);
    }
}

/// <summary>
/// The weather model: what the day is like, and what that costs to heat.
/// </summary>
public static class Weather
{
    /// <summary>
    /// Today's temperature: the seasonal mean, plus a day's own weather.
    /// </summary>
    /// <remarks>
    /// <paramref name="deviation"/> is a draw in <c>[0, 1)</c> from the caller's
    /// weather substream — passed in rather than drawn here so this stays a pure
    /// function and the stream discipline stays visible at the call site.
    /// </remarks>
    public static double TemperatureOn(Climate climate, int dayOfYear, double deviation)
    {
        ArgumentNullException.ThrowIfNull(climate);
        var swing = ((deviation * 2.0) - 1.0) * climate.DailySwingC;
        return climate.MeanOn(dayOfYear) + swing;
    }

    /// <summary>Whether it is cold enough out that buildings need heating.</summary>
    public static bool HeatingRequired(double temperatureC, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        return temperatureC < tables.HeatThresholdC;
    }

    /// <summary>
    /// Share of nominal heat demand today, from zero to the ceiling.
    /// </summary>
    /// <remarks>
    /// Mild days sip fuel and deep cold over-drives, which is the whole reason
    /// heating is temperature-driven rather than seasonal: the demand curve has
    /// to be able to spike.
    /// </remarks>
    public static double HeatDemandFactor(double temperatureC, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        if (!HeatingRequired(temperatureC, tables))
        {
            return 0.0;
        }

        var span = tables.HeatThresholdC - tables.HeatDesignC;
        return Math.Min((tables.HeatThresholdC - temperatureC) / span, tables.HeatDemandCeiling);
    }

    /// <summary>
    /// Millimetres of rain today. Below freezing this is snow, which is the
    /// ground's business rather than the sky's.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Rain is bursty and that matters: a month's water smeared evenly over
    /// thirty days never saturates anything, while the same water in nine falls
    /// turns the ground to mud twice and dries out in between. The wet week is
    /// the event the mechanic exists to produce, and an average cannot produce
    /// one.
    /// </para>
    /// <para>
    /// The distribution preserves the authored monthly mean exactly. The
    /// <c>0.4 + 1.2x</c> shape averages to 1.0 over a uniform draw, so scaling
    /// the month's mean by <c>1 / wetDayShare</c> conserves it — most wet days
    /// are small, a few are not, and the month still totals what the table says.
    /// </para>
    /// </remarks>
    public static double PrecipitationOn(
        Climate climate, int dayOfYear, double roll, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(climate);
        ArgumentNullException.ThrowIfNull(tables);

        if (roll >= tables.WetDayShare)
        {
            return 0.0;
        }

        // Where in the wet share the draw landed, 1.0 at the wettest.
        var intensity = 1.0 - (roll / tables.WetDayShare);
        return climate.RainOn(dayOfYear) / tables.WetDayShare * (0.4 + (1.2 * intensity));
    }
}
