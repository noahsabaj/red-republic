using System.Text.Json;

namespace RedRepublic.Sim;

/// <summary>
/// The authored balance table, loaded once and held as parallel arrays.
/// </summary>
/// <remarks>
/// <para>
/// <b>Balance lives in data, behaviour lives in systems.</b> <c>manifest.json</c>
/// is that data; nothing in this project may contain a list of building kinds,
/// because a list of ids inside logic is a thing you must remember to edit and
/// what you forget lands silently in a fallback.
/// </para>
/// <para>
/// <b>Parallel arrays rather than a class per building.</b> The per-tick
/// production pass walks every building every tick, and the top speed buys
/// 28,800 in-game seconds a real second — 480 ticks. Struct-of-arrays is what
/// keeps that pass linear over contiguous memory. It also serves the interface
/// rule directly: hand the UI a packed array and let it slice, never a
/// dictionary per entity.
/// </para>
/// <para>
/// Variable-length rows (inputs, outputs, materials, the rest) are held the way
/// a sparse matrix is: one flat array of values and an offset array saying where
/// each building's run begins.
/// </para>
/// <para>
/// The table is handed in as text rather than read from a path. This project has
/// no reference to Godot and therefore no notion of <c>res://</c>; the caller
/// that knows where files live is the one that opens them.
/// </para>
/// </remarks>
public sealed class Tables
{
    /// <summary>
    /// Enum orders the manifest names but does not enumerate, because they are
    /// the simulation's own vocabulary rather than balance figures. Index order
    /// is load-bearing: it reaches saves.
    /// </summary>
    public static readonly string[] Media = ["Road", "Rail", "Tram", "Metro", "Water", "Air"];

    public static readonly string[] Minerals = ["Coal", "IronOre", "Oil", "Gravel", "Groundwater"];

    public static readonly string[] Teaching = ["School", "University"];

    // ---- rosters, in the order the manifest fixes; these indices reach saves ----
    public string[] Resources { get; private set; } = [];
    public string[] ResourceNames { get; private set; } = [];
    public int[] ResourceForm { get; private set; } = [];
    public double[] ResourcePriceEast { get; private set; } = [];
    public double[] ResourcePriceWest { get; private set; } = [];
    public bool[] ResourceIsComfort { get; private set; } = [];

    /// <summary>-1 where the resource is not dug out of anything, else an index into <see cref="Minerals"/>.</summary>
    public int[] ResourceFromMineral { get; private set; } = [];

    public string[] Forms { get; private set; } = [];
    public string[] Needs { get; private set; } = [];
    public string[] Education { get; private set; } = [];
    public string[] Priorities { get; private set; } = [];

    // ---- buildings, struct-of-arrays ----
    public int BuildingCount => BuildingIds.Length;
    public string[] BuildingIds { get; private set; } = [];
    public string[] BName { get; private set; } = [];
    public double[] BWidth { get; private set; } = [];
    public double[] BDepth { get; private set; } = [];
    public int[] BWorkers { get; private set; } = [];
    public int[] BPriority { get; private set; } = [];
    public double[] BPowerDraw { get; private set; } = [];
    public double[] BPowerOutput { get; private set; } = [];
    public double[] BHeat { get; private set; } = [];
    public double[] BHeatOutput { get; private set; } = [];
    public int[] BSeats { get; private set; } = [];
    public double[] BLabour { get; private set; } = [];
    public int[] BResidents { get; private set; } = [];
    public double[] BStorage { get; private set; } = [];
    public int[] BBeds { get; private set; } = [];
    public double[] BWear { get; private set; } = [];
    public bool[] BFarms { get; private set; } = [];
    public bool[] BTransforms { get; private set; } = [];
    public double[] BWaste { get; private set; } = [];
    public double[] BPollution { get; private set; } = [];
    public bool[] BStoresToOrder { get; private set; } = [];
    public int[] BSchooling { get; private set; } = [];

    /// <summary>-1 where the building teaches nothing, else an index into <see cref="Teaching"/>.</summary>
    public int[] BTeaches { get; private set; } = [];

    /// <summary>-1 where the building taps nothing, else an index into <see cref="Minerals"/>.</summary>
    public int[] BTaps { get; private set; } = [];

    /// <summary>-1 where the building is not a terminal, else an index into <see cref="Media"/>.</summary>
    public int[] BMedium { get; private set; } = [];

