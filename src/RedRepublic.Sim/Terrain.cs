namespace RedRepublic.Sim;

public enum Surface : byte
{
    Grass,
    Forest,
    Rock,
    Water,
}

/// <summary>
/// The ground of one republic, and the generator that shapes it.
/// </summary>
/// <remarks>
/// <para>
/// <b>Space is continuous; the grid describes terrain, not buildings.</b> There
/// is no occupancy model here — a cell says what the ground is like at a place,
/// and buildings sit at real positions with real footprints. The word "tile"
/// does not belong in this vocabulary.
/// </para>
/// <para>
/// <b>Heights and flow are <c>float</c>, and that is load-bearing.</b> The
/// depression fill raises a hollow to one <c>float</c> ulp above its lip.
/// Widening to <c>double</c> would generate a different landscape from the same
/// seed, because a double's ulp is about 2²⁹ times smaller — still
/// deterministic, and not the map the seed promised.
/// </para>
/// <para>
/// <b>Determinism.</b> Integer hashing plus polynomial interpolation: no
/// transcendentals, no hash iteration order, nothing address-dependent. Map
/// generation must reproduce across machines, because a shared seed is a promise
/// between players.
/// </para>
/// </remarks>
public sealed class Terrain
{
    private Terrain(int cells, double cellSize)
    {
        Cells = cells;
        CellSize = cellSize;
        Height = new float[cells * cells];
        Surfaces = new Surface[cells * cells];
    }

    /// <summary>Cells along one side.</summary>
    public int Cells { get; }

    /// <summary>
    /// Metres across one sample cell. Carried on the map rather than read from a
    /// constant, so a save always knows the resolution it was written at instead
    /// of inheriting whatever the build currently defaults to.
    /// </summary>
    public double CellSize { get; }

    /// <summary>Metres above the datum, one per cell, row-major.</summary>
    public float[] Height { get; }

    /// <summary>One surface per cell, row-major.</summary>
    public Surface[] Surfaces { get; }

    /// <summary>How far the map reaches, in metres.</summary>
    public double Extent => Cells * CellSize;

    public static bool IsBuildable(Surface s) => s is Surface.Grass or Surface.Forest;

    public static bool IsWalkable(Surface s) => s != Surface.Water;

