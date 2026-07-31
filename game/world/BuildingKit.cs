using System;
using System.Collections.Generic;
using Godot;

namespace RedRepublic.World;

/// <summary>
/// Assembles one mesh per building kind out of the kit components.
/// </summary>
/// <remarks>
/// <para>
/// The parts — panel walls, roofs, chimneys, silos, a pithead gantry — are
/// modelled in a unit cell. This puts them together according to
/// <see cref="BuildingArt"/>, scaled to the real width and depth the simulation
/// authors.
/// </para>
/// <para>
/// Done once at load, giving one mesh per kind, each drawn as its own multimesh.
/// That keeps the per-instance cost at a transform in a packed buffer however
/// many buildings a republic ends up with.
/// </para>
/// </remarks>
public static class BuildingKit
{
    private const string KitPath = "res://models/kit.glb";

    /// <summary>
    /// The tones, as colours. Broad material families rather than per-building
    /// tints, because the point of a kit is that things which are alike look
    /// alike.
    /// </summary>
    private static readonly Dictionary<Tone, Color> Tones = new()
    {
        [Tone.Render] = new Color(0.78f, 0.75f, 0.69f),
        [Tone.Brick] = new Color(0.54f, 0.36f, 0.30f),
        [Tone.Concrete] = new Color(0.66f, 0.66f, 0.64f),
        [Tone.Metal] = new Color(0.55f, 0.57f, 0.60f),
        [Tone.Timber] = new Color(0.50f, 0.40f, 0.28f),
    };

    /// <summary>Load the kit and pull the component meshes out of it.</summary>
    public static Dictionary<string, Mesh> Components()
    {
        var found = new Dictionary<string, Mesh>();
        if (!ResourceLoader.Exists(KitPath))
        {
            return found;
        }

        var root = GD.Load<PackedScene>(KitPath).Instantiate();
        Collect(root, found);
        root.QueueFree();
        return found;
    }

    private static void Collect(Node node, Dictionary<string, Mesh> into)
    {
        if (node is MeshInstance3D instance && instance.Mesh is not null)
        {
            into[node.Name] = instance.Mesh;
        }

        foreach (var child in node.GetChildren())
        {
            Collect(child, into);
        }
    }

