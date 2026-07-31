namespace RedRepublic.Sim.Tests;

/// <summary>
/// The ground under a republic reproduces the body the seed promises.
/// </summary>
public sealed class GeologyTests
{
    /// <summary>
    /// The cross-machine tripwire: every authored field of every body, in draw
    /// order, hashed by its bits.
    /// </summary>
    /// <remarks>
    /// It fails if the draw order changes, if the plan changes, or — the case it
    /// really exists for — if generation ever picks up a float operation allowed
    /// to differ in its last bit between platforms. Any of those means two
    /// players with the same seed get different republics.
    /// </remarks>
    [Fact]
    public void Generated_ground_matches_the_rust_reference()
    {
        var t = Fixtures.Tables;
        var rows = Fixtures.Vectors.GetProperty("geology");
        var checked_ = 0;

        foreach (var row in rows.EnumerateArray())
        {
            var seed = row.GetProperty("seed").GetUInt64();
            var extent = row.GetProperty("extent").GetDouble();
            var g = Geology.Generate(seed, extent, t);

            Assert.Equal(row.GetProperty("deposits").GetInt32(), g.All.Count);

            var h = new Fnv1a();
            var layers = 0;
            var total = 0.0;
            foreach (var d in g.All)
            {
                h.Push(d.Id);
                h.Push((int)d.Mineral);
                h.Push(d.CentreX);
                h.Push(d.CentreY);
                h.Push(d.Radius);
                h.Push(d.Top);
                foreach (var l in d.Layers)
                {
                    h.Push(l.Thickness);
                    h.Push(l.Initial);
                    layers++;
                    total += l.Initial;
                }
            }

            Assert.Equal(row.GetProperty("layers").GetInt32(), layers);
            Assert.Equal(row.GetProperty("fnv").GetString(), h.Hex);

            // The tonnage as bits, so a total that drifts in its last place is
            // caught by something a person can also read off the survey.
            Assert.Equal(
                Fixtures.BitsToDouble(row.GetProperty("total_tonnes_bits").GetString()!), total);
            checked_++;
        }

        Assert.Equal(6, checked_);
    }

    /// <summary>The promise between players: same seed, same ground.</summary>
    [Fact]
    public void The_same_seed_generates_the_same_ground()
    {
        var a = Geology.Generate(1961, 6000.0, Fixtures.Tables);
        var b = Geology.Generate(1961, 6000.0, Fixtures.Tables);
        var c = Geology.Generate(1962, 6000.0, Fixtures.Tables);

        Assert.Equal(a.All.Count, b.All.Count);
        for (var i = 0; i < a.All.Count; i++)
        {
            Assert.Equal(a.All[i].CentreX, b.All[i].CentreX);
            Assert.Equal(a.All[i].Radius, b.All[i].Radius);
        }

        Assert.NotEqual(a.All[0].CentreX, c.All[0].CentreX);
    }

    /// <summary>
    /// The plan is what it says: gravel shallow and everywhere, oil scarce and
    /// deep. Those two are the ends of the range, and a plan that lost them
    /// would still generate a perfectly deterministic and quite wrong map.
    /// </summary>
    [Fact]
    public void The_plan_puts_gravel_shallow_and_oil_deep()
    {
        var g = Geology.Generate(1961, 6000.0, Fixtures.Tables);

        var gravel = g.All.Where(d => d.Mineral == Mineral.Gravel).ToList();
        var oil = g.All.Where(d => d.Mineral == Mineral.Oil).ToList();

        Assert.Equal(6, gravel.Count);
        Assert.Equal(2, oil.Count);
        Assert.All(gravel, d => Assert.InRange(d.Top, 0.0, 8.0));
        Assert.All(oil, d => Assert.InRange(d.Top, 400.0, 1200.0));

        // And depth is what makes it dearer to work.
        Assert.True(oil[0].DepthCostMultiplier > gravel[0].DepthCostMultiplier);
    }

    /// <summary>
    /// Extraction works down through the horizons and reports the depth it
    /// finished at, so the cost of a load reflects the deepest work it required
    /// rather than the cheapest.
    /// </summary>
    [Fact]
    public void Extraction_works_down_through_the_layers()
    {
        var d = new Deposit(
            1, Mineral.Coal, 100.0, 100.0, 50.0, 20.0,
            [new Layer(10.0, 100.0), new Layer(15.0, 200.0)]);

        Assert.Equal(300.0, d.Initial);
        Assert.Equal(20.0, d.WorkingDepth);

        var first = d.Extract(40.0);
        Assert.Equal(40.0, first.Tonnes);
        Assert.Equal(20.0, first.Depth);
        Assert.Equal(260.0, d.Remaining);

        // Spanning the boundary: it reports the deeper horizon.
        var second = d.Extract(100.0);
        Assert.Equal(100.0, second.Tonnes);
        Assert.Equal(30.0, second.Depth);

        // And asking for more than is there returns what was there, never a debt.
        var rest = d.Extract(10_000.0);
        Assert.Equal(160.0, rest.Tonnes);
        Assert.Equal(0.0, d.Remaining);
        Assert.True(d.IsExhausted);
        Assert.Equal(0.0, d.Extract(50.0).Tonnes);
    }

    /// <summary>
    /// An aquifer refills; a coal seam does not. Recharge never exceeds what the
    /// horizon originally held, so a well cannot manufacture an aquifer larger
    /// than the ground it sits in.
    /// </summary>
    [Fact]
    public void An_aquifer_refills_and_cannot_overfill()
    {
        Assert.True(Mineral.Groundwater.Recharges());
        Assert.False(Mineral.Coal.Recharges());

        var d = new Deposit(
            1, Mineral.Groundwater, 0.0, 0.0, 500.0, 5.0, [new Layer(20.0, 1000.0)]);

        d.Extract(400.0);
        Assert.Equal(600.0, d.Remaining);

        d.Recharge(100.0);
        Assert.Equal(700.0, d.Remaining);

        d.Recharge(10_000.0);
        Assert.Equal(1000.0, d.Remaining);
    }

    /// <summary>
    /// Standing on the edge of a seam is standing on the seam, which is what a
    /// founding card reports and what decides where a mine can go.
    /// </summary>
    [Fact]
    public void Coverage_and_distance_agree_about_the_edge()
    {
        var g = Geology.Generate(1961, 6000.0, Fixtures.Tables);
        var d = g.All[0];

        Assert.True(d.Covers(d.CentreX, d.CentreY));
        Assert.True(d.Covers(d.CentreX + d.Radius - 0.001, d.CentreY));
        Assert.False(d.Covers(d.CentreX + d.Radius + 1.0, d.CentreY));

        Assert.Contains(d.Id, g.TappableAt(d.CentreX, d.CentreY));
        Assert.Equal(0.0, g.DistanceToNearest(d.CentreX, d.CentreY, d.Mineral));

        var far = g.DistanceToNearest(d.CentreX + d.Radius + 100.0, d.CentreY, d.Mineral);
        Assert.NotNull(far);
        Assert.InRange(far.Value, 0.0, 100.0);
    }
}