    // ---- variable-length rows: value runs plus offsets ----
    public Run<int, double> Inputs { get; private set; } = Run<int, double>.Empty;
    public Run<int, double> Outputs { get; private set; } = Run<int, double>.Empty;
    public Run<int, double> Materials { get; private set; } = Run<int, double>.Empty;
    public Run<int, double> Serves { get; private set; } = Run<int, double>.Empty;
    public Run<int, int> Establishment { get; private set; } = Run<int, int>.Empty;
    public Run<int, byte> Sells { get; private set; } = Run<int, byte>.Empty;
    public Run<int, byte> Admits { get; private set; } = Run<int, byte>.Empty;

    // ---- worldgen plans ----
    public double TerrainCellSize { get; private set; }
    public double TerrainFeatureSize { get; private set; }
    public int TerrainOctaves { get; private set; }
    public double TerrainRelief { get; private set; }
    public double TerrainWaterBelow { get; private set; }
    public double TerrainForestAbove { get; private set; }
    public double TerrainRockAbove { get; private set; }

    /// <summary>
    /// How much ground must drain through a cell before it is a river, as a
    /// fraction of the map's area. Swept rather than picked, over five
    /// thresholds and three seeds, against how far the longest connected channel
    /// runs and how many of sixteen bearings a 2 km road can take without
    /// meeting water.
    /// </summary>
    public double TerrainRiverCatchment { get; private set; }

    public double TerrainBroadCatchment { get; private set; }

    /// <summary>
    /// The stream index geology draws on, kept apart from terrain's so changing
    /// one does not re-roll the other.
    /// </summary>
    public ulong GeologyStream { get; private set; }

    public MineralPlan[] MineralPlan { get; private set; } = [];

    // ---- the network ----

    /// <summary>How finely a fairway is sampled off the water.</summary>
    public double FairwaySpacing { get; private set; }

    /// <summary>
    /// How wide the water has to be before a barge can turn in it. A stream a
    /// metre across is water on the map and is not a waterway.
    /// </summary>
    public double NavigableBeam { get; private set; }

    public double NavigableKph { get; private set; }
    public double AirwayKph { get; private set; }
    public double DefaultRoadKph { get; private set; }

    // ---- people, and the journeys they make ----

    /// <summary>How far somebody will walk to work before they need carrying.</summary>
    public double MaxWalkM { get; private set; }

    /// <summary>How far from a road a building can be and still be reached.</summary>
    public double RoadAccessM { get; private set; }

    /// <summary>An unhurried adult pace.</summary>
    public double WalkKph { get; private set; }

    public int WorkingAgeFrom { get; private set; }
    public int WorkingAgeTo { get; private set; }
    public int SchoolAgeFrom { get; private set; }
    public int SchoolAgeTo { get; private set; }
    public int UniversityAgeFrom { get; private set; }
    public int UniversityAgeTo { get; private set; }

    /// <summary>Days of attendance that make somebody schooled.</summary>
    public int SchoolDays { get; private set; }

    /// <summary>Days at university, on top of school, that make somebody a graduate.</summary>
    public int UniversityDays { get; private set; }

    /// <summary>The longest journey somebody will make to a job.</summary>
    public double MaxCommuteS { get; private set; }

    /// <summary>How far somebody will walk to reach a stop.</summary>
    public double StopWalkM { get; private set; }

    /// <summary>How far somebody will walk in the dark, which is less.</summary>
    public double NightWalkM { get; private set; }

    public double FuelPerSeatDay { get; private set; }

    /// <summary>How close a terminal must be to the network it serves.</summary>
    public double TerminalReachM { get; private set; }

    public double MinLegTicks { get; private set; }
    public double ShuntingKph { get; private set; }

    private double[] _commercialKph = [];

    /// <summary>How fast something moves on a medium, in km/h.</summary>
    public double CommercialKph(int medium) => _commercialKph[medium];

    // ---- the working day ----

    /// <summary>
    /// One shift. <b>What every authored rate in the building table means</b>,
    /// so this is balance in the strongest sense: change it and every output
    /// figure in the game means something else.
    /// </summary>
    public double StandardHours { get; private set; }

    public double MinHours { get; private set; }
    public double MaxHours { get; private set; }

    /// <summary>
    /// Above this, somebody working here travels in the dark. A threshold rather
    /// than a daylight model: no working period longer than this fits inside
    /// daylight in any climate the republic is posted to.
    /// </summary>
    public double DaylightHours { get; private set; }

    public int MaxShifts { get; private set; }
    public double OverworkHealth { get; private set; }
    public double OverworkLoyalty { get; private set; }

    // ---- the ground model ----

    public double FreezeC { get; private set; }

    /// <summary>How far below freezing the ground reaches full frost.</summary>
    public double FrostRangeC { get; private set; }

