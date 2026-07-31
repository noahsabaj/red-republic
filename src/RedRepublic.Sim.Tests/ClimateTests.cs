namespace RedRepublic.Sim.Tests;

/// <summary>
/// The weather: the authored year, the day's own swing, and what it costs to
/// heat against.
/// </summary>
public sealed class ClimateTests
{
    private static Climate Of(string id) =>
        Fixtures.Tables.Climates.Single(c => c.Id == id);

    [Fact]
    public void The_four_postings_are_all_here()
    {
        var t = Fixtures.Tables;
        Assert.Equal(4, t.Climates.Length);
        Assert.Equal(
            ["Plains", "Taiga", "Steppe", "Maritime"],
            t.Climates.Select(c => c.Id).ToArray());
        Assert.Equal("Central Plains", Of("Plains").Name);
    }

    /// <summary>
    /// The postings are genuinely different problems, which is the point of
    /// there being four. A tuning pass that flattened them would still pass
    /// every other test in this file.
    /// </summary>
    [Fact]
    public void The_postings_differ_in_the_ways_that_matter()
    {
        var taiga = Of("Taiga");
        var maritime = Of("Maritime");
        var steppe = Of("Steppe");

        // The taiga is the cold one: twenty-six degrees colder in the worst
        // month than the maritime posting.
        Assert.Equal(-24.0, taiga.ColdestMeanC);
        Assert.Equal(2.0, maritime.ColdestMeanC);
        Assert.True(taiga.ColdestMeanC < steppe.ColdestMeanC);

        // And cold and dry against mild and wet: the taiga's wettest month is
        // drier than the maritime's driest.
        Assert.True(taiga.MonthlyRainMm.Max() > maritime.MonthlyRainMm.Min());
        Assert.True(maritime.MonthlyRainMm.Sum() > taiga.MonthlyRainMm.Sum());

        // The maritime posting swings least day to day, which is what maritime
        // means.
        Assert.Equal(4.0, maritime.DailySwingC);
        Assert.True(maritime.DailySwingC < taiga.DailySwingC);
    }

    /// <summary>
    /// The curve passes through the authored figures at the middle of each
    /// month, and wraps: a 1 January a whole degree from 31 December would be a
    /// calendar artefact, which is what the interpolation exists to avoid.
    /// </summary>
    [Fact]
    public void The_year_is_a_curve_and_not_twelve_plateaus()
    {
        var c = Of("Plains");

        // Day 15 is the middle of January, so it reads the authored figure back
        // exactly.
        Assert.Equal(c.MonthlyMeanC[0], c.MeanOn(15), 12);

        // Day 45 is the middle of February.
        Assert.Equal(c.MonthlyMeanC[1], c.MeanOn(45), 12);

        // And the year joins up: the step from the last day to the first is no
        // larger than any other step.
        var lastToFirst = Math.Abs(c.MeanOn(0) - c.MeanOn(SimClock.DaysPerYear - 1));
        var midYearStep = Math.Abs(c.MeanOn(180) - c.MeanOn(179));
        Assert.True(
            lastToFirst < midYearStep + 0.05,
            $"the year does not join up: {lastToFirst} across new year, {midYearStep} mid-year");

        // Every day of the year sits within the authored range.
        for (var d = 0; d < SimClock.DaysPerYear; d++)
        {
            Assert.InRange(c.MeanOn(d), c.ColdestMeanC, c.WarmestMeanC);
        }
    }

    /// <summary>
    /// The wrap is the half a naive remainder gets wrong. C#'s <c>%</c> keeps
    /// the sign of the dividend, so the first fortnight of January would index
    /// backwards through the table without the Euclidean form.
    /// </summary>
    [Fact]
    public void The_first_fortnight_of_january_reads_december()
    {
        var c = Of("Plains");

        // Day 0 is half a month before January's midpoint, so it sits halfway
        // between December and January — colder than January's mean here.
        var newYear = c.MeanOn(0);
        Assert.InRange(newYear, Math.Min(c.MonthlyMeanC[11], c.MonthlyMeanC[0]),
            Math.Max(c.MonthlyMeanC[11], c.MonthlyMeanC[0]));
        Assert.Equal((c.MonthlyMeanC[11] + c.MonthlyMeanC[0]) / 2.0, newYear, 12);
    }

