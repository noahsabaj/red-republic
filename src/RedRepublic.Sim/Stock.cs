namespace RedRepublic.Sim;

/// <summary>
/// Stockpiles, held as one flat array for the whole republic.
/// </summary>
/// <remarks>
/// <para>
/// Every building owns a run of <c>Resources</c> slots inside a single array, so
/// building <c>b</c>'s holding of resource <c>r</c> lives at
/// <c>b * count + r</c>. There is no stockpile object per building and
/// deliberately none: that is an allocation per building, in a pass that touches
/// every one of them every tick.
/// </para>
/// <para>
/// <b>A shortfall is a smaller delivery, never a debt.</b> Stock is clamped at
/// zero rather than allowed to go negative, which is the archived build's rule
/// and the reason a starved factory stalls instead of quietly poisoning every
/// downstream rate with a negative tonnage.
/// </para>
/// <para>
/// It grows and shrinks with the building list, and the two must stay in step —
/// a stockpile table one row out of alignment puts a mine's coal in the building
/// next door, and that reads as a balance problem for weeks.
/// </para>
/// </remarks>
public sealed class GrowableStock(int resources)
{
    private double[] _t = [];

    public int Resources { get; } = resources;

    public int Buildings { get; private set; }

    /// <summary>The whole table, for a caller that wants to read it in bulk.</summary>
    public ReadOnlySpan<double> All => _t.AsSpan(0, Buildings * Resources);

    /// <summary>Add a row of empty bins for a new building.</summary>
    public void Grow()
    {
        var needed = (Buildings + 1) * Resources;
        if (needed > _t.Length)
        {
            // Doubling, so adding a thousand buildings is not a thousand
            // copies of the whole table.
            Array.Resize(ref _t, Math.Max(needed, Math.Max(16, _t.Length * 2)));
        }

        Array.Clear(_t, Buildings * Resources, Resources);
        Buildings++;
    }

    /// <summary>
    /// Drop a building's row, sliding the rest down so the indices still match
    /// the building list.
    /// </summary>
    public void RemoveAt(int b)
    {
        var from = (b + 1) * Resources;
        var to = b * Resources;
        var tail = (Buildings * Resources) - from;
        if (tail > 0)
        {
            Array.Copy(_t, from, _t, to, tail);
        }

        Buildings--;
    }

    public double Get(int b, int r) => _t[(b * Resources) + r];

    public void Set(int b, int r, double tonnes) => _t[(b * Resources) + r] = Math.Max(0.0, tonnes);

    public void Add(int b, int r, double tonnes)
    {
        var i = (b * Resources) + r;
        _t[i] = Math.Max(0.0, _t[i] + tonnes);
    }

    /// <summary>Take up to <paramref name="tonnes"/>, returning what was actually there.</summary>
    public double Take(int b, int r, double tonnes)
    {
        var i = (b * Resources) + r;
        var got = Math.Min(_t[i], tonnes);
        _t[i] -= got;
        return got;
    }

    /// <summary>Everything one building is holding, across all resources.</summary>
    public double Total(int b)
    {
        var sum = 0.0;
        foreach (var v in Row(b))
        {
            sum += v;
        }

        return sum;
    }

    public bool IsEmpty(int b) => Total(b) <= 0.0;

    /// <summary>
    /// Whether there is any of <paramref name="r"/> at all — the check a
    /// production pass makes before deciding a building is starved.
    /// </summary>
    public bool Has(int b, int r) => _t[(b * Resources) + r] > 0.0;

    /// <summary>
    /// One building's whole run. A span, so reading a row copies nothing — this
    /// is what the interface is handed rather than a dictionary per entity.
    /// </summary>
    public ReadOnlySpan<double> Row(int b) => _t.AsSpan(b * Resources, Resources);
}