    /// <summary>
    /// How fast frost follows the air. Soil has thermal mass, and this lag is
    /// what makes the thaw an event rather than a switch.
    /// </summary>
    public double FrostLag { get; private set; }

    public double SaturationMm { get; private set; }
    public double MeltPerDegreeMm { get; private set; }
    public double DryingPerDay { get; private set; }
    public double DryingFullAtC { get; private set; }
    public double RootSaturationMm { get; private set; }
    public double RootDryingPerDay { get; private set; }

    /// <summary>How much longer fully soft ground takes to cross than firm.</summary>
    public double MudDrag { get; private set; }

    /// <summary>How much of a cell has to be water before nothing can cross it.</summary>
    public double Drowned { get; private set; }

    /// <summary>
    /// How much one laden pass packs a cell down. Fifty passes to turn open
    /// field into a made track, less the fading — deliberately a season's worth
    /// of traffic rather than a week's, so a track the republic did not plan
    /// arrives slowly enough to be noticed happening.
    /// </summary>
    public double WearPerPass { get; private set; }

    public double WearFadePerDay { get; private set; }

    /// <summary>Where a worn corridor becomes a track on the map.</summary>
    public double PromoteAt { get; private set; }

    public double SnowBlocksMm { get; private set; }

    /// <summary>
    /// How much longer a fully buried road takes to drive than a swept one.
    /// Deliberately gentler than <see cref="MudDrag"/>: a road under snow is
    /// still a road. What it costs a republic is hours, not journeys.
    /// </summary>
    public double SnowDrag { get; private set; }

    /// <summary>How much of the going a made track takes away.</summary>
    public double WearRelief { get; private set; }

    public double GroundCellSize { get; private set; }

    /// <summary>
    /// The shortest run of worn cells worth calling a road. Below that it is a
    /// gateway, not a route.
    /// </summary>
    public int MinTrackCells { get; private set; }

    private double[] _going = [];

    /// <summary>
    /// The static going multiplier of a surface — infinite for water, which is
    /// impassable rather than merely slow.
    /// </summary>
    public double Going(Surface s) => _going[(int)s];

    // ---- climate ----

    /// <summary>Below this, buildings need heating.</summary>
    public double HeatThresholdC { get; private set; }

    /// <summary>The design-cold day a building's nominal heat demand is quoted at.</summary>
    public double HeatDesignC { get; private set; }

    /// <summary>How far past nominal deep cold can drive demand.</summary>
    public double HeatDemandCeiling { get; private set; }

    /// <summary>
    /// Share of days that carry any rain at all. Rain is bursty because a
    /// month's water smeared over thirty days never saturates anything.
    /// </summary>
    public double WetDayShare { get; private set; }

    public Climate[] Climates { get; private set; } = [];

    // ---- vehicles ----
    public int VehicleCount => VehicleIds.Length;
    public string[] VehicleIds { get; private set; } = [];
    public string[] VName { get; private set; } = [];
    public string[] VRole { get; private set; } = [];
    public int[] VMedium { get; private set; } = [];
    public double[] VCapacity { get; private set; } = [];
    public int[] VSeats { get; private set; } = [];
    public double[] VOnRoadKph { get; private set; } = [];
    public double[] VCrossCountryKph { get; private set; } = [];
    public double[] VFuelPerKm { get; private set; } = [];
    public double[] VTank { get; private set; } = [];
    public double[] VGround { get; private set; } = [];
    public double[] VLoadPenalty { get; private set; } = [];

    /// <summary>What the manifest says the table hashes to.</summary>
    public string ChecksumExpected { get; private set; } = "";

    /// <summary>What the loaded table actually hashes to.</summary>
    public string ChecksumGot { get; private set; } = "";