    /// <summary>
    /// A day's weather is the mean plus its own swing, and the draw spans the
    /// authored range exactly — no more and no less.
    /// </summary>
    [Fact]
    public void A_days_temperature_is_the_mean_plus_its_swing()
    {
        var c = Of("Plains");
        const int day = 15;

        Assert.Equal(c.MeanOn(day) - c.DailySwingC, Weather.TemperatureOn(c, day, 0.0), 12);
        Assert.Equal(c.MeanOn(day), Weather.TemperatureOn(c, day, 0.5), 12);
        Assert.Equal(c.MeanOn(day) + c.DailySwingC, Weather.TemperatureOn(c, day, 1.0), 12);
    }

    /// <summary>
    /// Heating is temperature-driven rather than seasonal so the demand curve
    /// can spike: mild days sip fuel and deep cold over-drives.
    /// </summary>
    [Fact]
    public void Heat_demand_spikes_in_deep_cold_and_stops_when_it_is_mild()
    {
        var t = Fixtures.Tables;

        Assert.False(Weather.HeatingRequired(t.HeatThresholdC, t));
        Assert.False(Weather.HeatingRequired(20.0, t));
        Assert.True(Weather.HeatingRequired(t.HeatThresholdC - 0.1, t));

        Assert.Equal(0.0, Weather.HeatDemandFactor(20.0, t));
        Assert.Equal(0.0, Weather.HeatDemandFactor(t.HeatThresholdC, t));

        // At the design-cold day, demand is nominal.
        Assert.Equal(1.0, Weather.HeatDemandFactor(t.HeatDesignC, t), 12);

        // Half way between threshold and design is half nominal.
        var half = (t.HeatThresholdC + t.HeatDesignC) / 2.0;
        Assert.Equal(0.5, Weather.HeatDemandFactor(half, t), 12);

        // And past the design day it over-drives, up to the ceiling and no
        // further — a boiler house cannot be asked for unbounded output.
        Assert.True(Weather.HeatDemandFactor(t.HeatDesignC - 5.0, t) > 1.0);
        Assert.Equal(t.HeatDemandCeiling, Weather.HeatDemandFactor(-100.0, t));
    }

    /// <summary>
    /// Rain is bursty, and the burstiness conserves the authored monthly mean
    /// exactly. That is the property worth checking: a distribution that made
    /// wet days dramatic but changed the month's total would silently re-balance
    /// every farm in the game.
    /// </summary>
    [Fact]
    public void Bursty_rain_still_averages_to_the_authored_month()
    {
        var t = Fixtures.Tables;
        var c = Of("Maritime");
        const int day = 15;

        // Most days are dry.
        Assert.Equal(0.0, Weather.PrecipitationOn(c, day, t.WetDayShare, t));
        Assert.Equal(0.0, Weather.PrecipitationOn(c, day, 0.9, t));
        Assert.True(Weather.PrecipitationOn(c, day, 0.0, t) > 0.0);

        // The wettest draw is several times the daily mean — the wet week the
        // mechanic exists to produce.
        Assert.True(Weather.PrecipitationOn(c, day, 0.0, t) > c.RainOn(day) * 3.0);

        // And the expectation over a uniform draw is the authored mean. Sampled
        // on a fixed lattice rather than randomly, so this is exact and not a
        // test that fails one run in fifty.
        const int samples = 100_000;
        var total = 0.0;
        for (var i = 0; i < samples; i++)
        {
            total += Weather.PrecipitationOn(c, day, (i + 0.5) / samples, t);
        }

        Assert.Equal(c.RainOn(day), total / samples, 6);
    }
}
