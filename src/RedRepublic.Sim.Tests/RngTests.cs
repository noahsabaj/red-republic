namespace RedRepublic.Sim.Tests;

/// <summary>
/// The generator draws the same stream the Rust one did.
/// </summary>
/// <remarks>
/// Checked against <c>Data/vectors.json</c>, dumped from the Rust
/// implementation, rather than against numbers written down here. Map generation
/// must reproduce across machines — a shared seed is a promise between players —
/// and a reimplementation is only as good as the evidence that it draws the same
/// stream.
/// </remarks>
public sealed class RngTests
{
    /// <summary>Five seeds, sixteen draws each, compared as bit patterns.</summary>
    [Fact]
    public void NextULong_matches_the_rust_stream()
    {
        var table = Fixtures.Vectors.GetProperty("next_u64");
        var seeds = 0;
        foreach (var entry in table.EnumerateObject())
        {
            seeds++;
            var rng = Rng.FromSeed(Fixtures.Hex(entry.Name));
            var draws = 0;
            foreach (var want in entry.Value.EnumerateArray())
            {
                Assert.Equal(Fixtures.Hex(want.GetString()!), rng.NextULong());
                draws++;
            }

            Assert.Equal(16, draws);
        }

        Assert.Equal(5, seeds);
    }

    [Fact]
    public void NextDouble_matches_the_rust_stream()
    {
        var table = Fixtures.Vectors.GetProperty("next_f64_bits");
        foreach (var entry in table.EnumerateObject())
        {
            var rng = Rng.FromSeed(Fixtures.Hex(entry.Name));
            var draws = 0;
            foreach (var want in entry.Value.EnumerateArray())
            {
                // Bit equality, not a tolerance. An epsilon here would pass on
                // exactly the drift the test exists to catch.
                Assert.Equal(Fixtures.BitsToDouble(want.GetString()!), rng.NextDouble());
                draws++;
            }

            Assert.Equal(16, draws);
        }
    }

    /// <summary>
    /// The half of the bounded draw a naive <c>% n</c> gets wrong: the rejection
    /// threshold, which is what removes the bias toward small values.
    /// </summary>
    [Fact]
    public void NextBounded_matches_the_rust_stream()
    {
        var table = Fixtures.Vectors.GetProperty("next_bounded");
        var bounds = 0;
        foreach (var entry in table.EnumerateObject())
        {
            bounds++;
            var n = ulong.Parse(entry.Name, System.Globalization.CultureInfo.InvariantCulture);
            var rng = Rng.FromSeed(1961);
            var draws = 0;
            foreach (var want in entry.Value.EnumerateArray())
            {
                var got = rng.NextBounded(n);
                Assert.Equal(want.GetUInt64(), got);
                Assert.True(got < n);
                draws++;
            }

            Assert.Equal(24, draws);
        }

        Assert.Equal(6, bounds);
    }

    /// <summary>
    /// The save contract. A generator that restores the seed but not the
    /// position passes every test above and fails this one.
    /// </summary>
    [Fact]
    public void A_saved_position_resumes_the_same_future()
    {
        var r = Fixtures.Vectors.GetProperty("resume");
        var live = Rng.FromSeed(r.GetProperty("seed").GetUInt64());
        var before = r.GetProperty("draws_before").GetInt32();
        for (var i = 0; i < before; i++)
        {
            live.NextULong();
        }

        var saved = live.State;
        var want = new List<ulong>();
        foreach (var e in r.GetProperty("after").EnumerateArray())
        {
            want.Add(Fixtures.Hex(e.GetString()!));
        }

        Assert.NotEmpty(want);

        // The live generator goes on to produce the pinned values...
        foreach (var w in want)
        {
            Assert.Equal(w, live.NextULong());
        }

        // ...and one wound to the carried position produces exactly the same.
        var restored = Rng.FromSeed(0);
        restored.State = saved;
        foreach (var w in want)
        {
            Assert.Equal(w, restored.NextULong());
        }
    }

    [Fact]
    public void Doubles_stay_in_the_half_open_unit_interval()
    {
        var rng = Rng.FromSeed(99);
        for (var i = 0; i < 100_000; i++)
        {
            var x = rng.NextDouble();
            Assert.InRange(x, 0.0, 0.9999999999999999);
        }
    }

    [Fact]
    public void A_zero_bound_is_refused()
    {
        var rng = Rng.FromSeed(1);
        Assert.Throws<ArgumentOutOfRangeException>(() => rng.NextBounded(0));
    }

    /// <summary>
    /// Not a quality test — a smoke test that the distribution is not visibly
    /// broken. A generator that returned a constant, or only even numbers, would
    /// pass every test above and fail this one.
    /// </summary>
    [Fact]
    public void Draws_are_roughly_uniform_across_buckets()
    {
        var rng = Rng.FromSeed(2024);
        var buckets = new int[10];
        const int n = 200_000;
        for (var i = 0; i < n; i++)
        {
            buckets[(int)(rng.NextDouble() * 10.0)]++;
        }

        foreach (var count in buckets)
        {
            Assert.InRange(count, (n / 10) - (n / 100), (n / 10) + (n / 100));
        }
    }
}
