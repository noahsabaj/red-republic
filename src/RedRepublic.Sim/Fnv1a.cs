using System.Globalization;

namespace RedRepublic.Sim;

/// <summary>
/// FNV-1a, 64-bit, over the <b>bits</b> of the numbers pushed into it.
/// </summary>
/// <remarks>
/// <para>
/// Chosen because it is four lines and needs no dependency; this hashes a few
/// thousand numbers once at startup, so speed is not a consideration. It is not
/// a cryptographic hash and is not used as one — the question it answers is
/// "did this table arrive exactly as it was written", where the adversary is a
/// parser, not a person.
/// </para>
/// <para>
/// Numbers are hashed by bit pattern rather than by value on purpose. Comparing
/// doubles by value with a tolerance is exactly the thing that would let a
/// figure arrive one ulp out and pass.
/// </para>
/// </remarks>
public struct Fnv1a()
{
    private const ulong Prime = 0x100_0000_01b3;

    private ulong _h = 0xcbf2_9ce4_8422_2325;

    public readonly ulong Value => _h;

    public readonly string Hex => _h.ToString("x16", CultureInfo.InvariantCulture);

    public void Push(double v)
    {
        var bits = BitConverter.DoubleToUInt64Bits(v);
        for (var i = 0; i < 8; i++)
        {
            PushByte((byte)(bits >> (i * 8)));
        }
    }

    /// <summary>
    /// An integer, hashed as the double it converts to.
    /// </summary>
    /// <remarks>
    /// The Rust dumper widens counts to <c>f64</c> before hashing them, so this
    /// has to as well or the two sides disagree on every row with a worker count
    /// in it. Matching the dumper is the whole job.
    /// </remarks>
    public void Push(int v) => Push((double)v);

    public void Push(bool v) => Push(v ? 1.0 : 0.0);

    public void PushByte(byte b)
    {
        _h ^= b;
        _h *= Prime;
    }

    public void PushBytes(ReadOnlySpan<byte> bytes)
    {
        foreach (var b in bytes)
        {
            PushByte(b);
        }
    }
}