    /// <summary>
    /// Load the table and verify that it crossed intact.
    /// </summary>
    /// <remarks>
    /// The determinism rule says a double must round-trip bit-exactly; a balance
    /// table read at startup is held to the same bar, because a figure that
    /// arrives one ulp out is a republic that diverges from the one its seed
    /// promised and nothing downstream would ever say so. So every number is
    /// rehashed here, by its bits, in the order the dumper fixed.
    /// </remarks>
    /// <exception cref="InvalidDataException">If the table did not survive the crossing.</exception>
    public static Tables Load(string json)
    {
        var t = new Tables();
        using var doc = JsonDocument.Parse(json);
        var m = doc.RootElement;

        t.Resources = Strings(m, "resources");
        t.Forms = Strings(m, "forms");
        t.Needs = Strings(m, "needs");
        t.Education = Strings(m, "education");
        t.Priorities = Strings(m, "priorities");

        var facts = m.GetProperty("resource_facts");
        var n = t.Resources.Length;
        t.ResourceNames = new string[n];
        t.ResourceForm = new int[n];
        t.ResourcePriceEast = new double[n];
        t.ResourcePriceWest = new double[n];
        t.ResourceIsComfort = new bool[n];
        t.ResourceFromMineral = new int[n];
        for (var r = 0; r < n; r++)
        {
            var f = facts.GetProperty(t.Resources[r]);
            t.ResourceNames[r] = f.GetProperty("name").GetString()!;
            t.ResourceForm[r] = IndexIn(t.Forms, f.GetProperty("form").GetString()!);
            t.ResourcePriceEast[r] = f.GetProperty("price_east").GetDouble();
            t.ResourcePriceWest[r] = f.GetProperty("price_west").GetDouble();
            t.ResourceIsComfort[r] = f.GetProperty("is_comfort").GetBoolean();
            t.ResourceFromMineral[r] = OptionalIndex(Minerals, f.GetProperty("from_mineral"));
        }

        // Vehicles first: a building's establishment names vehicle kinds, and
        // the roster has to exist before those names can become indices.
        t.LoadVehicles(m.GetProperty("vehicles"));
        t.LoadBuildings(m);
        t.LoadPlans(m);
        t.LoadClimates(m);
        t.LoadGround(m);
        t.LoadShifts(m);
        t.LoadPeople(m);

        t.ChecksumExpected = m.GetProperty("checksum").GetString()!;
        t.ChecksumGot = t.Checksum();
        if (t.ChecksumGot != t.ChecksumExpected)
        {
            throw new InvalidDataException(
                $"the balance table did not survive the crossing: computed {t.ChecksumGot}, "
                + $"manifest says {t.ChecksumExpected}");
        }

        return t;
    }

    public int BuildingIndex(string id) => IndexIn(BuildingIds, id);

    public int ResourceIndex(string id) => IndexIn(Resources, id);

    private void LoadVehicles(JsonElement vs)
    {
        var ids = new List<string>();
        foreach (var p in vs.EnumerateObject())
        {
            ids.Add(p.Name);
        }

        VehicleIds = [.. ids];
        var n = VehicleIds.Length;
        VName = new string[n];
        VRole = new string[n];
        VMedium = new int[n];
        VCapacity = new double[n];
        VSeats = new int[n];
        VOnRoadKph = new double[n];
        VCrossCountryKph = new double[n];
        VFuelPerKm = new double[n];
        VTank = new double[n];
        VGround = new double[n];
        VLoadPenalty = new double[n];

        for (var v = 0; v < n; v++)
        {
            var d = vs.GetProperty(VehicleIds[v]);
            VName[v] = d.GetProperty("name").GetString()!;
            VRole[v] = d.GetProperty("role").GetString()!;
            VMedium[v] = IndexIn(Media, d.GetProperty("medium").GetString()!);
            VCapacity[v] = d.GetProperty("capacity_t").GetDouble();
            VSeats[v] = d.GetProperty("seats").GetInt32();
            VOnRoadKph[v] = d.GetProperty("on_road_kph").GetDouble();
            VCrossCountryKph[v] = d.GetProperty("cross_country_kph").GetDouble();
            VFuelPerKm[v] = d.GetProperty("fuel_per_km").GetDouble();
            VTank[v] = d.GetProperty("tank_t").GetDouble();
            VGround[v] = d.GetProperty("ground").GetDouble();
            VLoadPenalty[v] = d.GetProperty("load_penalty").GetDouble();
        }
    }

