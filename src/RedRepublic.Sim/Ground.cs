namespace RedRepublic.Sim;

/// <summary>
/// The state of the ground: how wet it is, how frozen, and what that costs to
/// drive across.
/// </summary>
/// <remarks>
/// <para>
/// <b>Why this is state and not a function of the calendar.</b> Weather makes
/// today's temperature and rain pure functions of <c>(seed, day)</c>, so a
/// forecast never perturbs anything. Soil is not like that. Water that fell last
/// week is still in the ground this week; snow that fell in December is still
/// lying in February and comes off all at once in March. Moisture and snow are
/// accumulated state, and modelling them as a function of the date would throw
/// away the only interesting thing about them.
/// </para>
/// <para>
/// <b>The spring thaw falls out; it is not written down.</b> Three rules, none
/// of which mentions spring: snow lies while it is below freezing and melts
/// above it, meltwater goes into the topsoil like rain does, and frost lags the
/// air temperature because soil has thermal mass. Run them through a winter and
/// the worst going of the year lands a week or so after the first warm spell —
/// a season's snow arriving in the topsoil at once while the frost that was
/// holding the ground up is still on its way out. That is the <i>rasputitsa</i>,
/// and it is the seasonal event this whole model exists to produce.
/// </para>
/// <para>
/// Everything here is one figure for the <b>whole republic</b>: it does not rain
/// on half a ten-kilometre map. What varies by place is the surface, which is
/// static and lives on the terrain, and the wear and clearance, which live on
/// <see cref="Lattice"/>.
/// </para>
/// </remarks>
public struct Ground()
{
    /// <summary>
    /// Water in the topsoil, 0 bone dry to 1 saturated.
    /// </summary>
    /// <remarks>
    /// <b>The trafficability figure, not the agricultural one.</b> It is the top
    /// few centimetres, it decides whether a lorry sinks, and it is dry most of
    /// the summer. Crops read <see cref="Water"/>.
    /// </remarks>
    public double Moisture { get; set; } = 0.5;

    /// <summary>
    /// Water in the root zone, 0 exhausted to 1 full.
    /// </summary>
    /// <remarks>
    /// Fed by the same rain and meltwater as <see cref="Moisture"/> and drained
    /// by the same warmth, but four times the reservoir and an eighth the drying
    /// rate — so it carries a crop through a dry fortnight the way real subsoil
    /// does. A peer field rather than derived, because the two answer genuinely
    /// different questions and neither is a function of the other.
    /// </remarks>
    public double Water { get; set; } = 0.5;

    /// <summary>Snow lying, in millimetres of water equivalent.</summary>
    public double Snow { get; set; }

    /// <summary>How frozen the ground is, 0 soft to 1 set hard.</summary>
    /// <remarks>
    /// A republic is founded on 1 March, at the end of a winter it did not
    /// simulate. Starting bone dry and unfrozen would hand every founding a
    /// spring that never happened; damp and part-frozen is the honest guess, and
    /// a week of real weather washes it out either way.
    /// </remarks>
    public double Frost { get; set; } = 0.3;

    /// <summary>Take one day of weather.</summary>
    public void Advance(double temperatureC, double precipitationMm, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);

        // Frost lags the air, because soil has thermal mass. This is what makes
        // the thaw an event rather than a switch: the ground is still holding
        // itself up for days after the air has turned.
        var target = Math.Clamp((t.FreezeC - temperatureC) / t.FrostRangeC, 0.0, 1.0);
        Frost += (target - Frost) * t.FrostLag;

        var freezing = temperatureC < t.FreezeC;
        var melt = freezing
            ? 0.0
            : Math.Min((temperatureC - t.FreezeC) * t.MeltPerDegreeMm, Snow);

        // Below freezing it falls as snow and lies; above it, it runs straight
        // into the ground along with whatever the pack is giving up.
        var fellAsSnow = freezing ? precipitationMm : 0.0;
        Snow = Math.Max(0.0, Snow + fellAsSnow - melt);

        var water = (freezing ? 0.0 : precipitationMm) + melt;
        Moisture = Math.Min(1.0, Moisture + (water / t.SaturationMm));
        Water = Math.Min(1.0, Water + (water / t.RootSaturationMm));

