using System;
using System.Collections.Generic;
using System.Linq;
using Godot;

namespace RedRepublic.World;

/// <summary>How a roof is shaped.</summary>
public enum Roof
{
    Flat,
    Pitch,
    Sawtooth,
}

/// <summary>
/// Broad material families.
/// </summary>
/// <remarks>
/// Not per-building colours: a roster of a hundred buildings each with its own
/// tint is noise, and the point of a kit is that things which are alike look
/// alike.
/// </remarks>
public enum Tone
{
    Render,
    Brick,
    Concrete,
    Metal,
    Timber,
}

/// <summary>What one kind of building looks like.</summary>
public readonly record struct Art(
    int Storeys, float StoreyM, Roof Roof, int Stacks, int Silos, bool Gantry, Tone Tone)
{
    public float Height => Storeys * StoreyM;
}

/// <summary>
/// What each building looks like, authored per kind.
/// </summary>
/// <remarks>
/// <para>
/// <b>This is art data, and it lives here rather than on the balance table</b>,
/// because height, roof type and whether there is a chimney change nothing about
/// how a republic runs. A field no system reads has no business being simulation
/// state, and keeping it here means the look can be changed without touching the
/// simulation.
/// </para>
/// <para>
/// The simulation still owns everything physical: real width and depth in metres
/// come from the table and are never guessed at here. Those footprints are large
/// — tens of metres — so a building needs real height or it reads as a slab
/// painted on the ground.
/// </para>
/// <para>
/// <b>Every kind must have a row.</b> <see cref="Check"/> fails loudly on an
/// unauthored one rather than letting it fall back to a default, because a
/// defaulted building is a decision nobody made — the same rule that puts an
/// explicit zero on every field of the balance table.
/// </para>
/// </remarks>
public static class BuildingArt
{
    /// <summary>Keyed by the name the balance table authors.</summary>
    public static IReadOnlyDictionary<string, Art> Table { get; } = new Dictionary<string, Art>
    {
        ["Small House"] = new(1, 3.2f, Roof.Pitch, 0, 0, false, Tone.Timber),
        ["Apartment Block"] = new(5, 3.0f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Panel Block"] = new(9, 3.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Woodcutter Post"] = new(1, 4.0f, Roof.Pitch, 0, 0, false, Tone.Timber),
        ["Gravel Quarry"] = new(1, 5.0f, Roof.Flat, 0, 2, false, Tone.Concrete),
        ["Coal Mine"] = new(3, 5.0f, Roof.Flat, 0, 0, true, Tone.Metal),
        ["Iron Ore Mine"] = new(3, 5.0f, Roof.Flat, 0, 0, true, Tone.Metal),
        ["Oil Pump"] = new(1, 3.0f, Roof.Flat, 0, 1, false, Tone.Metal),
        ["Sawmill"] = new(1, 8.5f, Roof.Sawtooth, 0, 0, false, Tone.Timber),
        ["Brickworks"] = new(2, 5.0f, Roof.Sawtooth, 1, 0, false, Tone.Brick),
        ["Steel Mill"] = new(3, 6.5f, Roof.Sawtooth, 2, 0, false, Tone.Metal),
        ["Oil Refinery"] = new(2, 6.0f, Roof.Flat, 1, 3, false, Tone.Metal),
        ["Machine Works"] = new(2, 6.0f, Roof.Sawtooth, 1, 0, false, Tone.Concrete),
        ["Cement Works"] = new(3, 7.0f, Roof.Flat, 1, 3, false, Tone.Concrete),
        ["Concrete Plant"] = new(2, 6.0f, Roof.Flat, 0, 2, true, Tone.Concrete),
        ["Panel Works"] = new(1, 9.0f, Roof.Sawtooth, 0, 0, true, Tone.Concrete),
        ["Asphalt Plant"] = new(2, 7.0f, Roof.Flat, 1, 2, false, Tone.Metal),
        ["Chemical Works"] = new(3, 6.0f, Roof.Flat, 2, 3, false, Tone.Metal),
        ["Electronics Combine"] = new(3, 4.2f, Roof.Sawtooth, 0, 0, false, Tone.Render),
        ["State Distillery"] = new(2, 5.5f, Roof.Flat, 1, 2, false, Tone.Brick),
        ["Coal Power Plant"] = new(4, 6.5f, Roof.Flat, 2, 0, false, Tone.Concrete),
        ["Oil Power Plant"] = new(4, 6.5f, Roof.Flat, 1, 2, false, Tone.Concrete),
        ["Heating Plant"] = new(2, 6.0f, Roof.Flat, 1, 0, false, Tone.Brick),
        ["Collective Farm"] = new(1, 6.5f, Roof.Pitch, 0, 2, false, Tone.Timber),
        ["Food Factory"] = new(2, 5.0f, Roof.Sawtooth, 1, 1, false, Tone.Render),
        ["Textile Mill"] = new(3, 4.5f, Roof.Sawtooth, 1, 0, false, Tone.Brick),
        ["State Store"] = new(2, 4.2f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Polyclinic"] = new(3, 3.4f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Culture Club"] = new(2, 5.5f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Ten-Year School"] = new(3, 3.6f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Polytechnic Institute"] = new(4, 3.8f, Roof.Flat, 0, 0, true, Tone.Concrete),
        ["Kindergarten"] = new(1, 3.4f, Roof.Pitch, 0, 0, false, Tone.Render),
        ["District Hospital"] = new(6, 3.4f, Roof.Flat, 1, 0, false, Tone.Render),
        ["Pharmacy"] = new(1, 4.0f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Fire Station"] = new(1, 6.5f, Roof.Flat, 0, 0, true, Tone.Brick),
        ["Militia Station"] = new(2, 3.6f, Roof.Flat, 0, 0, false, Tone.Render),
        ["People's Court"] = new(2, 5.5f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Corrective Labour Colony"] = new(2, 3.4f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Sports Hall"] = new(1, 12.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Cinema"] = new(1, 9.0f, Roof.Flat, 0, 0, false, Tone.Render),
        ["State Radio Centre"] = new(2, 4.0f, Roof.Flat, 0, 0, true, Tone.Render),
        ["Cemetery"] = new(1, 2.5f, Roof.Pitch, 0, 0, false, Tone.Brick),
        ["Transformer Station"] = new(1, 5.0f, Roof.Flat, 0, 0, true, Tone.Concrete),
        ["Landfill"] = new(1, 4.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Refuse Incinerator"] = new(2, 7.0f, Roof.Flat, 2, 0, false, Tone.Metal),
        ["Warehouse"] = new(1, 11.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Open Storage Yard"] = new(1, 1.6f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Aggregate Store"] = new(1, 4.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Storage Tank"] = new(1, 10.0f, Roof.Flat, 0, 3, false, Tone.Metal),
        ["Grain Silo"] = new(1, 14.0f, Roof.Flat, 0, 4, false, Tone.Concrete),
        ["Council Depot"] = new(1, 8.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Construction Office"] = new(2, 3.6f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Motor Depot"] = new(1, 9.0f, Roof.Sawtooth, 0, 0, false, Tone.Metal),
        ["Gas Station"] = new(1, 4.0f, Roof.Flat, 0, 2, false, Tone.Metal),
        ["Bus Depot"] = new(1, 9.5f, Roof.Sawtooth, 0, 0, false, Tone.Metal),
        ["Trolleybus Depot"] = new(1, 9.0f, Roof.Sawtooth, 0, 0, false, Tone.Metal),
        ["Tram Depot"] = new(1, 10.0f, Roof.Sawtooth, 0, 0, false, Tone.Metal),
        ["Metro Depot"] = new(1, 11.0f, Roof.Flat, 0, 0, true, Tone.Concrete),
        ["Railway Station"] = new(2, 5.0f, Roof.Flat, 0, 0, true, Tone.Concrete),
        ["River Port"] = new(1, 7.0f, Roof.Flat, 0, 1, true, Tone.Concrete),
        ["Aerodrome"] = new(1, 6.0f, Roof.Flat, 0, 0, false, Tone.Concrete),
        ["Distribution Office"] = new(2, 4.0f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Intourist Hotel"] = new(8, 3.2f, Roof.Flat, 0, 0, false, Tone.Render),
        ["Customs House"] = new(2, 4.0f, Roof.Flat, 0, 0, false, Tone.Render),
    };

    /// <summary>
    /// Fail on any building the simulation knows about and this table does not,
    /// and warn on a row for something that no longer exists.
    /// </summary>
    /// <remarks>
    /// The kit is what makes a hundred-plus roster affordable, and the way that
    /// goes wrong is a new building quietly rendering as a default box nobody
    /// chose. This turns that into a loud failure at load. The other direction is
    /// a warning rather than a failure: a stale row costs nothing but is nobody's
    /// intention either.
    /// </remarks>
    public static void Check(IReadOnlyList<string> names)
    {
        ArgumentNullException.ThrowIfNull(names);
        var missing = new List<string>();
        foreach (var name in names)
        {
            if (!Table.ContainsKey(name))
            {
                missing.Add(name);
            }
        }

        if (missing.Count > 0)
        {
            throw new KeyNotFoundException(
                $"building art is unauthored for: {string.Join(", ", missing)}");
        }

        foreach (var name in Table.Keys)
        {
            if (!names.Contains(name))
            {
                GD.PushWarning($"building art authored for '{name}', which no longer exists");
            }
        }
    }
}