    private void LoadBuildings(JsonElement m)
    {
        BuildingIds = Strings(m, "building_order");
        var bs = m.GetProperty("buildings");
        var n = BuildingIds.Length;

        BName = new string[n];
        BWidth = new double[n];
        BDepth = new double[n];
        BWorkers = new int[n];
        BPriority = new int[n];
        BPowerDraw = new double[n];
        BPowerOutput = new double[n];
        BHeat = new double[n];
        BHeatOutput = new double[n];
        BSeats = new int[n];
        BLabour = new double[n];
        BResidents = new int[n];
        BStorage = new double[n];
        BBeds = new int[n];
        BWear = new double[n];
        BFarms = new bool[n];
        BTransforms = new bool[n];
        BWaste = new double[n];
        BPollution = new double[n];
        BStoresToOrder = new bool[n];
        BSchooling = new int[n];
        BTeaches = new int[n];
        BTaps = new int[n];
        BMedium = new int[n];

        var inputs = new RunBuilder<int, double>(n);
        var outputs = new RunBuilder<int, double>(n);
        var materials = new RunBuilder<int, double>(n);
        var serves = new RunBuilder<int, double>(n);
        var establishment = new RunBuilder<int, int>(n);
        var sells = new RunBuilder<int, byte>(n);
        var admits = new RunBuilder<int, byte>(n);

        for (var b = 0; b < n; b++)
        {
            var d = bs.GetProperty(BuildingIds[b]);
            BName[b] = d.GetProperty("name").GetString()!;
            BWidth[b] = d.GetProperty("width").GetDouble();
            BDepth[b] = d.GetProperty("depth").GetDouble();
            BWorkers[b] = d.GetProperty("workers").GetInt32();
            BPriority[b] = IndexIn(Priorities, d.GetProperty("priority").GetString()!);
            BPowerDraw[b] = d.GetProperty("power_draw").GetDouble();
            BPowerOutput[b] = d.GetProperty("power_output").GetDouble();
            BHeat[b] = d.GetProperty("heat").GetDouble();
            BHeatOutput[b] = d.GetProperty("heat_output").GetDouble();
            BSeats[b] = d.GetProperty("seats").GetInt32();
            BLabour[b] = d.GetProperty("labour").GetDouble();
            BResidents[b] = d.GetProperty("residents").GetInt32();
            BStorage[b] = d.GetProperty("storage").GetDouble();
            BBeds[b] = d.GetProperty("beds").GetInt32();
            BWear[b] = d.GetProperty("wear").GetDouble();
            BFarms[b] = d.GetProperty("farms").GetBoolean();
            BTransforms[b] = d.GetProperty("transforms").GetBoolean();
            BWaste[b] = d.GetProperty("waste").GetDouble();
            BPollution[b] = d.GetProperty("pollution").GetDouble();
            BStoresToOrder[b] = d.GetProperty("stores_to_order").GetBoolean();
            BSchooling[b] = IndexIn(Education, d.GetProperty("schooling").GetString()!);
            BTeaches[b] = OptionalIndex(Teaching, d.GetProperty("teaches"));
            BTaps[b] = OptionalIndex(Minerals, d.GetProperty("taps"));
            BMedium[b] = OptionalIndex(Media, d.GetProperty("medium"));

            foreach (var pair in d.GetProperty("inputs").EnumerateArray())
            {
                inputs.Add(ResourceIndex(pair[0].GetString()!), pair[1].GetDouble());
            }

            foreach (var pair in d.GetProperty("outputs").EnumerateArray())
            {
                outputs.Add(ResourceIndex(pair[0].GetString()!), pair[1].GetDouble());
            }

            foreach (var pair in d.GetProperty("materials").EnumerateArray())
            {
                materials.Add(ResourceIndex(pair[0].GetString()!), pair[1].GetDouble());
            }

            foreach (var pair in d.GetProperty("serves").EnumerateArray())
            {
                serves.Add(IndexIn(Needs, pair[0].GetString()!), pair[1].GetDouble());
            }

            foreach (var pair in d.GetProperty("vehicles").EnumerateArray())
            {
                establishment.Add(IndexIn(VehicleIds, pair[0].GetString()!), pair[1].GetInt32());
            }

            foreach (var r in d.GetProperty("sells").EnumerateArray())
            {
                sells.Add(ResourceIndex(r.GetString()!), 0);
            }

            foreach (var f in d.GetProperty("admits").EnumerateArray())
            {
                admits.Add(IndexIn(Forms, f.GetString()!), 0);
            }

            inputs.Close();
            outputs.Close();
            materials.Close();
            serves.Close();
            establishment.Close();
            sells.Close();
            admits.Close();
        }

        Inputs = inputs.Build();
        Outputs = outputs.Build();
        Materials = materials.Build();
        Serves = serves.Build();
        Establishment = establishment.Build();
        Sells = sells.Build();
        Admits = admits.Build();
    }

