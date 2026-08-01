using System;
using System.Collections.Generic;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.World;

/// <summary>
/// The republic's buildings, drawn.
/// </summary>
/// <remarks>
/// <para>
/// <b>One multimesh per kind.</b> A republic ends up with hundreds of buildings
/// and the roster is sixty-four kinds, so drawing each kind's instances from a
/// packed transform buffer keeps the per-building cost at a matrix rather than a
/// node. That is the measured rule the whole boundary is built around.
/// </para>
/// <para>
/// <b>A site is not a building.</b> Something half-built is drawn at the height
/// it has actually reached, so a foundation reads as a foundation and the moment
/// a building tops out is visible from the camera — which is the only place a
/// player watches construction happen.
/// </para>
/// </remarks>
public sealed partial class Buildings : Node3D
{
    private readonly Dictionary<int, MultiMeshInstance3D> _byKind = [];
    private Tables _t = null!;
    private Terrain _terrain = null!;
    private int _drawn = -1;

    public void Raise(Tables tables, Terrain terrain)
    {
        ArgumentNullException.ThrowIfNull(tables);
        _t = tables;
        _terrain = terrain;

        // Fails loudly on a kind nobody has authored art for, rather than
        // letting it render as a default box that nobody chose.
        BuildingArt.Check(tables.BName);

        var parts = BuildingKit.Components();
        for (var kind = 0; kind < tables.BuildingCount; kind++)
        {
            var art = BuildingArt.Table[tables.BName[kind]];
            var mesh = BuildingKit.Assemble(
                parts, art, (float)tables.BWidth[kind], (float)tables.BDepth[kind]);

            var instance = new MultiMeshInstance3D
            {
                Name = tables.BuildingIds[kind],
                Multimesh = new MultiMesh
                {
                    TransformFormat = MultiMesh.TransformFormatEnum.Transform3D,
                    UseCustomData = true,
                    Mesh = mesh,
                    InstanceCount = 0,
                },
            };

            AddChild(instance);
            _byKind[kind] = instance;
        }
    }

    /// <summary>
    /// Put every building where the republic says it is.
    /// </summary>
    /// <remarks>
    /// Rebuilt only when the count changes or a site visibly advances, because
    /// a transform buffer rewritten every frame costs more than the simulation
    /// does and nothing on it moves.
    /// </remarks>
    public void Refresh(Sim.World world)
    {
        ArgumentNullException.ThrowIfNull(world);

        // <b>What is actually on screen, not what day it is.</b> The stamp used
        // to change once a day, so a site rose in twenty-four steps — at speed
        // one that is a building frozen for a real day at a time, in the one
        // place a player watches construction happen and against this class's
        // own promise that the moment a building tops out is visible.
        var stamp = world.Buildings.Count;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                // Sixty-fourths of the way up: finer than an eye can see move
                // and coarse enough that a buffer is not rewritten for nothing.
                stamp = (stamp * 31) + (int)(world.Buildings.Progress(b) * 64.0);
            }
        }

        if (stamp == _drawn)
        {
            return;
        }

        _drawn = stamp;

        var standing = new Dictionary<int, List<Transform3D>>();
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            var x = (float)world.Buildings.XAt(b);
            var z = (float)world.Buildings.YAt(b);
            var y = TerrainMesh.GroundAt(_terrain, x, z);

            // A site rises as it is built. Never to nothing, or a foundation
            // pegged out in clay would be invisible and the player could not
            // tell an order that was refused from one that was accepted.
            var risen = world.Buildings.IsBuilt(b)
                ? 1.0f
                : Mathf.Max(0.06f, (float)world.Buildings.Progress(b));

            if (!standing.TryGetValue(kind, out var list))
            {
                list = [];
                standing[kind] = list;
            }

            list.Add(new Transform3D(
                Basis.FromScale(new Vector3(1.0f, risen, 1.0f)), new Vector3(x, y, z)));
        }

        foreach (var (kind, instance) in _byKind)
        {
            var placed = standing.GetValueOrDefault(kind);
            var count = placed?.Count ?? 0;
            instance.Multimesh.InstanceCount = count;

            for (var i = 0; i < count; i++)
            {
                instance.Multimesh.SetInstanceTransform(i, placed![i]);
            }
        }
    }
}