    /// <summary>
    /// Build the mesh for one kind.
    /// </summary>
    /// <remarks>
    /// Width and depth are the real metric footprint the simulation authors;
    /// every vertical dimension comes from the art table. The result stands on
    /// the ground with its origin at the centre of its base, so a placement
    /// transform is the building's position and nothing else.
    /// </remarks>
    public static ArrayMesh Assemble(
        Dictionary<string, Mesh> parts, Art art, float width, float depth)
    {
        ArgumentNullException.ThrowIfNull(parts);

        var st = new SurfaceTool();
        st.Begin(Mesh.PrimitiveType.Triangles);

        var height = art.Height;
        Append(st, Part(parts, "panel_wall"),
            new Transform3D(Basis.FromScale(new Vector3(width, height, depth)), Vector3.Zero));

        var roofName = art.Roof switch
        {
            Roof.Pitch => "roof_pitch",
            Roof.Sawtooth => "roof_sawtooth",
            _ => "roof_flat",
        };

        var roofScale = new Vector3(width, 1.0f, depth);
        roofScale.Y = art.Roof switch
        {
            Roof.Pitch => Mathf.Min(width, depth) * 0.5f,
            Roof.Sawtooth => Mathf.Min(width, depth) * 0.35f,
            _ => 1.0f,
        };

        Append(st, Part(parts, roofName),
            new Transform3D(Basis.FromScale(roofScale), new Vector3(0.0f, height, 0.0f)));

        // Chimneys along the back edge, spaced so two do not overlap.
        var stack = Part(parts, "stack");
        for (var i = 0; stack is not null && i < art.Stacks; i++)
        {
            var span = width * 0.5f;
            var x = art.Stacks == 1
                ? 0.0f
                : Mathf.Lerp(-span * 0.5f, span * 0.5f, (float)i / (art.Stacks - 1));

            Append(st, stack,
                new Transform3D(Basis.Identity, new Vector3(x, height * 0.85f, -depth * 0.32f)));
        }

        // Silos along one flank.
        var silo = Part(parts, "silo");
        for (var i = 0; silo is not null && i < art.Silos; i++)
        {
            var z = art.Silos == 1
                ? 0.0f
                : Mathf.Lerp(-depth * 0.3f, depth * 0.3f, (float)i / (art.Silos - 1));

            Append(st, silo,
                new Transform3D(Basis.Identity, new Vector3((width * 0.5f) + 3.5f, 0.0f, z)));
        }

        // <b>The headframe stands beside the shaft house, not on its roof.</b>
        // Put on top it reads as a toy left on a slab. Scaled with the footprint
        // so it stays the dominant feature of a mine however big the building
        // under it is.
        var gantry = Part(parts, "gantry");
        if (art.Gantry && gantry is not null)
        {
            var g = Mathf.Clamp(Mathf.Min(width, depth) / 30.0f, 0.8f, 2.2f);
            Append(st, gantry, new Transform3D(
                Basis.FromScale(new Vector3(g, g, g)),
                new Vector3(0.0f, 0.0f, (depth * 0.5f) + (9.0f * g))));
        }

        st.GenerateNormals();
        var mesh = st.Commit();

        var body = new StandardMaterial3D
        {
            AlbedoColor = Tones.GetValueOrDefault(art.Tone, new Color(0.75f, 0.72f, 0.66f)),
            Roughness = 0.88f,
            Metallic = art.Tone == Tone.Metal ? 0.35f : 0.0f,
        };

        mesh.SurfaceSetMaterial(0, body);

        // <b>The windows are their own surface, and that is what makes them light
        // up.</b> Masking emission inside the wall material would need a
        // per-vertex channel the assembly does not carry, and a multimesh's own
        // per-instance colour already occupies the one that would have. A second
        // surface costs one more draw call per kind and needs nothing carried.
        //
        // A window band per storey, at real height — a five-metre window is the
        // fastest way to make a factory look like a toy.
        var band = Part(parts, "window_band");
        if (band is null || art.Storeys <= 0)
        {
            return mesh;
        }

        var glass = new SurfaceTool();
        glass.Begin(Mesh.PrimitiveType.Triangles);
        for (var i = 0; i < art.Storeys; i++)
        {
            Append(glass, band, new Transform3D(
                Basis.FromScale(new Vector3(width * 1.004f, 1.0f, depth * 1.004f)),
                new Vector3(0.0f, (i + 0.45f) * art.StoreyM, 0.0f)));
        }

        glass.GenerateNormals();
        glass.Commit(mesh);

        if (ResourceLoader.Exists("res://world/windows.gdshader"))
        {
            mesh.SurfaceSetMaterial(
                mesh.GetSurfaceCount() - 1,
                new ShaderMaterial { Shader = GD.Load<Shader>("res://world/windows.gdshader") });
        }

        return mesh;
    }

    private static Mesh? Part(Dictionary<string, Mesh> parts, string name) =>
        parts.GetValueOrDefault(name);

    private static void Append(SurfaceTool st, Mesh? mesh, Transform3D transform)
    {
        if (mesh is null)
        {
            return;
        }

        for (var surface = 0; surface < mesh.GetSurfaceCount(); surface++)
        {
            var arrays = mesh.SurfaceGetArrays(surface);
            var verts = arrays[(int)Mesh.ArrayType.Vertex].AsVector3Array();
            var indices = arrays[(int)Mesh.ArrayType.Index].AsInt32Array();

            if (indices.Length == 0)
            {
                foreach (var v in verts)
                {
                    st.AddVertex(transform * v);
                }
            }
            else
            {
                foreach (var i in indices)
                {
                    st.AddVertex(transform * verts[i]);
                }
            }
        }
    }
}
