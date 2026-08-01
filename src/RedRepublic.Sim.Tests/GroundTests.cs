namespace RedRepublic.Sim.Tests;

/// <summary>
/// The ground: how wet, how frozen, and what that costs to drive across.
/// </summary>
public sealed class GroundTests
{
    private static Climate Taiga => Fixtures.Tables.Climates.Single(c => c.Id == "Taiga");

    /// <summary>
    /// <b>The test this whole model exists for.</b>
    /// </summary>
    /// <remarks>
    /// <para>
    /// Nothing in the model mentions spring. Three rules — snow lies below
    /// freezing and melts above it, meltwater goes into the topsoil like rain,
    /// and frost lags the air — and the worst going of the year has to fall out
    /// of running them through a winter.
    /// </para>
    /// <para>
    /// This asserts the emergent property rather than any of the three rules:
    /// the mud peaks in <i>spring</i>, after the thaw begins, and it is worse
    /// than anything the summer rain manages. A tuning pass that broke the
    /// carry — say by making frost follow the air instantly — would leave every
    /// other test in this file green and quietly delete the seasonal event.
    /// </para>
    /// </remarks>
    [Fact]
    public void A_winter_produces_the_spring_thaw_without_being_told_to()
    {
        var t = Fixtures.Tables;
        var climate = Taiga;
        var rng = Rng.FromSeed(1961);
        var ground = new Ground();

        var softness = new double[SimClock.DaysPerYear];
        var snow = new double[SimClock.DaysPerYear];

        // Two years: the first settles the state out of its founding guess, the
        // second is the one measured.
        for (var year = 0; year < 2; year++)
        {
            for (var day = 0; day < SimClock.DaysPerYear; day++)
            {
                var temperature = Weather.TemperatureOn(climate, day, rng.NextDouble());
                var rain = Weather.PrecipitationOn(climate, day, rng.NextDouble(), t);
                ground.Advance(temperature, rain, t);
                if (year == 1)
                {
                    softness[day] = ground.Softness;
                    snow[day] = ground.Snow;
                }
            }
        }

        // Snow accumulates over winter and goes in spring.
        var midWinterSnow = snow[15];
        Assert.True(midWinterSnow > 0.0, "a taiga winter should lie snow");

        var worstDay = Array.IndexOf(softness, softness.Max());
        var worstMonth = SimClock.MonthOfDayOfYear(worstDay);

        // March, April or May: after the air has turned but while the frost is
        // still on its way out.
        Assert.True(
            worstMonth is >= 3 and <= 5,
            $"the worst going landed in month {worstMonth} (day {worstDay}), not in spring");

        // And it is genuinely worse than high summer, when the same climate is
        // at its wettest.
        var summerWorst = softness[150..240].Max();
        Assert.True(
            softness[worstDay] > summerWorst,
            $"spring {softness[worstDay]:F3} was no worse than summer {summerWorst:F3}");

        // The pack is gone by the time the mud peaks — that is what put the
        // water in the topsoil.
        Assert.Equal(0.0, snow[worstDay]);
    }

    /// <summary>
    /// Frozen ground is hard however wet it is. A frozen bog is a road, which is
    /// why winter haulage across country is easier than spring haulage.
    /// </summary>
    [Fact]
    public void Frozen_ground_is_firm_however_wet_it_is()
    {
        var soaked = new Ground { Moisture = 1.0, Frost = 0.0 };
        var frozenSolid = new Ground { Moisture = 1.0, Frost = 1.0 };
        var halfFrozen = new Ground { Moisture = 1.0, Frost = 0.5 };

        Assert.Equal(1.0, soaked.Softness);
        Assert.Equal(0.0, frozenSolid.Softness);
        Assert.Equal(0.5, halfFrozen.Softness);
    }

