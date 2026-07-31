namespace RedRepublic.Sim.Tests;

/// <summary>The calendar, and the fixed timestep under it.</summary>
public sealed class ClockTests
{
    [Fact]
    public void Founding_is_the_first_of_march_nineteen_sixty()
    {
        var c = new SimClock();

        Assert.Equal(new Date(1960, 3, 1), c.Date);

        // The archived build's day index put founding at 60, and contract
        // deadlines were absolute indices against it. Ported balance figures
        // assume this origin, so it is pinned rather than derived.
        Assert.Equal(SimClock.FoundingDayIndex, c.DayIndex);
        Assert.Equal(0.0, c.TimeOfDay);
        Assert.Equal(Season.Spring, c.Season);
    }

    [Fact]
    public void A_day_is_exactly_one_thousand_four_hundred_and_forty_ticks()
    {
        var c = new SimClock();
        for (var i = 0; i < SimClock.TicksPerDay - 1; i++)
        {
            c.Advance();
        }

        Assert.Equal(0, c.DaysElapsed);
        Assert.False(c.IsDayBoundary);

        c.Advance();
        Assert.Equal(1, c.DaysElapsed);
        Assert.True(c.IsDayBoundary);
        Assert.Equal(0.0, c.TimeOfDay);
    }

    /// <summary>
    /// Fast-forwarding must not diverge from playing. If these ever disagree, a
    /// republic advanced at speed 5 is a different republic from one advanced at
    /// speed 1, and nothing else would say so.
    /// </summary>
    [Fact]
    public void Advancing_in_bulk_matches_advancing_one_at_a_time()
    {
        var a = new SimClock();
        var b = new SimClock();
        for (var i = 0; i < 5000; i++)
        {
            a.Advance();
        }

        b.AdvanceBy(5000);

        Assert.Equal(a.Ticks, b.Ticks);
        Assert.Equal(a.DayIndex, b.DayIndex);
        Assert.Equal(a.TimeOfDay, b.TimeOfDay);
    }

    /// <summary>The calendar round-trips. Every day of a decade, out to a date and back.</summary>
    [Fact]
    public void A_date_survives_the_round_trip()
    {
        for (var index = 0; index < 3600; index++)
        {
            var d = Date.FromDayIndex(index);
            Assert.InRange(d.Month, 1, 12);
            Assert.InRange(d.Day, 1, 30);
            Assert.Equal(index, d.DayIndex);
        }
    }

    [Fact]
    public void The_year_is_three_hundred_and_sixty_days()
    {
        Assert.Equal(360, SimClock.DaysPerYear);
        Assert.Equal(1960, Date.FromDayIndex(0).Year);
        Assert.Equal(1960, Date.FromDayIndex(359).Year);
        Assert.Equal(1961, Date.FromDayIndex(360).Year);
    }

    [Theory]
    [InlineData(12, Season.Winter)]
    [InlineData(1, Season.Winter)]
    [InlineData(2, Season.Winter)]
    [InlineData(3, Season.Spring)]
    [InlineData(5, Season.Spring)]
    [InlineData(6, Season.Summer)]
    [InlineData(8, Season.Summer)]
    [InlineData(9, Season.Autumn)]
    [InlineData(11, Season.Autumn)]
    public void The_seasons_fall_where_the_calendar_puts_them(int month, Season season) =>
        Assert.Equal(season, new Date(1960, month, 1).Season);

    /// <summary>
    /// The day of year is what the climate curve is a function of, so it counts
    /// from January and not from founding.
    /// </summary>
    [Fact]
    public void The_day_of_year_counts_from_january()
    {
        var c = new SimClock();
        Assert.Equal(60, c.DayOfYear);

        c.AdvanceBy(SimClock.TicksPerDay * 300);
        Assert.Equal(0, c.DayOfYear);
        Assert.Equal(1961, c.Date.Year);
    }
}

/// <summary>The conversions, and how geometry is computed.</summary>
public sealed class UnitsTests
{
    [Fact]
    public void Speed_converts_both_ways()
    {
        Assert.Equal(1.0, Units.KphToMps(3.6));
        Assert.Equal(3.6, Units.MpsToKph(1.0));
        Assert.Equal(50.0, Units.MpsToKph(Units.KphToMps(50.0)), 12);
    }

    [Fact]
    public void Time_to_cover_is_distance_over_speed()
    {
        Assert.Equal(10.0, Units.TimeToCover(10.0, 100.0));

        // 900 m at 54 km/h is exactly one tick, which is where the tick length
        // came from.
        Assert.Equal(60.0, Units.TimeToCover(Units.KphToMps(54.0), 900.0));
    }