    private void LoadPlans(JsonElement m)
    {
        var tp = m.GetProperty("terrain_plan");
        TerrainCellSize = tp.GetProperty("cell_size").GetDouble();
        TerrainFeatureSize = tp.GetProperty("feature_size").GetDouble();
        TerrainOctaves = tp.GetProperty("octaves").GetInt32();
        TerrainRelief = tp.GetProperty("relief").GetDouble();
        TerrainWaterBelow = tp.GetProperty("water_below").GetDouble();
        TerrainForestAbove = tp.GetProperty("forest_above").GetDouble();
        TerrainRockAbove = tp.GetProperty("rock_above").GetDouble();
        TerrainRiverCatchment = tp.GetProperty("river_catchment").GetDouble();
        TerrainBroadCatchment = tp.GetProperty("broad_catchment").GetDouble();

        GeologyStream = m.GetProperty("geology_stream").GetUInt64();

        var plans = new List<MineralPlan>();
        foreach (var row in m.GetProperty("mineral_plan").EnumerateArray())
        {
            plans.Add(new MineralPlan(
                IndexIn(Minerals, row.GetProperty("mineral").GetString()!),
                row.GetProperty("bodies").GetInt32(),
                row.GetProperty("radius")[0].GetDouble(),
                row.GetProperty("radius")[1].GetDouble(),
                row.GetProperty("top")[0].GetDouble(),
                row.GetProperty("top")[1].GetDouble(),
                row.GetProperty("layers").GetInt32(),
                row.GetProperty("layer_thickness")[0].GetDouble(),
                row.GetProperty("layer_thickness")[1].GetDouble(),
                row.GetProperty("tonnes_per_layer")[0].GetDouble(),
                row.GetProperty("tonnes_per_layer")[1].GetDouble()));
        }

        MineralPlan = [.. plans];
    }

    private void LoadPeople(JsonElement m)
    {
        var net = m.GetProperty("network");
        FairwaySpacing = net.GetProperty("fairway_spacing_m").GetDouble();
        NavigableBeam = net.GetProperty("navigable_beam_m").GetDouble();
        NavigableKph = net.GetProperty("navigable_kph").GetDouble();
        AirwayKph = net.GetProperty("airway_kph").GetDouble();
        DefaultRoadKph = net.GetProperty("default_road_kph").GetDouble();

        var p = m.GetProperty("people");
        MaxWalkM = p.GetProperty("max_walk_m").GetDouble();
        RoadAccessM = p.GetProperty("road_access_m").GetDouble();
        WalkKph = p.GetProperty("walk_kph").GetDouble();
        WorkingAgeFrom = p.GetProperty("working_age")[0].GetInt32();
        WorkingAgeTo = p.GetProperty("working_age")[1].GetInt32();
        SchoolAgeFrom = p.GetProperty("school_age")[0].GetInt32();
        SchoolAgeTo = p.GetProperty("school_age")[1].GetInt32();
        UniversityAgeFrom = p.GetProperty("university_age")[0].GetInt32();
        UniversityAgeTo = p.GetProperty("university_age")[1].GetInt32();
        SchoolDays = p.GetProperty("school_days").GetInt32();
        UniversityDays = p.GetProperty("university_days").GetInt32();
        MaxCommuteS = p.GetProperty("max_commute_s").GetDouble();
        StopWalkM = p.GetProperty("stop_walk_m").GetDouble();
        NightWalkM = p.GetProperty("night_walk_m").GetDouble();
        FuelPerSeatDay = p.GetProperty("fuel_per_seat_day").GetDouble();

        var j = m.GetProperty("journey");
        TerminalReachM = j.GetProperty("terminal_reach_m").GetDouble();
        MinLegTicks = j.GetProperty("min_leg_ticks").GetDouble();
        ShuntingKph = j.GetProperty("shunting_kph").GetDouble();
        _commercialKph = Doubles(j, "commercial_kph");
    }

    private void LoadShifts(JsonElement m)
    {
        var s = m.GetProperty("shifts");
        StandardHours = s.GetProperty("standard_hours").GetDouble();
        MinHours = s.GetProperty("min_hours").GetDouble();
        MaxHours = s.GetProperty("max_hours").GetDouble();
        DaylightHours = s.GetProperty("daylight_hours").GetDouble();
        MaxShifts = s.GetProperty("max_shifts").GetInt32();
        OverworkHealth = s.GetProperty("overwork_health").GetDouble();
        OverworkLoyalty = s.GetProperty("overwork_loyalty").GetDouble();
    }

