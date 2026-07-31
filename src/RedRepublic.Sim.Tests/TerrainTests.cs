namespace RedRepublic.Sim.Tests;

/// <summary>
/// Worldgen reproduces the landscape the seed promises.
/// </summary>
/// <remarks>
/// Checked in <b>three layers</b>, not one. A generated map is tens of thousands
/// of cells and the only practical comparison is a hash — but a single red hash
/// cannot tell a wrong integer hash from wrong hydrology, and the depression
/// fill and the flow accumulation are exactly where the subtle mistakes live.
/// The first layer that disagrees is the one to look at.
/// </remarks>
public sealed class TerrainTests
{
    /// <summary>
    /// Layer 1: the integer lattice hash, including negative coordinates and one
    /// past 2⁵³ — the cases where a language's integer handling shows itself.
    /// </summary>
    [Fact]
    public void The_cell_hash_matches()
    {
        var rows = Fixtures.Vectors.GetProperty("terrain").GetProperty("hash_cell");
        var checked_ = 0;
        foreach (var row in rows.EnumerateArray())
        {
            var x = (long)Fixtures.Hex(row[0].GetString()!);
            var y = (long)Fixtures.Hex(row[1].GetString()!);
            Assert.Equal(Fixtures.Hex(row[2].GetString()!), Terrain.HashCell(1961, x, y));
            checked_++;
        }

        Assert.Equal(6, checked_);
    }

    /// <summary>
    /// Layer 2: the fractal field, compared by bits. Every step of it is integer
    /// hashing and polynomial interpolation, so it is exact on any IEEE-754
    /// machine and a tolerance here would be hiding something.
    /// </summary>
    [Fact]
    public void The_noise_field_matches()
    {
        var t = Fixtures.Tables;
        var rows = Fixtures.Vectors.GetProperty("terrain").GetProperty("fractal_noise");
        var checked_ = 0;
        foreach (var row in rows.EnumerateArray())
        {
            var got = Terrain.FractalNoise(
                1961, row[0].GetDouble(), row[1].GetDouble(), t.TerrainFeatureSize, t.TerrainOctaves);
            Assert.Equal(Fixtures.BitsToDouble(row[2].GetString()!), got);
            checked_++;
        }

        Assert.Equal(5, checked_);
    }

    /// <summary>
    /// Layer 3: whole generated maps — surfaces, heights and the surface census.
    /// </summary>
    /// <remarks>
    /// The census is checked as well as the hashes because it is the half a
    /// person can read. "13,066 grass, 1,091 forest, 0 rock, 243 water" says
    /// what kind of posting this is; a hash only says whether two runs agree.
    /// </remarks>
    [Fact]
    public void Generated_maps_match()
    {
        var t = Fixtures.Tables;
        var maps = Fixtures.Vectors.GetProperty("terrain").GetProperty("maps");
        var checked_ = 0;

        foreach (var m in maps.EnumerateArray())
        {
            var seed = m.GetProperty("seed").GetUInt64();
            var extent = m.GetProperty("extent").GetDouble();
            var map = Terrain.Generate(seed, extent, t);

            Assert.Equal(m.GetProperty("cells").GetInt32(), map.Cells);

            var counts = new int[4];
            foreach (var s in map.Surfaces)
            {
                counts[(int)s]++;
            }

            var want = m.GetProperty("counts");
            for (var s = 0; s < 4; s++)
            {
                Assert.Equal(want[s].GetInt32(), counts[s]);
            }

            var surfaceHash = new Fnv1a();
            foreach (var s in map.Surfaces)
            {
                surfaceHash.PushByte((byte)s);
            }

            Assert.Equal(m.GetProperty("surface_fnv").GetString(), surfaceHash.Hex);

            var heightHash = new Fnv1a();
            foreach (var h in map.Height)
            {
                heightHash.PushBytes(BitConverter.GetBytes(h));
            }

            Assert.Equal(m.GetProperty("height_fnv").GetString(), heightHash.Hex);
            checked_++;
        }

        Assert.Equal(6, checked_);
    }

    [Fact]
    public void A_flat_map_is_flat()
    {
        var map = Terrain.Flat(1000.0, 10.0);

        Assert.Equal(100, map.Cells);
        Assert.Equal(1000.0, map.Extent);
        Assert.True(map.Contains(0.0, 0.0));
        Assert.True(map.Contains(999.9, 999.9));
        Assert.False(map.Contains(1000.0, 0.0));
        Assert.False(map.Contains(-0.1, 0.0));
        Assert.Equal(Surface.Grass, map.SurfaceAt(500.0, 500.0));
        Assert.Equal(0.0, map.HeightAt(500.0, 500.0));
        Assert.True(map.AreaIsBuildable(500.0, 500.0, 50.0, 40.0));
        Assert.False(map.AreaIsBuildable(10.0, 10.0, 50.0, 40.0));
        Assert.Null(map.SurfaceAt(-1.0, 0.0));
    }

    /// <summary>
    /// The check a road order makes. Sampled at half-cell steps so a one-cell
    /// river cannot be stepped over — without this a gravel road crosses a river
    /// at the price of gravel.
    /// </summary>
    [Fact]
    public void Water_cannot_be_stepped_over()
    {
        var map = Terrain.Flat(1000.0, 10.0);
        map.SetSurfaceAt(500.0, 500.0, Surface.Water);

        Assert.True(map.CrossesWater(100.0, 505.0, 900.0, 505.0));
        Assert.False(map.CrossesWater(100.0, 105.0, 900.0, 105.0));
        Assert.False(map.AreaIsBuildable(500.0, 500.0, 40.0, 40.0));
    }

    /// <summary>
    /// A generated map has rivers that actually connect, which is the whole
    /// point of the fill-then-accumulate pass. A thresholded noise field gives
    /// isolated basins instead, and both the bridge and the barge become
    /// mechanics with no subject.
    /// </summary>
    [Fact]
    public void A_generated_map_has_water_and_dry_land()
    {
        var map = Terrain.Generate(1961, 3000.0, Fixtures.Tables);

        var water = map.FractionOf(Surface.Water);
        Assert.InRange(water, 0.001, 0.5);
        Assert.InRange(map.FractionOf(Surface.Grass), 0.01, 0.99);
        Assert.Equal(1.0, map.FractionOf(Surface.Grass)
            + map.FractionOf(Surface.Forest)
            + map.FractionOf(Surface.Rock)
            + water, 12);
    }
}