    /// <summary>
    /// Snow lies while it is below freezing and runs into the ground above it.
    /// Rain below freezing must not wet the soil — it is falling as snow.
    /// </summary>
    [Fact]
    public void Snow_lies_in_the_cold_and_melts_into_the_soil_in_the_warm()
    {
        var t = Fixtures.Tables;
        var g = new Ground { Moisture = 0.0, Water = 0.0, Snow = 0.0, Frost = 0.0 };

        g.Advance(-10.0, 20.0, t);
        Assert.Equal(20.0, g.Snow);
        Assert.Equal(0.0, g.Moisture);

        // Warm, no further precipitation: the pack gives up water into the soil.
        var before = g.Snow;
        g.Advance(5.0, 0.0, t);
        Assert.True(g.Snow < before);
        Assert.True(g.Moisture > 0.0);

        // It cannot melt more than is lying.
        for (var i = 0; i < 50; i++)
        {
            g.Advance(20.0, 0.0, t);
        }

        Assert.Equal(0.0, g.Snow);
    }

    /// <summary>
    /// The topsoil and the root zone answer different questions. The root zone
    /// carries a crop through a dry fortnight that leaves the surface bone dry —
    /// which is why they are peer fields rather than one derived from the other.
    /// </summary>
    [Fact]
    public void The_root_zone_outlasts_the_topsoil()
    {
        var t = Fixtures.Tables;
        var g = new Ground { Moisture = 1.0, Water = 1.0, Snow = 0.0, Frost = 0.0 };

        for (var day = 0; day < 14; day++)
        {
            g.Advance(20.0, 0.0, t);
        }

        Assert.Equal(0.0, g.Moisture);
        Assert.True(g.Water > 0.5, $"the root zone should still be carrying a crop, got {g.Water:F3}");
    }

    /// <summary>Nothing here can leave its range, however extreme the weather.</summary>
    [Fact]
    public void The_state_stays_within_its_bounds()
    {
        var t = Fixtures.Tables;
        var g = new Ground();
        var rng = Rng.FromSeed(7);

        for (var day = 0; day < 2000; day++)
        {
            // Deliberately absurd: forty degrees either way and a monsoon.
            g.Advance(rng.NextRange(-40.0, 40.0), rng.NextRange(0.0, 200.0), t);
            Assert.InRange(g.Moisture, 0.0, 1.0);
            Assert.InRange(g.Water, 0.0, 1.0);
            Assert.InRange(g.Frost, 0.0, 1.0);
            Assert.True(g.Snow >= 0.0);
            Assert.InRange(g.Softness, 0.0, 1.0);
        }
    }

    /// <summary>
    /// A forecast rolls the recurrence forward and moves nothing. It can only
    /// exist because temperature and rain are pure functions of the day.
    /// </summary>
    [Fact]
    public void A_forecast_does_not_disturb_the_present()
    {
        var t = Fixtures.Tables;
        var g = new Ground { Moisture = 0.4, Water = 0.6, Snow = 30.0, Frost = 0.8 };

        var ahead = g.Forecast(_ => (10.0, 0.0), 0, 10, t);

        Assert.Equal(0.4, g.Moisture);
        Assert.Equal(30.0, g.Snow);
        Assert.True(ahead.Snow < g.Snow);
        Assert.True(ahead.Frost < g.Frost);
    }

    /// <summary>
    /// A cell that is a quarter water is water: you cannot drive round the
    /// corner of a lake inside a hundred-metre square.
    /// </summary>
    [Fact]
    public void A_cell_that_is_a_quarter_water_is_impassable()
    {
        var t = Fixtures.Tables;
        var terrain = Terrain.Flat(1000.0, 10.0);
        var lattice = Lattice.FromTerrain(terrain, t);

        Assert.Equal(10, lattice.Cells);
        Assert.Equal(1.0, lattice.SurfaceAt(0));

        // Flood a third of the first cell and it stops being crossable.
        for (var y = 0; y < 100; y += 10)
        {
            for (var x = 0; x < 40; x += 10)
            {
                terrain.SetSurfaceAt(x + 5, y + 5, Surface.Water);
            }
        }

        var flooded = Lattice.FromTerrain(terrain, t);
        Assert.True(double.IsInfinity(flooded.SurfaceAt(0)));
    }

