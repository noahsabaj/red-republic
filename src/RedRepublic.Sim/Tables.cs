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

    // ---- the systems' own figures ----

    /// <summary>How many hands one Construction Office sends to a site.</summary>
    public int BuildersPerSite { get; private set; }

    /// <summary>Machinery worn out per builder-day — the industrialisation tax.</summary>
    public double MachineryPerBuilderDay { get; private set; }

    /// <summary>Builder-days a contracted firm works in a day. A gang of your own is ten people and works ten; a firm brings enough of its own that it works rather faster, because you are buying capacity you do not have to house, feed or train.</summary>
    public double ContractorDays { get; private set; }

    /// <summary>What one contracted builder-day costs, in the bloc's own currency. Several times what your own crews cost, which is the entire argument for building a Construction Office and training people.</summary>
    public double ContractorRate { get; private set; }

    /// <summary>Tonnes a person eats in a day.</summary>
    public double FoodPerCitizen { get; private set; }

    public double ClothesPerCitizen { get; private set; }

    public double AlcoholPerCitizen { get; private set; }

    public double ElectronicsPerCitizen { get; private set; }

    /// <summary>How far a clinic, a school or a shop reaches.</summary>
    public double ServiceRadius { get; private set; }

    /// <summary>How far a transformer station feeds.</summary>
    public double TransformerRange { get; private set; }

    /// <summary>How far a chimney's smoke carries.</summary>
    public double SmokeRadius { get; private set; }

    /// <summary>What a building runs at with a dry machinery bin. It runs worn rather than stopping — a dry bin never stalls the place.</summary>
    public double WornEfficiency { get; private set; }

    public double GrowingMinC { get; private set; }

    public double GrowingWarmC { get; private set; }

    public double DroughtBelow { get; private set; }

    public double DroughtFloor { get; private set; }

    public double WateredAt { get; private set; }

    public double WateredYield { get; private set; }

    public double ResupplyAtDays { get; private set; }

    /// <summary>The smallest load worth sending a lorry for.</summary>
    public double MinLoad { get; private set; }

    public double BogSpan { get; private set; }

    /// <summary>The chance of bogging on the worst going there is.</summary>
    public double WorstOdds { get; private set; }

    public double DigOut { get; private set; }

    public double RefuelRange { get; private set; }

    /// <summary>The age nobody outlives.</summary>
    public int Oldest { get; private set; }

    public double BirthsPerPairYear { get; private set; }

    /// <summary>How content a household has to be before it has a child.</summary>
    public double BirthsNeed { get; private set; }

    public double ArrivalOdds { get; private set; }

    /// <summary>How buried a road has to be before a plough is sent.</summary>
    public double PloughAt { get; private set; }

    // ---- ways and lines ----

    /// <summary>How often a long road drops a junction.</summary>
    public double JunctionSpacing { get; private set; }

    /// <summary>How close two junctions have to be to become one.</summary>
    public double JunctionMerge { get; private set; }

    /// <summary>The shortest stretch worth ordering.</summary>
    public double MinRoad { get; private set; }

    public double LampLabour { get; private set; }

    /// <summary>What a kilometre of lit road draws off the grid.</summary>
    public double LampMwPerKm { get; private set; }

    public Bill[] LampMaterials { get; private set; } = [];

    public GradeDef[] Grades { get; private set; } = [];

    public double UtilityJoin { get; private set; }
    public double MinLine { get; private set; }
    public UtilityDef[] Utilities { get; private set; } = [];

    // ---- how people are, and how they feel ----

    /// <summary>The most that fully-stocked comforts add to a home's score.</summary>
    public double ComfortLift { get; private set; }

    /// <summary>How fast loyalty follows the contentment of a home.</summary>
    /// <remarks>
    /// Slowly, so one bad winter does not empty a town and one good month does
    /// not fill it.
    /// </remarks>
    public double LoyaltyDrift { get; private set; }

    /// <summary>Below this, somebody starts thinking about leaving.</summary>
    public double LoyaltyLeaves { get; private set; }

    public double EmigrationOdds { get; private set; }
    public double HealthDrift { get; private set; }

    /// <summary>What drink costs in health — the price the one comfort carries.</summary>
    public double AlcoholHealthCost { get; private set; }

    /// <summary>Where health settles for somebody with no doctor in reach.</summary>
    public double HealthUnserved { get; private set; }

    /// <summary>How content a republic has to look before outsiders want in.</summary>
    public double ContentAttracts { get; private set; }

    public int ArrivalParty { get; private set; }

    /// <summary>How long a group waiting at a post will wait before giving up.</summary>
    public long PatienceDays { get; private set; }

    public double HiringFee { get; private set; }
    public double ForeignWage { get; private set; }
    public long StayDays { get; private set; }
    public int TourParty { get; private set; }
    public double SpendPerHeadPerDay { get; private set; }

    /// <summary>The least appealing a republic can be and still draw anyone.</summary>
    public double AppealFloor { get; private set; }

    // ---- trade, credit and tenders ----

    /// <summary>
    /// How many crossings a frontier gets. Enough that "which one" is a real
    /// choice and few enough that one is always a haul away.
    /// </summary>
    public int Crossings { get; private set; }

    /// <summary>How far inside the frontier a post stands.</summary>
    public double CrossingInset { get; private set; }

    public double CustomsRange { get; private set; }

    /// <summary>Tonnes a customs house can clear in a day.</summary>
    public double CustomsThroughputPerDay { get; private set; }

    public double BorderSpread { get; private set; }

    private double[] _contentmentWeights = [];

    /// <summary>
    /// How much each want counts, in the order <see cref="Contentment.Names"/>
    /// lists them. Carried so the guard can check the two agree.
    /// </summary>
    public IReadOnlyList<double> ContentmentWeights => _contentmentWeights;

    private Tier[] _ladderEast = [];
    private Tier[] _ladderWest = [];

    /// <summary>
    /// A bloc's lending ladder. The two are different instruments rather than
    /// one converted: the east lends roubles by the hundred thousand over years,
    /// the west dollars by the thousand over months, dearer.
    /// </summary>
    public IReadOnlyList<Tier> Ladder(Market market) =>
        market == Market.East ? _ladderEast : _ladderWest;

    /// <summary>Share of what was outstanding that a default costs.</summary>
    public double DefaultFine { get; private set; }

    public double DefaultRelations { get; private set; }

    // Tenders.
    public long OfferEveryMonths { get; private set; }
    public int MaxOpenOffers { get; private set; }
    public double ValueBandEastLo { get; private set; }
    public double ValueBandEastHi { get; private set; }
    public double ValueBandWestLo { get; private set; }
    public double ValueBandWestHi { get; private set; }
    public double MinTonnes { get; private set; }
    public double MaxTonnes { get; private set; }
    public double PremiumLo { get; private set; }
    public double PremiumHi { get; private set; }
    public long DeadlineDaysLo { get; private set; }
    public long DeadlineDaysHi { get; private set; }
    public long OfferDays { get; private set; }

    /// <summary>Share of a tender's value that a missed delivery costs.</summary>
    public double FineShare { get; private set; }

    public double RelationsHit { get; private set; }
    public double RelationsCap { get; private set; }
    public double RelationsDecayPerDay { get; private set; }

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
        var ru = m.GetProperty("rules");
        BuildersPerSite = ru.GetProperty("builders_per_site").GetInt32();
        MachineryPerBuilderDay = ru.GetProperty("machinery_per_builder_day").GetDouble();
        ContractorDays = ru.GetProperty("contractor_days").GetDouble();
        ContractorRate = ru.GetProperty("contractor_rate").GetDouble();
        FoodPerCitizen = ru.GetProperty("food_per_citizen").GetDouble();
        ClothesPerCitizen = ru.GetProperty("clothes_per_citizen").GetDouble();
        AlcoholPerCitizen = ru.GetProperty("alcohol_per_citizen").GetDouble();
        ElectronicsPerCitizen = ru.GetProperty("electronics_per_citizen").GetDouble();
        ServiceRadius = ru.GetProperty("service_radius_m").GetDouble();
        TransformerRange = ru.GetProperty("transformer_range_m").GetDouble();
        SmokeRadius = ru.GetProperty("smoke_radius_m").GetDouble();
        WornEfficiency = ru.GetProperty("worn_efficiency").GetDouble();
        GrowingMinC = ru.GetProperty("growing_min_c").GetDouble();
        GrowingWarmC = ru.GetProperty("growing_warm_c").GetDouble();
        DroughtBelow = ru.GetProperty("drought_below").GetDouble();
        DroughtFloor = ru.GetProperty("drought_floor").GetDouble();
        WateredAt = ru.GetProperty("watered_at").GetDouble();
        WateredYield = ru.GetProperty("watered_yield").GetDouble();
        ResupplyAtDays = ru.GetProperty("resupply_at_days").GetDouble();
        MinLoad = ru.GetProperty("min_load_t").GetDouble();
        BogSpan = ru.GetProperty("bog_span").GetDouble();
        WorstOdds = ru.GetProperty("worst_odds").GetDouble();
        DigOut = ru.GetProperty("dig_out").GetDouble();
        RefuelRange = ru.GetProperty("refuel_range_m").GetDouble();
        Oldest = ru.GetProperty("oldest").GetInt32();
        BirthsPerPairYear = ru.GetProperty("births_per_pair_year").GetDouble();
        BirthsNeed = ru.GetProperty("births_need").GetDouble();
        ArrivalOdds = ru.GetProperty("arrival_odds").GetDouble();
        PloughAt = ru.GetProperty("plough_at").GetDouble();

        var rw = m.GetProperty("roadworks");
        JunctionSpacing = rw.GetProperty("junction_spacing_m").GetDouble();
        JunctionMerge = rw.GetProperty("junction_merge_m").GetDouble();
        MinRoad = rw.GetProperty("min_road_m").GetDouble();
        LampLabour = rw.GetProperty("lamp_labour").GetDouble();
        LampMwPerKm = rw.GetProperty("lamp_mw_per_km").GetDouble();
        LampMaterials = ReadBill(rw.GetProperty("lamp_materials"));

        var grades = new List<GradeDef>();
        foreach (var g in m.GetProperty("grades").EnumerateArray())
        {
            grades.Add(new GradeDef(
                g.GetProperty("grade").GetString()!,
                g.GetProperty("name").GetString()!,
                (Medium)IndexIn(Media, g.GetProperty("carries").GetString()!),
                g.GetProperty("speed_kph").GetDouble(),
                g.GetProperty("labour").GetDouble(),
                g.GetProperty("lamps").GetBoolean(),
                ReadBill(g.GetProperty("materials"))));
        }

        Grades = [.. grades];

        var uj = m.GetProperty("utility_join");
        UtilityJoin = uj.GetProperty("join_m").GetDouble();
        MinLine = uj.GetProperty("min_line_m").GetDouble();

        var utilities = new List<UtilityDef>();
        foreach (var u in m.GetProperty("utilities").EnumerateArray())
        {
            var carries = new List<int>();
            foreach (var r in u.GetProperty("carries").EnumerateArray())
            {
                carries.Add(ResourceIndex(r.GetString()!));
            }

            utilities.Add(new UtilityDef(
                u.GetProperty("kind").GetString()!,
                u.GetProperty("name").GetString()!,
                u.GetProperty("labour").GetDouble(),
                u.GetProperty("reach_m").GetDouble(),
                u.GetProperty("loss_per_km").GetDouble(),
                u.GetProperty("throughput").GetDouble(),
                [.. carries],
                ReadBill(u.GetProperty("materials"))));
        }

        Utilities = [.. utilities];

        var wb = m.GetProperty("wellbeing");
        ComfortLift = wb.GetProperty("comfort_lift").GetDouble();
        LoyaltyDrift = wb.GetProperty("loyalty_drift").GetDouble();
        LoyaltyLeaves = wb.GetProperty("loyalty_leaves").GetDouble();
        EmigrationOdds = wb.GetProperty("emigration_odds").GetDouble();
        HealthDrift = wb.GetProperty("health_drift").GetDouble();
        AlcoholHealthCost = wb.GetProperty("alcohol_health_cost").GetDouble();
        HealthUnserved = wb.GetProperty("health_unserved").GetDouble();
        ContentAttracts = wb.GetProperty("content_attracts").GetDouble();
        ArrivalParty = wb.GetProperty("arrival_party").GetInt32();
        PatienceDays = wb.GetProperty("patience_days").GetInt64();
        HiringFee = wb.GetProperty("hiring_fee").GetDouble();
        ForeignWage = wb.GetProperty("foreign_wage").GetDouble();
        StayDays = wb.GetProperty("stay_days").GetInt64();
        TourParty = wb.GetProperty("tour_party").GetInt32();
        SpendPerHeadPerDay = wb.GetProperty("spend_per_head_per_day").GetDouble();
        AppealFloor = wb.GetProperty("appeal_floor").GetDouble();
        _contentmentWeights = Doubles(wb, "weights");

        var tr = m.GetProperty("trade");
        Crossings = tr.GetProperty("crossings").GetInt32();
        CrossingInset = tr.GetProperty("crossing_inset_m").GetDouble();
        CustomsRange = tr.GetProperty("customs_range_m").GetDouble();
        CustomsThroughputPerDay = tr.GetProperty("customs_throughput_per_day").GetDouble();
        BorderSpread = tr.GetProperty("border_spread").GetDouble();

        var ln = m.GetProperty("loans");
        DefaultFine = ln.GetProperty("default_fine").GetDouble();
        DefaultRelations = ln.GetProperty("default_relations").GetDouble();
        _ladderEast = ReadLadder(ln.GetProperty("east"));
        _ladderWest = ReadLadder(ln.GetProperty("west"));

        var ct = m.GetProperty("contracts");
        OfferEveryMonths = ct.GetProperty("offer_every_months").GetInt64();
        MaxOpenOffers = ct.GetProperty("max_open_offers").GetInt32();
        ValueBandEastLo = ct.GetProperty("value_band_east")[0].GetDouble();
        ValueBandEastHi = ct.GetProperty("value_band_east")[1].GetDouble();
        ValueBandWestLo = ct.GetProperty("value_band_west")[0].GetDouble();
        ValueBandWestHi = ct.GetProperty("value_band_west")[1].GetDouble();
        MinTonnes = ct.GetProperty("tonnes")[0].GetDouble();
        MaxTonnes = ct.GetProperty("tonnes")[1].GetDouble();
        PremiumLo = ct.GetProperty("premium")[0].GetDouble();
        PremiumHi = ct.GetProperty("premium")[1].GetDouble();
        DeadlineDaysLo = ct.GetProperty("deadline_days")[0].GetInt64();
        DeadlineDaysHi = ct.GetProperty("deadline_days")[1].GetInt64();
        OfferDays = ct.GetProperty("offer_days").GetInt64();
        FineShare = ct.GetProperty("fine_share").GetDouble();
        RelationsHit = ct.GetProperty("relations_hit").GetDouble();
        RelationsCap = ct.GetProperty("relations_cap").GetDouble();
        RelationsDecayPerDay = ct.GetProperty("relations_decay_per_day").GetDouble();

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

    private Bill[] ReadBill(JsonElement e)
    {
        var bill = new List<Bill>();
        foreach (var pair in e.EnumerateArray())
        {
            bill.Add(new Bill(ResourceIndex(pair[0].GetString()!), pair[1].GetDouble()));
        }

        return [.. bill];
    }

    private static Tier[] ReadLadder(JsonElement e)
    {
        var tiers = new List<Tier>();
        foreach (var t in e.EnumerateArray())
        {
            tiers.Add(new Tier(
                t.GetProperty("principal").GetDouble(),
                t.GetProperty("interest").GetDouble(),
                t.GetProperty("term_days").GetInt64()));
        }

        return [.. tiers];
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
        h.Push(BuildersPerSite);
        h.Push(MachineryPerBuilderDay);
        h.Push(ContractorDays);
        h.Push(ContractorRate);
        h.Push(FoodPerCitizen);
        h.Push(ClothesPerCitizen);
        h.Push(AlcoholPerCitizen);
        h.Push(ElectronicsPerCitizen);
        h.Push(ServiceRadius);
        h.Push(TransformerRange);
        h.Push(SmokeRadius);
        h.Push(WornEfficiency);
        h.Push(GrowingMinC);
        h.Push(GrowingWarmC);
        h.Push(DroughtBelow);
        h.Push(DroughtFloor);
        h.Push(WateredAt);
        h.Push(WateredYield);
        h.Push(ResupplyAtDays);
        h.Push(MinLoad);
        h.Push(BogSpan);
        h.Push(WorstOdds);
        h.Push(DigOut);
        h.Push(RefuelRange);
        h.Push(Oldest);
        h.Push(BirthsPerPairYear);
        h.Push(BirthsNeed);
        h.Push(ArrivalOdds);
        h.Push(PloughAt);
        h.Push(JunctionSpacing);
        h.Push(JunctionMerge);
        h.Push(MinRoad);
        h.Push(LampLabour);
        h.Push(LampMwPerKm);
        foreach (var b in LampMaterials)
        {
            h.Push(b.Tonnes);
        }

        foreach (var g in Grades)
        {
            h.Push(g.SpeedKph);
            h.Push(g.Labour);
            h.Push(g.Lamps);
            foreach (var b in g.Materials)
            {
                h.Push(b.Tonnes);
            }
        }

        h.Push(UtilityJoin);
        h.Push(MinLine);
        foreach (var u in Utilities)
        {
            h.Push(u.Labour);
            h.Push(u.Reach);
            h.Push(u.LossPerKm);
            h.Push(u.Throughput);
            foreach (var b in u.Materials)
            {
                h.Push(b.Tonnes);
            }
        }

        foreach (var w in _contentmentWeights)
        {
            h.Push(w);
        }

        h.Push(ComfortLift);
        h.Push(LoyaltyDrift);
        h.Push(LoyaltyLeaves);
        h.Push(EmigrationOdds);
        h.Push(HealthDrift);
        h.Push(AlcoholHealthCost);
        h.Push(HealthUnserved);
        h.Push(ContentAttracts);
        h.Push(ArrivalParty);
        h.Push(PatienceDays);
        h.Push(HiringFee);
        h.Push(ForeignWage);
        h.Push(StayDays);
        h.Push(TourParty);
        h.Push(SpendPerHeadPerDay);
        h.Push(AppealFloor);
        h.Push(Crossings);
        h.Push(CrossingInset);
        h.Push(CustomsRange);
        h.Push(CustomsThroughputPerDay);
        h.Push(BorderSpread);
        foreach (var ladder in new[] { _ladderEast, _ladderWest })
        {
            foreach (var tier in ladder)
            {
                h.Push(tier.Principal);
                h.Push(tier.Interest);
                h.Push(tier.TermDays);
            }
        }

        h.Push(DefaultFine);
        h.Push(DefaultRelations);
        h.Push(OfferEveryMonths);
        h.Push(MaxOpenOffers);
        h.Push(ValueBandEastLo);
        h.Push(ValueBandEastHi);
        h.Push(ValueBandWestLo);
        h.Push(ValueBandWestHi);
        h.Push(MinTonnes);
        h.Push(MaxTonnes);
        h.Push(PremiumLo);
        h.Push(PremiumHi);
        h.Push(DeadlineDaysLo);
        h.Push(DeadlineDaysHi);
        h.Push(OfferDays);
        h.Push(FineShare);
        h.Push(RelationsHit);
        h.Push(RelationsCap);
        h.Push(RelationsDecayPerDay);
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
