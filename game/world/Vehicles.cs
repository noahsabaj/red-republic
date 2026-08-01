using System;
using System.Collections.Generic;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.World;

/// <summary>
/// The republic's vehicles, drawn where they actually are.
/// </summary>
/// <remarks>
/// <para>
/// <b>The moving half of the world, and nothing drew it.</b> The camera rig's
/// own remarks say the view has to be worth watching a lorry cross a field at,
/// and there was no lorry: the fleet advanced continuous per-vehicle positions
/// every tick and the map showed a still life of buildings. Real time is the
/// thesis, and a world where nothing moves is one nobody has a reason to watch
/// at one second to the second.
/// </para>
/// <para>
/// <b>One multimesh per kind, fed a packed transform buffer</b>, exactly as the
/// buildings are and for the same measured reason — a node per lorry is a node
/// per lorry. Unlike the buildings this is rewritten on every refresh, because
/// unlike the buildings it has actually changed: that is what a vehicle is.
/// </para>
/// <para>
/// The shapes are cut from the same kit the buildings are assembled from, so a
/// lorry is made of the same vocabulary of parts as the shed it drives out of.
/// Nothing here is authored per vehicle beyond a size and a colour: a fleet of
/// distinguishable silhouettes is art, and art is Noah's.
/// </para>
/// </remarks>
public sealed partial class Vehicles : Node3D
{
    private readonly Dictionary<int, MultiMeshInstance3D> _byKind = [];
    private Tables _t = null!;
    private Terrain _terrain = null!;

    public void Raise(Tables tables, Terrain terrain)
    {
        ArgumentNullException.ThrowIfNull(tables);
        _t = tables;
        _terrain = terrain;

        for (var kind = 0; kind < tables.VehicleCount; kind++)
        {
            var instance = new MultiMeshInstance3D
            {
                Name = tables.VehicleIds[kind],
                Multimesh = new MultiMesh
                {
                    TransformFormat = MultiMesh.TransformFormatEnum.Transform3D,
                    Mesh = Body(kind),
                    InstanceCount = 0,
                },

                // A lorry is small and the map is six kilometres. Casting from
                // it costs more than it shows at any distance the camera sits
                // at, and receiving is what makes it look like it is on the
                // ground rather than over it.
                CastShadow = GeometryInstance3D.ShadowCastingSetting.Off,
            };

            AddChild(instance);
            _byKind[kind] = instance;
        }
    }

    /// <summary>Put every vehicle where the republic says it is.</summary>
    /// <remarks>
    /// Bogged vehicles are drawn where they stuck, which is the point of
    /// drawing them at all: a lorry in a field with its load still on it is a
    /// thing the player should be able to see from the camera rather than read
    /// about in a column.
    /// </remarks>
    public void Refresh(Sim.World world)
    {
        ArgumentNullException.ThrowIfNull(world);

        var running = new Dictionary<int, List<Transform3D>>();
        for (var v = 0; v < world.Fleet.Count; v++)
        {
            var kind = world.Fleet.KindAt(v);
            var x = (float)world.Fleet.XAt(v);
            var z = (float)world.Fleet.YAt(v);
            var y = TerrainMesh.GroundAt(_terrain, x, z);

            // Pointed the way it is going. A journey knows where its leg ends,
            // and a vehicle facing its destination is the difference between a
            // fleet and a scattering of boxes.
            var facing = 0.0f;
            if (world.Fleet.JourneyAt(v) is { } journey)
            {
                var toX = (float)journey.LegToX - x;
                var toZ = (float)journey.LegToY - z;
                if (Mathf.Abs(toX) + Mathf.Abs(toZ) > 0.01f)
                {
                    facing = Mathf.Atan2(toX, toZ);
                }
            }

            if (!running.TryGetValue(kind, out var placed))
            {
                placed = [];
                running[kind] = placed;
            }

            // Half its height up, because a box mesh is centred on its origin
            // and the fleet's position is where the wheels are.
            placed.Add(new Transform3D(
                new Basis(Vector3.Up, facing),
                new Vector3(x, y + (Height(kind) * 0.5f), z)));
        }

        foreach (var (kind, instance) in _byKind)
        {
            var placed = running.GetValueOrDefault(kind);
            var count = placed?.Count ?? 0;
            instance.Multimesh.InstanceCount = count;

            for (var i = 0; i < count; i++)
            {
                instance.Multimesh.SetInstanceTransform(i, placed![i]);
            }
        }
    }

    /// <summary>
    /// A shape for one kind of vehicle, sized off the table.
    /// </summary>
    /// <remarks>
    /// Its capacity and its seats decide how big it is, so a heavy lorry is
    /// visibly a heavy lorry and a coach is visibly long — read off the balance
    /// table rather than authored twice, which is the same rule the building
    /// footprints follow.
    /// </remarks>
    private float Height(int kind) =>
        Mathf.Clamp(2.2f + ((float)Carries(kind) * 0.01f), 2.2f, 4.4f);

    private double Carries(int kind) => _t.VCapacity[kind] + (_t.VSeats[kind] * 0.08);

    private BoxMesh Body(int kind)
    {
        var carries = Carries(kind);
        var length = Mathf.Clamp(4.5f + ((float)carries * 0.12f), 4.5f, 22.0f);
        var width = Mathf.Clamp(2.0f + ((float)carries * 0.01f), 2.0f, 3.4f);
        var tall = Height(kind);

        return new BoxMesh
        {
            Size = new Vector3(width, tall, length),
            Material = new StandardMaterial3D
            {
                AlbedoColor = Ui.Palette.Vehicle,
                Roughness = 0.7f,
                Metallic = 0.1f,
            },
        };
    }
}