    /// <summary>
    /// A stationary thing never arrives. Returning an infinity would propagate
    /// into arrival times and schedules as a silently poisoned number.
    /// </summary>
    [Fact]
    public void A_zero_speed_is_refused() =>
        Assert.Throws<ArgumentOutOfRangeException>(() => Units.TimeToCover(0.0, 100.0));

    [Fact]
    public void Durations_convert_both_ways()
    {
        Assert.Equal(60.0, Units.Minutes(1.0));
        Assert.Equal(3600.0, Units.Hours(1.0));
        Assert.Equal(86400.0, Units.Days(1.0));
        Assert.Equal(7.0, Units.AsHours(Units.Hours(7.0)));
        Assert.Equal(365.0, Units.AsDays(Units.Days(365.0)));
    }

    [Fact]
    public void Distance_is_the_pythagorean_one()
    {
        Assert.Equal(5.0, Units.Distance(0.0, 0.0, 3.0, 4.0));
        Assert.Equal(0.0, Units.Distance(1.0, 1.0, 1.0, 1.0));
        Assert.Equal(25.0, Units.DistanceSquared(0.0, 0.0, 3.0, 4.0));
    }

    /// <summary>
    /// The determinism rule, made concrete.
    /// </summary>
    /// <remarks>
    /// A 32-bit float cannot hold a map-scale position. At a 6 km posting with
    /// positions in metres, the rounding is visible in the seventh digit, which
    /// is exactly where a bit-exact save stops being bit-exact. This project has
    /// no reference to Godot and so cannot name <c>Vector2</c> at all, but the
    /// arithmetic it would do is reproducible here — and this is what says the
    /// difference is real rather than theoretical.
    /// </remarks>
    [Fact]
    public void A_thirty_two_bit_position_would_lose_the_point()
    {
        const double x = 5432.109876543210;

        var rounded = (float)x;
        Assert.NotEqual(x, rounded);
        Assert.True(Units.Distance(x, 0.0, rounded, 0.0) > 0.0);

        // And the double path is exact.
        Assert.Equal(0.0, Units.Distance(x, 0.0, x, 0.0));
    }
}

/// <summary>Stockpiles clamp at zero, and the flat layout addresses the right slot.</summary>
public sealed class StockTests
{
    private static GrowableStock Make(int buildings)
    {
        var s = new GrowableStock(Fixtures.Tables.Resources.Length);
        for (var i = 0; i < buildings; i++)
        {
            s.Grow();
        }

        return s;
    }

    [Fact]
    public void A_fresh_stockpile_is_empty()
    {
        var s = Make(4);
        Assert.Equal(4 * Fixtures.Tables.Resources.Length, s.All.Length);
        for (var b = 0; b < 4; b++)
        {
            Assert.True(s.IsEmpty(b));
            Assert.Equal(0.0, s.Total(b));
        }
    }

    /// <summary>
    /// The layout check. A flat array addressed with the wrong stride is the
    /// defect that puts a mine's coal into the building next door, and it reads
    /// as a balance problem for weeks.
    /// </summary>
    [Fact]
    public void Each_building_holds_its_own()
    {
        var t = Fixtures.Tables;
        var coal = t.ResourceIndex("Coal");
        var steel = t.ResourceIndex("Steel");
        var s = Make(3);

        s.Add(1, coal, 50.0);

        Assert.Equal(50.0, s.Get(1, coal));
        Assert.Equal(0.0, s.Get(0, coal));
        Assert.Equal(0.0, s.Get(2, coal));
        Assert.Equal(0.0, s.Get(1, steel));
        Assert.Equal(50.0, s.Total(1));
    }

    /// <summary>The rule: a shortfall is a smaller delivery, not a debt.</summary>
    [Fact]
    public void Stock_never_goes_negative()
    {
        var coal = Fixtures.Tables.ResourceIndex("Coal");
        var s = Make(1);
        s.Add(0, coal, 10.0);

        Assert.Equal(4.0, s.Take(0, coal, 4.0));
        Assert.Equal(6.0, s.Get(0, coal));

        Assert.Equal(6.0, s.Take(0, coal, 100.0));
        Assert.Equal(0.0, s.Get(0, coal));
        Assert.False(s.Has(0, coal));

        s.Add(0, coal, -50.0);
        Assert.Equal(0.0, s.Get(0, coal));
        s.Set(0, coal, -1.0);
        Assert.Equal(0.0, s.Get(0, coal));
    }

    [Fact]
    public void A_row_reads_back_in_resource_order()
    {
        var n = Fixtures.Tables.Resources.Length;
        var s = Make(2);
        for (var r = 0; r < n; r++)
        {
            s.Add(1, r, r + 1.0);
        }

        var row = s.Row(1);
        Assert.Equal(n, row.Length);
        for (var r = 0; r < n; r++)
        {
            Assert.Equal(r + 1.0, row[r]);
        }

        Assert.Equal(0.0, s.Total(0));
    }
}
