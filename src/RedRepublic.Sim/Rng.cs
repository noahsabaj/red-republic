namespace RedRepublic.Sim;

/// <summary>
/// The simulation's only source of randomness.
/// </summary>
/// <remarks>
/// <para>
/// <b>xoshiro256++, seeded by SplitMix64</b> — hand-implemented, no dependency.
/// A dependency bump must never be able to change a stream: a seed has to mean
/// the same thing forever. Both algorithms are short, fully specified and have
/// public reference implementations, so the cost of owning them is sixty lines.
/// </para>
/// <para>
/// 256 bits of state rather than 32, because short-period generators show their
/// correlations exactly when many consumers draw in lockstep, which is what tens
/// of thousands of citizens are.
/// </para>
/// <para>
/// <b>The save contract.</b> <see cref="State"/> carries the <i>stream
/// position</i>, not just the seed. A save that restores the seed but not the
/// position resumes a different future, and the failure is invisible until
/// someone compares two runs.
/// </para>
/// <para>
/// <b>Determinism.</b> Every operation is integer arithmetic with wrapping
/// semantics, so it is bit-identical on any machine — which is what worldgen
/// needs, since a shared seed is a promise between players.
/// <see cref="NextDouble"/> is the one float, and its conversion is exact: a
/// 53-bit integer scaled by a power of two, with nothing to round.
/// </para>
/// </remarks>
public sealed class Rng
{
    private ulong _s0;
    private ulong _s1;
    private ulong _s2;
    private ulong _s3;

    private Rng() { }

    /// <summary>
    /// Expand a seed into a full state with SplitMix64, as the xoshiro authors
    /// specify. Seeding the state directly from a small integer leaves it mostly
    /// zero, and xoshiro needs several rounds to recover from that.
    /// </summary>
    public static Rng FromSeed(ulong seed)
    {
        var z = seed;
        var rng = new Rng();
        Span<ulong> s = stackalloc ulong[4];
        for (var i = 0; i < 4; i++)
        {
            z += 0x9E37_79B9_7F4A_7C15;
            var x = z;
            x = (x ^ (x >> 30)) * 0xBF58_476D_1CE4_E5B9;
            x = (x ^ (x >> 27)) * 0x94D0_49BB_1331_11EB;
            s[i] = x ^ (x >> 31);
        }

        rng._s0 = s[0];
        rng._s1 = s[1];
        rng._s2 = s[2];
        rng._s3 = s[3];
        return rng;
    }

    /// <summary>The next 64 random bits — the primitive everything else is built on.</summary>
    public ulong NextULong()
    {
        var result = ulong.RotateLeft(_s0 + _s3, 23) + _s0;
        var t = _s1 << 17;
        _s2 ^= _s0;
        _s3 ^= _s1;
        _s1 ^= _s2;
        _s0 ^= _s3;
        _s2 ^= t;
        _s3 = ulong.RotateLeft(_s3, 45);
        return result;
    }

    /// <summary>
    /// A double in <c>[0, 1)</c>.
    /// </summary>
    /// <remarks>
    /// Takes the top 53 bits — the exact width of a double's mantissa — and
    /// scales by 2⁻⁵³. Both steps are exact, so this introduces no rounding of
    /// its own and reproduces bit-for-bit anywhere <see cref="NextULong"/> does.
    /// </remarks>
    public double NextDouble()
    {
        const double scale = 1.0 / (1UL << 53);
        return (NextULong() >> 11) * scale;
    }

    /// <summary>
    /// A uniform integer in <c>[0, n)</c>, with the bias removed.
    /// </summary>
    /// <remarks>
    /// Plain <c>NextULong() % n</c> is skewed toward small values whenever
    /// <c>n</c> does not divide 2⁶⁴. Rejecting the short leading block costs one
    /// extra draw vanishingly often and makes the distribution exact — worth it,
    /// because a bias here shows up as a map that subtly prefers one corner and
    /// takes a week to find.
    /// </remarks>
    public ulong NextBounded(ulong n)
    {
        ArgumentOutOfRangeException.ThrowIfZero(n);

        // (2^64) mod n, computed without overflowing.
        var threshold = (ulong.MaxValue - n + 1) % n;
        while (true)
        {
            var x = NextULong();
            if (x >= threshold)
            {
                return x % n;
            }
        }
    }

    /// <summary>A double in <c>[lo, hi)</c>.</summary>
    public double NextRange(double lo, double hi) => lo + (NextDouble() * (hi - lo));

    /// <summary>
    /// The current stream position. Persist this, not the seed.
    /// </summary>
    public RngState State
    {
        get => new(_s0, _s1, _s2, _s3);
        set
        {
            _s0 = value.S0;
            _s1 = value.S1;
            _s2 = value.S2;
            _s3 = value.S3;
        }
    }
}

/// <summary>
/// The full internal state of an <see cref="Rng"/> — enough to resume a stream
/// exactly.
/// </summary>
public readonly record struct RngState(ulong S0, ulong S1, ulong S2, ulong S3);