    /// <summary>
    /// A flat grass square <paramref name="extent"/> metres on a side — the test
    /// fixture, and the base a generator builds on.
    /// </summary>
    public static Terrain Flat(double extent, double cellSize)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(cellSize);
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(extent);
        return new Terrain((int)Math.Ceiling(extent / cellSize), cellSize);
    }

    public bool Contains(double x, double y)
    {
        var e = Extent;
        return x >= 0.0 && x < e && y >= 0.0 && y < e;
    }

    /// <summary>Index of the cell a point falls in, or -1 off the map.</summary>
    public int IndexOf(double x, double y) =>
        Contains(x, y) ? ((int)(y / CellSize) * Cells) + (int)(x / CellSize) : -1;

    /// <summary>
    /// The centre of a cell, in metres — the bridge from lattice back to the
    /// continuous world, and the only direction that conversion should ever go
    /// outside this file.
    /// </summary>
    public double CellCentre(int c) => (c + 0.5) * CellSize;

    public Surface? SurfaceAt(double x, double y)
    {
        var i = IndexOf(x, y);
        return i < 0 ? null : Surfaces[i];
    }

    public double? HeightAt(double x, double y)
    {
        var i = IndexOf(x, y);
        return i < 0 ? null : Height[i];
    }

    public void SetSurfaceAt(double x, double y, Surface s)
    {
        var i = IndexOf(x, y);
        if (i >= 0)
        {
            Surfaces[i] = s;
        }
    }

    public void SetHeightAt(double x, double y, double h)
    {
        var i = IndexOf(x, y);
        if (i >= 0)
        {
            Height[i] = (float)h;
        }
    }

    /// <summary>
    /// Whether a straight run between two points crosses open water.
    /// </summary>
    /// <remarks>
    /// Sampled at half-cell steps so a river cannot be stepped over. What a road
    /// order asks before it decides whether it is looking at a road or at a
    /// bridge — without it a gravel road could be laid straight across a river
    /// at the price of gravel.
    /// </remarks>
    public bool CrossesWater(double ax, double ay, double bx, double by)
    {
        var d = Units.Distance(ax, ay, bx, by);
        var steps = Math.Clamp((int)Math.Ceiling(d / (CellSize * 0.5)), 1, 512);
        for (var step = 0; step <= steps; step++)
        {
            var t = (double)step / steps;
            if (SurfaceAt(ax + ((bx - ax) * t), ay + ((by - ay) * t)) == Surface.Water)
            {
                return true;
            }
        }

        return false;
    }

    /// <summary>
    /// Whether every cell a rectangle touches is buildable — the check a
    /// placement makes.
    /// </summary>
    /// <remarks>
    /// Samples on the cell lattice rather than testing corner points, so a
    /// footprint cannot straddle a lake by landing its corners on dry ground.
    /// </remarks>
    public bool AreaIsBuildable(double cx, double cy, double width, double depth)
    {
        var minX = cx - (width / 2.0);
        var maxX = cx + (width / 2.0);
        var minY = cy - (depth / 2.0);
        var maxY = cy + (depth / 2.0);
        if (minX < 0.0 || minY < 0.0)
        {
            return false;
        }

        var e = Extent;
        if (maxX > e || maxY > e)
        {
            return false;
        }

        var loX = (int)(minX / CellSize);
        var hiX = Math.Min((int)Math.Ceiling(maxX / CellSize), Cells);
        var loY = (int)(minY / CellSize);
        var hiY = Math.Min((int)Math.Ceiling(maxY / CellSize), Cells);

        for (var gy = loY; gy < hiY; gy++)
        {
            for (var gx = loX; gx < hiX; gx++)
            {
                if (!IsBuildable(Surfaces[(gy * Cells) + gx]))
                {
                    return false;
                }
            }
        }

        return true;
    }

    /// <summary>
    /// Fraction of the map covered by one surface — a candidate card wants to
    /// say how much of a posting is forest or water.
    /// </summary>
    public double FractionOf(Surface s)
    {
        if (Surfaces.Length == 0)
        {
            return 0.0;
        }

        var hits = 0;
        foreach (var v in Surfaces)
        {
            if (v == s)
            {
                hits++;
            }
        }

        return (double)hits / Surfaces.Length;
    }

    // ---- deterministic value noise ----

    public static ulong HashCell(ulong seed, long x, long y)
    {
        var h = seed;
        h ^= (ulong)x * 0x9E37_79B9_7F4A_7C15;
        h = ulong.RotateLeft(h, 29);
        h ^= (ulong)y * 0xBF58_476D_1CE4_E5B9;
        h = ulong.RotateLeft(h, 31);
        h ^= h >> 27;
        return h * 0x94D0_49BB_1331_11EB;
    }

    /// <summary>Lattice value in <c>[0, 1)</c>.</summary>
    public static double Lattice(ulong seed, long x, long y)
    {
        const double scale = 1.0 / (1UL << 53);
        return (HashCell(seed, x, y) >> 11) * scale;
    }

    /// <summary>Smoothstep — a polynomial, so exact everywhere.</summary>
    public static double Smooth(double t) => t * t * (3.0 - (2.0 * t));

    /// <summary>One octave of value noise at a given feature size, in <c>[0, 1)</c>.</summary>
    public static double ValueNoise(ulong seed, double px, double py, double feature)
    {
        var fx = px / feature;
        var fy = py / feature;
        var x0 = Math.Floor(fx);
        var y0 = Math.Floor(fy);
        var tx = Smooth(fx - x0);
        var ty = Smooth(fy - y0);
        var ix = (long)x0;
        var iy = (long)y0;

        var n00 = Lattice(seed, ix, iy);
        var n10 = Lattice(seed, ix + 1, iy);
        var n01 = Lattice(seed, ix, iy + 1);
        var n11 = Lattice(seed, ix + 1, iy + 1);

        var top = n00 + ((n10 - n00) * tx);
        var bottom = n01 + ((n11 - n01) * tx);
        return top + ((bottom - top) * ty);
    }

    /// <summary>Several octaves summed — big shapes with small detail on top.</summary>
    public static double FractalNoise(ulong seed, double px, double py, double feature, int octaves)
    {
        var total = 0.0;
        var amplitude = 1.0;
        var sum = 0.0;
        var size = feature;
        for (var octave = 0; octave < octaves; octave++)
        {
            total += ValueNoise(seed + (ulong)octave, px, py, size) * amplitude;
            sum += amplitude;
            amplitude *= 0.5;
            size *= 0.5;
        }

        return total / sum;
    }

    /// <summary>
    /// Generate the ground of a square map.
    /// </summary>
    /// <remarks>
    /// The noise field is a continuous function of position, so changing the
    /// cell size re-samples the <i>same</i> landscape more or less finely rather
    /// than generating a different one.
    /// </remarks>
    public static Terrain Generate(ulong seed, double extent, Tables tables)
    {
        ArgumentNullException.ThrowIfNull(tables);
        var t = Flat(extent, tables.TerrainCellSize);
        var n = t.Cells;
        for (var cy = 0; cy < n; cy++)
        {
            var py = t.CellCentre(cy);
            var row = cy * n;
            for (var cx = 0; cx < n; cx++)
            {
                var v = FractalNoise(
                    seed, t.CellCentre(cx), py, tables.TerrainFeatureSize, tables.TerrainOctaves);
                var i = row + cx;
                t.Surfaces[i] = v < tables.TerrainWaterBelow ? Surface.Water
                    : v > tables.TerrainRockAbove ? Surface.Rock
                    : v > tables.TerrainForestAbove ? Surface.Forest
                    : Surface.Grass;
                t.Height[i] = (float)(v * tables.TerrainRelief);
            }
        }

        t.CarveRivers(tables);
        return t;
    }

    /// <summary>
    /// Cut river channels by following the water downhill.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Thresholding noise gives <b>lakes, not rivers</b>: isolated basins
    /// wherever the field dips below the water threshold, with nothing joining
    /// them. Measured across three seeds before this existed, a 6 km map had
    /// between 0% and 3.2% water in at most six disconnected bodies — a map on
    /// which a bridge almost never has a river to span and a barge has nowhere
    /// to go.
    /// </para>
    /// <para>
    /// A river is not a shape you draw, it is where the water ends up. So: every
    /// cell sheds its own rain, each shove goes to the lowest neighbour, and a
    /// cell carrying more than the catchment threshold is a channel. The result
    /// is dendritic and connected by construction.
    /// </para>
    /// </remarks>
    private void CarveRivers(Tables tables)
    {
        var n = Cells;
        var count = n * n;
        if (count == 0)
        {
            return;
        }

        // Fill the hollows before following the water. Measured on the first
        // version, which did not: 601 disconnected channels on a 6 km map, none
        // spanning more than 1.7 km. Multi-octave noise is full of local minima,
        // so a channel ran to the nearest pit a few hundred metres later and
        // stopped. Flow accumulation only tells you where water goes if there is
        // somewhere for it to go.
        var (filled, uphill) = FillDepressions();

        // Every cell sheds its own rain, and passes on whatever reached it.
        var flow = new float[count];
        Array.Fill(flow, 1.0f);

        // Which way each cell drains, kept so the channel can be carved along
        // the line the water actually takes rather than guessed at afterwards.
        var down = new int[count];
        Array.Fill(down, -1);

        for (var k = uphill.Count - 1; k >= 0; k--)
        {
            var i = uphill[k];
            var x = i % n;
            var y = i / n;
            var here = filled[i];

            // The steepest way down. Ties keep the neighbour found first, and
            // the neighbours are walked in a fixed order — so two equally low
            // ways down always resolve the same way on any machine.
            var best = 0.0f;
            var bestAt = -1;
            for (var dy = -1; dy <= 1; dy++)
            {
                var ny = y + dy;
                if (ny < 0 || ny >= n)
                {
                    continue;
                }

                for (var dx = -1; dx <= 1; dx++)
                {
                    if (dx == 0 && dy == 0)
                    {
                        continue;
                    }

                    var nx = x + dx;
                    if (nx < 0 || nx >= n)
                    {
                        continue;
                    }

                    var j = (ny * n) + nx;
                    var h = filled[j];
                    if (h >= here)
                    {
                        continue;
                    }

                    if (bestAt < 0 || h < best)
                    {
                        best = h;
                        bestAt = j;
                    }
                }
            }

            // Nowhere lower to go. After the fill this only happens at the map
            // edge, which is where the water leaves the republic.
            if (bestAt >= 0)
            {
                flow[bestAt] += flow[i];
                down[i] = bestAt;
            }
        }

        var river = (float)(tables.TerrainRiverCatchment * count);
        var broad = (float)(tables.TerrainBroadCatchment * count);
        if (river <= 0.0f)
        {
            return;
        }

        // Two passes, because widening reads the channel it is widening: marking
        // as we go would let a freshly widened cell seed further widening and
        // the rivers would creep outward across the map.
        var channels = new List<int>();
        for (var i = 0; i < count; i++)
        {
            if (flow[i] >= river)
            {
                channels.Add(i);
            }
        }

        foreach (var i in channels)
        {
            Surfaces[i] = Surface.Water;

            // A diagonal step is not a continuous river. Water flows to any of
            // eight neighbours, so a channel running across the grain is a chain
            // of cells touching only at their corners — which reads as a river
            // on a map and is not one on the ground: a road crosses it between
            // the corners without ever being over water. Measured before this
            // was here: 187 disconnected bodies on a 6 km map, which is one
            // river reported as thirty.
            var j = down[i];
            if (j < 0 || j >= count)
            {
                continue;
            }

            var x = i % n;
            var y = i / n;
            var jx = j % n;
            var jy = j / n;
            if (jx != x && jy != y)
            {
                // Of the two cells that square off the corner, take the lower —
                // that is the one the water would actually have cut through.
                var a = (y * n) + jx;
                var b = (jy * n) + x;
                Surfaces[Height[a] <= Height[b] ? a : b] = Surface.Water;
            }
        }

        foreach (var i in channels)
        {
            if (flow[i] < broad)
            {
                continue;
            }

            var x = i % n;
            var y = i / n;
            foreach (var (dx, dy) in new[] { (-1, 0), (1, 0), (0, -1), (0, 1) })
            {
                var nx = x + dx;
                var ny = y + dy;
                if (nx < 0 || ny < 0 || nx >= n || ny >= n)
                {
                    continue;
                }

                Surfaces[(ny * n) + nx] = Surface.Water;
            }
        }
    }

    /// <summary>
    /// Raise every hollow to the level of its lowest lip, so that from anywhere
    /// on the map the water can find its way to the edge.
    /// </summary>
    /// <remarks>
    /// <para>
    /// Priority-flood: start from the border, always take the lowest cell seen
    /// so far, and pull each neighbour up to at least that height. A cell can
    /// only be reached across the <i>lowest</i> rim between it and the outside,
    /// so the height it is pulled up to is exactly the level a real pond in that
    /// hollow would stand at.
    /// </para>
    /// <para>
    /// The terrain keeps its real heights — a filled basin is not a hill that
    /// got taller, it is a place where a lake would sit, and the map should
    /// still say how deep it is. The visit order comes back ascending by filled
    /// height, which is exactly what flow accumulation needs reversed; handing
    /// it back saves sorting a million cells for an answer this pass has already
    /// worked out.
    /// </para>
    /// <para>
    /// <b>A bucket queue, not a heap.</b> Priority-flood only ever pushes at a
    /// level at or above the one it is draining, so the frontier is monotone —
    /// the same property that lets Dijkstra with bounded integer weights use a
    /// bucket queue, and the pass becomes linear. The price is that two cells
    /// within a centimetre are treated as level, so a filled pond can stand up
    /// to a centimetre off where an exact fill would put it — against 120 m of
    /// relief, and only in deciding which way a river leaves a plateau.
    /// </para>
    /// </remarks>
    private (float[] Filled, List<int> Visited) FillDepressions()
    {
        // How finely the frontier is banded. A centimetre: fine enough that the
        // fill is exact for anything the simulation can see, coarse enough that
        // the level array stays small.
        const float step = 0.01f;

        var n = Cells;
        var count = n * n;
        var filled = (float[])Height.Clone();
        var visited = new List<int>(count);
        if (count == 0)
        {
            return (filled, visited);
        }

        var lowest = float.PositiveInfinity;
        var highest = float.NegativeInfinity;
        foreach (var h in Height)
        {
            if (h < lowest)
            {
                lowest = h;
            }

            if (h > highest)
            {
                highest = h;
            }
        }

        // One past the top, so a cell raised to the highest lip still has a home.
        var levels = LevelOf(highest, lowest, step) + 2;

        var closed = new bool[count];
        var frontier = new List<int>[levels];
        for (var l = 0; l < levels; l++)
        {
            frontier[l] = [];
        }

        for (var k = 0; k < n; k++)
        {
            foreach (var i in new[] { k, ((n - 1) * n) + k, k * n, (k * n) + n - 1 })
            {
                if (!closed[i])
                {
                    closed[i] = true;
                    frontier[Math.Min(LevelOf(filled[i], lowest, step), levels - 1)].Add(i);
                }
            }
        }

        // Walk the levels from the bottom. A level can grow while it is being
        // drained — a neighbour pulled up to the current lip lands back in this
        // same band — so the inner loop indexes rather than iterates.
        for (var level = 0; level < levels; level++)
        {
            for (var cursor = 0; cursor < frontier[level].Count; cursor++)
            {
                var i = frontier[level][cursor];
                visited.Add(i);
                var x = i % n;
                var y = i / n;

                // A hair above, never level. Raising a hollow to exactly its lip
                // makes the basin floor perfectly flat, and a flat has no lower
                // neighbour — so the water arrives in the middle of a filled
                // lake and stops, which is the failure this pass exists to fix
                // moved one step along. Because the fill spreads outward from
                // the outlet, adding one ulp per step leaves a gradient running
                // back the way it came, which is the way out.
                var lip = float.BitIncrement(filled[i]);

                for (var dy = -1; dy <= 1; dy++)
                {
                    var ny = y + dy;
                    if (ny < 0 || ny >= n)
                    {
                        continue;
                    }

                    for (var dx = -1; dx <= 1; dx++)
                    {
                        if (dx == 0 && dy == 0)
                        {
                            continue;
                        }

                        var nx = x + dx;
                        if (nx < 0 || nx >= n)
                        {
                            continue;
                        }

                        var j = (ny * n) + nx;
                        if (closed[j])
                        {
                            continue;
                        }

                        closed[j] = true;
                        if (filled[j] < lip)
                        {
                            filled[j] = lip;
                        }

                        frontier[Math.Min(LevelOf(filled[j], lowest, step), levels - 1)].Add(j);
                    }
                }
            }

            // Done with this band; give the memory back rather than holding a
            // million cells' worth of queues to the end of the pass.
            frontier[level] = [];
        }

        return (filled, visited);
    }

    private static int LevelOf(float h, float lowest, float step)
    {
        var band = (h - lowest) / step;
        return band <= 0.0f ? 0 : (int)band;
    }
}