    private void LoadGround(JsonElement m)
    {
        var g = m.GetProperty("ground");
        FreezeC = g.GetProperty("freeze_c").GetDouble();
        FrostRangeC = g.GetProperty("frost_range_c").GetDouble();
        FrostLag = g.GetProperty("frost_lag").GetDouble();
        SaturationMm = g.GetProperty("saturation_mm").GetDouble();
        MeltPerDegreeMm = g.GetProperty("melt_per_degree_mm").GetDouble();
        DryingPerDay = g.GetProperty("drying_per_day").GetDouble();
        DryingFullAtC = g.GetProperty("drying_full_at_c").GetDouble();
        RootSaturationMm = g.GetProperty("root_saturation_mm").GetDouble();
        RootDryingPerDay = g.GetProperty("root_drying_per_day").GetDouble();
        MudDrag = g.GetProperty("mud_drag").GetDouble();
        Drowned = g.GetProperty("drowned").GetDouble();
        WearPerPass = g.GetProperty("wear_per_pass").GetDouble();
        WearFadePerDay = g.GetProperty("wear_fade_per_day").GetDouble();
        PromoteAt = g.GetProperty("promote_at").GetDouble();
        SnowBlocksMm = g.GetProperty("snow_blocks_mm").GetDouble();
        SnowDrag = g.GetProperty("snow_drag").GetDouble();
        WearRelief = g.GetProperty("wear_relief").GetDouble();
        GroundCellSize = g.GetProperty("cell_size").GetDouble();
        MinTrackCells = g.GetProperty("min_track_cells").GetInt32();

        // `null` in the table means impassable. JSON has no infinity, and
        // writing a large finite number instead would make water something a
        // desperate router could still cross.
        var going = new List<double>();
        foreach (var v in g.GetProperty("going").EnumerateArray())
        {
            going.Add(v.ValueKind == JsonValueKind.Null ? double.PositiveInfinity : v.GetDouble());
        }

        _going = [.. going];
    }

    private void LoadClimates(JsonElement m)
    {
        var heat = m.GetProperty("heat");
        HeatThresholdC = heat.GetProperty("threshold_c").GetDouble();
        HeatDesignC = heat.GetProperty("design_c").GetDouble();
        HeatDemandCeiling = heat.GetProperty("demand_ceiling").GetDouble();
        WetDayShare = heat.GetProperty("wet_day_share").GetDouble();

        var list = new List<Climate>();
        foreach (var c in m.GetProperty("climates").EnumerateArray())
        {
            list.Add(new Climate(
                c.GetProperty("id").GetString()!,
                c.GetProperty("name").GetString()!,
                Doubles(c, "monthly_mean_c"),
                c.GetProperty("daily_swing_c").GetDouble(),
                Doubles(c, "monthly_rain_mm")));
        }

        Climates = [.. list];
    }

    private static double[] Doubles(JsonElement e, string name)
    {
        var list = new List<double>();
        foreach (var v in e.GetProperty(name).EnumerateArray())
        {
            list.Add(v.GetDouble());
        }

        return [.. list];
    }

