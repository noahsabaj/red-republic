namespace RedRepublic.Sim.Tests;

/// <summary>
/// The balance table crossed from the Rust reference intact, and is shaped the
/// way the simulation expects to read it.
/// </summary>
public sealed class TablesTests
{
    /// <summary>
    /// The load-bearing one: every number in the table, hashed by its bits,
    /// agrees with what the Rust table hashed to. A figure parsed one ulp out, a
    /// row reordered or a field dropped all change this — and all three are
    /// silent otherwise.
    /// </summary>
    [Fact]
    public void The_table_survives_the_crossing()
    {
        // Load throws on a mismatch, so reaching this line is most of the check.
        var t = Fixtures.Tables;
        Assert.Equal(t.ChecksumExpected, t.ChecksumGot);
        Assert.NotEmpty(t.ChecksumExpected);
    }

    [Fact]
    public void The_rosters_are_the_size_the_simulation_had()
    {
        var t = Fixtures.Tables;
        Assert.Equal(64, t.BuildingCount);
        Assert.Equal(22, t.Resources.Length);
        Assert.Equal(13, t.VehicleCount);
        Assert.Equal(5, t.Forms.Length);
        Assert.Equal(4, t.Needs.Length);
        Assert.Equal(3, t.Priorities.Length);
        Assert.Equal(3, t.Education.Length);
        Assert.Equal(5, t.MineralPlan.Length);
    }

    /// <summary>
    /// The offset arrays have to close: one more entry than there are buildings,
    /// and the last entry is the length of the run array.
    /// </summary>
    [Fact]
    public void The_variable_length_runs_close()
    {
        var t = Fixtures.Tables;
        var n = t.BuildingCount;
        foreach (var (label, count, total, offsets) in new (string, int, int, int)[]
        {
            ("inputs", t.Inputs.Count, t.Inputs.Total, t.Inputs.Offsets[^1]),
            ("outputs", t.Outputs.Count, t.Outputs.Total, t.Outputs.Offsets[^1]),
            ("materials", t.Materials.Count, t.Materials.Total, t.Materials.Offsets[^1]),
            ("serves", t.Serves.Count, t.Serves.Total, t.Serves.Offsets[^1]),
            ("establishment", t.Establishment.Count, t.Establishment.Total, t.Establishment.Offsets[^1]),
            ("sells", t.Sells.Count, t.Sells.Total, t.Sells.Offsets[^1]),
            ("admits", t.Admits.Count, t.Admits.Total, t.Admits.Offsets[^1]),
        })
        {
            Assert.Equal(n, count);
            Assert.Equal(total, offsets);
            Assert.True(label.Length > 0);
        }
    }

    /// <summary>
    /// Spot-check one row against the Rust source by hand, so a checksum that is
    /// self-consistently wrong — both sides hashing the same mistake — still
    /// fails. The Coal Mine, read out of <c>crates/sim/src/building.rs</c>.
    /// </summary>
    [Fact]
    public void The_coal_mine_reads_the_way_it_did()
    {
        var t = Fixtures.Tables;
        var b = t.BuildingIndex("CoalMine");

        Assert.Equal("Coal Mine", t.BName[b]);
        Assert.Equal(14, t.BWorkers[b]);
        Assert.Equal(6.0, t.BPowerDraw[b]);
        Assert.Equal(200.0, t.BLabour[b]);
        Assert.Equal(60.0, t.BStorage[b]);
        Assert.Equal(0.03, t.BWear[b]);
        Assert.Equal(2.0, t.BPollution[b]);

        var outputs = t.Outputs.KeysOf(b);
        Assert.Equal(1, outputs.Length);
        Assert.Equal("Coal", t.Resources[outputs[0]]);
        Assert.Equal(6.0, t.Outputs.ValuesOf(b)[0]);

        Assert.Equal(4, t.Materials.LengthOf(b));
        Assert.Equal(0, t.Inputs.LengthOf(b));
        Assert.Equal("Coal", Tables.Minerals[t.BTaps[b]]);
    }

    /// <summary>
    /// A value that is not a round number, to prove the crossing keeps precision
    /// rather than only keeping integers. 15 km/h cross-country is stored as
    /// metres per second and reads back as 15.000000000000002 — the correct
    /// answer, not a defect to round away.
    /// </summary>
    [Fact]
    public void Precision_is_not_quietly_rounded()
    {
        var t = Fixtures.Tables;
        var v = Array.IndexOf(t.VehicleIds, "Lorry");

        Assert.Equal(15.000000000000002, t.VCrossCountryKph[v]);
        Assert.Equal(0.0003, t.VFuelPerKm[v]);
        Assert.Equal(50.0, t.VOnRoadKph[v]);
    }

    /// <summary>
    /// Prices are balance: the dearest end of the table against the cheapest is
    /// the whole industrialisation incentive, and a republic that gets a chain
    /// running earns in a lorry what a mine earns in a month.
    /// </summary>
    [Fact]
    public void The_price_gap_that_makes_industry_worth_it_is_intact()
    {
        var t = Fixtures.Tables;
        var coal = t.ResourceIndex("Coal");
        var electronics = t.ResourceIndex("Electronics");

        Assert.Equal(2.5, t.ResourcePriceEast[coal]);
        Assert.Equal(140.0, t.ResourcePriceEast[electronics]);
        Assert.True(t.ResourceIsComfort[electronics]);
        Assert.False(t.ResourceIsComfort[coal]);

        // The west is the hard-currency market and sells cheaper in its own
        // money — the price asymmetry the whole trade game rests on.
        Assert.True(t.ResourcePriceWest[coal] < t.ResourcePriceEast[coal]);
    }

    /// <summary>
    /// An unrecognised name in the manifest must stop the build rather than
    /// becoming a -1 that reads as "the last one" somewhere much later.
    /// </summary>
    [Fact]
    public void An_unknown_name_in_the_manifest_is_refused()
    {
        var json = File.ReadAllText(
            Path.Combine(Fixtures.RepoRoot, "game", "data", "manifest.json"));

        // Only the roster entry, not every mention. Renaming all of them renames
        // the definition and its references together and the table stays
        // perfectly consistent — which is what the first version of this test
        // did, and why it passed while asserting nothing.
        var broken = json.Replace(
            "\"forms\": [\"Aggregate\"", "\"forms\": [\"Custard\"", StringComparison.Ordinal);
        Assert.NotEqual(json, broken);

        var ex = Assert.Throws<InvalidDataException>(() => Tables.Load(broken));
        Assert.Contains("Aggregate", ex.Message, StringComparison.Ordinal);
    }
}