        // It dries out only when it is warm and there is nothing lying on top.
        if (!freezing && Snow <= 0.0)
        {
            var warmth = Math.Clamp((temperatureC - t.FreezeC) / t.DryingFullAtC, 0.0, 1.5);
            Moisture = Math.Max(0.0, Moisture - (t.DryingPerDay * warmth));
            Water = Math.Max(0.0, Water - (t.RootDryingPerDay * warmth));
        }
    }

    /// <summary>
    /// How badly the open ground would bog a vehicle today: 0 firm, 1
    /// impassable.
    /// </summary>
    /// <remarks>
    /// <b>Frozen ground is hard however wet it is.</b> A frozen bog is a road,
    /// and that is not a quirk of the arithmetic — it is why winter haulage
    /// across country is easier than spring haulage, and why the thaw is the
    /// event rather than the rain.
    /// </remarks>
    public readonly double Softness => Math.Clamp(Moisture * (1.0 - Frost), 0.0, 1.0);

    /// <summary>How much of a stopping depth of snow is lying, 0 bare to 1.</summary>
    public readonly double SnowLoad(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return Math.Clamp(Snow / t.SnowBlocksMm, 0.0, 1.0);
    }

    /// <summary>What the going is on a particular surface today.</summary>
    public readonly double GoingOn(Surface surface, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var still = t.Going(surface);
        return double.IsInfinity(still)
            ? double.PositiveInfinity
            : Math.Clamp((Softness + SnowLoad(t)) * still, 0.0, 1.0);
    }

    /// <summary>
    /// The same, rolled forward some days from here.
    /// </summary>
    /// <remarks>
    /// A forecast, and the reason it can exist at all is that temperature and
    /// rain are pure: rolling the recurrence forward from today costs one
    /// substream draw per day and moves nothing.
    /// </remarks>
    public readonly Ground Forecast(
        Func<long, (double TemperatureC, double RainMm)> weather,
        long fromDay,
        int days,
        Tables t)
    {
        ArgumentNullException.ThrowIfNull(weather);
        var ahead = this;
        for (var step = 0; step < days; step++)
        {
            var (temperature, rain) = weather(fromDay + step + 1);
            ahead.Advance(temperature, rain, t);
        }

        return ahead;
    }
}

/// <summary>
/// The lattice a vehicle crosses country over.
/// </summary>
/// <remarks>
/// <para>
/// Carries the <b>static</b> part of the going — what the surface is — because
/// that is what varies by place. How wet it is today is one number for the whole
/// republic and lives on <see cref="Ground"/>. Wear rides here too, on the same
/// cells, so the thing that records a corridor and the thing that routes over
/// one are the same structure.
/// </para>
/// <para>
/// A hundred metres a side, which makes a ten-kilometre republic a 100×100
/// lattice — ten thousand cells against the million the terrain grid holds. That
/// two-orders-of-magnitude gap is what makes routing across country affordable
/// at all: what varies at ten metres is where a building can stand, and what
/// varies at a hundred is where a lorry would rather drive.
/// </para>
/// </remarks>
public sealed class Lattice
{
    /// <summary>Static going multiplier per cell, infinite where nothing crosses.</summary>
    private readonly float[] _surface;

    /// <summary>How worn each cell is, 0 untouched to 1 a made track.</summary>
    private readonly float[] _wear;

    /// <summary>How dirty the air and ground are here, 0 clean to 1 foul.</summary>
    private readonly float[] _pollution;

    /// <summary>
    /// How recently a plough came through, 0 buried to 1 clear.
    /// </summary>
    /// <remarks>
    /// How much snow lies is one figure for the whole republic and lives on
    /// <see cref="Ground"/>; <i>where it has been pushed aside</i> is a place,
    /// and lives here. One everywhere when nothing is lying, so a republic in
    /// July is not carrying a field of stale clearance values.
    /// </remarks>
    private readonly float[] _cleared;

    private Lattice(int cells, double cellSize)
    {
        Cells = cells;
        CellSize = cellSize;
        var total = cells * cells;
        _surface = new float[total];
        _wear = new float[total];
        _pollution = new float[total];
        _cleared = new float[total];
        Array.Fill(_cleared, 1.0f);
    }

    public int Cells { get; }

