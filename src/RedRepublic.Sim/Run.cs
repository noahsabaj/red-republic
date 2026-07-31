namespace RedRepublic.Sim;

/// <summary>
/// A variable-length row per entity, held the way a sparse matrix is: one flat
/// array of keys, one of values, and an offset array saying where each entity's
/// run begins.
/// </summary>
/// <remarks>
/// <para>
/// A building has between zero and four inputs; a list per building would be an
/// allocation per building and a pointer chase per read, in a pass that walks
/// every building every tick. This keeps the whole table in three contiguous
/// arrays and hands out spans into them.
/// </para>
/// <para>
/// <see cref="KeysOf"/> and <see cref="ValuesOf"/> return
/// <see cref="ReadOnlySpan{T}"/> rather than arrays, so reading a row copies
/// nothing at all.
/// </para>
/// </remarks>
public readonly struct Run<TKey, TValue>
    where TKey : struct
    where TValue : struct
{
    private readonly int[] _at;
    private readonly TKey[] _keys;
    private readonly TValue[] _values;

    internal Run(int[] at, TKey[] keys, TValue[] values)
    {
        _at = at;
        _keys = keys;
        _values = values;
    }

    /// <summary>A run covering no entities — what a table holds before it is loaded.</summary>
    /// <remarks>
    /// CA1000 objects to a static member on a generic type, on the grounds that
    /// it is awkward to reach from languages without good generic syntax. That
    /// is a rule for libraries published to an unknown audience; this is exactly
    /// the shape <c>ImmutableArray&lt;T&gt;.Empty</c> and
    /// <c>ReadOnlySpan&lt;T&gt;.Empty</c> use in the BCL, and the alternative is
    /// a nullable backing array that every accessor then has to check.
    /// </remarks>
#pragma warning disable CA1000
    public static Run<TKey, TValue> Empty { get; } = new([0], [], []);
#pragma warning restore CA1000

    /// <summary>How many entities this run covers.</summary>
    public int Count => _at.Length - 1;

    /// <summary>How many entries the whole table holds.</summary>
    public int Total => _at[^1];

    public ReadOnlySpan<TKey> KeysOf(int entity) =>
        _keys.AsSpan(_at[entity], _at[entity + 1] - _at[entity]);

    public ReadOnlySpan<TValue> ValuesOf(int entity) =>
        _values.AsSpan(_at[entity], _at[entity + 1] - _at[entity]);

    /// <summary>How many entries this entity has.</summary>
    public int LengthOf(int entity) => _at[entity + 1] - _at[entity];

    /// <summary>
    /// The offsets themselves, for the guard that checks the runs close. Nothing
    /// in the simulation should need these — use the spans.
    /// </summary>
    public ReadOnlySpan<int> Offsets => _at;
}

/// <summary>Builds a <see cref="Run{TKey, TValue}"/> one entity at a time.</summary>
internal sealed class RunBuilder<TKey, TValue>(int entities)
    where TKey : struct
    where TValue : struct
{
    private readonly List<int> _at = new(entities + 1) { 0 };
    private readonly List<TKey> _keys = [];
    private readonly List<TValue> _values = [];

    internal void Add(TKey key, TValue value)
    {
        _keys.Add(key);
        _values.Add(value);
    }

    /// <summary>Finish the entity in progress. Must be called once per entity, in order.</summary>
    internal void Close() => _at.Add(_keys.Count);

    internal Run<TKey, TValue> Build() => new([.. _at], [.. _keys], [.. _values]);
}
