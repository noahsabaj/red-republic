using System;
using Godot;
using RedRepublic.Sim;

namespace RedRepublic.World;

/// <summary>
/// The heightmap, as one mesh.
/// </summary>
/// <remarks>
/// <para>
/// <b>One mesh, not a chunked, LOD-ed, streamed one</b> — and that was measured
/// rather than assumed. A 10 km map at 10 m resolution is a million cells and
/// nearly two million triangles, and building plus uploading the whole thing
/// cost about thirty milliseconds once, at load. Chunking and LOD at this size
/// would be architecture bought against a problem nobody has. What <i>is</i>
/// real is the first frame's pipeline compilation, which makes a loading state a
/// genuine requirement rather than a nicety.
/// </para>
/// <para>
/// <b>Resolution is the terrain's, not ours.</b> The cell size is carried on the
/// map rather than read from a constant, so a save always knows what it was
/// written at, and this walks whatever it finds instead of assuming ten metres.
/// </para>
/// </remarks>
public static class TerrainMesh
{
    /// <summary>Build the surface arrays for an array mesh.</summary>
    /// <remarks>
    /// Colour carries the surface kind rather than a texture, so the terrain
    /// material can read it and decide what grass, forest, rock and water look
    /// like. That keeps the art decision in the shader, where it can be changed
    /// from a mockup without touching this.
    /// </remarks>
    public static Godot.Collections.Array Surface(Terrain terrain)
    {
        ArgumentNullException.ThrowIfNull(terrain);

        var cells = terrain.Cells;
        var size = (float)terrain.CellSize;
        var side = cells + 1;

        var vertices = new Vector3[side * side];
        var normals = new Vector3[side * side];
        var uvs = new Vector2[side * side];
        var colours = new Color[side * side];
        var indices = new int[cells * cells * 6];

        for (var vy = 0; vy < side; vy++)
        {
            for (var vx = 0; vx < side; vx++)
            {
                var at = (vy * side) + vx;
                var h = (float)HeightOfCorner(terrain, vx, vy);
                vertices[at] = new Vector3(vx * size, h, vy * size);

                // Central differences over the neighbouring corners. Cheap, and
                // at ten metres indistinguishable from anything cleverer.
                var left = HeightOfCorner(terrain, vx - 1, vy);
                var right = HeightOfCorner(terrain, vx + 1, vy);
                var down = HeightOfCorner(terrain, vx, vy - 1);
                var up = HeightOfCorner(terrain, vx, vy + 1);
                normals[at] = new Vector3(
                    (float)(left - right), 2.0f * size, (float)(down - up)).Normalized();

                uvs[at] = new Vector2((float)vx / side, (float)vy / side);
                colours[at] = SurfaceColour(SurfaceOfCorner(terrain, vx, vy));
            }
        }

        var n = 0;
        for (var y = 0; y < cells; y++)
        {
            for (var x = 0; x < cells; x++)
            {
                var i = (y * side) + x;
                var right = i + 1;
                var below = i + side;
                var both = below + 1;

                // Two triangles, wound so the face points <b>up</b>.
                //
                // Godot treats counter-clockwise as front-facing and culls the
                // back. Wound the other way the whole map faces the underworld
                // and renders as nothing at all — and the numbers cannot see it:
                // every vertex uploads fine and the frame is empty. Only looking
                // at a rendered frame says so.
                indices[n++] = i;
                indices[n++] = right;
                indices[n++] = below;
                indices[n++] = right;
                indices[n++] = both;
                indices[n++] = below;
            }
        }

        var arrays = new Godot.Collections.Array();
        arrays.Resize((int)Mesh.ArrayType.Max);
        arrays[(int)Mesh.ArrayType.Vertex] = vertices;
        arrays[(int)Mesh.ArrayType.Normal] = normals;
        arrays[(int)Mesh.ArrayType.TexUV] = uvs;
        arrays[(int)Mesh.ArrayType.Color] = colours;
        arrays[(int)Mesh.ArrayType.Index] = indices;
        return arrays;
    }

    /// <summary>Sample a point's height, for anything that has to sit on the ground.</summary>
    public static float GroundAt(Terrain terrain, double x, double z)
    {
        ArgumentNullException.ThrowIfNull(terrain);
        return (float)(terrain.HeightAt(x, z) ?? 0.0);
    }

    /// <summary>
    /// Vertices sit on cell corners, so each samples the cell that owns it,
    /// clamped at the far edges so the skirt closes.
    /// </summary>
    private static double HeightOfCorner(Terrain terrain, int vx, int vy)
    {
        var (x, y) = CornerCentre(terrain, vx, vy);
        return terrain.HeightAt(x, y) ?? 0.0;
    }

    private static Sim.Surface? SurfaceOfCorner(Terrain terrain, int vx, int vy)
    {
        var (x, y) = CornerCentre(terrain, vx, vy);
        return terrain.SurfaceAt(x, y);
    }

    private static (double X, double Y) CornerCentre(Terrain terrain, int vx, int vy) =>
        (terrain.CellCentre(Math.Clamp(vx, 0, terrain.Cells - 1)),
         terrain.CellCentre(Math.Clamp(vy, 0, terrain.Cells - 1)));

    /// <summary>
    /// A flat identifier per surface kind, carried in vertex colour.
    /// </summary>
    /// <remarks>
    /// Deliberately <b>not</b> the final look. These are channel markers the
    /// material reads, so the art direction lives in the shader rather than here
    /// — which is what lets the look be chosen from mockups without recompiling.
    /// </remarks>
    private static Color SurfaceColour(Sim.Surface? surface) => surface switch
    {
        Sim.Surface.Forest => new Color(0.0f, 1.0f, 0.0f, 1.0f),
        Sim.Surface.Rock => new Color(0.0f, 0.0f, 1.0f, 1.0f),
        Sim.Surface.Water => new Color(0.0f, 0.0f, 0.0f, 1.0f),
        _ => new Color(1.0f, 0.0f, 0.0f, 1.0f),
    };
}