    /// <summary>
    /// Persisted rather than read from the table, for the same reason the
    /// terrain carries its own resolution: a save always knows what it was
    /// written at, and re-measuring stays a one-line experiment.
    /// </summary>
    public double CellSize { get; }

    /// <summary>
    /// Build the lattice by sampling the terrain.
    /// </summary>
    /// <remarks>
    /// Each cell reads a 5×5 grid of the ground under it, so a cell is a summary
    /// of what is actually there rather than of whatever happened to be at its
    /// centre. <b>A cell that is a quarter water is water</b>: you cannot drive
    /// round the corner of a lake inside a hundred-metre square.
    /// </remarks>
    public static Lattice FromTerrain(Terrain terrain, Tables t)
    {
        ArgumentNullException.ThrowIfNull(terrain);
        ArgumentNullException.ThrowIfNull(t);

        var cellSize = t.GroundCellSize;
        var cells = Math.Max(1, (int)Math.Ceiling(terrain.Extent / cellSize));
        var lattice = new Lattice(cells, cellSize);

        const int samples = 5;
        for (var cy = 0; cy < cells; cy++)
        {
            for (var cx = 0; cx < cells; cx++)
            {
                var sum = 0.0;
                var dry = 0;
                var wet = 0;
                for (var sy = 0; sy < samples; sy++)
                {
                    for (var sx = 0; sx < samples; sx++)
                    {
                        var x = (cx + ((sx + 0.5) / samples)) * cellSize;
                        var y = (cy + ((sy + 0.5) / samples)) * cellSize;
                        var s = terrain.SurfaceAt(x, y);
                        if (s is null || s == Surface.Water)
                        {
                            wet++;
                        }
                        else
                        {
                            sum += t.Going(s.Value);
                            dry++;
                        }
                    }
                }

                var seen = wet + dry;
                var drowned = seen == 0 || (double)wet / seen >= t.Drowned;
                lattice._surface[(cy * cells) + cx] =
                    drowned ? float.PositiveInfinity : (float)(sum / dry);
            }
        }

        return lattice;
    }

    /// <summary>The cell a point falls in, or -1 off the map.</summary>
    public int CellOf(double x, double y)
    {
        if (x < 0.0 || y < 0.0)
        {
            return -1;
        }

        var cx = (int)(x / CellSize);
        var cy = (int)(y / CellSize);
        return cx >= Cells || cy >= Cells ? -1 : (cy * Cells) + cx;
    }

    public double CentreX(int index) => ((index % Cells) + 0.5) * CellSize;

    public double CentreY(int index) => ((index / Cells) + 0.5) * CellSize;

    public double SurfaceAt(int index) => _surface[index];

    public double WearAt(int index) => _wear[index];

    /// <summary>How clear of snow a cell is, 0 buried to 1 swept.</summary>
    public double ClearedAt(int index) => _cleared[index];

    /// <summary>The clearance at a point, or clear off the map.</summary>
    public double ClearedNear(double x, double y)
    {
        var c = CellOf(x, y);
        return c < 0 ? 1.0 : ClearedAt(c);
    }

    /// <summary>A plough came through here.</summary>
    public void Clear(int index) => _cleared[index] = 1.0f;

    /// <summary>
    /// Put one cell back exactly as it was — what a load does.
    /// </summary>
    /// <remarks>
    /// The three fields a republic writes into the ground over its life. The
    /// surface going is not among them: that is sampled from the terrain, which
    /// is a pure function of the seed and is regenerated rather than stored.
    /// </remarks>
    public void Restore(int index, double wear, double pollution, double cleared)
    {
        _wear[index] = (float)wear;
        _pollution[index] = (float)pollution;
        _cleared[index] = (float)cleared;
    }

    /// <summary>
    /// Snow falling on everything, <paramref name="by"/> being the share of
    /// clearance it undoes.
    /// </summary>
    /// <remarks>
    /// Applied to the <b>whole lattice</b> rather than to roads, because snow
    /// does not know where the roads are. What knows is the plough.
    /// </remarks>
    public void Bury(double by)
    {
        var share = Math.Clamp(by, 0.0, 1.0);
        for (var i = 0; i < _cleared.Length; i++)
        {
            _cleared[i] = (float)(_cleared[i] * (1.0 - share));
        }
    }