    /// <summary>
    /// FNV-1a over the bits of every number in the table, in the order the
    /// dumper fixed. Any drift — a value parsed one ulp out, a row reordered, a
    /// field dropped — changes this.
    /// </summary>
    private string Checksum()
    {
        var h = new Fnv1a();
        for (var r = 0; r < Resources.Length; r++)
        {
            h.Push(ResourcePriceEast[r]);
            h.Push(ResourcePriceWest[r]);
            h.Push(ResourceIsComfort[r]);
        }

        for (var b = 0; b < BuildingCount; b++)
        {
            h.Push(BWidth[b]);
            h.Push(BDepth[b]);
            h.Push(BWorkers[b]);
            h.Push(BPowerDraw[b]);
            h.Push(BPowerOutput[b]);
            h.Push(BHeat[b]);
            h.Push(BHeatOutput[b]);
            h.Push(BSeats[b]);
            foreach (var c in Establishment.ValuesOf(b))
            {
                h.Push(c);
            }

            foreach (var v in Inputs.ValuesOf(b))
            {
                h.Push(v);
            }

            foreach (var v in Outputs.ValuesOf(b))
            {
                h.Push(v);
            }

            foreach (var v in Materials.ValuesOf(b))
            {
                h.Push(v);
            }

            h.Push(BLabour[b]);
            h.Push(BResidents[b]);
            h.Push(BStorage[b]);
            h.Push(BBeds[b]);
            h.Push(BWear[b]);
            h.Push(BFarms[b]);
            h.Push(BTransforms[b]);
            h.Push(BWaste[b]);
            h.Push(BPollution[b]);
            foreach (var v in Serves.ValuesOf(b))
            {
                h.Push(v);
            }

            h.Push(BStoresToOrder[b]);
        }

        h.Push(StandardHours);
        h.Push(MinHours);
        h.Push(MaxHours);
        h.Push(DaylightHours);
        h.Push(MaxShifts);
        h.Push(OverworkHealth);
        h.Push(OverworkLoyalty);
        h.Push(FreezeC);
        h.Push(FrostRangeC);
        h.Push(FrostLag);
        h.Push(SaturationMm);
        h.Push(MeltPerDegreeMm);
        h.Push(DryingPerDay);
        h.Push(DryingFullAtC);
        h.Push(RootSaturationMm);
        h.Push(RootDryingPerDay);
        h.Push(MudDrag);
        h.Push(Drowned);
        h.Push(WearPerPass);
        h.Push(WearFadePerDay);
        h.Push(PromoteAt);
        h.Push(SnowBlocksMm);
        h.Push(SnowDrag);
        h.Push(WearRelief);
        h.Push(GroundCellSize);
        h.Push(MinTrackCells);
        foreach (var v in _going)
        {
            h.Push(v);
        }

        h.Push(HeatThresholdC);
        h.Push(HeatDesignC);
        h.Push(HeatDemandCeiling);
        h.Push(WetDayShare);
        foreach (var c in Climates)
        {
            foreach (var v in c.MonthlyMeanC)
            {
                h.Push(v);
            }

            h.Push(c.DailySwingC);
            foreach (var v in c.MonthlyRainMm)
            {
                h.Push(v);
            }
        }

        h.Push(TerrainCellSize);
        h.Push(TerrainFeatureSize);
        h.Push(TerrainOctaves);
        h.Push(TerrainRelief);
        h.Push(TerrainWaterBelow);
        h.Push(TerrainForestAbove);
        h.Push(TerrainRockAbove);
        h.Push(TerrainRiverCatchment);
        h.Push(TerrainBroadCatchment);
        foreach (var p in MineralPlan)
        {
            h.Push(p.Bodies);
            h.Push(p.RadiusLo);
            h.Push(p.RadiusHi);
            h.Push(p.TopLo);
            h.Push(p.TopHi);
            h.Push(p.Layers);
            h.Push(p.ThicknessLo);
            h.Push(p.ThicknessHi);
            h.Push(p.TonnesLo);
            h.Push(p.TonnesHi);
        }

        for (var v = 0; v < VehicleCount; v++)
        {
            h.Push(VCapacity[v]);
            h.Push(VSeats[v]);
            h.Push(VOnRoadKph[v]);
            h.Push(VCrossCountryKph[v]);
            h.Push(VFuelPerKm[v]);
            h.Push(VTank[v]);
            h.Push(VGround[v]);
            h.Push(VLoadPenalty[v]);
        }

        // People last, matching the order the dumper pushes them in.
        h.Push(FairwaySpacing);
        h.Push(NavigableBeam);
        h.Push(NavigableKph);
        h.Push(AirwayKph);
        h.Push(DefaultRoadKph);
        h.Push(MaxWalkM);
        h.Push(RoadAccessM);
        h.Push(WalkKph);
        h.Push(WorkingAgeFrom);
        h.Push(WorkingAgeTo);
        h.Push(SchoolAgeFrom);
        h.Push(SchoolAgeTo);
        h.Push(UniversityAgeFrom);
        h.Push(UniversityAgeTo);
        h.Push(SchoolDays);
        h.Push(UniversityDays);
        h.Push(MaxCommuteS);
        h.Push(StopWalkM);
        h.Push(NightWalkM);
        h.Push(FuelPerSeatDay);
        h.Push(TerminalReachM);
        h.Push(MinLegTicks);
        h.Push(ShuntingKph);
        foreach (var v in _commercialKph)
        {
            h.Push(v);
        }

        return h.Hex;
    }

    private static string[] Strings(JsonElement m, string name)
    {
        var list = new List<string>();
        foreach (var e in m.GetProperty(name).EnumerateArray())
        {
            list.Add(e.GetString()!);
        }

        return [.. list];
    }

    /// <summary>
    /// The index of <paramref name="value"/>, or a throw.
    /// </summary>
    /// <remarks>
    /// Deliberately not returning -1 on a miss. An unresolved index would sit in
    /// the arrays and read as "the last one" wherever it was used, or crash
    /// somewhere else much later with nothing pointing back here. The manifest
    /// is generated, so an unrecognised name means the dumper and the loader
    /// have drifted, and that should stop the build.
    /// </remarks>
    private static int IndexIn(string[] roster, string value)
    {
        var i = Array.IndexOf(roster, value);
        return i >= 0
            ? i
            : throw new InvalidDataException(
                $"the manifest names `{value}`, which is not in [{string.Join(", ", roster)}]");
    }

    private static int OptionalIndex(string[] roster, JsonElement e) =>
        e.ValueKind == JsonValueKind.Null ? -1 : IndexIn(roster, e.GetString()!);
}

/// <summary>How to scatter one mineral through the ground. Ranges are inclusive-low, exclusive-high.</summary>
public readonly record struct MineralPlan(
    int Mineral,
    int Bodies,
    double RadiusLo,
    double RadiusHi,
    double TopLo,
    double TopHi,
    int Layers,
    double ThicknessLo,
    double ThicknessHi,
    double TonnesLo,
    double TonnesHi);