    /// <summary>
    /// Snow does not know where the roads are; the plough does. And when the
    /// pack goes, the whole lattice is clear again — a road ploughed last
    /// February is not still credited for it next December.
    /// </summary>
    [Fact]
    public void Snow_buries_everything_and_the_plough_clears_one_cell()
    {
        var t = Fixtures.Tables;
        var lattice = Lattice.FromTerrain(Terrain.Flat(1000.0, 10.0), t);

        Assert.Equal(0.0, lattice.BuriedShare());

        lattice.Bury(0.5);
        Assert.Equal(0.5, lattice.BuriedShare(), 6);
        Assert.Equal(0.5, lattice.ClearedAt(0), 6);

        lattice.Clear(0);
        Assert.Equal(1.0, lattice.ClearedAt(0));
        Assert.Equal(0.5, lattice.ClearedAt(1), 6);

        lattice.Thaw();
        Assert.Equal(0.0, lattice.BuriedShare());
    }

    /// <summary>
    /// Wear grows with traffic and fades without it, so a corridor has to be
    /// kept. Without the fade every line any lorry ever drove is permanent and
    /// the map fills with the ghosts of routes nobody uses.
    /// </summary>
    [Fact]
    public void A_track_has_to_be_kept()
    {
        var t = Fixtures.Tables;
        var lattice = Lattice.FromTerrain(Terrain.Flat(1000.0, 10.0), t);

        // A season's traffic makes a track.
        for (var pass = 0; pass < 50; pass++)
        {
            lattice.WearIn(0, t.WearPerPass);
        }

        // Most of the way to a made track, which is what fifty laden passes is
        // authored to buy. The figure it used to be compared against —
        // `promote_at`, a threshold at which a worn corridor became a road on
        // its own — has gone: nothing read it, and building the mechanic it
        // implies is not a gap to close but a game to invent.
        Assert.True(lattice.WearAt(0) >= 0.8);

        // Abandoned, it goes back to field.
        for (var day = 0; day < 200; day++)
        {
            lattice.Fade(t.WearFadePerDay);
        }

        Assert.Equal(0.0, lattice.WearAt(0));
    }

    /// <summary>
    /// Pollution decays proportionally, not by a flat subtraction. A flat rate
    /// makes the steady state a step function — every source below the rate
    /// settles at exactly clean and every source above it at exactly filthy — so
    /// a brickworks and a steel works would look identical.
    /// </summary>
    [Fact]
    public void Dirty_places_settle_at_their_own_level()
    {
        var t = Fixtures.Tables;
        var lattice = Lattice.FromTerrain(Terrain.Flat(1000.0, 10.0), t);

        // Two sources of different strength, run to steady state.
        for (var day = 0; day < 500; day++)
        {
            lattice.Foul(0, 0.02);
            lattice.Foul(1, 0.10);
            lattice.Disperse(0.1);
        }

        var light = lattice.PollutionAt(0);
        var heavy = lattice.PollutionAt(1);

        Assert.True(light > 0.0);
        Assert.True(heavy > light, $"a heavier source should settle dirtier: {heavy:F3} vs {light:F3}");
        Assert.True(heavy <= 1.0);

        // And a works pulled down comes genuinely clean rather than approaching
        // it for ever.
        for (var day = 0; day < 500; day++)
        {
            lattice.Disperse(0.1);
        }

        Assert.Equal(0.0, lattice.PollutionAt(0));
        Assert.Equal(0.0, lattice.PollutionAt(1));
    }

    [Fact]
    public void Cells_within_a_radius_are_a_disc_around_the_point()
    {
        var t = Fixtures.Tables;
        var lattice = Lattice.FromTerrain(Terrain.Flat(1000.0, 10.0), t);

        var here = lattice.CellsWithin(450.0, 450.0, 0.0);
        Assert.Single(here);

        var near = lattice.CellsWithin(450.0, 450.0, 250.0);
        Assert.True(near.Count > 8);
        foreach (var c in near)
        {
            Assert.True(
                Units.Distance(lattice.CentreX(c), lattice.CentreY(c), 450.0, 450.0) <= 250.0);
        }

        Assert.Empty(lattice.CellsWithin(-50.0, 0.0, 100.0));
    }
}