    /// <summary>
    /// Everything is clear, because there is nothing lying. Called when the pack
    /// goes, so a road ploughed last February is not still credited for it next
    /// December.
    /// </summary>
    public void Thaw() => Array.Fill(_cleared, 1.0f);

    /// <summary>
    /// How buried the whole lattice is on average, 0 clear to 1.
    /// </summary>
    /// <remarks>
    /// <b>Not what a panel should show.</b> Nobody ploughs a field, so over a
    /// 6 km republic — three and a half thousand cells of empty countryside
    /// against nine kilometres of road — this sits near one all winter whatever
    /// the ploughs achieve. It exists because the tests that check burial and
    /// thaw are about the lattice rather than about a road network.
    /// </remarks>
    public double BuriedShare()
    {
        if (_cleared.Length == 0)
        {
            return 0.0;
        }

        var total = 0.0;
        foreach (var c in _cleared)
        {
            total += 1.0 - c;
        }

        return total / _cleared.Length;
    }

    /// <summary>Wear a cell in, capped at a made track.</summary>
    public void WearIn(int index, double by) =>
        _wear[index] = (float)Math.Clamp(_wear[index] + by, 0.0, 1.0);

    /// <summary>
    /// Let every cell recover a little. Without this, every route ever driven is
    /// permanent and the map ends up covered in the ghosts of routes nobody
    /// uses — a corridor has to be <i>kept</i>.
    /// </summary>
    public void Fade(double by)
    {
        var amount = Math.Max(0.0, by);
        for (var i = 0; i < _wear.Length; i++)
        {
            _wear[i] = (float)Math.Max(0.0, _wear[i] - amount);
        }
    }

    public double PollutionAt(int index) => _pollution[index];

    /// <summary>The dirt at a point, or clean off the map.</summary>
    public double PollutionNear(double x, double y)
    {
        var c = CellOf(x, y);
        return c < 0 ? 0.0 : PollutionAt(c);
    }

    /// <summary>
    /// Foul a cell. Saturates at one: past a point more smoke changes nothing,
    /// which is what stops one steel works making a whole valley uninhabitable
    /// and then making it uninhabitable again.
    /// </summary>
    public void Foul(int index, double by) =>
        _pollution[index] = (float)Math.Clamp(_pollution[index] + by, 0.0, 1.0);

    /// <summary>
    /// A day of weather carrying it away.
    /// </summary>
    /// <remarks>
    /// <para>
    /// <b>Proportional, not a flat subtraction</b>, and that took a measurement
    /// to get right. A flat rate makes the steady state a step function: a
    /// source emitting less than the rate settles at exactly clean and one
    /// emitting more settles at exactly filthy, with nothing in between — so a
    /// brickworks and a steel works would look identical. Decaying by a share
    /// gives every source its own level, which is the whole point of authoring
    /// different figures.
    /// </para>
    /// <para>
    /// The floor is what lets a valley come genuinely clean rather than
    /// approaching it for ever: an exponential never reaches zero, and a player
    /// who pulled a works down two months ago should not still be reading a
    /// trace of it.
    /// </para>
    /// </remarks>
    public void Disperse(double by)
    {
        var share = Math.Clamp(by, 0.0, 1.0);
        for (var i = 0; i < _pollution.Length; i++)
        {
            var left = _pollution[i] * (1.0 - share);
            _pollution[i] = left <= 1e-3 ? 0.0f : (float)left;
        }
    }

    /// <summary>
    /// Cells within a radius of a point, for spreading something around a
    /// source. Includes the cell the point is in.
    /// </summary>
    public List<int> CellsWithin(double x, double y, double radius)
    {
        var found = new List<int>();
        var centre = CellOf(x, y);
        if (centre < 0)
        {
            return found;
        }

        var reach = (int)Math.Ceiling(radius / CellSize);
        var cx = centre % Cells;
        var cy = centre / Cells;
        for (var dy = -reach; dy <= reach; dy++)
        {
            for (var dx = -reach; dx <= reach; dx++)
            {
                var gx = cx + dx;
                var gy = cy + dy;
                if (gx < 0 || gy < 0 || gx >= Cells || gy >= Cells)
                {
                    continue;
                }

                var index = (gy * Cells) + gx;
                if (Units.Distance(CentreX(index), CentreY(index), x, y) <= radius)
                {
                    found.Add(index);
                }
            }
        }

        return found;
    }
}
